use std::collections::{BTreeMap, HashMap, VecDeque};

use camino::Utf8PathBuf;
use tex_lexer::Mouth;
use tex_tokens::{CatCode, Token};

use crate::snapshot::VmReplayFrame;

#[derive(Debug, Clone)]
pub(crate) enum QueueItem {
    Token(Token),
    CharacterSource(Mouth),
    ModuleEnd {
        path: Utf8PathBuf,
        source_start_utf8: u32,
        source_end_utf8: u32,
        output_start_utf8: u32,
        checkpoint: Option<PendingModuleCheckpoint>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct PendingModuleCheckpoint {
    pub(crate) resume_path: Option<Utf8PathBuf>,
    pub(crate) source_offset_utf8: u32,
    pub(crate) continuation_stack: Vec<VmReplayFrame>,
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveSourceFrame {
    pub(crate) path: Utf8PathBuf,
    pub(crate) return_to_parent: Option<VmReplayFrame>,
    pub(crate) global_definition_base_scope: Option<usize>,
    pub(crate) module_kind: Option<ActiveModuleKind>,
    pub(crate) catcode_overrides: BTreeMap<char, CatCode>,
    pub(crate) suppressed_catcode_overrides: BTreeMap<char, usize>,
    pub(crate) end_hooks: Vec<Vec<Token>>,
    pub(crate) module_options: Option<ActiveModuleOptions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveModuleKind {
    Package,
    Class,
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveModuleOptions {
    pub(crate) default_options: Vec<String>,
    pub(crate) passed_options: Vec<String>,
    pub(crate) forwarded_options: Vec<String>,
    pub(crate) declared_options: HashMap<String, Vec<Token>>,
    pub(crate) default_option_body: Option<Vec<Token>>,
}

#[derive(Debug)]
pub(crate) struct RestoredInputContinuation {
    pub(crate) queue: VecDeque<QueueItem>,
    pub(crate) source_stack: Vec<ActiveSourceFrame>,
    pub(crate) last_token_end_utf8: u32,
}
