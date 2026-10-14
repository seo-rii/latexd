//! A durable pointer to the last published preview, independent of build success.
//! Draft manifests never become recovery roots until the publication gate commits.
use std::{fs, io::Write};

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::{LivePreviewState, PreviewSnapshot, compiler::PageArtifactMeta};

const MANIFEST: &str = "published-preview.json";
const VERSION: u32 = 1;

pub(crate) async fn reserve_revision(build_root: &Utf8Path, rev: u64) -> Result<()> {
    let build_root = build_root.to_owned();
    tokio::task::spawn_blocking(move || {
        fs::create_dir(build_root.join(format!("rev-{rev}")))
            .context("failed to reserve unique build revision")?;
        fs::File::open(build_root)?.sync_all()?;
        Ok(())
    })
    .await?
}

pub(crate) async fn published_page_metadata(
    build_root: &Utf8Path,
) -> Result<Option<(u64, Vec<PageArtifactMeta>)>> {
    match tokio::fs::read(build_root.join(MANIFEST)).await {
        Ok(bytes) => {
            let manifest: PublishedPreview = serde_json::from_slice(&bytes)?;
            if manifest.version != VERSION || manifest.snapshot.last_applied_rev == 0 {
                bail!("unsupported published preview manifest");
            }
            Ok(Some((
                manifest.snapshot.last_applied_rev,
                manifest.page_metadata,
            )))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[derive(Serialize, Deserialize)]
struct PublishedPreview {
    version: u32,
    snapshot: PreviewSnapshot,
    pdf_path: Utf8PathBuf,
    page_metadata: Vec<PageArtifactMeta>,
}

pub(crate) struct PreparedPreview {
    path: Utf8PathBuf,
    build_root: Utf8PathBuf,
}

impl Drop for PreparedPreview {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl PreparedPreview {
    /// No await is permitted between the accepted-edit gate and this rename.
    pub(crate) fn commit(self) -> Result<()> {
        fs::rename(&self.path, self.build_root.join(MANIFEST))
            .context("failed to atomically publish preview manifest")?;
        // Rename is the commit point. A directory fsync failure cannot roll it
        // back; report it while keeping live and durable ownership consistent.
        if let Err(error) = fs::File::open(&self.build_root).and_then(|dir| dir.sync_all()) {
            tracing::warn!("preview committed but directory sync failed: {error}");
        }
        Ok(())
    }
}

fn artifact_path(build_root: &Utf8Path, relative: &Utf8Path) -> Result<Utf8PathBuf> {
    if relative.as_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, camino::Utf8Component::Normal(_)))
    {
        bail!("invalid published artifact path {relative}");
    }
    let path = build_root.join(relative);
    if !path.is_file() && crate::pdf_export::validated_recipe(build_root, &path)? {
        return Ok(path);
    }
    let canonical = fs::canonicalize(&path)
        .with_context(|| format!("published artifact is missing: {relative}"))?;
    if !canonical.starts_with(fs::canonicalize(build_root)?) || !canonical.is_file() {
        bail!("published artifact is outside the build store: {relative}");
    }
    Ok(path)
}

fn validate_url(build_root: &Utf8Path, url: &str) -> Result<()> {
    let Some((prefix, relative)) = url.split_once("/artifacts/rev/") else {
        bail!("unsupported published artifact URL {url}");
    };
    if !prefix.is_empty() && !prefix.starts_with('/') {
        bail!("published artifact URL must be local");
    }
    let Some((rev, path)) = relative.split_once('/') else {
        bail!("invalid published revision URL {url}");
    };
    let rev = rev.parse::<u64>()?;
    artifact_path(build_root, &Utf8PathBuf::from(format!("rev-{rev}/{path}")))?;
    Ok(())
}

fn validate(build_root: &Utf8Path, manifest: &PublishedPreview) -> Result<()> {
    if manifest.version != VERSION || manifest.snapshot.last_applied_rev == 0 {
        bail!("unsupported published preview manifest");
    }
    artifact_path(build_root, &manifest.pdf_path)?;
    let snapshot = &manifest.snapshot;
    let pdf_url = snapshot
        .pdf_url
        .as_ref()
        .context("published PDF URL is missing")?;
    validate_url(build_root, pdf_url)?;
    for page in &snapshot.page_artifacts {
        validate_url(build_root, &page.pdf_url)?;
        if let Some(url) = &page.svg_url {
            validate_url(build_root, url)?;
        }
    }
    for page in &manifest.page_metadata {
        if !page.pdf_artifact_path.as_str().is_empty() {
            artifact_path(build_root, &page.pdf_artifact_path)?;
        }
    }
    if let Some(ir) = &snapshot.render_ir_artifacts {
        for url in [
            &ir.legacy_output_url,
            &ir.events_url,
            &ir.document_ir_url,
            &ir.page_display_list_url,
            &ir.display_list_pdf_url,
        ]
        .into_iter()
        .chain(ir.display_list_svg_urls.iter())
        {
            validate_url(build_root, url)?;
        }
    }
    // Sync-map/source requests after restart must resolve against the same input.
    artifact_path(
        build_root,
        &Utf8PathBuf::from(format!("rev-{}/sources.json", snapshot.last_applied_rev)),
    )?;
    Ok(())
}

pub(crate) async fn prepare(
    build_root: &Utf8Path,
    live: &LivePreviewState,
) -> Result<PreparedPreview> {
    let manifest = PublishedPreview {
        version: VERSION,
        snapshot: live.snapshot.clone(),
        pdf_path: live
            .latest_pdf_path
            .as_ref()
            .context("published PDF path is missing")?
            .strip_prefix(build_root)?
            .to_owned(),
        page_metadata: live.page_metadata.clone(),
    };
    let build_root = build_root.to_owned();
    tokio::task::spawn_blocking(move || {
        validate(&build_root, &manifest)?;
        let path = build_root.join(format!(
            "rev-{}/preview.pending",
            manifest.snapshot.last_applied_rev
        ));
        let bytes = serde_json::to_vec(&manifest)?;
        let mut file = fs::File::create(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        Ok(PreparedPreview { path, build_root })
    })
    .await?
}

pub(crate) async fn recover(
    build_root: &Utf8Path,
    editor_bridge_enabled: bool,
) -> Result<LivePreviewState> {
    let build_root = build_root.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut live = LivePreviewState::default();
        let manifest = fs::read(build_root.join(MANIFEST));
        match manifest {
            Ok(bytes) => match serde_json::from_slice::<PublishedPreview>(&bytes)
                .context("invalid preview manifest")
                .and_then(|manifest| {
                    validate(&build_root, &manifest)?;
                    Ok(manifest)
                }) {
                Ok(manifest) => {
                    live.latest_pdf_path = Some(build_root.join(manifest.pdf_path));
                    live.page_metadata = manifest.page_metadata;
                    live.snapshot = manifest.snapshot;
                }
                Err(error) => tracing::warn!("published preview recovery skipped: {error}"),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        // Failed/unpublished builds still reserve their ids across restarts.
        for entry in fs::read_dir(&build_root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Some(rev) = entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.strip_prefix("rev-"))
                    .and_then(|rev| rev.parse::<u64>().ok())
            {
                live.snapshot.current_rev = live.snapshot.current_rev.max(rev);
            }
        }
        live.snapshot.building = false;
        live.snapshot.preview_protocol_version = VERSION;
        live.snapshot.editor_bridge_enabled = editor_bridge_enabled;
        Ok(live)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmr_protocol::{PagePreviewArtifact, SourceSnapshotFile};

    #[tokio::test]
    async fn preview_persistence_real_build_failure_restart_serves_pdf_tiles_and_frozen_source() {
        use crate::{AppState, BuildCache, TileRendererConfig, compiler::CompilerDriver};
        use axum::{
            body::to_bytes,
            extract::{Path, Query, State},
            http::StatusCode,
        };
        use std::{
            collections::BTreeMap,
            sync::{Arc, Mutex as StdMutex},
            time::Duration,
        };
        use tokio::sync::{RwLock, broadcast, mpsc};

        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap().to_owned();
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .unwrap();
        let source = r"\begin{document}First page.\newpage Last good page.\end{document}";
        fs::write(root.join("main.tex"), source).unwrap();
        let build_root = root.join(".latexd/build");
        let artifacts_root = root.join(".latexd/artifacts");
        fs::create_dir_all(&build_root).unwrap();
        fs::create_dir_all(&artifacts_root).unwrap();
        let new_state = |live| {
            let (events, _) = broadcast::channel(64);
            Arc::new(AppState {
                root: root.clone(),
                build_root: build_root.clone(),
                artifacts_root: artifacts_root.clone(),
                world: tex_world::ProjectWorld::load(root.clone()).unwrap(),
                compiler: CompilerDriver::new(Some("internal".into()), Vec::new())
                    .with_preview_artifacts(),
                tile_renderer: TileRendererConfig::Mock,
                editor_bridge: None,
                raster_cache: RwLock::new(BTreeMap::new()),
                inflight_rasters: StdMutex::new(BTreeMap::new()),
                build_cache: RwLock::new(BuildCache::default()),
                live: RwLock::new(live),
                events,
            })
        };
        let state = new_state(LivePreviewState::default());
        let mut receiver = state.events.subscribe();
        let (tx, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(state.clone().run_build_loop(rx));
        tx.send(vec!["main.tex".into()]).unwrap();
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let hmr_protocol::ServerMsg::PreviewCommitted {
                    base_preview_rev,
                    new_preview_rev,
                    source_snapshot,
                    ..
                } = receiver.recv().await.unwrap()
                {
                    assert_eq!(base_preview_rev, 0);
                    assert_eq!(new_preview_rev, 1);
                    assert!(
                        build_root.join(MANIFEST).is_file(),
                        "durable before notification"
                    );
                    assert_eq!(
                        source_snapshot
                            .iter()
                            .find(|file| file.file == "main.tex")
                            .unwrap()
                            .content,
                        source
                    );
                    break;
                }
            }
        })
        .await
        .unwrap();
        fs::write(root.join("main.tex"), r"\errmessage{intentional failure}").unwrap();
        tx.send(vec!["main.tex".into()]).unwrap();
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let hmr_protocol::ServerMsg::BuildFinished {
                    rev: 2,
                    success: false,
                } = receiver.recv().await.unwrap()
                {
                    break;
                }
            }
        })
        .await
        .unwrap();
        task.abort();
        let _ = task.await;
        let recovered = recover(&build_root, false).await.unwrap();
        assert_eq!(recovered.snapshot.last_applied_rev, 1);
        assert_eq!(recovered.snapshot.current_rev, 2);
        assert!(
            !recovered.latest_pdf_path.as_ref().unwrap().exists(),
            "restart must not eagerly export full PDF"
        );
        let restarted = new_state(recovered);
        let axum::Json(snapshot) = crate::snapshot(State(restarted.clone())).await;
        assert_eq!(snapshot.page_artifacts.len(), 2);
        assert!(snapshot.render_ir_artifacts.is_none());
        // Block ownership acquisition without sleeps. Publication must stay
        // excluded until the selected preview revision is pinned for download.
        let gc_gate = crate::preview_leases::gc_roots(&build_root).await;
        let mut download = Box::pin(crate::latest_pdf(State(restarted.clone())));
        {
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            assert!(std::future::Future::poll(download.as_mut(), &mut context).is_pending());
            assert!(
                restarted.live.try_write().is_err(),
                "publication cannot advance a PDF revision before ownership is acquired"
            );
        }
        drop(gc_gate);
        let exported = download.await;
        assert_eq!(exported.status(), StatusCode::OK);
        let bytes = to_bytes(exported.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(
            lopdf::Document::load_mem(&bytes).unwrap().get_pages().len(),
            2
        );
        assert!(build_root.join("rev-1/main.pdf").is_file());

        for page in snapshot.page_artifacts {
            for url in std::iter::once(page.pdf_url).chain(page.svg_url) {
                let relative = url.split_once("/artifacts/rev/").unwrap().1;
                let (rev, path) = relative.split_once('/').unwrap();
                let response = crate::revision_artifact(
                    Path((rev.parse().unwrap(), path.into())),
                    State(restarted.clone()),
                )
                .await;
                assert_eq!(response.status(), StatusCode::OK, "{url}");
                assert!(
                    !to_bytes(response.into_body(), 10 * 1024 * 1024)
                        .await
                        .unwrap()
                        .is_empty()
                );
            }
            let response = crate::revision_tile_png(
                Path((1, page.page_id, 100, 0, "0.png".into())),
                Query(crate::RasterQuery {
                    scale: Some(1.0),
                    tile_size: None,
                }),
                State(restarted.clone()),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
        }
        let response = crate::source_file(
            Path(1),
            Query(crate::SourceFileQuery {
                file: "main.tex".into(),
            }),
            State(restarted),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let json: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(
            json["content"], source,
            "restart must not read the now-invalid disk source"
        );
    }

    fn candidate(root: &Utf8Path, rev: u64) -> LivePreviewState {
        fs::create_dir_all(root.join(format!("rev-{rev}"))).unwrap();
        let pdf = root.join(format!("rev-{rev}/main.pdf"));
        fs::write(&pdf, format!("PDF {rev}")).unwrap();
        fs::write(root.join(format!("rev-{rev}/sources.json")), serde_json::to_vec(
            &serde_json::json!({ "rev": rev, "files": { "main.tex": format!("source {rev}") } })
        ).unwrap()).unwrap();
        let owner = root.join("rev-1/pages");
        fs::create_dir_all(&owner).unwrap();
        fs::write(owner.join("tail.pdf"), "immutable page").unwrap();
        fs::write(owner.join("tail.svg"), "immutable SVG").unwrap();
        let pages = vec![PageArtifactMeta {
            page_id: "tail".into(),
            index: 0,
            line_count: 1,
            width_pt: 612,
            height_pt: 792,
            natural_size_pt: None,
            content_hash: "stable-paint".into(),
            text_start_utf8: 0,
            text_end_utf8: 8,
            pdf_artifact_path: "rev-1/pages/tail.pdf".into(),
            source_spans: Vec::new(),
        }];
        fs::write(
            root.join(format!("rev-{rev}/page-metadata.json")),
            serde_json::to_vec(&pages).unwrap(),
        )
        .unwrap();
        LivePreviewState {
            latest_pdf_path: Some(pdf),
            page_metadata: pages,
            snapshot: PreviewSnapshot {
                preview_protocol_version: VERSION,
                current_rev: rev,
                last_applied_rev: rev,
                pdf_url: Some(format!("/artifacts/rev/{rev}/main.pdf")),
                page_count: 1,
                page_ids: vec!["tail".into()],
                page_artifacts: vec![PagePreviewArtifact {
                    page_id: "tail".into(),
                    pdf_url: "/artifacts/rev/1/pages/tail.pdf".into(),
                    svg_url: Some("/artifacts/rev/1/pages/tail.svg".into()),
                }],
                source_snapshot: vec![SourceSnapshotFile {
                    file: "main.tex".into(),
                    content: format!("source {rev}"),
                    line_count: 1,
                }],
                last_build_succeeded: Some(true),
                ..PreviewSnapshot::default()
            },
        }
    }

    #[tokio::test]
    async fn preview_persistence_gc_uses_published_metadata_after_cache_loss() {
        for corrupt in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let root = Utf8Path::from_path(temp.path()).unwrap();
            candidate(root, 2);
            prepare(root, &candidate(root, 20))
                .await
                .unwrap()
                .commit()
                .unwrap();
            let metadata = root.join("rev-20/page-metadata.json");
            if corrupt {
                fs::write(metadata, "invalid cache metadata").unwrap();
            } else {
                fs::remove_file(metadata).unwrap();
            }
            assert_eq!(
                recover(root, false)
                    .await
                    .unwrap()
                    .snapshot
                    .last_applied_rev,
                20
            );
            crate::prune_old_build_revisions(root, 20).await;
            assert!(root.join("rev-1/pages/tail.pdf").is_file());
            assert!(root.join("rev-1/pages/tail.svg").is_file());
            assert!(
                !root.join("rev-2").exists(),
                "unreferenced output still reclaimed"
            );
            assert_eq!(
                recover(root, false)
                    .await
                    .unwrap()
                    .snapshot
                    .last_applied_rev,
                20
            );
        }
    }

    #[tokio::test]
    async fn preview_persistence_gc_fails_closed_for_leased_metadata_cache_loss() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        candidate(root, 3);
        fs::create_dir_all(root.join("rev-20")).unwrap();
        let lease = crate::preview_leases::ClientLease::new(root).await;
        assert!(lease.set_revision(3, 20).await);
        fs::remove_file(root.join("rev-3/page-metadata.json")).unwrap();
        crate::prune_old_build_revisions(root, 20).await;
        assert!(root.join("rev-1/pages/tail.pdf").is_file());
        assert!(root.join("rev-3/main.pdf").is_file());
        drop(lease);
        crate::prune_old_build_revisions(root, 20).await;
        assert!(!root.join("rev-1").exists());
        assert!(!root.join("rev-3").exists());
    }

    #[tokio::test]
    async fn preview_persistence_twenty_commits_gc_restart_and_reserved_revision() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        for rev in 1..=20 {
            let live = candidate(root, rev);
            prepare(root, &live).await.unwrap().commit().unwrap();
            crate::prune_old_build_revisions(root, rev).await;
            let recovered = recover(root, false).await.unwrap();
            assert_eq!(recovered.snapshot.last_applied_rev, rev);
            assert_eq!(
                recovered.snapshot.source_snapshot[0].content,
                format!("source {rev}")
            );
            assert_eq!(
                fs::read_to_string(recovered.latest_pdf_path.unwrap()).unwrap(),
                format!("PDF {rev}")
            );
            assert!(
                root.join(&recovered.page_metadata[0].pdf_artifact_path)
                    .is_file()
            );
            assert!(root.join("rev-1/pages/tail.svg").is_file());
        }
        assert!(
            !root.join("rev-2").exists(),
            "unreferenced old output reclaimed"
        );
        reserve_revision(root, 21).await.unwrap();
        let recovered = recover(root, true).await.unwrap();
        assert_eq!(recovered.snapshot.last_applied_rev, 20);
        assert_eq!(recovered.snapshot.current_rev, 21);
        assert!(recovered.snapshot.editor_bridge_enabled);
        assert!(!recovered.snapshot.building);
        assert!(
            reserve_revision(root, 21).await.is_err(),
            "ids cannot overwrite old builds"
        );
        reserve_revision(root, recovered.snapshot.current_rev + 1)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn preview_persistence_uncommitted_draft_keeps_lastgood_and_gc_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        prepare(root, &candidate(root, 1))
            .await
            .unwrap()
            .commit()
            .unwrap();
        let draft = prepare(root, &candidate(root, 20)).await.unwrap();
        let recovered = recover(root, false).await.unwrap();
        assert_eq!(recovered.snapshot.last_applied_rev, 1);
        assert_eq!(recovered.snapshot.current_rev, 20);
        drop(draft);
        assert!(!root.join("rev-20/preview.pending").exists());
        crate::prune_old_build_revisions(root, 20).await;
        assert!(
            root.join("rev-1/main.pdf").exists(),
            "published preview is a GC root"
        );
        assert_eq!(
            recover(root, false)
                .await
                .unwrap()
                .snapshot
                .last_applied_rev,
            1
        );
    }

    #[tokio::test]
    async fn preview_persistence_rejects_missing_artifact_without_replacing_good_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        prepare(root, &candidate(root, 1))
            .await
            .unwrap()
            .commit()
            .unwrap();
        let mut bad = candidate(root, 2);
        bad.snapshot.page_artifacts[0].svg_url = Some("/artifacts/rev/2/missing.svg".into());
        assert!(prepare(root, &bad).await.is_err());
        assert_eq!(
            recover(root, false)
                .await
                .unwrap()
                .snapshot
                .last_applied_rev,
            1
        );
        fs::remove_file(root.join("rev-1/pages/tail.svg")).unwrap();
        let recovered = recover(root, false).await.unwrap();
        assert_eq!(
            recovered.snapshot.last_applied_rev, 0,
            "do not advertise broken artifacts"
        );
        assert_eq!(recovered.snapshot.current_rev, 2);
    }

    #[tokio::test]
    async fn preview_persistence_corrupt_manifest_does_not_reuse_build_ids() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        reserve_revision(root, 9).await.unwrap();
        fs::write(root.join(MANIFEST), "truncated json").unwrap();
        let recovered = recover(root, false).await.unwrap();
        assert_eq!(recovered.snapshot.last_applied_rev, 0);
        assert_eq!(recovered.snapshot.current_rev, 9);
        assert!(!recovered.snapshot.building);
    }
}
