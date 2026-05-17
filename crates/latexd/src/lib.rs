pub mod compiler;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    process::Stdio,
    sync::atomic::{AtomicU64, Ordering as AtomicOrdering},
    sync::{Arc, Mutex as StdMutex, OnceLock},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use axum::{
    Router,
    extract::{
        Json, Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post, put},
};
use camino::{Utf8Path, Utf8PathBuf};
use hmr_protocol::{
    ClientMsg, Diagnostic, DiagnosticLevel, PagePreviewArtifact, ServerMsg, SourceSnapshotFile,
};
use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tex_render_gs::{
    CliRenderer, GsApiRenderer, GsApiRuntime, GsApiRuntimePool, MockRenderer, PageRenderInput,
    Rect, Renderer, TileImage, Viewport, required_tiles_for_viewport,
};
use tex_world::normalize_relative_path;
use tokio::{
    process::Command,
    sync::{Notify, RwLock, broadcast, mpsc, oneshot},
};
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};

use crate::compiler::{CompileRequest, CompilerDriver, PageArtifactMeta, PageSyncMapArtifact};

#[derive(Debug, Clone)]
pub struct ServeArgs {
    pub root: Utf8PathBuf,
    pub bind: String,
    pub compiler_bin: Option<String>,
    pub compiler_args: Vec<String>,
    pub tile_renderer: TileRendererConfig,
    pub editor_bridge: Option<EditorBridgeConfig>,
}

#[derive(Debug, Clone)]
pub struct EditorBridgeConfig {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum TileRendererConfig {
    Mock,
    GsCli {
        program: String,
    },
    GsApi {
        library_path: String,
        runtime: Option<Arc<GsApiRuntime>>,
        runtime_pool: Option<Arc<GsApiRuntimePool>>,
    },
    #[cfg(test)]
    CountingMock {
        calls: Arc<std::sync::atomic::AtomicUsize>,
        tile_calls: Arc<std::sync::atomic::AtomicUsize>,
        sessions: Arc<std::sync::atomic::AtomicUsize>,
        sleep_ms: u64,
    },
}

impl TileRendererConfig {
    fn session_identity(&self) -> String {
        match self {
            Self::Mock => "mock".to_string(),
            Self::GsCli { program } => format!("gs-cli:{program}"),
            Self::GsApi { library_path, .. } => format!("gs-api:{library_path}"),
            #[cfg(test)]
            Self::CountingMock { .. } => "counting-mock".to_string(),
        }
    }

    fn render_full_page(
        &self,
        page: &PageRenderInput,
        scale: f32,
    ) -> anyhow::Result<tex_render_gs::RasterImage> {
        match self {
            Self::Mock => {
                let mut renderer = MockRenderer;
                renderer.render_full_page(page, scale)
            }
            Self::GsCli { program } => {
                let mut renderer = CliRenderer {
                    program: program.clone(),
                };
                renderer.render_full_page(page, scale)
            }
            Self::GsApi {
                library_path,
                runtime,
                runtime_pool,
            } => {
                let mut renderer = GsApiRenderer {
                    library_path: Some(library_path.clone()),
                    runtime: runtime.clone(),
                    runtime_pool: runtime_pool.clone(),
                };
                renderer.render_full_page(page, scale)
            }
            #[cfg(test)]
            Self::CountingMock {
                calls, sleep_ms, ..
            } => {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(*sleep_ms));
                let mut renderer = MockRenderer;
                renderer.render_full_page(page, scale)
            }
        }
    }

    fn render_tiles(
        &self,
        page: &PageRenderInput,
        scale: f32,
        rects: &[Rect],
    ) -> anyhow::Result<Vec<TileImage>> {
        match self {
            Self::Mock => {
                let mut renderer = MockRenderer;
                renderer.render_tiles(page, scale, rects)
            }
            Self::GsCli { program } => {
                let mut renderer = CliRenderer {
                    program: program.clone(),
                };
                renderer.render_tiles(page, scale, rects)
            }
            Self::GsApi {
                library_path,
                runtime,
                runtime_pool,
            } => {
                let mut renderer = GsApiRenderer {
                    library_path: Some(library_path.clone()),
                    runtime: runtime.clone(),
                    runtime_pool: runtime_pool.clone(),
                };
                renderer.render_tiles(page, scale, rects)
            }
            #[cfg(test)]
            Self::CountingMock {
                tile_calls,
                sleep_ms,
                ..
            } => {
                tile_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(*sleep_ms));
                let mut renderer = MockRenderer;
                renderer.render_tiles(page, scale, rects)
            }
        }
    }

    fn build_session_renderer(&self) -> RenderSessionRenderer {
        match self {
            Self::Mock => RenderSessionRenderer::Mock(MockRenderer),
            Self::GsCli { program } => RenderSessionRenderer::GsCli(CliRenderer {
                program: program.clone(),
            }),
            Self::GsApi {
                library_path,
                runtime,
                runtime_pool,
            } => RenderSessionRenderer::GsApi(GsApiRenderer {
                library_path: Some(library_path.clone()),
                runtime: runtime.clone(),
                runtime_pool: runtime_pool.clone(),
            }),
            #[cfg(test)]
            Self::CountingMock {
                calls,
                tile_calls,
                sessions,
                sleep_ms,
            } => {
                sessions.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                RenderSessionRenderer::CountingMock {
                    calls: calls.clone(),
                    tile_calls: tile_calls.clone(),
                    sleep_ms: *sleep_ms,
                }
            }
        }
    }
}

enum RenderSessionRenderer {
    Mock(MockRenderer),
    GsCli(CliRenderer),
    GsApi(GsApiRenderer),
    #[cfg(test)]
    CountingMock {
        calls: Arc<std::sync::atomic::AtomicUsize>,
        tile_calls: Arc<std::sync::atomic::AtomicUsize>,
        sleep_ms: u64,
    },
}

impl RenderSessionRenderer {
    fn render_full_page(
        &mut self,
        page: &PageRenderInput,
        scale: f32,
    ) -> anyhow::Result<tex_render_gs::RasterImage> {
        match self {
            Self::Mock(renderer) => renderer.render_full_page(page, scale),
            Self::GsCli(renderer) => renderer.render_full_page(page, scale),
            Self::GsApi(renderer) => renderer.render_full_page(page, scale),
            #[cfg(test)]
            Self::CountingMock {
                calls, sleep_ms, ..
            } => {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(*sleep_ms));
                let mut renderer = MockRenderer;
                renderer.render_full_page(page, scale)
            }
        }
    }

    fn render_tiles(
        &mut self,
        page: &PageRenderInput,
        scale: f32,
        rects: &[Rect],
    ) -> anyhow::Result<Vec<TileImage>> {
        match self {
            Self::Mock(renderer) => renderer.render_tiles(page, scale, rects),
            Self::GsCli(renderer) => renderer.render_tiles(page, scale, rects),
            Self::GsApi(renderer) => renderer.render_tiles(page, scale, rects),
            #[cfg(test)]
            Self::CountingMock {
                tile_calls,
                sleep_ms,
                ..
            } => {
                tile_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(*sleep_ms));
                let mut renderer = MockRenderer;
                renderer.render_tiles(page, scale, rects)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PreviewSnapshot {
    pub current_rev: u64,
    pub last_applied_rev: u64,
    pub pdf_url: Option<String>,
    pub page_count: usize,
    pub page_ids: Vec<String>,
    pub page_artifacts: Vec<PagePreviewArtifact>,
    #[serde(default)]
    pub source_snapshot: Vec<SourceSnapshotFile>,
    pub diagnostics: Vec<Diagnostic>,
    pub changed_files: Vec<String>,
    pub building: bool,
    pub last_build_succeeded: Option<bool>,
    pub editor_bridge_enabled: bool,
}

#[derive(Debug, Clone, Default)]
struct LivePreviewState {
    snapshot: PreviewSnapshot,
    latest_pdf_path: Option<Utf8PathBuf>,
    page_metadata: Vec<PageArtifactMeta>,
}

#[derive(Debug)]
struct AppState {
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    artifacts_root: Utf8PathBuf,
    world: tex_world::ProjectWorld,
    compiler: CompilerDriver,
    tile_renderer: TileRendererConfig,
    editor_bridge: Option<EditorBridgeConfig>,
    raster_cache: RwLock<BTreeMap<RasterCacheKey, tex_render_gs::RasterImage>>,
    inflight_rasters: RwLock<BTreeMap<RasterCacheKey, Arc<Notify>>>,
    build_cache: RwLock<BuildCache>,
    live: RwLock<LivePreviewState>,
    events: broadcast::Sender<ServerMsg>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RasterCacheKey {
    rev: u64,
    page_id: String,
    content_hash: String,
    zoom_bucket: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RenderSessionTileCacheKey {
    bucket: RasterCacheKey,
    rect_x: u32,
    rect_y: u32,
    rect_width: u32,
    rect_height: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BuildCache {
    tracked_inputs: BTreeSet<Utf8PathBuf>,
    input_hashes: BTreeMap<Utf8PathBuf, Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RenderSessionKey {
    root: Utf8PathBuf,
    renderer_identity: String,
}

#[derive(Debug, Clone)]
struct RenderSessionHandle {
    tx: mpsc::Sender<RenderSessionRequest>,
}

#[derive(Debug, Default)]
struct RenderSessionMetrics {
    actor_spawn_count: AtomicU64,
    actor_restart_count: AtomicU64,
    actor_failure_count: AtomicU64,
    attach_count: AtomicU64,
    attached_page_count: AtomicU64,
    evict_count: AtomicU64,
    detach_count: AtomicU64,
    page_lookup_count: AtomicU64,
    revision_lookup_count: AtomicU64,
    warm_bucket_evict_count: AtomicU64,
    tile_cache_evict_count: AtomicU64,
    prewarm_request_count: AtomicU64,
    skipped_prewarm_count: AtomicU64,
    render_request_count: AtomicU64,
    tile_render_request_count: AtomicU64,
    fallback_prewarm_count: AtomicU64,
    fallback_render_count: AtomicU64,
    fallback_tile_render_count: AtomicU64,
    render_duration_total_ms: AtomicU64,
    render_duration_max_ms: AtomicU64,
    tile_render_duration_total_ms: AtomicU64,
    tile_render_duration_max_ms: AtomicU64,
    prewarm_duration_total_ms: AtomicU64,
    prewarm_duration_max_ms: AtomicU64,
    fallback_prewarm_duration_total_ms: AtomicU64,
    fallback_prewarm_duration_max_ms: AtomicU64,
    fallback_render_duration_total_ms: AtomicU64,
    fallback_render_duration_max_ms: AtomicU64,
    fallback_tile_render_duration_total_ms: AtomicU64,
    fallback_tile_render_duration_max_ms: AtomicU64,
    recent_events: StdMutex<VecDeque<RenderSessionEvent>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RenderSessionEventKind {
    ActorSpawn,
    ActorRestart,
    ActorFailure,
    AttachRevision,
    EvictRevision,
    DetachRevision,
    LookupRevisionPages,
    LookupPage,
    EvictWarmBucket,
    EvictTileCache,
    PrewarmPage,
    SkipPrewarmPage,
    RenderPage,
    RenderTiles,
    ReuseTiles,
    FallbackPrewarmPage,
    FallbackRender,
    FallbackRenderTiles,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RenderSessionEvent {
    kind: RenderSessionEventKind,
    rev: Option<u64>,
    page_id: Option<String>,
    scale_percent: Option<u16>,
    page_count: Option<usize>,
    duration_ms: Option<u32>,
    rendered_rect_count: Option<u32>,
    reused_rect_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RenderSessionMetricsSnapshot {
    active_session_count: usize,
    actor_spawn_count: u64,
    actor_restart_count: u64,
    actor_failure_count: u64,
    attach_count: u64,
    attached_page_count: u64,
    attached_live_page_count: usize,
    attached_revision_window_limit: usize,
    attached_page_budget: usize,
    warm_bucket_budget: usize,
    warm_bucket_page_budget: usize,
    warm_bucket_count: usize,
    tile_cache_budget: usize,
    tile_cache_page_budget: usize,
    tile_cache_count: usize,
    evict_count: u64,
    detach_count: u64,
    page_lookup_count: u64,
    revision_lookup_count: u64,
    warm_bucket_evict_count: u64,
    tile_cache_evict_count: u64,
    prewarm_request_count: u64,
    skipped_prewarm_count: u64,
    render_request_count: u64,
    tile_render_request_count: u64,
    fallback_prewarm_count: u64,
    fallback_render_count: u64,
    fallback_tile_render_count: u64,
    render_latency: RenderSessionLatencySummary,
    tile_render_latency: RenderSessionLatencySummary,
    prewarm_latency: RenderSessionLatencySummary,
    fallback_prewarm_latency: RenderSessionLatencySummary,
    fallback_render_latency: RenderSessionLatencySummary,
    fallback_tile_render_latency: RenderSessionLatencySummary,
    attached_revisions: Vec<RenderSessionAttachedRevisionSnapshot>,
    warm_buckets: Vec<RenderSessionWarmBucketSnapshot>,
    tile_cache_entries: Vec<RenderSessionTileCacheSnapshot>,
    recent_events: Vec<RenderSessionEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RenderSessionLatencySummary {
    count: u64,
    total_ms: u64,
    max_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RenderSessionAttachedRevisionSnapshot {
    rev: u64,
    page_count: usize,
    page_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RenderSessionWarmBucketSnapshot {
    rev: u64,
    page_id: String,
    content_hash: String,
    zoom_bucket: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RenderSessionTileCacheSnapshot {
    rev: u64,
    page_id: String,
    content_hash: String,
    zoom_bucket: u16,
    rect_x: u32,
    rect_y: u32,
    rect_width: u32,
    rect_height: u32,
}

impl RenderSessionMetrics {
    fn snapshot(&self, active_session_count: usize) -> RenderSessionMetricsSnapshot {
        RenderSessionMetricsSnapshot {
            active_session_count,
            actor_spawn_count: self.actor_spawn_count.load(AtomicOrdering::SeqCst),
            actor_restart_count: self.actor_restart_count.load(AtomicOrdering::SeqCst),
            actor_failure_count: self.actor_failure_count.load(AtomicOrdering::SeqCst),
            attach_count: self.attach_count.load(AtomicOrdering::SeqCst),
            attached_page_count: self.attached_page_count.load(AtomicOrdering::SeqCst),
            attached_live_page_count: 0,
            attached_revision_window_limit: RENDER_SESSION_ATTACHED_REVISION_WINDOW,
            attached_page_budget: RENDER_SESSION_ATTACHED_PAGE_BUDGET,
            warm_bucket_budget: RENDER_SESSION_WARM_BUCKET_BUDGET,
            warm_bucket_page_budget: RENDER_SESSION_WARM_BUCKET_PAGE_BUDGET,
            warm_bucket_count: 0,
            tile_cache_budget: RENDER_SESSION_TILE_CACHE_BUDGET,
            tile_cache_page_budget: RENDER_SESSION_TILE_CACHE_PAGE_BUDGET,
            tile_cache_count: 0,
            evict_count: self.evict_count.load(AtomicOrdering::SeqCst),
            detach_count: self.detach_count.load(AtomicOrdering::SeqCst),
            page_lookup_count: self.page_lookup_count.load(AtomicOrdering::SeqCst),
            revision_lookup_count: self.revision_lookup_count.load(AtomicOrdering::SeqCst),
            warm_bucket_evict_count: self.warm_bucket_evict_count.load(AtomicOrdering::SeqCst),
            tile_cache_evict_count: self.tile_cache_evict_count.load(AtomicOrdering::SeqCst),
            prewarm_request_count: self.prewarm_request_count.load(AtomicOrdering::SeqCst),
            skipped_prewarm_count: self.skipped_prewarm_count.load(AtomicOrdering::SeqCst),
            render_request_count: self.render_request_count.load(AtomicOrdering::SeqCst),
            tile_render_request_count: self.tile_render_request_count.load(AtomicOrdering::SeqCst),
            fallback_prewarm_count: self.fallback_prewarm_count.load(AtomicOrdering::SeqCst),
            fallback_render_count: self.fallback_render_count.load(AtomicOrdering::SeqCst),
            fallback_tile_render_count: self
                .fallback_tile_render_count
                .load(AtomicOrdering::SeqCst),
            render_latency: RenderSessionLatencySummary {
                count: self.render_request_count.load(AtomicOrdering::SeqCst),
                total_ms: self.render_duration_total_ms.load(AtomicOrdering::SeqCst),
                max_ms: self.render_duration_max_ms.load(AtomicOrdering::SeqCst) as u32,
            },
            tile_render_latency: RenderSessionLatencySummary {
                count: self.tile_render_request_count.load(AtomicOrdering::SeqCst),
                total_ms: self
                    .tile_render_duration_total_ms
                    .load(AtomicOrdering::SeqCst),
                max_ms: self
                    .tile_render_duration_max_ms
                    .load(AtomicOrdering::SeqCst) as u32,
            },
            prewarm_latency: RenderSessionLatencySummary {
                count: self.prewarm_request_count.load(AtomicOrdering::SeqCst),
                total_ms: self.prewarm_duration_total_ms.load(AtomicOrdering::SeqCst),
                max_ms: self.prewarm_duration_max_ms.load(AtomicOrdering::SeqCst) as u32,
            },
            fallback_prewarm_latency: RenderSessionLatencySummary {
                count: self.fallback_prewarm_count.load(AtomicOrdering::SeqCst),
                total_ms: self
                    .fallback_prewarm_duration_total_ms
                    .load(AtomicOrdering::SeqCst),
                max_ms: self
                    .fallback_prewarm_duration_max_ms
                    .load(AtomicOrdering::SeqCst) as u32,
            },
            fallback_render_latency: RenderSessionLatencySummary {
                count: self.fallback_render_count.load(AtomicOrdering::SeqCst),
                total_ms: self
                    .fallback_render_duration_total_ms
                    .load(AtomicOrdering::SeqCst),
                max_ms: self
                    .fallback_render_duration_max_ms
                    .load(AtomicOrdering::SeqCst) as u32,
            },
            fallback_tile_render_latency: RenderSessionLatencySummary {
                count: self.fallback_tile_render_count.load(AtomicOrdering::SeqCst),
                total_ms: self
                    .fallback_tile_render_duration_total_ms
                    .load(AtomicOrdering::SeqCst),
                max_ms: self
                    .fallback_tile_render_duration_max_ms
                    .load(AtomicOrdering::SeqCst) as u32,
            },
            attached_revisions: Vec::new(),
            warm_buckets: Vec::new(),
            tile_cache_entries: Vec::new(),
            recent_events: self
                .recent_events
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .iter()
                .cloned()
                .collect(),
        }
    }

    fn record_event(
        &self,
        kind: RenderSessionEventKind,
        rev: Option<u64>,
        page_id: Option<&str>,
        scale: Option<f32>,
        page_count: Option<usize>,
        duration: Option<Duration>,
    ) {
        self.record_event_inner(kind, rev, page_id, scale, page_count, duration, None, None);
    }

    fn record_tile_event(
        &self,
        kind: RenderSessionEventKind,
        rev: Option<u64>,
        page_id: Option<&str>,
        scale: Option<f32>,
        page_count: Option<usize>,
        duration: Option<Duration>,
        rendered_rect_count: u32,
        reused_rect_count: u32,
    ) {
        self.record_event_inner(
            kind,
            rev,
            page_id,
            scale,
            page_count,
            duration,
            Some(rendered_rect_count),
            Some(reused_rect_count),
        );
    }

    fn record_event_inner(
        &self,
        kind: RenderSessionEventKind,
        rev: Option<u64>,
        page_id: Option<&str>,
        scale: Option<f32>,
        page_count: Option<usize>,
        duration: Option<Duration>,
        rendered_rect_count: Option<u32>,
        reused_rect_count: Option<u32>,
    ) {
        let scale_percent =
            scale.map(|value| (value * 100.0).round().clamp(1.0, u16::MAX as f32) as u16);
        let duration_ms = duration
            .map(|value| value.as_millis().min(u128::from(u32::MAX)) as u32)
            .filter(|value| *value > 0);
        if let Some(duration_ms) = duration_ms {
            match kind {
                RenderSessionEventKind::RenderPage => {
                    self.render_duration_total_ms
                        .fetch_add(u64::from(duration_ms), AtomicOrdering::SeqCst);
                    self.render_duration_max_ms
                        .fetch_max(u64::from(duration_ms), AtomicOrdering::SeqCst);
                }
                RenderSessionEventKind::RenderTiles => {
                    self.tile_render_duration_total_ms
                        .fetch_add(u64::from(duration_ms), AtomicOrdering::SeqCst);
                    self.tile_render_duration_max_ms
                        .fetch_max(u64::from(duration_ms), AtomicOrdering::SeqCst);
                }
                RenderSessionEventKind::PrewarmPage => {
                    self.prewarm_duration_total_ms
                        .fetch_add(u64::from(duration_ms), AtomicOrdering::SeqCst);
                    self.prewarm_duration_max_ms
                        .fetch_max(u64::from(duration_ms), AtomicOrdering::SeqCst);
                }
                RenderSessionEventKind::FallbackPrewarmPage => {
                    self.fallback_prewarm_duration_total_ms
                        .fetch_add(u64::from(duration_ms), AtomicOrdering::SeqCst);
                    self.fallback_prewarm_duration_max_ms
                        .fetch_max(u64::from(duration_ms), AtomicOrdering::SeqCst);
                }
                RenderSessionEventKind::FallbackRender => {
                    self.fallback_render_duration_total_ms
                        .fetch_add(u64::from(duration_ms), AtomicOrdering::SeqCst);
                    self.fallback_render_duration_max_ms
                        .fetch_max(u64::from(duration_ms), AtomicOrdering::SeqCst);
                }
                RenderSessionEventKind::FallbackRenderTiles => {
                    self.fallback_tile_render_duration_total_ms
                        .fetch_add(u64::from(duration_ms), AtomicOrdering::SeqCst);
                    self.fallback_tile_render_duration_max_ms
                        .fetch_max(u64::from(duration_ms), AtomicOrdering::SeqCst);
                }
                _ => {}
            }
        }
        debug!(
            kind = ?kind,
            ?rev,
            page_id = page_id.unwrap_or(""),
            ?page_count,
            ?scale_percent,
            ?duration_ms,
            ?rendered_rect_count,
            ?reused_rect_count,
            "renderer session event"
        );
        let mut recent_events = self
            .recent_events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        recent_events.push_back(RenderSessionEvent {
            kind,
            rev,
            page_id: page_id.map(str::to_string),
            scale_percent,
            page_count,
            duration_ms,
            rendered_rect_count,
            reused_rect_count,
        });
        while recent_events.len() > RENDER_SESSION_RECENT_EVENT_WINDOW {
            recent_events.pop_front();
        }
    }
}

#[derive(Debug, Clone)]
struct AttachedRenderRevision {
    page_metadata: Vec<PageArtifactMeta>,
    page_inputs: BTreeMap<String, PageRenderInput>,
}

enum RenderSessionRequest {
    AttachRevision {
        rev: u64,
        page_metadata: Vec<PageArtifactMeta>,
        page_inputs: Vec<PageRenderInput>,
    },
    #[cfg(test)]
    DetachRevision { rev: u64 },
    LookupRevisionPages {
        rev: u64,
        reply: oneshot::Sender<Option<Vec<PageArtifactMeta>>>,
    },
    DebugState {
        reply: oneshot::Sender<(
            Vec<RenderSessionAttachedRevisionSnapshot>,
            Vec<RenderSessionWarmBucketSnapshot>,
            Vec<RenderSessionTileCacheSnapshot>,
        )>,
    },
    LookupPage {
        rev: u64,
        page_id: String,
        reply: oneshot::Sender<Option<PageRenderInput>>,
    },
    PrewarmPage {
        page: PageRenderInput,
        scale: f32,
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
    RenderPage {
        page: PageRenderInput,
        scale: f32,
        reply: oneshot::Sender<anyhow::Result<tex_render_gs::RasterImage>>,
    },
    RenderTiles {
        page: PageRenderInput,
        scale: f32,
        rects: Vec<Rect>,
        reply: oneshot::Sender<anyhow::Result<Vec<TileImage>>>,
    },
}

static RENDER_SESSIONS: OnceLock<StdMutex<BTreeMap<RenderSessionKey, RenderSessionHandle>>> =
    OnceLock::new();
static RENDER_SESSION_METRICS: OnceLock<
    StdMutex<BTreeMap<RenderSessionKey, Arc<RenderSessionMetrics>>>,
> = OnceLock::new();
static RENDER_SESSION_INFLIGHT_PREWARMS: OnceLock<
    StdMutex<BTreeSet<(RenderSessionKey, RasterCacheKey)>>,
> = OnceLock::new();

const RENDER_SESSION_ATTACHED_REVISION_WINDOW: usize = 4;
const RENDER_SESSION_ATTACHED_PAGE_BUDGET: usize = 48;
const RENDER_SESSION_WARM_BUCKET_BUDGET: usize = 96;
const RENDER_SESSION_WARM_BUCKET_PAGE_BUDGET: usize = 4;
const RENDER_SESSION_TILE_CACHE_BUDGET: usize = 192;
const RENDER_SESSION_TILE_CACHE_PAGE_BUDGET: usize = 32;
const RENDER_SESSION_RECENT_EVENT_WINDOW: usize = 32;
const RENDER_SESSION_VISIBLE_ZOOM_NEIGHBOR_PAGE_LIMIT: usize = 2;

fn render_session_prewarm_rect(page: &PageRenderInput, scale: f32) -> Rect {
    Rect {
        x: 0,
        y: 0,
        width: ((page.width_px as f32 * scale).round() as u32).clamp(1, 256),
        height: ((page.height_px as f32 * scale).round() as u32).clamp(1, 256),
    }
}

fn render_session_zoom_bucket(scale: f32) -> u16 {
    (scale * 100.0).round().clamp(1.0, u16::MAX as f32) as u16
}

fn render_session_bucket_key(page: &PageRenderInput, scale: f32) -> RasterCacheKey {
    RasterCacheKey {
        rev: page.revision,
        page_id: page.page_id.clone(),
        content_hash: page.content_hash.clone(),
        zoom_bucket: render_session_zoom_bucket(scale),
    }
}

fn render_session_tile_cache_key(
    page: &PageRenderInput,
    scale: f32,
    rect: &Rect,
) -> RenderSessionTileCacheKey {
    RenderSessionTileCacheKey {
        bucket: render_session_bucket_key(page, scale),
        rect_x: rect.x,
        rect_y: rect.y,
        rect_width: rect.width,
        rect_height: rect.height,
    }
}

fn render_session_inflight_prewarms()
-> &'static StdMutex<BTreeSet<(RenderSessionKey, RasterCacheKey)>> {
    RENDER_SESSION_INFLIGHT_PREWARMS.get_or_init(|| StdMutex::new(BTreeSet::new()))
}

impl PreviewSnapshot {
    pub fn apply_started(&mut self, rev: u64, changed_files: Vec<String>) {
        if rev < self.current_rev {
            return;
        }

        self.current_rev = rev;
        self.changed_files = changed_files;
        self.building = true;
    }

    pub fn apply_success(
        &mut self,
        rev: u64,
        diagnostics: Vec<Diagnostic>,
        pdf_url: String,
        page_count: usize,
        page_ids: Vec<String>,
        page_artifacts: Vec<PagePreviewArtifact>,
    ) {
        if rev < self.current_rev {
            return;
        }

        self.current_rev = rev;
        self.last_applied_rev = rev;
        self.pdf_url = Some(pdf_url);
        self.page_count = page_count;
        self.page_ids = page_ids;
        self.page_artifacts = page_artifacts;
        self.diagnostics = diagnostics;
        self.building = false;
        self.last_build_succeeded = Some(true);
    }

    pub fn apply_failure(&mut self, rev: u64, diagnostics: Vec<Diagnostic>) {
        if rev < self.current_rev {
            return;
        }

        self.current_rev = rev;
        self.diagnostics = diagnostics;
        self.building = false;
        self.last_build_succeeded = Some(false);
    }
}

pub async fn serve(args: ServeArgs) -> Result<()> {
    let canonical_root = std::fs::canonicalize(args.root.as_std_path())
        .with_context(|| format!("failed to access project root {}", args.root))?;
    let root = Utf8PathBuf::from_path_buf(canonical_root)
        .map_err(|path| anyhow!("project root is not valid UTF-8: {}", path.display()))?;
    let world = tex_world::ProjectWorld::load(root.clone())?;
    let build_root = root.join(".latexd/build");
    let artifacts_root = root.join(".latexd/artifacts");
    tokio::fs::create_dir_all(build_root.as_std_path())
        .await
        .with_context(|| format!("failed to create build root {}", build_root))?;
    tokio::fs::create_dir_all(artifacts_root.as_std_path())
        .await
        .with_context(|| format!("failed to create artifacts root {}", artifacts_root))?;

    let (events, _) = broadcast::channel(64);
    let editor_bridge_enabled = args.editor_bridge.is_some();
    let state = Arc::new(AppState {
        root: root.clone(),
        build_root,
        artifacts_root,
        world: world.clone(),
        compiler: CompilerDriver::new(args.compiler_bin, args.compiler_args),
        tile_renderer: args.tile_renderer,
        editor_bridge: args.editor_bridge,
        raster_cache: RwLock::new(BTreeMap::new()),
        inflight_rasters: RwLock::new(BTreeMap::new()),
        build_cache: RwLock::new(BuildCache::default()),
        live: RwLock::new(LivePreviewState {
            snapshot: PreviewSnapshot {
                editor_bridge_enabled,
                ..PreviewSnapshot::default()
            },
            ..LivePreviewState::default()
        }),
        events,
    });

    let (rebuild_tx, rebuild_rx) = mpsc::unbounded_channel::<Vec<Utf8PathBuf>>();
    let build_state = state.clone();
    tokio::spawn(async move {
        build_state.run_build_loop(rebuild_rx).await;
    });
    spawn_watcher(state.root.clone(), rebuild_tx.clone())?;
    rebuild_tx
        .send(state.world.manifest.toplevels.clone())
        .map_err(|error| anyhow!("failed to enqueue initial build: {error}"))?;

    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind(&args.bind)
        .await
        .with_context(|| format!("failed to bind {}", args.bind))?;

    tracing::info!("latexd listening on http://{}", args.bind);
    axum::serve(listener, router)
        .await
        .context("latexd server terminated unexpectedly")
}

fn build_router(state: Arc<AppState>) -> Router {
    build_router_with_base(state, &viewer_base_path())
}

fn build_router_with_base(state: Arc<AppState>, base_path: &str) -> Router {
    let api_router = build_api_router(state.clone());
    if base_path.is_empty() {
        return api_router
            .merge(build_viewer_router(state))
            .layer(TraceLayer::new_for_http());
    }

    let redirect_target = format!("{base_path}/");
    let root_redirect_target = redirect_target.clone();
    let base_redirect_target = redirect_target.clone();
    let viewer_asset_route = format!("{base_path}/{{*path}}");
    Router::new()
        .route(
            "/",
            get(move || {
                let redirect_target = root_redirect_target.clone();
                async move { Redirect::temporary(&redirect_target) }
            }),
        )
        .route(
            base_path,
            get(move || {
                let redirect_target = base_redirect_target.clone();
                async move { Redirect::temporary(&redirect_target) }
            }),
        )
        .route(&redirect_target, get(index))
        .route(&viewer_asset_route, get(viewer_asset))
        .merge(api_router)
        .nest(base_path, build_api_router(state))
        .layer(TraceLayer::new_for_http())
}

fn build_api_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/state", get(snapshot))
        .route("/api/debug/renderer-session", get(renderer_session_metrics))
        .route("/api/syncmap/{rev}/{page_id}", get(page_syncmap))
        .route("/api/source-files/{rev}", get(source_files))
        .route("/api/source-file/{rev}", get(source_file))
        .route("/api/source-file", put(update_source_file))
        .route("/api/source-jump/{rev}", get(source_jump))
        .route("/api/open-source/{rev}", post(open_source))
        .route("/api/tiles/{rev}/{page_id}", get(required_tiles))
        .route("/artifacts/latest.pdf", get(latest_pdf))
        .route(
            "/artifacts/rev/{rev}/page-raster/{page_png}",
            get(revision_page_png),
        )
        .route(
            "/artifacts/rev/{rev}/tiles/{page_id}/{zoom_bucket}/{tile_x}/{tile_y_png}",
            get(revision_tile_png),
        )
        .route("/artifacts/rev/{rev}/{*path}", get(revision_artifact))
        .route("/ws", get(ws))
        .with_state(state)
}

fn build_viewer_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/{*path}", get(viewer_asset))
        .with_state(state)
}

pub(crate) fn normalize_viewer_base_path(value: Option<&str>) -> String {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return String::new();
    };
    if value == "/" {
        return String::new();
    }
    let trimmed = value.trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

pub(crate) fn viewer_base_path() -> String {
    normalize_viewer_base_path(std::env::var("LATEXD_VIEWER_BASE_PATH").ok().as_deref())
}

pub(crate) fn viewer_prefixed_path(path: &str) -> String {
    viewer_prefixed_path_for(&viewer_base_path(), path)
}

pub(crate) fn viewer_prefixed_path_for(base_path: &str, path: &str) -> String {
    if base_path.is_empty() || path.is_empty() {
        return path.to_string();
    }
    if path.starts_with(base_path) {
        return path.to_string();
    }
    if path.starts_with('/') {
        format!("{base_path}{path}")
    } else {
        format!("{base_path}/{path}")
    }
}

impl AppState {
    async fn run_build_loop(
        self: Arc<Self>,
        mut rebuild_rx: mpsc::UnboundedReceiver<Vec<Utf8PathBuf>>,
    ) {
        let mut pending = BTreeSet::new();
        while let Some(first_batch) = rebuild_rx.recv().await {
            for path in first_batch {
                pending.insert(path);
            }

            let debounce = tokio::time::sleep(Duration::from_millis(150));
            tokio::pin!(debounce);
            loop {
                tokio::select! {
                    maybe_paths = rebuild_rx.recv() => {
                        match maybe_paths {
                            Some(paths) => {
                                for path in paths {
                                    pending.insert(path);
                                }
                            }
                            None => break,
                        }
                    }
                    _ = &mut debounce => break,
                }
            }

            let changed_files = pending
                .iter()
                .map(|path| path.to_string())
                .collect::<Vec<_>>();
            pending.clear();

            let rebuild_plan = match BuildCache::plan_rebuild(
                &self.root,
                &self.build_cache.read().await.clone(),
                &changed_files
                    .iter()
                    .map(Utf8PathBuf::from)
                    .collect::<Vec<_>>(),
            )
            .await
            {
                Ok(plan) => plan,
                Err(error) => {
                    let diagnostics = vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: None,
                        line: None,
                        message: format!("failed to evaluate dirty inputs: {error}"),
                    }];
                    let rev = {
                        let mut live = self.live.write().await;
                        let next_rev = live.snapshot.current_rev + 1;
                        live.snapshot.apply_started(next_rev, changed_files.clone());
                        live.snapshot.apply_failure(next_rev, diagnostics.clone());
                        next_rev
                    };
                    let _ = self.events.send(ServerMsg::BuildStarted {
                        rev,
                        changed_files: changed_files.clone(),
                    });
                    let _ = self.events.send(ServerMsg::Diagnostics {
                        rev,
                        items: diagnostics,
                    });
                    let _ = self.events.send(ServerMsg::BuildFinished {
                        rev,
                        success: false,
                    });
                    continue;
                }
            };

            if !rebuild_plan.needs_rebuild {
                info!(
                    changed = %changed_files.join(", "),
                    "skipping rebuild because tracked input hashes are unchanged"
                );
                continue;
            }

            let rev = {
                let mut live = self.live.write().await;
                let next_rev = live.snapshot.current_rev + 1;
                live.snapshot
                    .apply_started(next_rev, rebuild_plan.changed_inputs.clone());
                next_rev
            };
            let _ = self.events.send(ServerMsg::BuildStarted {
                rev,
                changed_files: rebuild_plan.changed_inputs.clone(),
            });

            let toplevel = self.world.manifest.toplevels.first().cloned();
            let Some(toplevel) = toplevel else {
                let diagnostics = vec![Diagnostic {
                    level: DiagnosticLevel::Error,
                    file: None,
                    line: None,
                    message: "manifest does not declare a toplevel document".to_string(),
                }];
                {
                    let mut live = self.live.write().await;
                    live.snapshot.apply_failure(rev, diagnostics.clone());
                }
                let _ = self.events.send(ServerMsg::Diagnostics {
                    rev,
                    items: diagnostics,
                });
                let _ = self.events.send(ServerMsg::BuildFinished {
                    rev,
                    success: false,
                });
                continue;
            };

            let outcome = self
                .compiler
                .compile(CompileRequest {
                    root: self.root.clone(),
                    manifest: self.world.manifest.clone(),
                    toplevel: toplevel.clone(),
                    rev,
                    build_root: self.build_root.clone(),
                    changed_files: rebuild_plan
                        .changed_inputs
                        .iter()
                        .map(Utf8PathBuf::from)
                        .collect(),
                })
                .await;

            match outcome {
                Ok(outcome) => {
                    let latest_pdf_path = self.artifacts_root.join("latest.pdf");
                    if let Err(error) = tokio::fs::copy(
                        outcome.pdf_path.as_std_path(),
                        latest_pdf_path.as_std_path(),
                    )
                    .await
                    {
                        let diagnostics = vec![Diagnostic {
                            level: DiagnosticLevel::Error,
                            file: Some(toplevel.to_string()),
                            line: None,
                            message: format!("failed to copy rendered PDF: {error}"),
                        }];
                        {
                            let mut live = self.live.write().await;
                            live.snapshot.apply_failure(rev, diagnostics.clone());
                        }
                        let _ = self.events.send(ServerMsg::Diagnostics {
                            rev,
                            items: diagnostics,
                        });
                        let _ = self.events.send(ServerMsg::BuildFinished {
                            rev,
                            success: false,
                        });
                        continue;
                    }

                    let pdf_file = outcome
                        .pdf_path
                        .file_name()
                        .unwrap_or("main.pdf")
                        .to_string();
                    let pdf_url = viewer_prefixed_path(&format!("/artifacts/rev/{rev}/{pdf_file}"));
                    let source_snapshot = build_source_snapshot(self.as_ref(), rev).await;
                    {
                        let mut live = self.live.write().await;
                        live.latest_pdf_path = Some(latest_pdf_path);
                        live.page_metadata = outcome.page_metadata.clone();
                        live.snapshot.apply_success(
                            rev,
                            outcome.diagnostics.clone(),
                            pdf_url.clone(),
                            outcome.page_metadata.len(),
                            outcome
                                .page_metadata
                                .iter()
                                .map(|page| page.page_id.clone())
                                .collect(),
                            outcome.page_artifacts.clone(),
                        );
                        live.snapshot.source_snapshot = source_snapshot.clone();
                    }
                    {
                        let mut build_cache = self.build_cache.write().await;
                        let mut tracked_inputs = outcome
                            .dep_trace
                            .inputs
                            .into_iter()
                            .collect::<BTreeSet<_>>();
                        for toplevel in &self.world.manifest.toplevels {
                            tracked_inputs.insert(toplevel.clone());
                        }
                        for candidate in
                            ["00README", "00README.yaml", "00README.yml", "00README.json"]
                        {
                            let path = Utf8PathBuf::from(candidate);
                            if self.root.join(&path).exists() {
                                tracked_inputs.insert(path);
                            }
                        }
                        build_cache.record_success(&self.root, tracked_inputs).await;
                    }
                    attach_render_revision(&self, rev, &outcome.page_metadata).await;
                    let _ = self.events.send(ServerMsg::Diagnostics {
                        rev,
                        items: outcome.diagnostics.clone(),
                    });
                    if !outcome.page_patches.is_empty() {
                        let _ = self.events.send(ServerMsg::PatchPages {
                            rev,
                            ops: outcome.page_patches.clone(),
                        });
                    }
                    let _ = self.events.send(ServerMsg::FullPdfReady {
                        rev,
                        pdf_url,
                        page_ids: outcome
                            .page_metadata
                            .iter()
                            .map(|page| page.page_id.clone())
                            .collect(),
                        page_artifacts: outcome.page_artifacts.clone(),
                    });
                    let _ = self.events.send(ServerMsg::SourceSnapshot {
                        rev,
                        files: source_snapshot,
                    });
                    let _ = self
                        .events
                        .send(ServerMsg::BuildFinished { rev, success: true });
                }
                Err(failure) => {
                    {
                        let mut live = self.live.write().await;
                        live.snapshot
                            .apply_failure(rev, failure.diagnostics.clone());
                    }
                    let _ = self.events.send(ServerMsg::Diagnostics {
                        rev,
                        items: failure.diagnostics.clone(),
                    });
                    let _ = self.events.send(ServerMsg::BuildFinished {
                        rev,
                        success: false,
                    });
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RebuildPlan {
    needs_rebuild: bool,
    changed_inputs: Vec<String>,
}

impl BuildCache {
    async fn plan_rebuild(
        root: &Utf8Path,
        cache: &BuildCache,
        changed_paths: &[Utf8PathBuf],
    ) -> Result<RebuildPlan> {
        if cache.tracked_inputs.is_empty() {
            return Ok(RebuildPlan {
                needs_rebuild: true,
                changed_inputs: changed_paths.iter().map(|path| path.to_string()).collect(),
            });
        }

        let mut dirty_inputs = Vec::new();
        for path in changed_paths {
            if !cache.tracked_inputs.contains(path) {
                continue;
            }

            let current_hash = hash_input(root, path).await?;
            if cache.input_hashes.get(path) != Some(&current_hash) {
                dirty_inputs.push(path.to_string());
            }
        }

        Ok(RebuildPlan {
            needs_rebuild: !dirty_inputs.is_empty(),
            changed_inputs: dirty_inputs,
        })
    }

    async fn record_success(&mut self, root: &Utf8Path, tracked_inputs: BTreeSet<Utf8PathBuf>) {
        self.tracked_inputs = tracked_inputs;
        self.input_hashes.clear();
        for path in &self.tracked_inputs {
            match hash_input(root, path).await {
                Ok(hash) => {
                    self.input_hashes.insert(path.clone(), hash);
                }
                Err(error) => {
                    warn!("failed to hash tracked input {}: {error}", path);
                    self.input_hashes.insert(path.clone(), None);
                }
            }
        }
    }
}

async fn hash_input(root: &Utf8Path, relative_path: &Utf8Path) -> Result<Option<String>> {
    let full_path = root.join(relative_path);
    match tokio::fs::read(full_path.as_std_path()).await {
        Ok(bytes) => Ok(Some(blake3::hash(&bytes).to_hex().to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(anyhow!("failed to read {}: {error}", full_path)),
    }
}

fn spawn_watcher(
    root: Utf8PathBuf,
    rebuild_tx: mpsc::UnboundedSender<Vec<Utf8PathBuf>>,
) -> Result<()> {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(move |result| {
        let _ = event_tx.send(result);
    })
    .context("failed to create filesystem watcher")?;
    watcher
        .watch(root.as_std_path(), RecursiveMode::Recursive)
        .with_context(|| format!("failed to watch {}", root))?;

    tokio::spawn(async move {
        let _watcher = watcher;
        while let Some(result) = event_rx.recv().await {
            match result {
                Ok(event) => {
                    if matches!(event.kind, notify::EventKind::Access(_)) {
                        continue;
                    }
                    if event.paths.is_empty() {
                        continue;
                    }
                    let mut changed_paths = Vec::new();
                    for path in event.paths {
                        if let Ok(relative) = path.strip_prefix(root.as_std_path()) {
                            if let Some(relative) = Utf8Path::from_path(relative) {
                                if relative.starts_with(".latexd") || relative.as_str().is_empty() {
                                    continue;
                                }
                                changed_paths.push(relative.to_path_buf());
                            }
                        }
                    }
                    if !changed_paths.is_empty() && rebuild_tx.send(changed_paths).is_err() {
                        warn!("file watcher dropped rebuild event because build loop is gone");
                        break;
                    }
                }
                Err(error) => {
                    warn!("file watcher error: {error}");
                }
            }
        }
    });

    Ok(())
}

fn viewer_dist_root() -> Utf8PathBuf {
    std::env::var("LATEXD_VIEWER_DIST")
        .ok()
        .filter(|path| !path.is_empty())
        .map(Utf8PathBuf::from)
        .unwrap_or_else(|| {
            Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/apps/viewer/build")
        })
}

fn viewer_dist_file(path: &str) -> Option<Utf8PathBuf> {
    let mut relative = Utf8PathBuf::new();
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        if segment == "." || segment == ".." {
            return None;
        }
        relative.push(segment);
    }
    Some(viewer_dist_root().join(relative))
}

fn viewer_content_type(path: &Utf8Path) -> &'static str {
    match path.extension() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") | Some("map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain; charset=utf-8",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn viewer_build_missing_response() -> Response {
    Html(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>latexd viewer build missing</title></head><body><p>Viewer assets are missing. Run <code>pnpm -C web install</code> and <code>pnpm -C web build</code>.</p></body></html>"
            .to_string(),
    )
    .into_response()
}

async fn serve_viewer_dist_file(path: &str) -> Response {
    let Some(path) = viewer_dist_file(path) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(viewer_content_type(&path)),
            )],
            bytes,
        )
            .into_response(),
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && path.file_name() == Some("index.html") =>
        {
            viewer_build_missing_response()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn index() -> Response {
    serve_viewer_dist_file("index.html").await
}

async fn viewer_asset(Path(path): Path<String>) -> Response {
    if path.is_empty() {
        return index().await;
    }
    serve_viewer_dist_file(&path).await
}

async fn snapshot(State(state): State<Arc<AppState>>) -> axum::Json<PreviewSnapshot> {
    let mut snapshot = state.live.read().await.snapshot.clone();
    if snapshot.source_snapshot.is_empty() {
        snapshot.source_snapshot = build_source_snapshot(&state, snapshot.last_applied_rev).await;
    }
    axum::Json(snapshot)
}

async fn renderer_session_metrics(
    State(state): State<Arc<AppState>>,
) -> axum::Json<RenderSessionMetricsSnapshot> {
    axum::Json(render_session_metrics_snapshot(&state).await)
}

async fn latest_pdf(State(state): State<Arc<AppState>>) -> Response {
    let path = state.live.read().await.latest_pdf_path.clone();
    let Some(path) = path else {
        return (
            StatusCode::NOT_FOUND,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            )],
            "no successful PDF has been produced yet".to_string(),
        )
            .into_response();
    };

    match tokio::fs::read(path.as_std_path()).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/pdf"),
            )],
            bytes,
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            )],
            format!("failed to read preview PDF: {error}"),
        )
            .into_response(),
    }
}

async fn revision_artifact(
    Path((rev, path)): Path<(u64, String)>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Ok(relative_path) = normalize_relative_path(Utf8Path::new(&path)) else {
        return (
            StatusCode::BAD_REQUEST,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            )],
            "invalid artifact path".to_string(),
        )
            .into_response();
    };
    if relative_path.as_str().is_empty()
        || relative_path
            .extension()
            .is_none_or(|extension| extension != "pdf" && extension != "svg")
    {
        return (
            StatusCode::BAD_REQUEST,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            )],
            "invalid artifact path".to_string(),
        )
            .into_response();
    }

    let artifact_path = state
        .build_root
        .join(format!("rev-{rev}"))
        .join(relative_path);
    let content_type = match artifact_path.extension() {
        Some("svg") => "image/svg+xml; charset=utf-8",
        _ => "application/pdf",
    };
    match tokio::fs::read(artifact_path.as_std_path()).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, HeaderValue::from_static(content_type))],
            bytes,
        )
            .into_response(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
            StatusCode::NOT_FOUND,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            )],
            "requested revision artifact was not found".to_string(),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            )],
            format!("failed to read revision artifact: {error}"),
        )
            .into_response(),
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RasterQuery {
    scale: Option<f32>,
    tile_size: Option<u32>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RequiredTilesQuery {
    scale: Option<f32>,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    tile_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TileManifestItem {
    page_id: String,
    zoom_bucket: u16,
    tile_x: u32,
    tile_y: u32,
    png_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TileManifestResponse {
    rev: u64,
    page_id: String,
    tile_size: u32,
    items: Vec<TileManifestItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SyncMapItem {
    item_id: String,
    file: Utf8PathBuf,
    start_utf8: u32,
    end_utf8: u32,
    output_start_utf8: u32,
    output_end_utf8: u32,
    start_line: u32,
    end_line: u32,
    left_px: u32,
    right_px: u32,
    top_px: u32,
    bottom_px: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PageSyncMapResponse {
    rev: u64,
    page_id: String,
    page_width_px: u32,
    page_height_px: u32,
    page_source_start_utf8: u32,
    page_source_end_utf8: u32,
    page_output_start_utf8: u32,
    page_output_end_utf8: u32,
    items: Vec<SyncMapItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum EditorPreviewKind {
    None,
    Uri,
    Command,
    CommandAndUri,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SourceJumpResponse {
    rev: u64,
    file: Utf8PathBuf,
    absolute_file: Utf8PathBuf,
    file_uri: String,
    editor_uri: String,
    editor_preview_kind: EditorPreviewKind,
    offset_utf8: u32,
    line: u32,
    line0: u32,
    column: u32,
    column0: u32,
    source_hash: String,
    editor_cwd: Utf8PathBuf,
    editor_launch_supported: bool,
    editor_program: String,
    editor_args: Vec<String>,
    editor_command_line: String,
    page_id: String,
    page_index: usize,
    page_width_px: u32,
    page_height_px: u32,
    page_source_start_utf8: u32,
    page_source_end_utf8: u32,
    page_output_start_utf8: u32,
    page_output_end_utf8: u32,
    item: SyncMapItem,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RevisionSourceTexts {
    #[serde(default)]
    files: BTreeMap<Utf8PathBuf, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SourceJumpQuery {
    #[serde(default)]
    file: String,
    offset: Option<u32>,
    line: Option<u32>,
    column: Option<u32>,
    source_hash: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SourceFileQuery {
    file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SourceFilesResponse {
    rev: u64,
    files: Vec<Utf8PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SourceFileResponse {
    rev: u64,
    file: Utf8PathBuf,
    content: String,
    line_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct UpdateSourceFileRequest {
    file: String,
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct UpdateSourceFileResponse {
    file: Utf8PathBuf,
    line_count: u32,
    byte_len: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct OpenSourceRequest {
    #[serde(default)]
    file: String,
    offset: Option<u32>,
    line: Option<u32>,
    column: Option<u32>,
    source_hash: Option<String>,
    launch: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct OpenSourceResponse {
    rev: u64,
    file: Utf8PathBuf,
    absolute_file: Utf8PathBuf,
    file_uri: String,
    editor_uri: String,
    editor_preview_kind: EditorPreviewKind,
    offset_utf8: u32,
    line: u32,
    line0: u32,
    column: u32,
    column0: u32,
    source_hash: String,
    editor_cwd: Utf8PathBuf,
    editor_launch_supported: bool,
    editor_program: String,
    editor_args: Vec<String>,
    editor_command_line: String,
    page_id: Option<String>,
    page_index: Option<usize>,
    page_width_px: Option<u32>,
    page_height_px: Option<u32>,
    page_source_start_utf8: Option<u32>,
    page_source_end_utf8: Option<u32>,
    page_output_start_utf8: Option<u32>,
    page_output_end_utf8: Option<u32>,
    item: Option<SyncMapItem>,
    launched: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EditorBridgePreview {
    file_uri: String,
    editor_uri: String,
    editor_preview_kind: EditorPreviewKind,
    editor_cwd: Utf8PathBuf,
    editor_launch_supported: bool,
    editor_program: String,
    editor_args: Vec<String>,
    editor_command_line: String,
}

async fn revision_page_png(
    Path((rev, page_png)): Path<(u64, String)>,
    Query(query): Query<RasterQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(page_id) = parse_png_path_suffix(&page_png) else {
        return text_response(StatusCode::NOT_FOUND, "requested page id was not found");
    };
    let page = match load_revision_page_input(&state, rev, &page_id).await {
        Ok(page) => page,
        Err(response) => return response,
    };
    let scale = query.scale.unwrap_or(1.0).max(0.1);
    let image = match cache_raster_image(&state, &page, scale).await {
        Ok(image) => image,
        Err(error) => {
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to render page: {error}"),
            );
        }
    };
    png_response(image)
}

async fn page_syncmap(
    Path((rev, page_id)): Path<(u64, String)>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let source_texts = load_revision_source_texts(&state.build_root, rev).await;
    if let Ok(Some(syncmap_pages)) = load_revision_syncmap_artifacts(&state.build_root, rev).await {
        let Some(page) = syncmap_pages
            .into_iter()
            .find(|page| page.page_id == page_id)
        else {
            return text_response(StatusCode::NOT_FOUND, "requested page id was not found");
        };
        let page_output_start_utf8 = page
            .items
            .iter()
            .map(|item| item.output_start_utf8)
            .min()
            .unwrap_or(0);
        let page_output_end_utf8 = page
            .items
            .iter()
            .map(|item| item.output_end_utf8)
            .max()
            .unwrap_or(page_output_start_utf8);
        let page_source_start_utf8 = page
            .items
            .iter()
            .map(|item| item.start_utf8)
            .min()
            .unwrap_or(0);
        let page_source_end_utf8 = page
            .items
            .iter()
            .map(|item| item.end_utf8)
            .max()
            .unwrap_or(page_source_start_utf8);
        return axum::Json(PageSyncMapResponse {
            rev,
            page_id,
            page_width_px: page.width_pt,
            page_height_px: page.height_pt,
            page_source_start_utf8,
            page_source_end_utf8,
            page_output_start_utf8,
            page_output_end_utf8,
            items: build_sync_items_from_artifacts(&page, &source_texts),
        })
        .into_response();
    }
    let page = match load_revision_page_metadata(&state, rev, &page_id).await {
        Ok(page) => page,
        Err(response) => return response,
    };
    let items = build_page_sync_items(&page, &source_texts);

    axum::Json(PageSyncMapResponse {
        rev,
        page_id,
        page_width_px: page.width_pt,
        page_height_px: page.height_pt,
        page_source_start_utf8: items.iter().map(|item| item.start_utf8).min().unwrap_or(0),
        page_source_end_utf8: items.iter().map(|item| item.end_utf8).max().unwrap_or(0),
        page_output_start_utf8: page.text_start_utf8,
        page_output_end_utf8: page.text_end_utf8,
        items,
    })
    .into_response()
}

async fn source_jump(
    Path(rev): Path<u64>,
    Query(query): Query<SourceJumpQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let parsed_source_hash = parse_canonical_source_hash(query.source_hash.as_deref());
    let file = if let Some((file, _, _, _)) = parsed_source_hash.as_ref() {
        file.clone()
    } else {
        if query.file.trim().is_empty() {
            return text_response(
                StatusCode::BAD_REQUEST,
                "source jump requires either a source file or source hash",
            );
        }
        let Ok(file) = normalize_relative_path(Utf8Path::new(&query.file)) else {
            return text_response(StatusCode::BAD_REQUEST, "invalid source file path");
        };
        file
    };
    let source_texts = load_revision_source_texts(&state.build_root, rev).await;
    let requested_offset = if let Some(offset) = parsed_source_hash
        .as_ref()
        .and_then(|(_, offset, _, _)| *offset)
    {
        offset
    } else if let Some(line) = parsed_source_hash
        .as_ref()
        .and_then(|(_, _, line, _)| *line)
    {
        let Some(text) = source_texts.get(&file) else {
            return text_response(
                StatusCode::NOT_FOUND,
                "requested source file was not found in the revision snapshot",
            );
        };
        source_line_column_offset(
            text,
            line,
            parsed_source_hash
                .as_ref()
                .and_then(|(_, _, _, column)| *column)
                .or(query.column),
        )
    } else if let Some(offset) = query.offset {
        offset
    } else if let Some(line) = query.line {
        let Some(text) = source_texts.get(&file) else {
            return text_response(
                StatusCode::NOT_FOUND,
                "requested source file was not found in the revision snapshot",
            );
        };
        source_line_column_offset(text, line, query.column)
    } else {
        return text_response(
            StatusCode::BAD_REQUEST,
            "source jump requires either an offset or a line",
        );
    };
    let jump =
        match resolve_source_jump_response(&state, rev, &file, requested_offset, &source_texts)
            .await
        {
            Ok(Some(jump)) => jump,
            Ok(None) => {
                return text_response(
                    StatusCode::NOT_FOUND,
                    "requested source location was not found in the revision syncmap",
                );
            }
            Err(response) => return response,
        };

    axum::Json(jump).into_response()
}

async fn source_file(
    Path(rev): Path<u64>,
    Query(query): Query<SourceFileQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Ok(file) = normalize_relative_path(Utf8Path::new(&query.file)) else {
        return text_response(StatusCode::BAD_REQUEST, "invalid source file path");
    };
    let source_texts = load_revision_source_texts(&state.build_root, rev).await;
    let content = if let Some(content) = source_texts.get(&file).cloned() {
        content
    } else {
        match tokio::fs::read_to_string(state.root.join(&file).as_std_path()).await {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return text_response(StatusCode::NOT_FOUND, "requested source file was not found");
            }
            Err(error) => {
                return text_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("failed to read source file: {error}"),
                );
            }
        }
    };
    axum::Json(SourceFileResponse {
        rev,
        file,
        line_count: source_line_count(&content),
        content,
    })
    .into_response()
}

async fn source_files(Path(rev): Path<u64>, State(state): State<Arc<AppState>>) -> Response {
    let mut files = build_source_snapshot(&state, rev)
        .await
        .into_iter()
        .map(|entry| Utf8PathBuf::from(entry.file))
        .collect::<Vec<_>>();
    if files.is_empty() {
        files = state.world.manifest.toplevels.clone();
    }
    axum::Json(SourceFilesResponse { rev, files }).into_response()
}

async fn update_source_file(
    State(state): State<Arc<AppState>>,
    Json(request): Json<UpdateSourceFileRequest>,
) -> Response {
    let Ok(file) = normalize_relative_path(Utf8Path::new(&request.file)) else {
        return text_response(StatusCode::BAD_REQUEST, "invalid source file path");
    };
    if file.as_str().is_empty() {
        return text_response(StatusCode::BAD_REQUEST, "invalid source file path");
    }
    let absolute_file = state.root.join(&file);
    match tokio::fs::metadata(absolute_file.as_std_path()).await {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => {
            return text_response(
                StatusCode::BAD_REQUEST,
                "requested source path does not point to a file",
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return text_response(StatusCode::NOT_FOUND, "requested source file was not found");
        }
        Err(error) => {
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to access source file: {error}"),
            );
        }
    }
    if let Err(error) = tokio::fs::write(absolute_file.as_std_path(), &request.content).await {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("failed to write source file: {error}"),
        );
    }
    axum::Json(UpdateSourceFileResponse {
        file,
        line_count: source_line_count(&request.content),
        byte_len: request.content.len() as u64,
    })
    .into_response()
}

async fn open_source(
    Path(rev): Path<u64>,
    State(state): State<Arc<AppState>>,
    Json(request): Json<OpenSourceRequest>,
) -> Response {
    let launch_requested = request.launch.unwrap_or(true);
    let should_launch = launch_requested && state.editor_bridge.is_some();
    let parsed_source_hash = parse_canonical_source_hash(request.source_hash.as_deref());
    let file = if let Some((file, _, _, _)) = parsed_source_hash.as_ref() {
        file.clone()
    } else {
        if request.file.trim().is_empty() {
            return text_response(
                StatusCode::BAD_REQUEST,
                "open source requires either a source file or source hash",
            );
        }
        let Ok(file) = normalize_relative_path(Utf8Path::new(&request.file)) else {
            return text_response(StatusCode::BAD_REQUEST, "invalid source file path");
        };
        file
    };
    let source_texts = load_revision_source_texts(&state.build_root, rev).await;
    let source_text = if let Some(text) = source_texts.get(&file) {
        Some(text.clone())
    } else {
        tokio::fs::read_to_string(state.root.join(&file).as_std_path())
            .await
            .ok()
    };
    let requested_offset = parsed_source_hash
        .as_ref()
        .and_then(|(_, offset, _, _)| *offset)
        .or(request.offset);
    let requested_line = parsed_source_hash
        .as_ref()
        .and_then(|(_, _, line, _)| *line)
        .or(request.line);
    let requested_column = parsed_source_hash
        .as_ref()
        .and_then(|(_, _, _, column)| *column)
        .or(request.column);
    let offset_utf8 = if let Some(offset) = requested_offset {
        offset
    } else if let (Some(line), Some(text)) = (requested_line, source_text.as_ref()) {
        source_line_column_offset(text, line, requested_column)
    } else {
        return text_response(
            StatusCode::BAD_REQUEST,
            "open source requires either an offset or a line",
        );
    };
    let line = if let Some(line) = requested_line {
        line.max(1)
    } else {
        source_offset_line(source_text.as_ref(), offset_utf8)
    };
    let line0 = line.saturating_sub(1);
    let column = source_offset_column(source_text.as_ref(), offset_utf8);
    let column0 = column.saturating_sub(1);
    let absolute_file = state.root.join(&file);
    if tokio::fs::metadata(absolute_file.as_std_path())
        .await
        .is_err()
    {
        return text_response(StatusCode::NOT_FOUND, "requested source file was not found");
    }
    let source_context =
        match resolve_source_jump_response(&state, rev, &file, offset_utf8, &source_texts).await {
            Ok(context) => context,
            Err(response) if response.status() == StatusCode::NOT_FOUND => None,
            Err(response) => return response,
        };
    let source_hash = source_selection_hash(&file, line, column);
    let editor_preview = build_editor_bridge_preview(
        &state,
        &file,
        &absolute_file,
        rev,
        offset_utf8,
        line,
        line0,
        column,
        column0,
        &source_hash,
        source_context.as_ref(),
    );
    if should_launch {
        if let Some(editor_bridge) = state.editor_bridge.as_ref() {
            let mut command = Command::new(&editor_bridge.program);
            command.args(&editor_preview.editor_args);
            command.current_dir(&state.root);
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if let Err(error) = command.spawn() {
                return text_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("failed to launch editor bridge command: {error}"),
                );
            }
        }
    }
    axum::Json(OpenSourceResponse {
        rev,
        file,
        absolute_file,
        file_uri: editor_preview.file_uri,
        editor_uri: editor_preview.editor_uri,
        editor_preview_kind: editor_preview.editor_preview_kind,
        offset_utf8,
        line,
        line0,
        column,
        column0,
        source_hash,
        editor_cwd: editor_preview.editor_cwd,
        editor_launch_supported: editor_preview.editor_launch_supported,
        editor_program: editor_preview.editor_program,
        editor_args: editor_preview.editor_args,
        editor_command_line: editor_preview.editor_command_line,
        page_id: source_context
            .as_ref()
            .map(|context| context.page_id.clone()),
        page_index: source_context.as_ref().map(|context| context.page_index),
        page_width_px: source_context.as_ref().map(|context| context.page_width_px),
        page_height_px: source_context
            .as_ref()
            .map(|context| context.page_height_px),
        page_source_start_utf8: source_context
            .as_ref()
            .map(|context| context.page_source_start_utf8),
        page_source_end_utf8: source_context
            .as_ref()
            .map(|context| context.page_source_end_utf8),
        page_output_start_utf8: source_context
            .as_ref()
            .map(|context| context.page_output_start_utf8),
        page_output_end_utf8: source_context
            .as_ref()
            .map(|context| context.page_output_end_utf8),
        item: source_context.map(|context| context.item),
        launched: should_launch,
    })
    .into_response()
}

async fn revision_tile_png(
    Path((rev, page_id, zoom_bucket, tile_x, tile_y_png)): Path<(u64, String, u16, u32, String)>,
    Query(query): Query<RasterQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(tile_y) = parse_png_u32_path_suffix(&tile_y_png) else {
        return text_response(StatusCode::NOT_FOUND, "requested tile was not found");
    };
    let page = match load_revision_page_input(&state, rev, &page_id).await {
        Ok(page) => page,
        Err(response) => return response,
    };
    let tile_size = query.tile_size.unwrap_or(256).max(1);
    let scale = (zoom_bucket as f32 / 100.0).max(0.1);
    let scaled_width = ((page.width_px as f32 * scale).round() as u32).max(1);
    let scaled_height = ((page.height_px as f32 * scale).round() as u32).max(1);
    let x = tile_x.saturating_mul(tile_size);
    let y = tile_y.saturating_mul(tile_size);
    if x >= scaled_width || y >= scaled_height {
        return text_response(
            StatusCode::NOT_FOUND,
            "requested tile is outside the page bounds",
        );
    }
    let rect = Rect {
        x,
        y,
        width: (scaled_width - x).min(tile_size),
        height: (scaled_height - y).min(tile_size),
    };
    let rendered_tile = {
        let rects = vec![rect.clone()];
        let mut rendered_tile = None;
        for _ in 0..2 {
            let handle = render_session_handle(&state);
            let (reply_tx, reply_rx) = oneshot::channel();
            if handle
                .tx
                .send(RenderSessionRequest::RenderTiles {
                    page: page.clone(),
                    scale,
                    rects: rects.clone(),
                    reply: reply_tx,
                })
                .await
                .is_err()
            {
                note_render_session_failure(
                    &state,
                    Some(page.revision),
                    Some(&page.page_id),
                    Some(scale),
                );
                drop_render_session_handle(&state);
                continue;
            }
            match reply_rx.await {
                Ok(result) => match result {
                    Ok(mut tiles) => {
                        rendered_tile = tiles.pop();
                        break;
                    }
                    Err(error) => {
                        return text_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            &format!("failed to render tile: {error}"),
                        );
                    }
                },
                Err(_) => {
                    note_render_session_failure(
                        &state,
                        Some(page.revision),
                        Some(&page.page_id),
                        Some(scale),
                    );
                    drop_render_session_handle(&state);
                }
            }
        }
        if let Some(tile) = rendered_tile {
            tile
        } else {
            let metrics = render_session_metrics_handle(&state);
            metrics
                .fallback_tile_render_count
                .fetch_add(1, AtomicOrdering::SeqCst);
            warn!(
                rev = page.revision,
                page_id = %page.page_id,
                scale,
                "renderer session unavailable for tile request, falling back to direct tile render path"
            );
            let started = std::time::Instant::now();
            match state.tile_renderer.render_tiles(&page, scale, &[rect]) {
                Ok(mut tiles) => {
                    metrics.record_event(
                        RenderSessionEventKind::FallbackRenderTiles,
                        Some(page.revision),
                        Some(&page.page_id),
                        Some(scale),
                        Some(1),
                        Some(started.elapsed()),
                    );
                    match tiles.pop() {
                        Some(tile) => tile,
                        None => {
                            return text_response(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "tile renderer returned no tiles for requested rectangle",
                            );
                        }
                    }
                }
                Err(error) => {
                    metrics.record_event(
                        RenderSessionEventKind::FallbackRenderTiles,
                        Some(page.revision),
                        Some(&page.page_id),
                        Some(scale),
                        Some(1),
                        Some(started.elapsed()),
                    );
                    return text_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &format!("failed to render tile: {error}"),
                    );
                }
            }
        }
    };
    png_response(rendered_tile.image)
}

fn parse_png_path_suffix(segment: &str) -> Option<String> {
    segment.strip_suffix(".png").map(ToString::to_string)
}

fn parse_png_u32_path_suffix(segment: &str) -> Option<u32> {
    parse_png_path_suffix(segment)?.parse().ok()
}

async fn required_tiles(
    Path((rev, page_id)): Path<(u64, String)>,
    Query(query): Query<RequiredTilesQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let page = match load_revision_page_input(&state, rev, &page_id).await {
        Ok(page) => page,
        Err(response) => return response,
    };
    let tile_size = query.tile_size.unwrap_or(256).max(1);
    let scale = query.scale.unwrap_or(1.0).max(0.1);
    let zoom_bucket = (scale * 100.0).round().clamp(1.0, u16::MAX as f32) as u16;
    let tiles = required_tiles_for_viewport(
        &page,
        scale,
        &Viewport {
            left: query.left,
            top: query.top,
            width: query.width,
            height: query.height,
        },
        tile_size,
    );
    let response = TileManifestResponse {
        rev,
        page_id: page_id.clone(),
        tile_size,
        items: tiles
            .into_iter()
            .map(|tile| TileManifestItem {
                page_id: tile.key.page_id,
                zoom_bucket,
                tile_x: tile.key.tile_x,
                tile_y: tile.key.tile_y,
                png_url: format!(
                    "{}",
                    viewer_prefixed_path(&format!(
                        "/artifacts/rev/{rev}/tiles/{page_id}/{zoom_bucket}/{}/{}.png",
                        tile.key.tile_x, tile.key.tile_y
                    ))
                ),
            })
            .collect(),
    };

    axum::Json(response).into_response()
}

async fn load_revision_page_input(
    state: &Arc<AppState>,
    rev: u64,
    page_id: &str,
) -> std::result::Result<PageRenderInput, Response> {
    if let Some(page) = lookup_attached_page_input(state, rev, page_id).await {
        return Ok(page);
    }
    let pages = load_revision_page_metadata_set(state, rev).await?;
    let page_inputs = pages
        .into_iter()
        .map(|page| page_input_from_metadata(&state.build_root, rev, page))
        .collect::<Vec<_>>();
    let Some(page) = page_inputs
        .iter()
        .find(|page| page.page_id == page_id)
        .cloned()
    else {
        return Err(text_response(
            StatusCode::NOT_FOUND,
            "requested page id was not found",
        ));
    };
    Ok(page)
}

async fn load_revision_page_metadata(
    state: &Arc<AppState>,
    rev: u64,
    page_id: &str,
) -> std::result::Result<PageArtifactMeta, Response> {
    let pages = load_revision_page_metadata_set(state, rev).await?;
    let Some(page) = pages.into_iter().find(|page| page.page_id == page_id) else {
        return Err(text_response(
            StatusCode::NOT_FOUND,
            "requested page id was not found",
        ));
    };

    Ok(page)
}

async fn load_revision_page_metadata_set(
    state: &Arc<AppState>,
    rev: u64,
) -> std::result::Result<Vec<PageArtifactMeta>, Response> {
    if let Some(pages) = lookup_attached_revision_pages(state, rev).await {
        return Ok(pages);
    }
    let build_root = &state.build_root;
    let metadata_path = build_root.join(format!("rev-{rev}/page-metadata.json"));
    let bytes = tokio::fs::read(metadata_path.as_std_path())
        .await
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => text_response(
                StatusCode::NOT_FOUND,
                "page metadata for requested revision was not found",
            ),
            _ => text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to read page metadata: {error}"),
            ),
        })?;
    let pages: Vec<PageArtifactMeta> = serde_json::from_slice(&bytes).map_err(|error| {
        text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("failed to decode page metadata: {error}"),
        )
    })?;
    attach_render_revision(state, rev, &pages).await;
    Ok(pages)
}

async fn load_revision_source_texts(
    build_root: &Utf8Path,
    rev: u64,
) -> BTreeMap<Utf8PathBuf, String> {
    match tokio::fs::read(
        build_root
            .join(format!("rev-{rev}/sources.json"))
            .as_std_path(),
    )
    .await
    {
        Ok(bytes) => serde_json::from_slice::<RevisionSourceTexts>(&bytes)
            .map(|snapshot| snapshot.files)
            .unwrap_or_default(),
        Err(_) => BTreeMap::new(),
    }
}

async fn load_live_workspace_source_snapshot(
    root: &Utf8Path,
    files: &[Utf8PathBuf],
) -> Vec<SourceSnapshotFile> {
    let mut snapshot = Vec::new();
    for file in files {
        match tokio::fs::read_to_string(root.join(file).as_std_path()).await {
            Ok(content) => snapshot.push(SourceSnapshotFile {
                file: file.to_string(),
                line_count: source_line_count(&content) as usize,
                content,
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => warn!("failed to read live source file {}: {error}", file),
        }
    }
    snapshot
}

async fn build_source_snapshot(state: &AppState, rev: u64) -> Vec<SourceSnapshotFile> {
    let source_texts = load_revision_source_texts(&state.build_root, rev).await;
    if !source_texts.is_empty() {
        return source_texts
            .into_iter()
            .map(|(file, content)| SourceSnapshotFile {
                file: file.to_string(),
                line_count: source_line_count(&content) as usize,
                content,
            })
            .collect();
    }
    load_live_workspace_source_snapshot(&state.root, &state.world.manifest.toplevels).await
}

fn source_line_count(content: &str) -> u32 {
    content
        .as_bytes()
        .iter()
        .filter(|byte| **byte == b'\n')
        .count() as u32
        + 1
}

async fn load_revision_syncmap_artifacts(
    build_root: &Utf8Path,
    rev: u64,
) -> std::result::Result<Option<Vec<PageSyncMapArtifact>>, Response> {
    let syncmap_path = build_root.join(format!("rev-{rev}/page-syncmap.json"));
    let bytes = match tokio::fs::read(syncmap_path.as_std_path()).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to read page syncmap: {error}"),
            ));
        }
    };
    let pages: Vec<PageSyncMapArtifact> = serde_json::from_slice(&bytes).map_err(|error| {
        text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("failed to decode page syncmap: {error}"),
        )
    })?;
    Ok(Some(pages))
}

fn source_offset_line(source_text: Option<&String>, offset: u32) -> u32 {
    let Some(text) = source_text else {
        return 1;
    };
    let clamped = usize::try_from(offset)
        .ok()
        .map(|value| value.min(text.len()))
        .unwrap_or(text.len());
    text.as_bytes()[..clamped]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count() as u32
        + 1
}

fn source_offset_column(source_text: Option<&String>, offset: u32) -> u32 {
    let Some(text) = source_text else {
        return 1;
    };
    let clamped = usize::try_from(offset)
        .ok()
        .map(|value| value.min(text.len()))
        .unwrap_or(text.len());
    let line_start = text.as_bytes()[..clamped]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    text[line_start..clamped].chars().count() as u32 + 1
}

fn source_line_offset(source_text: &str, line: u32) -> u32 {
    let target_line = usize::try_from(line.max(1)).unwrap_or(usize::MAX);
    let mut current_line = 1usize;
    let mut line_offset = 0usize;
    if target_line > 1 {
        for (index, byte) in source_text.as_bytes().iter().enumerate() {
            if *byte == b'\n' {
                current_line += 1;
                line_offset = index + 1;
                if current_line == target_line {
                    break;
                }
            }
        }
        if current_line < target_line {
            line_offset = source_text.len();
        }
    }
    u32::try_from(line_offset).unwrap_or(u32::MAX)
}

fn source_line_column_offset(source_text: &str, line: u32, column: Option<u32>) -> u32 {
    let mut offset = usize::try_from(source_line_offset(source_text, line))
        .unwrap_or(source_text.len())
        .min(source_text.len());
    let mut remaining_columns = usize::try_from(column.unwrap_or(1).max(1)).unwrap_or(usize::MAX);
    if remaining_columns <= 1 {
        return u32::try_from(offset).unwrap_or(u32::MAX);
    }
    remaining_columns -= 1;
    for ch in source_text[offset..].chars() {
        if remaining_columns == 0 || ch == '\n' {
            break;
        }
        offset += ch.len_utf8();
        remaining_columns -= 1;
    }
    u32::try_from(offset).unwrap_or(u32::MAX)
}

fn source_selection_hash(file: &Utf8Path, line: u32, column: u32) -> String {
    let mut source_hash = String::from("#src=");
    for byte in file.as_str().bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            source_hash.push(byte as char);
        } else {
            source_hash.push('%');
            source_hash.push_str(&format!("{byte:02X}"));
        }
    }
    source_hash.push_str(&format!("&line={}", line.max(1)));
    if column > 1 {
        source_hash.push_str(&format!("&column={column}"));
    }
    source_hash
}

fn parse_canonical_source_hash(
    value: Option<&str>,
) -> Option<(Utf8PathBuf, Option<u32>, Option<u32>, Option<u32>)> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    let mut file = None;
    let mut offset = None;
    let mut line = None;
    let mut column = None;
    for part in value.strip_prefix('#').unwrap_or(value).split('&') {
        let Some((key, raw_value)) = part.split_once('=') else {
            continue;
        };
        match key {
            "src" => {
                let mut decoded = Vec::with_capacity(raw_value.len());
                let bytes = raw_value.as_bytes();
                let mut index = 0usize;
                while index < bytes.len() {
                    if bytes[index] == b'%' && index + 2 < bytes.len() {
                        let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
                        decoded.push(u8::from_str_radix(hex, 16).ok()?);
                        index += 3;
                    } else {
                        decoded.push(bytes[index]);
                        index += 1;
                    }
                }
                let decoded = String::from_utf8(decoded).ok()?;
                file = normalize_relative_path(Utf8Path::new(&decoded)).ok();
            }
            "offset" => {
                offset = raw_value.parse::<u32>().ok();
            }
            "line" => {
                line = raw_value.parse::<u32>().ok().map(|value| value.max(1));
            }
            "column" => {
                column = raw_value.parse::<u32>().ok().map(|value| value.max(1));
            }
            _ => {}
        }
    }
    file.map(|file| (file, offset, line, column))
}

fn source_file_uri(path: &Utf8Path) -> String {
    let mut file_uri = String::from("file://");
    for byte in path.as_str().bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            file_uri.push(byte as char);
        } else {
            file_uri.push('%');
            file_uri.push_str(&format!("{byte:02X}"));
        }
    }
    file_uri
}

fn source_editor_uri(program: &str, path: &Utf8Path, line: u32, column: u32) -> String {
    let scheme = match std::path::Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
    {
        "code" => "vscode",
        "code-insiders" => "vscode-insiders",
        "codium" => "vscodium",
        "cursor" => "cursor",
        _ => return String::new(),
    };
    let mut uri = format!("{scheme}://file");
    uri.push_str(
        source_file_uri(path)
            .strip_prefix("file://")
            .unwrap_or(path.as_str()),
    );
    uri.push(':');
    uri.push_str(&line.max(1).to_string());
    uri.push(':');
    uri.push_str(&column.max(1).to_string());
    uri
}

fn editor_preview_kind(editor_uri: &str, editor_command_line: &str) -> EditorPreviewKind {
    match (!editor_uri.is_empty(), !editor_command_line.is_empty()) {
        (false, false) => EditorPreviewKind::None,
        (true, false) => EditorPreviewKind::Uri,
        (false, true) => EditorPreviewKind::Command,
        (true, true) => EditorPreviewKind::CommandAndUri,
    }
}

fn format_editor_command_line(editor_program: &str, materialized_args: &[String]) -> String {
    if editor_program.is_empty() {
        return String::new();
    }
    std::iter::once(editor_program)
        .chain(materialized_args.iter().map(String::as_str))
        .map(|argument| {
            if argument.is_empty() {
                "''".to_string()
            } else if argument.chars().all(|ch| {
                ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':' | '=')
            }) {
                argument.to_string()
            } else {
                format!("'{}'", argument.replace('\'', "'\"'\"'"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_editor_bridge_preview(
    state: &AppState,
    file: &Utf8Path,
    absolute_file: &Utf8Path,
    rev: u64,
    offset_utf8: u32,
    line: u32,
    line0: u32,
    column: u32,
    column0: u32,
    source_hash: &str,
    source_context: Option<&SourceJumpResponse>,
) -> EditorBridgePreview {
    let file_uri = source_file_uri(absolute_file);
    let editor_uri = state
        .editor_bridge
        .as_ref()
        .map(|bridge| source_editor_uri(&bridge.program, absolute_file, line, column))
        .unwrap_or_default();
    let mut materialized_args = Vec::new();
    if let Some(editor_bridge) = state.editor_bridge.as_ref() {
        if editor_bridge.args.is_empty() {
            let program_name = std::path::Path::new(&editor_bridge.program)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if matches!(program_name, "code" | "code-insiders" | "codium" | "cursor") {
                materialized_args.push("--goto".to_string());
                materialized_args.push(format!("{absolute_file}:{line}:{column}"));
            } else if matches!(program_name, "vim" | "nvim" | "vi") {
                materialized_args.push(format!("+{line}"));
                materialized_args.push(absolute_file.as_str().to_string());
            } else if program_name == "emacsclient" {
                materialized_args.push(format!("+{line}:{column}"));
                materialized_args.push(absolute_file.as_str().to_string());
            } else {
                materialized_args.push(absolute_file.as_str().to_string());
            }
        } else {
            let page_id = source_context
                .as_ref()
                .map(|context| context.page_id.as_str())
                .unwrap_or_default();
            let page_index = source_context
                .as_ref()
                .map(|context| context.page_index.to_string())
                .unwrap_or_default();
            let page_width_px = source_context
                .as_ref()
                .map(|context| context.page_width_px.to_string())
                .unwrap_or_default();
            let page_height_px = source_context
                .as_ref()
                .map(|context| context.page_height_px.to_string())
                .unwrap_or_default();
            let page_source_start_utf8 = source_context
                .as_ref()
                .map(|context| context.page_source_start_utf8.to_string())
                .unwrap_or_default();
            let page_source_end_utf8 = source_context
                .as_ref()
                .map(|context| context.page_source_end_utf8.to_string())
                .unwrap_or_default();
            let page_output_start_utf8 = source_context
                .as_ref()
                .map(|context| context.page_output_start_utf8.to_string())
                .unwrap_or_default();
            let page_output_end_utf8 = source_context
                .as_ref()
                .map(|context| context.page_output_end_utf8.to_string())
                .unwrap_or_default();
            let item_file = source_context
                .as_ref()
                .map(|context| context.item.file.as_str())
                .unwrap_or_default();
            let item_start_utf8 = source_context
                .as_ref()
                .map(|context| context.item.start_utf8.to_string())
                .unwrap_or_default();
            let item_end_utf8 = source_context
                .as_ref()
                .map(|context| context.item.end_utf8.to_string())
                .unwrap_or_default();
            let item_output_start_utf8 = source_context
                .as_ref()
                .map(|context| context.item.output_start_utf8.to_string())
                .unwrap_or_default();
            let item_output_end_utf8 = source_context
                .as_ref()
                .map(|context| context.item.output_end_utf8.to_string())
                .unwrap_or_default();
            let item_id = source_context
                .as_ref()
                .map(|context| context.item.item_id.as_str())
                .unwrap_or_default();
            for argument in &editor_bridge.args {
                materialized_args.push(
                    argument
                        .replace("{root}", state.root.as_str())
                        .replace("{editor_cwd}", state.root.as_str())
                        .replace("{file}", file.as_str())
                        .replace("{abs_file}", absolute_file.as_str())
                        .replace("{absolute_file}", absolute_file.as_str())
                        .replace("{file_uri}", &file_uri)
                        .replace("{editor_uri}", &editor_uri)
                        .replace("{rev}", &rev.to_string())
                        .replace("{line}", &line.to_string())
                        .replace("{line0}", &line0.to_string())
                        .replace("{column}", &column.to_string())
                        .replace("{column0}", &column0.to_string())
                        .replace("{offset}", &offset_utf8.to_string())
                        .replace("{source_hash}", source_hash)
                        .replace("{page_id}", page_id)
                        .replace("{page_index}", &page_index)
                        .replace("{page_width}", &page_width_px)
                        .replace("{page_height}", &page_height_px)
                        .replace("{page_source_start}", &page_source_start_utf8)
                        .replace("{page_source_end}", &page_source_end_utf8)
                        .replace("{page_output_start}", &page_output_start_utf8)
                        .replace("{page_output_end}", &page_output_end_utf8)
                        .replace("{item_file}", item_file)
                        .replace("{item_start}", &item_start_utf8)
                        .replace("{item_end}", &item_end_utf8)
                        .replace("{item_output_start}", &item_output_start_utf8)
                        .replace("{item_output_end}", &item_output_end_utf8)
                        .replace("{item_id}", item_id),
                );
            }
        }
    }
    let editor_program = state
        .editor_bridge
        .as_ref()
        .map(|bridge| bridge.program.clone())
        .unwrap_or_default();
    let preview_kind = editor_preview_kind(
        &editor_uri,
        if editor_program.is_empty() {
            ""
        } else {
            "<command>"
        },
    );
    let preview_kind_label = match preview_kind {
        EditorPreviewKind::None => "none",
        EditorPreviewKind::Uri => "uri",
        EditorPreviewKind::Command => "command",
        EditorPreviewKind::CommandAndUri => "command_and_uri",
    };
    let preview_materialized_args = if editor_program.is_empty() {
        materialized_args.clone()
    } else {
        materialized_args
            .iter()
            .map(|argument| {
                argument
                    .replace("{editor_preview_kind}", preview_kind_label)
                    .replace("{editor_program}", &editor_program)
                    .replace("{editor_command_line}", "")
            })
            .collect::<Vec<_>>()
    };
    let preview_editor_command_line =
        format_editor_command_line(&editor_program, &preview_materialized_args);
    let materialized_args = if editor_program.is_empty() {
        materialized_args
    } else {
        materialized_args
            .into_iter()
            .map(|argument| {
                argument
                    .replace("{editor_preview_kind}", preview_kind_label)
                    .replace("{editor_program}", &editor_program)
                    .replace("{editor_command_line}", &preview_editor_command_line)
            })
            .collect::<Vec<_>>()
    };
    let editor_command_line = preview_editor_command_line;
    let editor_preview_kind = editor_preview_kind(&editor_uri, &editor_command_line);
    EditorBridgePreview {
        file_uri,
        editor_uri,
        editor_preview_kind,
        editor_cwd: state.root.clone(),
        editor_launch_supported: state.editor_bridge.is_some(),
        editor_program,
        editor_args: materialized_args,
        editor_command_line,
    }
}

fn normalize_sync_item_bounds(
    page_width_px: u32,
    page_height_px: u32,
    left_px: u32,
    right_px: u32,
    top_px: u32,
    bottom_px: u32,
    fallback_top_px: u32,
    fallback_bottom_px: u32,
) -> (u32, u32, u32, u32) {
    let page_width_px = page_width_px.max(1);
    let page_height_px = page_height_px.max(1);
    let fallback_top_px = fallback_top_px.min(page_height_px.saturating_sub(1));
    let fallback_bottom_px = fallback_bottom_px
        .max(fallback_top_px.saturating_add(1))
        .min(page_height_px);
    let (left_px, right_px) = if right_px > left_px {
        let left_px = left_px.min(page_width_px.saturating_sub(1));
        let right_px = right_px.min(page_width_px).max(left_px.saturating_add(1));
        (left_px, right_px)
    } else {
        (0, page_width_px)
    };
    let (top_px, bottom_px) = if bottom_px > top_px {
        let top_px = top_px.min(page_height_px.saturating_sub(1));
        let bottom_px = bottom_px.min(page_height_px).max(top_px.saturating_add(1));
        (top_px, bottom_px)
    } else {
        (fallback_top_px, fallback_bottom_px)
    };
    (left_px, right_px, top_px, bottom_px)
}

fn build_sync_items_from_artifacts(
    page: &PageSyncMapArtifact,
    source_texts: &BTreeMap<Utf8PathBuf, String>,
) -> Vec<SyncMapItem> {
    let total_output = page
        .items
        .iter()
        .map(|item| {
            item.output_end_utf8
                .max(item.output_start_utf8.saturating_add(1))
        })
        .max()
        .unwrap_or(1);
    let mut items = Vec::with_capacity(page.items.len());
    for item in &page.items {
        let output_start_utf8 = item.output_start_utf8;
        let output_end_utf8 = item
            .output_end_utf8
            .max(item.output_start_utf8.saturating_add(1));
        let fallback_top_px = ((u64::from(output_start_utf8) * u64::from(page.height_pt))
            / u64::from(total_output.max(1))) as u32;
        let fallback_bottom_px = ((u64::from(output_end_utf8) * u64::from(page.height_pt))
            / u64::from(total_output.max(1))) as u32;
        let source_text = source_texts.get(&item.file);
        let start_line = source_offset_line(source_text, item.start_utf8);
        let end_line = if item.end_utf8 <= item.start_utf8 {
            start_line
        } else {
            source_offset_line(source_text, item.end_utf8.saturating_sub(1))
        };
        let item_id = format!(
            "{}:{}:{}:{}:{}:{}",
            page.page_id, item.file, item.start_utf8, item.end_utf8, start_line, end_line
        );
        let (left_px, right_px, top_px, bottom_px) = normalize_sync_item_bounds(
            page.width_pt,
            page.height_pt,
            item.left_px,
            item.right_px,
            item.top_px,
            item.bottom_px,
            fallback_top_px,
            fallback_bottom_px,
        );
        items.push(SyncMapItem {
            item_id,
            file: item.file.clone(),
            start_utf8: item.start_utf8,
            end_utf8: item.end_utf8,
            output_start_utf8,
            output_end_utf8,
            start_line,
            end_line,
            left_px,
            right_px,
            top_px,
            bottom_px,
        });
    }
    items
}

fn build_page_sync_items(
    page: &PageArtifactMeta,
    source_texts: &BTreeMap<Utf8PathBuf, String>,
) -> Vec<SyncMapItem> {
    let total_weight = page
        .source_spans
        .iter()
        .map(|span| span.end_utf8.saturating_sub(span.start_utf8).max(1) as u64)
        .sum::<u64>()
        .max(1);
    let page_output_start_utf8 = page.text_start_utf8;
    let page_output_end_utf8 = page
        .text_end_utf8
        .max(page.text_start_utf8.saturating_add(1));
    let total_output_weight = u64::from(
        page_output_end_utf8
            .saturating_sub(page_output_start_utf8)
            .max(1),
    );
    let mut consumed = 0u64;
    let mut items = Vec::with_capacity(page.source_spans.len());
    for (index, span) in page.source_spans.iter().enumerate() {
        let consumed_before = consumed;
        let top_px = ((consumed * page.height_pt as u64) / total_weight) as u32;
        consumed += span.end_utf8.saturating_sub(span.start_utf8).max(1) as u64;
        let bottom_px = if index + 1 == page.source_spans.len() {
            page.height_pt
        } else {
            ((consumed * page.height_pt as u64) / total_weight) as u32
        };
        let output_start_utf8 = page_output_start_utf8
            .saturating_add(((consumed_before * total_output_weight) / total_weight) as u32);
        let output_end_utf8 = if index + 1 == page.source_spans.len() {
            page_output_end_utf8
        } else {
            page_output_start_utf8
                .saturating_add(((consumed * total_output_weight) / total_weight) as u32)
        }
        .max(output_start_utf8.saturating_add(1));
        let source_text = source_texts.get(&span.file);
        let start_line = source_offset_line(source_text, span.start_utf8);
        let end_line = if span.end_utf8 <= span.start_utf8 {
            start_line
        } else {
            source_offset_line(source_text, span.end_utf8.saturating_sub(1))
        };
        let item_id = format!(
            "{}:{}:{}:{}:{}:{}",
            page.page_id, span.file, span.start_utf8, span.end_utf8, start_line, end_line
        );
        let (left_px, right_px, top_px, bottom_px) = normalize_sync_item_bounds(
            page.width_pt,
            page.height_pt,
            0,
            page.width_pt,
            top_px,
            bottom_px,
            top_px,
            bottom_px,
        );
        items.push(SyncMapItem {
            item_id,
            file: span.file.clone(),
            start_utf8: span.start_utf8,
            end_utf8: span.end_utf8,
            output_start_utf8,
            output_end_utf8,
            start_line,
            end_line,
            left_px,
            right_px,
            top_px,
            bottom_px,
        });
    }
    items
}

fn resolve_source_jump_from_syncmap_pages(
    rev: u64,
    file: &Utf8PathBuf,
    requested_offset: u32,
    source_texts: &BTreeMap<Utf8PathBuf, String>,
    syncmap_pages: &[PageSyncMapArtifact],
) -> Option<SourceJumpResponse> {
    let mut best_match: Option<(u32, usize, usize, &PageSyncMapArtifact, SyncMapItem)> = None;
    for page in syncmap_pages {
        for (item_index, item) in build_sync_items_from_artifacts(page, source_texts)
            .into_iter()
            .enumerate()
        {
            if &item.file != file {
                continue;
            }
            let distance = if requested_offset < item.start_utf8 {
                item.start_utf8 - requested_offset
            } else if requested_offset > item.end_utf8 {
                requested_offset - item.end_utf8
            } else {
                0
            };
            let replace = match &best_match {
                None => true,
                Some((best_distance, best_page_index, best_item_index, _, _)) => {
                    distance < *best_distance
                        || (distance == *best_distance
                            && (page.index < *best_page_index
                                || (page.index == *best_page_index
                                    && item_index < *best_item_index)))
                }
            };
            if replace {
                best_match = Some((distance, page.index, item_index, page, item));
            }
        }
    }
    let (_, page_index, _, page, item) = best_match?;
    let line = source_offset_line(source_texts.get(file), requested_offset);
    let column = source_offset_column(source_texts.get(file), requested_offset);
    Some(SourceJumpResponse {
        rev,
        file: file.clone(),
        absolute_file: Utf8PathBuf::new(),
        file_uri: String::new(),
        editor_uri: String::new(),
        editor_preview_kind: EditorPreviewKind::None,
        offset_utf8: requested_offset,
        line,
        line0: line.saturating_sub(1),
        column,
        column0: column.saturating_sub(1),
        source_hash: source_selection_hash(file, line, column),
        editor_cwd: Utf8PathBuf::new(),
        editor_launch_supported: false,
        editor_program: String::new(),
        editor_args: Vec::new(),
        editor_command_line: String::new(),
        page_id: page.page_id.clone(),
        page_index,
        page_width_px: page.width_pt,
        page_height_px: page.height_pt,
        page_source_start_utf8: page
            .items
            .iter()
            .map(|item| item.start_utf8)
            .min()
            .unwrap_or(0),
        page_source_end_utf8: page
            .items
            .iter()
            .map(|item| item.end_utf8)
            .max()
            .unwrap_or(0),
        page_output_start_utf8: page
            .items
            .iter()
            .map(|item| item.output_start_utf8)
            .min()
            .unwrap_or(0),
        page_output_end_utf8: page
            .items
            .iter()
            .map(|item| item.output_end_utf8)
            .max()
            .unwrap_or(0),
        item,
    })
}

fn resolve_source_jump_from_page_metadata_set(
    rev: u64,
    file: &Utf8PathBuf,
    requested_offset: u32,
    source_texts: &BTreeMap<Utf8PathBuf, String>,
    pages: &[PageArtifactMeta],
) -> Option<SourceJumpResponse> {
    let mut best_match: Option<(u32, usize, usize, &PageArtifactMeta, SyncMapItem)> = None;
    for page in pages {
        for (item_index, item) in build_page_sync_items(page, source_texts)
            .into_iter()
            .enumerate()
        {
            if &item.file != file {
                continue;
            }
            let distance = if requested_offset < item.start_utf8 {
                item.start_utf8 - requested_offset
            } else if requested_offset > item.end_utf8 {
                requested_offset - item.end_utf8
            } else {
                0
            };
            let replace = match &best_match {
                None => true,
                Some((best_distance, best_page_index, best_item_index, _, _)) => {
                    distance < *best_distance
                        || (distance == *best_distance
                            && (page.index < *best_page_index
                                || (page.index == *best_page_index
                                    && item_index < *best_item_index)))
                }
            };
            if replace {
                best_match = Some((distance, page.index, item_index, page, item));
            }
        }
    }
    let (_, page_index, _, page, item) = best_match?;
    let line = source_offset_line(source_texts.get(file), requested_offset);
    let column = source_offset_column(source_texts.get(file), requested_offset);
    Some(SourceJumpResponse {
        rev,
        file: file.clone(),
        absolute_file: Utf8PathBuf::new(),
        file_uri: String::new(),
        editor_uri: String::new(),
        editor_preview_kind: EditorPreviewKind::None,
        offset_utf8: requested_offset,
        line,
        line0: line.saturating_sub(1),
        column,
        column0: column.saturating_sub(1),
        source_hash: source_selection_hash(file, line, column),
        editor_cwd: Utf8PathBuf::new(),
        editor_launch_supported: false,
        editor_program: String::new(),
        editor_args: Vec::new(),
        editor_command_line: String::new(),
        page_id: page.page_id.clone(),
        page_index,
        page_width_px: page.width_pt,
        page_height_px: page.height_pt,
        page_source_start_utf8: page
            .source_spans
            .iter()
            .map(|span| span.start_utf8)
            .min()
            .unwrap_or(0),
        page_source_end_utf8: page
            .source_spans
            .iter()
            .map(|span| span.end_utf8)
            .max()
            .unwrap_or(0),
        page_output_start_utf8: page.text_start_utf8,
        page_output_end_utf8: page.text_end_utf8,
        item,
    })
}

async fn resolve_source_jump_response(
    state: &Arc<AppState>,
    rev: u64,
    file: &Utf8PathBuf,
    requested_offset: u32,
    source_texts: &BTreeMap<Utf8PathBuf, String>,
) -> std::result::Result<Option<SourceJumpResponse>, Response> {
    let jump = if let Some(syncmap_pages) =
        load_revision_syncmap_artifacts(&state.build_root, rev).await?
    {
        resolve_source_jump_from_syncmap_pages(
            rev,
            file,
            requested_offset,
            source_texts,
            &syncmap_pages,
        )
    } else {
        let pages = load_revision_page_metadata_set(state, rev).await?;
        resolve_source_jump_from_page_metadata_set(
            rev,
            file,
            requested_offset,
            source_texts,
            &pages,
        )
    };
    Ok(jump.map(|jump| source_jump_with_editor_preview(state, jump)))
}

fn source_jump_with_editor_preview(
    state: &AppState,
    mut jump: SourceJumpResponse,
) -> SourceJumpResponse {
    let absolute_file = state.root.join(&jump.file);
    let editor_preview = build_editor_bridge_preview(
        state,
        &jump.file,
        &absolute_file,
        jump.rev,
        jump.offset_utf8,
        jump.line,
        jump.line0,
        jump.column,
        jump.column0,
        &jump.source_hash,
        Some(&jump),
    );
    jump.absolute_file = absolute_file;
    jump.file_uri = editor_preview.file_uri;
    jump.editor_uri = editor_preview.editor_uri;
    jump.editor_preview_kind = editor_preview.editor_preview_kind;
    jump.editor_cwd = editor_preview.editor_cwd;
    jump.editor_launch_supported = editor_preview.editor_launch_supported;
    jump.editor_program = editor_preview.editor_program;
    jump.editor_args = editor_preview.editor_args;
    jump.editor_command_line = editor_preview.editor_command_line;
    jump
}

fn page_input_from_metadata(
    build_root: &Utf8Path,
    rev: u64,
    page: PageArtifactMeta,
) -> PageRenderInput {
    let page_id = page.page_id;
    PageRenderInput {
        page_id: page_id.clone(),
        revision: rev,
        content_hash: page.content_hash,
        width_px: page.width_pt,
        height_px: page.height_pt,
        pdf_path: build_root
            .join(if page.pdf_artifact_path.as_str().is_empty() {
                Utf8PathBuf::from(format!("rev-{rev}/pages/{page_id}.pdf"))
            } else {
                page.pdf_artifact_path
            })
            .to_string(),
    }
}

fn render_sessions() -> &'static StdMutex<BTreeMap<RenderSessionKey, RenderSessionHandle>> {
    RENDER_SESSIONS.get_or_init(|| StdMutex::new(BTreeMap::new()))
}

fn render_session_metrics_map()
-> &'static StdMutex<BTreeMap<RenderSessionKey, Arc<RenderSessionMetrics>>> {
    RENDER_SESSION_METRICS.get_or_init(|| StdMutex::new(BTreeMap::new()))
}

fn render_session_key(state: &AppState) -> RenderSessionKey {
    RenderSessionKey {
        root: state.root.clone(),
        renderer_identity: state.tile_renderer.session_identity(),
    }
}

fn render_session_metrics_handle(state: &AppState) -> Arc<RenderSessionMetrics> {
    let key = render_session_key(state);
    let mut metrics = render_session_metrics_map()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    metrics
        .entry(key)
        .or_insert_with(|| Arc::new(RenderSessionMetrics::default()))
        .clone()
}

async fn render_session_metrics_snapshot(state: &Arc<AppState>) -> RenderSessionMetricsSnapshot {
    let key = render_session_key(state);
    let metrics = render_session_metrics_handle(state);
    let handle = render_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&key)
        .cloned();
    let active_session_count = handle.is_some() as usize;
    let mut snapshot = metrics.snapshot(active_session_count);
    let Some(handle) = handle else {
        return snapshot;
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    if handle
        .tx
        .send(RenderSessionRequest::DebugState { reply: reply_tx })
        .await
        .is_err()
    {
        note_render_session_failure(state, None, None, None);
        drop_render_session_handle(state);
        return snapshot;
    }
    match reply_rx.await {
        Ok((attached_revisions, warm_buckets, tile_cache_entries)) => {
            snapshot.attached_live_page_count = attached_revisions
                .iter()
                .map(|revision| revision.page_count)
                .sum();
            snapshot.attached_revisions = attached_revisions;
            snapshot.warm_bucket_count = warm_buckets.len();
            snapshot.warm_buckets = warm_buckets;
            snapshot.tile_cache_count = tile_cache_entries.len();
            snapshot.tile_cache_entries = tile_cache_entries;
            snapshot
        }
        Err(_) => {
            note_render_session_failure(state, None, None, None);
            drop_render_session_handle(state);
            snapshot
        }
    }
}

fn render_session_handle(state: &Arc<AppState>) -> RenderSessionHandle {
    let key = render_session_key(state);
    let mut sessions = render_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(handle) = sessions.get(&key) {
        return handle.clone();
    }
    let (tx, mut rx) = mpsc::channel::<RenderSessionRequest>(16);
    let config = state.tile_renderer.clone();
    let metrics = render_session_metrics_handle(state);
    metrics
        .actor_spawn_count
        .fetch_add(1, AtomicOrdering::SeqCst);
    metrics.record_event(
        RenderSessionEventKind::ActorSpawn,
        None,
        None,
        None,
        None,
        None,
    );
    tokio::spawn(async move {
        let mut renderer = config.build_session_renderer();
        let mut attached_revisions = BTreeMap::<u64, AttachedRenderRevision>::new();
        let mut attached_order = VecDeque::<u64>::new();
        let mut attached_page_total = 0usize;
        let mut warm_buckets = BTreeSet::<RasterCacheKey>::new();
        let mut warm_order = VecDeque::<RasterCacheKey>::new();
        let mut cached_tiles = BTreeMap::<RenderSessionTileCacheKey, TileImage>::new();
        let mut tile_order = VecDeque::<RenderSessionTileCacheKey>::new();
        let forget_revision_warm_buckets =
            |rev: u64,
             warm_buckets: &mut BTreeSet<RasterCacheKey>,
             warm_order: &mut VecDeque<RasterCacheKey>| {
                warm_buckets.retain(|candidate| candidate.rev != rev);
                warm_order.retain(|candidate| candidate.rev != rev);
            };
        let remember_warm_bucket =
            |bucket: RasterCacheKey,
             warm_buckets: &mut BTreeSet<RasterCacheKey>,
             warm_order: &mut VecDeque<RasterCacheKey>| {
                if warm_buckets.insert(bucket.clone()) {
                    warm_order.push_back(bucket.clone());
                } else if let Some(index) =
                    warm_order.iter().position(|candidate| *candidate == bucket)
                {
                    if let Some(existing) = warm_order.remove(index) {
                        warm_order.push_back(existing);
                    }
                }
                while warm_order
                    .iter()
                    .filter(|candidate| {
                        candidate.rev == bucket.rev && candidate.page_id == bucket.page_id
                    })
                    .count()
                    > RENDER_SESSION_WARM_BUCKET_PAGE_BUDGET
                {
                    let Some(index) = warm_order.iter().position(|candidate| {
                        candidate.rev == bucket.rev && candidate.page_id == bucket.page_id
                    }) else {
                        break;
                    };
                    if let Some(evicted) = warm_order.remove(index) {
                        warm_buckets.remove(&evicted);
                        metrics
                            .warm_bucket_evict_count
                            .fetch_add(1, AtomicOrdering::SeqCst);
                        metrics.record_event(
                            RenderSessionEventKind::EvictWarmBucket,
                            Some(evicted.rev),
                            Some(&evicted.page_id),
                            Some(evicted.zoom_bucket as f32 / 100.0),
                            None,
                            None,
                        );
                    }
                }
                while warm_order.len() > RENDER_SESSION_WARM_BUCKET_BUDGET {
                    if let Some(evicted) = warm_order.pop_front() {
                        warm_buckets.remove(&evicted);
                        metrics
                            .warm_bucket_evict_count
                            .fetch_add(1, AtomicOrdering::SeqCst);
                        metrics.record_event(
                            RenderSessionEventKind::EvictWarmBucket,
                            Some(evicted.rev),
                            Some(&evicted.page_id),
                            Some(evicted.zoom_bucket as f32 / 100.0),
                            None,
                            None,
                        );
                    }
                }
            };
        let forget_revision_cached_tiles =
            |rev: u64,
             cached_tiles: &mut BTreeMap<RenderSessionTileCacheKey, TileImage>,
             tile_order: &mut VecDeque<RenderSessionTileCacheKey>| {
                cached_tiles.retain(|candidate, _| candidate.bucket.rev != rev);
                tile_order.retain(|candidate| candidate.bucket.rev != rev);
            };
        let remember_cached_tile =
            |key: RenderSessionTileCacheKey,
             tile: TileImage,
             cached_tiles: &mut BTreeMap<RenderSessionTileCacheKey, TileImage>,
             tile_order: &mut VecDeque<RenderSessionTileCacheKey>| {
                let record_evicted_tile = |evicted: &RenderSessionTileCacheKey| {
                    metrics
                        .tile_cache_evict_count
                        .fetch_add(1, AtomicOrdering::SeqCst);
                    metrics.record_event(
                        RenderSessionEventKind::EvictTileCache,
                        Some(evicted.bucket.rev),
                        Some(&evicted.bucket.page_id),
                        Some(evicted.bucket.zoom_bucket as f32 / 100.0),
                        None,
                        None,
                    );
                };
                let key_rev = key.bucket.rev;
                let key_page_id = key.bucket.page_id.clone();
                cached_tiles.insert(key.clone(), tile);
                if let Some(index) = tile_order.iter().position(|candidate| *candidate == key) {
                    tile_order.remove(index);
                }
                tile_order.push_back(key);
                while tile_order
                    .iter()
                    .filter(|candidate| {
                        candidate.bucket.rev == key_rev && candidate.bucket.page_id == key_page_id
                    })
                    .count()
                    > RENDER_SESSION_TILE_CACHE_PAGE_BUDGET
                {
                    let Some(index) = tile_order.iter().position(|candidate| {
                        candidate.bucket.rev == key_rev && candidate.bucket.page_id == key_page_id
                    }) else {
                        break;
                    };
                    if let Some(evicted) = tile_order.remove(index) {
                        cached_tiles.remove(&evicted);
                        record_evicted_tile(&evicted);
                    }
                }
                while tile_order.len() > RENDER_SESSION_TILE_CACHE_BUDGET {
                    if let Some(evicted) = tile_order.pop_front() {
                        cached_tiles.remove(&evicted);
                        record_evicted_tile(&evicted);
                    }
                }
            };
        while let Some(request) = rx.recv().await {
            match request {
                RenderSessionRequest::AttachRevision {
                    rev,
                    page_metadata,
                    page_inputs,
                } => {
                    metrics.attach_count.fetch_add(1, AtomicOrdering::SeqCst);
                    metrics
                        .attached_page_count
                        .fetch_add(page_metadata.len() as u64, AtomicOrdering::SeqCst);
                    metrics.record_event(
                        RenderSessionEventKind::AttachRevision,
                        Some(rev),
                        None,
                        None,
                        Some(page_metadata.len()),
                        None,
                    );
                    if let Some(existing) = attached_revisions.get(&rev) {
                        attached_page_total =
                            attached_page_total.saturating_sub(existing.page_metadata.len());
                    }
                    forget_revision_warm_buckets(rev, &mut warm_buckets, &mut warm_order);
                    forget_revision_cached_tiles(rev, &mut cached_tiles, &mut tile_order);
                    attached_revisions.insert(
                        rev,
                        AttachedRenderRevision {
                            page_metadata,
                            page_inputs: page_inputs
                                .into_iter()
                                .map(|page| (page.page_id.clone(), page))
                                .collect(),
                        },
                    );
                    attached_page_total += attached_revisions
                        .get(&rev)
                        .map(|revision| revision.page_metadata.len())
                        .unwrap_or(0);
                    if let Some(index) = attached_order
                        .iter()
                        .position(|candidate| *candidate == rev)
                    {
                        attached_order.remove(index);
                    }
                    attached_order.push_back(rev);
                    while (attached_order.len() > RENDER_SESSION_ATTACHED_REVISION_WINDOW
                        || attached_page_total > RENDER_SESSION_ATTACHED_PAGE_BUDGET)
                        && attached_order.len() > 1
                    {
                        if let Some(evicted) = attached_order.pop_front() {
                            if let Some(revision) = attached_revisions.remove(&evicted) {
                                attached_page_total = attached_page_total
                                    .saturating_sub(revision.page_metadata.len());
                                forget_revision_warm_buckets(
                                    evicted,
                                    &mut warm_buckets,
                                    &mut warm_order,
                                );
                                forget_revision_cached_tiles(
                                    evicted,
                                    &mut cached_tiles,
                                    &mut tile_order,
                                );
                                metrics.evict_count.fetch_add(1, AtomicOrdering::SeqCst);
                                metrics.record_event(
                                    RenderSessionEventKind::EvictRevision,
                                    Some(evicted),
                                    None,
                                    None,
                                    Some(revision.page_metadata.len()),
                                    None,
                                );
                            }
                        }
                    }
                }
                #[cfg(test)]
                RenderSessionRequest::DetachRevision { rev } => {
                    metrics.detach_count.fetch_add(1, AtomicOrdering::SeqCst);
                    metrics.record_event(
                        RenderSessionEventKind::DetachRevision,
                        Some(rev),
                        None,
                        None,
                        None,
                        None,
                    );
                    if let Some(revision) = attached_revisions.remove(&rev) {
                        attached_page_total =
                            attached_page_total.saturating_sub(revision.page_metadata.len());
                    }
                    forget_revision_warm_buckets(rev, &mut warm_buckets, &mut warm_order);
                    forget_revision_cached_tiles(rev, &mut cached_tiles, &mut tile_order);
                    if let Some(index) = attached_order
                        .iter()
                        .position(|candidate| *candidate == rev)
                    {
                        attached_order.remove(index);
                    }
                }
                RenderSessionRequest::LookupRevisionPages { rev, reply } => {
                    metrics
                        .revision_lookup_count
                        .fetch_add(1, AtomicOrdering::SeqCst);
                    metrics.record_event(
                        RenderSessionEventKind::LookupRevisionPages,
                        Some(rev),
                        None,
                        None,
                        None,
                        None,
                    );
                    if let Some(index) = attached_order
                        .iter()
                        .position(|candidate| *candidate == rev)
                    {
                        if let Some(touched) = attached_order.remove(index) {
                            attached_order.push_back(touched);
                        }
                    }
                    let pages = attached_revisions
                        .get(&rev)
                        .map(|revision| revision.page_metadata.clone());
                    let _ = reply.send(pages);
                }
                RenderSessionRequest::DebugState { reply } => {
                    let attached_revisions = attached_order
                        .iter()
                        .filter_map(|rev| {
                            attached_revisions.get(rev).map(|revision| {
                                RenderSessionAttachedRevisionSnapshot {
                                    rev: *rev,
                                    page_count: revision.page_metadata.len(),
                                    page_ids: revision
                                        .page_metadata
                                        .iter()
                                        .map(|page| page.page_id.clone())
                                        .collect(),
                                }
                            })
                        })
                        .collect();
                    let warm_buckets = warm_order
                        .iter()
                        .map(|bucket| RenderSessionWarmBucketSnapshot {
                            rev: bucket.rev,
                            page_id: bucket.page_id.clone(),
                            content_hash: bucket.content_hash.clone(),
                            zoom_bucket: bucket.zoom_bucket,
                        })
                        .collect();
                    let tile_cache_entries = tile_order
                        .iter()
                        .map(|tile| RenderSessionTileCacheSnapshot {
                            rev: tile.bucket.rev,
                            page_id: tile.bucket.page_id.clone(),
                            content_hash: tile.bucket.content_hash.clone(),
                            zoom_bucket: tile.bucket.zoom_bucket,
                            rect_x: tile.rect_x,
                            rect_y: tile.rect_y,
                            rect_width: tile.rect_width,
                            rect_height: tile.rect_height,
                        })
                        .collect();
                    let _ = reply.send((attached_revisions, warm_buckets, tile_cache_entries));
                }
                RenderSessionRequest::LookupPage {
                    rev,
                    page_id,
                    reply,
                } => {
                    metrics
                        .page_lookup_count
                        .fetch_add(1, AtomicOrdering::SeqCst);
                    metrics.record_event(
                        RenderSessionEventKind::LookupPage,
                        Some(rev),
                        Some(&page_id),
                        None,
                        None,
                        None,
                    );
                    if let Some(index) = attached_order
                        .iter()
                        .position(|candidate| *candidate == rev)
                    {
                        if let Some(touched) = attached_order.remove(index) {
                            attached_order.push_back(touched);
                        }
                    }
                    let page = attached_revisions
                        .get(&rev)
                        .and_then(|revision| revision.page_inputs.get(&page_id).cloned());
                    let _ = reply.send(page);
                }
                RenderSessionRequest::PrewarmPage { page, scale, reply } => {
                    if let Some(index) = attached_order
                        .iter()
                        .position(|candidate| *candidate == page.revision)
                    {
                        if let Some(touched) = attached_order.remove(index) {
                            attached_order.push_back(touched);
                        }
                    }
                    let warm_bucket = render_session_bucket_key(&page, scale);
                    if warm_buckets.contains(&warm_bucket) {
                        remember_warm_bucket(warm_bucket, &mut warm_buckets, &mut warm_order);
                        metrics
                            .skipped_prewarm_count
                            .fetch_add(1, AtomicOrdering::SeqCst);
                        metrics.record_event(
                            RenderSessionEventKind::SkipPrewarmPage,
                            Some(page.revision),
                            Some(&page.page_id),
                            Some(scale),
                            None,
                            None,
                        );
                        let _ = reply.send(Ok(()));
                        continue;
                    }
                    let rect = render_session_prewarm_rect(&page, scale);
                    let cache_key = render_session_tile_cache_key(&page, scale, &rect);
                    if cached_tiles.contains_key(&cache_key) {
                        if let Some(position) = tile_order
                            .iter()
                            .position(|candidate| *candidate == cache_key)
                        {
                            if let Some(existing) = tile_order.remove(position) {
                                tile_order.push_back(existing);
                            }
                        }
                        remember_warm_bucket(warm_bucket, &mut warm_buckets, &mut warm_order);
                        metrics
                            .skipped_prewarm_count
                            .fetch_add(1, AtomicOrdering::SeqCst);
                        metrics.record_event(
                            RenderSessionEventKind::SkipPrewarmPage,
                            Some(page.revision),
                            Some(&page.page_id),
                            Some(scale),
                            None,
                            None,
                        );
                        let _ = reply.send(Ok(()));
                        continue;
                    }
                    metrics
                        .prewarm_request_count
                        .fetch_add(1, AtomicOrdering::SeqCst);
                    let started = std::time::Instant::now();
                    let result = renderer.render_tiles(&page, scale, &[rect]);
                    if let Ok(tiles) = &result {
                        remember_warm_bucket(warm_bucket, &mut warm_buckets, &mut warm_order);
                        for tile in tiles.iter().cloned() {
                            remember_cached_tile(
                                render_session_tile_cache_key(&page, scale, &tile.rect),
                                tile,
                                &mut cached_tiles,
                                &mut tile_order,
                            );
                        }
                    }
                    metrics.record_event(
                        RenderSessionEventKind::PrewarmPage,
                        Some(page.revision),
                        Some(&page.page_id),
                        Some(scale),
                        Some(1),
                        Some(started.elapsed()),
                    );
                    let _ = reply.send(result.map(|_| ()));
                }
                RenderSessionRequest::RenderPage { page, scale, reply } => {
                    metrics
                        .render_request_count
                        .fetch_add(1, AtomicOrdering::SeqCst);
                    let started = std::time::Instant::now();
                    if let Some(index) = attached_order
                        .iter()
                        .position(|candidate| *candidate == page.revision)
                    {
                        if let Some(touched) = attached_order.remove(index) {
                            attached_order.push_back(touched);
                        }
                    }
                    let result = renderer.render_full_page(&page, scale);
                    if result.is_ok() {
                        remember_warm_bucket(
                            render_session_bucket_key(&page, scale),
                            &mut warm_buckets,
                            &mut warm_order,
                        );
                    }
                    metrics.record_event(
                        RenderSessionEventKind::RenderPage,
                        Some(page.revision),
                        Some(&page.page_id),
                        Some(scale),
                        None,
                        Some(started.elapsed()),
                    );
                    let _ = reply.send(result);
                }
                RenderSessionRequest::RenderTiles {
                    page,
                    scale,
                    rects,
                    reply,
                } => {
                    metrics
                        .tile_render_request_count
                        .fetch_add(1, AtomicOrdering::SeqCst);
                    let started = std::time::Instant::now();
                    if let Some(index) = attached_order
                        .iter()
                        .position(|candidate| *candidate == page.revision)
                    {
                        if let Some(touched) = attached_order.remove(index) {
                            attached_order.push_back(touched);
                        }
                    }
                    let warm_bucket = render_session_bucket_key(&page, scale);
                    let requested_rect_count = rects.len();
                    let mut ordered_tiles = vec![None; requested_rect_count];
                    let mut missing_indexes = Vec::new();
                    let mut missing_rects = Vec::new();
                    for (index, rect) in rects.iter().enumerate() {
                        let cache_key = render_session_tile_cache_key(&page, scale, rect);
                        if let Some(tile) = cached_tiles.get(&cache_key).cloned() {
                            if let Some(position) = tile_order
                                .iter()
                                .position(|candidate| *candidate == cache_key)
                            {
                                if let Some(existing) = tile_order.remove(position) {
                                    tile_order.push_back(existing);
                                }
                            }
                            ordered_tiles[index] = Some(tile);
                        } else {
                            missing_indexes.push(index);
                            missing_rects.push(rect.clone());
                        }
                    }
                    let had_missing_rects = !missing_rects.is_empty();
                    let result = if missing_rects.is_empty() {
                        Ok(ordered_tiles
                            .into_iter()
                            .map(|tile| tile.expect("cached tile present"))
                            .collect())
                    } else {
                        match renderer.render_tiles(&page, scale, &missing_rects) {
                            Ok(rendered_tiles) => {
                                if rendered_tiles.len() != missing_indexes.len() {
                                    Err(anyhow!(
                                        "renderer session returned {} tiles for {} requested rectangles",
                                        rendered_tiles.len(),
                                        missing_indexes.len(),
                                    ))
                                } else {
                                    for (missing_index, tile) in
                                        missing_indexes.into_iter().zip(rendered_tiles.into_iter())
                                    {
                                        remember_cached_tile(
                                            render_session_tile_cache_key(&page, scale, &tile.rect),
                                            tile.clone(),
                                            &mut cached_tiles,
                                            &mut tile_order,
                                        );
                                        ordered_tiles[missing_index] = Some(tile);
                                    }
                                    Ok(ordered_tiles
                                        .into_iter()
                                        .map(|tile| tile.expect("tile present after render"))
                                        .collect())
                                }
                            }
                            Err(error) => Err(error),
                        }
                    };
                    if result.is_ok() {
                        remember_warm_bucket(warm_bucket, &mut warm_buckets, &mut warm_order);
                    }
                    metrics.record_tile_event(
                        if had_missing_rects {
                            RenderSessionEventKind::RenderTiles
                        } else {
                            RenderSessionEventKind::ReuseTiles
                        },
                        Some(page.revision),
                        Some(&page.page_id),
                        Some(scale),
                        Some(requested_rect_count),
                        had_missing_rects.then(|| started.elapsed()),
                        missing_rects.len() as u32,
                        requested_rect_count.saturating_sub(missing_rects.len()) as u32,
                    );
                    let _ = reply.send(result);
                }
            }
        }
    });
    let handle = RenderSessionHandle { tx };
    sessions.insert(key, handle.clone());
    handle
}

fn drop_render_session_handle(state: &Arc<AppState>) {
    let key = render_session_key(state);
    let mut sessions = render_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    sessions.remove(&key);
}

fn note_render_session_failure(
    state: &Arc<AppState>,
    rev: Option<u64>,
    page_id: Option<&str>,
    scale: Option<f32>,
) {
    let metrics = render_session_metrics_handle(state);
    metrics
        .actor_failure_count
        .fetch_add(1, AtomicOrdering::SeqCst);
    metrics.record_event(
        RenderSessionEventKind::ActorFailure,
        rev,
        page_id,
        scale,
        None,
        None,
    );
    metrics
        .actor_restart_count
        .fetch_add(1, AtomicOrdering::SeqCst);
    metrics.record_event(
        RenderSessionEventKind::ActorRestart,
        rev,
        page_id,
        scale,
        None,
        None,
    );
}

async fn render_page_with_session(
    state: &Arc<AppState>,
    page: &PageRenderInput,
    scale: f32,
) -> anyhow::Result<tex_render_gs::RasterImage> {
    for _ in 0..2 {
        let handle = render_session_handle(state);
        let (reply_tx, reply_rx) = oneshot::channel();
        if handle
            .tx
            .send(RenderSessionRequest::RenderPage {
                page: page.clone(),
                scale,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            note_render_session_failure(
                state,
                Some(page.revision),
                Some(&page.page_id),
                Some(scale),
            );
            drop_render_session_handle(state);
            continue;
        }
        match reply_rx.await {
            Ok(result) => return result,
            Err(_) => {
                note_render_session_failure(
                    state,
                    Some(page.revision),
                    Some(&page.page_id),
                    Some(scale),
                );
                drop_render_session_handle(state);
            }
        }
    }
    let metrics = render_session_metrics_handle(state);
    metrics
        .fallback_render_count
        .fetch_add(1, AtomicOrdering::SeqCst);
    warn!(
        rev = page.revision,
        page_id = %page.page_id,
        scale,
        "renderer session unavailable, falling back to direct render path"
    );
    let started = std::time::Instant::now();
    let result = state.tile_renderer.render_full_page(page, scale);
    metrics.record_event(
        RenderSessionEventKind::FallbackRender,
        Some(page.revision),
        Some(&page.page_id),
        Some(scale),
        None,
        Some(started.elapsed()),
    );
    result
}

async fn attach_render_revision(state: &Arc<AppState>, rev: u64, pages: &[PageArtifactMeta]) {
    let page_metadata = pages.to_vec();
    let page_inputs = page_metadata
        .iter()
        .cloned()
        .map(|page| page_input_from_metadata(&state.build_root, rev, page))
        .collect::<Vec<_>>();
    attach_render_revision_owned(state, rev, page_metadata, page_inputs).await;
}

async fn attach_render_revision_owned(
    state: &Arc<AppState>,
    rev: u64,
    page_metadata: Vec<PageArtifactMeta>,
    page_inputs: Vec<PageRenderInput>,
) {
    let handle = render_session_handle(state);
    if handle
        .tx
        .send(RenderSessionRequest::AttachRevision {
            rev,
            page_metadata,
            page_inputs,
        })
        .await
        .is_err()
    {
        note_render_session_failure(state, Some(rev), None, None);
        drop_render_session_handle(state);
    }
}

#[cfg(test)]
async fn detach_render_revision(state: &Arc<AppState>, rev: u64) {
    let handle = render_session_handle(state);
    if handle
        .tx
        .send(RenderSessionRequest::DetachRevision { rev })
        .await
        .is_err()
    {
        note_render_session_failure(state, Some(rev), None, None);
        drop_render_session_handle(state);
    }
}

async fn lookup_attached_page_input(
    state: &Arc<AppState>,
    rev: u64,
    page_id: &str,
) -> Option<PageRenderInput> {
    let handle = render_session_handle(state);
    let (reply_tx, reply_rx) = oneshot::channel();
    if handle
        .tx
        .send(RenderSessionRequest::LookupPage {
            rev,
            page_id: page_id.to_string(),
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        note_render_session_failure(state, Some(rev), Some(page_id), None);
        drop_render_session_handle(state);
        return None;
    }
    match reply_rx.await {
        Ok(page) => page,
        Err(_) => {
            note_render_session_failure(state, Some(rev), Some(page_id), None);
            drop_render_session_handle(state);
            None
        }
    }
}

async fn lookup_attached_revision_pages(
    state: &Arc<AppState>,
    rev: u64,
) -> Option<Vec<PageArtifactMeta>> {
    let handle = render_session_handle(state);
    let (reply_tx, reply_rx) = oneshot::channel();
    if handle
        .tx
        .send(RenderSessionRequest::LookupRevisionPages {
            rev,
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        note_render_session_failure(state, Some(rev), None, None);
        drop_render_session_handle(state);
        return None;
    }
    match reply_rx.await {
        Ok(pages) => pages,
        Err(_) => {
            note_render_session_failure(state, Some(rev), None, None);
            drop_render_session_handle(state);
            None
        }
    }
}

async fn cache_raster_image(
    state: &Arc<AppState>,
    page: &PageRenderInput,
    scale: f32,
) -> anyhow::Result<tex_render_gs::RasterImage> {
    let zoom_bucket = (scale * 100.0).round().clamp(1.0, u16::MAX as f32) as u16;
    let key = RasterCacheKey {
        rev: page.revision,
        page_id: page.page_id.clone(),
        content_hash: page.content_hash.clone(),
        zoom_bucket,
    };
    loop {
        if let Some(image) = state.raster_cache.read().await.get(&key).cloned() {
            return Ok(image);
        }
        let cache_path = raster_cache_path(&state.build_root, &key);
        match tokio::fs::read(cache_path.as_std_path()).await {
            Ok(bytes) => match image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            {
                Ok(image) => {
                    let image = image.into_rgba8();
                    let (width, height) = image.dimensions();
                    let restored = tex_render_gs::RasterImage {
                        width,
                        height,
                        rgba: image.into_raw(),
                    };
                    let mut cache = state.raster_cache.write().await;
                    let cached = cache.entry(key.clone()).or_insert_with(|| restored.clone());
                    return Ok(cached.clone());
                }
                Err(error) => {
                    warn!("failed to decode cached raster {}: {error}", cache_path);
                    let _ = tokio::fs::remove_file(cache_path.as_std_path()).await;
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                warn!("failed to read cached raster {}: {error}", cache_path);
            }
        }

        let (notify, is_leader) = {
            let mut inflight = state.inflight_rasters.write().await;
            if let Some(notify) = inflight.get(&key) {
                (notify.clone(), false)
            } else {
                let notify = Arc::new(Notify::new());
                inflight.insert(key.clone(), notify.clone());
                (notify, true)
            }
        };

        if !is_leader {
            notify.notified().await;
            continue;
        }

        let result = async {
            let rendered = render_page_with_session(state, page, scale).await?;
            let png_bytes = encode_png_bytes(&rendered)?;
            if let Some(parent) = cache_path.parent() {
                if let Err(error) = tokio::fs::create_dir_all(parent.as_std_path()).await {
                    warn!(
                        "failed to create raster cache directory {}: {error}",
                        parent
                    );
                } else if let Err(error) =
                    tokio::fs::write(cache_path.as_std_path(), png_bytes).await
                {
                    warn!("failed to write raster cache {}: {error}", cache_path);
                }
            }
            let mut cache = state.raster_cache.write().await;
            let cached = cache.entry(key.clone()).or_insert_with(|| rendered.clone());
            Ok(cached.clone())
        }
        .await;

        let mut inflight = state.inflight_rasters.write().await;
        let notify = inflight.remove(&key);
        drop(inflight);
        if let Some(notify) = notify {
            notify.notify_waiters();
        }
        return result;
    }
}

fn raster_cache_path(build_root: &Utf8Path, key: &RasterCacheKey) -> Utf8PathBuf {
    build_root
        .join(format!("rev-{}/raster-cache/{}", key.rev, key.zoom_bucket))
        .join(format!("{}-{}.png", key.page_id, key.content_hash))
}

fn encode_png_bytes(image: &tex_render_gs::RasterImage) -> anyhow::Result<Vec<u8>> {
    let buffer = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())
        .ok_or_else(|| anyhow!("failed to construct raster buffer"))?;
    let mut bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    image::ImageEncoder::write_image(
        encoder,
        buffer.as_raw(),
        buffer.width(),
        buffer.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|error| anyhow!("failed to encode png: {error}"))?;
    Ok(bytes)
}

fn png_response(image: tex_render_gs::RasterImage) -> Response {
    let bytes = match encode_png_bytes(&image) {
        Ok(bytes) => bytes,
        Err(error) => {
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to encode png: {error}"),
            );
        }
    };

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))],
        bytes,
    )
        .into_response()
}

fn text_response(status: StatusCode, message: &str) -> Response {
    (
        status,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        )],
        message.to_string(),
    )
        .into_response()
}

async fn ws(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(error) = handle_socket(socket, state).await {
            error!("websocket session ended with error: {error}");
        }
    })
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) -> Result<()> {
    let mut receiver = state.events.subscribe();
    loop {
        tokio::select! {
            result = receiver.recv() => {
                match result {
                    Ok(message) => {
                        let payload = serde_json::to_string(&message)?;
                        socket.send(Message::Text(payload.into())).await?;
                    }
                    Err(error) => {
                        return Err(anyhow!("websocket broadcast channel closed: {error}"));
                    }
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => return Ok(()),
                    Some(Ok(Message::Text(payload))) => {
                        match serde_json::from_str::<ClientMsg>(&payload) {
                            Ok(message) => {
                                let state = state.clone();
                                tokio::spawn(async move {
                                    prewarm_viewport_rasters(&state, message).await;
                                });
                            }
                            Err(error) => {
                                warn!("failed to decode client websocket message: {error}");
                            }
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => return Err(anyhow!("websocket receive failure: {error}")),
                }
            }
        }
    }
}

async fn prewarm_viewport_rasters(state: &Arc<AppState>, message: ClientMsg) {
    let ClientMsg::ViewportChanged {
        zoom,
        current_page,
        visible_pages,
        ..
    } = message
    else {
        return;
    };
    let live = state.live.read().await;
    let rev = live.snapshot.last_applied_rev;
    if rev == 0 {
        return;
    }
    let mut targets = Vec::new();
    let mut queued_targets = BTreeSet::new();
    let mut push_target = |page: &PageArtifactMeta, scale: f32| {
        let zoom_bucket = (scale * 100.0).round().clamp(1.0, u16::MAX as f32) as u16;
        if queued_targets.insert((page.page_id.clone(), zoom_bucket)) {
            targets.push((page.clone(), scale));
        }
    };
    let current_page_meta = live
        .page_metadata
        .get(current_page.saturating_sub(1) as usize)
        .cloned();
    if let Some(page) = current_page_meta.as_ref() {
        push_target(page, zoom);
    }
    for page_id in &visible_pages {
        if let Some(page) = live
            .page_metadata
            .iter()
            .find(|page| page.page_id == *page_id)
        {
            push_target(page, zoom);
        }
    }
    for adjacent_index in [
        current_page.saturating_sub(2) as usize,
        current_page as usize,
    ] {
        if let Some(page) = live.page_metadata.get(adjacent_index) {
            push_target(page, zoom);
        }
    }
    if let Some(page) = current_page_meta.as_ref() {
        for scheduled_zoom in [(zoom - 0.1).clamp(0.5, 3.0), (zoom + 0.1).clamp(0.5, 3.0)] {
            push_target(page, scheduled_zoom);
        }
    }
    let mut visible_neighbor_pages = 0usize;
    for page_id in &visible_pages {
        if visible_neighbor_pages >= RENDER_SESSION_VISIBLE_ZOOM_NEIGHBOR_PAGE_LIMIT {
            break;
        }
        let Some(page) = live
            .page_metadata
            .iter()
            .find(|page| page.page_id == *page_id)
        else {
            continue;
        };
        if current_page_meta
            .as_ref()
            .is_some_and(|current| current.page_id == page.page_id)
        {
            continue;
        }
        for scheduled_zoom in [(zoom - 0.1).clamp(0.5, 3.0), (zoom + 0.1).clamp(0.5, 3.0)] {
            push_target(page, scheduled_zoom);
        }
        visible_neighbor_pages += 1;
    }
    drop(live);

    for (page, scheduled_zoom) in targets {
        let input = page_input_from_metadata(&state.build_root, rev, page);
        let prewarm_key = RasterCacheKey {
            rev: input.revision,
            page_id: input.page_id.clone(),
            content_hash: input.content_hash.clone(),
            zoom_bucket: (scheduled_zoom * 100.0).round().clamp(1.0, u16::MAX as f32) as u16,
        };
        if state.raster_cache.read().await.contains_key(&prewarm_key) {
            continue;
        }
        let session_key = render_session_key(state);
        let claimed = {
            let mut inflight = render_session_inflight_prewarms()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            inflight.insert((session_key.clone(), prewarm_key.clone()))
        };
        if !claimed {
            let metrics = render_session_metrics_handle(state);
            metrics
                .skipped_prewarm_count
                .fetch_add(1, AtomicOrdering::SeqCst);
            metrics.record_event(
                RenderSessionEventKind::SkipPrewarmPage,
                Some(input.revision),
                Some(&input.page_id),
                Some(scheduled_zoom),
                None,
                None,
            );
            continue;
        }
        let mut warmed = false;
        for _ in 0..2 {
            let handle = render_session_handle(state);
            let (reply_tx, reply_rx) = oneshot::channel();
            if handle
                .tx
                .send(RenderSessionRequest::PrewarmPage {
                    page: input.clone(),
                    scale: scheduled_zoom,
                    reply: reply_tx,
                })
                .await
                .is_err()
            {
                note_render_session_failure(
                    state,
                    Some(input.revision),
                    Some(&input.page_id),
                    Some(scheduled_zoom),
                );
                drop_render_session_handle(state);
                continue;
            }
            match reply_rx.await {
                Ok(Ok(())) => {
                    warmed = true;
                    break;
                }
                Ok(Err(error)) => {
                    warn!(
                        page_id = %input.page_id,
                        zoom = scheduled_zoom,
                        "failed to prewarm renderer session page: {error}"
                    );
                    break;
                }
                Err(_) => {
                    note_render_session_failure(
                        state,
                        Some(input.revision),
                        Some(&input.page_id),
                        Some(scheduled_zoom),
                    );
                    drop_render_session_handle(state);
                }
            }
        }
        let mut inflight = render_session_inflight_prewarms()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inflight.remove(&(session_key, prewarm_key));
        drop(inflight);
        if warmed {
            continue;
        }
        let metrics = render_session_metrics_handle(state);
        metrics
            .fallback_prewarm_count
            .fetch_add(1, AtomicOrdering::SeqCst);
        let started = std::time::Instant::now();
        let rect = render_session_prewarm_rect(&input, scheduled_zoom);
        match state
            .tile_renderer
            .render_tiles(&input, scheduled_zoom, &[rect])
        {
            Ok(_) => {
                metrics.record_event(
                    RenderSessionEventKind::FallbackPrewarmPage,
                    Some(input.revision),
                    Some(&input.page_id),
                    Some(scheduled_zoom),
                    Some(1),
                    Some(started.elapsed()),
                );
            }
            Err(error) => {
                metrics.record_event(
                    RenderSessionEventKind::FallbackPrewarmPage,
                    Some(input.revision),
                    Some(&input.page_id),
                    Some(scheduled_zoom),
                    Some(1),
                    Some(started.elapsed()),
                );
                warn!(
                    page_id = %input.page_id,
                    zoom = scheduled_zoom,
                    "failed to prewarm renderer fallback path: {error}"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        sync::Arc,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use axum::{
        body::{Body, to_bytes},
        extract::{Json, Path, Query, State},
        http::{Request, StatusCode, header},
    };
    use camino::Utf8PathBuf;
    use tempfile::tempdir;
    use tex_render_gs::{GsApiRuntime, PageRenderInput, Rect, TileImage, probe_gsapi_library};
    use tokio::{
        sync::{RwLock, broadcast, mpsc, oneshot},
        time::{Duration, sleep, timeout},
    };
    use tower::util::ServiceExt;

    use hmr_protocol::{
        ClientMsg, Diagnostic, DiagnosticLevel, PagePreviewArtifact, SourceSnapshotFile,
    };
    use tex_world::ProjectWorld;

    use super::{
        AppState, BuildCache, EditorBridgeConfig, EditorPreviewKind, LivePreviewState,
        OpenSourceRequest, OpenSourceResponse, PageSyncMapResponse, PreviewSnapshot,
        RENDER_SESSION_ATTACHED_PAGE_BUDGET, RENDER_SESSION_ATTACHED_REVISION_WINDOW,
        RENDER_SESSION_RECENT_EVENT_WINDOW, RENDER_SESSION_TILE_CACHE_BUDGET,
        RENDER_SESSION_TILE_CACHE_PAGE_BUDGET, RENDER_SESSION_WARM_BUCKET_BUDGET,
        RENDER_SESSION_WARM_BUCKET_PAGE_BUDGET, RasterCacheKey, RasterQuery,
        RenderSessionAttachedRevisionSnapshot, RenderSessionEvent, RenderSessionEventKind,
        RenderSessionHandle, RenderSessionLatencySummary, RenderSessionMetrics,
        RenderSessionRequest, RenderSessionTileCacheSnapshot, RenderSessionWarmBucketSnapshot,
        RequiredTilesQuery, SourceFileQuery, SourceFileResponse, SourceFilesResponse,
        SourceJumpQuery, SourceJumpResponse, TileManifestResponse, TileRendererConfig,
        UpdateSourceFileRequest, UpdateSourceFileResponse, attach_render_revision, build_router,
        build_router_with_base, cache_raster_image, detach_render_revision, hash_input,
        load_revision_page_input, load_revision_page_metadata, load_revision_page_metadata_set,
        lookup_attached_page_input, normalize_viewer_base_path, open_source, page_syncmap,
        parse_png_path_suffix, parse_png_u32_path_suffix, prewarm_viewport_rasters,
        raster_cache_path, render_session_handle, render_session_key,
        render_session_metrics_snapshot, render_sessions, renderer_session_metrics, required_tiles,
        revision_artifact, revision_page_png, revision_tile_png, snapshot, source_editor_uri,
        source_file, source_file_uri, source_files, source_jump, update_source_file,
        viewer_prefixed_path_for,
    };
    use crate::compiler::{
        ArtifactSourceSpan, ArtifactSyncSpan, CompilerDriver, PageArtifactMeta, PageSyncMapArtifact,
    };

    #[tokio::test]
    async fn build_cache_ignores_untracked_paths_and_unchanged_hashes() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        fs::write(root.join("notes.txt"), "scratch").expect("notes");

        let mut cache = BuildCache::default();
        cache
            .record_success(&root, BTreeSet::from([Utf8PathBuf::from("main.tex")]))
            .await;

        let unchanged = BuildCache::plan_rebuild(&root, &cache, &[Utf8PathBuf::from("main.tex")])
            .await
            .expect("unchanged plan");
        assert!(!unchanged.needs_rebuild);

        let unrelated = BuildCache::plan_rebuild(&root, &cache, &[Utf8PathBuf::from("notes.txt")])
            .await
            .expect("unrelated plan");
        assert!(!unrelated.needs_rebuild);

        fs::write(root.join("main.tex"), "hello again").expect("updated main tex");
        let changed = BuildCache::plan_rebuild(&root, &cache, &[Utf8PathBuf::from("main.tex")])
            .await
            .expect("changed plan");
        assert!(changed.needs_rebuild);
        assert_eq!(changed.changed_inputs, vec!["main.tex".to_string()]);
    }

    async fn render_tiles_with_session(
        state: &Arc<AppState>,
        page: &PageRenderInput,
        scale: f32,
        rects: Vec<Rect>,
    ) -> Vec<TileImage> {
        let handle = render_session_handle(state);
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .tx
            .send(RenderSessionRequest::RenderTiles {
                page: page.clone(),
                scale,
                rects,
                reply: reply_tx,
            })
            .await
            .expect("send render tiles");
        reply_rx
            .await
            .expect("tile render reply")
            .expect("tile render result")
    }

    #[tokio::test]
    async fn hashing_returns_none_for_deleted_files() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");

        let hash = hash_input(&root, "missing.tex".into())
            .await
            .expect("hash missing file");
        assert_eq!(hash, None);
    }

    #[test]
    fn preview_snapshot_tracks_page_count_and_ignores_stale_updates() {
        let mut snapshot = PreviewSnapshot::default();
        snapshot.apply_started(2, vec!["main.tex".to_string()]);
        snapshot.apply_success(
            2,
            Vec::new(),
            "/artifacts/rev/2/main.pdf".to_string(),
            3,
            vec!["p0".to_string(), "p1".to_string(), "p2".to_string()],
            vec![PagePreviewArtifact {
                page_id: "p0".to_string(),
                pdf_url: "/artifacts/rev/2/pages/p0.pdf".to_string(),
                svg_url: Some("/artifacts/rev/2/pages/p0.svg".to_string()),
            }],
        );
        snapshot.apply_success(
            1,
            vec![Diagnostic {
                level: DiagnosticLevel::Error,
                file: None,
                line: None,
                message: "stale".to_string(),
            }],
            "/artifacts/rev/1/main.pdf".to_string(),
            1,
            vec!["stale".to_string()],
            vec![PagePreviewArtifact {
                page_id: "stale".to_string(),
                pdf_url: "/artifacts/rev/1/pages/stale.pdf".to_string(),
                svg_url: Some("/artifacts/rev/1/pages/stale.svg".to_string()),
            }],
        );

        assert_eq!(snapshot.current_rev, 2);
        assert_eq!(snapshot.last_applied_rev, 2);
        assert_eq!(snapshot.page_count, 3);
        assert_eq!(snapshot.page_ids, vec!["p0", "p1", "p2"]);
        assert_eq!(snapshot.page_artifacts.len(), 1);
        assert_eq!(
            snapshot.pdf_url.as_deref(),
            Some("/artifacts/rev/2/main.pdf")
        );
        assert!(snapshot.diagnostics.is_empty());
    }

    #[tokio::test]
    async fn build_cache_marks_deleted_tracked_files_dirty() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(root.join("main.tex"), "hello").expect("main tex");

        let mut cache = BuildCache::default();
        cache
            .record_success(&root, BTreeSet::from([Utf8PathBuf::from("main.tex")]))
            .await;
        fs::remove_file(root.join("main.tex")).expect("remove main tex");

        let changed = BuildCache::plan_rebuild(&root, &cache, &[Utf8PathBuf::from("main.tex")])
            .await
            .expect("changed plan");
        assert!(changed.needs_rebuild);
        assert_eq!(changed.changed_inputs, vec!["main.tex".to_string()]);
    }

    #[tokio::test]
    async fn run_build_loop_skips_unchanged_warm_rebuilds() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "warm build").expect("main tex");
        let build_root = root.join(".latexd/build");
        let artifacts_root = root.join(".latexd/artifacts");
        tokio::fs::create_dir_all(build_root.as_std_path())
            .await
            .expect("build root");
        tokio::fs::create_dir_all(artifacts_root.as_std_path())
            .await
            .expect("artifacts root");
        let (events, _) = broadcast::channel(8);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root,
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(Some("internal".to_string()), Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events,
        });
        let (rebuild_tx, rebuild_rx) = mpsc::unbounded_channel();
        let loop_state = state.clone();
        let build_task = tokio::spawn(async move {
            loop_state.run_build_loop(rebuild_rx).await;
        });

        rebuild_tx
            .send(vec![Utf8PathBuf::from("main.tex")])
            .expect("initial rebuild");
        timeout(Duration::from_secs(5), async {
            loop {
                if state.live.read().await.snapshot.last_applied_rev == 1 {
                    break;
                }
                sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("first build completion");

        assert!(build_root.join("rev-1/main.pdf").exists());

        rebuild_tx
            .send(vec![Utf8PathBuf::from("main.tex")])
            .expect("warm rebuild");
        sleep(Duration::from_millis(400)).await;

        let live = state.live.read().await;
        assert_eq!(live.snapshot.current_rev, 1);
        assert_eq!(live.snapshot.last_applied_rev, 1);
        assert_eq!(
            live.snapshot.pdf_url.as_deref(),
            Some("/artifacts/rev/1/main.pdf")
        );
        drop(live);
        assert!(!build_root.join("rev-2").exists());

        drop(rebuild_tx);
        timeout(Duration::from_secs(2), build_task)
            .await
            .expect("build loop exit")
            .expect("build loop join");
    }

    #[tokio::test]
    async fn raster_cache_reuses_same_revision_page_and_zoom() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let (events, _) = broadcast::channel(4);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events,
        });
        let page = PageRenderInput {
            page_id: "page-a".to_string(),
            revision: 11,
            content_hash: "hash-a".to_string(),
            width_px: 256,
            height_px: 256,
            pdf_path: String::new(),
        };

        let first = cache_raster_image(&state, &page, 1.0)
            .await
            .expect("first render");
        let second = cache_raster_image(&state, &page, 1.0)
            .await
            .expect("cached render");

        assert_eq!(first, second);
        let cache = state.raster_cache.read().await;
        assert_eq!(cache.len(), 1);
        assert!(cache.contains_key(&RasterCacheKey {
            rev: 11,
            page_id: "page-a".to_string(),
            content_hash: "hash-a".to_string(),
            zoom_bucket: 100,
        }));
    }

    #[tokio::test]
    async fn raster_cache_separates_zoom_and_content_boundaries() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let (events, _) = broadcast::channel(4);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events,
        });
        let page = PageRenderInput {
            page_id: "page-a".to_string(),
            revision: 12,
            content_hash: "hash-a".to_string(),
            width_px: 256,
            height_px: 256,
            pdf_path: String::new(),
        };
        let changed = PageRenderInput {
            page_id: "page-a".to_string(),
            revision: 12,
            content_hash: "hash-b".to_string(),
            width_px: 256,
            height_px: 256,
            pdf_path: String::new(),
        };

        cache_raster_image(&state, &page, 1.0)
            .await
            .expect("base render");
        cache_raster_image(&state, &page, 1.5)
            .await
            .expect("zoom render");
        cache_raster_image(&state, &changed, 1.0)
            .await
            .expect("content render");

        let cache = state.raster_cache.read().await;
        assert_eq!(cache.len(), 3);
    }

    #[tokio::test]
    async fn raster_cache_persists_png_to_disk() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let page = PageRenderInput {
            page_id: "page-a".to_string(),
            revision: 30,
            content_hash: "hash-a".to_string(),
            width_px: 256,
            height_px: 256,
            pdf_path: String::new(),
        };

        cache_raster_image(&state, &page, 1.0)
            .await
            .expect("cached render");

        let path = raster_cache_path(
            &state.build_root,
            &RasterCacheKey {
                rev: 30,
                page_id: "page-a".to_string(),
                content_hash: "hash-a".to_string(),
                zoom_bucket: 100,
            },
        );
        let bytes = fs::read(path).expect("disk raster");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[tokio::test]
    async fn raster_cache_reloads_from_disk_without_hitting_renderer() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        let page = PageRenderInput {
            page_id: "page-a".to_string(),
            revision: 31,
            content_hash: "hash-a".to_string(),
            width_px: 256,
            height_px: 256,
            pdf_path: String::new(),
        };

        let first = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let expected = cache_raster_image(&first, &page, 1.0)
            .await
            .expect("first render");

        let second = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::GsCli {
                program: "/definitely/missing-gs".to_string(),
            },
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let restored = cache_raster_image(&second, &page, 1.0)
            .await
            .expect("disk cache hit");

        assert_eq!(expected, restored);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn raster_cache_deduplicates_concurrent_cold_misses() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let calls = Arc::new(AtomicUsize::new(0));
        let tile_calls = Arc::new(AtomicUsize::new(0));
        let sessions = Arc::new(AtomicUsize::new(0));
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::CountingMock {
                calls: calls.clone(),
                tile_calls: tile_calls.clone(),
                sessions: sessions.clone(),
                sleep_ms: 50,
            },
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let page = PageRenderInput {
            page_id: "page-a".to_string(),
            revision: 32,
            content_hash: "hash-a".to_string(),
            width_px: 256,
            height_px: 256,
            pdf_path: String::new(),
        };

        let left = tokio::spawn({
            let state = state.clone();
            let page = page.clone();
            async move {
                cache_raster_image(&state, &page, 1.0)
                    .await
                    .expect("left render")
            }
        });
        let right = tokio::spawn({
            let state = state.clone();
            let page = page.clone();
            async move {
                cache_raster_image(&state, &page, 1.0)
                    .await
                    .expect("right render")
            }
        });

        let left = left.await.expect("left join");
        let right = right.await.expect("right join");

        assert_eq!(left, right);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(tile_calls.load(Ordering::SeqCst), 0);
        assert_eq!(sessions.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn renderer_session_reuses_single_renderer_actor_across_cold_misses() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let calls = Arc::new(AtomicUsize::new(0));
        let tile_calls = Arc::new(AtomicUsize::new(0));
        let sessions = Arc::new(AtomicUsize::new(0));
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::CountingMock {
                calls: calls.clone(),
                tile_calls: tile_calls.clone(),
                sessions: sessions.clone(),
                sleep_ms: 1,
            },
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let first_page = PageRenderInput {
            page_id: "page-a".to_string(),
            revision: 33,
            content_hash: "hash-a".to_string(),
            width_px: 256,
            height_px: 256,
            pdf_path: String::new(),
        };
        let second_page = PageRenderInput {
            page_id: "page-b".to_string(),
            revision: 33,
            content_hash: "hash-b".to_string(),
            width_px: 256,
            height_px: 256,
            pdf_path: String::new(),
        };

        cache_raster_image(&state, &first_page, 1.0)
            .await
            .expect("first cold render");
        cache_raster_image(&state, &second_page, 1.0)
            .await
            .expect("second cold render");

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(tile_calls.load(Ordering::SeqCst), 0);
        assert_eq!(sessions.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn attached_revision_page_lookup_works_without_disk_metadata() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        attach_render_revision(
            &state,
            40,
            &[PageArtifactMeta {
                page_id: "page-a".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 256,
                height_pt: 384,
                content_hash: "hash-a".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
                pdf_artifact_path: Utf8PathBuf::from("rev-40/pages/page-a.pdf"),
                source_spans: vec![],
            }],
        )
        .await;

        let page = load_revision_page_input(&state, 40, "page-a")
            .await
            .expect("attached page");

        assert_eq!(page.page_id, "page-a");
        assert_eq!(page.revision, 40);
        assert_eq!(
            page.pdf_path,
            state.build_root.join("rev-40/pages/page-a.pdf").to_string()
        );
    }

    #[tokio::test]
    async fn attached_revision_metadata_set_works_without_disk_metadata() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        attach_render_revision(
            &state,
            41,
            &[PageArtifactMeta {
                page_id: "page-a".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 256,
                height_pt: 384,
                content_hash: "hash-a".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
                pdf_artifact_path: Utf8PathBuf::from("rev-41/pages/page-a.pdf"),
                source_spans: vec![],
            }],
        )
        .await;

        let page = load_revision_page_metadata(&state, 41, "page-a")
            .await
            .expect("attached metadata");
        let pages = load_revision_page_metadata_set(&state, 41)
            .await
            .expect("attached metadata set");

        assert_eq!(page.page_id, "page-a");
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].page_id, "page-a");
    }

    #[tokio::test]
    async fn revision_metadata_disk_fallback_attaches_pages_into_render_session() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-60")).expect("revision directory");
        fs::write(
            build_root.join("rev-60/page-metadata.json"),
            serde_json::to_vec(&vec![PageArtifactMeta {
                page_id: "page-a".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 256,
                height_pt: 384,
                content_hash: "hash-a".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
                pdf_artifact_path: Utf8PathBuf::from("rev-60/pages/page-a.pdf"),
                source_spans: vec![],
            }])
            .expect("page metadata"),
        )
        .expect("write page metadata");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let first = load_revision_page_metadata_set(&state, 60)
            .await
            .expect("disk metadata load");
        fs::remove_file(build_root.join("rev-60/page-metadata.json")).expect("remove metadata");
        let second = load_revision_page_metadata_set(&state, 60)
            .await
            .expect("attached metadata load");

        assert_eq!(first, second);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].page_id, "page-a");
    }

    #[tokio::test]
    async fn renderer_session_attached_revisions_keep_recent_four_and_support_detach() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        for rev in 50..=54 {
            attach_render_revision(
                &state,
                rev,
                &[PageArtifactMeta {
                    page_id: format!("page-{rev}"),
                    index: 0,
                    line_count: 1,
                    width_pt: 128,
                    height_pt: 128,
                    content_hash: format!("hash-{rev}"),
                    text_start_utf8: 0,
                    text_end_utf8: 1,
                    pdf_artifact_path: Utf8PathBuf::from(format!("rev-{rev}/pages/page-{rev}.pdf")),
                    source_spans: vec![],
                }],
            )
            .await;
        }

        assert!(
            lookup_attached_page_input(&state, 50, "page-50")
                .await
                .is_none()
        );
        assert!(
            lookup_attached_page_input(&state, 51, "page-51")
                .await
                .is_some()
        );
        assert!(
            lookup_attached_page_input(&state, 52, "page-52")
                .await
                .is_some()
        );
        assert!(
            lookup_attached_page_input(&state, 53, "page-53")
                .await
                .is_some()
        );
        assert!(
            lookup_attached_page_input(&state, 54, "page-54")
                .await
                .is_some()
        );

        detach_render_revision(&state, 54).await;
        assert!(
            lookup_attached_page_input(&state, 54, "page-54")
                .await
                .is_none()
        );
        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.attached_live_page_count, 3);
        assert_eq!(
            snapshot.attached_revision_window_limit,
            RENDER_SESSION_ATTACHED_REVISION_WINDOW
        );
        assert_eq!(
            snapshot.attached_page_budget,
            RENDER_SESSION_ATTACHED_PAGE_BUDGET
        );
        assert_eq!(snapshot.evict_count, 1);
        assert_eq!(
            snapshot.attached_revisions,
            vec![
                RenderSessionAttachedRevisionSnapshot {
                    rev: 51,
                    page_count: 1,
                    page_ids: vec!["page-51".to_string()],
                },
                RenderSessionAttachedRevisionSnapshot {
                    rev: 52,
                    page_count: 1,
                    page_ids: vec!["page-52".to_string()],
                },
                RenderSessionAttachedRevisionSnapshot {
                    rev: 53,
                    page_count: 1,
                    page_ids: vec!["page-53".to_string()],
                },
            ]
        );
    }

    #[tokio::test]
    async fn renderer_session_page_lookup_refreshes_attached_revision_lru_order() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        for rev in 50..=53 {
            attach_render_revision(
                &state,
                rev,
                &[PageArtifactMeta {
                    page_id: format!("page-{rev}"),
                    index: 0,
                    line_count: 1,
                    width_pt: 128,
                    height_pt: 128,
                    content_hash: format!("hash-{rev}"),
                    text_start_utf8: 0,
                    text_end_utf8: 1,
                    pdf_artifact_path: Utf8PathBuf::from(format!("rev-{rev}/pages/page-{rev}.pdf")),
                    source_spans: vec![],
                }],
            )
            .await;
        }

        assert!(
            lookup_attached_page_input(&state, 50, "page-50")
                .await
                .is_some()
        );

        attach_render_revision(
            &state,
            54,
            &[PageArtifactMeta {
                page_id: "page-54".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 128,
                height_pt: 128,
                content_hash: "hash-54".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 1,
                pdf_artifact_path: Utf8PathBuf::from("rev-54/pages/page-54.pdf"),
                source_spans: vec![],
            }],
        )
        .await;

        assert!(
            lookup_attached_page_input(&state, 51, "page-51")
                .await
                .is_none()
        );
        assert!(
            lookup_attached_page_input(&state, 50, "page-50")
                .await
                .is_some()
        );
        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(
            snapshot.attached_revisions,
            vec![
                RenderSessionAttachedRevisionSnapshot {
                    rev: 52,
                    page_count: 1,
                    page_ids: vec!["page-52".to_string()],
                },
                RenderSessionAttachedRevisionSnapshot {
                    rev: 53,
                    page_count: 1,
                    page_ids: vec!["page-53".to_string()],
                },
                RenderSessionAttachedRevisionSnapshot {
                    rev: 54,
                    page_count: 1,
                    page_ids: vec!["page-54".to_string()],
                },
                RenderSessionAttachedRevisionSnapshot {
                    rev: 50,
                    page_count: 1,
                    page_ids: vec!["page-50".to_string()],
                },
            ]
        );
        assert_eq!(snapshot.evict_count, 1);
    }

    #[tokio::test]
    async fn renderer_session_eviction_respects_attached_page_budget() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let rev70_pages = (0..24)
            .map(|index| PageArtifactMeta {
                page_id: format!("page-70-{index}"),
                index,
                line_count: 1,
                width_pt: 128,
                height_pt: 128,
                content_hash: format!("hash-70-{index}"),
                text_start_utf8: 0,
                text_end_utf8: 1,
                pdf_artifact_path: Utf8PathBuf::from(format!("rev-70/pages/page-70-{index}.pdf")),
                source_spans: vec![],
            })
            .collect::<Vec<_>>();
        let rev71_pages = (0..25)
            .map(|index| PageArtifactMeta {
                page_id: format!("page-71-{index}"),
                index,
                line_count: 1,
                width_pt: 128,
                height_pt: 128,
                content_hash: format!("hash-71-{index}"),
                text_start_utf8: 0,
                text_end_utf8: 1,
                pdf_artifact_path: Utf8PathBuf::from(format!("rev-71/pages/page-71-{index}.pdf")),
                source_spans: vec![],
            })
            .collect::<Vec<_>>();

        attach_render_revision(&state, 70, &rev70_pages).await;
        attach_render_revision(&state, 71, &rev71_pages).await;

        assert!(
            lookup_attached_page_input(&state, 70, "page-70-0")
                .await
                .is_none()
        );
        assert!(
            lookup_attached_page_input(&state, 71, "page-71-0")
                .await
                .is_some()
        );

        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.attached_live_page_count, 25);
        assert_eq!(snapshot.evict_count, 1);
        assert_eq!(
            snapshot.attached_revisions,
            vec![RenderSessionAttachedRevisionSnapshot {
                rev: 71,
                page_count: 25,
                page_ids: rev71_pages
                    .iter()
                    .map(|page| page.page_id.clone())
                    .collect(),
            }]
        );
    }

    #[tokio::test]
    async fn renderer_session_metrics_report_actor_activity() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let page_metadata = PageArtifactMeta {
            page_id: "page-a".to_string(),
            index: 0,
            line_count: 1,
            width_pt: 256,
            height_pt: 256,
            content_hash: "hash-a".to_string(),
            text_start_utf8: 0,
            text_end_utf8: 1,
            pdf_artifact_path: Utf8PathBuf::from("rev-70/pages/page-a.pdf"),
            source_spans: vec![],
        };

        attach_render_revision(&state, 70, std::slice::from_ref(&page_metadata)).await;
        let _ = load_revision_page_metadata_set(&state, 70)
            .await
            .expect("revision pages");
        let page = load_revision_page_input(&state, 70, "page-a")
            .await
            .expect("page input");
        cache_raster_image(&state, &page, 1.0)
            .await
            .expect("rendered image");

        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.active_session_count, 1);
        assert_eq!(snapshot.actor_spawn_count, 1);
        assert_eq!(snapshot.actor_restart_count, 0);
        assert_eq!(snapshot.actor_failure_count, 0);
        assert_eq!(snapshot.attach_count, 1);
        assert_eq!(snapshot.attached_page_count, 1);
        assert_eq!(snapshot.attached_live_page_count, 1);
        assert_eq!(
            snapshot.attached_revision_window_limit,
            RENDER_SESSION_ATTACHED_REVISION_WINDOW
        );
        assert_eq!(
            snapshot.attached_page_budget,
            RENDER_SESSION_ATTACHED_PAGE_BUDGET
        );
        assert_eq!(
            snapshot.warm_bucket_budget,
            RENDER_SESSION_WARM_BUCKET_BUDGET
        );
        assert_eq!(
            snapshot.warm_bucket_page_budget,
            RENDER_SESSION_WARM_BUCKET_PAGE_BUDGET
        );
        assert_eq!(snapshot.warm_bucket_count, 1);
        assert_eq!(snapshot.tile_cache_budget, RENDER_SESSION_TILE_CACHE_BUDGET);
        assert_eq!(
            snapshot.tile_cache_page_budget,
            RENDER_SESSION_TILE_CACHE_PAGE_BUDGET
        );
        assert_eq!(snapshot.tile_cache_count, 0);
        assert_eq!(snapshot.evict_count, 0);
        assert_eq!(snapshot.detach_count, 0);
        assert_eq!(snapshot.page_lookup_count, 1);
        assert_eq!(snapshot.revision_lookup_count, 1);
        assert_eq!(snapshot.warm_bucket_evict_count, 0);
        assert_eq!(snapshot.tile_cache_evict_count, 0);
        assert_eq!(snapshot.prewarm_request_count, 0);
        assert_eq!(snapshot.skipped_prewarm_count, 0);
        assert_eq!(snapshot.render_request_count, 1);
        assert_eq!(snapshot.tile_render_request_count, 0);
        assert_eq!(snapshot.fallback_prewarm_count, 0);
        assert_eq!(snapshot.fallback_render_count, 0);
        assert_eq!(snapshot.fallback_tile_render_count, 0);
        assert_eq!(snapshot.render_latency.count, 1);
        assert_eq!(
            snapshot.render_latency.total_ms,
            u64::from(snapshot.render_latency.max_ms)
        );
        assert_eq!(
            snapshot.tile_render_latency,
            RenderSessionLatencySummary {
                count: 0,
                total_ms: 0,
                max_ms: 0,
            }
        );
        assert_eq!(
            snapshot.prewarm_latency,
            RenderSessionLatencySummary {
                count: 0,
                total_ms: 0,
                max_ms: 0,
            }
        );
        assert_eq!(
            snapshot.fallback_prewarm_latency,
            RenderSessionLatencySummary {
                count: 0,
                total_ms: 0,
                max_ms: 0,
            }
        );
        assert_eq!(
            snapshot.fallback_render_latency,
            RenderSessionLatencySummary {
                count: 0,
                total_ms: 0,
                max_ms: 0,
            }
        );
        assert_eq!(
            snapshot.fallback_tile_render_latency,
            RenderSessionLatencySummary {
                count: 0,
                total_ms: 0,
                max_ms: 0,
            }
        );
        assert_eq!(
            snapshot.attached_revisions,
            vec![RenderSessionAttachedRevisionSnapshot {
                rev: 70,
                page_count: 1,
                page_ids: vec!["page-a".to_string()],
            }]
        );
        assert_eq!(
            snapshot.warm_buckets,
            vec![RenderSessionWarmBucketSnapshot {
                rev: 70,
                page_id: "page-a".to_string(),
                content_hash: "hash-a".to_string(),
                zoom_bucket: 100,
            }]
        );
        assert!(snapshot.tile_cache_entries.is_empty());
        assert_eq!(
            snapshot
                .recent_events
                .iter()
                .map(|event| event.kind.clone())
                .collect::<Vec<_>>(),
            vec![
                RenderSessionEventKind::ActorSpawn,
                RenderSessionEventKind::AttachRevision,
                RenderSessionEventKind::LookupRevisionPages,
                RenderSessionEventKind::LookupPage,
                RenderSessionEventKind::RenderPage,
            ]
        );
        assert_eq!(
            snapshot.recent_events.last().map(|event| event.rev),
            Some(Some(70))
        );
        assert_eq!(
            snapshot
                .recent_events
                .last()
                .and_then(|event| event.page_id.as_deref()),
            Some("page-a")
        );
        assert_eq!(
            snapshot
                .recent_events
                .last()
                .and_then(|event| event.scale_percent),
            Some(100)
        );
        let response = renderer_session_metrics(State(state.clone())).await;
        assert_eq!(response.0, snapshot);
    }

    #[tokio::test]
    async fn renderer_session_metrics_track_actor_restart_after_channel_failure() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let key = render_session_key(&state);
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        render_sessions()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key, RenderSessionHandle { tx });
        let page = PageRenderInput {
            page_id: "page-a".to_string(),
            revision: 71,
            content_hash: "hash-a".to_string(),
            width_px: 128,
            height_px: 128,
            pdf_path: String::new(),
        };

        cache_raster_image(&state, &page, 1.0)
            .await
            .expect("rendered via restarted actor");

        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.active_session_count, 1);
        assert_eq!(snapshot.actor_spawn_count, 1);
        assert_eq!(snapshot.actor_restart_count, 1);
        assert_eq!(snapshot.actor_failure_count, 1);
        assert_eq!(snapshot.render_request_count, 1);
        assert_eq!(snapshot.tile_render_request_count, 0);
        assert_eq!(snapshot.prewarm_request_count, 0);
        assert_eq!(snapshot.skipped_prewarm_count, 0);
        assert_eq!(snapshot.fallback_prewarm_count, 0);
        assert_eq!(snapshot.fallback_render_count, 0);
        assert_eq!(snapshot.fallback_tile_render_count, 0);
        assert!(snapshot.attached_revisions.is_empty());
        let mut recent_events = snapshot.recent_events.clone();
        for event in &mut recent_events {
            event.duration_ms = None;
        }
        assert_eq!(
            recent_events,
            vec![
                RenderSessionEvent {
                    kind: RenderSessionEventKind::ActorFailure,
                    rev: Some(71),
                    page_id: Some("page-a".to_string()),
                    scale_percent: Some(100),
                    page_count: None,
                    duration_ms: None,
                    rendered_rect_count: None,
                    reused_rect_count: None,
                },
                RenderSessionEvent {
                    kind: RenderSessionEventKind::ActorRestart,
                    rev: Some(71),
                    page_id: Some("page-a".to_string()),
                    scale_percent: Some(100),
                    page_count: None,
                    duration_ms: None,
                    rendered_rect_count: None,
                    reused_rect_count: None,
                },
                RenderSessionEvent {
                    kind: RenderSessionEventKind::ActorSpawn,
                    rev: None,
                    page_id: None,
                    scale_percent: None,
                    page_count: None,
                    duration_ms: None,
                    rendered_rect_count: None,
                    reused_rect_count: None,
                },
                RenderSessionEvent {
                    kind: RenderSessionEventKind::RenderPage,
                    rev: Some(71),
                    page_id: Some("page-a".to_string()),
                    scale_percent: Some(100),
                    page_count: None,
                    duration_ms: None,
                    rendered_rect_count: None,
                    reused_rect_count: None,
                },
            ]
        );
    }

    #[test]
    fn renderer_session_metrics_keep_recent_event_window() {
        let metrics = RenderSessionMetrics::default();
        for rev in 0..40 {
            metrics.record_event(
                RenderSessionEventKind::LookupRevisionPages,
                Some(rev),
                None,
                None,
                None,
                None,
            );
        }

        let snapshot = metrics.snapshot(0);
        assert_eq!(
            snapshot.recent_events.len(),
            RENDER_SESSION_RECENT_EVENT_WINDOW
        );
        assert_eq!(
            snapshot.recent_events.first().and_then(|event| event.rev),
            Some(8)
        );
        assert_eq!(
            snapshot.recent_events.last().and_then(|event| event.rev),
            Some(39)
        );
    }

    #[test]
    fn renderer_session_metrics_accumulate_latency_summaries() {
        let metrics = RenderSessionMetrics::default();
        metrics.render_request_count.fetch_add(2, Ordering::SeqCst);
        metrics.prewarm_request_count.fetch_add(1, Ordering::SeqCst);
        metrics
            .fallback_tile_render_count
            .fetch_add(1, Ordering::SeqCst);
        metrics.record_event(
            RenderSessionEventKind::RenderPage,
            Some(1),
            Some("page-a"),
            Some(1.0),
            None,
            Some(Duration::from_millis(12)),
        );
        metrics.record_event(
            RenderSessionEventKind::RenderPage,
            Some(1),
            Some("page-b"),
            Some(1.0),
            None,
            Some(Duration::from_millis(7)),
        );
        metrics.record_event(
            RenderSessionEventKind::PrewarmPage,
            Some(1),
            Some("page-a"),
            Some(1.25),
            Some(1),
            Some(Duration::from_millis(4)),
        );
        metrics.record_event(
            RenderSessionEventKind::FallbackRenderTiles,
            Some(1),
            Some("page-c"),
            Some(1.5),
            Some(1),
            Some(Duration::from_millis(9)),
        );

        let snapshot = metrics.snapshot(0);
        assert_eq!(
            snapshot.render_latency,
            RenderSessionLatencySummary {
                count: 2,
                total_ms: 19,
                max_ms: 12,
            }
        );
        assert_eq!(
            snapshot.prewarm_latency,
            RenderSessionLatencySummary {
                count: 1,
                total_ms: 4,
                max_ms: 4,
            }
        );
        assert_eq!(
            snapshot.fallback_tile_render_latency,
            RenderSessionLatencySummary {
                count: 1,
                total_ms: 9,
                max_ms: 9,
            }
        );
    }

    #[tokio::test]
    async fn viewport_prewarm_prioritizes_current_visible_then_adjacent_pages() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let calls = Arc::new(AtomicUsize::new(0));
        let tile_calls = Arc::new(AtomicUsize::new(0));
        let sessions = Arc::new(AtomicUsize::new(0));
        let (events, _) = broadcast::channel(4);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::CountingMock {
                calls: calls.clone(),
                tile_calls: tile_calls.clone(),
                sessions: sessions.clone(),
                sleep_ms: 1,
            },
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState {
                snapshot: PreviewSnapshot {
                    last_applied_rev: 21,
                    ..PreviewSnapshot::default()
                },
                latest_pdf_path: None,
                page_metadata: vec![
                    PageArtifactMeta {
                        page_id: "page-a".to_string(),
                        index: 0,
                        line_count: 1,
                        width_pt: 256,
                        height_pt: 256,
                        content_hash: "hash-a".to_string(),
                        text_start_utf8: 0,
                        text_end_utf8: 10,
                        pdf_artifact_path: Utf8PathBuf::from("rev-21/pages/page-a.pdf"),
                        source_spans: vec![],
                    },
                    PageArtifactMeta {
                        page_id: "page-b".to_string(),
                        index: 1,
                        line_count: 1,
                        width_pt: 256,
                        height_pt: 256,
                        content_hash: "hash-b".to_string(),
                        text_start_utf8: 10,
                        text_end_utf8: 20,
                        pdf_artifact_path: Utf8PathBuf::from("rev-21/pages/page-b.pdf"),
                        source_spans: vec![],
                    },
                    PageArtifactMeta {
                        page_id: "page-c".to_string(),
                        index: 2,
                        line_count: 1,
                        width_pt: 256,
                        height_pt: 256,
                        content_hash: "hash-c".to_string(),
                        text_start_utf8: 20,
                        text_end_utf8: 30,
                        pdf_artifact_path: Utf8PathBuf::from("rev-21/pages/page-c.pdf"),
                        source_spans: vec![],
                    },
                ],
            }),
            events,
        });

        prewarm_viewport_rasters(
            &state,
            ClientMsg::ViewportChanged {
                zoom: 1.0,
                current_page: 2,
                scroll_top: 0.0,
                visible_pages: vec!["page-c".to_string(), "page-b".to_string()],
            },
        )
        .await;

        assert!(state.raster_cache.read().await.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(tile_calls.load(Ordering::SeqCst), 7);
        assert_eq!(sessions.load(Ordering::SeqCst), 1);
        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.prewarm_request_count, 7);
        assert_eq!(snapshot.skipped_prewarm_count, 0);
        assert_eq!(snapshot.fallback_prewarm_count, 0);
        assert_eq!(snapshot.render_request_count, 0);
        assert_eq!(snapshot.tile_render_request_count, 0);
        assert_eq!(
            snapshot
                .recent_events
                .iter()
                .filter(|event| event.kind == RenderSessionEventKind::PrewarmPage)
                .map(|event| {
                    (
                        event.page_id.as_deref().unwrap_or_default().to_string(),
                        event.scale_percent,
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("page-b".to_string(), Some(100)),
                ("page-c".to_string(), Some(100)),
                ("page-a".to_string(), Some(100)),
                ("page-b".to_string(), Some(90)),
                ("page-b".to_string(), Some(110)),
                ("page-c".to_string(), Some(90)),
                ("page-c".to_string(), Some(110)),
            ]
        );
    }

    #[tokio::test]
    async fn viewport_prewarm_falls_back_to_current_page_then_adjacent_pages() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let calls = Arc::new(AtomicUsize::new(0));
        let tile_calls = Arc::new(AtomicUsize::new(0));
        let sessions = Arc::new(AtomicUsize::new(0));
        let (events, _) = broadcast::channel(4);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::CountingMock {
                calls: calls.clone(),
                tile_calls: tile_calls.clone(),
                sessions: sessions.clone(),
                sleep_ms: 1,
            },
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState {
                snapshot: PreviewSnapshot {
                    last_applied_rev: 22,
                    ..PreviewSnapshot::default()
                },
                latest_pdf_path: None,
                page_metadata: vec![
                    PageArtifactMeta {
                        page_id: "page-a".to_string(),
                        index: 0,
                        line_count: 1,
                        width_pt: 256,
                        height_pt: 256,
                        content_hash: "hash-a".to_string(),
                        text_start_utf8: 0,
                        text_end_utf8: 10,
                        pdf_artifact_path: Utf8PathBuf::from("rev-22/pages/page-a.pdf"),
                        source_spans: vec![],
                    },
                    PageArtifactMeta {
                        page_id: "page-b".to_string(),
                        index: 1,
                        line_count: 1,
                        width_pt: 256,
                        height_pt: 256,
                        content_hash: "hash-b".to_string(),
                        text_start_utf8: 10,
                        text_end_utf8: 20,
                        pdf_artifact_path: Utf8PathBuf::from("rev-22/pages/page-b.pdf"),
                        source_spans: vec![],
                    },
                    PageArtifactMeta {
                        page_id: "page-c".to_string(),
                        index: 2,
                        line_count: 1,
                        width_pt: 256,
                        height_pt: 256,
                        content_hash: "hash-c".to_string(),
                        text_start_utf8: 20,
                        text_end_utf8: 30,
                        pdf_artifact_path: Utf8PathBuf::from("rev-22/pages/page-c.pdf"),
                        source_spans: vec![],
                    },
                ],
            }),
            events,
        });

        prewarm_viewport_rasters(
            &state,
            ClientMsg::ViewportChanged {
                zoom: 1.5,
                current_page: 2,
                scroll_top: 42.0,
                visible_pages: Vec::new(),
            },
        )
        .await;

        assert!(state.raster_cache.read().await.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(tile_calls.load(Ordering::SeqCst), 5);
        assert_eq!(sessions.load(Ordering::SeqCst), 1);
        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.prewarm_request_count, 5);
        assert_eq!(snapshot.skipped_prewarm_count, 0);
        assert_eq!(snapshot.fallback_prewarm_count, 0);
        assert_eq!(
            snapshot
                .recent_events
                .iter()
                .filter(|event| event.kind == RenderSessionEventKind::PrewarmPage)
                .map(|event| {
                    (
                        event.page_id.as_deref().unwrap_or_default().to_string(),
                        event.scale_percent,
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("page-b".to_string(), Some(150)),
                ("page-a".to_string(), Some(150)),
                ("page-c".to_string(), Some(150)),
                ("page-b".to_string(), Some(140)),
                ("page-b".to_string(), Some(160)),
            ]
        );
    }

    #[tokio::test]
    async fn viewport_prewarm_suppresses_duplicate_inflight_requests_for_same_page_and_zoom() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let calls = Arc::new(AtomicUsize::new(0));
        let tile_calls = Arc::new(AtomicUsize::new(0));
        let sessions = Arc::new(AtomicUsize::new(0));
        let (events, _) = broadcast::channel(4);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::CountingMock {
                calls: calls.clone(),
                tile_calls: tile_calls.clone(),
                sessions: sessions.clone(),
                sleep_ms: 10,
            },
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState {
                snapshot: PreviewSnapshot {
                    last_applied_rev: 23,
                    ..PreviewSnapshot::default()
                },
                latest_pdf_path: None,
                page_metadata: vec![
                    PageArtifactMeta {
                        page_id: "page-a".to_string(),
                        index: 0,
                        line_count: 1,
                        width_pt: 256,
                        height_pt: 256,
                        content_hash: "hash-a".to_string(),
                        text_start_utf8: 0,
                        text_end_utf8: 10,
                        pdf_artifact_path: Utf8PathBuf::from("rev-23/pages/page-a.pdf"),
                        source_spans: vec![],
                    },
                    PageArtifactMeta {
                        page_id: "page-b".to_string(),
                        index: 1,
                        line_count: 1,
                        width_pt: 256,
                        height_pt: 256,
                        content_hash: "hash-b".to_string(),
                        text_start_utf8: 10,
                        text_end_utf8: 20,
                        pdf_artifact_path: Utf8PathBuf::from("rev-23/pages/page-b.pdf"),
                        source_spans: vec![],
                    },
                ],
            }),
            events,
        });
        let message = ClientMsg::ViewportChanged {
            zoom: 1.25,
            current_page: 1,
            scroll_top: 0.0,
            visible_pages: vec!["page-a".to_string(), "page-b".to_string()],
        };

        let first = prewarm_viewport_rasters(&state, message.clone());
        let second = prewarm_viewport_rasters(&state, message);
        tokio::join!(first, second);

        assert!(state.raster_cache.read().await.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(tile_calls.load(Ordering::SeqCst), 6);
        assert_eq!(sessions.load(Ordering::SeqCst), 1);
        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.prewarm_request_count, 6);
        assert_eq!(snapshot.skipped_prewarm_count, 6);
        assert_eq!(snapshot.fallback_prewarm_count, 0);
        assert_eq!(
            snapshot
                .recent_events
                .iter()
                .filter(|event| event.kind == RenderSessionEventKind::SkipPrewarmPage)
                .map(|event| {
                    (
                        event.page_id.as_deref().unwrap_or_default().to_string(),
                        event.scale_percent,
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("page-a".to_string(), Some(125)),
                ("page-b".to_string(), Some(125)),
                ("page-a".to_string(), Some(115)),
                ("page-a".to_string(), Some(135)),
                ("page-b".to_string(), Some(115)),
                ("page-b".to_string(), Some(135)),
            ]
        );
    }

    #[tokio::test]
    async fn viewport_prewarm_skips_already_warm_buckets_without_leaking_inflight_claims() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let calls = Arc::new(AtomicUsize::new(0));
        let tile_calls = Arc::new(AtomicUsize::new(0));
        let sessions = Arc::new(AtomicUsize::new(0));
        let (events, _) = broadcast::channel(4);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::CountingMock {
                calls: calls.clone(),
                tile_calls: tile_calls.clone(),
                sessions: sessions.clone(),
                sleep_ms: 1,
            },
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState {
                snapshot: PreviewSnapshot {
                    last_applied_rev: 24,
                    ..PreviewSnapshot::default()
                },
                latest_pdf_path: None,
                page_metadata: vec![
                    PageArtifactMeta {
                        page_id: "page-a".to_string(),
                        index: 0,
                        line_count: 1,
                        width_pt: 256,
                        height_pt: 256,
                        content_hash: "hash-a".to_string(),
                        text_start_utf8: 0,
                        text_end_utf8: 10,
                        pdf_artifact_path: Utf8PathBuf::from("rev-24/pages/page-a.pdf"),
                        source_spans: vec![],
                    },
                    PageArtifactMeta {
                        page_id: "page-b".to_string(),
                        index: 1,
                        line_count: 1,
                        width_pt: 256,
                        height_pt: 256,
                        content_hash: "hash-b".to_string(),
                        text_start_utf8: 10,
                        text_end_utf8: 20,
                        pdf_artifact_path: Utf8PathBuf::from("rev-24/pages/page-b.pdf"),
                        source_spans: vec![],
                    },
                ],
            }),
            events,
        });
        let message = ClientMsg::ViewportChanged {
            zoom: 1.25,
            current_page: 1,
            scroll_top: 0.0,
            visible_pages: vec!["page-a".to_string(), "page-b".to_string()],
        };

        prewarm_viewport_rasters(&state, message.clone()).await;
        prewarm_viewport_rasters(&state, message).await;

        assert!(state.raster_cache.read().await.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(tile_calls.load(Ordering::SeqCst), 6);
        assert_eq!(sessions.load(Ordering::SeqCst), 1);
        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.prewarm_request_count, 6);
        assert_eq!(snapshot.skipped_prewarm_count, 6);
        assert_eq!(snapshot.fallback_prewarm_count, 0);
        assert_eq!(snapshot.warm_bucket_count, 6);
        assert_eq!(
            snapshot
                .recent_events
                .iter()
                .filter(|event| event.kind == RenderSessionEventKind::SkipPrewarmPage)
                .map(|event| {
                    (
                        event.page_id.as_deref().unwrap_or_default().to_string(),
                        event.scale_percent,
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("page-a".to_string(), Some(125)),
                ("page-b".to_string(), Some(125)),
                ("page-a".to_string(), Some(115)),
                ("page-a".to_string(), Some(135)),
                ("page-b".to_string(), Some(115)),
                ("page-b".to_string(), Some(135)),
            ]
        );
    }

    #[tokio::test]
    async fn renderer_session_limits_warm_buckets_per_page_to_recent_zoom_buckets() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let page_metadata = PageArtifactMeta {
            page_id: "page-a".to_string(),
            index: 0,
            line_count: 1,
            width_pt: 256,
            height_pt: 256,
            content_hash: "hash-a".to_string(),
            text_start_utf8: 0,
            text_end_utf8: 10,
            pdf_artifact_path: Utf8PathBuf::from("rev-81/pages/page-a.pdf"),
            source_spans: vec![],
        };

        attach_render_revision(&state, 81, std::slice::from_ref(&page_metadata)).await;
        let page = load_revision_page_input(&state, 81, "page-a")
            .await
            .expect("page input");
        for scale in [0.8_f32, 0.9, 1.0, 1.1, 1.2] {
            cache_raster_image(&state, &page, scale)
                .await
                .expect("rendered image");
        }

        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(
            snapshot.warm_bucket_page_budget,
            RENDER_SESSION_WARM_BUCKET_PAGE_BUDGET
        );
        assert_eq!(
            snapshot.warm_bucket_count,
            RENDER_SESSION_WARM_BUCKET_PAGE_BUDGET
        );
        assert_eq!(snapshot.warm_bucket_evict_count, 1);
        assert_eq!(
            snapshot
                .warm_buckets
                .iter()
                .map(|bucket| bucket.zoom_bucket)
                .collect::<Vec<_>>(),
            vec![90, 100, 110, 120]
        );
        assert!(snapshot.recent_events.iter().any(|event| {
            event.kind == RenderSessionEventKind::EvictWarmBucket
                && event.rev == Some(81)
                && event.page_id.as_deref() == Some("page-a")
                && event.scale_percent == Some(80)
        }));
    }

    #[tokio::test]
    async fn renderer_session_prewarm_reuses_cached_tile_after_warm_bucket_eviction() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let tile_calls = Arc::new(AtomicUsize::new(0));
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::CountingMock {
                calls: Arc::new(AtomicUsize::new(0)),
                tile_calls: tile_calls.clone(),
                sessions: Arc::new(AtomicUsize::new(0)),
                sleep_ms: 1,
            },
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let page_metadata = PageArtifactMeta {
            page_id: "page-a".to_string(),
            index: 0,
            line_count: 1,
            width_pt: 256,
            height_pt: 256,
            content_hash: "hash-a".to_string(),
            text_start_utf8: 0,
            text_end_utf8: 10,
            pdf_artifact_path: Utf8PathBuf::from("rev-90/pages/page-a.pdf"),
            source_spans: vec![],
        };

        attach_render_revision(&state, 90, std::slice::from_ref(&page_metadata)).await;
        let page = load_revision_page_input(&state, 90, "page-a")
            .await
            .expect("page input");
        let handle = render_session_handle(&state);

        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .tx
            .send(RenderSessionRequest::PrewarmPage {
                page: page.clone(),
                scale: 1.0,
                reply: reply_tx,
            })
            .await
            .expect("send prewarm");
        reply_rx.await.expect("prewarm reply").expect("prewarm ok");
        assert_eq!(tile_calls.load(Ordering::SeqCst), 1);

        for scale in [0.8_f32, 0.9, 1.1, 1.2, 1.3] {
            cache_raster_image(&state, &page, scale)
                .await
                .expect("rendered image");
        }

        let snapshot = render_session_metrics_snapshot(&state).await;
        assert!(!snapshot.warm_buckets.iter().any(|bucket| {
            bucket.rev == 90 && bucket.page_id == "page-a" && bucket.zoom_bucket == 100
        }));
        let skipped_before = snapshot.skipped_prewarm_count;

        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .tx
            .send(RenderSessionRequest::PrewarmPage {
                page,
                scale: 1.0,
                reply: reply_tx,
            })
            .await
            .expect("send prewarm");
        reply_rx.await.expect("prewarm reply").expect("prewarm ok");
        assert_eq!(tile_calls.load(Ordering::SeqCst), 1);

        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.skipped_prewarm_count, skipped_before + 1);
        assert!(snapshot.warm_buckets.iter().any(|bucket| {
            bucket.rev == 90 && bucket.page_id == "page-a" && bucket.zoom_bucket == 100
        }));
        assert!(snapshot.recent_events.iter().any(|event| {
            event.kind == RenderSessionEventKind::SkipPrewarmPage
                && event.rev == Some(90)
                && event.page_id.as_deref() == Some("page-a")
                && event.scale_percent == Some(100)
        }));
    }

    #[tokio::test]
    async fn renderer_session_detach_clears_warm_buckets_and_tile_cache_for_removed_revision() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let page_metadata = PageArtifactMeta {
            page_id: "page-a".to_string(),
            index: 0,
            line_count: 1,
            width_pt: 256,
            height_pt: 256,
            content_hash: "hash-a".to_string(),
            text_start_utf8: 0,
            text_end_utf8: 10,
            pdf_artifact_path: Utf8PathBuf::from("rev-80/pages/page-a.pdf"),
            source_spans: vec![],
        };

        attach_render_revision(&state, 80, std::slice::from_ref(&page_metadata)).await;
        let page = load_revision_page_input(&state, 80, "page-a")
            .await
            .expect("page input");
        let handle = render_session_handle(&state);
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .tx
            .send(RenderSessionRequest::RenderTiles {
                page,
                scale: 1.0,
                rects: vec![Rect {
                    x: 0,
                    y: 0,
                    width: 128,
                    height: 128,
                }],
                reply: reply_tx,
            })
            .await
            .expect("send render tiles");
        reply_rx
            .await
            .expect("tile render reply")
            .expect("session tile render");

        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.warm_bucket_count, 1);
        assert_eq!(snapshot.tile_cache_count, 1);
        detach_render_revision(&state, 80).await;
        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.warm_bucket_count, 0);
        assert_eq!(snapshot.tile_cache_count, 0);
        assert!(snapshot.warm_buckets.is_empty());
    }

    #[tokio::test]
    async fn revision_artifact_serves_nested_svg_bytes() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-3/pages")).expect("rev dir");
        fs::write(
            build_root.join("rev-3/pages/page-a.svg"),
            b"<svg>page-a</svg>",
        )
        .expect("rev svg");
        let (events, _) = broadcast::channel(4);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events,
        });

        let response =
            revision_artifact(Path((3, "pages/page-a.svg".to_string())), State(state)).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(body.as_ref(), b"<svg>page-a</svg>");
    }

    #[test]
    fn png_route_suffix_helpers_accept_expected_segments() {
        assert_eq!(
            parse_png_path_suffix("page-a.png"),
            Some("page-a".to_string())
        );
        assert_eq!(parse_png_path_suffix("page-a"), None);
        assert_eq!(parse_png_u32_path_suffix("7.png"), Some(7));
        assert_eq!(parse_png_u32_path_suffix("oops.png"), None);
    }

    #[test]
    fn viewer_base_path_helpers_normalize_and_prefix_paths() {
        assert_eq!(normalize_viewer_base_path(None), "");
        assert_eq!(normalize_viewer_base_path(Some("")), "");
        assert_eq!(normalize_viewer_base_path(Some("/")), "");
        assert_eq!(normalize_viewer_base_path(Some("viewer")), "/viewer");
        assert_eq!(normalize_viewer_base_path(Some("/viewer/")), "/viewer");
        assert_eq!(
            viewer_prefixed_path_for("/viewer", "/api/state"),
            "/viewer/api/state"
        );
        assert_eq!(
            viewer_prefixed_path_for("/viewer", "api/state"),
            "/viewer/api/state"
        );
        assert_eq!(viewer_prefixed_path_for("", "/api/state"), "/api/state");
    }

    #[tokio::test]
    async fn build_router_accepts_artifact_png_routes_without_panicking() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let _router = build_router(state);
    }

    #[tokio::test]
    async fn build_router_with_base_serves_nested_api_and_redirects_root() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let router = build_router_with_base(state, "/viewer");

        let redirect = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .expect("root request"),
            )
            .await
            .expect("redirect response");
        assert_eq!(redirect.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            redirect
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok()),
            Some("/viewer/")
        );

        let nested_index = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/viewer/")
                    .body(Body::empty())
                    .expect("nested index request"),
            )
            .await
            .expect("nested index response");
        assert_eq!(nested_index.status(), StatusCode::OK);
        let nested_index_body = to_bytes(nested_index.into_body(), usize::MAX)
            .await
            .expect("nested index body");
        let nested_index_text =
            std::str::from_utf8(nested_index_body.as_ref()).expect("nested index utf8");
        assert!(
            nested_index_text.contains("latexd studio")
                || nested_index_text.contains("Viewer assets are missing")
        );

        let nested_state = router
            .oneshot(
                Request::builder()
                    .uri("/viewer/api/state")
                    .body(Body::empty())
                    .expect("nested state request"),
            )
            .await
            .expect("nested state response");
        assert_eq!(nested_state.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn build_router_preserves_page_pdf_and_svg_artifact_routes() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-3/pages")).expect("rev dir");
        fs::write(build_root.join("rev-3/pages/page-a.pdf"), b"%PDF-page-a").expect("rev pdf");
        fs::write(
            build_root.join("rev-3/pages/page-a.svg"),
            b"<svg>page-a</svg>",
        )
        .expect("rev svg");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let router = build_router(state);

        let svg_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/artifacts/rev/3/pages/page-a.svg")
                    .body(Body::empty())
                    .expect("svg request"),
            )
            .await
            .expect("svg response");
        assert_eq!(svg_response.status(), StatusCode::OK);
        let svg_body = to_bytes(svg_response.into_body(), usize::MAX)
            .await
            .expect("svg body");
        assert_eq!(svg_body.as_ref(), b"<svg>page-a</svg>");

        let pdf_response = router
            .oneshot(
                Request::builder()
                    .uri("/artifacts/rev/3/pages/page-a.pdf")
                    .body(Body::empty())
                    .expect("pdf request"),
            )
            .await
            .expect("pdf response");
        assert_eq!(pdf_response.status(), StatusCode::OK);
        let pdf_body = to_bytes(pdf_response.into_body(), usize::MAX)
            .await
            .expect("pdf body");
        assert_eq!(pdf_body.as_ref(), b"%PDF-page-a");
    }

    #[tokio::test]
    async fn revision_artifact_rejects_invalid_paths() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let (events, _) = broadcast::channel(4);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events,
        });

        let response = revision_artifact(Path((2, "../main.txt".to_string())), State(state)).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn revision_page_png_serves_png_for_saved_metadata() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-7")).expect("rev dir");
        fs::write(
            build_root.join("rev-7/page-metadata.json"),
            serde_json::to_vec(&vec![PageArtifactMeta {
                page_id: "page-a".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 512,
                height_pt: 640,
                content_hash: "hash-a".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
                pdf_artifact_path: Utf8PathBuf::from("rev-7/pages/page-a.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 10,
                }],
            }])
            .expect("serialize metadata"),
        )
        .expect("write page metadata");
        let (events, _) = broadcast::channel(4);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events,
        });

        let response = revision_page_png(
            Path((7, "page-a.png".to_string())),
            Query(RasterQuery {
                scale: Some(1.0),
                tile_size: None,
            }),
            State(state),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(&body[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[tokio::test]
    async fn revision_tile_png_uses_session_tile_render_path_without_full_page_render() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-17")).expect("rev dir");
        fs::write(
            build_root.join("rev-17/page-metadata.json"),
            serde_json::to_vec(&vec![PageArtifactMeta {
                page_id: "page-a".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 512,
                height_pt: 640,
                content_hash: "hash-a".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
                pdf_artifact_path: Utf8PathBuf::from("rev-17/pages/page-a.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 10,
                }],
            }])
            .expect("serialize metadata"),
        )
        .expect("write page metadata");
        let calls = Arc::new(AtomicUsize::new(0));
        let tile_calls = Arc::new(AtomicUsize::new(0));
        let sessions = Arc::new(AtomicUsize::new(0));
        let (events, _) = broadcast::channel(4);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::CountingMock {
                calls: calls.clone(),
                tile_calls: tile_calls.clone(),
                sessions: sessions.clone(),
                sleep_ms: 1,
            },
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events,
        });

        let response = revision_tile_png(
            Path((17, "page-a".to_string(), 100, 0, "0.png".to_string())),
            Query(RasterQuery {
                scale: Some(1.0),
                tile_size: Some(256),
            }),
            State(state.clone()),
        )
        .await;
        let second_response = revision_tile_png(
            Path((17, "page-a".to_string(), 100, 0, "0.png".to_string())),
            Query(RasterQuery {
                scale: Some(1.0),
                tile_size: Some(256),
            }),
            State(state.clone()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(&body[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(second_response.status(), StatusCode::OK);
        let second_body = to_bytes(second_response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(&second_body[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(tile_calls.load(Ordering::SeqCst), 1);
        assert_eq!(sessions.load(Ordering::SeqCst), 1);
        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.render_request_count, 0);
        assert_eq!(snapshot.tile_render_request_count, 2);
        assert_eq!(snapshot.fallback_render_count, 0);
        assert_eq!(snapshot.fallback_tile_render_count, 0);
        assert_eq!(snapshot.tile_cache_count, 1);
        assert_eq!(
            snapshot.tile_cache_entries,
            vec![RenderSessionTileCacheSnapshot {
                rev: 17,
                page_id: "page-a".to_string(),
                content_hash: "hash-a".to_string(),
                zoom_bucket: 100,
                rect_x: 0,
                rect_y: 0,
                rect_width: 256,
                rect_height: 256,
            }]
        );
        assert!(snapshot.recent_events.iter().any(|event| event.kind
            == RenderSessionEventKind::RenderTiles
            && event.rev == Some(17)
            && event.page_id.as_deref() == Some("page-a")
            && event.page_count == Some(1)
            && event.rendered_rect_count == Some(1)
            && event.reused_rect_count == Some(0)
            && event.duration_ms.is_some()));
        assert_eq!(
            snapshot
                .recent_events
                .iter()
                .filter(|event| {
                    matches!(
                        event.kind,
                        RenderSessionEventKind::RenderTiles | RenderSessionEventKind::ReuseTiles
                    ) && event.rev == Some(17)
                        && event.page_id.as_deref() == Some("page-a")
                })
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn renderer_session_tile_cache_only_renders_missing_rectangles_from_mixed_batches() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let calls = Arc::new(AtomicUsize::new(0));
        let tile_calls = Arc::new(AtomicUsize::new(0));
        let sessions = Arc::new(AtomicUsize::new(0));
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::CountingMock {
                calls: calls.clone(),
                tile_calls: tile_calls.clone(),
                sessions: sessions.clone(),
                sleep_ms: 1,
            },
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let page_metadata = PageArtifactMeta {
            page_id: "page-a".to_string(),
            index: 0,
            line_count: 1,
            width_pt: 256,
            height_pt: 256,
            content_hash: "hash-a".to_string(),
            text_start_utf8: 0,
            text_end_utf8: 10,
            pdf_artifact_path: Utf8PathBuf::from("rev-82/pages/page-a.pdf"),
            source_spans: vec![],
        };

        attach_render_revision(&state, 82, std::slice::from_ref(&page_metadata)).await;
        let page = load_revision_page_input(&state, 82, "page-a")
            .await
            .expect("page input");
        let rect_a = Rect {
            x: 0,
            y: 0,
            width: 128,
            height: 128,
        };
        let rect_b = Rect {
            x: 128,
            y: 0,
            width: 128,
            height: 128,
        };
        let rect_c = Rect {
            x: 0,
            y: 128,
            width: 128,
            height: 128,
        };

        let first =
            render_tiles_with_session(&state, &page, 1.0, vec![rect_a.clone(), rect_b.clone()])
                .await;
        let second =
            render_tiles_with_session(&state, &page, 1.0, vec![rect_a.clone(), rect_c.clone()])
                .await;

        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 2);
        assert_eq!(second[0].rect, rect_a);
        assert_eq!(second[1].rect, rect_c);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(tile_calls.load(Ordering::SeqCst), 2);
        assert_eq!(sessions.load(Ordering::SeqCst), 1);
        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.tile_render_request_count, 2);
        assert_eq!(snapshot.tile_cache_count, 3);
        assert_eq!(snapshot.warm_bucket_count, 1);
        let mut cached_rects = snapshot
            .tile_cache_entries
            .iter()
            .map(|tile| (tile.rect_x, tile.rect_y, tile.rect_width, tile.rect_height))
            .collect::<Vec<_>>();
        cached_rects.sort_unstable();
        assert_eq!(
            cached_rects,
            vec![(0, 0, 128, 128), (0, 128, 128, 128), (128, 0, 128, 128)]
        );
        let mut tile_events = snapshot
            .recent_events
            .iter()
            .filter(|event| {
                event.kind == RenderSessionEventKind::RenderTiles
                    && event.rev == Some(82)
                    && event.page_id.as_deref() == Some("page-a")
            })
            .collect::<Vec<_>>();
        tile_events.sort_by_key(|event| event.rendered_rect_count);
        assert_eq!(tile_events.len(), 2);
        assert_eq!(tile_events[0].page_count, Some(2));
        assert_eq!(tile_events[0].rendered_rect_count, Some(1));
        assert_eq!(tile_events[0].reused_rect_count, Some(1));
        assert_eq!(tile_events[1].page_count, Some(2));
        assert_eq!(tile_events[1].rendered_rect_count, Some(2));
        assert_eq!(tile_events[1].reused_rect_count, Some(0));
    }

    #[tokio::test]
    async fn renderer_session_limits_tile_cache_per_page_to_recent_rectangles() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let page_a = PageArtifactMeta {
            page_id: "page-a".to_string(),
            index: 0,
            line_count: 1,
            width_pt: 1024,
            height_pt: 1024,
            content_hash: "hash-a".to_string(),
            text_start_utf8: 0,
            text_end_utf8: 10,
            pdf_artifact_path: Utf8PathBuf::from("rev-83/pages/page-a.pdf"),
            source_spans: vec![],
        };
        let page_b = PageArtifactMeta {
            page_id: "page-b".to_string(),
            index: 1,
            line_count: 1,
            width_pt: 1024,
            height_pt: 1024,
            content_hash: "hash-b".to_string(),
            text_start_utf8: 0,
            text_end_utf8: 10,
            pdf_artifact_path: Utf8PathBuf::from("rev-83/pages/page-b.pdf"),
            source_spans: vec![],
        };

        attach_render_revision(&state, 83, &[page_a.clone(), page_b.clone()]).await;
        let page_a_input = load_revision_page_input(&state, 83, "page-a")
            .await
            .expect("page a input");
        let page_b_input = load_revision_page_input(&state, 83, "page-b")
            .await
            .expect("page b input");

        let stable_rect = Rect {
            x: 900,
            y: 0,
            width: 32,
            height: 32,
        };
        render_tiles_with_session(&state, &page_b_input, 1.0, vec![stable_rect.clone()]).await;

        for index in 0..(RENDER_SESSION_TILE_CACHE_PAGE_BUDGET as u32 + 1) {
            let rect = Rect {
                x: index,
                y: 0,
                width: 1,
                height: 1,
            };
            render_tiles_with_session(&state, &page_a_input, 1.0, vec![rect]).await;
        }

        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(
            snapshot.tile_cache_page_budget,
            RENDER_SESSION_TILE_CACHE_PAGE_BUDGET
        );
        assert_eq!(
            snapshot.tile_cache_count,
            RENDER_SESSION_TILE_CACHE_PAGE_BUDGET + 1
        );
        assert_eq!(snapshot.tile_cache_evict_count, 1);
        let page_a_rects = snapshot
            .tile_cache_entries
            .iter()
            .filter(|entry| entry.page_id == "page-a")
            .map(|entry| entry.rect_x)
            .collect::<Vec<_>>();
        assert_eq!(page_a_rects.len(), RENDER_SESSION_TILE_CACHE_PAGE_BUDGET);
        assert_eq!(page_a_rects.first().copied(), Some(1));
        assert_eq!(
            page_a_rects.last().copied(),
            Some(RENDER_SESSION_TILE_CACHE_PAGE_BUDGET as u32)
        );
        assert!(snapshot.tile_cache_entries.iter().any(|entry| {
            entry.page_id == "page-b"
                && entry.rect_x == stable_rect.x
                && entry.rect_y == stable_rect.y
                && entry.rect_width == stable_rect.width
                && entry.rect_height == stable_rect.height
        }));
        assert!(snapshot.recent_events.iter().any(|event| {
            event.kind == RenderSessionEventKind::EvictTileCache
                && event.rev == Some(83)
                && event.page_id.as_deref() == Some("page-a")
                && event.scale_percent == Some(100)
        }));
    }

    #[tokio::test]
    async fn renderer_session_detach_clears_cached_tiles_for_removed_revision() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });
        let page_metadata = PageArtifactMeta {
            page_id: "page-a".to_string(),
            index: 0,
            line_count: 1,
            width_pt: 256,
            height_pt: 256,
            content_hash: "hash-a".to_string(),
            text_start_utf8: 0,
            text_end_utf8: 10,
            pdf_artifact_path: Utf8PathBuf::from("rev-81/pages/page-a.pdf"),
            source_spans: vec![],
        };

        attach_render_revision(&state, 81, std::slice::from_ref(&page_metadata)).await;
        let page = load_revision_page_input(&state, 81, "page-a")
            .await
            .expect("page input");
        let handle = render_session_handle(&state);
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .tx
            .send(RenderSessionRequest::RenderTiles {
                page,
                scale: 1.0,
                rects: vec![Rect {
                    x: 0,
                    y: 0,
                    width: 128,
                    height: 128,
                }],
                reply: reply_tx,
            })
            .await
            .expect("send tile render");
        reply_rx
            .await
            .expect("tile render reply")
            .expect("tile render result");

        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.tile_cache_count, 1);
        assert_eq!(snapshot.tile_cache_entries.len(), 1);
        detach_render_revision(&state, 81).await;
        let snapshot = render_session_metrics_snapshot(&state).await;
        assert_eq!(snapshot.tile_cache_count, 0);
        assert!(snapshot.tile_cache_entries.is_empty());
    }

    #[tokio::test]
    async fn revision_page_png_can_render_with_gs_cli_from_reused_page_pdf_path() {
        let Ok(program) = which::which("gs") else {
            return;
        };
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-6/pages")).expect("rev dir");
        fs::create_dir_all(build_root.join("rev-7")).expect("metadata dir");
        let stream = "BT /F1 12 Tf 18 36 Td (latexd gs route smoke) Tj ET";
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.4\n");
        let objects = [
            "1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".to_string(),
            "2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n".to_string(),
            "3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 144 72] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >> endobj\n".to_string(),
            "4 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n"
                .to_string(),
            format!(
                "5 0 obj << /Length {} >> stream\n{}\nendstream\nendobj\n",
                stream.len(),
                stream
            ),
        ];
        let mut offsets = vec![0usize];
        for object in &objects {
            offsets.push(pdf.len());
            pdf.extend_from_slice(object.as_bytes());
        }
        let xref_offset = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                objects.len() + 1,
                xref_offset
            )
            .as_bytes(),
        );
        fs::write(build_root.join("rev-6/pages/page-a.pdf"), pdf).expect("page pdf");
        fs::write(
            build_root.join("rev-7/page-metadata.json"),
            serde_json::to_vec(&vec![PageArtifactMeta {
                page_id: "page-a".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 144,
                height_pt: 72,
                content_hash: "hash-a".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
                pdf_artifact_path: Utf8PathBuf::from("rev-6/pages/page-a.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 10,
                }],
            }])
            .expect("serialize metadata"),
        )
        .expect("write page metadata");
        let (events, _) = broadcast::channel(4);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::GsCli {
                program: program.to_string_lossy().to_string(),
            },
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events,
        });

        let response = revision_page_png(
            Path((7, "page-a.png".to_string())),
            Query(RasterQuery {
                scale: Some(1.0),
                tile_size: None,
            }),
            State(state),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(&body[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[tokio::test]
    async fn revision_page_png_can_render_with_gs_api_from_reused_page_pdf_path() {
        let Some(library_path) = probe_gsapi_library(None) else {
            return;
        };
        let runtime = Arc::new(
            GsApiRuntime::new(Some(library_path.to_string_lossy().as_ref())).expect("runtime"),
        );
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-8/pages")).expect("rev dir");
        fs::create_dir_all(build_root.join("rev-9")).expect("metadata dir");
        let stream = "BT /F1 12 Tf 18 36 Td (latexd gs api route smoke) Tj ET";
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.4\n");
        let objects = [
            "1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".to_string(),
            "2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n".to_string(),
            "3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 144 72] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >> endobj\n".to_string(),
            "4 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n"
                .to_string(),
            format!(
                "5 0 obj << /Length {} >> stream\n{}\nendstream\nendobj\n",
                stream.len(),
                stream
            ),
        ];
        let mut offsets = vec![0usize];
        for object in &objects {
            offsets.push(pdf.len());
            pdf.extend_from_slice(object.as_bytes());
        }
        let xref_offset = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                objects.len() + 1,
                xref_offset
            )
            .as_bytes(),
        );
        fs::write(build_root.join("rev-8/pages/page-a.pdf"), pdf).expect("page pdf");
        fs::write(
            build_root.join("rev-9/page-metadata.json"),
            serde_json::to_vec(&vec![PageArtifactMeta {
                page_id: "page-a".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 144,
                height_pt: 72,
                content_hash: "hash-a".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
                pdf_artifact_path: Utf8PathBuf::from("rev-8/pages/page-a.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 10,
                }],
            }])
            .expect("serialize metadata"),
        )
        .expect("write page metadata");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::GsApi {
                library_path: library_path.to_string_lossy().to_string(),
                runtime: Some(runtime),
                runtime_pool: None,
            },
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = revision_page_png(
            Path((9, "page-a.png".to_string())),
            Query(RasterQuery {
                scale: Some(1.0),
                tile_size: None,
            }),
            State(state),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        assert_eq!(&body[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[tokio::test]
    async fn required_tiles_endpoint_returns_tile_manifest() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-9")).expect("rev dir");
        fs::write(
            build_root.join("rev-9/page-metadata.json"),
            serde_json::to_vec(&vec![PageArtifactMeta {
                page_id: "page-a".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 512,
                height_pt: 640,
                content_hash: "hash-a".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
                pdf_artifact_path: Utf8PathBuf::from("rev-9/pages/page-a.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 10,
                }],
            }])
            .expect("serialize metadata"),
        )
        .expect("write page metadata");
        let (events, _) = broadcast::channel(4);
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events,
        });

        let response = required_tiles(
            Path((9, "page-a".to_string())),
            Query(RequiredTilesQuery {
                scale: Some(1.0),
                left: 0,
                top: 0,
                width: 300,
                height: 300,
                tile_size: Some(256),
            }),
            State(state),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let manifest: TileManifestResponse =
            serde_json::from_slice(&body).expect("decode manifest");

        assert_eq!(manifest.items.len(), 4);
        assert_eq!(
            manifest.items[0].png_url,
            "/artifacts/rev/9/tiles/page-a/100/0/0.png"
        );
    }

    #[tokio::test]
    async fn page_syncmap_endpoint_returns_ordered_bands_for_source_spans() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-10")).expect("rev dir");
        fs::write(
            build_root.join("rev-10/sources.json"),
            serde_json::json!({
                "files": {
                    "main.tex": "ab\ncd\nef\n",
                    "parent.tex": "zero\none\ntwo\nthree\n",
                    "child.tex": "left\nright\n"
                }
            })
            .to_string(),
        )
        .expect("write sources");
        fs::write(
            build_root.join("rev-10/page-metadata.json"),
            serde_json::to_vec(&vec![PageArtifactMeta {
                page_id: "page-a".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 512,
                height_pt: 600,
                content_hash: "hash-a".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 32,
                pdf_artifact_path: Utf8PathBuf::from("rev-10/pages/page-a.pdf"),
                source_spans: vec![
                    ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 4,
                    },
                    ArtifactSourceSpan {
                        file: Utf8PathBuf::from("parent.tex"),
                        start_utf8: 10,
                        end_utf8: 18,
                    },
                    ArtifactSourceSpan {
                        file: Utf8PathBuf::from("child.tex"),
                        start_utf8: 2,
                        end_utf8: 6,
                    },
                ],
            }])
            .expect("serialize metadata"),
        )
        .expect("write page metadata");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = page_syncmap(Path((10, "page-a".to_string())), State(state)).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let syncmap: PageSyncMapResponse = serde_json::from_slice(&body).expect("decode syncmap");

        assert_eq!(syncmap.rev, 10);
        assert_eq!(syncmap.page_id, "page-a");
        assert_eq!(syncmap.page_width_px, 512);
        assert_eq!(syncmap.page_height_px, 600);
        assert_eq!(syncmap.page_source_start_utf8, 0);
        assert_eq!(syncmap.page_source_end_utf8, 18);
        assert_eq!(syncmap.page_output_start_utf8, 0);
        assert_eq!(syncmap.page_output_end_utf8, 32);
        assert_eq!(syncmap.items.len(), 3);
        assert_eq!(
            syncmap
                .items
                .iter()
                .map(|item| item.file.as_str())
                .collect::<Vec<_>>(),
            vec!["main.tex", "parent.tex", "child.tex"]
        );
        assert_eq!(syncmap.items[0].top_px, 0);
        assert_eq!(syncmap.items[0].item_id, "page-a:main.tex:0:4:1:2");
        assert_eq!(syncmap.items[0].left_px, 0);
        assert_eq!(syncmap.items[0].right_px, 512);
        assert_eq!(syncmap.items[0].output_start_utf8, 0);
        assert_eq!(syncmap.items[0].output_end_utf8, 8);
        assert_eq!(syncmap.items[0].start_line, 1);
        assert_eq!(syncmap.items[0].end_line, 2);
        assert_eq!(syncmap.items[1].output_start_utf8, 8);
        assert_eq!(syncmap.items[1].output_end_utf8, 24);
        assert_eq!(syncmap.items[1].start_line, 3);
        assert_eq!(syncmap.items[1].end_line, 4);
        assert_eq!(syncmap.items[2].output_start_utf8, 24);
        assert_eq!(syncmap.items[2].output_end_utf8, 32);
        assert_eq!(syncmap.items[2].start_line, 1);
        assert_eq!(syncmap.items[2].end_line, 2);
        assert!(syncmap.items[0].bottom_px < syncmap.items[1].bottom_px);
        assert!(syncmap.items[1].top_px <= syncmap.items[1].bottom_px);
        assert_eq!(syncmap.items[2].bottom_px, 600);
    }

    #[tokio::test]
    async fn page_syncmap_endpoint_prefers_output_range_artifacts_when_present() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-10")).expect("rev dir");
        fs::write(
            build_root.join("rev-10/sources.json"),
            serde_json::json!({
                "files": {
                    "main.tex": "alpha\nbeta\ngamma\n"
                }
            })
            .to_string(),
        )
        .expect("write sources");
        fs::write(
            build_root.join("rev-10/page-syncmap.json"),
            serde_json::to_vec(&vec![PageSyncMapArtifact {
                page_id: "page-a".to_string(),
                index: 0,
                width_pt: 512,
                height_pt: 600,
                items: vec![
                    ArtifactSyncSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 5,
                        output_start_utf8: 0,
                        output_end_utf8: 20,
                        left_px: 72,
                        right_px: 120,
                        top_px: 40,
                        bottom_px: 54,
                    },
                    ArtifactSyncSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 6,
                        end_utf8: 16,
                        output_start_utf8: 80,
                        output_end_utf8: 100,
                        left_px: 180,
                        right_px: 264,
                        top_px: 68,
                        bottom_px: 82,
                    },
                ],
            }])
            .expect("serialize syncmap"),
        )
        .expect("write page syncmap");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = page_syncmap(Path((10, "page-a".to_string())), State(state)).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let syncmap: PageSyncMapResponse = serde_json::from_slice(&body).expect("decode syncmap");

        assert_eq!(syncmap.items.len(), 2);
        assert_eq!(syncmap.page_width_px, 512);
        assert_eq!(syncmap.page_source_start_utf8, 0);
        assert_eq!(syncmap.page_source_end_utf8, 16);
        assert_eq!(syncmap.page_output_start_utf8, 0);
        assert_eq!(syncmap.page_output_end_utf8, 100);
        assert_eq!(syncmap.items[0].output_start_utf8, 0);
        assert_eq!(syncmap.items[0].output_end_utf8, 20);
        assert_eq!(syncmap.items[0].item_id, "page-a:main.tex:0:5:1:1");
        assert_eq!(syncmap.items[0].left_px, 72);
        assert_eq!(syncmap.items[0].right_px, 120);
        assert_eq!(syncmap.items[0].top_px, 40);
        assert_eq!(syncmap.items[0].bottom_px, 54);
        assert_eq!(syncmap.items[1].output_start_utf8, 80);
        assert_eq!(syncmap.items[1].output_end_utf8, 100);
        assert_eq!(syncmap.items[1].item_id, "page-a:main.tex:6:16:2:3");
        assert_eq!(syncmap.items[1].left_px, 180);
        assert_eq!(syncmap.items[1].right_px, 264);
        assert_eq!(syncmap.items[1].top_px, 68);
        assert_eq!(syncmap.items[1].bottom_px, 82);
        assert_eq!(syncmap.items[0].start_line, 1);
        assert_eq!(syncmap.items[1].start_line, 2);
    }

    #[tokio::test]
    async fn page_syncmap_endpoint_normalizes_invalid_artifact_box_geometry() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-10")).expect("rev dir");
        fs::write(
            build_root.join("rev-10/sources.json"),
            serde_json::json!({
                "files": {
                    "main.tex": "alpha\nbeta\ngamma\n"
                }
            })
            .to_string(),
        )
        .expect("write sources");
        fs::write(
            build_root.join("rev-10/page-syncmap.json"),
            serde_json::to_vec(&vec![PageSyncMapArtifact {
                page_id: "page-a".to_string(),
                index: 0,
                width_pt: 512,
                height_pt: 600,
                items: vec![ArtifactSyncSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 6,
                    end_utf8: 16,
                    output_start_utf8: 80,
                    output_end_utf8: 100,
                    left_px: 520,
                    right_px: 500,
                    top_px: 620,
                    bottom_px: 610,
                }],
            }])
            .expect("serialize syncmap"),
        )
        .expect("write page syncmap");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = page_syncmap(Path((10, "page-a".to_string())), State(state)).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let syncmap: PageSyncMapResponse = serde_json::from_slice(&body).expect("decode syncmap");

        assert_eq!(syncmap.items.len(), 1);
        assert_eq!(syncmap.items[0].item_id, "page-a:main.tex:6:16:2:3");
        assert_eq!(syncmap.items[0].output_start_utf8, 80);
        assert_eq!(syncmap.items[0].output_end_utf8, 100);
        assert_eq!(syncmap.items[0].left_px, 0);
        assert_eq!(syncmap.items[0].right_px, 512);
        assert_eq!(syncmap.items[0].top_px, 480);
        assert_eq!(syncmap.items[0].bottom_px, 600);
    }

    #[tokio::test]
    async fn source_jump_endpoint_returns_nearest_page_band_for_source_offset() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-11")).expect("rev dir");
        fs::write(
            build_root.join("rev-11/sources.json"),
            serde_json::json!({
                "files": {
                    "main.tex": "lead\nbody\ntail\n"
                }
            })
            .to_string(),
        )
        .expect("write sources");
        fs::write(
            build_root.join("rev-11/page-metadata.json"),
            serde_json::to_vec(&vec![
                PageArtifactMeta {
                    page_id: "page-a".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 512,
                    height_pt: 600,
                    content_hash: "hash-a".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 32,
                    pdf_artifact_path: Utf8PathBuf::from("rev-11/pages/page-a.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 4,
                    }],
                },
                PageArtifactMeta {
                    page_id: "page-b".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 512,
                    height_pt: 600,
                    content_hash: "hash-b".to_string(),
                    text_start_utf8: 32,
                    text_end_utf8: 64,
                    pdf_artifact_path: Utf8PathBuf::from("rev-11/pages/page-b.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 5,
                        end_utf8: 14,
                    }],
                },
            ])
            .expect("serialize metadata"),
        )
        .expect("write page metadata");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = source_jump(
            Path(11),
            Query(SourceJumpQuery {
                file: "main.tex".to_string(),
                offset: Some(8),
                line: None,
                column: None,
                source_hash: None,
            }),
            State(state),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let jump: SourceJumpResponse = serde_json::from_slice(&body).expect("decode source jump");

        assert_eq!(jump.line, 2);
        assert_eq!(jump.line0, 1);
        assert_eq!(jump.column, 4);
        assert_eq!(jump.column0, 3);
        assert_eq!(jump.absolute_file, root.join("main.tex"));
        assert_eq!(jump.file_uri, source_file_uri(&root.join("main.tex")));
        assert_eq!(jump.editor_uri, "");
        assert_eq!(jump.editor_preview_kind, EditorPreviewKind::None);
        assert_eq!(jump.page_id, "page-b");
        assert_eq!(jump.page_index, 1);
        assert_eq!(jump.source_hash, "#src=main.tex&line=2&column=4");
        assert_eq!(jump.editor_cwd, root.clone());
        assert!(!jump.editor_launch_supported);
        assert_eq!(jump.editor_program, "");
        assert!(jump.editor_args.is_empty());
        assert_eq!(jump.editor_command_line, "");
        assert_eq!(jump.page_source_start_utf8, 5);
        assert_eq!(jump.page_source_end_utf8, 14);
        assert_eq!(jump.page_output_start_utf8, 32);
        assert_eq!(jump.page_output_end_utf8, 64);
        assert_eq!(jump.item.item_id, "page-b:main.tex:5:14:2:3");
        assert_eq!(jump.item.file, Utf8PathBuf::from("main.tex"));
        assert_eq!(jump.item.output_start_utf8, 32);
        assert_eq!(jump.item.output_end_utf8, 64);
        assert_eq!(jump.item.start_line, 2);
        assert_eq!(jump.item.end_line, 3);
    }

    #[tokio::test]
    async fn source_jump_endpoint_accepts_line_and_column_lookup_when_offset_is_missing() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-12")).expect("rev dir");
        fs::write(
            build_root.join("rev-12/sources.json"),
            serde_json::json!({
                "files": {
                    "main.tex": "lead\nbody\ntail\n"
                }
            })
            .to_string(),
        )
        .expect("write sources");
        fs::write(
            build_root.join("rev-12/page-metadata.json"),
            serde_json::to_vec(&vec![
                PageArtifactMeta {
                    page_id: "page-a".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 512,
                    height_pt: 600,
                    content_hash: "hash-a".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 32,
                    pdf_artifact_path: Utf8PathBuf::from("rev-12/pages/page-a.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 4,
                    }],
                },
                PageArtifactMeta {
                    page_id: "page-b".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 512,
                    height_pt: 600,
                    content_hash: "hash-b".to_string(),
                    text_start_utf8: 32,
                    text_end_utf8: 64,
                    pdf_artifact_path: Utf8PathBuf::from("rev-12/pages/page-b.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 5,
                        end_utf8: 14,
                    }],
                },
            ])
            .expect("serialize metadata"),
        )
        .expect("write page metadata");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: Some(EditorBridgeConfig {
                program: "/usr/bin/code".to_string(),
                args: vec![
                    "{absolute_file}".to_string(),
                    "{line}".to_string(),
                    "{column}".to_string(),
                    "{page_id}".to_string(),
                    "{item_id}".to_string(),
                    "{editor_uri}".to_string(),
                ],
            }),
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = source_jump(
            Path(12),
            Query(SourceJumpQuery {
                file: "main.tex".to_string(),
                offset: None,
                line: Some(3),
                column: Some(2),
                source_hash: None,
            }),
            State(state),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let jump: SourceJumpResponse = serde_json::from_slice(&body).expect("decode source jump");

        assert_eq!(jump.offset_utf8, 11);
        assert_eq!(jump.line, 3);
        assert_eq!(jump.line0, 2);
        assert_eq!(jump.column, 2);
        assert_eq!(jump.column0, 1);
        assert_eq!(jump.absolute_file, root.join("main.tex"));
        assert_eq!(jump.file_uri, source_file_uri(&root.join("main.tex")));
        assert_eq!(
            jump.editor_uri,
            format!("vscode://file{}:3:2", root.join("main.tex").as_str())
        );
        assert_eq!(jump.editor_preview_kind, EditorPreviewKind::CommandAndUri);
        assert_eq!(jump.source_hash, "#src=main.tex&line=3&column=2");
        assert_eq!(jump.editor_cwd, root.clone());
        assert!(jump.editor_launch_supported);
        assert_eq!(jump.editor_program, "/usr/bin/code");
        assert_eq!(
            jump.editor_args,
            vec![
                root.join("main.tex").as_str().to_string(),
                "3".to_string(),
                "2".to_string(),
                "page-b".to_string(),
                "page-b:main.tex:5:14:2:3".to_string(),
                format!("vscode://file{}:3:2", root.join("main.tex").as_str()),
            ]
        );
        assert_eq!(
            jump.editor_command_line,
            format!(
                "/usr/bin/code {} 3 2 page-b page-b:main.tex:5:14:2:3 {}",
                root.join("main.tex").as_str(),
                format!("vscode://file{}:3:2", root.join("main.tex").as_str()),
            )
        );
        assert_eq!(jump.page_id, "page-b");
        assert_eq!(jump.page_source_start_utf8, 5);
        assert_eq!(jump.page_source_end_utf8, 14);
        assert_eq!(jump.page_output_start_utf8, 32);
        assert_eq!(jump.page_output_end_utf8, 64);
        assert_eq!(jump.item.item_id, "page-b:main.tex:5:14:2:3");
        assert_eq!(jump.item.output_start_utf8, 32);
        assert_eq!(jump.item.output_end_utf8, 64);
        assert_eq!(jump.item.start_line, 2);
        assert_eq!(jump.item.end_line, 3);
    }

    #[tokio::test]
    async fn source_file_endpoint_returns_revision_source_text() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-13")).expect("rev dir");
        fs::write(
            build_root.join("rev-13/sources.json"),
            serde_json::json!({
                "files": {
                    "main.tex": "lead\nbody\ntail\n"
                }
            })
            .to_string(),
        )
        .expect("write sources");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = source_file(
            Path(13),
            Query(SourceFileQuery {
                file: "main.tex".to_string(),
            }),
            State(state),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let file: SourceFileResponse = serde_json::from_slice(&body).expect("decode source file");
        assert_eq!(file.rev, 13);
        assert_eq!(file.file, Utf8PathBuf::from("main.tex"));
        assert_eq!(file.content, "lead\nbody\ntail\n");
        assert_eq!(file.line_count, 4);
    }

    #[tokio::test]
    async fn source_files_endpoint_returns_revision_source_file_list() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-13")).expect("rev dir");
        fs::write(
            build_root.join("rev-13/sources.json"),
            serde_json::json!({
                "files": {
                    "main.tex": "lead\nbody\ntail\n",
                    "sections/intro.tex": "nested\n"
                }
            })
            .to_string(),
        )
        .expect("write sources");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = source_files(Path(13), State(state)).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: SourceFilesResponse =
            serde_json::from_slice(&body).expect("decode source files");
        assert_eq!(payload.rev, 13);
        assert_eq!(
            payload.files,
            vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/intro.tex")
            ]
        );
    }

    #[tokio::test]
    async fn snapshot_endpoint_includes_live_workspace_source_snapshot_before_first_revision() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n  - appendix.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        fs::write(root.join("appendix.tex"), "appendix\n").expect("appendix tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let Json(payload) = snapshot(State(state)).await;

        assert_eq!(payload.last_applied_rev, 0);
        assert_eq!(
            payload.source_snapshot,
            vec![
                SourceSnapshotFile {
                    file: "main.tex".to_string(),
                    content: "hello".to_string(),
                    line_count: 1,
                },
                SourceSnapshotFile {
                    file: "appendix.tex".to_string(),
                    content: "appendix\n".to_string(),
                    line_count: 2,
                },
            ]
        );
    }

    #[tokio::test]
    async fn snapshot_endpoint_prefers_revision_source_snapshot_for_last_applied_revision() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "workspace").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-13")).expect("rev dir");
        fs::write(
            build_root.join("rev-13/sources.json"),
            serde_json::json!({
                "files": {
                    "main.tex": "revision\nbody\n"
                }
            })
            .to_string(),
        )
        .expect("write sources");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState {
                snapshot: PreviewSnapshot {
                    current_rev: 13,
                    last_applied_rev: 13,
                    ..PreviewSnapshot::default()
                },
                ..LivePreviewState::default()
            }),
            events: broadcast::channel(4).0,
        });

        let Json(payload) = snapshot(State(state)).await;

        assert_eq!(payload.last_applied_rev, 13);
        assert_eq!(
            payload.source_snapshot,
            vec![SourceSnapshotFile {
                file: "main.tex".to_string(),
                content: "revision\nbody\n".to_string(),
                line_count: 3,
            }]
        );
    }

    #[tokio::test]
    async fn source_files_endpoint_falls_back_to_manifest_toplevels_before_first_revision() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n  - appendix.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        fs::write(root.join("appendix.tex"), "appendix").expect("appendix tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = source_files(Path(0), State(state)).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: SourceFilesResponse =
            serde_json::from_slice(&body).expect("decode source files");
        assert_eq!(payload.rev, 0);
        assert_eq!(
            payload.files,
            vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("appendix.tex")
            ]
        );
    }

    #[tokio::test]
    async fn source_file_endpoint_falls_back_to_live_workspace_text_when_snapshot_is_missing() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "alpha\nbeta\n").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = source_file(
            Path(0),
            Query(SourceFileQuery {
                file: "main.tex".to_string(),
            }),
            State(state),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let file: SourceFileResponse = serde_json::from_slice(&body).expect("decode source file");
        assert_eq!(file.rev, 0);
        assert_eq!(file.file, Utf8PathBuf::from("main.tex"));
        assert_eq!(file.content, "alpha\nbeta\n");
        assert_eq!(file.line_count, 3);
    }

    #[tokio::test]
    async fn update_source_file_endpoint_writes_workspace_source_text() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "alpha\nbeta\n").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = update_source_file(
            State(state),
            Json(UpdateSourceFileRequest {
                file: "main.tex".to_string(),
                content: "alpha\nbeta\ngamma\n".to_string(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: UpdateSourceFileResponse =
            serde_json::from_slice(&body).expect("decode updated source response");
        assert_eq!(payload.file, Utf8PathBuf::from("main.tex"));
        assert_eq!(payload.line_count, 4);
        assert_eq!(payload.byte_len, 17);
        assert_eq!(
            fs::read_to_string(root.join("main.tex")).expect("read updated source"),
            "alpha\nbeta\ngamma\n"
        );
    }

    #[tokio::test]
    async fn update_source_file_endpoint_rejects_invalid_paths() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "alpha\nbeta\n").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = update_source_file(
            State(state),
            Json(UpdateSourceFileRequest {
                file: "../escape.tex".to_string(),
                content: "nope".to_string(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn open_source_endpoint_launches_editor_bridge_command() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "alpha\nbeta\ngamma\n").expect("main tex");
        let output_path = root.join("editor-call.txt");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: Some(EditorBridgeConfig {
                program: "/bin/sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    format!(
                        "printf '%s\\n' \"$PWD\" \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" > '{}'",
                        output_path.as_str()
                    ),
                    "latexd-open".to_string(),
                    "{abs_file}".to_string(),
                    "{line}".to_string(),
                    "{offset}".to_string(),
                    "{rev}".to_string(),
                    "{source_hash}".to_string(),
                ],
            }),
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = open_source(
            Path(14),
            State(state),
            Json(OpenSourceRequest {
                file: "main.tex".to_string(),
                offset: None,
                line: Some(3),
                column: None,
                source_hash: None,
                launch: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let opened: OpenSourceResponse =
            serde_json::from_slice(&body).expect("decode open source response");
        assert_eq!(opened.file, Utf8PathBuf::from("main.tex"));
        assert_eq!(opened.absolute_file, root.join("main.tex"));
        assert_eq!(opened.file_uri, source_file_uri(&root.join("main.tex")));
        assert_eq!(opened.editor_uri, "");
        assert_eq!(opened.editor_preview_kind, EditorPreviewKind::Command);
        assert_eq!(opened.line, 3);
        assert_eq!(opened.line0, 2);
        assert_eq!(opened.column, 1);
        assert_eq!(opened.column0, 0);
        assert_eq!(opened.offset_utf8, 11);
        assert_eq!(opened.source_hash, "#src=main.tex&line=3");
        assert_eq!(opened.editor_cwd, root.clone());
        assert!(opened.editor_launch_supported);
        assert_eq!(opened.editor_program, "/bin/sh");
        assert_eq!(
            opened.editor_args,
            vec![
                "-c".to_string(),
                format!(
                    "printf '%s\\n' \"$PWD\" \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" > '{}'",
                    output_path.as_str()
                ),
                "latexd-open".to_string(),
                root.join("main.tex").as_str().to_string(),
                "3".to_string(),
                "11".to_string(),
                "14".to_string(),
                "#src=main.tex&line=3".to_string(),
            ]
        );
        assert_eq!(
            opened.editor_command_line,
            format!(
                "/bin/sh -c 'printf '\"'\"'%s\\n'\"'\"' \"$PWD\" \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" > '\"'\"'{}'\"'\"'' latexd-open {} 3 11 14 '#src=main.tex&line=3'",
                output_path.as_str(),
                root.join("main.tex").as_str(),
            )
        );
        assert_eq!(opened.page_id, None);
        assert_eq!(opened.page_index, None);
        assert_eq!(opened.page_width_px, None);
        assert_eq!(opened.page_height_px, None);
        assert_eq!(opened.page_source_start_utf8, None);
        assert_eq!(opened.page_source_end_utf8, None);
        assert_eq!(opened.page_output_start_utf8, None);
        assert_eq!(opened.page_output_end_utf8, None);
        assert_eq!(opened.item, None);

        let lines = timeout(Duration::from_secs(2), async {
            loop {
                if output_path.exists() {
                    let captured = fs::read_to_string(&output_path).expect("captured args");
                    let lines = captured.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
                    if lines.len() >= 6 {
                        break lines;
                    }
                }
                sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("editor bridge execution");
        assert_eq!(lines[0], root.as_str());
        assert_eq!(lines[1], root.join("main.tex").as_str());
        assert_eq!(lines[2], "3");
        assert_eq!(lines[3], "11");
        assert_eq!(lines[4], "14");
        assert_eq!(lines[5], "#src=main.tex&line=3");
    }

    #[tokio::test]
    async fn open_source_endpoint_can_preview_editor_bridge_command_without_launching() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "alpha\nbeta\ngamma\n").expect("main tex");
        let output_path = root.join("editor-call.txt");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: Some(EditorBridgeConfig {
                program: "/bin/sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    format!(
                        "printf '%s\\n' \"$PWD\" \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" > '{}'",
                        output_path.as_str()
                    ),
                    "latexd-open".to_string(),
                    "{abs_file}".to_string(),
                    "{line}".to_string(),
                    "{offset}".to_string(),
                    "{rev}".to_string(),
                    "{source_hash}".to_string(),
                ],
            }),
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = open_source(
            Path(14),
            State(state),
            Json(OpenSourceRequest {
                file: "main.tex".to_string(),
                offset: None,
                line: Some(3),
                column: None,
                source_hash: None,
                launch: Some(false),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let opened: OpenSourceResponse =
            serde_json::from_slice(&body).expect("decode open source response");
        assert_eq!(opened.source_hash, "#src=main.tex&line=3");
        assert_eq!(opened.absolute_file, root.join("main.tex"));
        assert_eq!(opened.file_uri, source_file_uri(&root.join("main.tex")));
        assert_eq!(opened.editor_uri, "");
        assert_eq!(opened.editor_preview_kind, EditorPreviewKind::Command);
        assert_eq!(opened.line0, 2);
        assert_eq!(opened.column, 1);
        assert_eq!(opened.column0, 0);
        assert_eq!(opened.editor_cwd, root.clone());
        assert!(opened.editor_launch_supported);
        assert_eq!(opened.editor_program, "/bin/sh");
        assert_eq!(
            opened.editor_args,
            vec![
                "-c".to_string(),
                format!(
                    "printf '%s\\n' \"$PWD\" \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" > '{}'",
                    output_path.as_str()
                ),
                "latexd-open".to_string(),
                root.join("main.tex").as_str().to_string(),
                "3".to_string(),
                "11".to_string(),
                "14".to_string(),
                "#src=main.tex&line=3".to_string(),
            ]
        );
        assert_eq!(
            opened.editor_command_line,
            format!(
                "/bin/sh -c 'printf '\"'\"'%s\\n'\"'\"' \"$PWD\" \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" > '\"'\"'{}'\"'\"'' latexd-open {} 3 11 14 '#src=main.tex&line=3'",
                output_path.as_str(),
                root.join("main.tex").as_str(),
            )
        );
        assert!(!opened.launched);
        sleep(Duration::from_millis(200)).await;
        assert!(
            !output_path.exists(),
            "preview-only open_source should not spawn editor"
        );
    }

    #[tokio::test]
    async fn open_source_endpoint_can_preview_without_configured_editor_bridge() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "alpha\nbeta\ngamma\n").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = open_source(
            Path(14),
            State(state),
            Json(OpenSourceRequest {
                file: "main.tex".to_string(),
                offset: None,
                line: Some(3),
                column: Some(2),
                source_hash: None,
                launch: Some(false),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let opened: OpenSourceResponse =
            serde_json::from_slice(&body).expect("decode open source response");
        assert_eq!(opened.file, Utf8PathBuf::from("main.tex"));
        assert_eq!(opened.absolute_file, root.join("main.tex"));
        assert_eq!(opened.file_uri, source_file_uri(&root.join("main.tex")));
        assert_eq!(opened.editor_uri, "");
        assert_eq!(opened.line, 3);
        assert_eq!(opened.line0, 2);
        assert_eq!(opened.column, 2);
        assert_eq!(opened.column0, 1);
        assert_eq!(opened.offset_utf8, 12);
        assert_eq!(opened.source_hash, "#src=main.tex&line=3&column=2");
        assert_eq!(opened.editor_cwd, root.clone());
        assert!(!opened.editor_launch_supported);
        assert_eq!(opened.editor_program, "");
        assert!(opened.editor_args.is_empty());
        assert_eq!(opened.editor_command_line, "");
        assert!(!opened.launched);
    }

    #[tokio::test]
    async fn open_source_endpoint_without_configured_editor_bridge_degrades_to_preview() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "alpha\nbeta\ngamma\n").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = open_source(
            Path(14),
            State(state),
            Json(OpenSourceRequest {
                file: "main.tex".to_string(),
                offset: None,
                line: Some(3),
                column: Some(2),
                source_hash: None,
                launch: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let opened: OpenSourceResponse =
            serde_json::from_slice(&body).expect("decode open source response");
        assert_eq!(opened.file, Utf8PathBuf::from("main.tex"));
        assert_eq!(opened.absolute_file, root.join("main.tex"));
        assert_eq!(opened.file_uri, source_file_uri(&root.join("main.tex")));
        assert_eq!(opened.editor_uri, "");
        assert_eq!(opened.line, 3);
        assert_eq!(opened.line0, 2);
        assert_eq!(opened.column, 2);
        assert_eq!(opened.column0, 1);
        assert_eq!(opened.offset_utf8, 12);
        assert_eq!(opened.source_hash, "#src=main.tex&line=3&column=2");
        assert_eq!(opened.editor_cwd, root.clone());
        assert!(!opened.editor_launch_supported);
        assert_eq!(opened.editor_program, "");
        assert!(opened.editor_args.is_empty());
        assert_eq!(opened.editor_command_line, "");
        assert!(!opened.launched);
    }

    #[tokio::test]
    async fn open_source_endpoint_returns_page_local_context_when_syncmap_exists() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "lead\nbody\ntail\n").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-15")).expect("rev dir");
        fs::write(
            build_root.join("rev-15/sources.json"),
            serde_json::json!({
                "files": {
                    "main.tex": "lead\nbody\ntail\n"
                }
            })
            .to_string(),
        )
        .expect("write sources");
        fs::write(
            build_root.join("rev-15/page-syncmap.json"),
            serde_json::to_vec(&vec![PageSyncMapArtifact {
                page_id: "page-a".to_string(),
                index: 0,
                width_pt: 512,
                height_pt: 600,
                items: vec![ArtifactSyncSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 5,
                    end_utf8: 14,
                    output_start_utf8: 32,
                    output_end_utf8: 64,
                    left_px: 72,
                    right_px: 180,
                    top_px: 80,
                    bottom_px: 140,
                }],
            }])
            .expect("serialize syncmap"),
        )
        .expect("write syncmap");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: Some(EditorBridgeConfig {
                program: "/bin/true".to_string(),
                args: Vec::new(),
            }),
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = open_source(
            Path(15),
            State(state),
            Json(OpenSourceRequest {
                file: "main.tex".to_string(),
                offset: Some(8),
                line: None,
                column: None,
                source_hash: None,
                launch: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let opened: OpenSourceResponse =
            serde_json::from_slice(&body).expect("decode open source response");
        assert_eq!(opened.file, Utf8PathBuf::from("main.tex"));
        assert_eq!(opened.absolute_file, root.join("main.tex"));
        assert_eq!(opened.file_uri, source_file_uri(&root.join("main.tex")));
        assert_eq!(opened.editor_uri, "");
        assert_eq!(opened.line, 2);
        assert_eq!(opened.line0, 1);
        assert_eq!(opened.column, 4);
        assert_eq!(opened.column0, 3);
        assert_eq!(opened.offset_utf8, 8);
        assert_eq!(opened.source_hash, "#src=main.tex&line=2&column=4");
        assert_eq!(opened.editor_cwd, root.clone());
        assert!(opened.editor_launch_supported);
        assert_eq!(opened.editor_program, "/bin/true");
        assert_eq!(
            opened.editor_args,
            vec![root.join("main.tex").as_str().to_string()]
        );
        assert_eq!(
            opened.editor_command_line,
            format!("/bin/true {}", root.join("main.tex").as_str())
        );
        assert_eq!(opened.page_id, Some("page-a".to_string()));
        assert_eq!(opened.page_index, Some(0));
        assert_eq!(opened.page_width_px, Some(512));
        assert_eq!(opened.page_height_px, Some(600));
        assert_eq!(opened.page_source_start_utf8, Some(5));
        assert_eq!(opened.page_source_end_utf8, Some(14));
        assert_eq!(opened.page_output_start_utf8, Some(32));
        assert_eq!(opened.page_output_end_utf8, Some(64));
        assert_eq!(
            opened.item.as_ref().map(|item| item.output_start_utf8),
            Some(32)
        );
        assert_eq!(
            opened.item.as_ref().map(|item| item.output_end_utf8),
            Some(64)
        );
        assert_eq!(
            opened.item.as_ref().map(|item| item.item_id.as_str()),
            Some("page-a:main.tex:5:14:2:3")
        );
    }

    #[tokio::test]
    async fn open_source_endpoint_materializes_editor_bridge_page_and_item_placeholders() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "lead\nbody\ntail\n").expect("main tex");
        let build_root = root.join(".latexd/build");
        let output_path = root.join("editor-context.txt");
        fs::create_dir_all(build_root.join("rev-15")).expect("rev dir");
        fs::write(
            build_root.join("rev-15/sources.json"),
            serde_json::json!({
                "files": {
                    "main.tex": "lead\nbody\ntail\n"
                }
            })
            .to_string(),
        )
        .expect("write sources");
        fs::write(
            build_root.join("rev-15/page-syncmap.json"),
            serde_json::to_vec(&vec![PageSyncMapArtifact {
                page_id: "page-a".to_string(),
                index: 0,
                width_pt: 512,
                height_pt: 600,
                items: vec![ArtifactSyncSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 5,
                    end_utf8: 14,
                    output_start_utf8: 32,
                    output_end_utf8: 64,
                    left_px: 72,
                    right_px: 180,
                    top_px: 80,
                    bottom_px: 140,
                }],
            }])
            .expect("serialize syncmap"),
        )
        .expect("write syncmap");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: Some(EditorBridgeConfig {
                program: "/bin/sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    format!(
                        "printf '%s\\n' \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" \"$6\" \"$7\" \"$8\" \"$9\" \"${{10}}\" > '{}'",
                        output_path.as_str()
                    ),
                    "latexd-open".to_string(),
                    "{page_id}".to_string(),
                    "{page_index}".to_string(),
                    "{page_source_start}".to_string(),
                    "{page_source_end}".to_string(),
                    "{page_output_start}".to_string(),
                    "{page_output_end}".to_string(),
                    "{item_file}".to_string(),
                    "{item_start}".to_string(),
                    "{item_output_end}".to_string(),
                    "{item_id}".to_string(),
                ],
            }),
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = open_source(
            Path(15),
            State(state),
            Json(OpenSourceRequest {
                file: "main.tex".to_string(),
                offset: Some(8),
                line: None,
                column: None,
                source_hash: None,
                launch: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let opened: OpenSourceResponse =
            serde_json::from_slice(&body).expect("decode open source response");
        assert_eq!(opened.absolute_file, root.join("main.tex"));
        assert_eq!(opened.editor_cwd, root.clone());
        assert!(opened.editor_launch_supported);
        assert_eq!(opened.editor_program, "/bin/sh");
        assert_eq!(
            opened.editor_args,
            vec![
                "-c".to_string(),
                format!(
                    "printf '%s\\n' \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" \"$6\" \"$7\" \"$8\" \"$9\" \"${{10}}\" > '{}'",
                    output_path.as_str()
                ),
                "latexd-open".to_string(),
                "page-a".to_string(),
                "0".to_string(),
                "5".to_string(),
                "14".to_string(),
                "32".to_string(),
                "64".to_string(),
                "main.tex".to_string(),
                "5".to_string(),
                "64".to_string(),
                "page-a:main.tex:5:14:2:3".to_string(),
            ]
        );
        let lines = timeout(Duration::from_secs(2), async {
            loop {
                if output_path.exists() {
                    let captured = fs::read_to_string(&output_path).expect("captured args");
                    let lines = captured.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
                    if lines.len() >= 10 {
                        break lines;
                    }
                }
                sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("editor bridge execution");
        assert_eq!(lines[0], "page-a");
        assert_eq!(lines[1], "0");
        assert_eq!(lines[2], "5");
        assert_eq!(lines[3], "14");
        assert_eq!(lines[4], "32");
        assert_eq!(lines[5], "64");
        assert_eq!(lines[6], "main.tex");
        assert_eq!(lines[7], "5");
        assert_eq!(lines[8], "64");
        assert_eq!(lines[9], "page-a:main.tex:5:14:2:3");
    }

    #[tokio::test]
    async fn open_source_endpoint_materializes_absolute_file_alias_and_editor_cwd() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "alpha\nbeta\ngamma\n").expect("main tex");
        let output_path = root.join("editor-alias.txt");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: Some(EditorBridgeConfig {
                program: "/bin/sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    format!(
                        "printf '%s\\n' \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" \"$6\" \"$7\" > '{}'",
                        output_path.as_str()
                    ),
                    "latexd-open".to_string(),
                    "{absolute_file}".to_string(),
                    "{editor_cwd}".to_string(),
                    "{file_uri}".to_string(),
                    "{editor_uri}".to_string(),
                    "{line0}".to_string(),
                    "{column}".to_string(),
                    "{column0}".to_string(),
                ],
            }),
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = open_source(
            Path(14),
            State(state),
            Json(OpenSourceRequest {
                file: "main.tex".to_string(),
                offset: None,
                line: Some(2),
                column: None,
                source_hash: None,
                launch: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let opened: OpenSourceResponse =
            serde_json::from_slice(&body).expect("decode open source response");
        assert_eq!(opened.absolute_file, root.join("main.tex"));
        assert_eq!(opened.editor_cwd, root.clone());
        assert_eq!(opened.file_uri, source_file_uri(&root.join("main.tex")));
        assert_eq!(opened.editor_uri, "");
        assert!(opened.editor_launch_supported);
        assert_eq!(
            opened.editor_args,
            vec![
                "-c".to_string(),
                format!(
                    "printf '%s\\n' \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" \"$6\" \"$7\" > '{}'",
                    output_path.as_str()
                ),
                "latexd-open".to_string(),
                root.join("main.tex").as_str().to_string(),
                root.as_str().to_string(),
                source_file_uri(&root.join("main.tex")),
                "".to_string(),
                "1".to_string(),
                "1".to_string(),
                "0".to_string(),
            ]
        );
        assert_eq!(opened.line0, 1);
        assert_eq!(opened.column, 1);
        assert_eq!(opened.column0, 0);

        let lines = timeout(Duration::from_secs(2), async {
            loop {
                if output_path.exists() {
                    let captured = fs::read_to_string(&output_path).expect("captured args");
                    let lines = captured.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
                    if lines.len() >= 7 {
                        break lines;
                    }
                }
                sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("editor bridge execution");
        assert_eq!(lines[0], root.join("main.tex").as_str());
        assert_eq!(lines[1], root.as_str());
        assert_eq!(lines[2], source_file_uri(&root.join("main.tex")));
        assert_eq!(
            lines[3],
            source_editor_uri("/bin/sh", &root.join("main.tex"), 2, 1)
        );
        assert_eq!(lines[4], "1");
        assert_eq!(lines[5], "1");
        assert_eq!(lines[6], "0");
    }

    #[tokio::test]
    async fn open_source_endpoint_materializes_editor_preview_placeholders() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "alpha\nbeta\ngamma\n").expect("main tex");
        let output_path = root.join("editor-preview-placeholders.txt");
        let editor_program = root.join("code");
        std::os::unix::fs::symlink("/bin/sh", editor_program.as_std_path()).expect("symlink code");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: Some(EditorBridgeConfig {
                program: editor_program.to_string(),
                args: vec![
                    "-c".to_string(),
                    format!(
                        "printf '%s\\n' \"$1\" \"$2\" \"$3\" \"$4\" > '{}'",
                        output_path.as_str()
                    ),
                    "latexd-open".to_string(),
                    "{editor_preview_kind}".to_string(),
                    "{editor_program}".to_string(),
                    "{editor_command_line}".to_string(),
                    "{editor_uri}".to_string(),
                ],
            }),
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = open_source(
            Path(17),
            State(state),
            Json(OpenSourceRequest {
                file: "main.tex".to_string(),
                offset: None,
                line: Some(2),
                column: Some(4),
                source_hash: None,
                launch: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let opened: OpenSourceResponse =
            serde_json::from_slice(&body).expect("decode open source response");
        assert_eq!(opened.editor_preview_kind, EditorPreviewKind::CommandAndUri);
        assert_eq!(opened.editor_program, editor_program.to_string());
        assert_eq!(
            opened.editor_uri,
            source_editor_uri("code", &root.join("main.tex"), 2, 4)
        );
        assert_eq!(opened.editor_args[3], "command_and_uri");
        assert_eq!(opened.editor_args[4], editor_program.as_str());
        assert_eq!(opened.editor_args[5], opened.editor_command_line);
        assert_eq!(opened.editor_args[6], opened.editor_uri);
        assert!(opened.editor_command_line.contains(editor_program.as_str()));
        assert!(opened.editor_command_line.contains("command_and_uri"));
        assert!(opened.editor_command_line.contains(&opened.editor_uri));
        assert!(!opened.editor_command_line.contains("{editor_"));

        let lines = timeout(Duration::from_secs(2), async {
            loop {
                if output_path.exists() {
                    let captured = fs::read_to_string(&output_path).expect("captured args");
                    let lines = captured.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
                    if lines.len() >= 4 {
                        break lines;
                    }
                }
                sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("editor bridge execution");
        assert_eq!(lines[0], "command_and_uri");
        assert_eq!(lines[1], editor_program.as_str());
        assert_eq!(lines[2], opened.editor_command_line);
        assert_eq!(lines[3], opened.editor_uri);
    }

    #[tokio::test]
    async fn open_source_endpoint_uses_line_and_column_for_default_code_bridge_args() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "lead\nbody\ntail\n").expect("main tex");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: root.join(".latexd/build"),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: Some(EditorBridgeConfig {
                program: "code".to_string(),
                args: Vec::new(),
            }),
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = open_source(
            Path(15),
            State(state),
            Json(OpenSourceRequest {
                file: "main.tex".to_string(),
                offset: None,
                line: Some(2),
                column: Some(4),
                source_hash: None,
                launch: Some(false),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let opened: OpenSourceResponse =
            serde_json::from_slice(&body).expect("decode open source response");
        assert_eq!(opened.line, 2);
        assert_eq!(opened.line0, 1);
        assert_eq!(opened.column, 4);
        assert_eq!(opened.column0, 3);
        assert_eq!(opened.editor_program, "code");
        assert_eq!(
            opened.editor_uri,
            source_editor_uri("code", &root.join("main.tex"), 2, 4)
        );
        assert_eq!(
            opened.editor_args,
            vec![
                "--goto".to_string(),
                format!("{}:2:4", root.join("main.tex").as_str()),
            ]
        );
    }

    #[tokio::test]
    async fn open_source_endpoint_encodes_nested_source_hash_paths() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::create_dir_all(root.join("sections").as_std_path()).expect("sections dir");
        fs::write(root.join("main.tex"), "\\input{sections/intro}\n").expect("main tex");
        fs::write(root.join("sections/intro.tex"), "nested\n").expect("intro tex");
        let build_root = root.join(".latexd/build");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: Some(EditorBridgeConfig {
                program: "/bin/true".to_string(),
                args: Vec::new(),
            }),
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let request: OpenSourceRequest = serde_json::from_value(serde_json::json!({
            "source_hash": "#src=sections%2Fintro.tex&line=1"
        }))
        .expect("decode open source request");
        assert!(request.file.is_empty());

        let response = open_source(Path(3), State(state), Json(request)).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let opened: OpenSourceResponse =
            serde_json::from_slice(&body).expect("decode open source response");
        assert_eq!(opened.file, Utf8PathBuf::from("sections/intro.tex"));
        assert_eq!(opened.source_hash, "#src=sections%2Fintro.tex&line=1");
    }

    #[tokio::test]
    async fn source_jump_endpoint_accepts_source_hash_without_explicit_file() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "hello").expect("main tex");
        let build_root = root.join(".latexd/build");
        fs::create_dir_all(build_root.join("rev-13")).expect("rev dir");
        fs::write(
            build_root.join("rev-13/sources.json"),
            serde_json::json!({
                "files": {
                    "main.tex": "lead\nbody\ntail\n"
                }
            })
            .to_string(),
        )
        .expect("write sources");
        fs::write(
            build_root.join("rev-13/page-metadata.json"),
            serde_json::to_vec(&vec![
                PageArtifactMeta {
                    page_id: "page-a".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 512,
                    height_pt: 600,
                    content_hash: "hash-a".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 32,
                    pdf_artifact_path: Utf8PathBuf::from("rev-13/pages/page-a.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 4,
                    }],
                },
                PageArtifactMeta {
                    page_id: "page-b".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 512,
                    height_pt: 600,
                    content_hash: "hash-b".to_string(),
                    text_start_utf8: 32,
                    text_end_utf8: 64,
                    pdf_artifact_path: Utf8PathBuf::from("rev-13/pages/page-b.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 5,
                        end_utf8: 14,
                    }],
                },
            ])
            .expect("serialize metadata"),
        )
        .expect("write page metadata");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: None,
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = source_jump(
            Path(13),
            Query(SourceJumpQuery {
                file: String::new(),
                offset: Some(0),
                line: Some(1),
                column: Some(1),
                source_hash: Some("#src=main.tex&line=2&column=4".to_string()),
            }),
            State(state),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let jump: SourceJumpResponse = serde_json::from_slice(&body).expect("decode source jump");

        assert_eq!(jump.file, Utf8PathBuf::from("main.tex"));
        assert_eq!(jump.line, 2);
        assert_eq!(jump.column, 4);
        assert_eq!(jump.source_hash, "#src=main.tex&line=2&column=4");
    }

    #[tokio::test]
    async fn open_source_endpoint_prefers_source_hash_over_inconsistent_explicit_fields() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::create_dir_all(root.join("sections").as_std_path()).expect("sections dir");
        fs::write(root.join("main.tex"), "\\input{sections/intro}\n").expect("main tex");
        fs::write(root.join("wrong.tex"), "wrong\n").expect("wrong tex");
        fs::write(root.join("sections/intro.tex"), "nested\n").expect("intro tex");
        let build_root = root.join(".latexd/build");
        let state = Arc::new(AppState {
            root: root.clone(),
            build_root: build_root.clone(),
            artifacts_root: root.join(".latexd/artifacts"),
            world: ProjectWorld::load(root.clone()).expect("world"),
            compiler: CompilerDriver::new(None, Vec::new()),
            tile_renderer: TileRendererConfig::Mock,
            editor_bridge: Some(EditorBridgeConfig {
                program: "/bin/true".to_string(),
                args: Vec::new(),
            }),
            raster_cache: RwLock::new(BTreeMap::new()),
            inflight_rasters: RwLock::new(BTreeMap::new()),
            build_cache: RwLock::new(BuildCache::default()),
            live: RwLock::new(LivePreviewState::default()),
            events: broadcast::channel(4).0,
        });

        let response = open_source(
            Path(4),
            State(state),
            Json(OpenSourceRequest {
                file: "wrong.tex".to_string(),
                offset: None,
                line: Some(99),
                column: Some(9),
                source_hash: Some("#src=sections%2Fintro.tex&line=1&column=3".to_string()),
                launch: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let opened: OpenSourceResponse =
            serde_json::from_slice(&body).expect("decode open source response");
        assert_eq!(opened.file, Utf8PathBuf::from("sections/intro.tex"));
        assert_eq!(opened.line, 1);
        assert_eq!(opened.column, 3);
        assert_eq!(
            opened.source_hash,
            "#src=sections%2Fintro.tex&line=1&column=3"
        );
    }
}
