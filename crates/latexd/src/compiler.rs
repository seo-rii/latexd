use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{Display, Formatter},
    fs,
};

use anyhow::Context;
use camino::{Utf8Path, Utf8PathBuf};
use hmr_protocol::{Diagnostic, DiagnosticLevel, PagePatchOp, PagePreviewArtifact};
use tex_aux::{
    MaterializedProject, MaterializedRewriteSpan, PageSourceSlice, SemanticAux, SourceSpan,
    derive_semantic_aux, derive_semantic_aux_index, materialize_project,
    parse_concrete_semantic_aux, scan_project,
    serialize_concrete_semantic_aux_backdated_with_previous,
    serialize_semantic_aux_backdated_with_previous,
};
use tex_bootstrap::{
    ProjectPageMeta, ProjectPdfBuild, ProjectReplayCheckpoint, build_project_pdf_from_checkpoint,
    build_project_pdf_from_checkpoint_with_mounts, capture_page_checkpoints,
    compile_mini_kernel_snapshot, run_project_pdf_from_base_snapshot,
    run_project_pdf_from_base_snapshot_with_mounts,
};
use tex_checkpoint::{
    CheckpointBundle, CheckpointKind, CheckpointPage, InputBoundaryCheckpoint, ShipoutCheckpoint,
    StoredCheckpoint, build_checkpoint_bundle_with_shipouts, find_unchanged_tail,
    load_checkpoint_bundle, preamble_key_for_source, save_checkpoint_bundle,
    select_reusable_preamble,
};
use tex_pdf::{
    PAGE_FONT_SIZE_PT, PAGE_LINE_HEIGHT_PT, PAGE_TEXT_LEFT_PT, PAGE_TEXT_TOP_PT,
    render_display_list_pdf, render_display_list_svg, render_page_svg, render_single_page_pdf,
};
use tex_render_model::{AuxView, DocumentIr, PageDisplayList, RenderEventStream, to_pretty_json};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{VmModuleCheckpointKind, VmReplayFrame};
use tex_world::{CompilerMode, ProjectManifest, normalize_relative_path};

use crate::viewer_prefixed_path;

#[derive(Debug, Clone)]
pub struct CompilerDriver {
    compiler_bin: Option<String>,
    compiler_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CompileRequest {
    pub root: Utf8PathBuf,
    pub manifest: ProjectManifest,
    pub toplevel: Utf8PathBuf,
    pub rev: u64,
    pub build_root: Utf8PathBuf,
    pub changed_files: Vec<Utf8PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CompileOutcome {
    pub pdf_path: Utf8PathBuf,
    pub diagnostics: Vec<Diagnostic>,
    pub dep_trace: DepTrace,
    pub page_metadata: Vec<PageArtifactMeta>,
    pub page_artifacts: Vec<PagePreviewArtifact>,
    pub reused_checkpoint_id: Option<String>,
    pub unchanged_tail: Option<UnchangedTail>,
    pub page_patches: Vec<PagePatchOp>,
}

#[derive(Debug, Clone)]
pub struct CompileFailure {
    pub diagnostics: Vec<Diagnostic>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct InternalRenderIrCapture {
    pub legacy_output: String,
    pub events: RenderEventStream,
    pub document_ir: DocumentIr,
    pub page_display_lists: Vec<PageDisplayList>,
    pub display_list_pdf: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalRenderArtifactPaths {
    pub legacy_output: Utf8PathBuf,
    pub events: Utf8PathBuf,
    pub document_ir: Utf8PathBuf,
    pub page_display_list: Utf8PathBuf,
    pub display_list_svgs: Vec<Utf8PathBuf>,
    pub display_list_pdf: Utf8PathBuf,
}

impl InternalRenderIrCapture {
    pub fn write_debug_artifacts(
        &self,
        output_dir: impl AsRef<Utf8Path>,
    ) -> anyhow::Result<InternalRenderArtifactPaths> {
        let output_dir = output_dir.as_ref();
        fs::create_dir_all(output_dir.as_std_path())
            .with_context(|| format!("failed to create render artifact dir {output_dir}"))?;

        let paths = InternalRenderArtifactPaths {
            legacy_output: output_dir.join("legacy-output.txt"),
            events: output_dir.join("events.json"),
            document_ir: output_dir.join("document-ir.json"),
            page_display_list: output_dir.join("page-display-list.json"),
            display_list_svgs: self
                .page_display_lists
                .iter()
                .enumerate()
                .map(|(index, _)| output_dir.join(format!("display-list-page-{index}.svg")))
                .collect(),
            display_list_pdf: output_dir.join("display-list.pdf"),
        };

        fs::write(paths.legacy_output.as_std_path(), &self.legacy_output)
            .with_context(|| format!("failed to write {}", paths.legacy_output))?;
        fs::write(
            paths.events.as_std_path(),
            to_pretty_json(&self.events).context("failed to serialize render events artifact")?,
        )
        .with_context(|| format!("failed to write {}", paths.events))?;
        fs::write(
            paths.document_ir.as_std_path(),
            to_pretty_json(&self.document_ir)
                .context("failed to serialize document IR artifact")?,
        )
        .with_context(|| format!("failed to write {}", paths.document_ir))?;
        fs::write(
            paths.page_display_list.as_std_path(),
            to_pretty_json(&self.page_display_lists)
                .context("failed to serialize page display-list artifact")?,
        )
        .with_context(|| format!("failed to write {}", paths.page_display_list))?;
        for (page, path) in self.page_display_lists.iter().zip(&paths.display_list_svgs) {
            fs::write(path.as_std_path(), render_display_list_svg(page))
                .with_context(|| format!("failed to write {path}"))?;
        }
        fs::write(paths.display_list_pdf.as_std_path(), &self.display_list_pdf)
            .with_context(|| format!("failed to write {}", paths.display_list_pdf))?;

        Ok(paths)
    }
}

pub fn capture_internal_render_ir(
    source_path: impl Into<Utf8PathBuf>,
    source: &str,
    aux: &impl AuxView,
) -> InternalRenderIrCapture {
    capture_internal_render_ir_with_mounted_files(source_path, source, aux, &[])
}

pub fn capture_internal_render_ir_with_mounted_files(
    source_path: impl Into<Utf8PathBuf>,
    source: &str,
    aux: &impl AuxView,
    mounted_files: &[(&str, &str)],
) -> InternalRenderIrCapture {
    let source_path = source_path.into();
    let mut interner = ControlSequenceInterner::new();
    let mut vm = tex_vm::Vm::new(&mut interner);
    vm.set_entry_source_path(source_path.clone());
    for (path, source) in mounted_files {
        vm.mount_file(*path, *source);
    }
    vm.enable_render_event_capture();
    let outcome = vm.run_plain(source);
    let events = RenderEventStream::new(Some(source_path.to_string()), outcome.render_events);
    let document_ir = tex_layout::build_document_ir(&events, aux);
    let page_display_lists = tex_layout::build_page_display_lists(
        &document_ir,
        tex_layout::PageDisplayListOptions::default(),
    );
    let display_list_pdf = render_display_list_pdf(&page_display_lists);

    InternalRenderIrCapture {
        legacy_output: outcome.output,
        events,
        document_ir,
        page_display_lists,
        display_list_pdf,
    }
}

impl Display for CompileFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CompileFailure {}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PageArtifactMeta {
    pub page_id: String,
    pub index: usize,
    pub line_count: usize,
    pub width_pt: u32,
    pub height_pt: u32,
    pub content_hash: String,
    pub text_start_utf8: u32,
    pub text_end_utf8: u32,
    #[serde(default)]
    pub pdf_artifact_path: Utf8PathBuf,
    pub source_spans: Vec<ArtifactSourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnchangedTail {
    pub previous_rev: u64,
    pub resume_checkpoint_id: String,
    pub previous_page_start: usize,
    pub current_page_start: usize,
    pub page_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactSourceSpan {
    pub file: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactSyncSpan {
    pub file: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
    pub output_start_utf8: u32,
    pub output_end_utf8: u32,
    #[serde(default)]
    pub left_px: u32,
    #[serde(default)]
    pub right_px: u32,
    #[serde(default)]
    pub top_px: u32,
    #[serde(default)]
    pub bottom_px: u32,
}

fn default_syncmap_width_pt() -> u32 {
    612
}

const MAX_SEMANTIC_AUX_PASSES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PageSyncMapArtifact {
    pub page_id: String,
    pub index: usize,
    #[serde(default = "default_syncmap_width_pt")]
    pub width_pt: u32,
    pub height_pt: u32,
    pub items: Vec<ArtifactSyncSpan>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DepTrace {
    pub inputs: Vec<Utf8PathBuf>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StoredSourceTexts {
    files: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    executed_files: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    rewrite_spans: BTreeMap<Utf8PathBuf, Vec<MaterializedRewriteSpan>>,
    #[serde(default)]
    module_traces: Vec<StoredModuleTrace>,
    #[serde(default)]
    module_checkpoints: Vec<StoredModuleCheckpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct StoredModuleTrace {
    path: Utf8PathBuf,
    #[serde(default)]
    source_start_utf8: u32,
    #[serde(default)]
    source_end_utf8: u32,
    output_start_utf8: u32,
    output_end_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct StoredModuleCheckpoint {
    #[serde(default)]
    kind: VmModuleCheckpointKind,
    #[serde(alias = "path")]
    module_path: Utf8PathBuf,
    #[serde(default)]
    resume_path: Option<Utf8PathBuf>,
    source_offset_utf8: u32,
    #[serde(default)]
    continuation_stack: Vec<VmReplayFrame>,
    output_start_utf8: u32,
    snapshot: tex_vm::VmSnapshot,
}

#[derive(Debug, Clone)]
struct PreviousInternalBuild {
    rev: u64,
    bundle: CheckpointBundle,
    page_metadata: Vec<PageArtifactMeta>,
    output: String,
    sources: BTreeMap<Utf8PathBuf, String>,
    executed_sources: BTreeMap<Utf8PathBuf, String>,
    rewrite_spans: BTreeMap<Utf8PathBuf, Vec<MaterializedRewriteSpan>>,
    module_traces: Vec<StoredModuleTrace>,
    module_checkpoints: Vec<StoredModuleCheckpoint>,
    semantic_aux: Option<SemanticAux>,
    semantic_aux_payload: Option<Vec<u8>>,
    semantic_aux_concrete_payload: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct ShipoutReplayPlan {
    checkpoint: ProjectReplayCheckpoint,
    checkpoint_id: String,
    start_page_index: usize,
    output_prefix: String,
    specificity_rank: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct BuildMeta {
    aux_sensitive: bool,
    dirty_files: Vec<Utf8PathBuf>,
    start_checkpoint_id: Option<String>,
    start_page_index: usize,
    page_count: usize,
    rebuilt_page_count: usize,
    reused_page_count: usize,
    semantic_pass_count: usize,
    semantic_rerun_count: usize,
    semantic_fixpoint_reached: bool,
    semantic_aux_backdated: bool,
}

impl DepTrace {
    pub fn from_inputs(inputs: impl IntoIterator<Item = Utf8PathBuf>) -> Self {
        Self {
            inputs: inputs
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        }
    }
}

impl CompilerDriver {
    pub fn new(compiler_bin: Option<String>, compiler_args: Vec<String>) -> Self {
        Self {
            compiler_bin,
            compiler_args,
        }
    }

    pub async fn compile(&self, request: CompileRequest) -> Result<CompileOutcome, CompileFailure> {
        let rev_dir = request.build_root.join(format!("rev-{}", request.rev));
        tokio::fs::create_dir_all(rev_dir.as_std_path())
            .await
            .map_err(|error| CompileFailure {
                diagnostics: vec![Diagnostic {
                    level: DiagnosticLevel::Error,
                    file: Some(request.toplevel.to_string()),
                    line: None,
                    message: format!("failed to create build directory: {error}"),
                }],
                message: format!("failed to create build directory {}: {error}", rev_dir),
            })?;

        let pdf_path = rev_dir.join(
            Utf8Path::new(request.toplevel.file_stem().unwrap_or("main")).with_extension("pdf"),
        );
        let depfile_path = rev_dir.join("deps.mk");
        let fls_path = rev_dir.join(
            Utf8Path::new(request.toplevel.file_stem().unwrap_or("main")).with_extension("fls"),
        );

        if self.compiler_bin.as_deref() == Some("internal") {
            let world = tex_world::ProjectWorld {
                root: request.root.clone(),
                manifest: request.manifest.clone(),
            };
            let toplevel_source = fs::read_to_string(
                request.root.join(&request.toplevel).as_std_path(),
            )
            .map_err(|error| CompileFailure {
                diagnostics: vec![Diagnostic {
                    level: DiagnosticLevel::Error,
                    file: Some(request.toplevel.to_string()),
                    line: None,
                    message: format!(
                        "failed to read toplevel source {}: {error}",
                        request.root.join(&request.toplevel)
                    ),
                }],
                message: format!(
                    "failed to read toplevel source {}: {error}",
                    request.root.join(&request.toplevel)
                ),
            })?;
            let preamble_key = preamble_key_for_source(&toplevel_source);
            let previous_build =
                load_latest_previous_internal_build(&request.build_root, request.rev).map_err(
                    |error| CompileFailure {
                        diagnostics: vec![Diagnostic {
                            level: DiagnosticLevel::Error,
                            file: Some(request.toplevel.to_string()),
                            line: None,
                            message: format!("failed to load prior internal build state: {error}"),
                        }],
                        message: format!("failed to load prior internal build state: {error}"),
                    },
                )?;
            let aux_scan =
                scan_project(&request.root, &request.toplevel).map_err(|error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to scan semantic aux inputs: {error}"),
                    }],
                    message: format!("failed to scan semantic aux inputs: {error}"),
                })?;
            let aux_sensitive = aux_scan.has_table_of_contents
                || aux_scan.has_float_lists
                || aux_scan.has_bibliography_heading
                || !aux_scan.labels.is_empty()
                || !aux_scan.captions.is_empty()
                || !aux_scan.citations.is_empty()
                || !aux_scan.bibliography_files.is_empty();
            let mut semantic_aux = None::<SemanticAux>;
            let mut materialized_project = None::<MaterializedProject>;
            let mut semantic_pass_count = 0usize;
            let mut semantic_fixpoint_reached = !aux_sensitive;
            let reusable_preamble = previous_build.as_ref().and_then(|previous| {
                select_reusable_preamble(&previous.bundle, &request.changed_files, &preamble_key)
            });
            let mut replay_plan = None;
            let (build, preamble_checkpoint, reused_checkpoint_id) = if aux_sensitive {
                let base_snapshot = compile_mini_kernel_snapshot();
                let mut next_aux = previous_build
                    .as_ref()
                    .and_then(|previous| previous.semantic_aux.clone())
                    .unwrap_or_default();
                let semantic_seed_checkpoint =
                    reusable_preamble
                        .as_ref()
                        .map(|reused_checkpoint| {
                            let snapshot = reused_checkpoint.snapshot.clone().ok_or_else(|| {
                                CompileFailure {
                                    diagnostics: vec![Diagnostic {
                                        level: DiagnosticLevel::Error,
                                        file: Some(request.toplevel.to_string()),
                                        line: None,
                                        message:
                                            "reusable preamble checkpoint does not carry a snapshot"
                                                .to_string(),
                                    }],
                                    message:
                                        "reusable preamble checkpoint does not carry a snapshot"
                                            .to_string(),
                                }
                            })?;
                            Ok::<_, CompileFailure>((
                                ProjectReplayCheckpoint {
                                    snapshot,
                                    resume_path: request.toplevel.clone(),
                                    source_offset_utf8: reused_checkpoint.meta.source_offset_utf8,
                                    continuation_stack: Vec::new(),
                                },
                                reused_checkpoint.meta.checkpoint_id.clone(),
                            ))
                        })
                        .transpose()?;
                let mut final_build = None;
                let mut final_preamble_checkpoint = None;
                let mut final_reused_checkpoint_id = semantic_seed_checkpoint
                    .as_ref()
                    .map(|(_, checkpoint_id)| checkpoint_id.clone());
                for _ in 0..MAX_SEMANTIC_AUX_PASSES {
                    semantic_pass_count += 1;
                    let materialized = materialize_project(
                        &request.root,
                        &request.toplevel,
                        &next_aux,
                    )
                    .map_err(|error| CompileFailure {
                        diagnostics: vec![Diagnostic {
                            level: DiagnosticLevel::Error,
                            file: Some(request.toplevel.to_string()),
                            line: None,
                            message: format!("failed to materialize semantic aux sources: {error}"),
                        }],
                        message: format!("failed to materialize semantic aux sources: {error}"),
                    })?;
                    let (candidate_build, candidate_preamble_checkpoint) = if let Some((
                        checkpoint,
                        _,
                    )) =
                        semantic_seed_checkpoint.as_ref()
                    {
                        (
                                build_project_pdf_from_checkpoint_with_mounts(
                                    &world,
                                    checkpoint,
                                    "",
                                    &materialized.files,
                                )
                                .map_err(|error| CompileFailure {
                                    diagnostics: vec![Diagnostic {
                                        level: DiagnosticLevel::Error,
                                        file: Some(request.toplevel.to_string()),
                                        line: None,
                                        message: format!(
                                            "internal compiler semantic aux replay from preamble checkpoint failed: {error}"
                                        ),
                                    }],
                                    message: format!(
                                        "internal compiler semantic aux replay from preamble checkpoint failed: {error}"
                                    ),
                                })?,
                                checkpoint.clone(),
                            )
                    } else {
                        run_project_pdf_from_base_snapshot_with_mounts(
                            &world,
                            &base_snapshot,
                            &materialized.files,
                        )
                        .map_err(|error| CompileFailure {
                            diagnostics: vec![Diagnostic {
                                level: DiagnosticLevel::Error,
                                file: Some(request.toplevel.to_string()),
                                line: None,
                                message: format!(
                                    "internal compiler semantic aux build failed: {error}"
                                ),
                            }],
                            message: format!(
                                "internal compiler semantic aux build failed: {error}"
                            ),
                        })?
                    };
                    if !candidate_build.run.diagnostics.is_empty() {
                        return Err(internal_diagnostics_failure(
                            &request.toplevel,
                            candidate_build,
                        ));
                    }
                    let derived_aux = derive_semantic_aux(
                        &materialized.scan,
                        &candidate_build
                            .page_metadata
                            .iter()
                            .map(|page| PageSourceSlice {
                                page_index: page.index,
                                source_spans: page
                                    .source_spans
                                    .iter()
                                    .map(|span| SourceSpan {
                                        file: span.file.clone(),
                                        start_utf8: span.start_utf8,
                                        end_utf8: span.end_utf8,
                                    })
                                    .collect(),
                            })
                            .collect::<Vec<_>>(),
                    );
                    let converged = derived_aux.equivalent_to(&next_aux);
                    semantic_aux = Some(derived_aux.clone());
                    materialized_project = Some(materialized);
                    final_build = Some(candidate_build);
                    final_preamble_checkpoint = Some(candidate_preamble_checkpoint);
                    if converged {
                        semantic_fixpoint_reached = true;
                        break;
                    }
                    next_aux = derived_aux;
                }
                if !semantic_fixpoint_reached {
                    return Err(CompileFailure {
                        diagnostics: vec![Diagnostic {
                            level: DiagnosticLevel::Error,
                            file: Some(request.toplevel.to_string()),
                            line: None,
                            message: format!(
                                "semantic aux did not reach a fixpoint within {MAX_SEMANTIC_AUX_PASSES} passes"
                            ),
                        }],
                        message: format!(
                            "semantic aux did not reach a fixpoint within {MAX_SEMANTIC_AUX_PASSES} passes"
                        ),
                    });
                }
                if let (Some(previous), Some(materialized), Some((_, seed_checkpoint_id))) = (
                    previous_build.as_ref(),
                    materialized_project.as_ref(),
                    semantic_seed_checkpoint.as_ref(),
                ) {
                    let changed_bibliography_positions = request
                        .changed_files
                        .iter()
                        .filter_map(|path| {
                            materialized
                                .scan
                                .bibliography_files
                                .iter()
                                .position(|input| input == path)
                        })
                        .collect::<Vec<_>>();
                    let skip_shipout_replay_for_multi_bibliography =
                        materialized.scan.bibliography_files.len() > 1
                            && changed_bibliography_positions.iter().copied().any(|index| {
                                index > 0
                                    && (0..index).any(|earlier| {
                                        !changed_bibliography_positions.contains(&earlier)
                                    })
                            });
                    let force_conservative_replay_for_changed_bibliography =
                        !changed_bibliography_positions.is_empty()
                            && previous
                                .semantic_aux
                                .as_ref()
                                .zip(semantic_aux.as_ref())
                                .is_none_or(|(previous_aux, current_aux)| {
                                    !previous_aux.equivalent_to(current_aux)
                                });
                    let changed_bibliography_files =
                        if force_conservative_replay_for_changed_bibliography {
                            request
                                .changed_files
                                .iter()
                                .filter(|path| materialized.scan.bibliography_files.contains(path))
                                .cloned()
                                .collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        };
                    if force_conservative_replay_for_changed_bibliography {
                        let (replayed_build, replayed_preamble_checkpoint) =
                            run_project_pdf_from_base_snapshot_with_mounts(
                                &world,
                                &base_snapshot,
                                &materialized.files,
                            )
                            .map_err(|error| CompileFailure {
                                diagnostics: vec![Diagnostic {
                                    level: DiagnosticLevel::Error,
                                    file: Some(request.toplevel.to_string()),
                                    line: None,
                                    message: format!(
                                        "internal compiler semantic aux rebuild from base snapshot failed: {error}"
                                    ),
                                }],
                                message: format!(
                                    "internal compiler semantic aux rebuild from base snapshot failed: {error}"
                                ),
                            })?;
                        if !replayed_build.run.diagnostics.is_empty() {
                            return Err(internal_diagnostics_failure(
                                &request.toplevel,
                                replayed_build,
                            ));
                        }
                        final_build = Some(replayed_build);
                        final_preamble_checkpoint = Some(replayed_preamble_checkpoint);
                        final_reused_checkpoint_id = None;
                    } else if !skip_shipout_replay_for_multi_bibliography {
                        if let Some(plan) = select_shipout_replay_plan_with_spans(
                            previous,
                            &request.root,
                            &request.toplevel,
                            &request.changed_files,
                            Some(&materialized.files),
                            Some(&materialized.rewrite_spans),
                            (!changed_bibliography_files.is_empty())
                                .then_some(changed_bibliography_files.as_slice()),
                        )
                        .map_err(|error| CompileFailure {
                            diagnostics: vec![Diagnostic {
                                level: DiagnosticLevel::Error,
                                file: Some(request.toplevel.to_string()),
                                line: None,
                                message: format!(
                                    "failed to choose semantic aux shipout replay checkpoint: {error}"
                                ),
                            }],
                            message: format!(
                                "failed to choose semantic aux shipout replay checkpoint: {error}"
                            ),
                        })? {
                            if plan.checkpoint_id != *seed_checkpoint_id {
                                let replayed_build = build_project_pdf_from_checkpoint_with_mounts(
                                    &world,
                                    &plan.checkpoint,
                                    &plan.output_prefix,
                                    &materialized.files,
                                )
                                .map_err(|error| CompileFailure {
                                    diagnostics: vec![Diagnostic {
                                        level: DiagnosticLevel::Error,
                                        file: Some(request.toplevel.to_string()),
                                        line: None,
                                        message: format!(
                                            "internal compiler semantic aux replay from shipout checkpoint failed: {error}"
                                        ),
                                    }],
                                    message: format!(
                                        "internal compiler semantic aux replay from shipout checkpoint failed: {error}"
                                    ),
                                })?;
                                replay_plan = Some(plan.clone());
                                final_build = Some(replayed_build);
                                final_reused_checkpoint_id = Some(plan.checkpoint_id.clone());
                            }
                        }
                    }
                }
                (
                    final_build.expect("semantic aux build"),
                    final_preamble_checkpoint.expect("semantic aux preamble checkpoint"),
                    final_reused_checkpoint_id,
                )
            } else if let Some(reused_checkpoint) = reusable_preamble.as_ref() {
                let preamble_snapshot =
                    reused_checkpoint
                        .snapshot
                        .clone()
                        .ok_or_else(|| CompileFailure {
                            diagnostics: vec![Diagnostic {
                                level: DiagnosticLevel::Error,
                                file: Some(request.toplevel.to_string()),
                                line: None,
                                message: "reusable preamble checkpoint does not carry a snapshot"
                                    .to_string(),
                            }],
                            message: "reusable preamble checkpoint does not carry a snapshot"
                                .to_string(),
                        })?;
                let preamble_checkpoint = ProjectReplayCheckpoint {
                    snapshot: preamble_snapshot,
                    resume_path: request.toplevel.clone(),
                    source_offset_utf8: reused_checkpoint.meta.source_offset_utf8,
                    continuation_stack: Vec::new(),
                };
                if let Some(previous) = previous_build.as_ref().filter(|previous| {
                    previous
                        .bundle
                        .checkpoints
                        .first()
                        .is_some_and(|checkpoint| checkpoint.meta.boundary_hash == preamble_key)
                }) {
                    if let Some(plan) = select_shipout_replay_plan(
                        previous,
                        &request.root,
                        &request.toplevel,
                        &request.changed_files,
                        None,
                    )
                    .map_err(|error| CompileFailure {
                        diagnostics: vec![Diagnostic {
                            level: DiagnosticLevel::Error,
                            file: Some(request.toplevel.to_string()),
                            line: None,
                            message: format!("failed to choose shipout replay checkpoint: {error}"),
                        }],
                        message: format!("failed to choose shipout replay checkpoint: {error}"),
                    })? {
                        let build = build_project_pdf_from_checkpoint(
                                &world,
                                &plan.checkpoint,
                                &plan.output_prefix,
                            )
                            .map_err(|error| CompileFailure {
                                diagnostics: vec![Diagnostic {
                                    level: DiagnosticLevel::Error,
                                    file: Some(request.toplevel.to_string()),
                                    line: None,
                                    message: format!(
                                        "internal compiler replay from shipout checkpoint failed: {error}"
                                    ),
                                }],
                                message: format!(
                                    "internal compiler replay from shipout checkpoint failed: {error}"
                                ),
                            })?;
                        replay_plan = Some(plan.clone());
                        (build, preamble_checkpoint, Some(plan.checkpoint_id.clone()))
                    } else {
                        let build =
                                build_project_pdf_from_checkpoint(&world, &preamble_checkpoint, "")
                                    .map_err(|error| CompileFailure {
                                        diagnostics: vec![Diagnostic {
                                            level: DiagnosticLevel::Error,
                                            file: Some(request.toplevel.to_string()),
                                            line: None,
                                            message: format!(
                                                "internal compiler replay from preamble checkpoint failed: {error}"
                                            ),
                                        }],
                                        message: format!(
                                            "internal compiler replay from preamble checkpoint failed: {error}"
                                        ),
                                    })?;
                        (
                            build,
                            preamble_checkpoint,
                            Some(reused_checkpoint.meta.checkpoint_id.clone()),
                        )
                    }
                } else {
                    let build =
                            build_project_pdf_from_checkpoint(&world, &preamble_checkpoint, "")
                                .map_err(|error| CompileFailure {
                                    diagnostics: vec![Diagnostic {
                                        level: DiagnosticLevel::Error,
                                        file: Some(request.toplevel.to_string()),
                                        line: None,
                                        message: format!(
                                            "internal compiler replay from preamble checkpoint failed: {error}"
                                        ),
                                    }],
                                    message: format!(
                                        "internal compiler replay from preamble checkpoint failed: {error}"
                                    ),
                                })?;
                    (
                        build,
                        preamble_checkpoint,
                        Some(reused_checkpoint.meta.checkpoint_id.clone()),
                    )
                }
            } else {
                let base_snapshot = compile_mini_kernel_snapshot();
                let (build, preamble_checkpoint) =
                    run_project_pdf_from_base_snapshot(&world, &base_snapshot).map_err(
                        |error| CompileFailure {
                            diagnostics: vec![Diagnostic {
                                level: DiagnosticLevel::Error,
                                file: Some(request.toplevel.to_string()),
                                line: None,
                                message: format!("internal compiler full build failed: {error}"),
                            }],
                            message: format!("internal compiler full build failed: {error}"),
                        },
                    )?;
                (build, preamble_checkpoint, None)
            };
            if !build.run.diagnostics.is_empty() {
                return Err(internal_diagnostics_failure(&request.toplevel, build));
            }
            fs::write(pdf_path.as_std_path(), &build.pdf_bytes).map_err(|error| {
                CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to write internal PDF {}: {error}", pdf_path),
                    }],
                    message: format!("failed to write internal PDF {}: {error}", pdf_path),
                }
            })?;
            let page_dir = rev_dir.join("pages");
            fs::create_dir_all(page_dir.as_std_path()).map_err(|error| CompileFailure {
                diagnostics: vec![Diagnostic {
                    level: DiagnosticLevel::Error,
                    file: Some(request.toplevel.to_string()),
                    line: None,
                    message: format!("failed to create page artifact dir {}: {error}", page_dir),
                }],
                message: format!("failed to create page artifact dir {}: {error}", page_dir),
            })?;
            let checkpoint_pages = build
                .page_metadata
                .iter()
                .map(|page| CheckpointPage {
                    page_id: page.page_id.clone(),
                    index: page.index,
                    content_hash: page.content_hash.clone(),
                    text_start_utf8: page.text_span.start_utf8,
                    text_end_utf8: page.text_span.end_utf8,
                })
                .collect::<Vec<_>>();
            let unchanged_tail = previous_build
                .as_ref()
                .and_then(|previous| find_unchanged_tail(&previous.bundle, &checkpoint_pages))
                .map(|tail| UnchangedTail {
                    previous_rev: tail.previous_rev,
                    resume_checkpoint_id: tail.resume_checkpoint_id,
                    previous_page_start: tail.previous_page_start,
                    current_page_start: tail.current_page_start,
                    page_count: tail.page_count,
                });
            let mut executed_sources = BTreeMap::new();
            if let Some(materialized) = materialized_project.as_ref() {
                executed_sources.extend(materialized.files.clone());
            } else {
                executed_sources.insert(request.toplevel.clone(), toplevel_source.clone());
                for path in &build.run.loaded_modules {
                    let full_path = request.root.join(path);
                    if !full_path.exists() {
                        continue;
                    }
                    if let Ok(source) = fs::read_to_string(full_path.as_std_path()) {
                        executed_sources.insert(path.clone(), source);
                    }
                }
            }
            let mut current_sources = executed_sources.clone();
            if let Some(previous) = previous_build.as_ref() {
                for path in previous.executed_sources.keys() {
                    if current_sources.contains_key(path) {
                        continue;
                    }
                    let full_path = request.root.join(path);
                    if !full_path.exists() {
                        continue;
                    }
                    if let Ok(source) = fs::read_to_string(full_path.as_std_path()) {
                        current_sources.insert(path.clone(), source);
                    }
                }
            }
            let previous_reusable_pages = previous_build.as_ref().map(|previous| {
                (
                    previous.rev,
                    previous
                        .bundle
                        .pages
                        .iter()
                        .map(|page| (page.page_id.clone(), page.content_hash.clone()))
                        .collect::<std::collections::BTreeMap<_, _>>(),
                )
            });
            let current_page_hashes = checkpoint_pages
                .iter()
                .map(|page| (page.page_id.clone(), page.content_hash.clone()))
                .collect::<BTreeMap<_, _>>();
            let mut page_artifacts = Vec::with_capacity(build.layout.pages.len());
            let mut page_pdf_paths = BTreeMap::new();
            let mut reused_page_count = 0usize;
            let mut rebuilt_page_count = 0usize;
            for page in &build.layout.pages {
                let current_hash = current_page_hashes.get(&page.page_id).map(String::as_str);
                let reused_artifact_rev =
                    previous_reusable_pages
                        .as_ref()
                        .and_then(|(previous_rev, pages)| {
                            let previous_hash = pages.get(&page.page_id)?;
                            if Some(previous_hash.as_str()) != current_hash {
                                return None;
                            }
                            Some(*previous_rev)
                        });
                if let Some(previous_rev) = reused_artifact_rev {
                    reused_page_count += 1;
                    page_pdf_paths.insert(
                        page.page_id.clone(),
                        Utf8PathBuf::from(format!("rev-{previous_rev}/pages/{}.pdf", page.page_id)),
                    );
                    page_artifacts.push(PagePreviewArtifact {
                        page_id: page.page_id.clone(),
                        pdf_url: viewer_prefixed_path(&format!(
                            "/artifacts/rev/{previous_rev}/pages/{}.pdf",
                            page.page_id
                        )),
                        svg_url: Some(viewer_prefixed_path(&format!(
                            "/artifacts/rev/{previous_rev}/pages/{}.svg",
                            page.page_id
                        ))),
                    });
                    continue;
                }

                rebuilt_page_count += 1;
                let page_path = page_dir.join(format!("{}.pdf", page.page_id));
                let page_pdf = render_single_page_pdf(page, &build.layout.options);
                let page_svg_path = page_dir.join(format!("{}.svg", page.page_id));
                let page_svg = render_page_svg(page, &build.layout.options);
                fs::write(page_path.as_std_path(), &page_pdf).map_err(|error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to write page artifact {}: {error}", page_path),
                    }],
                    message: format!("failed to write page artifact {}: {error}", page_path),
                })?;
                fs::write(page_svg_path.as_std_path(), page_svg).map_err(|error| {
                    CompileFailure {
                        diagnostics: vec![Diagnostic {
                            level: DiagnosticLevel::Error,
                            file: Some(request.toplevel.to_string()),
                            line: None,
                            message: format!(
                                "failed to write page svg artifact {}: {error}",
                                page_svg_path
                            ),
                        }],
                        message: format!(
                            "failed to write page svg artifact {}: {error}",
                            page_svg_path
                        ),
                    }
                })?;
                page_pdf_paths.insert(
                    page.page_id.clone(),
                    Utf8PathBuf::from(format!("rev-{}/pages/{}.pdf", request.rev, page.page_id)),
                );
                page_artifacts.push(PagePreviewArtifact {
                    page_id: page.page_id.clone(),
                    pdf_url: viewer_prefixed_path(&format!(
                        "/artifacts/rev/{}/pages/{}.pdf",
                        request.rev, page.page_id
                    )),
                    svg_url: Some(viewer_prefixed_path(&format!(
                        "/artifacts/rev/{}/pages/{}.svg",
                        request.rev, page.page_id
                    ))),
                });
            }
            let shipout_checkpoints = if let Some(plan) = replay_plan.as_ref() {
                let previous = previous_build.as_ref().ok_or_else(|| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: "missing previous build state for shipout replay".to_string(),
                    }],
                    message: "missing previous build state for shipout replay".to_string(),
                })?;
                let mut checkpoints = Vec::with_capacity(build.page_metadata.len());
                for page_index in 0..plan.start_page_index {
                    let stored = previous
                        .bundle
                        .checkpoints
                        .iter()
                        .find(|checkpoint| checkpoint.meta.page_index_after == page_index + 1)
                        .and_then(|checkpoint| {
                            replay_checkpoint_from_stored(checkpoint, &request.toplevel)
                        })
                        .ok_or_else(|| CompileFailure {
                            diagnostics: vec![Diagnostic {
                                level: DiagnosticLevel::Error,
                                file: Some(request.toplevel.to_string()),
                                line: None,
                                message: format!(
                                    "missing reusable shipout snapshot for page checkpoint {}",
                                    page_index + 1
                                ),
                            }],
                            message: format!(
                                "missing reusable shipout snapshot for page checkpoint {}",
                                page_index + 1
                            ),
                        })?;
                    checkpoints.push(stored);
                }
                let capture_end = unchanged_tail
                    .as_ref()
                    .map(|tail| tail.current_page_start.max(plan.start_page_index))
                    .unwrap_or(build.page_metadata.len());
                let mut captured = capture_page_checkpoints(
                    &world,
                    &plan.checkpoint,
                    &build.page_metadata[plan.start_page_index..capture_end],
                )
                .map_err(|error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to capture replay checkpoints: {error}"),
                    }],
                    message: format!("failed to capture replay checkpoints: {error}"),
                })?;
                checkpoints.append(&mut captured);
                if let Some(tail) = unchanged_tail.as_ref() {
                    for current_page_index in capture_end..build.page_metadata.len() {
                        let previous_page_index =
                            tail.previous_page_start + current_page_index - tail.current_page_start;
                        let stored = previous
                            .bundle
                            .checkpoints
                            .iter()
                            .find(|checkpoint| {
                                checkpoint.meta.page_index_after == previous_page_index + 1
                            })
                            .ok_or_else(|| CompileFailure {
                                diagnostics: vec![Diagnostic {
                                    level: DiagnosticLevel::Error,
                                    file: Some(request.toplevel.to_string()),
                                    line: None,
                                    message: format!(
                                        "missing reusable unchanged-tail checkpoint {}",
                                        previous_page_index + 1
                                    ),
                                }],
                                message: format!(
                                    "missing reusable unchanged-tail checkpoint {}",
                                    previous_page_index + 1
                                ),
                            })?;
                        let mut reused = replay_checkpoint_from_stored(stored, &request.toplevel)
                            .ok_or_else(|| CompileFailure {
                            diagnostics: vec![Diagnostic {
                                level: DiagnosticLevel::Error,
                                file: Some(request.toplevel.to_string()),
                                line: None,
                                message: format!(
                                    "missing reusable unchanged-tail snapshot for checkpoint {}",
                                    previous_page_index + 1
                                ),
                            }],
                            message: format!(
                                "missing reusable unchanged-tail snapshot for checkpoint {}",
                                previous_page_index + 1
                            ),
                        })?;
                        rebase_reused_shipout_checkpoint(
                            &previous.sources,
                            &previous.module_traces,
                            &previous.module_checkpoints,
                            &current_sources,
                            &previous.page_metadata,
                            previous_page_index,
                            &build.page_metadata,
                            current_page_index,
                            &mut reused,
                        );
                        checkpoints.push(reused);
                    }
                }
                checkpoints
            } else {
                let capture_end = unchanged_tail
                    .as_ref()
                    .map(|tail| tail.current_page_start)
                    .unwrap_or(build.page_metadata.len());
                let mut checkpoints = capture_page_checkpoints(
                    &world,
                    &preamble_checkpoint,
                    &build.page_metadata[..capture_end],
                )
                .map_err(|error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to capture page checkpoints: {error}"),
                    }],
                    message: format!("failed to capture page checkpoints: {error}"),
                })?;
                if let Some((previous, tail)) = previous_build.as_ref().zip(unchanged_tail.as_ref())
                {
                    for current_page_index in capture_end..build.page_metadata.len() {
                        let previous_page_index =
                            tail.previous_page_start + current_page_index - tail.current_page_start;
                        let stored = previous
                            .bundle
                            .checkpoints
                            .iter()
                            .find(|checkpoint| {
                                checkpoint.meta.page_index_after == previous_page_index + 1
                            })
                            .ok_or_else(|| CompileFailure {
                                diagnostics: vec![Diagnostic {
                                    level: DiagnosticLevel::Error,
                                    file: Some(request.toplevel.to_string()),
                                    line: None,
                                    message: format!(
                                        "missing reusable unchanged-tail checkpoint {}",
                                        previous_page_index + 1
                                    ),
                                }],
                                message: format!(
                                    "missing reusable unchanged-tail checkpoint {}",
                                    previous_page_index + 1
                                ),
                            })?;
                        let mut reused = replay_checkpoint_from_stored(stored, &request.toplevel)
                            .ok_or_else(|| CompileFailure {
                            diagnostics: vec![Diagnostic {
                                level: DiagnosticLevel::Error,
                                file: Some(request.toplevel.to_string()),
                                line: None,
                                message: format!(
                                    "missing reusable unchanged-tail snapshot for checkpoint {}",
                                    previous_page_index + 1
                                ),
                            }],
                            message: format!(
                                "missing reusable unchanged-tail snapshot for checkpoint {}",
                                previous_page_index + 1
                            ),
                        })?;
                        rebase_reused_shipout_checkpoint(
                            &previous.sources,
                            &previous.module_traces,
                            &previous.module_checkpoints,
                            &current_sources,
                            &previous.page_metadata,
                            previous_page_index,
                            &build.page_metadata,
                            current_page_index,
                            &mut reused,
                        );
                        checkpoints.push(reused);
                    }
                }
                checkpoints
            };
            let checkpoint_bundle = build_checkpoint_bundle_with_shipouts(
                request.rev,
                &preamble_checkpoint.snapshot,
                &preamble_key,
                preamble_checkpoint.source_offset_utf8,
                &checkpoint_pages,
                &shipout_checkpoints
                    .iter()
                    .map(|checkpoint| ShipoutCheckpoint {
                        snapshot: checkpoint.snapshot.clone(),
                        source_offset_utf8: checkpoint.source_offset_utf8,
                        resume_path: Some(checkpoint.resume_path.clone()),
                        continuation_stack: checkpoint.continuation_stack.clone(),
                    })
                    .collect::<Vec<_>>(),
                &build
                    .run
                    .module_checkpoints
                    .iter()
                    .map(|checkpoint| InputBoundaryCheckpoint {
                        kind: checkpoint.kind,
                        module_path: checkpoint.module_path.clone(),
                        resume_path: checkpoint.resume_path.clone(),
                        source_offset_utf8: checkpoint.source_offset_utf8,
                        continuation_stack: checkpoint.continuation_stack.clone(),
                        output_start_utf8: checkpoint.output_start_utf8,
                        page_index_after: build
                            .page_metadata
                            .iter()
                            .find(|page| page.text_span.end_utf8 > checkpoint.output_start_utf8)
                            .map(|page| page.index)
                            .unwrap_or_default(),
                        snapshot: checkpoint.snapshot.clone(),
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(|error| CompileFailure {
                diagnostics: vec![Diagnostic {
                    level: DiagnosticLevel::Error,
                    file: Some(request.toplevel.to_string()),
                    line: None,
                    message: format!("failed to build checkpoint bundle: {error}"),
                }],
                message: format!("failed to build checkpoint bundle: {error}"),
            })?;
            let page_metadata = build
                .page_metadata
                .iter()
                .map(|page| PageArtifactMeta {
                    page_id: page.page_id.clone(),
                    index: page.index,
                    line_count: page.line_count,
                    width_pt: page.width_pt.round() as u32,
                    height_pt: page.height_pt.round() as u32,
                    content_hash: page.content_hash.clone(),
                    text_start_utf8: page.text_span.start_utf8,
                    text_end_utf8: page.text_span.end_utf8,
                    pdf_artifact_path: page_pdf_paths.get(&page.page_id).cloned().unwrap_or_else(
                        || {
                            Utf8PathBuf::from(format!(
                                "rev-{}/pages/{}.pdf",
                                request.rev, page.page_id
                            ))
                        },
                    ),
                    source_spans: page
                        .source_spans
                        .iter()
                        .map(|span| ArtifactSourceSpan {
                            file: span.file.clone(),
                            start_utf8: span.start_utf8,
                            end_utf8: span.end_utf8,
                        })
                        .collect(),
                })
                .collect::<Vec<_>>();
            let page_syncmap = build
                .page_metadata
                .iter()
                .map(|page| PageSyncMapArtifact {
                    page_id: page.page_id.clone(),
                    index: page.index,
                    width_pt: page.width_pt.round() as u32,
                    height_pt: page.height_pt.round() as u32,
                    items: {
                        let mut items = Vec::new();
                        if let Some(layout_page) = build.layout.pages.get(page.index) {
                            let char_width_pt = (((page.width_pt.round() as f32)
                                - PAGE_TEXT_LEFT_PT * 2.0)
                                .max(1.0)
                                / build.layout.options.chars_per_line.max(1) as f32)
                                .max(1.0);
                            let max_right = ((page.width_pt.round() as f32) - PAGE_TEXT_LEFT_PT)
                                .max(PAGE_TEXT_LEFT_PT + char_width_pt);
                            let mut line_start_utf8 = 0u32;
                            let mut line_boxes = Vec::with_capacity(layout_page.lines.len());
                            for (line_index, line) in layout_page.lines.iter().enumerate() {
                                let line_len_utf8 = line.len() as u32;
                                let line_end_utf8 = line_start_utf8 + line_len_utf8;
                                let line_top = (PAGE_TEXT_TOP_PT
                                    + PAGE_LINE_HEIGHT_PT * line_index as f32
                                    - PAGE_FONT_SIZE_PT)
                                    .max(0.0);
                                let line_bottom =
                                    (line_top + PAGE_LINE_HEIGHT_PT).min(page.height_pt as f32);
                                let visible_chars = line.chars().count().max(1) as f32;
                                let line_right = (PAGE_TEXT_LEFT_PT
                                    + visible_chars * char_width_pt)
                                    .min(max_right)
                                    .max(PAGE_TEXT_LEFT_PT + char_width_pt);
                                line_boxes.push((
                                    line_start_utf8,
                                    line_end_utf8,
                                    PAGE_TEXT_LEFT_PT,
                                    line_right,
                                    line_top,
                                    line_bottom,
                                ));
                                line_start_utf8 = line_end_utf8.saturating_add(1);
                            }
                            for span in &page.sync_spans {
                                let span_output_len = span
                                    .output_end_utf8
                                    .saturating_sub(span.output_start_utf8)
                                    .max(1);
                                let span_source_len =
                                    span.end_utf8.saturating_sub(span.start_utf8).max(1);
                                let mut span_items = 0usize;
                                for (
                                    line_start_utf8,
                                    line_end_utf8,
                                    line_left,
                                    line_right,
                                    line_top,
                                    line_bottom,
                                ) in &line_boxes
                                {
                                    if *line_end_utf8 <= span.output_start_utf8
                                        || *line_start_utf8 >= span.output_end_utf8
                                    {
                                        continue;
                                    }
                                    let overlap_start =
                                        (*line_start_utf8).max(span.output_start_utf8);
                                    let overlap_end = (*line_end_utf8).min(span.output_end_utf8);
                                    if overlap_end <= overlap_start {
                                        continue;
                                    }
                                    let start_utf8 = span.start_utf8
                                        + ((span_source_len as u64
                                            * (overlap_start - span.output_start_utf8) as u64)
                                            / span_output_len as u64)
                                            as u32;
                                    let end_utf8 = if overlap_end == span.output_end_utf8 {
                                        span.end_utf8
                                    } else {
                                        span.start_utf8
                                            + ((span_source_len as u64
                                                * (overlap_end - span.output_start_utf8) as u64)
                                                / span_output_len as u64)
                                                as u32
                                    };
                                    let line_len_utf8 =
                                        line_end_utf8.saturating_sub(*line_start_utf8).max(1);
                                    let line_width = (line_right - line_left).max(char_width_pt);
                                    let left = line_left
                                        + line_width
                                            * ((overlap_start - *line_start_utf8) as f32
                                                / line_len_utf8 as f32);
                                    let right = if overlap_end == *line_end_utf8 {
                                        *line_right
                                    } else {
                                        line_left
                                            + line_width
                                                * ((overlap_end - *line_start_utf8) as f32
                                                    / line_len_utf8 as f32)
                                    };
                                    items.push(ArtifactSyncSpan {
                                        file: span.file.clone(),
                                        start_utf8,
                                        end_utf8,
                                        output_start_utf8: overlap_start,
                                        output_end_utf8: overlap_end,
                                        left_px: left.round() as u32,
                                        right_px: right.max(left + 1.0).round() as u32,
                                        top_px: line_top.round() as u32,
                                        bottom_px: line_bottom.max(line_top + 1.0).round() as u32,
                                    });
                                    span_items += 1;
                                }
                                if span_items == 0 {
                                    items.push(ArtifactSyncSpan {
                                        file: span.file.clone(),
                                        start_utf8: span.start_utf8,
                                        end_utf8: span.end_utf8,
                                        output_start_utf8: span.output_start_utf8,
                                        output_end_utf8: span.output_end_utf8,
                                        left_px: 0,
                                        right_px: page.width_pt.round() as u32,
                                        top_px: 0,
                                        bottom_px: page.height_pt.round() as u32,
                                    });
                                }
                            }
                        } else {
                            items.extend(page.sync_spans.iter().map(|span| ArtifactSyncSpan {
                                file: span.file.clone(),
                                start_utf8: span.start_utf8,
                                end_utf8: span.end_utf8,
                                output_start_utf8: span.output_start_utf8,
                                output_end_utf8: span.output_end_utf8,
                                left_px: 0,
                                right_px: page.width_pt.round() as u32,
                                top_px: 0,
                                bottom_px: page.height_pt.round() as u32,
                            }));
                        }
                        items.sort_by_key(|item| {
                            (item.output_start_utf8, item.top_px, item.left_px)
                        });
                        items
                    },
                })
                .collect::<Vec<_>>();
            let serialized_page_metadata =
                serde_json::to_vec_pretty(&page_metadata).map_err(|error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to serialize page metadata: {error}"),
                    }],
                    message: format!("failed to serialize page metadata: {error}"),
                })?;
            let serialized_page_syncmap =
                serde_json::to_vec_pretty(&page_syncmap).map_err(|error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to serialize page syncmap: {error}"),
                    }],
                    message: format!("failed to serialize page syncmap: {error}"),
                })?;
            let page_metadata_path = rev_dir.join("page-metadata.json");
            fs::write(page_metadata_path.as_std_path(), serialized_page_metadata).map_err(
                |error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!(
                            "failed to write page metadata {}: {error}",
                            page_metadata_path
                        ),
                    }],
                    message: format!(
                        "failed to write page metadata {}: {error}",
                        page_metadata_path
                    ),
                },
            )?;
            let page_syncmap_path = rev_dir.join("page-syncmap.json");
            fs::write(page_syncmap_path.as_std_path(), serialized_page_syncmap).map_err(
                |error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!(
                            "failed to write page syncmap {}: {error}",
                            page_syncmap_path
                        ),
                    }],
                    message: format!(
                        "failed to write page syncmap {}: {error}",
                        page_syncmap_path
                    ),
                },
            )?;
            let output_path = rev_dir.join("output.txt");
            fs::write(output_path.as_std_path(), &build.run.output).map_err(|error| {
                CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to write replay output {}: {error}", output_path),
                    }],
                    message: format!("failed to write replay output {}: {error}", output_path),
                }
            })?;
            let page_patches = previous_build
                .as_ref()
                .map(|previous| {
                    plan_page_patches(
                        &previous.bundle.pages,
                        &checkpoint_pages,
                        &page_artifacts,
                        unchanged_tail.as_ref(),
                    )
                })
                .unwrap_or_default();
            let checkpoint_path = rev_dir.join("checkpoints.json");
            save_checkpoint_bundle(&checkpoint_path, &checkpoint_bundle).map_err(|error| {
                CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to save checkpoints {}: {error}", checkpoint_path),
                    }],
                    message: format!("failed to save checkpoints {}: {error}", checkpoint_path),
                }
            })?;
            let mut tracked_inputs = materialized_project
                .as_ref()
                .map(|materialized| materialized.tracked_inputs.clone())
                .unwrap_or_else(|| {
                    let mut tracked_inputs = build.run.loaded_modules.clone();
                    tracked_inputs.push(request.toplevel.clone());
                    tracked_inputs
                });
            tracked_inputs.sort();
            tracked_inputs.dedup();
            let source_texts_path = rev_dir.join("sources.json");
            let empty_rewrite_spans = BTreeMap::new();
            save_source_texts(
                &source_texts_path,
                &request.root,
                &tracked_inputs,
                &executed_sources,
                materialized_project
                    .as_ref()
                    .map(|materialized| &materialized.rewrite_spans)
                    .unwrap_or(&empty_rewrite_spans),
                &build
                    .run
                    .module_traces
                    .iter()
                    .map(|trace| StoredModuleTrace {
                        path: trace.path.clone(),
                        source_start_utf8: trace.source_start_utf8,
                        source_end_utf8: trace.source_end_utf8,
                        output_start_utf8: trace.output_start_utf8,
                        output_end_utf8: trace.output_end_utf8,
                    })
                    .collect::<Vec<_>>(),
                &build
                    .run
                    .module_checkpoints
                    .iter()
                    .map(|checkpoint| StoredModuleCheckpoint {
                        kind: checkpoint.kind,
                        module_path: checkpoint.module_path.clone(),
                        resume_path: checkpoint.resume_path.clone(),
                        source_offset_utf8: checkpoint.source_offset_utf8,
                        continuation_stack: checkpoint.continuation_stack.clone(),
                        output_start_utf8: checkpoint.output_start_utf8,
                        snapshot: checkpoint.snapshot.clone(),
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(|error| CompileFailure {
                diagnostics: vec![Diagnostic {
                    level: DiagnosticLevel::Error,
                    file: Some(request.toplevel.to_string()),
                    line: None,
                    message: format!(
                        "failed to save source snapshots {}: {error}",
                        source_texts_path
                    ),
                }],
                message: format!(
                    "failed to save source snapshots {}: {error}",
                    source_texts_path
                ),
            })?;
            let mut semantic_aux_backdated = false;
            if let Some(aux) = semantic_aux.as_ref() {
                let aux_path = rev_dir.join("aux.json");
                let concrete_aux_path = rev_dir.join("semantic.aux");
                let previous_aux_payload = previous_build
                    .as_ref()
                    .and_then(|previous| previous.semantic_aux_payload.as_deref());
                let previous_concrete_aux_payload = previous_build
                    .as_ref()
                    .and_then(|previous| previous.semantic_aux_concrete_payload.as_deref());
                let aux_payload =
                    serialize_semantic_aux_backdated_with_previous(previous_aux_payload, aux)
                        .map_err(|error| CompileFailure {
                            diagnostics: vec![Diagnostic {
                                level: DiagnosticLevel::Error,
                                file: Some(request.toplevel.to_string()),
                                line: None,
                                message: format!("failed to serialize semantic aux: {error}"),
                            }],
                            message: format!("failed to serialize semantic aux: {error}"),
                        })?;
                semantic_aux_backdated = previous_aux_payload
                    .is_some_and(|previous_payload| previous_payload == aux_payload.as_slice());
                fs::write(aux_path.as_std_path(), aux_payload).map_err(|error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to write semantic aux {}: {error}", aux_path),
                    }],
                    message: format!("failed to write semantic aux {}: {error}", aux_path),
                })?;
                let concrete_aux_payload = serialize_concrete_semantic_aux_backdated_with_previous(
                    previous_concrete_aux_payload,
                    aux,
                )
                .map_err(|error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to serialize concrete semantic aux: {error}"),
                    }],
                    message: format!("failed to serialize concrete semantic aux: {error}"),
                })?;
                fs::write(concrete_aux_path.as_std_path(), concrete_aux_payload).map_err(
                    |error| CompileFailure {
                        diagnostics: vec![Diagnostic {
                            level: DiagnosticLevel::Error,
                            file: Some(request.toplevel.to_string()),
                            line: None,
                            message: format!(
                                "failed to write concrete semantic aux {}: {error}",
                                concrete_aux_path
                            ),
                        }],
                        message: format!(
                            "failed to write concrete semantic aux {}: {error}",
                            concrete_aux_path
                        ),
                    },
                )?;
                let semantic_index_path = rev_dir.join("semantic-index.json");
                let semantic_index = derive_semantic_aux_index(
                    materialized_project
                        .as_ref()
                        .map(|materialized| &materialized.scan)
                        .unwrap_or(&aux_scan),
                    aux,
                );
                let semantic_index_payload =
                    serde_json::to_vec_pretty(&semantic_index).map_err(|error| CompileFailure {
                        diagnostics: vec![Diagnostic {
                            level: DiagnosticLevel::Error,
                            file: Some(request.toplevel.to_string()),
                            line: None,
                            message: format!("failed to serialize semantic aux index: {error}"),
                        }],
                        message: format!("failed to serialize semantic aux index: {error}"),
                    })?;
                fs::write(semantic_index_path.as_std_path(), semantic_index_payload).map_err(
                    |error| CompileFailure {
                        diagnostics: vec![Diagnostic {
                            level: DiagnosticLevel::Error,
                            file: Some(request.toplevel.to_string()),
                            line: None,
                            message: format!(
                                "failed to write semantic aux index {}: {error}",
                                semantic_index_path
                            ),
                        }],
                        message: format!(
                            "failed to write semantic aux index {}: {error}",
                            semantic_index_path
                        ),
                    },
                )?;
            }
            let build_meta_path = rev_dir.join("build-meta.json");
            let build_meta = BuildMeta {
                aux_sensitive,
                dirty_files: request.changed_files.clone(),
                start_checkpoint_id: reused_checkpoint_id.clone(),
                start_page_index: replay_plan
                    .as_ref()
                    .map(|plan| plan.start_page_index)
                    .unwrap_or_default(),
                page_count: build.page_metadata.len(),
                rebuilt_page_count,
                reused_page_count,
                semantic_pass_count,
                semantic_rerun_count: semantic_pass_count.saturating_sub(1),
                semantic_fixpoint_reached,
                semantic_aux_backdated,
            };
            let serialized_build_meta =
                serde_json::to_vec_pretty(&build_meta).map_err(|error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to serialize build metadata: {error}"),
                    }],
                    message: format!("failed to serialize build metadata: {error}"),
                })?;
            fs::write(build_meta_path.as_std_path(), serialized_build_meta).map_err(|error| {
                CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!(
                            "failed to write build metadata {}: {error}",
                            build_meta_path
                        ),
                    }],
                    message: format!(
                        "failed to write build metadata {}: {error}",
                        build_meta_path
                    ),
                }
            })?;
            return Ok(CompileOutcome {
                pdf_path,
                diagnostics: Vec::new(),
                dep_trace: DepTrace::from_inputs(tracked_inputs),
                page_artifacts,
                page_metadata,
                reused_checkpoint_id,
                unchanged_tail,
                page_patches,
            });
        }

        let main_stem = request.toplevel.file_stem().unwrap_or("main");
        let dvi_path = rev_dir.join(format!("{main_stem}.dvi"));
        let ps_path = rev_dir.join(format!("{main_stem}.ps"));
        let mut commands = Vec::new();
        if let Some(compiler_bin) = &self.compiler_bin {
            let mut materialized_args = Vec::with_capacity(self.compiler_args.len());
            for argument in &self.compiler_args {
                materialized_args.push(
                    argument
                        .replace("{root}", request.root.as_str())
                        .replace("{main}", request.toplevel.as_str())
                        .replace("{out_dir}", rev_dir.as_str())
                        .replace("{out_pdf}", pdf_path.as_str())
                        .replace("{depfile}", depfile_path.as_str())
                        .replace("{fls}", fls_path.as_str())
                        .replace("{rev}", &request.rev.to_string()),
                );
            }
            commands.push((
                compiler_bin.clone(),
                materialized_args,
                Some(pdf_path.clone()),
                "PDF",
            ));
        } else {
            match request.manifest.compiler {
                CompilerMode::PdfLatex => {
                    if let Ok(program) = which::which("tectonic") {
                        commands.push((
                            program.to_string_lossy().into_owned(),
                            vec![
                                "-X".to_string(),
                                "compile".to_string(),
                                "--makefile-rules".to_string(),
                                depfile_path.to_string(),
                                "--keep-logs".to_string(),
                                "--keep-intermediates".to_string(),
                                "--outdir".to_string(),
                                rev_dir.to_string(),
                                request.toplevel.to_string(),
                            ],
                            Some(pdf_path.clone()),
                            "PDF",
                        ));
                    } else if let Ok(program) = which::which("pdflatex") {
                        commands.push((
                            program.to_string_lossy().into_owned(),
                            vec![
                                "-interaction=nonstopmode".to_string(),
                                "-halt-on-error".to_string(),
                                "-recorder".to_string(),
                                "-output-directory".to_string(),
                                rev_dir.to_string(),
                                request.toplevel.to_string(),
                            ],
                            Some(pdf_path.clone()),
                            "PDF",
                        ));
                    } else {
                        return Err(CompileFailure {
                            diagnostics: vec![Diagnostic {
                                level: DiagnosticLevel::Error,
                                file: Some(request.toplevel.to_string()),
                                line: None,
                                message: "no TeX compiler found; pass --compiler-bin to configure an external oracle compiler".to_string(),
                            }],
                            message: "no TeX compiler found on PATH".to_string(),
                        });
                    }
                }
                CompilerMode::XeLatex => {
                    if let Ok(program) = which::which("xelatex") {
                        commands.push((
                            program.to_string_lossy().into_owned(),
                            vec![
                                "-interaction=nonstopmode".to_string(),
                                "-halt-on-error".to_string(),
                                "-recorder".to_string(),
                                "-output-directory".to_string(),
                                rev_dir.to_string(),
                                request.toplevel.to_string(),
                            ],
                            Some(pdf_path.clone()),
                            "PDF",
                        ));
                    } else {
                        return Err(CompileFailure {
                            diagnostics: vec![Diagnostic {
                                level: DiagnosticLevel::Error,
                                file: Some(request.toplevel.to_string()),
                                line: None,
                                message: "xelatex is not installed; pass --compiler-bin to configure a compatible compiler".to_string(),
                            }],
                            message: "xelatex is not installed".to_string(),
                        });
                    }
                }
                CompilerMode::LatexDvipsPs2Pdf => {
                    let latex_program = which::which("latex").map_err(|_| CompileFailure {
                        diagnostics: vec![Diagnostic {
                            level: DiagnosticLevel::Error,
                            file: Some(request.toplevel.to_string()),
                            line: None,
                            message: "latex is not installed; pass --compiler-bin to configure a compatible compiler".to_string(),
                        }],
                        message: "latex is not installed".to_string(),
                    })?;
                    let dvips_program = which::which("dvips").map_err(|_| CompileFailure {
                        diagnostics: vec![Diagnostic {
                            level: DiagnosticLevel::Error,
                            file: Some(request.toplevel.to_string()),
                            line: None,
                            message: "dvips is not installed; install a DVI-to-PostScript toolchain or pass --compiler-bin".to_string(),
                        }],
                        message: "dvips is not installed".to_string(),
                    })?;
                    let ps2pdf_program = which::which("ps2pdf").map_err(|_| CompileFailure {
                        diagnostics: vec![Diagnostic {
                            level: DiagnosticLevel::Error,
                            file: Some(request.toplevel.to_string()),
                            line: None,
                            message: "ps2pdf is not installed; install a PostScript-to-PDF toolchain or pass --compiler-bin".to_string(),
                        }],
                        message: "ps2pdf is not installed".to_string(),
                    })?;
                    commands.push((
                        latex_program.to_string_lossy().into_owned(),
                        vec![
                            "-interaction=nonstopmode".to_string(),
                            "-halt-on-error".to_string(),
                            "-recorder".to_string(),
                            "-output-directory".to_string(),
                            rev_dir.to_string(),
                            request.toplevel.to_string(),
                        ],
                        Some(dvi_path.clone()),
                        "DVI",
                    ));
                    commands.push((
                        dvips_program.to_string_lossy().into_owned(),
                        vec!["-o".to_string(), ps_path.to_string(), dvi_path.to_string()],
                        Some(ps_path.clone()),
                        "PostScript",
                    ));
                    commands.push((
                        ps2pdf_program.to_string_lossy().into_owned(),
                        vec![ps_path.to_string(), pdf_path.to_string()],
                        Some(pdf_path.clone()),
                        "PDF",
                    ));
                }
            }
        }

        let mut diagnostics = Vec::new();
        for (program, args, expected_output_path, expected_output_kind) in commands {
            let output = tokio::process::Command::new(&program)
                .args(&args)
                .current_dir(request.root.as_std_path())
                .output()
                .await
                .map_err(|error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to spawn compiler `{program}`: {error}"),
                    }],
                    message: format!("failed to spawn compiler `{program}`: {error}"),
                })?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !output.status.success() {
                let details = if stderr.trim().is_empty() {
                    stdout.lines().rev().take(8).collect::<Vec<_>>().join("\n")
                } else {
                    stderr.lines().rev().take(8).collect::<Vec<_>>().join("\n")
                };
                return Err(CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: if details.is_empty() {
                            format!("compiler `{program}` exited with status {}", output.status)
                        } else {
                            details
                        },
                    }],
                    message: format!("compiler `{program}` exited with status {}", output.status),
                });
            }

            for line in stderr.lines().chain(stdout.lines()) {
                let lower = line.to_ascii_lowercase();
                if lower.contains("warning") {
                    diagnostics.push(Diagnostic {
                        level: DiagnosticLevel::Warning,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: line.to_string(),
                    });
                }
            }

            if let Some(expected_output_path) = expected_output_path {
                if !expected_output_path.exists() {
                    return Err(CompileFailure {
                        diagnostics: vec![Diagnostic {
                            level: DiagnosticLevel::Error,
                            file: Some(request.toplevel.to_string()),
                            line: None,
                            message: format!(
                                "compiler `{program}` succeeded but did not produce expected {expected_output_kind} {}",
                                expected_output_path
                            ),
                        }],
                        message: format!(
                            "expected {expected_output_kind} {} was not created",
                            expected_output_path
                        ),
                    });
                }
            }
        }

        if !pdf_path.exists() {
            return Err(CompileFailure {
                diagnostics: vec![Diagnostic {
                    level: DiagnosticLevel::Error,
                    file: Some(request.toplevel.to_string()),
                    line: None,
                    message: format!(
                        "compiler succeeded but did not produce expected PDF {}",
                        pdf_path
                    ),
                }],
                message: format!("expected PDF {} was not created", pdf_path),
            });
        }

        let dep_trace = if depfile_path.exists() {
            let contents =
                fs::read_to_string(depfile_path.as_std_path()).map_err(|error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to read depfile {}: {error}", depfile_path),
                    }],
                    message: format!("failed to read depfile {}: {error}", depfile_path),
                })?;
            parse_depfile(&request.root, &contents)
        } else if fls_path.exists() {
            let contents =
                fs::read_to_string(fls_path.as_std_path()).map_err(|error| CompileFailure {
                    diagnostics: vec![Diagnostic {
                        level: DiagnosticLevel::Error,
                        file: Some(request.toplevel.to_string()),
                        line: None,
                        message: format!("failed to read recorder file {}: {error}", fls_path),
                    }],
                    message: format!("failed to read recorder file {}: {error}", fls_path),
                })?;
            parse_fls(&request.root, &contents)
        } else {
            DepTrace::from_inputs([request.toplevel.clone()])
        };
        let dep_trace = if dep_trace
            .inputs
            .iter()
            .any(|path| path == &request.toplevel)
        {
            dep_trace
        } else {
            DepTrace::from_inputs(
                dep_trace
                    .inputs
                    .into_iter()
                    .chain([request.toplevel.clone()]),
            )
        };
        let build_meta_path = rev_dir.join("build-meta.json");
        let build_meta = BuildMeta {
            aux_sensitive: false,
            dirty_files: request.changed_files.clone(),
            start_checkpoint_id: None,
            start_page_index: 0,
            page_count: 0,
            rebuilt_page_count: 0,
            reused_page_count: 0,
            semantic_pass_count: 0,
            semantic_rerun_count: 0,
            semantic_fixpoint_reached: false,
            semantic_aux_backdated: false,
        };
        let serialized_build_meta =
            serde_json::to_vec_pretty(&build_meta).map_err(|error| CompileFailure {
                diagnostics: vec![Diagnostic {
                    level: DiagnosticLevel::Error,
                    file: Some(request.toplevel.to_string()),
                    line: None,
                    message: format!("failed to serialize build metadata: {error}"),
                }],
                message: format!("failed to serialize build metadata: {error}"),
            })?;
        fs::write(build_meta_path.as_std_path(), serialized_build_meta).map_err(|error| {
            CompileFailure {
                diagnostics: vec![Diagnostic {
                    level: DiagnosticLevel::Error,
                    file: Some(request.toplevel.to_string()),
                    line: None,
                    message: format!(
                        "failed to write build metadata {}: {error}",
                        build_meta_path
                    ),
                }],
                message: format!(
                    "failed to write build metadata {}: {error}",
                    build_meta_path
                ),
            }
        })?;

        Ok(CompileOutcome {
            pdf_path,
            diagnostics,
            dep_trace,
            page_artifacts: Vec::new(),
            page_metadata: Vec::new(),
            reused_checkpoint_id: None,
            unchanged_tail: None,
            page_patches: Vec::new(),
        })
    }
}

fn internal_diagnostics_failure(toplevel: &Utf8Path, build: ProjectPdfBuild) -> CompileFailure {
    CompileFailure {
        diagnostics: build
            .run
            .diagnostics
            .into_iter()
            .map(|diagnostic| Diagnostic {
                level: DiagnosticLevel::Error,
                file: Some(toplevel.to_string()),
                line: None,
                message: diagnostic.detail,
            })
            .collect(),
        message: "internal compiler reported diagnostics".to_string(),
    }
}

fn load_latest_previous_internal_build(
    build_root: &Utf8Path,
    current_rev: u64,
) -> anyhow::Result<Option<PreviousInternalBuild>> {
    if current_rev <= 1 {
        return Ok(None);
    }

    for previous_rev in (1..current_rev).rev() {
        let rev_dir = build_root.join(format!("rev-{previous_rev}"));
        let checkpoint_path = rev_dir.join("checkpoints.json");
        let page_metadata_path = rev_dir.join("page-metadata.json");
        let output_path = rev_dir.join("output.txt");
        let sources_path = rev_dir.join("sources.json");
        let aux_path = rev_dir.join("aux.json");
        let concrete_aux_path = rev_dir.join("semantic.aux");
        if !checkpoint_path.exists()
            || !page_metadata_path.exists()
            || !output_path.exists()
            || !sources_path.exists()
        {
            continue;
        }
        let bundle = load_checkpoint_bundle(&checkpoint_path)
            .with_context(|| format!("failed to load {checkpoint_path}"))?;
        let page_metadata = serde_json::from_slice::<Vec<PageArtifactMeta>>(
            &fs::read(page_metadata_path.as_std_path())
                .with_context(|| format!("failed to read {page_metadata_path}"))?,
        )
        .with_context(|| format!("failed to parse {page_metadata_path}"))?;
        let output = fs::read_to_string(output_path.as_std_path())
            .with_context(|| format!("failed to read {output_path}"))?;
        let sources = serde_json::from_slice::<StoredSourceTexts>(
            &fs::read(sources_path.as_std_path())
                .with_context(|| format!("failed to read {sources_path}"))?,
        )
        .with_context(|| format!("failed to parse {sources_path}"))?;
        let semantic_aux_payload = if aux_path.exists() {
            Some(
                fs::read(aux_path.as_std_path())
                    .with_context(|| format!("failed to read {aux_path}"))?,
            )
        } else {
            None
        };
        let semantic_aux_concrete_payload = if concrete_aux_path.exists() {
            Some(
                fs::read(concrete_aux_path.as_std_path())
                    .with_context(|| format!("failed to read {concrete_aux_path}"))?,
            )
        } else {
            None
        };
        let raw_sources = sources.files;
        let executed_sources = if sources.executed_files.is_empty() {
            raw_sources.clone()
        } else {
            sources.executed_files
        };
        return Ok(Some(PreviousInternalBuild {
            rev: previous_rev,
            bundle,
            page_metadata,
            output,
            sources: raw_sources,
            executed_sources,
            rewrite_spans: sources.rewrite_spans,
            module_traces: sources.module_traces,
            module_checkpoints: sources.module_checkpoints,
            semantic_aux: if let Some(payload) = semantic_aux_payload.as_deref() {
                Some(
                    serde_json::from_slice::<SemanticAux>(payload)
                        .with_context(|| format!("failed to parse {aux_path}"))?,
                )
            } else if let Some(payload) = semantic_aux_concrete_payload.as_deref() {
                Some(
                    parse_concrete_semantic_aux(payload)
                        .with_context(|| format!("failed to parse {concrete_aux_path}"))?,
                )
            } else {
                None
            },
            semantic_aux_payload,
            semantic_aux_concrete_payload,
        }));
    }

    Ok(None)
}

fn save_source_texts(
    path: &Utf8Path,
    root: &Utf8Path,
    inputs: &[Utf8PathBuf],
    executed_files: &BTreeMap<Utf8PathBuf, String>,
    rewrite_spans: &BTreeMap<Utf8PathBuf, Vec<MaterializedRewriteSpan>>,
    module_traces: &[StoredModuleTrace],
    module_checkpoints: &[StoredModuleCheckpoint],
) -> anyhow::Result<()> {
    let mut files = BTreeMap::new();
    for input in inputs {
        let full_path = root.join(input);
        if !full_path.exists() {
            continue;
        }
        if let Ok(contents) = fs::read_to_string(full_path.as_std_path()) {
            files.insert(input.clone(), contents);
        }
    }
    let payload = serde_json::to_vec_pretty(&StoredSourceTexts {
        files,
        executed_files: executed_files.clone(),
        rewrite_spans: rewrite_spans.clone(),
        module_traces: module_traces.to_vec(),
        module_checkpoints: module_checkpoints.to_vec(),
    })
    .context("failed to serialize source text snapshot")?;
    fs::write(path.as_std_path(), payload).with_context(|| format!("failed to write {path}"))?;
    Ok(())
}

fn replay_checkpoint_from_stored(
    checkpoint: &StoredCheckpoint,
    toplevel: &Utf8Path,
) -> Option<ProjectReplayCheckpoint> {
    checkpoint
        .snapshot
        .as_ref()
        .map(|snapshot| ProjectReplayCheckpoint {
            snapshot: snapshot.clone(),
            resume_path: checkpoint
                .meta
                .resume_path
                .clone()
                .unwrap_or_else(|| toplevel.to_path_buf()),
            source_offset_utf8: checkpoint.meta.source_offset_utf8,
            continuation_stack: checkpoint.meta.continuation_stack.clone(),
        })
}

fn select_shipout_replay_plan(
    previous: &PreviousInternalBuild,
    root: &Utf8Path,
    toplevel: &Utf8Path,
    changed_files: &[Utf8PathBuf],
    current_source_overrides: Option<&BTreeMap<Utf8PathBuf, String>>,
) -> anyhow::Result<Option<ShipoutReplayPlan>> {
    select_shipout_replay_plan_with_spans(
        previous,
        root,
        toplevel,
        changed_files,
        current_source_overrides,
        None,
        None,
    )
}

fn select_shipout_replay_plan_with_spans(
    previous: &PreviousInternalBuild,
    root: &Utf8Path,
    toplevel: &Utf8Path,
    changed_files: &[Utf8PathBuf],
    current_source_overrides: Option<&BTreeMap<Utf8PathBuf, String>>,
    current_rewrite_spans: Option<&BTreeMap<Utf8PathBuf, Vec<MaterializedRewriteSpan>>>,
    force_conservative_files: Option<&[Utf8PathBuf]>,
) -> anyhow::Result<Option<ShipoutReplayPlan>> {
    let cp0_plan = || -> anyhow::Result<ShipoutReplayPlan> {
        Ok(ShipoutReplayPlan {
            checkpoint: ProjectReplayCheckpoint {
                snapshot: previous
                    .bundle
                    .checkpoints
                    .first()
                    .and_then(|checkpoint| checkpoint.snapshot.clone())
                    .context("missing preamble snapshot")?,
                resume_path: toplevel.to_path_buf(),
                source_offset_utf8: previous
                    .bundle
                    .checkpoints
                    .first()
                    .map(|checkpoint| checkpoint.meta.source_offset_utf8)
                    .unwrap_or_default(),
                continuation_stack: Vec::new(),
            },
            checkpoint_id: previous
                .bundle
                .checkpoints
                .first()
                .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
                .unwrap_or_default(),
            start_page_index: 0,
            output_prefix: String::new(),
            specificity_rank: 0,
        })
    };
    let mut best_plan: Option<ShipoutReplayPlan> = None;
    let mut replay_dirty_files = changed_files.to_vec();
    if let Some(source_overrides) = current_source_overrides {
        for (path, current_contents) in source_overrides {
            if changed_files.contains(path) {
                continue;
            }
            if previous
                .executed_sources
                .get(path)
                .is_some_and(|previous_contents| previous_contents == current_contents)
            {
                continue;
            }
            replay_dirty_files.push(path.clone());
        }
    }
    for changed_file in &replay_dirty_files {
        let force_conservative_replay = force_conservative_files
            .is_some_and(|files| files.iter().any(|forced_file| forced_file == changed_file));
        let checkpoint_start_page_index = |checkpoint: &StoredCheckpoint| {
            if checkpoint.meta.kind == CheckpointKind::InputBoundary {
                previous
                    .page_metadata
                    .last()
                    .map(|page| checkpoint.meta.page_index_after.min(page.index))
                    .unwrap_or_default()
            } else {
                previous
                    .page_metadata
                    .iter()
                    .find(|page| page.text_end_utf8 > checkpoint.meta.output_start_utf8)
                    .map(|page| page.index)
                    .or_else(|| previous.page_metadata.last().map(|page| page.index))
                    .unwrap_or_default()
            }
        };
        let current_path = root.join(changed_file);
        if !current_path.exists()
            && current_source_overrides.is_none_or(|sources| !sources.contains_key(changed_file))
        {
            let candidate_plan = if changed_file.as_path() == toplevel {
                Some(cp0_plan()?)
            } else {
                previous
                    .bundle
                    .checkpoints
                    .iter()
                    .filter(|checkpoint| {
                        checkpoint.meta.kind == CheckpointKind::InputBoundary
                            && checkpoint.meta.input_boundary_kind
                                == Some(VmModuleCheckpointKind::Enter)
                            && checkpoint.meta.module_path.as_ref() == Some(changed_file)
                    })
                    .min_by_key(|checkpoint| checkpoint.meta.output_start_utf8)
                    .and_then(|checkpoint| {
                        replay_checkpoint_from_stored(checkpoint, toplevel).map(
                            |replay_checkpoint| {
                                let start_page_index = checkpoint_start_page_index(checkpoint);
                                let output_prefix_end = (checkpoint.meta.output_start_utf8
                                    as usize)
                                    .min(previous.output.len());
                                ShipoutReplayPlan {
                                    checkpoint: replay_checkpoint,
                                    checkpoint_id: checkpoint.meta.checkpoint_id.clone(),
                                    start_page_index,
                                    output_prefix: previous.output[..output_prefix_end].to_string(),
                                    specificity_rank: 2,
                                }
                            },
                        )
                    })
                    .or_else(|| cp0_plan().ok())
            };
            best_plan = match (best_plan, candidate_plan) {
                (Some(existing), Some(candidate))
                    if existing.start_page_index < candidate.start_page_index
                        || (existing.start_page_index == candidate.start_page_index
                            && (existing.output_prefix.len() < candidate.output_prefix.len()
                                || (existing.output_prefix.len()
                                    == candidate.output_prefix.len()
                                    && existing.specificity_rank
                                        >= candidate.specificity_rank))) =>
                {
                    Some(existing)
                }
                (_, plan) => plan,
            };
            continue;
        }
        let semantic_override_changed = current_source_overrides
            .and_then(|sources| sources.get(changed_file))
            .is_some_and(|current_contents| {
                previous
                    .executed_sources
                    .get(changed_file)
                    .is_none_or(|previous_contents| previous_contents != current_contents)
            });
        let override_only_dirty =
            !changed_files.contains(changed_file) && semantic_override_changed;
        let semantic_override_rewrite_changed = semantic_override_changed
            && current_rewrite_spans
                .and_then(|spans| spans.get(changed_file))
                .and_then(|current_spans| {
                    previous
                        .rewrite_spans
                        .get(changed_file)
                        .and_then(|previous_spans| {
                            earliest_changed_rewrite_span_offset(previous_spans, current_spans)
                        })
                })
                .is_some();
        if force_conservative_replay || semantic_override_rewrite_changed {
            let candidate_plan = if changed_file.as_path() == toplevel {
                Some(cp0_plan()?)
            } else {
                previous
                    .bundle
                    .checkpoints
                    .iter()
                    .filter(|checkpoint| {
                        checkpoint.meta.kind == CheckpointKind::InputBoundary
                            && checkpoint.meta.input_boundary_kind
                                == Some(VmModuleCheckpointKind::Enter)
                            && checkpoint.meta.module_path.as_ref() == Some(changed_file)
                    })
                    .min_by_key(|checkpoint| checkpoint.meta.output_start_utf8)
                    .and_then(|checkpoint| {
                        replay_checkpoint_from_stored(checkpoint, toplevel).map(
                            |replay_checkpoint| {
                                let start_page_index = checkpoint_start_page_index(checkpoint);
                                let output_prefix_end = (checkpoint.meta.output_start_utf8
                                    as usize)
                                    .min(previous.output.len());
                                ShipoutReplayPlan {
                                    checkpoint: replay_checkpoint,
                                    checkpoint_id: checkpoint.meta.checkpoint_id.clone(),
                                    start_page_index,
                                    output_prefix: previous.output[..output_prefix_end].to_string(),
                                    specificity_rank: 2,
                                }
                            },
                        )
                    })
                    .or_else(|| cp0_plan().ok())
            };
            best_plan = match (best_plan, candidate_plan) {
                (Some(existing), Some(candidate))
                    if existing.start_page_index < candidate.start_page_index
                        || (existing.start_page_index == candidate.start_page_index
                            && (existing.output_prefix.len() < candidate.output_prefix.len()
                                || (existing.output_prefix.len()
                                    == candidate.output_prefix.len()
                                    && existing.specificity_rank
                                        >= candidate.specificity_rank))) =>
                {
                    Some(existing)
                }
                (_, plan) => plan,
            };
            continue;
        }
        let semantic_override_diff_offset = if override_only_dirty {
            current_rewrite_spans
                .and_then(|spans| spans.get(changed_file))
                .and_then(|current_spans| {
                    previous
                        .rewrite_spans
                        .get(changed_file)
                        .and_then(|previous_spans| {
                            earliest_changed_rewrite_span_offset(previous_spans, current_spans)
                        })
                })
        } else {
            None
        };
        let semantic_override_checkpoint_source_offset = if override_only_dirty {
            current_rewrite_spans
                .and_then(|spans| spans.get(changed_file))
                .and_then(|current_spans| {
                    previous
                        .rewrite_spans
                        .get(changed_file)
                        .and_then(|previous_spans| {
                            earliest_changed_rewrite_span_source_offset(
                                previous_spans,
                                current_spans,
                            )
                        })
                })
        } else {
            None
        };
        if override_only_dirty {
            if semantic_override_diff_offset.is_some() {
                // Fall through into the regular file-local replay selector with a semantic offset.
            } else {
                let candidate_plan = if changed_file.as_path() == toplevel {
                    Some(cp0_plan()?)
                } else {
                    previous
                        .bundle
                        .checkpoints
                        .iter()
                        .filter(|checkpoint| {
                            checkpoint.meta.kind == CheckpointKind::InputBoundary
                                && checkpoint.meta.input_boundary_kind
                                    == Some(VmModuleCheckpointKind::Enter)
                                && checkpoint.meta.module_path.as_ref() == Some(changed_file)
                        })
                        .min_by_key(|checkpoint| checkpoint.meta.output_start_utf8)
                        .and_then(|checkpoint| {
                            replay_checkpoint_from_stored(checkpoint, toplevel).map(
                                |replay_checkpoint| {
                                    let start_page_index = checkpoint_start_page_index(checkpoint);
                                    let output_prefix_end = (checkpoint.meta.output_start_utf8
                                        as usize)
                                        .min(previous.output.len());
                                    ShipoutReplayPlan {
                                        checkpoint: replay_checkpoint,
                                        checkpoint_id: checkpoint.meta.checkpoint_id.clone(),
                                        start_page_index,
                                        output_prefix: previous.output[..output_prefix_end]
                                            .to_string(),
                                        specificity_rank: 2,
                                    }
                                },
                            )
                        })
                        .or_else(|| cp0_plan().ok())
                };
                best_plan = match (best_plan, candidate_plan) {
                    (Some(existing), Some(candidate))
                        if existing.start_page_index < candidate.start_page_index
                            || (existing.start_page_index == candidate.start_page_index
                                && (existing.output_prefix.len()
                                    < candidate.output_prefix.len()
                                    || (existing.output_prefix.len()
                                        == candidate.output_prefix.len()
                                        && existing.specificity_rank
                                            >= candidate.specificity_rank))) =>
                    {
                        Some(existing)
                    }
                    (_, plan) => plan,
                };
                continue;
            }
        }
        let current_contents = match fs::read_to_string(current_path.as_std_path()) {
            Ok(contents) => contents,
            Err(_) => return Ok(None),
        };
        let diff_offset = semantic_override_diff_offset
            .map(|offset| offset as usize)
            .unwrap_or_else(|| {
                previous
                    .sources
                    .get(changed_file)
                    .map_or(0, |previous_contents| {
                        earliest_changed_offset(previous_contents, &current_contents)
                    })
            });
        let checkpoint_diff_offset = semantic_override_checkpoint_source_offset
            .map(|offset| offset as usize)
            .unwrap_or(diff_offset);

        let mut span_page_index = None;
        let mut last_page_index = None;
        for page in &previous.page_metadata {
            for span in &page.source_spans {
                if span.file != *changed_file {
                    continue;
                }
                last_page_index = Some(page.index);
                if (diff_offset as u32) < span.end_utf8 {
                    span_page_index = Some(page.index);
                    break;
                }
            }
            if span_page_index.is_some() {
                break;
            }
        }
        let span_page_index = span_page_index.or(last_page_index);
        let candidate_plan = if changed_file.as_path() == toplevel {
            let mut shipout_candidate = None;
            let file_page_index = {
                let offset_page_index = previous
                    .bundle
                    .checkpoints
                    .iter()
                    .filter(|checkpoint| checkpoint.meta.page_index_after > 0)
                    .take_while(|checkpoint| {
                        checkpoint.meta.source_offset_utf8 <= checkpoint_diff_offset as u32
                    })
                    .last()
                    .map(|checkpoint| checkpoint.meta.page_index_after);
                offset_page_index.or(span_page_index)
            };
            if let Some(file_page_index) = file_page_index {
                if file_page_index > 0 {
                    let checkpoint = previous
                        .bundle
                        .checkpoints
                        .iter()
                        .find(|checkpoint| checkpoint.meta.page_index_after == file_page_index)
                        .and_then(|checkpoint| {
                            replay_checkpoint_from_stored(checkpoint, toplevel).map(
                                |replay_checkpoint| {
                                    (checkpoint.meta.checkpoint_id.clone(), replay_checkpoint)
                                },
                            )
                        });
                    if let Some((checkpoint_id, checkpoint)) = checkpoint {
                        let prefix_output_end = previous
                            .page_metadata
                            .get(file_page_index)
                            .map(|page| page.text_start_utf8 as usize)
                            .unwrap_or(previous.output.len())
                            .min(previous.output.len());
                        shipout_candidate = Some(ShipoutReplayPlan {
                            checkpoint,
                            checkpoint_id,
                            start_page_index: file_page_index,
                            output_prefix: previous.output[..prefix_output_end].to_string(),
                            specificity_rank: 1,
                        });
                    }
                }
            }
            let input_boundary_candidate = previous
                .bundle
                .checkpoints
                .iter()
                .filter(|checkpoint| {
                    checkpoint.meta.kind == CheckpointKind::InputBoundary
                        && checkpoint.meta.resume_path.as_ref() == Some(changed_file)
                        && checkpoint.meta.source_offset_utf8 <= checkpoint_diff_offset as u32
                        && semantic_override_diff_offset
                            .is_none_or(|limit| checkpoint.meta.output_start_utf8 <= limit)
                })
                .max_by_key(|checkpoint| {
                    (
                        checkpoint.meta.source_offset_utf8,
                        checkpoint.meta.output_start_utf8,
                    )
                })
                .and_then(|checkpoint| {
                    replay_checkpoint_from_stored(checkpoint, toplevel).map(|replay_checkpoint| {
                        let start_page_index = checkpoint_start_page_index(checkpoint);
                        let output_prefix_end =
                            (checkpoint.meta.output_start_utf8 as usize).min(previous.output.len());
                        ShipoutReplayPlan {
                            checkpoint: replay_checkpoint,
                            checkpoint_id: checkpoint.meta.checkpoint_id.clone(),
                            start_page_index,
                            output_prefix: previous.output[..output_prefix_end].to_string(),
                            specificity_rank: 2,
                        }
                    })
                });
            match (shipout_candidate, input_boundary_candidate) {
                (Some(shipout), Some(input_boundary))
                    if shipout.start_page_index > input_boundary.start_page_index
                        || (shipout.start_page_index == input_boundary.start_page_index
                            && (shipout.output_prefix.len()
                                > input_boundary.output_prefix.len()
                                || (shipout.output_prefix.len()
                                    == input_boundary.output_prefix.len()
                                    && shipout.specificity_rank
                                        > input_boundary.specificity_rank))) =>
                {
                    Some(shipout)
                }
                (Some(_), Some(input_boundary)) => Some(input_boundary),
                (Some(shipout), None) => Some(shipout),
                (None, Some(input_boundary)) => Some(input_boundary),
                (None, None) => Some(cp0_plan()?),
            }
        } else {
            let mut selected_internal_checkpoints: Vec<&StoredCheckpoint> = Vec::new();
            for checkpoint in previous.bundle.checkpoints.iter().filter(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.resume_path.as_ref() == Some(changed_file)
                    && checkpoint.meta.source_offset_utf8 <= checkpoint_diff_offset as u32
                    && semantic_override_diff_offset
                        .is_none_or(|limit| checkpoint.meta.output_start_utf8 <= limit)
            }) {
                if let Some(existing_index) =
                    selected_internal_checkpoints.iter().position(|existing| {
                        existing.meta.continuation_stack == checkpoint.meta.continuation_stack
                    })
                {
                    let existing = selected_internal_checkpoints[existing_index];
                    if existing.meta.source_offset_utf8 < checkpoint.meta.source_offset_utf8
                        || (existing.meta.source_offset_utf8 == checkpoint.meta.source_offset_utf8
                            && existing.meta.output_start_utf8 < checkpoint.meta.output_start_utf8)
                    {
                        selected_internal_checkpoints[existing_index] = checkpoint;
                    }
                    continue;
                }
                selected_internal_checkpoints.push(checkpoint);
            }
            let selected_internal_checkpoint = if selected_internal_checkpoints.len() > 1 {
                selected_internal_checkpoints
                    .into_iter()
                    .min_by_key(|checkpoint| checkpoint.meta.output_start_utf8)
            } else {
                selected_internal_checkpoints.into_iter().next()
            };
            let mut candidate_plan = selected_internal_checkpoint.and_then(|checkpoint| {
                replay_checkpoint_from_stored(checkpoint, toplevel).map(|replay_checkpoint| {
                    let start_page_index = checkpoint_start_page_index(checkpoint);
                    let output_prefix_end =
                        (checkpoint.meta.output_start_utf8 as usize).min(previous.output.len());
                    ShipoutReplayPlan {
                        checkpoint: replay_checkpoint,
                        checkpoint_id: checkpoint.meta.checkpoint_id.clone(),
                        start_page_index,
                        output_prefix: previous.output[..output_prefix_end].to_string(),
                        specificity_rank: 2,
                    }
                })
            });
            if candidate_plan.is_none() {
                candidate_plan = previous
                    .bundle
                    .checkpoints
                    .iter()
                    .filter(|checkpoint| {
                        checkpoint.meta.kind == CheckpointKind::InputBoundary
                            && checkpoint.meta.input_boundary_kind
                                == Some(VmModuleCheckpointKind::Enter)
                            && checkpoint.meta.module_path.as_ref() == Some(changed_file)
                            && semantic_override_diff_offset
                                .is_none_or(|limit| checkpoint.meta.output_start_utf8 <= limit)
                    })
                    .min_by_key(|checkpoint| checkpoint.meta.output_start_utf8)
                    .and_then(|checkpoint| {
                        replay_checkpoint_from_stored(checkpoint, toplevel).map(
                            |replay_checkpoint| {
                                let start_page_index = checkpoint_start_page_index(checkpoint);
                                let output_prefix_end = (checkpoint.meta.output_start_utf8
                                    as usize)
                                    .min(previous.output.len());
                                ShipoutReplayPlan {
                                    checkpoint: replay_checkpoint,
                                    checkpoint_id: checkpoint.meta.checkpoint_id.clone(),
                                    start_page_index,
                                    output_prefix: previous.output[..output_prefix_end].to_string(),
                                    specificity_rank: 2,
                                }
                            },
                        )
                    });
            }
            if candidate_plan.is_none() {
                let trace_page_index =
                    previous
                        .sources
                        .get(changed_file)
                        .and_then(|previous_source| {
                            previous
                                .module_traces
                                .iter()
                                .filter(|trace| trace.path == *changed_file)
                                .map(|trace| {
                                    let trace_source_start =
                                        trace.source_start_utf8.min(previous_source.len() as u32);
                                    let trace_source_end =
                                        if trace.source_end_utf8 > trace_source_start {
                                            trace.source_end_utf8.min(previous_source.len() as u32)
                                        } else {
                                            previous_source.len() as u32
                                        };
                                    if (diff_offset as u32) < trace_source_start
                                        || (diff_offset as u32) > trace_source_end
                                    {
                                        return None;
                                    }
                                    let trace_output_len = trace
                                        .output_end_utf8
                                        .saturating_sub(trace.output_start_utf8)
                                        .max(1);
                                    let trace_source_len =
                                        trace_source_end.saturating_sub(trace_source_start).max(1);
                                    let target_output_utf8 = if (diff_offset as u32)
                                        >= trace_source_end
                                    {
                                        trace.output_end_utf8
                                    } else {
                                        trace.output_start_utf8
                                            + ((((diff_offset as u32) - trace_source_start) as u64
                                                * trace_output_len as u64)
                                                / trace_source_len as u64)
                                                as u32
                                    };
                                    previous
                                        .page_metadata
                                        .iter()
                                        .find(|page| page.text_end_utf8 > target_output_utf8)
                                        .map(|page| page.index)
                                        .or_else(|| {
                                            previous.page_metadata.last().map(|page| page.index)
                                        })
                                })
                                .min()
                                .flatten()
                        });
                let file_page_index = trace_page_index.or(span_page_index);
                if let Some(file_page_index) = file_page_index.filter(|index| *index > 0) {
                    let checkpoint = previous
                        .bundle
                        .checkpoints
                        .iter()
                        .find(|checkpoint| checkpoint.meta.page_index_after == file_page_index)
                        .and_then(|checkpoint| {
                            replay_checkpoint_from_stored(checkpoint, toplevel).map(
                                |replay_checkpoint| {
                                    (checkpoint.meta.checkpoint_id.clone(), replay_checkpoint)
                                },
                            )
                        });
                    if let Some((checkpoint_id, checkpoint)) = checkpoint {
                        let prefix_output_end = previous
                            .page_metadata
                            .get(file_page_index)
                            .map(|page| page.text_start_utf8 as usize)
                            .unwrap_or(previous.output.len())
                            .min(previous.output.len());
                        candidate_plan = Some(ShipoutReplayPlan {
                            checkpoint,
                            checkpoint_id,
                            start_page_index: file_page_index,
                            output_prefix: previous.output[..prefix_output_end].to_string(),
                            specificity_rank: 1,
                        });
                    }
                }
            }
            candidate_plan
        };
        best_plan = match (best_plan, candidate_plan) {
            (Some(existing), Some(candidate))
                if existing.start_page_index < candidate.start_page_index
                    || (existing.start_page_index == candidate.start_page_index
                        && (existing.output_prefix.len() < candidate.output_prefix.len()
                            || (existing.output_prefix.len()
                                == candidate.output_prefix.len()
                                && existing.specificity_rank >= candidate.specificity_rank))) =>
            {
                Some(existing)
            }
            (_, plan) => plan,
        };
    }

    Ok(best_plan)
}

fn earliest_changed_rewrite_span_offsets(
    previous_spans: &[MaterializedRewriteSpan],
    current_spans: &[MaterializedRewriteSpan],
) -> Option<(u32, u32)> {
    let mut span_keys = previous_spans
        .iter()
        .map(|span| (span.start_utf8, span.end_utf8))
        .collect::<BTreeSet<_>>();
    span_keys.extend(
        current_spans
            .iter()
            .map(|span| (span.start_utf8, span.end_utf8)),
    );
    for (start_utf8, end_utf8) in span_keys {
        let previous_span = previous_spans
            .iter()
            .find(|span| span.start_utf8 == start_utf8 && span.end_utf8 == end_utf8);
        let current_span = current_spans
            .iter()
            .find(|span| span.start_utf8 == start_utf8 && span.end_utf8 == end_utf8);
        let previous_rendered = previous_span.map(|span| span.rendered.as_str());
        let current_rendered = current_span.map(|span| span.rendered.as_str());
        if previous_rendered != current_rendered {
            return Some((
                start_utf8,
                previous_span
                    .map(|span| span.output_start_utf8)
                    .unwrap_or(start_utf8),
            ));
        }
    }
    None
}

fn earliest_changed_rewrite_span_offset(
    previous_spans: &[MaterializedRewriteSpan],
    current_spans: &[MaterializedRewriteSpan],
) -> Option<u32> {
    earliest_changed_rewrite_span_offsets(previous_spans, current_spans)
        .map(|(start_utf8, _)| start_utf8)
}

fn earliest_changed_rewrite_span_source_offset(
    previous_spans: &[MaterializedRewriteSpan],
    current_spans: &[MaterializedRewriteSpan],
) -> Option<u32> {
    earliest_changed_rewrite_span_offsets(previous_spans, current_spans)
        .map(|(_, output_start_utf8)| output_start_utf8)
}

fn earliest_changed_offset(previous: &str, current: &str) -> usize {
    let shared_prefix = previous
        .bytes()
        .zip(current.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    shared_prefix.min(previous.len()).min(current.len())
}

fn shift_shipout_source_offset(
    previous_toplevel_source: Option<&str>,
    current_toplevel_source: &str,
    previous_source_offset_utf8: u32,
    current_page_floor_utf8: u32,
) -> u32 {
    let mut shifted = previous_source_offset_utf8 as usize;
    if let Some(previous_source) = previous_toplevel_source {
        let shared_prefix = earliest_changed_offset(previous_source, current_toplevel_source);
        let previous_suffix = &previous_source.as_bytes()[shared_prefix..];
        let current_suffix = &current_toplevel_source.as_bytes()[shared_prefix..];
        let shared_suffix = previous_suffix
            .iter()
            .rev()
            .zip(current_suffix.iter().rev())
            .take_while(|(left, right)| left == right)
            .count();
        let previous_change_end = previous_source.len().saturating_sub(shared_suffix);
        let current_change_end = current_toplevel_source.len().saturating_sub(shared_suffix);
        if shifted <= shared_prefix {
            shifted = shifted.min(current_toplevel_source.len());
        } else if shifted >= previous_change_end {
            shifted = current_change_end
                .saturating_add(shifted.saturating_sub(previous_change_end))
                .min(current_toplevel_source.len());
        } else {
            shifted = shared_prefix
                .saturating_add(
                    shifted
                        .saturating_sub(shared_prefix)
                        .min(current_change_end.saturating_sub(shared_prefix)),
                )
                .min(current_toplevel_source.len());
        }
    } else {
        shifted = shifted.min(current_toplevel_source.len());
    }
    shifted = shifted
        .max(current_page_floor_utf8 as usize)
        .min(current_toplevel_source.len());
    while shifted < current_toplevel_source.len()
        && !current_toplevel_source.is_char_boundary(shifted)
    {
        shifted += 1;
    }
    shifted as u32
}

fn rebase_reused_shipout_checkpoint(
    previous_sources: &BTreeMap<Utf8PathBuf, String>,
    previous_module_traces: &[StoredModuleTrace],
    previous_module_checkpoints: &[StoredModuleCheckpoint],
    current_sources: &BTreeMap<Utf8PathBuf, String>,
    previous_page_metadata: &[PageArtifactMeta],
    previous_page_index: usize,
    page_metadata: &[ProjectPageMeta],
    current_page_index: usize,
    checkpoint: &mut ProjectReplayCheckpoint,
) {
    let rebase_with_bounds = |current_source: &str,
                              previous_source: Option<&str>,
                              source_offset_utf8: u32,
                              bounds: Option<(u32, u32, u32)>| {
        if let Some((previous_floor, current_floor, current_ceiling)) = bounds {
            let mut rebased = (current_floor as usize)
                .saturating_add(source_offset_utf8.saturating_sub(previous_floor) as usize)
                .min(current_source.len())
                .min(current_ceiling as usize);
            while rebased < current_source.len() && !current_source.is_char_boundary(rebased) {
                rebased += 1;
            }
            rebased as u32
        } else {
            shift_shipout_source_offset(previous_source, current_source, source_offset_utf8, 0)
        }
    };
    let derive_path_bounds = |path: &Utf8PathBuf, source_offset_utf8: u32, current_source: &str| {
        let previous_floor = previous_page_metadata[..=previous_page_index]
            .iter()
            .flat_map(|page| page.source_spans.iter())
            .filter(|span| span.file == *path)
            .map(|span| span.end_utf8)
            .max();
        let current_floor = page_metadata[..=current_page_index]
            .iter()
            .flat_map(|page| page.source_spans.iter())
            .filter(|span| span.file == *path)
            .map(|span| span.end_utf8)
            .max();
        let page_bounds = match (previous_floor, current_floor) {
            (Some(previous_floor), Some(current_floor))
                if previous_floor > 0 || current_floor > 0 =>
            {
                Some((previous_floor, current_floor, current_source.len() as u32))
            }
            _ => None,
        };
        let checkpoint_bounds = previous_sources.get(path).and_then(|previous_source| {
            let mut previous_floor = None;
            let mut previous_ceiling = None;
            for checkpoint in previous_module_checkpoints {
                if checkpoint.resume_path.as_ref() == Some(path) {
                    if checkpoint.source_offset_utf8 <= source_offset_utf8 {
                        previous_floor = Some(
                            previous_floor
                                .unwrap_or(0)
                                .max(checkpoint.source_offset_utf8),
                        );
                    } else {
                        previous_ceiling = Some(
                            previous_ceiling
                                .map_or(checkpoint.source_offset_utf8, |ceiling: u32| {
                                    ceiling.min(checkpoint.source_offset_utf8)
                                }),
                        );
                    }
                }
                for frame in &checkpoint.continuation_stack {
                    if &frame.path != path {
                        continue;
                    }
                    if frame.source_offset_utf8 <= source_offset_utf8 {
                        previous_floor =
                            Some(previous_floor.unwrap_or(0).max(frame.source_offset_utf8));
                    } else {
                        previous_ceiling = Some(
                            previous_ceiling.map_or(frame.source_offset_utf8, |ceiling: u32| {
                                ceiling.min(frame.source_offset_utf8)
                            }),
                        );
                    }
                }
            }
            let previous_floor = previous_floor?;
            let previous_ceiling = previous_ceiling.unwrap_or(previous_source.len() as u32);
            let interval = previous_source
                .get(previous_floor as usize..previous_ceiling as usize)
                .filter(|slice| !slice.is_empty())?;
            let mut matches = current_source.match_indices(interval);
            let (current_floor, _) = matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            Some((
                previous_floor,
                current_floor as u32,
                current_floor as u32 + interval.len() as u32,
            ))
        });
        let trace_bounds = previous_sources.get(path).and_then(|previous_source| {
            previous_module_traces
                .iter()
                .filter(|trace| trace.path == *path)
                .filter_map(|trace| {
                    let trace_start = trace.source_start_utf8.min(previous_source.len() as u32);
                    let trace_end = if trace.source_end_utf8 > trace_start {
                        trace.source_end_utf8.min(previous_source.len() as u32)
                    } else {
                        previous_source.len() as u32
                    };
                    if source_offset_utf8 < trace_start || source_offset_utf8 > trace_end {
                        return None;
                    }
                    let interval = previous_source
                        .get(trace_start as usize..trace_end as usize)
                        .filter(|slice| !slice.is_empty())?;
                    let mut matches = current_source.match_indices(interval);
                    let (current_start, _) = matches.next()?;
                    if matches.next().is_some() {
                        return None;
                    }
                    Some((
                        trace_start,
                        current_start as u32,
                        current_start as u32 + interval.len() as u32,
                    ))
                })
                .max_by_key(|(trace_start, _, _)| *trace_start)
        });
        trace_bounds.or(checkpoint_bounds).or(page_bounds)
    };
    if let Some(current_source) = current_sources.get(&checkpoint.resume_path) {
        let bounds = derive_path_bounds(
            &checkpoint.resume_path,
            checkpoint.source_offset_utf8,
            current_source,
        );
        checkpoint.source_offset_utf8 = rebase_with_bounds(
            current_source,
            previous_sources
                .get(&checkpoint.resume_path)
                .map(String::as_str),
            checkpoint.source_offset_utf8,
            bounds,
        );
    }
    for frame in &mut checkpoint.continuation_stack {
        if let Some(current_source) = current_sources.get(&frame.path) {
            let bounds = derive_path_bounds(&frame.path, frame.source_offset_utf8, current_source);
            frame.source_offset_utf8 = rebase_with_bounds(
                current_source,
                previous_sources.get(&frame.path).map(String::as_str),
                frame.source_offset_utf8,
                bounds,
            );
        }
    }
}

#[cfg(test)]
fn rebase_shipout_path_offset(
    previous_source: Option<&str>,
    current_source: &str,
    previous_source_offset_utf8: u32,
    previous_page_floor_utf8: u32,
    current_page_floor_utf8: u32,
) -> u32 {
    let previous_offset = previous_source_offset_utf8 as usize;
    if previous_page_floor_utf8 > 0 || current_page_floor_utf8 > 0 {
        let relative_offset = previous_offset.saturating_sub(previous_page_floor_utf8 as usize);
        let shifted = (current_page_floor_utf8 as usize)
            .saturating_add(relative_offset)
            .min(current_source.len());
        let mut aligned = shifted.max(current_page_floor_utf8 as usize);
        while aligned < current_source.len() && !current_source.is_char_boundary(aligned) {
            aligned += 1;
        }
        return aligned as u32;
    }

    shift_shipout_source_offset(
        previous_source,
        current_source,
        previous_source_offset_utf8,
        current_page_floor_utf8,
    )
}

fn parse_depfile(root: &Utf8Path, contents: &str) -> DepTrace {
    let mut normalized = String::with_capacity(contents.len());
    let mut chars = contents.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\\' && matches!(chars.peek(), Some('\n')) {
            chars.next();
            normalized.push(' ');
            continue;
        }
        if character == '\\' && matches!(chars.peek(), Some('\r')) {
            chars.next();
            if matches!(chars.peek(), Some('\n')) {
                chars.next();
            }
            normalized.push(' ');
            continue;
        }
        normalized.push(character);
    }

    let Some((_, deps)) = normalized.split_once(':') else {
        return DepTrace::default();
    };
    let mut inputs = BTreeSet::new();
    for token in deps.split_whitespace() {
        if let Some(path) = relativize_dependency_path(root, token) {
            inputs.insert(path);
        }
    }

    DepTrace::from_inputs(inputs)
}

fn parse_fls(root: &Utf8Path, contents: &str) -> DepTrace {
    let mut inputs = BTreeSet::new();
    for line in contents.lines() {
        let Some(path) = line.strip_prefix("INPUT ") else {
            continue;
        };
        if let Some(path) = relativize_dependency_path(root, path) {
            inputs.insert(path);
        }
    }

    DepTrace::from_inputs(inputs)
}

fn relativize_dependency_path(root: &Utf8Path, token: &str) -> Option<Utf8PathBuf> {
    let candidate = token.trim();
    if candidate.is_empty() {
        return None;
    }

    let candidate = if let Some(relative) = Utf8Path::new(candidate)
        .as_std_path()
        .strip_prefix(root.as_std_path())
        .ok()
        .and_then(Utf8Path::from_path)
    {
        relative.to_path_buf()
    } else {
        normalize_relative_path(Utf8Path::new(candidate)).ok()?
    };

    Some(candidate)
}

fn plan_page_patches(
    previous_pages: &[CheckpointPage],
    current_pages: &[CheckpointPage],
    current_artifacts: &[PagePreviewArtifact],
    unchanged_tail: Option<&UnchangedTail>,
) -> Vec<PagePatchOp> {
    let previous_prefix_len = unchanged_tail
        .map(|tail| tail.previous_page_start)
        .unwrap_or(previous_pages.len());
    let current_prefix_len = unchanged_tail
        .map(|tail| tail.current_page_start)
        .unwrap_or(current_pages.len());
    let previous_prefix = &previous_pages[..previous_prefix_len];
    let current_prefix = &current_pages[..current_prefix_len];
    let overlap = previous_prefix.len().min(current_prefix.len());
    let mut ops = Vec::new();
    let artifact_urls = current_artifacts
        .iter()
        .map(|artifact| {
            (
                artifact.page_id.as_str(),
                (artifact.pdf_url.as_str(), artifact.svg_url.as_deref()),
            )
        })
        .collect::<BTreeMap<_, _>>();

    for index in 0..overlap {
        if previous_prefix[index].content_hash != current_prefix[index].content_hash {
            let (pdf_url, svg_url) = artifact_urls
                .get(current_prefix[index].page_id.as_str())
                .expect("page artifact must exist for current page");
            ops.push(PagePatchOp::ReplacePage {
                index,
                page_id: current_prefix[index].page_id.clone(),
                pdf_url: (*pdf_url).to_string(),
                svg_url: svg_url.map(str::to_string),
            });
        }
    }

    if current_prefix.len() > previous_prefix.len() {
        for (index, page) in current_prefix
            .iter()
            .enumerate()
            .skip(previous_prefix.len())
        {
            let (pdf_url, svg_url) = artifact_urls
                .get(page.page_id.as_str())
                .expect("page artifact must exist for inserted page");
            ops.push(PagePatchOp::InsertPage {
                index,
                page_id: page.page_id.clone(),
                pdf_url: (*pdf_url).to_string(),
                svg_url: svg_url.map(str::to_string),
            });
        }
    } else if previous_prefix.len() > current_prefix.len() {
        for index in (current_prefix.len()..previous_prefix.len()).rev() {
            ops.push(PagePatchOp::DeletePage { index });
        }
    }

    ops
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs};

    use camino::{Utf8Path, Utf8PathBuf};
    use hmr_protocol::{PagePatchOp, PagePreviewArtifact};
    use tempfile::tempdir;
    use tex_aux::MaterializedRewriteSpan;
    use tex_bootstrap::{ProjectPageMeta, ProjectReplayCheckpoint};
    use tex_checkpoint::{
        CheckpointKind, InputBoundaryCheckpoint, ShipoutCheckpoint,
        build_checkpoint_bundle_with_shipouts, build_checkpoint_bundle_with_snapshots,
        preamble_key_for_source,
    };
    use tex_layout::TextSpan;
    use tex_render_model::RenderEvent;
    use tex_tokens::ControlSequenceInterner;
    use tex_vm::{VmModuleCheckpointKind, VmReplayFrame, compile_format_snapshot};

    use super::{
        ArtifactSourceSpan, CheckpointPage, DepTrace, PageArtifactMeta, PreviousInternalBuild,
        SemanticAux, StoredModuleCheckpoint, StoredModuleTrace, UnchangedTail,
        capture_internal_render_ir, earliest_changed_offset, earliest_changed_rewrite_span_offset,
        earliest_changed_rewrite_span_source_offset, load_latest_previous_internal_build,
        parse_depfile, parse_fls, plan_page_patches, rebase_reused_shipout_checkpoint,
        rebase_shipout_path_offset, replay_checkpoint_from_stored, save_source_texts,
        select_shipout_replay_plan, select_shipout_replay_plan_with_spans,
        shift_shipout_source_offset,
    };

    #[test]
    fn internal_render_ir_capture_builds_events_and_ir_without_pdf_path() {
        let capture = capture_internal_render_ir(
            "main.tex",
            r"\title{A Paper}\begin{document}\maketitle\section{Intro}Hello \cite{key}.\end{document}",
            &SemanticAux::default(),
        );

        assert!(
            capture
                .events
                .events
                .iter()
                .any(|event| matches!(&event.event, RenderEvent::FlushTitleBlock(_)))
        );
        assert!(capture.document_ir.extracted_text().contains("A Paper"));
        assert!(capture.document_ir.extracted_text().contains("Intro"));
        assert!(capture.document_ir.extracted_text().contains("[?]"));
        assert_eq!(capture.page_display_lists.len(), 1);
        assert!(String::from_utf8_lossy(&capture.display_list_pdf).contains("(A Paper) Tj"));
        assert!(!capture.legacy_output.is_empty());
    }

    #[test]
    fn parses_makefile_depfile_with_continuations() {
        let trace = parse_depfile(
            Utf8Path::new("/tmp/project"),
            "out.pdf: main.tex sections/intro.tex \\\n figures/plot.pdf\n",
        );

        assert_eq!(
            trace,
            DepTrace::from_inputs([
                "figures/plot.pdf".into(),
                "main.tex".into(),
                "sections/intro.tex".into(),
            ])
        );
    }

    #[test]
    fn parses_latex_recorder_input_lines() {
        let trace = parse_fls(
            Utf8Path::new("/tmp/project"),
            "PWD /tmp/project\nINPUT /tmp/project/main.tex\nINPUT /tmp/project/figures/plot.pdf\nOUTPUT /tmp/project/main.pdf\n",
        );

        assert_eq!(
            trace,
            DepTrace::from_inputs(["figures/plot.pdf".into(), "main.tex".into()])
        );
    }

    #[test]
    fn changed_offset_stops_at_first_divergent_byte() {
        assert_eq!(earliest_changed_offset("abcdef", "abcXYZ"), 3);
        assert_eq!(earliest_changed_offset("abc", "abc123"), 3);
    }

    #[test]
    fn shifted_shipout_source_offset_rebases_forward_after_insert() {
        assert_eq!(
            shift_shipout_source_offset(Some("hello world"), "hello brave world", 11, 0),
            17
        );
    }

    #[test]
    fn shifted_shipout_source_offset_respects_current_page_floor() {
        assert_eq!(
            shift_shipout_source_offset(Some("hello world"), "hello brave world", 6, 12),
            12
        );
    }

    #[test]
    fn shifted_shipout_source_offset_ignores_length_delta_after_offset() {
        assert_eq!(
            shift_shipout_source_offset(Some("abcdefghij"), "abXdefghijZZ", 6, 0),
            6
        );
        assert_eq!(
            shift_shipout_source_offset(Some("abcdefghij"), "abXdefghijZZ", 10, 0),
            12
        );
    }

    #[test]
    fn rebased_shipout_path_offset_prefers_page_floor_delta_over_full_file_delta() {
        assert_eq!(
            rebase_shipout_path_offset(Some("12AA34ZZ"), "12XXAA34ZZTT", 6, 4, 6),
            8
        );
    }

    #[test]
    fn rebased_shipout_checkpoint_prefers_exact_trace_slice_match() {
        let path = Utf8PathBuf::from("sections/tail.tex");
        let mut checkpoint = ProjectReplayCheckpoint {
            snapshot: compile_format_snapshot(&mut ControlSequenceInterner::new(), r"\def\fmt{p}"),
            resume_path: path.clone(),
            source_offset_utf8: 6,
            continuation_stack: vec![VmReplayFrame {
                path: path.clone(),
                source_offset_utf8: 5,
            }],
        };

        rebase_reused_shipout_checkpoint(
            &BTreeMap::from([(path.clone(), "AAtraceZZ".to_string())]),
            &[StoredModuleTrace {
                path: path.clone(),
                source_start_utf8: 2,
                source_end_utf8: 7,
                output_start_utf8: 0,
                output_end_utf8: 5,
            }],
            &[],
            &BTreeMap::from([(path.clone(), "XXAAtraceYYZZWW".to_string())]),
            &[PageArtifactMeta {
                page_id: "prev-p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-prev".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 5,
                pdf_artifact_path: Utf8PathBuf::from("rev-1/pages/prev-p0.pdf"),
                source_spans: vec![],
            }],
            0,
            &[ProjectPageMeta {
                page_id: "curr-p0".to_string(),
                index: 0,
                content_hash: "hash-curr".to_string(),
                line_count: 1,
                width_pt: 612.0,
                height_pt: 792.0,
                text_span: TextSpan {
                    start_utf8: 0,
                    end_utf8: 5,
                },
                source_spans: vec![],
                sync_spans: vec![],
            }],
            0,
            &mut checkpoint,
        );

        assert_eq!(checkpoint.source_offset_utf8, 8);
        assert_eq!(checkpoint.continuation_stack[0].source_offset_utf8, 7);
    }

    #[test]
    fn rebased_shipout_checkpoint_prefers_exact_checkpoint_interval_match() {
        let path = Utf8PathBuf::from("sections/body.tex");
        let mut checkpoint = ProjectReplayCheckpoint {
            snapshot: compile_format_snapshot(&mut ControlSequenceInterner::new(), r"\def\fmt{p}"),
            resume_path: path.clone(),
            source_offset_utf8: 7,
            continuation_stack: vec![VmReplayFrame {
                path: path.clone(),
                source_offset_utf8: 6,
            }],
        };

        rebase_reused_shipout_checkpoint(
            &BTreeMap::from([(path.clone(), "AAsegmentZZ".to_string())]),
            &[],
            &[
                StoredModuleCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: path.clone(),
                    resume_path: Some(path.clone()),
                    source_offset_utf8: 2,
                    continuation_stack: vec![],
                    output_start_utf8: 0,
                    snapshot: compile_format_snapshot(
                        &mut ControlSequenceInterner::new(),
                        r"\def\fmt{enter}",
                    ),
                },
                StoredModuleCheckpoint {
                    kind: VmModuleCheckpointKind::Exit,
                    module_path: path.clone(),
                    resume_path: Some(path.clone()),
                    source_offset_utf8: 9,
                    continuation_stack: vec![],
                    output_start_utf8: 7,
                    snapshot: compile_format_snapshot(
                        &mut ControlSequenceInterner::new(),
                        r"\def\fmt{exit}",
                    ),
                },
            ],
            &BTreeMap::from([(path.clone(), "XXAAsegmentYYZZ".to_string())]),
            &[PageArtifactMeta {
                page_id: "prev-p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-prev".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 5,
                pdf_artifact_path: Utf8PathBuf::from("rev-1/pages/prev-p0.pdf"),
                source_spans: vec![],
            }],
            0,
            &[ProjectPageMeta {
                page_id: "curr-p0".to_string(),
                index: 0,
                content_hash: "hash-curr".to_string(),
                line_count: 1,
                width_pt: 612.0,
                height_pt: 792.0,
                text_span: TextSpan {
                    start_utf8: 0,
                    end_utf8: 5,
                },
                source_spans: vec![],
                sync_spans: vec![],
            }],
            0,
            &mut checkpoint,
        );

        assert_eq!(checkpoint.source_offset_utf8, 9);
        assert_eq!(checkpoint.continuation_stack[0].source_offset_utf8, 8);
    }

    #[test]
    fn rebased_shipout_checkpoint_updates_resume_path_and_continuation_stack() {
        let mut checkpoint = ProjectReplayCheckpoint {
            snapshot: compile_format_snapshot(&mut ControlSequenceInterner::new(), r"\def\fmt{p}"),
            resume_path: Utf8PathBuf::from("sections/tail.tex"),
            source_offset_utf8: 6,
            continuation_stack: vec![VmReplayFrame {
                path: Utf8PathBuf::from("main.tex"),
                source_offset_utf8: 6,
            }],
        };

        rebase_reused_shipout_checkpoint(
            &BTreeMap::from([
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "12AA34ZZ".to_string(),
                ),
                (Utf8PathBuf::from("main.tex"), "12AA34ZZ".to_string()),
            ]),
            &[],
            &[],
            &BTreeMap::from([
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "12XXAA34ZZTT".to_string(),
                ),
                (Utf8PathBuf::from("main.tex"), "12XXAA34ZZTT".to_string()),
            ]),
            &[PageArtifactMeta {
                page_id: "prev-p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-prev".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
                pdf_artifact_path: Utf8PathBuf::from("rev-1/pages/prev-p0.pdf"),
                source_spans: vec![
                    ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/tail.tex"),
                        start_utf8: 0,
                        end_utf8: 4,
                    },
                    ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 4,
                    },
                ],
            }],
            0,
            &[ProjectPageMeta {
                page_id: "p0".to_string(),
                index: 0,
                width_pt: 612.0,
                height_pt: 792.0,
                content_hash: "hash-0".to_string(),
                text_span: TextSpan {
                    start_utf8: 0,
                    end_utf8: 10,
                },
                line_count: 1,
                source_spans: vec![
                    tex_bootstrap::ProjectSourceSpan {
                        file: Utf8PathBuf::from("sections/tail.tex"),
                        start_utf8: 0,
                        end_utf8: 6,
                    },
                    tex_bootstrap::ProjectSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 6,
                    },
                ],
                sync_spans: vec![],
            }],
            0,
            &mut checkpoint,
        );

        assert_eq!(checkpoint.source_offset_utf8, 8);
        assert_eq!(checkpoint.continuation_stack[0].source_offset_utf8, 8);
    }

    #[test]
    fn replay_checkpoint_from_stored_preserves_shipout_resume_metadata() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let shipout_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{ship}");
        let bundle = build_checkpoint_bundle_with_shipouts(
            7,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[ShipoutCheckpoint {
                snapshot: shipout_snapshot.clone(),
                source_offset_utf8: 27,
                resume_path: Some(Utf8PathBuf::from("sections/tail.tex")),
                continuation_stack: vec![VmReplayFrame {
                    path: Utf8PathBuf::from("main.tex"),
                    source_offset_utf8: 44,
                }],
            }],
            &[],
        )
        .expect("bundle");

        let replay =
            replay_checkpoint_from_stored(&bundle.checkpoints[1], Utf8Path::new("main.tex"))
                .expect("replay checkpoint");
        assert_eq!(replay.resume_path, Utf8PathBuf::from("sections/tail.tex"));
        assert_eq!(replay.source_offset_utf8, 27);
        assert_eq!(
            replay.continuation_stack,
            vec![VmReplayFrame {
                path: Utf8PathBuf::from("main.tex"),
                source_offset_utf8: 44,
            }]
        );
    }

    #[test]
    fn loads_latest_previous_internal_build_state() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().join("root")).expect("utf8 root");
        let build_root =
            Utf8PathBuf::from_path_buf(tempdir.path().join("build")).expect("utf8 build root");
        let rev_dir = build_root.join("rev-2");
        fs::create_dir_all(root.as_std_path()).expect("root dir");
        fs::create_dir_all(rev_dir.as_std_path()).expect("rev dir");
        fs::write(root.join("main.tex"), "body").expect("write source");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let shipout_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{page}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            2,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            11,
            &[checkpoint_page("p0", 0, "hash-0")],
            std::slice::from_ref(&shipout_snapshot),
            &[23],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("main.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 0,
                continuation_stack: Vec::new(),
                output_start_utf8: 0,
                page_index_after: 0,
                snapshot: preamble_snapshot.clone(),
            }],
        )
        .expect("bundle");
        fs::write(
            rev_dir.join("checkpoints.json").as_std_path(),
            serde_json::to_vec_pretty(&bundle).expect("serialize bundle"),
        )
        .expect("write checkpoints");
        fs::write(
            rev_dir.join("page-metadata.json").as_std_path(),
            serde_json::to_vec_pretty(&vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
                pdf_artifact_path: Utf8PathBuf::from("rev-2/pages/p0.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 11,
                    end_utf8: 23,
                }],
            }])
            .expect("serialize metadata"),
        )
        .expect("write metadata");
        fs::write(rev_dir.join("output.txt").as_std_path(), "prefix tail").expect("write output");
        save_source_texts(
            &rev_dir.join("sources.json"),
            &root,
            &[Utf8PathBuf::from("main.tex")],
            &BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "body [materialized]".to_string(),
            )]),
            &BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                vec![MaterializedRewriteSpan {
                    start_utf8: 0,
                    end_utf8: 4,
                    output_start_utf8: 0,
                    output_end_utf8: 19,
                    rendered: "body [materialized]".to_string(),
                }],
            )]),
            &[StoredModuleTrace {
                path: Utf8PathBuf::from("main.tex"),
                source_start_utf8: 0,
                source_end_utf8: 4,
                output_start_utf8: 0,
                output_end_utf8: 4,
            }],
            &[StoredModuleCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("main.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 0,
                continuation_stack: Vec::new(),
                output_start_utf8: 0,
                snapshot: preamble_snapshot.clone(),
            }],
        )
        .expect("write sources");
        fs::write(
            rev_dir.join("aux.json").as_std_path(),
            serde_json::to_vec_pretty(&SemanticAux {
                labels: vec![tex_aux::SemanticLabel {
                    key: "sec:intro".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                }],
                toc: Vec::new(),
                citation_keys: vec!["alpha".to_string()],
                bibliography_inputs: vec![Utf8PathBuf::from("refs.bbl")],
                bibliography_style: Some("plain".to_string()),
                citation_aliases: Vec::new(),
                bibliography: Vec::new(),
                bibliography_titles: Vec::new(),
                bibliography_authors: Vec::new(),
                bibliography_years: Vec::new(),
                bibliography_fields: Vec::new(),
                bibliography_urls: Vec::new(),
                bibliography_dois: Vec::new(),
                bibliography_eprints: Vec::new(),
                float_captions: Vec::new(),
            })
            .expect("serialize aux"),
        )
        .expect("write aux");
        fs::write(
            rev_dir.join("semantic.aux").as_std_path(),
            tex_aux::render_concrete_semantic_aux(&SemanticAux {
                labels: vec![tex_aux::SemanticLabel {
                    key: "sec:intro".to_string(),
                    number: "1".to_string(),
                    page: 1,
                    file: Utf8PathBuf::from("main.tex"),
                    offset_utf8: 0,
                }],
                toc: Vec::new(),
                citation_keys: vec!["alpha".to_string()],
                bibliography_inputs: vec![Utf8PathBuf::from("refs.bbl")],
                bibliography_style: Some("plain".to_string()),
                citation_aliases: Vec::new(),
                bibliography: Vec::new(),
                bibliography_titles: Vec::new(),
                bibliography_authors: Vec::new(),
                bibliography_years: Vec::new(),
                bibliography_fields: Vec::new(),
                bibliography_urls: Vec::new(),
                bibliography_dois: Vec::new(),
                bibliography_eprints: Vec::new(),
                float_captions: Vec::new(),
            })
            .expect("render concrete aux"),
        )
        .expect("write concrete aux");

        let loaded = load_latest_previous_internal_build(&build_root, 3)
            .expect("load previous state")
            .expect("previous state");

        assert_eq!(loaded.rev, 2);
        assert_eq!(loaded.output, "prefix tail");
        assert_eq!(loaded.bundle.checkpoints[1].meta.source_offset_utf8, 23);
        assert_eq!(
            loaded.page_metadata[0].pdf_artifact_path,
            Utf8PathBuf::from("rev-2/pages/p0.pdf")
        );
        assert_eq!(loaded.sources[&Utf8PathBuf::from("main.tex")], "body");
        assert_eq!(
            loaded.executed_sources[&Utf8PathBuf::from("main.tex")],
            "body [materialized]"
        );
        assert_eq!(
            loaded.rewrite_spans[&Utf8PathBuf::from("main.tex")],
            vec![MaterializedRewriteSpan {
                start_utf8: 0,
                end_utf8: 4,
                output_start_utf8: 0,
                output_end_utf8: 19,
                rendered: "body [materialized]".to_string(),
            }]
        );
        assert_eq!(loaded.module_traces.len(), 1);
        assert_eq!(loaded.module_traces[0].path, Utf8PathBuf::from("main.tex"));
        assert_eq!(loaded.module_traces[0].source_start_utf8, 0);
        assert_eq!(loaded.module_traces[0].source_end_utf8, 4);
        assert_eq!(
            loaded
                .semantic_aux
                .as_ref()
                .expect("semantic aux")
                .citation_keys,
            vec!["alpha".to_string()]
        );
        assert_eq!(
            loaded.semantic_aux_payload.as_deref(),
            Some(
                fs::read(rev_dir.join("aux.json").as_std_path())
                    .expect("read aux")
                    .as_slice()
            )
        );
        assert_eq!(
            loaded.semantic_aux_concrete_payload.as_deref(),
            Some(
                fs::read(rev_dir.join("semantic.aux").as_std_path())
                    .expect("read concrete aux")
                    .as_slice()
            )
        );
        assert!(loaded.bundle.checkpoints.iter().any(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("main.tex"))
        }));
    }

    #[test]
    fn earliest_changed_rewrite_span_offset_returns_first_changed_raw_offset() {
        let previous = vec![
            MaterializedRewriteSpan {
                start_utf8: 12,
                end_utf8: 24,
                output_start_utf8: 4,
                output_end_utf8: 7,
                rendered: "[1]".to_string(),
            },
            MaterializedRewriteSpan {
                start_utf8: 80,
                end_utf8: 96,
                output_start_utf8: 24,
                output_end_utf8: 28,
                rendered: "2024".to_string(),
            },
        ];
        let current = vec![
            MaterializedRewriteSpan {
                start_utf8: 12,
                end_utf8: 24,
                output_start_utf8: 4,
                output_end_utf8: 7,
                rendered: "[1]".to_string(),
            },
            MaterializedRewriteSpan {
                start_utf8: 80,
                end_utf8: 96,
                output_start_utf8: 24,
                output_end_utf8: 28,
                rendered: "2025".to_string(),
            },
        ];

        assert_eq!(
            earliest_changed_rewrite_span_offset(&previous, &current),
            Some(80)
        );
    }

    #[test]
    fn earliest_changed_rewrite_span_source_offset_returns_materialized_offset() {
        let previous = vec![
            MaterializedRewriteSpan {
                start_utf8: 12,
                end_utf8: 24,
                output_start_utf8: 4,
                output_end_utf8: 7,
                rendered: "[1]".to_string(),
            },
            MaterializedRewriteSpan {
                start_utf8: 80,
                end_utf8: 96,
                output_start_utf8: 24,
                output_end_utf8: 28,
                rendered: "2024".to_string(),
            },
        ];
        let current = vec![
            MaterializedRewriteSpan {
                start_utf8: 12,
                end_utf8: 24,
                output_start_utf8: 4,
                output_end_utf8: 7,
                rendered: "[1]".to_string(),
            },
            MaterializedRewriteSpan {
                start_utf8: 80,
                end_utf8: 96,
                output_start_utf8: 24,
                output_end_utf8: 28,
                rendered: "2025".to_string(),
            },
        ];

        assert_eq!(
            earliest_changed_rewrite_span_source_offset(&previous, &current),
            Some(24)
        );
    }

    #[test]
    fn shipout_replay_plan_chooses_checkpoint_before_dirty_page() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "aaaaaaaaaabbbbbbbbbbCCCCCCCCCC").expect("write main");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let shipout_a = compile_format_snapshot(&mut interner, r"\def\fmt{a}");
        let shipout_b = compile_format_snapshot(&mut interner, r"\def\fmt{b}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            4,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                shipout_a,
                shipout_b,
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 4,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-4/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-4/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 10,
                        end_utf8: 20,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-4/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 20,
                        end_utf8: 30,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "aaaaaaaaaabbbbbbbbbbcccccccccc".to_string(),
            )]),
            executed_sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "aaaaaaaaaabbbbbbbbbbcccccccccc".to_string(),
            )]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };
        fs::write(root.join("main.tex"), "aaaaaaaaaabbbbbbbbbbZZZZZZZZZZ").expect("rewrite main");

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("main.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 2);
        assert_eq!(plan.checkpoint.source_offset_utf8, 20);
        assert_eq!(plan.output_prefix, "page0\npage1\n");
        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[2].meta.checkpoint_id
        );
    }

    #[test]
    fn shipout_replay_plan_uses_checkpoint_offsets_when_toplevel_spans_are_missing() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "aaaaaaaaaabbbbbbbbbbbbZZZZZZZZZZ").expect("write main");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            8,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 8,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-8/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("chapter-0.tex"),
                        start_utf8: 0,
                        end_utf8: 18,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-8/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("chapter-1.tex"),
                        start_utf8: 18,
                        end_utf8: 40,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-8/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("chapter-2.tex"),
                        start_utf8: 40,
                        end_utf8: 60,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "aaaaaaaaaabbbbbbbbbbbbcccccccccc".to_string(),
            )]),
            executed_sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "aaaaaaaaaabbbbbbbbbbbbcccccccccc".to_string(),
            )]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("main.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 2);
        assert_eq!(plan.checkpoint.source_offset_utf8, 20);
        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[2].meta.checkpoint_id
        );
    }

    #[test]
    fn shipout_replay_plan_uses_last_toplevel_span_when_diff_is_after_all_spans() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        let previous_source = "a".repeat(400);
        fs::write(root.join("main.tex"), &previous_source).expect("write main");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            40,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 40,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-40/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 120,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-40/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 121,
                        end_utf8: 240,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-40/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 241,
                        end_utf8: 360,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([(Utf8PathBuf::from("main.tex"), previous_source.clone())]),
            executed_sources: BTreeMap::from([(Utf8PathBuf::from("main.tex"), previous_source)]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };
        fs::write(root.join("main.tex"), format!("{}Z", "a".repeat(400))).expect("rewrite main");

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("main.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 3);
        assert_eq!(plan.checkpoint.source_offset_utf8, 30);
        assert_eq!(plan.output_prefix, "page0\npage1\npage2");
        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[3].meta.checkpoint_id
        );
    }

    #[test]
    fn shipout_replay_plan_uses_module_checkpoint_for_non_toplevel_input() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/tail.tex"), "tail-B").expect("write tail");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            6,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/tail.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 14,
                continuation_stack: Vec::new(),
                output_start_utf8: 12,
                page_index_after: 2,
                snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{tail}"),
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 6,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-6/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 120,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-6/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 121,
                        end_utf8: 240,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-6/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 241,
                        end_utf8: 360,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (Utf8PathBuf::from("sections/tail.tex"), "tail-A".to_string()),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (Utf8PathBuf::from("sections/tail.tex"), "tail-A".to_string()),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("sections/tail.tex"),
                source_start_utf8: 0,
                source_end_utf8: "tail-A".len() as u32,
                output_start_utf8: 12,
                output_end_utf8: 17,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/tail.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 2);
        assert_eq!(plan.checkpoint.source_offset_utf8, 14);
        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/tail.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("input boundary checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
    }

    #[test]
    fn shipout_replay_plan_uses_module_enter_checkpoint_for_deleted_input_file() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{tail}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            11,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/tail.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 14,
                continuation_stack: Vec::new(),
                output_start_utf8: 12,
                page_index_after: 2,
                snapshot: enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 11,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-11/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 120,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-11/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 121,
                        end_utf8: 240,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-11/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 241,
                        end_utf8: 360,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (Utf8PathBuf::from("sections/tail.tex"), "tail-A".to_string()),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (Utf8PathBuf::from("sections/tail.tex"), "tail-A".to_string()),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("sections/tail.tex"),
                source_start_utf8: 0,
                source_end_utf8: "tail-A".len() as u32,
                output_start_utf8: 12,
                output_end_utf8: 17,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/tail.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/tail.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("input boundary checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 2);
        assert_eq!(plan.output_prefix, "page0\npage1\n");
    }

    #[test]
    fn shipout_replay_plan_prefers_earliest_enter_checkpoint_for_deleted_repeated_input_file() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let first_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{tail-a}");
        let second_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{tail-b}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            32,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship2}"),
            ],
            &[6, 12, 18],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/tail.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 8,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 6,
                    page_index_after: 1,
                    snapshot: first_enter_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/tail.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 14,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 12,
                    page_index_after: 2,
                    snapshot: second_enter_snapshot,
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 32,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-32/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 120,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-32/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 121,
                        end_utf8: 240,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-32/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 241,
                        end_utf8: 360,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("sections/tail.tex"),
                source_start_utf8: 0,
                source_end_utf8: "tail-old".len() as u32,
                output_start_utf8: 12,
                output_end_utf8: 17,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/tail.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/tail.tex"))
                    && checkpoint.meta.output_start_utf8 == 6
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("earliest repeated enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\n");
    }

    #[test]
    fn shipout_replay_plan_falls_back_to_cp0_for_deleted_input_without_enter_checkpoint() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            33,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 33,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-33/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 120,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-33/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 121,
                        end_utf8: 240,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-33/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 241,
                        end_utf8: 360,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (Utf8PathBuf::from("sections/tail.tex"), "tail-A".to_string()),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (Utf8PathBuf::from("sections/tail.tex"), "tail-A".to_string()),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("sections/tail.tex"),
                source_start_utf8: 0,
                source_end_utf8: "tail-A".len() as u32,
                output_start_utf8: 12,
                output_end_utf8: 17,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/tail.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_keeps_deleted_input_cp0_fallback_across_multiple_changed_files() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("write appendix");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{appendix}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            34,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 28,
                continuation_stack: Vec::new(),
                output_start_utf8: 12,
                page_index_after: 2,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 34,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-34/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 40,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-34/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/tail.tex"),
                        start_utf8: 0,
                        end_utf8: 6,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-34/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: 12,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![
                StoredModuleTrace {
                    path: Utf8PathBuf::from("sections/tail.tex"),
                    source_start_utf8: 0,
                    source_end_utf8: 8,
                    output_start_utf8: 6,
                    output_end_utf8: 11,
                },
                StoredModuleTrace {
                    path: Utf8PathBuf::from("sections/appendix.tex"),
                    source_start_utf8: 0,
                    source_end_utf8: 12,
                    output_start_utf8: 12,
                    output_end_utf8: 17,
                },
            ],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("sections/appendix.tex"),
            ],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_prefers_deleted_input_enter_boundary_across_multiple_changed_files() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("write appendix");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let tail_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{tail}");
        let appendix_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{appendix}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            12,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship2}"),
            ],
            &[6, 12, 18],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/tail.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 14,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 6,
                    page_index_after: 1,
                    snapshot: tail_enter_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/appendix.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 28,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 12,
                    page_index_after: 2,
                    snapshot: appendix_enter_snapshot,
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 12,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-12/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 40,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-12/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/tail.tex"),
                        start_utf8: 0,
                        end_utf8: 6,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-12/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: 12,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![
                StoredModuleTrace {
                    path: Utf8PathBuf::from("sections/tail.tex"),
                    source_start_utf8: 0,
                    source_end_utf8: 8,
                    output_start_utf8: 6,
                    output_end_utf8: 11,
                },
                StoredModuleTrace {
                    path: Utf8PathBuf::from("sections/appendix.tex"),
                    source_start_utf8: 0,
                    source_end_utf8: 12,
                    output_start_utf8: 12,
                    output_end_utf8: 17,
                },
            ],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("sections/appendix.tex"),
            ],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/tail.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("tail enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\n");
    }

    #[test]
    fn shipout_replay_plan_uses_input_boundary_page_index_after() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("sections_tail.tex"), "tail-new").expect("write tail");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let exit_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{exit}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            16,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Exit,
                module_path: Utf8PathBuf::from("sections_tail.tex"),
                resume_path: Some(Utf8PathBuf::from("sections_tail.tex")),
                source_offset_utf8: 4,
                continuation_stack: Vec::new(),
                output_start_utf8: 8,
                page_index_after: 2,
                snapshot: exit_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 16,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-16/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections_tail.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-16/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 29,
                    pdf_artifact_path: Utf8PathBuf::from("rev-16/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 11,
                        end_utf8: 20,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections_tail.tex"),
                    "tail-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections_tail.tex"),
                    "tail-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections_tail.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 2);
        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.resume_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections_tail.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("input boundary checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
    }

    #[test]
    fn shipout_replay_plan_uses_partial_module_trace_source_range() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg.sty"), "prefix suffix OLD").expect("write package");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            12,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 12,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-12/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-12/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 11,
                        end_utf8: 20,
                    }],
                },
            ],
            output: "page0\npage1".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("pkg.sty"),
                source_start_utf8: "prefix ".len() as u32,
                source_end_utf8: "prefix suffix".len() as u32,
                output_start_utf8: 10,
                output_end_utf8: 19,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        fs::write(root.join("pkg.sty"), "prefix suffiX OLD").expect("rewrite package");

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("pkg.sty")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\npage");
    }

    #[test]
    fn shipout_replay_plan_returns_none_for_page_zero_trace_only_candidate() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg.sty"), "prefix suffix OLD").expect("write package");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            34,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{a}")],
            &[10],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 34,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 9,
                pdf_artifact_path: Utf8PathBuf::from("rev-34/pages/p0.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 20,
                }],
            }],
            output: "page0".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("pkg.sty"),
                source_start_utf8: 0,
                source_end_utf8: "prefix".len() as u32,
                output_start_utf8: 0,
                output_end_utf8: 5,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        fs::write(root.join("pkg.sty"), "prefiX suffix OLD").expect("rewrite package");

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("pkg.sty")],
            None,
        )
        .expect("plan");

        assert!(plan.is_none());
    }

    #[test]
    fn shipout_replay_plan_ignores_page_zero_trace_only_candidate_across_multiple_changed_files() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("pkg.sty"), "prefix suffix OLD").expect("write package");
        fs::write(root.join("sections/appendix.tex"), "appendix-old").expect("write appendix");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{appendix}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            36,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 28,
                continuation_stack: Vec::new(),
                output_start_utf8: 12,
                page_index_after: 2,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 36,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-36/pages/p0.pdf"),
                    source_spans: vec![
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("main.tex"),
                            start_utf8: 0,
                            end_utf8: 40,
                        },
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg.sty"),
                            start_utf8: 0,
                            end_utf8: "prefix suffix OLD".len() as u32,
                        },
                    ],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-36/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 41,
                        end_utf8: 80,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-36/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: "appendix-old".len() as u32,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("pkg.sty"),
                source_start_utf8: 0,
                source_end_utf8: "prefix".len() as u32,
                output_start_utf8: 0,
                output_end_utf8: 5,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        fs::write(root.join("pkg.sty"), "prefiX suffix OLD").expect("rewrite package");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("rewrite appendix");

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("pkg.sty"),
                Utf8PathBuf::from("sections/appendix.tex"),
            ],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/appendix.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("appendix enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 2);
        assert_eq!(plan.output_prefix, "page0\npage1\n");
    }

    #[test]
    fn shipout_replay_plan_returns_none_for_page_zero_placeholder_only_candidate() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg.sty"), "prefix suffix OLD").expect("write package");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            37,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{a}")],
            &[10],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 37,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 9,
                pdf_artifact_path: Utf8PathBuf::from("rev-37/pages/p0.pdf"),
                source_spans: vec![
                    ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    },
                    ArtifactSourceSpan {
                        file: Utf8PathBuf::from("pkg.sty"),
                        start_utf8: 0,
                        end_utf8: "prefix suffix OLD".len() as u32,
                    },
                ],
            }],
            output: "page0".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        fs::write(root.join("pkg.sty"), "prefix suffiX OLD").expect("rewrite package");

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("pkg.sty")],
            None,
        )
        .expect("plan");

        assert!(plan.is_none());
    }

    #[test]
    fn shipout_replay_plan_ignores_page_zero_placeholder_only_candidate_across_multiple_changed_files()
     {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("pkg.sty"), "prefix suffix OLD").expect("write package");
        fs::write(root.join("sections/appendix.tex"), "appendix-old").expect("write appendix");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{appendix}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            39,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 28,
                continuation_stack: Vec::new(),
                output_start_utf8: 12,
                page_index_after: 2,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 39,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-39/pages/p0.pdf"),
                    source_spans: vec![
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("main.tex"),
                            start_utf8: 0,
                            end_utf8: 40,
                        },
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg.sty"),
                            start_utf8: 0,
                            end_utf8: "prefix suffix OLD".len() as u32,
                        },
                    ],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-39/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 41,
                        end_utf8: 80,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-39/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: "appendix-old".len() as u32,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        fs::write(root.join("pkg.sty"), "prefix suffiX OLD").expect("rewrite package");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("rewrite appendix");

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("pkg.sty"),
                Utf8PathBuf::from("sections/appendix.tex"),
            ],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/appendix.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("appendix enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 2);
        assert_eq!(plan.output_prefix, "page0\npage1\n");
    }

    #[test]
    fn shipout_replay_plan_clamps_trace_only_candidate_to_last_page() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg.sty"), "prefix suffix OLD").expect("write package");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            35,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 35,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-35/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-35/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 11,
                        end_utf8: 20,
                    }],
                },
            ],
            output: "page0\npage1".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("pkg.sty"),
                source_start_utf8: 0,
                source_end_utf8: "prefix suffix".len() as u32,
                output_start_utf8: 30,
                output_end_utf8: 40,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        fs::write(root.join("pkg.sty"), "prefix suffiX OLD").expect("rewrite package");

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("pkg.sty")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\npage");
    }

    #[test]
    fn shipout_replay_plan_falls_back_to_cp0_when_override_changes_toplevel_earlier_than_changed_input()
     {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "raw main").expect("write main");
        fs::write(root.join("refs.bbl"), "[1] Alpha entry.").expect("write bbl");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            18,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("refs.bbl"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 80,
                continuation_stack: Vec::new(),
                output_start_utf8: 12,
                page_index_after: 1,
                snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{refs}"),
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 18,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-18/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 40,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 23,
                    pdf_artifact_path: Utf8PathBuf::from("rev-18/pages/p1.pdf"),
                    source_spans: vec![
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("main.tex"),
                            start_utf8: 41,
                            end_utf8: 120,
                        },
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("refs.bbl"),
                            start_utf8: 0,
                            end_utf8: 16,
                        },
                    ],
                },
            ],
            output: "page0-cite\npage1-bib".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "raw main".to_string()),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "See [1] before bibliography. \\input{refs.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("refs.bbl"),
                source_start_utf8: 0,
                source_end_utf8: 16,
                output_start_utf8: 12,
                output_end_utf8: 28,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("refs.bbl")],
            Some(&BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "See [2] before bibliography. \\input{refs.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Beta entry.\n[2] Alpha entry.".to_string(),
                ),
            ])),
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 0);
        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_ignores_raw_checkpoint_offsets_for_override_only_toplevel_diff() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "Intro. Cite \\cite{alpha}. \\bibliography{refs}",
        )
        .expect("write main");
        fs::write(root.join("refs.bbl"), "[1] Alpha entry.").expect("write bbl");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            19,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
            ],
            &[12, 24],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("refs.bbl"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 24,
                continuation_stack: Vec::new(),
                output_start_utf8: 18,
                page_index_after: 1,
                snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{refs}"),
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 19,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 18,
                    pdf_artifact_path: Utf8PathBuf::from("rev-19/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 23,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 18,
                    text_end_utf8: 32,
                    pdf_artifact_path: Utf8PathBuf::from("rev-19/pages/p1.pdf"),
                    source_spans: vec![
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("main.tex"),
                            start_utf8: 23,
                            end_utf8: 48,
                        },
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("refs.bbl"),
                            start_utf8: 0,
                            end_utf8: 16,
                        },
                    ],
                },
            ],
            output: "page0-cite\npage1-bib".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "Intro. Cite \\cite{alpha}. \\bibliography{refs}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "Intro. Cite [1]. \\input{refs.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("refs.bbl"),
                source_start_utf8: 0,
                source_end_utf8: 16,
                output_start_utf8: 18,
                output_end_utf8: 34,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("refs.bbl")],
            Some(&BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "Intro. Cite [2]. \\input{refs.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Beta entry.\n[2] Alpha entry.".to_string(),
                ),
            ])),
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 0);
        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_prefers_earlier_same_page_semantic_override_boundary() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "See \\cite{alpha}. \\bibliography{refs}",
        )
        .expect("write main");
        fs::write(root.join("refs.bbl"), "[1] Alpha entry.").expect("write bbl");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            23,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{ship0}")],
            &[24],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("refs.bbl"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 18,
                continuation_stack: Vec::new(),
                output_start_utf8: 9,
                page_index_after: 0,
                snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{refs}"),
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 23,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 24,
                pdf_artifact_path: Utf8PathBuf::from("rev-23/pages/p0.pdf"),
                source_spans: vec![
                    ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 35,
                    },
                    ArtifactSourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 16,
                    },
                ],
            }],
            output: "See [1].\n[1] Alpha entry.".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "See \\cite{alpha}. \\bibliography{refs}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "See [1]. \\input{refs.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                vec![MaterializedRewriteSpan {
                    start_utf8: 4,
                    end_utf8: 16,
                    output_start_utf8: 4,
                    output_end_utf8: 7,
                    rendered: "[1]".to_string(),
                }],
            )]),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("refs.bbl"),
                source_start_utf8: 0,
                source_end_utf8: 16,
                output_start_utf8: 9,
                output_end_utf8: 25,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("refs.bbl")],
            Some(&BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "See [2]. \\input{refs.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Beta entry.\n[2] Alpha entry.".to_string(),
                ),
            ])),
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                vec![MaterializedRewriteSpan {
                    start_utf8: 4,
                    end_utf8: 16,
                    output_start_utf8: 4,
                    output_end_utf8: 7,
                    rendered: "[2]".to_string(),
                }],
            )])),
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 0);
        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_uses_semantic_rewrite_span_for_override_only_late_toplevel_diff() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "Intro filler filler filler filler filler filler filler filler filler filler \\citeyear{beta}. \\bibliography{refs}",
        )
        .expect("write main");
        fs::write(root.join("refs.bbl"), "[1] Alpha entry.").expect("write bbl");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            22,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
            ],
            &[70, 120],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("refs.bbl"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 101,
                continuation_stack: Vec::new(),
                output_start_utf8: 90,
                page_index_after: 1,
                snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{refs}"),
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 22,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 90,
                    pdf_artifact_path: Utf8PathBuf::from("rev-22/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 96,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 90,
                    text_end_utf8: 118,
                    pdf_artifact_path: Utf8PathBuf::from("rev-22/pages/p1.pdf"),
                    source_spans: vec![
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("main.tex"),
                            start_utf8: 96,
                            end_utf8: 120,
                        },
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("refs.bbl"),
                            start_utf8: 0,
                            end_utf8: 16,
                        },
                    ],
                },
            ],
            output: "page0-prefix\npage1-late".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "Intro filler filler filler filler filler filler filler filler filler filler \\citeyear{beta}. \\bibliography{refs}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "Intro filler filler filler filler filler filler filler filler filler filler 2024. \\input{refs.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                vec![MaterializedRewriteSpan {
                    start_utf8: 77,
                    end_utf8: 92,
                    output_start_utf8: 72,
                    output_end_utf8: 76,
                    rendered: "2024".to_string(),
                }],
            )]),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("refs.bbl"),
                source_start_utf8: 0,
                source_end_utf8: 16,
                output_start_utf8: 94,
                output_end_utf8: 110,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("refs.bbl")],
            Some(&BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "Intro filler filler filler filler filler filler filler filler filler filler 2025. \\input{refs.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
            ])),
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                vec![MaterializedRewriteSpan {
                    start_utf8: 77,
                    end_utf8: 92,
                    output_start_utf8: 72,
                    output_end_utf8: 76,
                    rendered: "2025".to_string(),
                }],
            )])),
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 0);
        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_uses_earliest_enter_checkpoint_for_override_only_non_toplevel_diff() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\input{sections/body}").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "cite \\cite{alpha}").expect("write body");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let body_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{body-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            20,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{ship0}")],
            &[10],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/body.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 0,
                continuation_stack: Vec::new(),
                output_start_utf8: 0,
                page_index_after: 0,
                snapshot: body_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 20,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 18,
                pdf_artifact_path: Utf8PathBuf::from("rev-20/pages/p0.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("sections/body.tex"),
                    start_utf8: 0,
                    end_utf8: 17,
                }],
            }],
            output: "body output".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite \\cite{alpha}".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite [1]".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[],
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                "cite [2]".to_string(),
            )])),
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/body.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("body enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_uses_override_only_non_toplevel_boundary_without_reading_disk_file() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\input{sections/body}").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::create_dir_all(root.join("sections/body.tex")).expect("body dir");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let body_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{body-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            46,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{ship0}")],
            &[10],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/body.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 0,
                continuation_stack: Vec::new(),
                output_start_utf8: 0,
                page_index_after: 0,
                snapshot: body_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 46,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 18,
                pdf_artifact_path: Utf8PathBuf::from("rev-46/pages/p0.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("sections/body.tex"),
                    start_utf8: 0,
                    end_utf8: 17,
                }],
            }],
            output: "body output".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite \\cite{alpha}".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite [1]".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[],
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                "cite [2]".to_string(),
            )])),
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/body.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("body enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_falls_back_to_cp0_for_override_only_non_toplevel_diff_without_enter_checkpoint()
     {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\input{sections/body}").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "cite \\cite{alpha}").expect("write body");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            47,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{ship0}")],
            &[10],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 47,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 18,
                pdf_artifact_path: Utf8PathBuf::from("rev-47/pages/p0.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("sections/body.tex"),
                    start_utf8: 0,
                    end_utf8: 17,
                }],
            }],
            output: "body output".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite \\cite{alpha}".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite [1]".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[],
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                "cite [2]".to_string(),
            )])),
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_prefers_override_only_non_toplevel_cp0_fallback_over_later_changed_file()
    {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\input{sections/body}\\input{sections/appendix}",
        )
        .expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "cite \\cite{alpha}").expect("write body");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("write appendix");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot =
            compile_format_snapshot(&mut interner, r"\def\fmt{appendix-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            48,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 22,
                continuation_stack: Vec::new(),
                output_start_utf8: 10,
                page_index_after: 1,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 48,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                    pdf_artifact_path: Utf8PathBuf::from("rev-48/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/body.tex"),
                        start_utf8: 0,
                        end_utf8: 17,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 22,
                    pdf_artifact_path: Utf8PathBuf::from("rev-48/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: 12,
                    }],
                },
            ],
            output: "page0\npage1".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite \\cite{alpha}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite [1]".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/appendix.tex")],
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                "cite [2]".to_string(),
            )])),
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_falls_back_to_cp0_for_override_only_non_toplevel_semantic_diff_without_enter_checkpoint()
     {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\input{sections/body}").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "cite \\cite{alpha}").expect("write body");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            49,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
            ],
            &[10, 20],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 49,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-49/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 22,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 21,
                    pdf_artifact_path: Utf8PathBuf::from("rev-49/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/body.tex"),
                        start_utf8: 0,
                        end_utf8: 17,
                    }],
                },
            ],
            output: "page0-prefix\npage1-body".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite \\cite{alpha}".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite [1]".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                vec![MaterializedRewriteSpan {
                    start_utf8: 5,
                    end_utf8: 17,
                    output_start_utf8: 5,
                    output_end_utf8: 8,
                    rendered: "[1]".to_string(),
                }],
            )]),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("sections/body.tex"),
                source_start_utf8: 0,
                source_end_utf8: 17,
                output_start_utf8: 12,
                output_end_utf8: 22,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[],
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                "cite [2]".to_string(),
            )])),
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                vec![MaterializedRewriteSpan {
                    start_utf8: 5,
                    end_utf8: 17,
                    output_start_utf8: 5,
                    output_end_utf8: 8,
                    rendered: "[2]".to_string(),
                }],
            )])),
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| checkpoint.meta.kind == CheckpointKind::Preamble)
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("cp0 checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_uses_enter_checkpoint_for_override_only_non_toplevel_semantic_diff() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\input{sections/body}").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "cite \\cite{alpha}").expect("write body");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let body_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{body-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            50,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{ship0}")],
            &[10],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/body.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 0,
                continuation_stack: Vec::new(),
                output_start_utf8: 0,
                page_index_after: 0,
                snapshot: body_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 50,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 18,
                pdf_artifact_path: Utf8PathBuf::from("rev-50/pages/p0.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("sections/body.tex"),
                    start_utf8: 0,
                    end_utf8: 17,
                }],
            }],
            output: "body output".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite \\cite{alpha}".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite [1]".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                vec![MaterializedRewriteSpan {
                    start_utf8: 5,
                    end_utf8: 17,
                    output_start_utf8: 5,
                    output_end_utf8: 8,
                    rendered: "[1]".to_string(),
                }],
            )]),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("sections/body.tex"),
                source_start_utf8: 0,
                source_end_utf8: 17,
                output_start_utf8: 0,
                output_end_utf8: 11,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[],
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                "cite [2]".to_string(),
            )])),
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                vec![MaterializedRewriteSpan {
                    start_utf8: 5,
                    end_utf8: 17,
                    output_start_utf8: 5,
                    output_end_utf8: 8,
                    rendered: "[2]".to_string(),
                }],
            )])),
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/body.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("body enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_uses_enter_checkpoint_for_override_only_non_toplevel_semantic_diff_without_reading_disk_file()
     {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\input{sections/body}").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::create_dir_all(root.join("sections/body.tex")).expect("body dir");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let body_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{body-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            50,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{ship0}")],
            &[10],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/body.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 0,
                continuation_stack: Vec::new(),
                output_start_utf8: 0,
                page_index_after: 0,
                snapshot: body_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 50,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 18,
                pdf_artifact_path: Utf8PathBuf::from("rev-50/pages/p0.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("sections/body.tex"),
                    start_utf8: 0,
                    end_utf8: 17,
                }],
            }],
            output: "body output".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite \\cite{alpha}".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite [1]".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                vec![MaterializedRewriteSpan {
                    start_utf8: 5,
                    end_utf8: 17,
                    output_start_utf8: 5,
                    output_end_utf8: 8,
                    rendered: "[1]".to_string(),
                }],
            )]),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("sections/body.tex"),
                source_start_utf8: 0,
                source_end_utf8: 17,
                output_start_utf8: 0,
                output_end_utf8: 11,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[],
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                "cite [2]".to_string(),
            )])),
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                vec![MaterializedRewriteSpan {
                    start_utf8: 5,
                    end_utf8: 17,
                    output_start_utf8: 5,
                    output_end_utf8: 8,
                    rendered: "[2]".to_string(),
                }],
            )])),
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/body.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("body enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_prefers_override_only_non_toplevel_semantic_cp0_fallback_over_later_changed_file()
     {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\input{sections/body}\\input{sections/appendix}",
        )
        .expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "cite \\cite{alpha}").expect("write body");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("write appendix");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot =
            compile_format_snapshot(&mut interner, r"\def\fmt{appendix-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            51,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 22,
                continuation_stack: Vec::new(),
                output_start_utf8: 10,
                page_index_after: 1,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 51,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-51/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/body.tex"),
                        start_utf8: 0,
                        end_utf8: 17,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-51/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: 12,
                    }],
                },
            ],
            output: "page0-body\npage1-app".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite \\cite{alpha}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite [1]".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                vec![MaterializedRewriteSpan {
                    start_utf8: 5,
                    end_utf8: 17,
                    output_start_utf8: 5,
                    output_end_utf8: 8,
                    rendered: "[1]".to_string(),
                }],
            )]),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/appendix.tex")],
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                "cite [2]".to_string(),
            )])),
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                vec![MaterializedRewriteSpan {
                    start_utf8: 5,
                    end_utf8: 17,
                    output_start_utf8: 5,
                    output_end_utf8: 8,
                    rendered: "[2]".to_string(),
                }],
            )])),
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_prefers_earlier_override_only_semantic_boundary_over_later_changed_file()
    {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\input{sections/body}\\input{sections/appendix}",
        )
        .expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "cite \\cite{alpha}").expect("write body");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("write appendix");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let body_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{body-enter}");
        let appendix_enter_snapshot =
            compile_format_snapshot(&mut interner, r"\def\fmt{appendix-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            21,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
            ],
            &[10, 20],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/body.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 0,
                    page_index_after: 0,
                    snapshot: body_enter_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/appendix.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 22,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 10,
                    page_index_after: 1,
                    snapshot: appendix_enter_snapshot,
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 21,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-21/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/body.tex"),
                        start_utf8: 0,
                        end_utf8: 17,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-21/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: 12,
                    }],
                },
            ],
            output: "page0-body\npage1-app".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite \\cite{alpha}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "cite [1]".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/appendix.tex")],
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("sections/body.tex"),
                "cite [2]".to_string(),
            )])),
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/body.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("body enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_prefers_non_toplevel_checkpoint_over_equally_early_cp0_override() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\input{sections/body}\\bibliography{refs}",
        )
        .expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "Body cite \\cite{alpha}.").expect("write body");
        fs::write(root.join("refs.bbl"), "[1] Beta entry.\n[2] Alpha entry.").expect("write bbl");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let body_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{body-enter}");
        let refs_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{refs-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            21,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{ship0}")],
            &[10],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/body.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 0,
                    page_index_after: 0,
                    snapshot: body_enter_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("refs.bbl"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 24,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 14,
                    page_index_after: 0,
                    snapshot: refs_enter_snapshot,
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 21,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 32,
                pdf_artifact_path: Utf8PathBuf::from("rev-21/pages/p0.pdf"),
                source_spans: vec![
                    ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/body.tex"),
                        start_utf8: 0,
                        end_utf8: 23,
                    },
                    ArtifactSourceSpan {
                        file: Utf8PathBuf::from("refs.bbl"),
                        start_utf8: 0,
                        end_utf8: 16,
                    },
                ],
            }],
            output: "Body cite [1].\n[1] Alpha entry.".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\bibliography{refs}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "Body cite \\cite{alpha}.".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "Body cite [1].\\input{refs.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "Body cite [1].".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![
                StoredModuleTrace {
                    path: Utf8PathBuf::from("sections/body.tex"),
                    source_start_utf8: 0,
                    source_end_utf8: 23,
                    output_start_utf8: 0,
                    output_end_utf8: 13,
                },
                StoredModuleTrace {
                    path: Utf8PathBuf::from("refs.bbl"),
                    source_start_utf8: 0,
                    source_end_utf8: 16,
                    output_start_utf8: 14,
                    output_end_utf8: 30,
                },
            ],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("refs.bbl")],
            Some(&BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "Body cite [2].\\input{refs.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "Body cite [2].".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Beta entry.\n[2] Alpha entry.".to_string(),
                ),
            ])),
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/body.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("body enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_forces_earliest_enter_checkpoint_for_semantic_bibliography_change() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\bibliography{refs}").expect("write main");
        fs::write(root.join("refs.bbl"), "[1] Alpha entry.").expect("write bbl");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            24,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{ship0}")],
            &[30],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("refs.bbl"),
                    resume_path: Some(Utf8PathBuf::from("refs.bbl")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 8,
                    page_index_after: 0,
                    snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{refs-enter}"),
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("refs.bbl"),
                    resume_path: Some(Utf8PathBuf::from("refs.bbl")),
                    source_offset_utf8: 12,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 20,
                    page_index_after: 0,
                    snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{refs-late}"),
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 24,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 30,
                pdf_artifact_path: Utf8PathBuf::from("rev-24/pages/p0.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("refs.bbl"),
                    start_utf8: 0,
                    end_utf8: 16,
                }],
            }],
            output: "Prelude [1] Alpha entry.\nTail".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\bibliography{refs}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{refs.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("refs.bbl"),
                source_start_utf8: 0,
                source_end_utf8: 16,
                output_start_utf8: 8,
                output_end_utf8: 24,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("refs.bbl")],
            Some(&BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{refs.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refs.bbl"),
                    "[1] Alpha revised entry.".to_string(),
                ),
            ])),
            None,
            Some(&[Utf8PathBuf::from("refs.bbl")]),
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("refs.bbl"))
                    && checkpoint.meta.output_start_utf8 == 8
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("refs enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "Prelude ");
    }

    #[test]
    fn shipout_replay_plan_prefers_force_conservative_cp0_fallback_over_later_changed_file() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\input{sections/body}\\input{sections/appendix}",
        )
        .expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "body-new").expect("write body");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot =
            compile_format_snapshot(&mut interner, r"\def\fmt{appendix-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            28,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 12,
                continuation_stack: Vec::new(),
                output_start_utf8: 10,
                page_index_after: 1,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 28,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-28/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/body.tex"),
                        start_utf8: 0,
                        end_utf8: 8,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 21,
                    pdf_artifact_path: Utf8PathBuf::from("rev-28/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: 12,
                    }],
                },
            ],
            output: "page0-body\npage1-app".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("sections/body.tex"),
                Utf8PathBuf::from("sections/appendix.tex"),
            ],
            None,
            None,
            Some(&[Utf8PathBuf::from("sections/body.tex")]),
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_prefers_force_conservative_toplevel_cp0_over_later_changed_file() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\input{sections/body}\\input{sections/appendix}% changed",
        )
        .expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "body-old").expect("write body");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("write appendix");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot =
            compile_format_snapshot(&mut interner, r"\def\fmt{appendix-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            30,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 22,
                continuation_stack: Vec::new(),
                output_start_utf8: 10,
                page_index_after: 1,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 30,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-30/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/body.tex"),
                        start_utf8: 0,
                        end_utf8: 8,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 21,
                    pdf_artifact_path: Utf8PathBuf::from("rev-30/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: 12,
                    }],
                },
            ],
            output: "page0-body\npage1-app".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/appendix.tex"),
            ],
            None,
            None,
            Some(&[Utf8PathBuf::from("main.tex")]),
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_prefers_force_conservative_toplevel_cp0_over_later_changed_file_with_reversed_order()
     {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\input{sections/body}\\input{sections/appendix}% changed",
        )
        .expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "body-old").expect("write body");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("write appendix");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot =
            compile_format_snapshot(&mut interner, r"\def\fmt{appendix-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            32,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 22,
                continuation_stack: Vec::new(),
                output_start_utf8: 10,
                page_index_after: 1,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 32,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-32/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/body.tex"),
                        start_utf8: 0,
                        end_utf8: 8,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 21,
                    pdf_artifact_path: Utf8PathBuf::from("rev-32/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: 12,
                    }],
                },
            ],
            output: "page0-body\npage1-app".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("sections/appendix.tex"),
                Utf8PathBuf::from("main.tex"),
            ],
            None,
            None,
            Some(&[Utf8PathBuf::from("main.tex")]),
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_prefers_force_conservative_cp0_fallback_over_later_changed_file_with_reversed_order()
     {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\input{sections/body}\\input{sections/appendix}",
        )
        .expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "body-new").expect("write body");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot =
            compile_format_snapshot(&mut interner, r"\def\fmt{appendix-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            30,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 12,
                continuation_stack: Vec::new(),
                output_start_utf8: 10,
                page_index_after: 1,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 30,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-30/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/body.tex"),
                        start_utf8: 0,
                        end_utf8: 8,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 21,
                    pdf_artifact_path: Utf8PathBuf::from("rev-30/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: 12,
                    }],
                },
            ],
            output: "page0-body\npage1-app".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("sections/appendix.tex"),
                Utf8PathBuf::from("sections/body.tex"),
            ],
            None,
            None,
            Some(&[Utf8PathBuf::from("sections/body.tex")]),
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_ignores_force_conservative_request_for_non_replay_dirty_file() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\input{sections/body}\\input{sections/appendix}",
        )
        .expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "body-old").expect("write body");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("write appendix");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot =
            compile_format_snapshot(&mut interner, r"\def\fmt{appendix-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            31,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 22,
                continuation_stack: Vec::new(),
                output_start_utf8: 10,
                page_index_after: 1,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 31,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-31/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/body.tex"),
                        start_utf8: 0,
                        end_utf8: 8,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 21,
                    pdf_artifact_path: Utf8PathBuf::from("rev-31/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: 12,
                    }],
                },
            ],
            output: "page0-body\npage1-app".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/appendix.tex")],
            None,
            None,
            Some(&[Utf8PathBuf::from("sections/body.tex")]),
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/appendix.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("appendix enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0-body");
    }

    #[test]
    fn shipout_replay_plan_prefers_force_conservative_enter_boundary_over_later_changed_file() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\input{sections/body}\\input{sections/appendix}",
        )
        .expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "body-new").expect("write body");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let body_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{body-enter}");
        let appendix_enter_snapshot =
            compile_format_snapshot(&mut interner, r"\def\fmt{appendix-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            29,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
            ],
            &[10, 20],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/body.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 0,
                    page_index_after: 0,
                    snapshot: body_enter_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/appendix.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 12,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 10,
                    page_index_after: 1,
                    snapshot: appendix_enter_snapshot,
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 29,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-29/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/body.tex"),
                        start_utf8: 0,
                        end_utf8: 8,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 21,
                    pdf_artifact_path: Utf8PathBuf::from("rev-29/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: 12,
                    }],
                },
            ],
            output: "page0-body\npage1-app".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("sections/body.tex"),
                Utf8PathBuf::from("sections/appendix.tex"),
            ],
            None,
            None,
            Some(&[Utf8PathBuf::from("sections/body.tex")]),
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/body.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("body enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_prefers_force_conservative_enter_boundary_over_later_changed_file_with_reversed_order()
     {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\input{sections/body}\\input{sections/appendix}",
        )
        .expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/body.tex"), "body-new").expect("write body");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let body_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{body-enter}");
        let appendix_enter_snapshot =
            compile_format_snapshot(&mut interner, r"\def\fmt{appendix-enter}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            31,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
            ],
            &[10, 20],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/body.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 0,
                    page_index_after: 0,
                    snapshot: body_enter_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/appendix.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 12,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 10,
                    page_index_after: 1,
                    snapshot: appendix_enter_snapshot,
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 31,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-31/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/body.tex"),
                        start_utf8: 0,
                        end_utf8: 8,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 21,
                    pdf_artifact_path: Utf8PathBuf::from("rev-31/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: 12,
                    }],
                },
            ],
            output: "page0-body\npage1-app".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/body}\\input{sections/appendix}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/body.tex"),
                    "body-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("sections/appendix.tex"),
                Utf8PathBuf::from("sections/body.tex"),
            ],
            None,
            None,
            Some(&[Utf8PathBuf::from("sections/body.tex")]),
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/body.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("body enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_prefers_shorter_same_page_prefix_across_multiple_semantic_files() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\bibliography{refsa}\\bibliography{refsb}",
        )
        .expect("write main");
        fs::write(root.join("refsa.bbl"), "[1] Alpha entry.").expect("write refsa");
        fs::write(root.join("refsb.bbl"), "[1] Beta entry.").expect("write refsb");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            27,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{ship0}")],
            &[48],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("refsa.bbl"),
                    resume_path: Some(Utf8PathBuf::from("refsa.bbl")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 8,
                    page_index_after: 0,
                    snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{refsa-enter}"),
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("refsb.bbl"),
                    resume_path: Some(Utf8PathBuf::from("refsb.bbl")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 24,
                    page_index_after: 0,
                    snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{refsb-enter}"),
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 27,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 48,
                pdf_artifact_path: Utf8PathBuf::from("rev-27/pages/p0.pdf"),
                source_spans: vec![
                    ArtifactSourceSpan {
                        file: Utf8PathBuf::from("refsa.bbl"),
                        start_utf8: 0,
                        end_utf8: 16,
                    },
                    ArtifactSourceSpan {
                        file: Utf8PathBuf::from("refsb.bbl"),
                        start_utf8: 0,
                        end_utf8: 15,
                    },
                ],
            }],
            output: "Prelude [1] Alpha entry. Middle [1] Beta entry.".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\bibliography{refsa}\\bibliography{refsb}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refsa.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
                (
                    Utf8PathBuf::from("refsb.bbl"),
                    "[1] Beta entry.".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{refsa.bbl}\\input{refsb.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refsa.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
                (
                    Utf8PathBuf::from("refsb.bbl"),
                    "[1] Beta entry.".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![
                StoredModuleTrace {
                    path: Utf8PathBuf::from("refsa.bbl"),
                    source_start_utf8: 0,
                    source_end_utf8: 16,
                    output_start_utf8: 8,
                    output_end_utf8: 24,
                },
                StoredModuleTrace {
                    path: Utf8PathBuf::from("refsb.bbl"),
                    source_start_utf8: 0,
                    source_end_utf8: 15,
                    output_start_utf8: 32,
                    output_end_utf8: 47,
                },
            ],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("refsa.bbl"),
            ],
            Some(&BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{refsa.bbl}\\input{refsb.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refsa.bbl"),
                    "[1] Alpha revised entry.".to_string(),
                ),
                (
                    Utf8PathBuf::from("refsb.bbl"),
                    "[1] Beta revised entry.".to_string(),
                ),
            ])),
            None,
            Some(&[
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("refsa.bbl"),
            ]),
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("refsa.bbl"))
                    && checkpoint.meta.output_start_utf8 == 8
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("refsa enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "Prelude ");
    }

    #[test]
    fn shipout_replay_plan_prefers_earlier_page_across_multiple_semantic_files() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "\\section{Intro}\\bibliography{refsa}\\bibliography{refsb}",
        )
        .expect("write main");
        fs::write(root.join("refsa.bbl"), "[1] Alpha entry.").expect("write refsa");
        fs::write(root.join("refsb.bbl"), "[1] Beta entry.").expect("write refsb");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            28,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{ship0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship1}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{ship2}"),
            ],
            &[8, 32, 64],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("refsa.bbl"),
                    resume_path: Some(Utf8PathBuf::from("refsa.bbl")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 32,
                    page_index_after: 1,
                    snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{refsa-enter}"),
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("refsb.bbl"),
                    resume_path: Some(Utf8PathBuf::from("refsb.bbl")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 64,
                    page_index_after: 2,
                    snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{refsb-enter}"),
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 28,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 31,
                    pdf_artifact_path: Utf8PathBuf::from("rev-28/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 56,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 32,
                    text_end_utf8: 63,
                    pdf_artifact_path: Utf8PathBuf::from("rev-28/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("refsa.bbl"),
                        start_utf8: 0,
                        end_utf8: 16,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 64,
                    text_end_utf8: 95,
                    pdf_artifact_path: Utf8PathBuf::from("rev-28/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("refsb.bbl"),
                        start_utf8: 0,
                        end_utf8: 15,
                    }],
                },
            ],
            output: "Prelude page. Alpha page. Beta page.".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\section{Intro}\\bibliography{refsa}\\bibliography{refsb}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refsa.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
                (
                    Utf8PathBuf::from("refsb.bbl"),
                    "[1] Beta entry.".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\section{Intro}\\input{refsa.bbl}\\input{refsb.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refsa.bbl"),
                    "[1] Alpha entry.".to_string(),
                ),
                (
                    Utf8PathBuf::from("refsb.bbl"),
                    "[1] Beta entry.".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![
                StoredModuleTrace {
                    path: Utf8PathBuf::from("refsa.bbl"),
                    source_start_utf8: 0,
                    source_end_utf8: 16,
                    output_start_utf8: 32,
                    output_end_utf8: 48,
                },
                StoredModuleTrace {
                    path: Utf8PathBuf::from("refsb.bbl"),
                    source_start_utf8: 0,
                    source_end_utf8: 15,
                    output_start_utf8: 64,
                    output_end_utf8: 79,
                },
            ],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan_with_spans(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("refsa.bbl"),
            ],
            Some(&BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\section{Intro}\\input{refsa.bbl}\\input{refsb.bbl}".to_string(),
                ),
                (
                    Utf8PathBuf::from("refsa.bbl"),
                    "[1] Alpha revised entry.".to_string(),
                ),
                (
                    Utf8PathBuf::from("refsb.bbl"),
                    "[1] Beta revised entry.".to_string(),
                ),
            ])),
            None,
            Some(&[
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("refsa.bbl"),
            ]),
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("refsa.bbl"))
                    && checkpoint.meta.page_index_after == 1
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("refsa enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "Prelude page. Alpha page. Beta p");
    }

    #[test]
    fn shipout_replay_plan_prefers_trace_page_over_earlier_placeholder_span() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg.sty"), "prefix suffix OLD").expect("write package");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            14,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 14,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-14/pages/p0.pdf"),
                    source_spans: vec![
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("main.tex"),
                            start_utf8: 0,
                            end_utf8: 10,
                        },
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg.sty"),
                            start_utf8: 0,
                            end_utf8: 16,
                        },
                    ],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-14/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 11,
                        end_utf8: 20,
                    }],
                },
            ],
            output: "page0\npage1".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("pkg.sty"),
                source_start_utf8: "prefix ".len() as u32,
                source_end_utf8: "prefix suffix".len() as u32,
                output_start_utf8: 10,
                output_end_utf8: 19,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        fs::write(root.join("pkg.sty"), "prefix suffiX OLD").expect("rewrite package");

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("pkg.sty")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\npage");
    }

    #[test]
    fn shipout_replay_plan_uses_last_placeholder_page_when_diff_is_after_all_spans() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg.sty"), "prefix suffix OLD").expect("write package");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            36,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 36,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-36/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-36/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("pkg.sty"),
                        start_utf8: 0,
                        end_utf8: 8,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 29,
                    pdf_artifact_path: Utf8PathBuf::from("rev-36/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("pkg.sty"),
                        start_utf8: 8,
                        end_utf8: "prefix suffix OLD".len() as u32,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        fs::write(root.join("pkg.sty"), "prefix suffix OLD EXTRA").expect("rewrite package");

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("pkg.sty")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 2);
        assert_eq!(plan.output_prefix, "page0\npage1\npage2");
    }

    #[test]
    fn shipout_replay_plan_falls_back_to_placeholder_page_when_trace_is_out_of_range() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg.sty"), "prefix suffix OLD").expect("write package");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            38,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 38,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-38/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-38/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("pkg.sty"),
                        start_utf8: 0,
                        end_utf8: "prefix suffix OLD".len() as u32,
                    }],
                },
            ],
            output: "page0\npage1".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("pkg.sty"),
                source_start_utf8: "prefix ".len() as u32,
                source_end_utf8: "prefix suffix".len() as u32,
                output_start_utf8: 10,
                output_end_utf8: 19,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        fs::write(root.join("pkg.sty"), "Prefix suffix OLD").expect("rewrite package");

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("pkg.sty")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\npage");
    }

    #[test]
    fn shipout_replay_plan_ignores_partial_module_trace_outside_changed_range() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg.sty"), "prefix suffix OLD").expect("write package");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            13,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 13,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-13/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-13/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 11,
                        end_utf8: 20,
                    }],
                },
            ],
            output: "page0\npage1".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("pkg.sty"),
                source_start_utf8: "prefix ".len() as u32,
                source_end_utf8: "prefix suffix".len() as u32,
                output_start_utf8: 10,
                output_end_utf8: 19,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        fs::write(root.join("pkg.sty"), "Prefix suffix OLD").expect("rewrite package");

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("pkg.sty")],
            None,
        )
        .expect("plan");

        assert!(plan.is_none());
    }

    #[test]
    fn shipout_replay_plan_returns_none_when_no_files_changed() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            14,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{a}")],
            &[10],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 14,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 9,
                pdf_artifact_path: Utf8PathBuf::from("rev-14/pages/p0.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 10,
                }],
            }],
            output: "page0".to_string(),
            sources: BTreeMap::from([(Utf8PathBuf::from("main.tex"), "main body".to_string())]),
            executed_sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "main body".to_string(),
            )]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan =
            select_shipout_replay_plan(&previous, &root, Utf8Path::new("main.tex"), &[], None)
                .expect("plan");

        assert!(plan.is_none());
    }

    #[test]
    fn shipout_replay_plan_returns_none_when_changed_file_cannot_be_read() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::create_dir_all(root.join("sections/tail.tex")).expect("tail dir");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            44,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/tail.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 14,
                continuation_stack: Vec::new(),
                output_start_utf8: 12,
                page_index_after: 1,
                snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{tail}"),
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 44,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-44/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 120,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-44/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/tail.tex"),
                        start_utf8: 0,
                        end_utf8: 8,
                    }],
                },
            ],
            output: "page0\npage1".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/tail.tex")],
            None,
        )
        .expect("plan");

        assert!(plan.is_none());
    }

    #[test]
    fn shipout_replay_plan_returns_none_for_readable_untracked_changed_file() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("notes.txt"), "fresh scratch notes").expect("write notes");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            46,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{a}")],
            &[10],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 46,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 9,
                pdf_artifact_path: Utf8PathBuf::from("rev-46/pages/p0.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 9,
                }],
            }],
            output: "page0".to_string(),
            sources: BTreeMap::from([(Utf8PathBuf::from("main.tex"), "main body".to_string())]),
            executed_sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "main body".to_string(),
            )]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("notes.txt")],
            None,
        )
        .expect("plan");

        assert!(plan.is_none());
    }

    #[test]
    fn shipout_replay_plan_returns_none_when_later_changed_file_is_readable_but_untracked() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("write appendix");
        fs::write(root.join("notes.txt"), "fresh scratch notes").expect("write notes");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{appendix}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            47,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 28,
                continuation_stack: Vec::new(),
                output_start_utf8: 12,
                page_index_after: 2,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 47,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-47/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 40,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-47/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/tail.tex"),
                        start_utf8: 0,
                        end_utf8: 8,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-47/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: "appendix-old".len() as u32,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![
                StoredModuleTrace {
                    path: Utf8PathBuf::from("sections/tail.tex"),
                    source_start_utf8: 0,
                    source_end_utf8: 8,
                    output_start_utf8: 6,
                    output_end_utf8: 11,
                },
                StoredModuleTrace {
                    path: Utf8PathBuf::from("sections/appendix.tex"),
                    source_start_utf8: 0,
                    source_end_utf8: "appendix-old".len() as u32,
                    output_start_utf8: 12,
                    output_end_utf8: 17,
                },
            ],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("sections/appendix.tex"),
                Utf8PathBuf::from("notes.txt"),
            ],
            None,
        )
        .expect("plan");

        assert!(plan.is_none());
    }

    #[test]
    fn shipout_replay_plan_ignores_earlier_readable_untracked_changed_file_before_later_candidate()
    {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("write appendix");
        fs::write(root.join("notes.txt"), "fresh scratch notes").expect("write notes");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{appendix}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            48,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 28,
                continuation_stack: Vec::new(),
                output_start_utf8: 12,
                page_index_after: 2,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 48,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-48/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 40,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-48/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/tail.tex"),
                        start_utf8: 0,
                        end_utf8: 8,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-48/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: "appendix-old".len() as u32,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![
                StoredModuleTrace {
                    path: Utf8PathBuf::from("sections/tail.tex"),
                    source_start_utf8: 0,
                    source_end_utf8: 8,
                    output_start_utf8: 6,
                    output_end_utf8: 11,
                },
                StoredModuleTrace {
                    path: Utf8PathBuf::from("sections/appendix.tex"),
                    source_start_utf8: 0,
                    source_end_utf8: "appendix-old".len() as u32,
                    output_start_utf8: 12,
                    output_end_utf8: 17,
                },
            ],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("sections/appendix.tex"),
            ],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/appendix.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("appendix enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 2);
        assert_eq!(plan.output_prefix, "page0\npage1\n");
    }

    #[test]
    fn shipout_replay_plan_returns_none_when_earlier_changed_file_cannot_be_read_before_later_candidate()
     {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::create_dir_all(root.join("sections/tail.tex")).expect("tail dir");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("write appendix");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{appendix}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            49,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 28,
                continuation_stack: Vec::new(),
                output_start_utf8: 12,
                page_index_after: 2,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 49,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-49/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 40,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-49/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/tail.tex"),
                        start_utf8: 0,
                        end_utf8: 8,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-49/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: "appendix-old".len() as u32,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![
                StoredModuleTrace {
                    path: Utf8PathBuf::from("sections/tail.tex"),
                    source_start_utf8: 0,
                    source_end_utf8: 8,
                    output_start_utf8: 6,
                    output_end_utf8: 11,
                },
                StoredModuleTrace {
                    path: Utf8PathBuf::from("sections/appendix.tex"),
                    source_start_utf8: 0,
                    source_end_utf8: "appendix-old".len() as u32,
                    output_start_utf8: 12,
                    output_end_utf8: 17,
                },
            ],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("sections/tail.tex"),
                Utf8PathBuf::from("sections/appendix.tex"),
            ],
            None,
        )
        .expect("plan");

        assert!(plan.is_none());
    }

    #[test]
    fn shipout_replay_plan_returns_none_when_later_changed_file_cannot_be_read() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::create_dir_all(root.join("sections/tail.tex")).expect("tail dir");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/appendix.tex"), "appendix-new").expect("write appendix");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let appendix_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{appendix}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            45,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/appendix.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 28,
                continuation_stack: Vec::new(),
                output_start_utf8: 12,
                page_index_after: 2,
                snapshot: appendix_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 45,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-45/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 40,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-45/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/tail.tex"),
                        start_utf8: 0,
                        end_utf8: 8,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-45/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/appendix.tex"),
                        start_utf8: 0,
                        end_utf8: "appendix-old".len() as u32,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/appendix.tex"),
                    "appendix-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![
                StoredModuleTrace {
                    path: Utf8PathBuf::from("sections/tail.tex"),
                    source_start_utf8: 0,
                    source_end_utf8: 8,
                    output_start_utf8: 6,
                    output_end_utf8: 11,
                },
                StoredModuleTrace {
                    path: Utf8PathBuf::from("sections/appendix.tex"),
                    source_start_utf8: 0,
                    source_end_utf8: "appendix-old".len() as u32,
                    output_start_utf8: 12,
                    output_end_utf8: 17,
                },
            ],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("sections/appendix.tex"),
                Utf8PathBuf::from("sections/tail.tex"),
            ],
            None,
        )
        .expect("plan");

        assert!(plan.is_none());
    }

    #[test]
    fn shipout_replay_plan_ignores_identical_source_override() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            15,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{a}")],
            &[10],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 15,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 9,
                pdf_artifact_path: Utf8PathBuf::from("rev-15/pages/p0.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 10,
                }],
            }],
            output: "page0".to_string(),
            sources: BTreeMap::from([(Utf8PathBuf::from("main.tex"), "main body".to_string())]),
            executed_sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "main body".to_string(),
            )]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[],
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "main body".to_string(),
            )])),
        )
        .expect("plan");

        assert!(plan.is_none());
    }

    #[test]
    fn shipout_replay_plan_ignores_identical_override_when_other_file_changed() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/tail.tex"), "tail-new").expect("write tail");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let tail_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{tail}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            40,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/tail.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 14,
                continuation_stack: Vec::new(),
                output_start_utf8: 12,
                page_index_after: 2,
                snapshot: tail_enter_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 40,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-40/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 120,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 11,
                    pdf_artifact_path: Utf8PathBuf::from("rev-40/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 121,
                        end_utf8: 240,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-40/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 241,
                        end_utf8: 360,
                    }],
                },
            ],
            output: "page0\npage1\npage2".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "tail-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("sections/tail.tex"),
                source_start_utf8: 0,
                source_end_utf8: "tail-old".len() as u32,
                output_start_utf8: 12,
                output_end_utf8: 17,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/tail.tex")],
            Some(&BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "main body".to_string(),
            )])),
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/tail.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("tail enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 2);
        assert_eq!(plan.output_prefix, "page0\npage1\n");
    }

    #[test]
    fn shipout_replay_plan_uses_input_boundary_for_toplevel_edit_after_include() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "intro \\input{sections/tail} after-new",
        )
        .expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/tail.tex"), "tail-body").expect("write tail");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let tail_exit_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{tail-exit}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            7,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[checkpoint_page("p0", 0, "hash-0")],
            &[compile_format_snapshot(&mut interner, r"\def\fmt{page0}")],
            &[40],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Exit,
                module_path: Utf8PathBuf::from("sections/tail.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 27,
                continuation_stack: Vec::new(),
                output_start_utf8: 8,
                page_index_after: 0,
                snapshot: tail_exit_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 7,
            bundle,
            page_metadata: vec![PageArtifactMeta {
                page_id: "p0".to_string(),
                index: 0,
                line_count: 1,
                width_pt: 612,
                height_pt: 792,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 18,
                pdf_artifact_path: Utf8PathBuf::from("rev-7/pages/p0.pdf"),
                source_spans: vec![ArtifactSourceSpan {
                    file: Utf8PathBuf::from("main.tex"),
                    start_utf8: 0,
                    end_utf8: 36,
                }],
            }],
            output: "intro tail after-old".to_string(),
            sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "intro \\input{sections/tail} after-old".to_string(),
            )]),
            executed_sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "intro \\input{sections/tail} after-old".to_string(),
            )]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("main.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Exit)
                    && checkpoint.meta.resume_path.as_ref() == Some(&Utf8PathBuf::from("main.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("input boundary checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.checkpoint.resume_path, Utf8PathBuf::from("main.tex"));
        assert_eq!(plan.checkpoint.source_offset_utf8, 27);
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "intro ta");
    }

    #[test]
    fn shipout_replay_plan_prefers_input_boundary_over_equally_early_toplevel_shipout_candidate() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(
            root.join("main.tex"),
            "page0 words \\input{sections/tail} page1 changed text",
        )
        .expect("write main");
        fs::write(root.join("sections/tail.tex"), "tail-body").expect("write tail");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let tail_exit_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{tail-exit}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            39,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{page0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{page1}"),
            ],
            &[10, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Exit,
                module_path: Utf8PathBuf::from("sections/tail.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 32,
                continuation_stack: Vec::new(),
                output_start_utf8: 10,
                page_index_after: 1,
                snapshot: tail_exit_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 39,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-39/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-39/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 32,
                        end_utf8: 64,
                    }],
                },
            ],
            output: "page0-----page1-----".to_string(),
            sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "page0 words \\input{sections/tail} page1 old text".to_string(),
            )]),
            executed_sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "page0 words \\input{sections/tail} page1 old text".to_string(),
            )]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("main.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Exit)
                    && checkpoint.meta.resume_path.as_ref() == Some(&Utf8PathBuf::from("main.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("input boundary checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0-----");
    }

    #[test]
    fn shipout_replay_plan_prefers_toplevel_shipout_candidate_when_prefix_is_longer() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(
            root.join("main.tex"),
            "page0 words \\input{sections/tail} page1 changed text",
        )
        .expect("write main");
        fs::write(root.join("sections/tail.tex"), "tail-body").expect("write tail");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let tail_exit_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{tail-exit}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            43,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{page0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{page1}"),
            ],
            &[10, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Exit,
                module_path: Utf8PathBuf::from("sections/tail.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 32,
                continuation_stack: Vec::new(),
                output_start_utf8: 8,
                page_index_after: 1,
                snapshot: tail_exit_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 43,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-43/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 31,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-43/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 32,
                        end_utf8: 64,
                    }],
                },
            ],
            output: "page0-----page1-----".to_string(),
            sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "page0 words \\input{sections/tail} page1 old text".to_string(),
            )]),
            executed_sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "page0 words \\input{sections/tail} page1 old text".to_string(),
            )]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("main.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::Shipout
                    && checkpoint.meta.page_index_after == 1
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("page-1 shipout checkpoint");
        let input_boundary_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Exit)
                    && checkpoint.meta.resume_path.as_ref() == Some(&Utf8PathBuf::from("main.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("input boundary checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_ne!(plan.checkpoint_id, input_boundary_checkpoint_id);
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0-----");
    }

    #[test]
    fn shipout_replay_plan_prefers_checkpoint_offset_over_earlier_toplevel_span() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "AAAA BBBB changed").expect("write main");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            15,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[4, 12],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 15,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-15/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 20,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-15/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 21,
                        end_utf8: 40,
                    }],
                },
            ],
            output: "page0\npage1".to_string(),
            sources: BTreeMap::from([(Utf8PathBuf::from("main.tex"), "AAAA BBBB old".to_string())]),
            executed_sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "AAAA BBBB old".to_string(),
            )]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("main.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\npage");
    }

    #[test]
    fn shipout_replay_plan_prefers_more_conservative_candidate_across_multiple_changed_files() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "Xain body").expect("write current main");
        fs::write(root.join("pkg.sty"), "prefix suffiX OLD").expect("write current package");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            25,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 25,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-25/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-25/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("pkg.sty"),
                        start_utf8: 0,
                        end_utf8: 18,
                    }],
                },
            ],
            output: "page0\npage1".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg.sty"),
                    "prefix suffix OLD".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("pkg.sty"),
                source_start_utf8: "prefix ".len() as u32,
                source_end_utf8: "prefix suffix".len() as u32,
                output_start_utf8: 10,
                output_end_utf8: 19,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("pkg.sty")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(
            plan.checkpoint_id,
            previous.bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_eq!(plan.start_page_index, 0);
        assert_eq!(plan.output_prefix, "");
    }

    #[test]
    fn shipout_replay_plan_prefers_shorter_prefix_when_changed_files_share_start_page() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg-a.sty"), "prefiX suffix A").expect("write pkg a");
        fs::write(root.join("pkg-b.sty"), "prefix suffiX B").expect("write pkg b");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            26,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 26,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-26/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 24,
                    pdf_artifact_path: Utf8PathBuf::from("rev-26/pages/p1.pdf"),
                    source_spans: vec![
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-a.sty"),
                            start_utf8: 0,
                            end_utf8: 15,
                        },
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-b.sty"),
                            start_utf8: 0,
                            end_utf8: 16,
                        },
                    ],
                },
            ],
            output: "page0\npage1-trailer".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![
                StoredModuleTrace {
                    path: Utf8PathBuf::from("pkg-a.sty"),
                    source_start_utf8: 0,
                    source_end_utf8: 6,
                    output_start_utf8: 10,
                    output_end_utf8: 13,
                },
                StoredModuleTrace {
                    path: Utf8PathBuf::from("pkg-b.sty"),
                    source_start_utf8: 7,
                    source_end_utf8: 13,
                    output_start_utf8: 14,
                    output_end_utf8: 19,
                },
            ],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("pkg-a.sty"),
                Utf8PathBuf::from("pkg-b.sty"),
            ],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\npage");
    }

    #[test]
    fn shipout_replay_plan_prefers_shorter_prefix_over_more_specific_same_page_candidate() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg-a.sty"), "prefiX suffix A").expect("write pkg a");
        fs::write(root.join("pkg-b.sty"), "prefix suffiX B").expect("write pkg b");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            27,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("pkg-b.sty"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 7,
                continuation_stack: Vec::new(),
                output_start_utf8: 14,
                page_index_after: 1,
                snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{pkg-b}"),
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 27,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-27/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 24,
                    pdf_artifact_path: Utf8PathBuf::from("rev-27/pages/p1.pdf"),
                    source_spans: vec![
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-a.sty"),
                            start_utf8: 0,
                            end_utf8: 15,
                        },
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-b.sty"),
                            start_utf8: 0,
                            end_utf8: 16,
                        },
                    ],
                },
            ],
            output: "page0\npage1-trailer".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("pkg-a.sty"),
                source_start_utf8: 0,
                source_end_utf8: 6,
                output_start_utf8: 10,
                output_end_utf8: 13,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("pkg-b.sty"),
                Utf8PathBuf::from("pkg-a.sty"),
            ],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::Shipout
                    && checkpoint.meta.page_index_after == 1
            })
            .expect("page-1 shipout checkpoint");
        let longer_prefix_checkpoint = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("pkg-b.sty"))
            })
            .expect("pkg-b input boundary");
        assert_eq!(plan.checkpoint_id, expected_checkpoint.meta.checkpoint_id);
        assert_ne!(
            plan.checkpoint_id,
            longer_prefix_checkpoint.meta.checkpoint_id
        );
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\npage");
    }

    #[test]
    fn shipout_replay_plan_prefers_more_specific_candidate_when_changed_files_share_page_and_prefix()
     {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg-a.sty"), "prefix suffiX A").expect("write pkg a");
        fs::write(root.join("pkg-b.sty"), "prefix suffiX B").expect("write pkg b");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            28,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("pkg-b.sty"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 0,
                continuation_stack: Vec::new(),
                output_start_utf8: 10,
                page_index_after: 1,
                snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{pkg-b}"),
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 28,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-28/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-28/pages/p1.pdf"),
                    source_spans: vec![
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-a.sty"),
                            start_utf8: 0,
                            end_utf8: 15,
                        },
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-b.sty"),
                            start_utf8: 0,
                            end_utf8: 15,
                        },
                    ],
                },
            ],
            output: "page0\npage1".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("pkg-a.sty"),
                source_start_utf8: 7,
                source_end_utf8: 13,
                output_start_utf8: 10,
                output_end_utf8: 19,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("pkg-a.sty"),
                Utf8PathBuf::from("pkg-b.sty"),
            ],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("pkg-b.sty"))
            })
            .expect("pkg-b input boundary");
        assert_eq!(plan.checkpoint_id, expected_checkpoint.meta.checkpoint_id);
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\npage");
    }

    #[test]
    fn shipout_replay_plan_keeps_more_specific_existing_candidate_when_later_changed_file_is_less_specific()
     {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg-a.sty"), "prefix suffiX A").expect("write pkg a");
        fs::write(root.join("pkg-b.sty"), "prefix suffiX B").expect("write pkg b");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            29,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("pkg-b.sty"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 0,
                continuation_stack: Vec::new(),
                output_start_utf8: 10,
                page_index_after: 1,
                snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{pkg-b}"),
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 29,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-29/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-29/pages/p1.pdf"),
                    source_spans: vec![
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-a.sty"),
                            start_utf8: 0,
                            end_utf8: 15,
                        },
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-b.sty"),
                            start_utf8: 0,
                            end_utf8: 15,
                        },
                    ],
                },
            ],
            output: "page0\npage1".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("pkg-a.sty"),
                source_start_utf8: 7,
                source_end_utf8: 13,
                output_start_utf8: 10,
                output_end_utf8: 19,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("pkg-b.sty"),
                Utf8PathBuf::from("pkg-a.sty"),
            ],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("pkg-b.sty"))
            })
            .expect("pkg-b input boundary");
        assert_eq!(plan.checkpoint_id, expected_checkpoint.meta.checkpoint_id);
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\npage");
    }

    #[test]
    fn shipout_replay_plan_keeps_existing_candidate_when_same_page_prefix_and_specificity_tie() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg-a.sty"), "prefix suffiX A").expect("write pkg a");
        fs::write(root.join("pkg-b.sty"), "prefix suffiX B").expect("write pkg b");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            30,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("pkg-a.sty"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 10,
                    page_index_after: 1,
                    snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{pkg-a}"),
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("pkg-b.sty"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 10,
                    page_index_after: 1,
                    snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{pkg-b}"),
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 30,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-30/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-30/pages/p1.pdf"),
                    source_spans: vec![
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-a.sty"),
                            start_utf8: 0,
                            end_utf8: 15,
                        },
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-b.sty"),
                            start_utf8: 0,
                            end_utf8: 15,
                        },
                    ],
                },
            ],
            output: "page0\npage1".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("pkg-a.sty"),
                Utf8PathBuf::from("pkg-b.sty"),
            ],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("pkg-a.sty"))
            })
            .expect("pkg-a input boundary");
        assert_eq!(plan.checkpoint_id, expected_checkpoint.meta.checkpoint_id);
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\npage");
    }

    #[test]
    fn shipout_replay_plan_keeps_first_changed_file_when_same_page_prefix_and_specificity_tie_is_reversed()
     {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg-a.sty"), "prefix suffiX A").expect("write pkg a");
        fs::write(root.join("pkg-b.sty"), "prefix suffiX B").expect("write pkg b");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            31,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("pkg-a.sty"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 10,
                    page_index_after: 1,
                    snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{pkg-a}"),
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("pkg-b.sty"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 10,
                    page_index_after: 1,
                    snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{pkg-b}"),
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 31,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-31/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 19,
                    pdf_artifact_path: Utf8PathBuf::from("rev-31/pages/p1.pdf"),
                    source_spans: vec![
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-a.sty"),
                            start_utf8: 0,
                            end_utf8: 15,
                        },
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-b.sty"),
                            start_utf8: 0,
                            end_utf8: 15,
                        },
                    ],
                },
            ],
            output: "page0\npage1".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("pkg-b.sty"),
                Utf8PathBuf::from("pkg-a.sty"),
            ],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("pkg-b.sty"))
            })
            .expect("pkg-b input boundary");
        assert_eq!(plan.checkpoint_id, expected_checkpoint.meta.checkpoint_id);
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\npage");
    }

    #[test]
    fn shipout_replay_plan_keeps_shorter_prefix_candidate_when_later_changed_file_is_more_specific()
    {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "main body").expect("write main");
        fs::write(root.join("pkg-a.sty"), "prefiX suffix A").expect("write pkg a");
        fs::write(root.join("pkg-b.sty"), "prefix suffiX B").expect("write pkg b");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            30,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("pkg-b.sty"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 7,
                continuation_stack: Vec::new(),
                output_start_utf8: 14,
                page_index_after: 1,
                snapshot: compile_format_snapshot(&mut interner, r"\def\fmt{pkg-b}"),
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 30,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 9,
                    pdf_artifact_path: Utf8PathBuf::from("rev-30/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 10,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 24,
                    pdf_artifact_path: Utf8PathBuf::from("rev-30/pages/p1.pdf"),
                    source_spans: vec![
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-a.sty"),
                            start_utf8: 0,
                            end_utf8: 15,
                        },
                        ArtifactSourceSpan {
                            file: Utf8PathBuf::from("pkg-b.sty"),
                            start_utf8: 0,
                            end_utf8: 16,
                        },
                    ],
                },
            ],
            output: "page0\npage1-trailer".to_string(),
            sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (Utf8PathBuf::from("main.tex"), "main body".to_string()),
                (
                    Utf8PathBuf::from("pkg-a.sty"),
                    "prefix suffix A".to_string(),
                ),
                (
                    Utf8PathBuf::from("pkg-b.sty"),
                    "prefix suffix B".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("pkg-a.sty"),
                source_start_utf8: 0,
                source_end_utf8: 6,
                output_start_utf8: 10,
                output_end_utf8: 13,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[
                Utf8PathBuf::from("pkg-a.sty"),
                Utf8PathBuf::from("pkg-b.sty"),
            ],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::Shipout
                    && checkpoint.meta.page_index_after == 1
            })
            .expect("page-1 shipout checkpoint");
        let later_more_specific_checkpoint = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("pkg-b.sty"))
            })
            .expect("pkg-b input boundary");
        assert_eq!(plan.checkpoint_id, expected_checkpoint.meta.checkpoint_id);
        assert_ne!(
            plan.checkpoint_id,
            later_more_specific_checkpoint.meta.checkpoint_id
        );
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\npage");
    }

    #[test]
    fn shipout_replay_plan_prefers_earliest_occurrence_for_repeated_include_file_edits() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(
            root.join("sections/tail.tex"),
            "before \\input{sections/child} after-new",
        )
        .expect("write tail");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let first_exit_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{first}");
        let second_exit_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{second}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            12,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
                checkpoint_page("p3", 3, "hash-3"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{d}"),
            ],
            &[10, 20, 30, 40],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Exit,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/tail.tex")),
                    source_offset_utf8: 29,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 18,
                    }],
                    output_start_utf8: 12,
                    page_index_after: 1,
                    snapshot: first_exit_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Exit,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/tail.tex")),
                    source_offset_utf8: 29,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 54,
                    }],
                    output_start_utf8: 28,
                    page_index_after: 3,
                    snapshot: second_exit_snapshot,
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 12,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 8,
                    pdf_artifact_path: Utf8PathBuf::from("rev-12/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 20,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 8,
                    text_end_utf8: 16,
                    pdf_artifact_path: Utf8PathBuf::from("rev-12/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/tail.tex"),
                        start_utf8: 0,
                        end_utf8: 40,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 16,
                    text_end_utf8: 24,
                    pdf_artifact_path: Utf8PathBuf::from("rev-12/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 20,
                        end_utf8: 60,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p3".to_string(),
                    index: 3,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-3".to_string(),
                    text_start_utf8: 24,
                    text_end_utf8: 32,
                    pdf_artifact_path: Utf8PathBuf::from("rev-12/pages/p3.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/tail.tex"),
                        start_utf8: 0,
                        end_utf8: 40,
                    }],
                },
            ],
            output: "page0\nfirst-tail\npage2\nsecond-tail".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "head \\input{sections/tail} mid \\input{sections/tail} tail".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "before \\input{sections/child} after-old".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "head \\input{sections/tail} mid \\input{sections/tail} tail".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/tail.tex"),
                    "before \\input{sections/child} after-old".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/tail.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.resume_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/tail.tex"))
                    && checkpoint.meta.output_start_utf8 == 12
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("first occurrence checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(
            plan.checkpoint.continuation_stack,
            vec![VmReplayFrame {
                path: Utf8PathBuf::from("main.tex"),
                source_offset_utf8: 18,
            }]
        );
    }

    #[test]
    fn shipout_replay_plan_clamps_toplevel_input_boundary_to_last_page() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(
            root.join("main.tex"),
            "intro \\input{sections/tail} epilogue-new",
        )
        .expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/tail.tex"), "tail-body").expect("write tail");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let tail_exit_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{tail-exit}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            10,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{page0}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{page1}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Exit,
                module_path: Utf8PathBuf::from("sections/tail.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 27,
                continuation_stack: Vec::new(),
                output_start_utf8: 19,
                page_index_after: 1,
                snapshot: tail_exit_snapshot,
            }],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 10,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 8,
                    pdf_artifact_path: Utf8PathBuf::from("rev-10/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 24,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 8,
                    text_end_utf8: 17,
                    pdf_artifact_path: Utf8PathBuf::from("rev-10/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 24,
                        end_utf8: 42,
                    }],
                },
            ],
            output: "intro tail epilogue".to_string(),
            sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "intro \\input{sections/tail} epilogue-old".to_string(),
            )]),
            executed_sources: BTreeMap::from([(
                Utf8PathBuf::from("main.tex"),
                "intro \\input{sections/tail} epilogue-old".to_string(),
            )]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("main.tex")],
            None,
        )
        .expect("plan")
        .expect("shipout replay plan");

        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "intro tail epilogue");
    }

    #[test]
    fn shipout_replay_plan_prefers_nested_exit_checkpoint_inside_changed_file() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\input{sections/parent}").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(
            root.join("sections/parent.tex"),
            "before \\input{sections/child} after-new",
        )
        .expect("write parent");
        fs::write(root.join("sections/child.tex"), "nested").expect("write child");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let nested_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{enter}");
        let nested_exit_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{exit}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            9,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 7,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 8,
                    page_index_after: 1,
                    snapshot: nested_enter_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Exit,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 29,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 15,
                    page_index_after: 2,
                    snapshot: nested_exit_snapshot,
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 9,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-9/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 24,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 14,
                    pdf_artifact_path: Utf8PathBuf::from("rev-9/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 0,
                        end_utf8: 29,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 15,
                    text_end_utf8: 24,
                    pdf_artifact_path: Utf8PathBuf::from("rev-9/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 29,
                        end_utf8: 40,
                    }],
                },
            ],
            output: "page0\nnested\npage2".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/parent}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/parent.tex"),
                    "before \\input{sections/child} after-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/child.tex"),
                    "nested".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/parent}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/parent.tex"),
                    "before \\input{sections/child} after-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/child.tex"),
                    "nested".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("sections/child.tex"),
                source_start_utf8: 0,
                source_end_utf8: "nested".len() as u32,
                output_start_utf8: 8,
                output_end_utf8: 14,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/parent.tex")],
            None,
        )
        .expect("plan")
        .expect("nested replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Exit)
                    && checkpoint.meta.resume_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/parent.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("nested exit checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(
            plan.checkpoint.resume_path,
            Utf8PathBuf::from("sections/parent.tex")
        );
        assert_eq!(
            plan.checkpoint.continuation_stack,
            vec![VmReplayFrame {
                path: Utf8PathBuf::from("main.tex"),
                source_offset_utf8: 22,
            }]
        );
        assert_eq!(plan.checkpoint.source_offset_utf8, 29);
        assert_eq!(plan.start_page_index, 2);
        assert_eq!(plan.output_prefix, "page0\nnested\npa");
    }

    #[test]
    fn shipout_replay_plan_keeps_later_output_boundary_for_same_continuation_stack() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\input{sections/parent}").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(
            root.join("sections/parent.tex"),
            "before \\input{sections/child} after-old trailer-new",
        )
        .expect("write parent");
        fs::write(root.join("sections/child.tex"), "nested").expect("write child");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let nested_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{enter}");
        let early_exit_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{early}");
        let later_exit_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{later}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            33,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
                checkpoint_page("p3", 3, "hash-3"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{d}"),
            ],
            &[10, 20, 30, 40],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 7,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 8,
                    page_index_after: 1,
                    snapshot: nested_enter_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Exit,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 29,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 15,
                    page_index_after: 2,
                    snapshot: early_exit_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Exit,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 29,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 19,
                    page_index_after: 3,
                    snapshot: later_exit_snapshot,
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 33,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-33/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 24,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 14,
                    pdf_artifact_path: Utf8PathBuf::from("rev-33/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 0,
                        end_utf8: 29,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 15,
                    text_end_utf8: 18,
                    pdf_artifact_path: Utf8PathBuf::from("rev-33/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 29,
                        end_utf8: 41,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p3".to_string(),
                    index: 3,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-3".to_string(),
                    text_start_utf8: 19,
                    text_end_utf8: 24,
                    pdf_artifact_path: Utf8PathBuf::from("rev-33/pages/p3.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 41,
                        end_utf8: 56,
                    }],
                },
            ],
            output: "page0\nnested\npage2\npage3".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/parent}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/parent.tex"),
                    "before \\input{sections/child} after-old trailer-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/child.tex"),
                    "nested".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/parent}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/parent.tex"),
                    "before \\input{sections/child} after-old trailer-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/child.tex"),
                    "nested".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/parent.tex")],
            None,
        )
        .expect("plan")
        .expect("nested replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.resume_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/parent.tex"))
                    && checkpoint.meta.output_start_utf8 == 19
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("later nested exit checkpoint");
        let earlier_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.resume_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/parent.tex"))
                    && checkpoint.meta.output_start_utf8 == 15
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("earlier nested exit checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_ne!(plan.checkpoint_id, earlier_checkpoint_id);
        assert_eq!(
            plan.checkpoint.resume_path,
            Utf8PathBuf::from("sections/parent.tex")
        );
        assert_eq!(
            plan.checkpoint.continuation_stack,
            vec![VmReplayFrame {
                path: Utf8PathBuf::from("main.tex"),
                source_offset_utf8: 22,
            }]
        );
        assert_eq!(plan.checkpoint.source_offset_utf8, 29);
        assert_eq!(plan.start_page_index, 3);
        assert_eq!(plan.output_prefix, "page0\nnested\npage2\n");
    }

    #[test]
    fn shipout_replay_plan_prefers_later_source_boundary_for_same_continuation_stack() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\input{sections/parent}").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(
            root.join("sections/parent.tex"),
            "before \\input{sections/child} after-olX trailer-new",
        )
        .expect("write parent");
        fs::write(root.join("sections/child.tex"), "nested").expect("write child");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let nested_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{enter}");
        let earlier_exit_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{earlier}");
        let later_source_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{later}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            41,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
                checkpoint_page("p3", 3, "hash-3"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{d}"),
            ],
            &[10, 20, 30, 40],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 7,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 8,
                    page_index_after: 1,
                    snapshot: nested_enter_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Exit,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 23,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 19,
                    page_index_after: 3,
                    snapshot: earlier_exit_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Exit,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 29,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 15,
                    page_index_after: 2,
                    snapshot: later_source_snapshot,
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 41,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-41/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 24,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 14,
                    pdf_artifact_path: Utf8PathBuf::from("rev-41/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 0,
                        end_utf8: 29,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 15,
                    text_end_utf8: 18,
                    pdf_artifact_path: Utf8PathBuf::from("rev-41/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 29,
                        end_utf8: 41,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p3".to_string(),
                    index: 3,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-3".to_string(),
                    text_start_utf8: 19,
                    text_end_utf8: 24,
                    pdf_artifact_path: Utf8PathBuf::from("rev-41/pages/p3.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 41,
                        end_utf8: 56,
                    }],
                },
            ],
            output: "page0\nnested\npage2\npage3".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/parent}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/parent.tex"),
                    "before \\input{sections/child} after-old trailer-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/child.tex"),
                    "nested".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/parent}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/parent.tex"),
                    "before \\input{sections/child} after-old trailer-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/child.tex"),
                    "nested".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/parent.tex")],
            None,
        )
        .expect("plan")
        .expect("nested replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.resume_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/parent.tex"))
                    && checkpoint.meta.source_offset_utf8 == 29
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("later source checkpoint");
        let later_output_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.resume_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/parent.tex"))
                    && checkpoint.meta.output_start_utf8 == 19
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("later output checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_ne!(plan.checkpoint_id, later_output_checkpoint_id);
        assert_eq!(
            plan.checkpoint.resume_path,
            Utf8PathBuf::from("sections/parent.tex")
        );
        assert_eq!(
            plan.checkpoint.continuation_stack,
            vec![VmReplayFrame {
                path: Utf8PathBuf::from("main.tex"),
                source_offset_utf8: 22,
            }]
        );
        assert_eq!(plan.checkpoint.source_offset_utf8, 29);
        assert_eq!(plan.start_page_index, 2);
        assert_eq!(plan.output_prefix, "page0\nnested\npa");
    }

    #[test]
    fn shipout_replay_plan_prefers_earlier_output_boundary_across_continuation_stacks() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\input{sections/parent}").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(
            root.join("sections/parent.tex"),
            "before \\input{sections/child} after-olX trailer-new",
        )
        .expect("write parent");
        fs::write(root.join("sections/child.tex"), "nested").expect("write child");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let nested_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{enter}");
        let later_output_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{later}");
        let earlier_output_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{earlier}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            42,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
                checkpoint_page("p3", 3, "hash-3"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{d}"),
            ],
            &[10, 20, 30, 40],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 7,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 8,
                    page_index_after: 1,
                    snapshot: nested_enter_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Exit,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 29,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 19,
                    page_index_after: 3,
                    snapshot: later_output_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Exit,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 17,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 30,
                    }],
                    output_start_utf8: 15,
                    page_index_after: 2,
                    snapshot: earlier_output_snapshot,
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 42,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-42/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 24,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 14,
                    pdf_artifact_path: Utf8PathBuf::from("rev-42/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 0,
                        end_utf8: 29,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 15,
                    text_end_utf8: 18,
                    pdf_artifact_path: Utf8PathBuf::from("rev-42/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 29,
                        end_utf8: 41,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p3".to_string(),
                    index: 3,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-3".to_string(),
                    text_start_utf8: 19,
                    text_end_utf8: 24,
                    pdf_artifact_path: Utf8PathBuf::from("rev-42/pages/p3.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 41,
                        end_utf8: 56,
                    }],
                },
            ],
            output: "page0\nnested\npage2\npage3".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/parent}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/parent.tex"),
                    "before \\input{sections/child} after-old trailer-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/child.tex"),
                    "nested".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/parent}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/parent.tex"),
                    "before \\input{sections/child} after-old trailer-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/child.tex"),
                    "nested".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/parent.tex")],
            None,
        )
        .expect("plan")
        .expect("nested replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.resume_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/parent.tex"))
                    && checkpoint.meta.output_start_utf8 == 15
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("earlier output checkpoint");
        let later_output_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.resume_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/parent.tex"))
                    && checkpoint.meta.output_start_utf8 == 19
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("later output checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_ne!(plan.checkpoint_id, later_output_checkpoint_id);
        assert_eq!(
            plan.checkpoint.resume_path,
            Utf8PathBuf::from("sections/parent.tex")
        );
        assert_eq!(
            plan.checkpoint.continuation_stack,
            vec![VmReplayFrame {
                path: Utf8PathBuf::from("main.tex"),
                source_offset_utf8: 30,
            }]
        );
        assert_eq!(plan.checkpoint.source_offset_utf8, 17);
        assert_eq!(plan.start_page_index, 2);
        assert_eq!(plan.output_prefix, "page0\nnested\npa");
    }

    #[test]
    fn shipout_replay_plan_prefers_shipout_checkpoint_before_nested_child_output() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\input{sections/parent}").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(
            root.join("sections/parent.tex"),
            "befoXe \\input{sections/child} after-old",
        )
        .expect("write parent");
        fs::write(root.join("sections/child.tex"), "nested").expect("write child");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let nested_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{enter}");
        let nested_exit_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{exit}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            31,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 7,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 8,
                    page_index_after: 1,
                    snapshot: nested_enter_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Exit,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 29,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 15,
                    page_index_after: 2,
                    snapshot: nested_exit_snapshot,
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 31,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-31/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 24,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 14,
                    pdf_artifact_path: Utf8PathBuf::from("rev-31/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 0,
                        end_utf8: 29,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 15,
                    text_end_utf8: 24,
                    pdf_artifact_path: Utf8PathBuf::from("rev-31/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 29,
                        end_utf8: 40,
                    }],
                },
            ],
            output: "page0\nnested\npage2".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/parent}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/parent.tex"),
                    "before \\input{sections/child} after-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/child.tex"),
                    "nested".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/parent}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/parent.tex"),
                    "before \\input{sections/child} after-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/child.tex"),
                    "nested".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("sections/child.tex"),
                source_start_utf8: 0,
                source_end_utf8: "nested".len() as u32,
                output_start_utf8: 8,
                output_end_utf8: 14,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/parent.tex")],
            None,
        )
        .expect("plan")
        .expect("nested replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::Shipout
                    && checkpoint.meta.page_index_after == 1
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("page-1 shipout checkpoint");
        let nested_enter_checkpoint = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.resume_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/parent.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("nested enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_ne!(plan.checkpoint_id, nested_enter_checkpoint);
        assert_eq!(plan.checkpoint.resume_path, Utf8PathBuf::from("main.tex"));
        assert_eq!(
            plan.checkpoint.continuation_stack,
            Vec::<VmReplayFrame>::new()
        );
        assert_eq!(plan.checkpoint.source_offset_utf8, 10);
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\n");
    }

    #[test]
    fn shipout_replay_plan_prefers_nested_enter_checkpoint_for_changed_include_token() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 root");
        fs::write(root.join("main.tex"), "\\input{sections/parent}").expect("write main");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(
            root.join("sections/parent.tex"),
            "before \\input{sectionS/child} after-old",
        )
        .expect("write parent");
        fs::write(root.join("sections/child.tex"), "nested").expect("write child");

        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let nested_enter_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{enter}");
        let nested_exit_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{exit}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            32,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                checkpoint_page("p0", 0, "hash-0"),
                checkpoint_page("p1", 1, "hash-1"),
                checkpoint_page("p2", 2, "hash-2"),
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\fmt{a}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{b}"),
                compile_format_snapshot(&mut interner, r"\def\fmt{c}"),
            ],
            &[10, 20, 30],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 7,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 8,
                    page_index_after: 1,
                    snapshot: nested_enter_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Exit,
                    module_path: Utf8PathBuf::from("sections/child.tex"),
                    resume_path: Some(Utf8PathBuf::from("sections/parent.tex")),
                    source_offset_utf8: 29,
                    continuation_stack: vec![VmReplayFrame {
                        path: Utf8PathBuf::from("main.tex"),
                        source_offset_utf8: 22,
                    }],
                    output_start_utf8: 15,
                    page_index_after: 2,
                    snapshot: nested_exit_snapshot,
                },
            ],
        )
        .expect("bundle");
        let previous = PreviousInternalBuild {
            rev: 32,
            bundle,
            page_metadata: vec![
                PageArtifactMeta {
                    page_id: "p0".to_string(),
                    index: 0,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 5,
                    pdf_artifact_path: Utf8PathBuf::from("rev-32/pages/p0.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("main.tex"),
                        start_utf8: 0,
                        end_utf8: 24,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p1".to_string(),
                    index: 1,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-1".to_string(),
                    text_start_utf8: 6,
                    text_end_utf8: 14,
                    pdf_artifact_path: Utf8PathBuf::from("rev-32/pages/p1.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 0,
                        end_utf8: 29,
                    }],
                },
                PageArtifactMeta {
                    page_id: "p2".to_string(),
                    index: 2,
                    line_count: 1,
                    width_pt: 612,
                    height_pt: 792,
                    content_hash: "hash-2".to_string(),
                    text_start_utf8: 15,
                    text_end_utf8: 24,
                    pdf_artifact_path: Utf8PathBuf::from("rev-32/pages/p2.pdf"),
                    source_spans: vec![ArtifactSourceSpan {
                        file: Utf8PathBuf::from("sections/parent.tex"),
                        start_utf8: 29,
                        end_utf8: 40,
                    }],
                },
            ],
            output: "page0\nnested\npage2".to_string(),
            sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/parent}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/parent.tex"),
                    "before \\input{sections/child} after-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/child.tex"),
                    "nested".to_string(),
                ),
            ]),
            executed_sources: BTreeMap::from([
                (
                    Utf8PathBuf::from("main.tex"),
                    "\\input{sections/parent}".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/parent.tex"),
                    "before \\input{sections/child} after-old".to_string(),
                ),
                (
                    Utf8PathBuf::from("sections/child.tex"),
                    "nested".to_string(),
                ),
            ]),
            rewrite_spans: BTreeMap::new(),
            module_traces: vec![StoredModuleTrace {
                path: Utf8PathBuf::from("sections/child.tex"),
                source_start_utf8: 0,
                source_end_utf8: "nested".len() as u32,
                output_start_utf8: 8,
                output_end_utf8: 14,
            }],
            module_checkpoints: vec![],
            semantic_aux: None,
            semantic_aux_payload: None,
            semantic_aux_concrete_payload: None,
        };

        let plan = select_shipout_replay_plan(
            &previous,
            &root,
            Utf8Path::new("main.tex"),
            &[Utf8PathBuf::from("sections/parent.tex")],
            None,
        )
        .expect("plan")
        .expect("nested replay plan");

        let expected_checkpoint_id = previous
            .bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                    && checkpoint.meta.resume_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/parent.tex"))
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .expect("nested enter checkpoint");
        assert_eq!(plan.checkpoint_id, expected_checkpoint_id);
        assert_eq!(
            plan.checkpoint.resume_path,
            Utf8PathBuf::from("sections/parent.tex")
        );
        assert_eq!(
            plan.checkpoint.continuation_stack,
            vec![VmReplayFrame {
                path: Utf8PathBuf::from("main.tex"),
                source_offset_utf8: 22,
            }]
        );
        assert_eq!(plan.checkpoint.source_offset_utf8, 7);
        assert_eq!(plan.start_page_index, 1);
        assert_eq!(plan.output_prefix, "page0\nne");
    }

    #[test]
    fn plans_insert_before_unchanged_tail() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
            checkpoint_page("old-3", 3, "hash-3"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "inserted"),
            checkpoint_page("new-2", 2, "hash-1"),
            checkpoint_page("new-3", 3, "hash-2"),
            checkpoint_page("new-4", 4, "hash-3"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/2/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/2/pages/new-1.pdf"),
            artifact("new-2", "/artifacts/rev/1/pages/new-2.pdf"),
            artifact("new-3", "/artifacts/rev/1/pages/new-3.pdf"),
            artifact("new-4", "/artifacts/rev/1/pages/new-4.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 1,
                current_page_start: 2,
                page_count: 3,
            }),
        );

        assert_eq!(
            ops,
            vec![PagePatchOp::InsertPage {
                index: 1,
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: Some("/artifacts/rev/2/pages/new-1.svg".to_string()),
            }]
        );
    }

    #[test]
    fn plans_insert_before_unchanged_tail_preserves_missing_svg_url() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "inserted"),
            checkpoint_page("new-2", 2, "hash-1"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            PagePreviewArtifact {
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: None,
            },
            artifact("new-2", "/artifacts/rev/1/pages/new-2.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 1,
                current_page_start: 2,
                page_count: 1,
            }),
        );

        assert_eq!(
            ops,
            vec![PagePatchOp::InsertPage {
                index: 1,
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: None,
            }]
        );
    }

    #[test]
    fn plans_no_ops_before_unchanged_tail_when_prefixes_are_identical() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
            checkpoint_page("old-3", 3, "hash-3"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "hash-1"),
            checkpoint_page("new-2", 2, "hash-2"),
            checkpoint_page("new-3", 3, "hash-3"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/1/pages/new-1.pdf"),
            artifact("new-2", "/artifacts/rev/1/pages/new-2.pdf"),
            artifact("new-3", "/artifacts/rev/1/pages/new-3.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 2,
                current_page_start: 2,
                page_count: 2,
            }),
        );

        assert!(ops.is_empty());
    }

    #[test]
    fn plans_replace_before_unchanged_tail() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
            checkpoint_page("old-3", 3, "hash-3"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "changed"),
            checkpoint_page("new-2", 2, "hash-2"),
            checkpoint_page("new-3", 3, "hash-3"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/2/pages/new-1.pdf"),
            artifact("new-2", "/artifacts/rev/1/pages/new-2.pdf"),
            artifact("new-3", "/artifacts/rev/1/pages/new-3.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 2,
                current_page_start: 2,
                page_count: 2,
            }),
        );

        assert_eq!(
            ops,
            vec![PagePatchOp::ReplacePage {
                index: 1,
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: Some("/artifacts/rev/2/pages/new-1.svg".to_string()),
            }]
        );
    }

    #[test]
    fn plans_replace_before_unchanged_tail_preserves_missing_svg_url() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "changed"),
            checkpoint_page("new-2", 2, "hash-2"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            PagePreviewArtifact {
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: None,
            },
            artifact("new-2", "/artifacts/rev/1/pages/new-2.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 2,
                current_page_start: 2,
                page_count: 1,
            }),
        );

        assert_eq!(
            ops,
            vec![PagePatchOp::ReplacePage {
                index: 1,
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: None,
            }]
        );
    }

    #[test]
    fn plans_replace_all_before_unchanged_tail() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
            checkpoint_page("old-3", 3, "hash-3"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "changed-0"),
            checkpoint_page("new-1", 1, "changed-1"),
            checkpoint_page("new-2", 2, "hash-2"),
            checkpoint_page("new-3", 3, "hash-3"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/2/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/2/pages/new-1.pdf"),
            artifact("new-2", "/artifacts/rev/1/pages/new-2.pdf"),
            artifact("new-3", "/artifacts/rev/1/pages/new-3.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 2,
                current_page_start: 2,
                page_count: 2,
            }),
        );

        assert_eq!(
            ops,
            vec![
                PagePatchOp::ReplacePage {
                    index: 0,
                    page_id: "new-0".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-0.svg".to_string()),
                },
                PagePatchOp::ReplacePage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-1.svg".to_string()),
                },
            ]
        );
    }

    #[test]
    fn plans_replace_all_before_unchanged_tail_preserve_missing_svg_urls() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
            checkpoint_page("old-3", 3, "hash-3"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "changed-0"),
            checkpoint_page("new-1", 1, "changed-1"),
            checkpoint_page("new-2", 2, "hash-2"),
            checkpoint_page("new-3", 3, "hash-3"),
        ];
        let artifacts = vec![
            PagePreviewArtifact {
                page_id: "new-0".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                svg_url: None,
            },
            PagePreviewArtifact {
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: None,
            },
            artifact("new-2", "/artifacts/rev/1/pages/new-2.pdf"),
            artifact("new-3", "/artifacts/rev/1/pages/new-3.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 2,
                current_page_start: 2,
                page_count: 2,
            }),
        );

        assert_eq!(
            ops,
            vec![
                PagePatchOp::ReplacePage {
                    index: 0,
                    page_id: "new-0".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                    svg_url: None,
                },
                PagePatchOp::ReplacePage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: None,
                },
            ]
        );
    }

    #[test]
    fn plans_delete_before_unchanged_tail() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
            checkpoint_page("old-3", 3, "hash-3"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "hash-2"),
            checkpoint_page("new-2", 2, "hash-3"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/1/pages/new-1.pdf"),
            artifact("new-2", "/artifacts/rev/1/pages/new-2.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 2,
                current_page_start: 1,
                page_count: 2,
            }),
        );

        assert_eq!(ops, vec![PagePatchOp::DeletePage { index: 1 }]);
    }

    #[test]
    fn plans_replace_and_delete_before_unchanged_tail() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
            checkpoint_page("old-3", 3, "hash-3"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "changed"),
            checkpoint_page("new-2", 2, "hash-3"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/2/pages/new-1.pdf"),
            artifact("new-2", "/artifacts/rev/1/pages/new-2.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 3,
                current_page_start: 2,
                page_count: 1,
            }),
        );

        assert_eq!(
            ops,
            vec![
                PagePatchOp::ReplacePage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-1.svg".to_string()),
                },
                PagePatchOp::DeletePage { index: 2 },
            ]
        );
    }

    #[test]
    fn plans_replace_and_delete_before_unchanged_tail_preserve_missing_svg_url() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
            checkpoint_page("old-3", 3, "hash-3"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "changed"),
            checkpoint_page("new-2", 2, "hash-3"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            PagePreviewArtifact {
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: None,
            },
            artifact("new-2", "/artifacts/rev/1/pages/new-2.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 3,
                current_page_start: 2,
                page_count: 1,
            }),
        );

        assert_eq!(
            ops,
            vec![
                PagePatchOp::ReplacePage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: None,
                },
                PagePatchOp::DeletePage { index: 2 },
            ]
        );
    }

    #[test]
    fn plans_replace_and_insert_before_unchanged_tail() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
            checkpoint_page("old-3", 3, "hash-3"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "changed"),
            checkpoint_page("new-2", 2, "inserted"),
            checkpoint_page("new-3", 3, "hash-2"),
            checkpoint_page("new-4", 4, "hash-3"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/2/pages/new-1.pdf"),
            artifact("new-2", "/artifacts/rev/2/pages/new-2.pdf"),
            artifact("new-3", "/artifacts/rev/1/pages/new-3.pdf"),
            artifact("new-4", "/artifacts/rev/1/pages/new-4.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 2,
                current_page_start: 3,
                page_count: 2,
            }),
        );

        assert_eq!(
            ops,
            vec![
                PagePatchOp::ReplacePage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-1.svg".to_string()),
                },
                PagePatchOp::InsertPage {
                    index: 2,
                    page_id: "new-2".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-2.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-2.svg".to_string()),
                },
            ]
        );
    }

    #[test]
    fn plans_replace_and_insert_before_unchanged_tail_preserve_missing_svg_urls() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
            checkpoint_page("old-3", 3, "hash-3"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "changed"),
            checkpoint_page("new-2", 2, "inserted"),
            checkpoint_page("new-3", 3, "hash-2"),
            checkpoint_page("new-4", 4, "hash-3"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            PagePreviewArtifact {
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: None,
            },
            PagePreviewArtifact {
                page_id: "new-2".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-2.pdf".to_string(),
                svg_url: None,
            },
            artifact("new-3", "/artifacts/rev/1/pages/new-3.pdf"),
            artifact("new-4", "/artifacts/rev/1/pages/new-4.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 2,
                current_page_start: 3,
                page_count: 2,
            }),
        );

        assert_eq!(
            ops,
            vec![
                PagePatchOp::ReplacePage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: None,
                },
                PagePatchOp::InsertPage {
                    index: 2,
                    page_id: "new-2".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-2.pdf".to_string(),
                    svg_url: None,
                },
            ]
        );
    }

    #[test]
    fn plans_delete_prefix_before_unchanged_tail_with_zero_overlap() {
        let previous = vec![
            checkpoint_page("old-0", 0, "delete-0"),
            checkpoint_page("old-1", 1, "delete-1"),
            checkpoint_page("old-2", 2, "tail-0"),
            checkpoint_page("old-3", 3, "tail-1"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "tail-0"),
            checkpoint_page("new-1", 1, "tail-1"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/1/pages/new-1.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 2,
                current_page_start: 0,
                page_count: 2,
            }),
        );

        assert_eq!(
            ops,
            vec![
                PagePatchOp::DeletePage { index: 1 },
                PagePatchOp::DeletePage { index: 0 }
            ]
        );
    }

    #[test]
    fn plans_insert_prefix_before_unchanged_tail_with_zero_overlap() {
        let previous = vec![
            checkpoint_page("old-0", 0, "tail-0"),
            checkpoint_page("old-1", 1, "tail-1"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "insert-0"),
            checkpoint_page("new-1", 1, "insert-1"),
            checkpoint_page("new-2", 2, "tail-0"),
            checkpoint_page("new-3", 3, "tail-1"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/2/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/2/pages/new-1.pdf"),
            artifact("new-2", "/artifacts/rev/1/pages/new-2.pdf"),
            artifact("new-3", "/artifacts/rev/1/pages/new-3.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 0,
                current_page_start: 2,
                page_count: 2,
            }),
        );

        assert_eq!(
            ops,
            vec![
                PagePatchOp::InsertPage {
                    index: 0,
                    page_id: "new-0".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-0.svg".to_string()),
                },
                PagePatchOp::InsertPage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-1.svg".to_string()),
                },
            ]
        );
    }

    #[test]
    fn plans_insert_prefix_before_unchanged_tail_with_zero_overlap_preserves_missing_svg_url() {
        let previous = vec![
            checkpoint_page("old-0", 0, "tail-0"),
            checkpoint_page("old-1", 1, "tail-1"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "insert-0"),
            checkpoint_page("new-1", 1, "insert-1"),
            checkpoint_page("new-2", 2, "tail-0"),
            checkpoint_page("new-3", 3, "tail-1"),
        ];
        let artifacts = vec![
            PagePreviewArtifact {
                page_id: "new-0".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                svg_url: None,
            },
            PagePreviewArtifact {
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: None,
            },
            artifact("new-2", "/artifacts/rev/1/pages/new-2.pdf"),
            artifact("new-3", "/artifacts/rev/1/pages/new-3.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 0,
                current_page_start: 2,
                page_count: 2,
            }),
        );

        assert_eq!(
            ops,
            vec![
                PagePatchOp::InsertPage {
                    index: 0,
                    page_id: "new-0".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                    svg_url: None,
                },
                PagePatchOp::InsertPage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: None,
                },
            ]
        );
    }

    #[test]
    fn plans_emit_no_ops_when_unchanged_tail_covers_all_pages() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "hash-1"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/2/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/2/pages/new-1.pdf"),
        ];

        let ops = plan_page_patches(
            &previous,
            &current,
            &artifacts,
            Some(&UnchangedTail {
                previous_rev: 1,
                resume_checkpoint_id: "cp1".to_string(),
                previous_page_start: 0,
                current_page_start: 0,
                page_count: 2,
            }),
        );

        assert!(ops.is_empty());
    }

    #[test]
    fn plans_insert_without_tail_alignment() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "hash-1"),
            checkpoint_page("new-2", 2, "hash-2"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/1/pages/new-1.pdf"),
            artifact("new-2", "/artifacts/rev/2/pages/new-2.pdf"),
        ];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(
            ops,
            vec![PagePatchOp::InsertPage {
                index: 2,
                page_id: "new-2".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-2.pdf".to_string(),
                svg_url: Some("/artifacts/rev/2/pages/new-2.svg".to_string()),
            }]
        );
    }

    #[test]
    fn plans_insert_without_tail_alignment_preserves_missing_svg_url() {
        let previous = vec![checkpoint_page("old-0", 0, "hash-0")];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "hash-1"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            PagePreviewArtifact {
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: None,
            },
        ];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(
            ops,
            vec![PagePatchOp::InsertPage {
                index: 1,
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: None,
            }]
        );
    }

    #[test]
    fn plans_no_ops_without_tail_alignment_when_pages_are_identical() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "hash-1"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/1/pages/new-1.pdf"),
        ];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert!(ops.is_empty());
    }

    #[test]
    fn plans_no_ops_when_both_documents_are_empty() {
        let previous = vec![];
        let current = vec![];
        let artifacts = vec![];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert!(ops.is_empty());
    }

    #[test]
    fn plans_insert_all_pages_when_previous_document_is_empty() {
        let previous = vec![];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "hash-1"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/2/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/2/pages/new-1.pdf"),
        ];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(
            ops,
            vec![
                PagePatchOp::InsertPage {
                    index: 0,
                    page_id: "new-0".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-0.svg".to_string()),
                },
                PagePatchOp::InsertPage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-1.svg".to_string()),
                },
            ]
        );
    }

    #[test]
    fn plans_insert_all_pages_when_previous_document_is_empty_preserve_missing_svg_urls() {
        let previous = vec![];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "hash-1"),
        ];
        let artifacts = vec![
            PagePreviewArtifact {
                page_id: "new-0".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                svg_url: None,
            },
            PagePreviewArtifact {
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: None,
            },
        ];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(
            ops,
            vec![
                PagePatchOp::InsertPage {
                    index: 0,
                    page_id: "new-0".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                    svg_url: None,
                },
                PagePatchOp::InsertPage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: None,
                },
            ]
        );
    }

    #[test]
    fn plans_delete_all_pages_when_current_document_is_empty() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
        ];
        let current = vec![];
        let artifacts = vec![];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(
            ops,
            vec![
                PagePatchOp::DeletePage { index: 1 },
                PagePatchOp::DeletePage { index: 0 }
            ]
        );
    }

    #[test]
    fn plans_delete_without_tail_alignment() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "hash-1"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/1/pages/new-1.pdf"),
        ];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(ops, vec![PagePatchOp::DeletePage { index: 2 }]);
    }

    #[test]
    fn plans_replace_without_tail_alignment() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "changed"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/2/pages/new-1.pdf"),
        ];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(
            ops,
            vec![PagePatchOp::ReplacePage {
                index: 1,
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: Some("/artifacts/rev/2/pages/new-1.svg".to_string()),
            }]
        );
    }

    #[test]
    fn plans_replace_without_tail_alignment_preserves_missing_svg_url() {
        let previous = vec![checkpoint_page("old-0", 0, "hash-0")];
        let current = vec![checkpoint_page("new-0", 0, "changed")];
        let artifacts = vec![PagePreviewArtifact {
            page_id: "new-0".to_string(),
            pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
            svg_url: None,
        }];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(
            ops,
            vec![PagePatchOp::ReplacePage {
                index: 0,
                page_id: "new-0".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                svg_url: None,
            }]
        );
    }

    #[test]
    fn plans_replace_all_without_tail_alignment() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "changed-0"),
            checkpoint_page("new-1", 1, "changed-1"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/2/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/2/pages/new-1.pdf"),
        ];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(
            ops,
            vec![
                PagePatchOp::ReplacePage {
                    index: 0,
                    page_id: "new-0".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-0.svg".to_string()),
                },
                PagePatchOp::ReplacePage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-1.svg".to_string()),
                },
            ]
        );
    }

    #[test]
    fn plans_replace_all_without_tail_alignment_preserve_missing_svg_urls() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "changed-0"),
            checkpoint_page("new-1", 1, "changed-1"),
        ];
        let artifacts = vec![
            PagePreviewArtifact {
                page_id: "new-0".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                svg_url: None,
            },
            PagePreviewArtifact {
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: None,
            },
        ];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(
            ops,
            vec![
                PagePatchOp::ReplacePage {
                    index: 0,
                    page_id: "new-0".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                    svg_url: None,
                },
                PagePatchOp::ReplacePage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: None,
                },
            ]
        );
    }

    #[test]
    fn plans_replace_and_delete_without_tail_alignment() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "changed"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/2/pages/new-1.pdf"),
        ];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(
            ops,
            vec![
                PagePatchOp::ReplacePage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-1.svg".to_string()),
                },
                PagePatchOp::DeletePage { index: 2 },
            ]
        );
    }

    #[test]
    fn plans_replace_and_delete_without_tail_alignment_preserve_missing_svg_url() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
            checkpoint_page("old-2", 2, "hash-2"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "changed"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            PagePreviewArtifact {
                page_id: "new-1".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                svg_url: None,
            },
        ];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(
            ops,
            vec![
                PagePatchOp::ReplacePage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: None,
                },
                PagePatchOp::DeletePage { index: 2 },
            ]
        );
    }

    #[test]
    fn plans_replace_and_insert_without_tail_alignment() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "hash-0"),
            checkpoint_page("new-1", 1, "changed"),
            checkpoint_page("new-2", 2, "hash-2"),
        ];
        let artifacts = vec![
            artifact("new-0", "/artifacts/rev/1/pages/new-0.pdf"),
            artifact("new-1", "/artifacts/rev/2/pages/new-1.pdf"),
            artifact("new-2", "/artifacts/rev/2/pages/new-2.pdf"),
        ];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(
            ops,
            vec![
                PagePatchOp::ReplacePage {
                    index: 1,
                    page_id: "new-1".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-1.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-1.svg".to_string()),
                },
                PagePatchOp::InsertPage {
                    index: 2,
                    page_id: "new-2".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-2.pdf".to_string(),
                    svg_url: Some("/artifacts/rev/2/pages/new-2.svg".to_string()),
                },
            ]
        );
    }

    #[test]
    fn plans_replace_and_insert_without_tail_alignment_preserve_missing_svg_urls() {
        let previous = vec![
            checkpoint_page("old-0", 0, "hash-0"),
            checkpoint_page("old-1", 1, "hash-1"),
        ];
        let current = vec![
            checkpoint_page("new-0", 0, "changed"),
            checkpoint_page("new-1", 1, "hash-1"),
            checkpoint_page("new-2", 2, "inserted"),
        ];
        let artifacts = vec![
            PagePreviewArtifact {
                page_id: "new-0".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                svg_url: None,
            },
            artifact("new-1", "/artifacts/rev/1/pages/new-1.pdf"),
            PagePreviewArtifact {
                page_id: "new-2".to_string(),
                pdf_url: "/artifacts/rev/2/pages/new-2.pdf".to_string(),
                svg_url: None,
            },
        ];

        let ops = plan_page_patches(&previous, &current, &artifacts, None);

        assert_eq!(
            ops,
            vec![
                PagePatchOp::ReplacePage {
                    index: 0,
                    page_id: "new-0".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-0.pdf".to_string(),
                    svg_url: None,
                },
                PagePatchOp::InsertPage {
                    index: 2,
                    page_id: "new-2".to_string(),
                    pdf_url: "/artifacts/rev/2/pages/new-2.pdf".to_string(),
                    svg_url: None,
                },
            ]
        );
    }

    fn checkpoint_page(page_id: &str, index: usize, content_hash: &str) -> CheckpointPage {
        CheckpointPage {
            page_id: page_id.to_string(),
            index,
            content_hash: content_hash.to_string(),
            text_start_utf8: (index * 10) as u32,
            text_end_utf8: (index * 10 + 10) as u32,
        }
    }

    fn artifact(page_id: &str, pdf_url: &str) -> PagePreviewArtifact {
        PagePreviewArtifact {
            page_id: page_id.to_string(),
            pdf_url: pdf_url.to_string(),
            svg_url: Some(
                pdf_url
                    .replace("/pages/", "/pages/")
                    .replace(".pdf", ".svg"),
            ),
        }
    }
}
