use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use tex_render_model::RenderEventEnvelope;

use crate::{diagnostic::VmDiagnostic, snapshot::VmModuleCheckpoint};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmOutcome {
    pub output: String,
    pub render_events: Vec<RenderEventEnvelope>,
    pub registers: BTreeMap<u32, i32>,
    pub transcript: Vec<String>,
    pub diagnostics: Vec<VmDiagnostic>,
    pub loaded_modules: Vec<Utf8PathBuf>,
    pub module_traces: Vec<VmModuleTrace>,
    pub module_checkpoints: Vec<VmModuleCheckpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmModuleTrace {
    pub path: Utf8PathBuf,
    pub source_start_utf8: u32,
    pub source_end_utf8: u32,
    pub output_start_utf8: u32,
    pub output_end_utf8: u32,
}
