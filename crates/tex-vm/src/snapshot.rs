use std::collections::{BTreeMap, HashMap};

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use tex_lexer::{Mouth, MouthSnapshot};
use tex_tokens::CatCode;

pub const VM_CONTINUATION_SAFETY_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmReplayFrame {
    pub path: Utf8PathBuf,
    pub source_offset_utf8: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmModuleCheckpointKind {
    #[default]
    Enter,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmModuleCheckpoint {
    pub kind: VmModuleCheckpointKind,
    pub module_path: Utf8PathBuf,
    pub resume_path: Option<Utf8PathBuf>,
    pub source_offset_utf8: u32,
    pub continuation_stack: Vec<VmReplayFrame>,
    pub output_start_utf8: u32,
    pub snapshot: VmSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmContinuationBlocker {
    UnverifiedSnapshot,
    OpenGroup,
    OpenConditional,
    ActiveInput,
    PendingGlobalPrefix,
    RenderEventSink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmContinuationSafety {
    pub schema_version: u32,
    pub blockers: Vec<VmContinuationBlocker>,
}

impl VmContinuationSafety {
    pub fn is_safe(&self) -> bool {
        self.schema_version == VM_CONTINUATION_SAFETY_SCHEMA_VERSION && self.blockers.is_empty()
    }
}

impl Default for VmContinuationSafety {
    fn default() -> Self {
        Self {
            schema_version: VM_CONTINUATION_SAFETY_SCHEMA_VERSION,
            blockers: vec![VmContinuationBlocker::UnverifiedSnapshot],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSnapshot {
    #[serde(default)]
    pub continuation_safety: VmContinuationSafety,
    #[serde(default)]
    pub input_continuation: Option<VmInputContinuationSnapshot>,
    pub scopes: Vec<HashMap<String, SnapshotMeaning>>,
    pub registers: BTreeMap<u32, i32>,
    #[serde(default)]
    pub dimen_registers: BTreeMap<u32, i32>,
    #[serde(default)]
    pub skip_registers: BTreeMap<u32, i32>,
    #[serde(default)]
    pub token_registers: BTreeMap<u32, Vec<SnapshotToken>>,
    #[serde(default)]
    pub catcodes: BTreeMap<char, CatCode>,
    #[serde(default = "default_next_count_register")]
    pub next_count_register: u32,
    #[serde(default = "default_next_dimen_register")]
    pub next_dimen_register: u32,
    #[serde(default = "default_next_skip_register")]
    pub next_skip_register: u32,
    #[serde(default = "default_next_toks_register")]
    pub next_toks_register: u32,
    #[serde(default = "default_next_read_stream")]
    pub next_read_stream: u32,
    #[serde(default = "default_next_write_stream")]
    pub next_write_stream: u32,
    pub loaded_modules: Vec<Utf8PathBuf>,
    pub include_only: Option<Vec<Utf8PathBuf>>,
    #[serde(default)]
    pub aftergroup_tokens: Vec<Vec<SnapshotToken>>,
    #[serde(default)]
    pub after_assignment_token: Option<SnapshotToken>,
    #[serde(default)]
    pub at_end_document_hooks: Vec<Vec<SnapshotToken>>,
    #[serde(default)]
    pub tempswa: bool,
    #[serde(default = "default_filesw")]
    pub filesw: bool,
    #[serde(default)]
    pub in_at: bool,
    #[serde(default)]
    pub negate_next_conditional: bool,
    #[serde(default)]
    pub provided_files: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    pub provided_packages: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    pub provided_classes: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    pub loaded_package_options: BTreeMap<Utf8PathBuf, Vec<String>>,
    #[serde(default)]
    pub loaded_class_options: BTreeMap<Utf8PathBuf, Vec<String>>,
    #[serde(default)]
    pub pending_package_options: BTreeMap<Utf8PathBuf, Vec<String>>,
    #[serde(default)]
    pub pending_class_options: BTreeMap<Utf8PathBuf, Vec<String>>,
    #[serde(default)]
    pub counter_resets: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub read_stream_lines: BTreeMap<u32, Vec<String>>,
    #[serde(default)]
    pub read_stream_eof: BTreeMap<u32, bool>,
    #[serde(default)]
    pub legacy_math_output_active: bool,
    #[serde(default)]
    pub legacy_math_pending_word_boundary: bool,
    #[serde(default)]
    pub legacy_math_text_wrapper_restore_scope_depth: Option<usize>,
    #[serde(default)]
    pub legacy_math_script_boundary_scope_depths: Vec<usize>,
    #[serde(default)]
    pub legacy_output_last_char: Option<char>,
    #[serde(default)]
    pub legacy_text_script_boundary_pending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmInputContinuationSnapshot {
    pub queue: Vec<VmQueueItemSnapshot>,
    pub source_stack: Vec<VmActiveSourceFrameSnapshot>,
    pub last_token_end_utf8: u32,
}

impl VmInputContinuationSnapshot {
    pub fn is_restorable(&self) -> bool {
        !self.source_stack.is_empty()
            && self.queue.iter().all(|item| match item {
                VmQueueItemSnapshot::Token { token } => token.start_utf8 <= token.end_utf8,
                VmQueueItemSnapshot::CharacterSource { mouth } => Mouth::restore(mouth).is_some(),
                VmQueueItemSnapshot::ModuleEnd {
                    source_start_utf8,
                    source_end_utf8,
                    ..
                } => source_start_utf8 <= source_end_utf8,
            })
    }

    pub fn matches_character_sources<'a>(
        &self,
        sources: impl IntoIterator<Item = &'a str>,
    ) -> bool {
        let mut sources = sources.into_iter();
        for mouth in self.queue.iter().filter_map(|item| match item {
            VmQueueItemSnapshot::CharacterSource { mouth } => Some(mouth),
            VmQueueItemSnapshot::Token { .. } | VmQueueItemSnapshot::ModuleEnd { .. } => None,
        }) {
            if sources.next() != Some(mouth.input()) {
                return false;
            }
        }
        sources.next().is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VmQueueItemSnapshot {
    Token {
        token: SnapshotToken,
    },
    CharacterSource {
        mouth: MouthSnapshot,
    },
    ModuleEnd {
        path: Utf8PathBuf,
        source_start_utf8: u32,
        source_end_utf8: u32,
        output_start_utf8: u32,
        checkpoint: Option<VmPendingModuleCheckpointSnapshot>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmPendingModuleCheckpointSnapshot {
    pub resume_path: Option<Utf8PathBuf>,
    pub source_offset_utf8: u32,
    pub continuation_stack: Vec<VmReplayFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmActiveSourceFrameSnapshot {
    pub path: Utf8PathBuf,
    pub return_to_parent: Option<VmReplayFrame>,
    pub global_definition_base_scope: Option<usize>,
    pub module_kind: Option<VmActiveModuleKindSnapshot>,
    pub catcode_overrides: BTreeMap<char, CatCode>,
    pub suppressed_catcode_overrides: BTreeMap<char, usize>,
    pub end_hooks: Vec<Vec<SnapshotToken>>,
    pub module_options: Option<VmActiveModuleOptionsSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmActiveModuleKindSnapshot {
    Package,
    Class,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmActiveModuleOptionsSnapshot {
    pub default_options: Vec<String>,
    pub passed_options: Vec<String>,
    pub forwarded_options: Vec<String>,
    pub declared_options: BTreeMap<String, Vec<SnapshotToken>>,
    pub default_option_body: Option<Vec<SnapshotToken>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SnapshotMeaning {
    Macro {
        parameter_count: u8,
        #[serde(default)]
        optional_first_argument_default: Option<Vec<SnapshotToken>>,
        body: Vec<SnapshotToken>,
    },
    Primitive {
        name: String,
    },
    Token {
        token: SnapshotToken,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotToken {
    pub kind: SnapshotTokenKind,
    #[serde(default)]
    pub start_utf8: u32,
    #[serde(default)]
    pub end_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SnapshotTokenKind {
    ControlSequence { name: String },
    Character { ch: char, catcode: CatCode },
}

pub(crate) fn default_next_count_register() -> u32 {
    256
}

pub(crate) fn default_next_dimen_register() -> u32 {
    256
}

pub(crate) fn default_next_skip_register() -> u32 {
    256
}

pub(crate) fn default_next_toks_register() -> u32 {
    0
}

pub(crate) fn default_next_read_stream() -> u32 {
    0
}

pub(crate) fn default_next_write_stream() -> u32 {
    16
}

fn default_filesw() -> bool {
    true
}
