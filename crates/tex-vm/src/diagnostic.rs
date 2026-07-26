#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmDiagnostic {
    pub kind: VmDiagnosticKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmDiagnosticKind {
    UndefinedControlSequence,
    MissingFile,
    ExplicitError,
}
