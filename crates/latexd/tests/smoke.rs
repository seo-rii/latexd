use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    sync::{Mutex, MutexGuard, OnceLock},
};

use camino::{Utf8Path, Utf8PathBuf};
use hmr_protocol::PagePatchOp;
use latexd::{
    PreviewSnapshot,
    compiler::{CompileOutcome, CompileRequest, CompilerDriver, PageSyncMapArtifact},
};
use tempfile::tempdir;
use tex_aux::{MaterializedRewriteSpan, SemanticAuxIndex, load_semantic_aux};
use tex_checkpoint::{CheckpointKind, load_checkpoint_bundle};
use tex_vm::VmModuleCheckpointKind;
use tex_world::ProjectWorld;

include!("support/smoke_harness_helpers.rs");

include!("smoke/external_oracle.rs");

include!("smoke/bibliography_features.rs");

include!("smoke/reference_variants.rs");

include!("smoke/internal_baselines.rs");

include!("smoke/bibliography_output.rs");

include!("smoke/bibliography.rs");

include!("smoke/unchanged_tail.rs");

include!("smoke/replay_selection.rs");
