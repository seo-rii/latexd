use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt,
    ops::{Deref, DerefMut},
};

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use tex_lexer::{Mouth, MouthSnapshot};
use tex_render_model::{
    CaptionInlinePlaceholderEvent, CaptionKind, EventProducer, EventSequence, FootnoteId, ListKind,
    ProvenanceSpan, RenderEvent, RenderEventEnvelope, SourceProvenance, SourceSpanRole,
    TableCellEvent, TableColumnSpec, TableRowEvent,
};
use tex_tokens::CatCode;

use crate::{
    diagnostic::VmDiagnostic,
    dimension_parameter::{DimensionParameterId, VmDimensionParameterStateV1},
    outcome::VmModuleTrace,
};

pub const VM_CONTINUATION_SAFETY_SCHEMA_VERSION: u32 = 2;
pub const VM_SEMANTIC_CAPTURE_SCHEMA_VERSION: u32 = 22;
pub const VM_SNAPSHOT_DOCUMENT_FORMAT: &str = "latexd.vm-snapshot";
pub const VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION: u32 = 1;
pub const VM_SNAPSHOT_MUSKIP_ALIAS_V1_CAPABILITY: &str = "eqtb.muskip.alias-v1";
pub const VM_SNAPSHOT_MUSKIP_SCALAR_V1_CAPABILITY: &str = "eqtb.muskip.scalar-v1";
pub const VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY: &str = "eqtb.mathcode.table-v1";
pub const VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY: &str = "eqtb.delcode.table-v1";
pub const VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY: &str =
    "eqtb.integer-parameter-state.v1";
pub const VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY: &str =
    "eqtb.layout-integer-parameter-state.v1";
pub const VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY: &str =
    "primitive.layout-integer-parameter-command.v1";
pub const VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY: &str =
    "eqtb.dimension-parameter-state.v1";
pub const VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY: &str =
    "primitive.dimension-parameter-command.v1";
pub const VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES: &[&str] = &[
    VM_SNAPSHOT_MUSKIP_ALIAS_V1_CAPABILITY,
    VM_SNAPSHOT_MUSKIP_SCALAR_V1_CAPABILITY,
    VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY,
    VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY,
    VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
    VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
    VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY,
];
pub const VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES: &[&str] = &[
    VM_SNAPSHOT_MUSKIP_ALIAS_V1_CAPABILITY,
    VM_SNAPSHOT_MUSKIP_SCALAR_V1_CAPABILITY,
    VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY,
    VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY,
    VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
    VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
    VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY,
    VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY,
    VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY,
];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct SnapshotCapability(String);

impl SnapshotCapability {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SnapshotCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmSnapshotDocument {
    pub format: String,
    pub schema_version: u32,
    pub required_capabilities: BTreeSet<SnapshotCapability>,
    pub state: VmSnapshot,
}

#[derive(Serialize)]
struct VmSnapshotDocumentWriteWire<'a> {
    format: &'a str,
    schema_version: u32,
    required_capabilities: &'a BTreeSet<SnapshotCapability>,
    state: &'a serde_json::Value,
}

impl Serialize for VmSnapshotDocument {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.validate_for_write()
            .map_err(serde::ser::Error::custom)?;
        let legacy: &LegacyVmSnapshotV1 = &self.state;
        let mut state = serde_json::to_value(legacy).map_err(serde::ser::Error::custom)?;
        let state_fields = state.as_object_mut().ok_or_else(|| {
            serde::ser::Error::custom("legacy VM snapshot did not serialize as an object")
        })?;
        if state_fields
            .insert(
                "muskip_registers".to_string(),
                serde_json::to_value(&self.state.muskip_registers)
                    .map_err(serde::ser::Error::custom)?,
            )
            .is_some()
        {
            return Err(serde::ser::Error::custom(
                "legacy VM snapshot collides with reserved field muskip_registers",
            ));
        }
        if state_fields
            .insert(
                "next_muskip_register".to_string(),
                serde_json::Value::from(self.state.next_muskip_register),
            )
            .is_some()
        {
            return Err(serde::ser::Error::custom(
                "legacy VM snapshot collides with reserved field next_muskip_register",
            ));
        }
        for (name, code_table_state) in [
            ("mathcode_state", self.state.mathcode_state.as_ref()),
            ("delcode_state", self.state.delcode_state.as_ref()),
        ] {
            let Some(code_table_state) = code_table_state else {
                continue;
            };
            if state_fields
                .insert(
                    name.to_string(),
                    serde_json::to_value(code_table_state).map_err(serde::ser::Error::custom)?,
                )
                .is_some()
            {
                return Err(serde::ser::Error::custom(format!(
                    "legacy VM snapshot collides with reserved field {name}"
                )));
            }
        }
        if let Some(integer_parameter_state) = &self.state.integer_parameter_state {
            let name = "integer_parameter_state";
            if state_fields
                .insert(
                    name.to_string(),
                    serde_json::to_value(integer_parameter_state)
                        .map_err(serde::ser::Error::custom)?,
                )
                .is_some()
            {
                return Err(serde::ser::Error::custom(format!(
                    "legacy VM snapshot collides with reserved field {name}"
                )));
            }
        }
        if let Some(layout_integer_parameter_state) = &self.state.layout_integer_parameter_state {
            let name = "layout_integer_parameter_state";
            if state_fields
                .insert(
                    name.to_string(),
                    serde_json::to_value(layout_integer_parameter_state)
                        .map_err(serde::ser::Error::custom)?,
                )
                .is_some()
            {
                return Err(serde::ser::Error::custom(format!(
                    "legacy VM snapshot collides with reserved field {name}"
                )));
            }
        }

        VmSnapshotDocumentWriteWire {
            format: &self.format,
            schema_version: self.schema_version,
            required_capabilities: &self.required_capabilities,
            state: &state,
        }
        .serialize(serializer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmSnapshotDocumentError {
    MalformedDocument(String),
    UnsupportedFormat(String),
    UnsupportedSchemaVersion(u32),
    UnsupportedCapability(String),
    InvalidState(String),
}

impl fmt::Display for VmSnapshotDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedDocument(error) => {
                write!(formatter, "malformed VM snapshot document: {error}")
            }
            Self::UnsupportedFormat(format) => {
                write!(
                    formatter,
                    "unsupported VM snapshot document format: {format}"
                )
            }
            Self::UnsupportedSchemaVersion(version) => write!(
                formatter,
                "unsupported VM snapshot document schema version: {version}"
            ),
            Self::UnsupportedCapability(capability) => {
                write!(
                    formatter,
                    "unsupported VM snapshot capability: {capability}"
                )
            }
            Self::InvalidState(error) => {
                write!(formatter, "invalid VM snapshot document state: {error}")
            }
        }
    }
}

impl std::error::Error for VmSnapshotDocumentError {}

impl VmSnapshotDocument {
    pub fn from_snapshot(state: VmSnapshot) -> Self {
        let required_capabilities = state.required_capabilities();
        Self {
            format: VM_SNAPSHOT_DOCUMENT_FORMAT.to_string(),
            schema_version: VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION,
            required_capabilities,
            state,
        }
    }

    pub fn validate_for_write(&self) -> Result<(), VmSnapshotDocumentError> {
        if self.format != VM_SNAPSHOT_DOCUMENT_FORMAT {
            return Err(VmSnapshotDocumentError::UnsupportedFormat(
                self.format.clone(),
            ));
        }
        if self.schema_version != VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION {
            return Err(VmSnapshotDocumentError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        if let Some(capability) = self.required_capabilities.iter().find(|capability| {
            !VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES.contains(&capability.as_str())
        }) {
            return Err(VmSnapshotDocumentError::UnsupportedCapability(
                capability.as_str().to_string(),
            ));
        }
        let derived_capabilities = self.state.required_capabilities();
        if self.required_capabilities != derived_capabilities {
            return Err(VmSnapshotDocumentError::InvalidState(format!(
                "declared capabilities [{}] do not match state requirements [{}]",
                self.required_capabilities
                    .iter()
                    .map(SnapshotCapability::as_str)
                    .collect::<Vec<_>>()
                    .join(", "),
                derived_capabilities
                    .iter()
                    .map(SnapshotCapability::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        crate::validate_snapshot_for_restore(&self.state)
            .map_err(|error| VmSnapshotDocumentError::InvalidState(error.to_string()))?;
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVmSnapshotDocument<'a> {
    format: String,
    schema_version: u32,
    required_capabilities: BTreeSet<String>,
    #[serde(borrow)]
    state: &'a RawValue,
}

struct RawVersionedVmSnapshotState {
    fields: BTreeMap<String, Box<RawValue>>,
}

impl<'de> Deserialize<'de> for RawVersionedVmSnapshotState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StateVisitor;

        impl<'de> serde::de::Visitor<'de> for StateVisitor {
            type Value = RawVersionedVmSnapshotState;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a VM snapshot state object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut fields = BTreeMap::new();
                while let Some(name) = map.next_key::<String>()? {
                    if fields.contains_key(&name) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate state member `{name}`"
                        )));
                    }
                    let value = map.next_value::<Box<RawValue>>()?;
                    fields.insert(name, value);
                }
                Ok(RawVersionedVmSnapshotState { fields })
            }
        }

        deserializer.deserialize_map(StateVisitor)
    }
}

struct StrictMuskipRegisters(BTreeMap<u32, i32>);

impl<'de> Deserialize<'de> for StrictMuskipRegisters {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RegistersVisitor;

        impl<'de> serde::de::Visitor<'de> for RegistersVisitor {
            type Value = StrictMuskipRegisters;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object of canonical muskip register indices")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut raw_registers = BTreeMap::new();
                while let Some(raw_index) = map.next_key::<String>()? {
                    let index = raw_index.parse::<u32>().map_err(|_| {
                        serde::de::Error::custom(format!(
                            "invalid muskip register index `{raw_index}`"
                        ))
                    })?;
                    if raw_index != index.to_string() {
                        return Err(serde::de::Error::custom(format!(
                            "non-canonical muskip register index `{raw_index}`"
                        )));
                    }
                    if raw_registers.contains_key(&index) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate muskip register `{index}`"
                        )));
                    }
                    let value = map.next_value::<Box<RawValue>>()?;
                    raw_registers.insert(index, value);
                }

                let mut registers = BTreeMap::new();
                for (index, raw_value) in raw_registers {
                    let value = serde_json::from_str::<i32>(raw_value.get())
                        .map_err(serde::de::Error::custom)?;
                    registers.insert(index, value);
                }
                Ok(StrictMuskipRegisters(registers))
            }
        }

        deserializer.deserialize_map(RegistersVisitor)
    }
}

pub fn decode_vm_snapshot_document(
    document: &[u8],
) -> Result<VmSnapshotDocument, VmSnapshotDocumentError> {
    let document = serde_json::from_slice::<RawVmSnapshotDocument>(document)
        .map_err(|error| VmSnapshotDocumentError::MalformedDocument(error.to_string()))?;
    if document.format != VM_SNAPSHOT_DOCUMENT_FORMAT {
        return Err(VmSnapshotDocumentError::UnsupportedFormat(document.format));
    }
    if document.schema_version != VM_SNAPSHOT_DOCUMENT_SCHEMA_VERSION {
        return Err(VmSnapshotDocumentError::UnsupportedSchemaVersion(
            document.schema_version,
        ));
    }
    if let Some(capability) = document.required_capabilities.iter().find(|capability| {
        !VM_SNAPSHOT_DOCUMENT_READABLE_CAPABILITIES.contains(&capability.as_str())
    }) {
        return Err(VmSnapshotDocumentError::UnsupportedCapability(
            capability.clone(),
        ));
    }

    let declared_capabilities = document
        .required_capabilities
        .iter()
        .cloned()
        .map(SnapshotCapability::new)
        .collect::<BTreeSet<_>>();
    let mut state_fields =
        serde_json::from_str::<RawVersionedVmSnapshotState>(document.state.get())
            .map_err(|error| VmSnapshotDocumentError::InvalidState(error.to_string()))?
            .fields;
    let muskip_registers = state_fields
        .remove("muskip_registers")
        .map(|raw| serde_json::from_str::<StrictMuskipRegisters>(raw.get()))
        .transpose()
        .map_err(|error| VmSnapshotDocumentError::InvalidState(error.to_string()))?
        .map(|registers| registers.0)
        .unwrap_or_default();
    let next_muskip_register = state_fields
        .remove("next_muskip_register")
        .map(|raw| serde_json::from_str::<u32>(raw.get()))
        .transpose()
        .map_err(|error| VmSnapshotDocumentError::InvalidState(error.to_string()))?
        .unwrap_or_else(default_next_muskip_register);
    let mathcode_state = state_fields
        .remove("mathcode_state")
        .map(|raw| serde_json::from_str::<VmCodeTableStateV1>(raw.get()))
        .transpose()
        .map_err(|error| VmSnapshotDocumentError::InvalidState(error.to_string()))?;
    let delcode_state = state_fields
        .remove("delcode_state")
        .map(|raw| serde_json::from_str::<VmCodeTableStateV1>(raw.get()))
        .transpose()
        .map_err(|error| VmSnapshotDocumentError::InvalidState(error.to_string()))?;
    let integer_parameter_state = state_fields
        .remove("integer_parameter_state")
        .map(|raw| serde_json::from_str::<VmIntegerParameterStateV1>(raw.get()))
        .transpose()
        .map_err(|error| VmSnapshotDocumentError::InvalidState(error.to_string()))?;
    let layout_integer_parameter_state = state_fields
        .remove("layout_integer_parameter_state")
        .map(|raw| serde_json::from_str::<VmLayoutIntegerParameterStateV1>(raw.get()))
        .transpose()
        .map_err(|error| VmSnapshotDocumentError::InvalidState(error.to_string()))?;
    let dimension_parameter_state = state_fields
        .remove("dimension_parameter_state")
        .map(|raw| serde_json::from_str::<VmDimensionParameterStateV1>(raw.get()))
        .transpose()
        .map_err(|error| VmSnapshotDocumentError::InvalidState(error.to_string()))?;
    let mut legacy_fields = serde_json::Map::new();
    for (name, raw) in state_fields {
        let value = serde_json::from_str(raw.get())
            .map_err(|error| VmSnapshotDocumentError::InvalidState(error.to_string()))?;
        legacy_fields.insert(name, value);
    }
    let legacy =
        serde_json::from_value::<LegacyVmSnapshotV1>(serde_json::Value::Object(legacy_fields))
            .map_err(|error| VmSnapshotDocumentError::InvalidState(error.to_string()))?;
    if let Some(state) = &mathcode_state {
        state
            .validate_mathcode(legacy.scopes.len())
            .map_err(VmSnapshotDocumentError::InvalidState)?;
    }
    if let Some(state) = &delcode_state {
        state
            .validate_delcode(legacy.scopes.len())
            .map_err(VmSnapshotDocumentError::InvalidState)?;
    }
    if let Some(state) = &integer_parameter_state {
        state
            .validate(legacy.scopes.len())
            .map_err(VmSnapshotDocumentError::InvalidState)?;
    }
    if let Some(state) = &layout_integer_parameter_state {
        state
            .validate(legacy.scopes.len())
            .map_err(VmSnapshotDocumentError::InvalidState)?;
    }
    if let Some(state) = &dimension_parameter_state {
        state
            .validate(legacy.scopes.len())
            .map_err(VmSnapshotDocumentError::InvalidState)?;
    }
    let state = VmSnapshot::from_parts(
        legacy,
        muskip_registers,
        next_muskip_register,
        mathcode_state,
        delcode_state,
        integer_parameter_state,
        layout_integer_parameter_state,
        dimension_parameter_state,
    );
    let derived_capabilities = state.required_capabilities();
    if declared_capabilities != derived_capabilities {
        return Err(VmSnapshotDocumentError::InvalidState(format!(
            "declared capabilities [{}] do not match state requirements [{}]",
            declared_capabilities
                .iter()
                .map(SnapshotCapability::as_str)
                .collect::<Vec<_>>()
                .join(", "),
            derived_capabilities
                .iter()
                .map(SnapshotCapability::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    Ok(VmSnapshotDocument {
        format: document.format,
        schema_version: document.schema_version,
        required_capabilities: declared_capabilities,
        state,
    })
}

pub fn normalize_legacy_vm_snapshot(snapshot: VmSnapshot) -> VmSnapshotDocument {
    VmSnapshotDocument::from_snapshot(snapshot)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct VmReplayFrame {
    pub path: Utf8PathBuf,
    pub source_offset_utf8: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct VmExecutionAnchor {
    pub path: Utf8PathBuf,
    pub continuation_stack: Vec<VmReplayFrame>,
    #[serde(default)]
    pub occurrence: u64,
}

impl VmExecutionAnchor {
    fn is_restorable(&self) -> bool {
        !self.path.as_str().is_empty()
            && self
                .continuation_stack
                .iter()
                .all(|frame| !frame.path.as_str().is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExecutionOccurrenceSnapshot {
    pub base_anchor: VmExecutionAnchor,
    pub next_occurrence: u64,
}

impl VmExecutionOccurrenceSnapshot {
    fn is_restorable(&self) -> bool {
        self.base_anchor.occurrence == 0
            && self.base_anchor.is_restorable()
            && self.next_occurrence >= 1
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmEventExecutionAnchorSnapshot {
    #[serde(alias = "event_id")]
    pub event_sequence: EventSequence,
    pub execution_anchor: VmExecutionAnchor,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmModuleCheckpointKind {
    #[default]
    Enter,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmModuleBoundary {
    pub kind: VmModuleCheckpointKind,
    pub module_path: Utf8PathBuf,
    pub resume_path: Option<Utf8PathBuf>,
    pub source_offset_utf8: u32,
    pub continuation_stack: Vec<VmReplayFrame>,
    pub output_start_utf8: u32,
}

impl From<&VmModuleCheckpoint> for VmModuleBoundary {
    fn from(checkpoint: &VmModuleCheckpoint) -> Self {
        Self {
            kind: checkpoint.kind,
            module_path: checkpoint.module_path.clone(),
            resume_path: checkpoint.resume_path.clone(),
            source_offset_utf8: checkpoint.source_offset_utf8,
            continuation_stack: checkpoint.continuation_stack.clone(),
            output_start_utf8: checkpoint.output_start_utf8,
        }
    }
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
    // Kept so schema-v2 snapshots produced before sink replay support still deserialize.
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
pub struct VmSemanticSinkSnapshot {
    #[serde(default)]
    pub events: Vec<RenderEventEnvelope>,
    #[serde(
        default = "default_next_render_event_sequence",
        alias = "next_event_id"
    )]
    pub next_event_sequence: EventSequence,
    #[serde(
        default = "default_next_render_event_sequence",
        alias = "batch_start_event_id"
    )]
    pub batch_start_event_sequence: EventSequence,
    #[serde(default)]
    pub epoch: u64,
}

impl VmSemanticSinkSnapshot {
    pub fn is_restorable(&self) -> bool {
        let mut event_sequences = BTreeSet::new();
        self.next_event_sequence >= 1
            && self.batch_start_event_sequence >= 1
            && self.batch_start_event_sequence <= self.next_event_sequence
            && self.events.iter().all(|event| {
                event.meta.sequence >= 1
                    && event.meta.sequence < self.next_event_sequence
                    && event_sequences.insert(event.meta.sequence)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticCaptureSnapshot {
    pub schema_version: u32,
    #[serde(default)]
    pub source_buffers: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    pub scanner_event_anchors: Vec<VmEventExecutionAnchorSnapshot>,
    #[serde(default)]
    pub execution_occurrences: Vec<VmExecutionOccurrenceSnapshot>,
    #[serde(default)]
    pub execution_in_document: bool,
    #[serde(default)]
    pub execution_no_hyper_depth: u32,
    #[serde(default)]
    pub math: VmSemanticMathSnapshot,
    #[serde(default)]
    pub text: VmSemanticTextSnapshot,
    #[serde(default)]
    pub graphic: VmSemanticGraphicSnapshot,
    #[serde(default)]
    pub list: VmSemanticListSnapshot,
    #[serde(default)]
    pub environment: VmSemanticEnvironmentSnapshot,
    #[serde(default)]
    pub table: VmSemanticTableSnapshot,
    #[serde(default)]
    pub inline: VmSemanticInlineSnapshot,
    #[serde(default)]
    pub footnote: VmSemanticFootnoteSnapshot,
    #[serde(default)]
    pub front_matter: VmSemanticFrontMatterSnapshot,
    #[serde(default)]
    pub heading: VmSemanticHeadingSnapshot,
    #[serde(default)]
    pub caption: VmSemanticCaptionSnapshot,
    #[serde(default)]
    pub bibliography: VmSemanticBibliographySnapshot,
}

impl VmSemanticCaptureSnapshot {
    pub fn is_restorable(&self) -> bool {
        let active_math_source_is_valid = self.math.active_capture.as_ref().is_none_or(|capture| {
            let Some(source) = self.source_buffers.get(&capture.source_path) else {
                return false;
            };
            let invocation_start = capture.invocation_start_utf8 as usize;
            let content_start = capture.content_start_utf8 as usize;
            self.execution_in_document
                && invocation_start <= content_start
                && content_start <= source.len()
                && source.is_char_boundary(invocation_start)
                && source.is_char_boundary(content_start)
        });
        let active_text_source_is_valid = self.text.active_capture.as_ref().is_none_or(|capture| {
            let Some(path) = &capture.literal_path else {
                return true;
            };
            let Some(source) = self.source_buffers.get(path) else {
                return false;
            };
            let end_utf8 = capture.end_utf8 as usize;
            end_utf8 <= source.len() && source.is_char_boundary(end_utf8)
        });
        let runtime_execution_anchors = self
            .text
            .executed_event_anchors
            .iter()
            .map(|anchor| &anchor.execution_anchor)
            .chain(
                self.text
                    .forced_execution_ranges
                    .iter()
                    .map(|range| &range.execution_anchor),
            )
            .chain(
                self.text
                    .active_capture
                    .iter()
                    .map(|capture| &capture.execution_anchor),
            )
            .chain(
                self.bibliography
                    .executed_event_anchors
                    .iter()
                    .map(|anchor| &anchor.execution_anchor),
            )
            .chain(
                self.bibliography
                    .active_item
                    .iter()
                    .map(|capture| &capture.execution_anchor),
            )
            .collect::<Vec<_>>();
        self.schema_version == VM_SEMANTIC_CAPTURE_SCHEMA_VERSION
            && event_execution_anchors_are_valid(&self.scanner_event_anchors)
            && execution_occurrences_are_valid(&self.execution_occurrences)
            && execution_occurrences_cover(&self.execution_occurrences, &runtime_execution_anchors)
            && self.math.is_restorable()
            && self.text.is_restorable()
            && self.graphic.is_restorable()
            && self.list.is_restorable()
            && self.environment.is_restorable()
            && self.table.is_restorable()
            && self.inline.is_restorable(
                self.text.executed_events.len(),
                self.math.executed_events.len(),
            )
            && self.footnote.is_restorable(
                self.text.executed_events.len(),
                self.math.executed_events.len(),
                &self.inline,
            )
            && self.front_matter.is_restorable()
            && self.heading.is_restorable(
                self.text.executed_events.len(),
                self.math.executed_events.len(),
                &self.inline,
            )
            && self.caption.is_restorable(
                self.text.executed_events.len(),
                self.math.executed_events.len(),
                &self.inline,
            )
            && self.bibliography.is_restorable(
                self.text.executed_events.len(),
                self.math.executed_events.len(),
                &self.inline,
            )
            && active_math_source_is_valid
            && active_text_source_is_valid
    }
}

fn execution_occurrences_are_valid(occurrences: &[VmExecutionOccurrenceSnapshot]) -> bool {
    occurrences
        .iter()
        .all(VmExecutionOccurrenceSnapshot::is_restorable)
        && values_are_unique(
            &occurrences
                .iter()
                .map(|occurrence| occurrence.base_anchor.clone())
                .collect::<Vec<_>>(),
        )
}

fn execution_occurrences_cover(
    occurrences: &[VmExecutionOccurrenceSnapshot],
    anchors: &[&VmExecutionAnchor],
) -> bool {
    anchors.iter().all(|anchor| {
        let mut base_anchor = (*anchor).clone();
        base_anchor.occurrence = 0;
        occurrences.iter().any(|occurrence| {
            occurrence.base_anchor == base_anchor && occurrence.next_occurrence > anchor.occurrence
        })
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticMathSnapshot {
    #[serde(default)]
    pub scanner_dollar_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub scanner_command_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub executed_invocations: Vec<VmSemanticMathInvocationSnapshot>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub active_capture: Option<VmExecutedMathCaptureSnapshot>,
}

impl VmSemanticMathSnapshot {
    pub fn is_restorable(&self) -> bool {
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        let scanner_event_ids = self
            .scanner_dollar_event_ids
            .iter()
            .chain(&self.scanner_command_event_ids)
            .copied()
            .collect::<BTreeSet<_>>();
        values_are_unique_nonzero(&self.scanner_dollar_event_ids)
            && values_are_unique_nonzero(&self.scanner_command_event_ids)
            && self
                .scanner_dollar_event_ids
                .iter()
                .all(|event_id| !self.scanner_command_event_ids.contains(event_id))
            && values_are_unique(&self.executed_invocations)
            && values_are_unique_nonzero(&executed_event_ids)
            && executed_event_ids
                .iter()
                .all(|event_id| !scanner_event_ids.contains(event_id))
            && self
                .active_capture
                .as_ref()
                .is_none_or(VmExecutedMathCaptureSnapshot::is_restorable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VmSemanticMathInvocationSnapshot {
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExecutedMathCaptureSnapshot {
    pub display: bool,
    #[serde(default)]
    pub command_delimited: bool,
    #[serde(default)]
    pub environment: Option<String>,
    pub raw_source: String,
    pub source_path: Utf8PathBuf,
    pub invocation_start_utf8: u32,
    pub content_start_utf8: u32,
    #[serde(default)]
    pub semantic_source: Option<SourceProvenance>,
    #[serde(default)]
    pub producer: Option<EventProducer>,
}

impl VmExecutedMathCaptureSnapshot {
    fn is_restorable(&self) -> bool {
        self.invocation_start_utf8 <= self.content_start_utf8
            && self.semantic_source.is_some()
            && matches!(
                self.producer,
                Some(EventProducer::Primitive | EventProducer::Macro)
            )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticTextSnapshot {
    #[serde(default)]
    pub scanner_slots: Vec<VmScannerTextSlotSnapshot>,
    #[serde(default)]
    pub suppressed_ranges: Vec<VmSuppressedSourceRangeSnapshot>,
    #[serde(default)]
    pub forced_execution_ranges: Vec<VmExecutionAuthorityRangeSnapshot>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub executed_event_anchors: Vec<VmEventExecutionAnchorSnapshot>,
    #[serde(default)]
    pub active_capture: Option<VmExecutedTextCaptureSnapshot>,
    #[serde(default)]
    pub paragraph_has_content: bool,
    #[serde(default)]
    pub space_run_active: bool,
    #[serde(default)]
    pub marker_actions: Vec<VmExpansionMarkerSnapshot>,
    #[serde(default)]
    pub expansion_stack: Vec<VmExpansionContextSnapshot>,
    #[serde(default)]
    pub next_marker_id: u64,
}

impl VmSemanticTextSnapshot {
    pub fn is_restorable(&self) -> bool {
        let scanner_event_ids = self
            .scanner_slots
            .iter()
            .flat_map(|slot| slot.event_ids.iter().copied())
            .collect::<Vec<_>>();
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        let marker_names = self
            .marker_actions
            .iter()
            .map(|marker| marker.control_sequence.clone())
            .collect::<Vec<_>>();
        let expansion_ids = self
            .expansion_stack
            .iter()
            .map(|context| context.marker_id)
            .collect::<Vec<_>>();
        let expansion_id_set = expansion_ids.iter().copied().collect::<BTreeSet<_>>();
        let mut action_state = BTreeMap::<u64, (bool, bool)>::new();
        for marker in &self.marker_actions {
            let marker_id = marker.action.marker_id();
            if marker_id >= self.next_marker_id {
                return false;
            }
            let state = action_state.entry(marker_id).or_default();
            match &marker.action {
                VmExpansionMarkerActionSnapshot::Begin { .. } if state.0 => return false,
                VmExpansionMarkerActionSnapshot::Begin { .. } => state.0 = true,
                VmExpansionMarkerActionSnapshot::End { .. } if state.1 => return false,
                VmExpansionMarkerActionSnapshot::End { .. } => state.1 = true,
            }
        }
        let marker_state_is_valid = action_state
            .iter()
            .all(|(marker_id, (has_begin, has_end))| {
                if expansion_id_set.contains(marker_id) {
                    !has_begin && *has_end
                } else {
                    *has_begin && *has_end
                }
            })
            && expansion_ids
                .iter()
                .all(|marker_id| action_state.contains_key(marker_id));
        marker_names.iter().all(|name| !name.is_empty())
            && values_are_unique(&marker_names)
            && values_are_unique(&expansion_ids)
            && expansion_ids
                .iter()
                .all(|marker_id| *marker_id < self.next_marker_id)
            && marker_state_is_valid
            && self
                .scanner_slots
                .iter()
                .all(VmScannerTextSlotSnapshot::is_restorable)
            && values_are_unique_nonzero(&scanner_event_ids)
            && self
                .suppressed_ranges
                .iter()
                .all(VmSuppressedSourceRangeSnapshot::is_restorable)
            && self
                .forced_execution_ranges
                .iter()
                .all(VmExecutionAuthorityRangeSnapshot::is_restorable)
            && values_are_unique_nonzero(&executed_event_ids)
            && event_execution_anchors_are_restorable(
                &self.executed_event_anchors,
                &executed_event_ids,
            )
            && self
                .active_capture
                .as_ref()
                .is_none_or(VmExecutedTextCaptureSnapshot::is_restorable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExpansionMarkerSnapshot {
    pub control_sequence: String,
    pub action: VmExpansionMarkerActionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VmExpansionMarkerActionSnapshot {
    Begin { context: VmExpansionContextSnapshot },
    End { marker_id: u64 },
}

impl VmExpansionMarkerActionSnapshot {
    fn marker_id(&self) -> u64 {
        match self {
            Self::Begin { context } => context.marker_id,
            Self::End { marker_id } => *marker_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExpansionContextSnapshot {
    pub marker_id: u64,
    pub source: SourceProvenance,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticInlineSnapshot {
    #[serde(default)]
    pub scanner_citation_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub scanner_reference_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub scanner_link_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub scanner_label_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub executed_citations: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub executed_references: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub executed_links: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub executed_labels: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub overridden_label_invocations: Vec<VmSuppressedSourceRangeSnapshot>,
    #[serde(default)]
    pub caption_placeholders: Vec<CaptionInlinePlaceholderEvent>,
    #[serde(default)]
    pub active_link_actions: Vec<VmActiveLinkCaptureSnapshot>,
    #[serde(default)]
    pub next_link_marker_id: u64,
}

impl VmSemanticInlineSnapshot {
    pub fn is_restorable(&self, text_event_count: usize, math_event_count: usize) -> bool {
        let text_event_count = u64::try_from(text_event_count).unwrap_or(u64::MAX);
        let math_event_count = u64::try_from(math_event_count).unwrap_or(u64::MAX);
        let citation_event_count = u64::try_from(self.executed_citations.len()).unwrap_or(u64::MAX);
        let reference_event_count =
            u64::try_from(self.executed_references.len()).unwrap_or(u64::MAX);
        let link_event_count = u64::try_from(self.executed_links.len()).unwrap_or(u64::MAX);
        let scanner_event_ids = self
            .scanner_citation_event_ids
            .iter()
            .chain(&self.scanner_reference_event_ids)
            .chain(&self.scanner_link_event_ids)
            .chain(&self.scanner_label_event_ids)
            .copied()
            .collect::<Vec<_>>();
        let executed_event_ids = self
            .executed_citations
            .iter()
            .chain(&self.executed_references)
            .chain(&self.executed_links)
            .chain(&self.executed_labels)
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        let executed_event_id_set = executed_event_ids.iter().copied().collect::<BTreeSet<_>>();
        let marker_names = self
            .active_link_actions
            .iter()
            .map(|capture| capture.control_sequence.clone())
            .collect::<Vec<_>>();
        let marker_ids = self
            .active_link_actions
            .iter()
            .map(|capture| capture.marker_id)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&scanner_event_ids)
            && values_are_unique_nonzero(&executed_event_ids)
            && scanner_event_ids
                .iter()
                .all(|event_id| !executed_event_id_set.contains(event_id))
            && marker_names.iter().all(|name| !name.is_empty())
            && values_are_unique(&marker_names)
            && values_are_unique(&marker_ids)
            && marker_ids
                .iter()
                .all(|marker_id| *marker_id < self.next_link_marker_id)
            && self
                .overridden_label_invocations
                .iter()
                .all(VmSuppressedSourceRangeSnapshot::is_restorable)
            && self.active_link_actions.iter().all(|capture| {
                capture.text_event_mark <= text_event_count
                    && capture.citation_event_mark <= citation_event_count
                    && capture.reference_event_mark <= reference_event_count
                    && capture.link_event_mark <= link_event_count
                    && capture.math_event_mark <= math_event_count
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmActiveLinkCaptureSnapshot {
    pub control_sequence: String,
    pub marker_id: u64,
    pub command: String,
    pub target: String,
    pub source: SourceProvenance,
    pub producer: EventProducer,
    pub visible_output_prefix: String,
    pub text_event_mark: u64,
    pub citation_event_mark: u64,
    pub reference_event_mark: u64,
    pub link_event_mark: u64,
    pub math_event_mark: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExecutedInlineEventMarkSnapshot {
    pub citations: u64,
    pub references: u64,
    pub links: u64,
    #[serde(default)]
    pub labels: u64,
    pub caption_placeholders: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExecutedTextFlowMarkSnapshot {
    pub event_mark: u64,
    pub paragraph_has_content: bool,
    pub space_run_active: bool,
}

impl VmExecutedInlineEventMarkSnapshot {
    fn is_restorable(&self, inline: &VmSemanticInlineSnapshot) -> bool {
        self.citations <= u64::try_from(inline.executed_citations.len()).unwrap_or(u64::MAX)
            && self.references
                <= u64::try_from(inline.executed_references.len()).unwrap_or(u64::MAX)
            && self.links <= u64::try_from(inline.executed_links.len()).unwrap_or(u64::MAX)
            && self.labels <= u64::try_from(inline.executed_labels.len()).unwrap_or(u64::MAX)
            && self.caption_placeholders
                <= u64::try_from(inline.caption_placeholders.len()).unwrap_or(u64::MAX)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticHeadingSnapshot {
    #[serde(default)]
    pub scanner_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub active_heading_actions: Vec<VmActiveHeadingCaptureSnapshot>,
    #[serde(default)]
    pub next_marker_id: u64,
}

impl VmSemanticHeadingSnapshot {
    pub fn is_restorable(
        &self,
        text_event_count: usize,
        math_event_count: usize,
        inline: &VmSemanticInlineSnapshot,
    ) -> bool {
        let text_event_count = u64::try_from(text_event_count).unwrap_or(u64::MAX);
        let math_event_count = u64::try_from(math_event_count).unwrap_or(u64::MAX);
        let scanner_event_ids = self
            .scanner_event_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        let marker_names = self
            .active_heading_actions
            .iter()
            .map(|capture| capture.control_sequence.clone())
            .collect::<Vec<_>>();
        let marker_ids = self
            .active_heading_actions
            .iter()
            .map(|capture| capture.marker_id)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_event_ids)
            && values_are_unique_nonzero(&executed_event_ids)
            && executed_event_ids
                .iter()
                .all(|event_id| !scanner_event_ids.contains(event_id))
            && marker_names.iter().all(|name| !name.is_empty())
            && values_are_unique(&marker_names)
            && values_are_unique(&marker_ids)
            && marker_ids
                .iter()
                .all(|marker_id| *marker_id < self.next_marker_id)
            && self.active_heading_actions.iter().all(|capture| {
                executed_capture_producer_is_restorable(capture.producer)
                    && capture.text_event_mark <= text_event_count
                    && capture.inline_event_mark.is_restorable(inline)
                    && capture.math_event_mark <= math_event_count
                    && capture.heading_event_mark
                        <= u64::try_from(self.executed_events.len()).unwrap_or(u64::MAX)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmActiveHeadingCaptureSnapshot {
    pub control_sequence: String,
    pub marker_id: u64,
    pub level: u8,
    pub numbered: bool,
    pub source: SourceProvenance,
    pub producer: EventProducer,
    pub visible_output_prefix: String,
    pub lossy_before_restore: bool,
    pub text_event_mark: u64,
    pub inline_event_mark: VmExecutedInlineEventMarkSnapshot,
    pub math_event_mark: u64,
    pub heading_event_mark: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticCaptionSnapshot {
    #[serde(default)]
    pub scanner_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub active_caption_actions: Vec<VmActiveCaptionCaptureSnapshot>,
    #[serde(default)]
    pub next_marker_id: u64,
}

impl VmSemanticCaptionSnapshot {
    pub fn is_restorable(
        &self,
        text_event_count: usize,
        math_event_count: usize,
        inline: &VmSemanticInlineSnapshot,
    ) -> bool {
        let text_event_count = u64::try_from(text_event_count).unwrap_or(u64::MAX);
        let math_event_count = u64::try_from(math_event_count).unwrap_or(u64::MAX);
        let scanner_event_ids = self
            .scanner_event_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        let marker_names = self
            .active_caption_actions
            .iter()
            .map(|capture| capture.control_sequence.clone())
            .collect::<Vec<_>>();
        let marker_ids = self
            .active_caption_actions
            .iter()
            .map(|capture| capture.marker_id)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_event_ids)
            && values_are_unique_nonzero(&executed_event_ids)
            && executed_event_ids
                .iter()
                .all(|event_id| !scanner_event_ids.contains(event_id))
            && marker_names.iter().all(|name| !name.is_empty())
            && values_are_unique(&marker_names)
            && values_are_unique(&marker_ids)
            && marker_ids
                .iter()
                .all(|marker_id| *marker_id < self.next_marker_id)
            && self.active_caption_actions.iter().all(|capture| {
                executed_capture_producer_is_restorable(capture.producer)
                    && capture.text_event_mark <= text_event_count
                    && capture.inline_event_mark.is_restorable(inline)
                    && capture.math_event_mark <= math_event_count
                    && capture.caption_event_mark
                        <= u64::try_from(self.executed_events.len()).unwrap_or(u64::MAX)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmActiveCaptionCaptureSnapshot {
    pub control_sequence: String,
    pub marker_id: u64,
    pub numbered: bool,
    pub caption_kind: Option<CaptionKind>,
    pub source: SourceProvenance,
    pub producer: EventProducer,
    pub visible_output_prefix: String,
    pub lossy_before_restore: bool,
    pub text_event_mark: u64,
    pub inline_event_mark: VmExecutedInlineEventMarkSnapshot,
    pub math_event_mark: u64,
    pub caption_event_mark: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticBibliographySnapshot {
    #[serde(default)]
    pub scanner_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub scanner_input_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub scanner_event_anchors: Vec<VmEventExecutionAnchorSnapshot>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub executed_event_anchors: Vec<VmEventExecutionAnchorSnapshot>,
    #[serde(default)]
    pub executed_invocations: Vec<VmSuppressedSourceRangeSnapshot>,
    #[serde(default)]
    pub environment_depth: u64,
    #[serde(default)]
    pub active_item: Option<VmActiveBibliographyCaptureSnapshot>,
}

impl VmSemanticBibliographySnapshot {
    pub fn is_restorable(
        &self,
        text_event_count: usize,
        math_event_count: usize,
        inline: &VmSemanticInlineSnapshot,
    ) -> bool {
        let text_event_count = u64::try_from(text_event_count).unwrap_or(u64::MAX);
        let math_event_count = u64::try_from(math_event_count).unwrap_or(u64::MAX);
        let scanner_event_ids = self
            .scanner_event_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_event_ids)
            && values_are_unique_nonzero(&self.scanner_input_event_ids)
            && event_execution_anchors_are_restorable(
                &self.scanner_event_anchors,
                &self.scanner_event_ids,
            )
            && values_are_unique_nonzero(&executed_event_ids)
            && event_execution_anchors_are_restorable(
                &self.executed_event_anchors,
                &executed_event_ids,
            )
            && executed_event_ids
                .iter()
                .all(|event_id| !scanner_event_ids.contains(event_id))
            && self
                .executed_invocations
                .iter()
                .all(VmSuppressedSourceRangeSnapshot::is_restorable)
            && usize::try_from(self.environment_depth).is_ok()
            && self.active_item.as_ref().is_none_or(|capture| {
                self.environment_depth > 0
                    && executed_capture_producer_is_restorable(capture.producer)
                    && capture.execution_anchor.is_restorable()
                    && capture.text_event_mark <= text_event_count
                    && capture.inline_event_mark.is_restorable(inline)
                    && capture.math_event_mark <= math_event_count
                    && capture.nested_semantics.is_restorable(
                        text_event_count,
                        math_event_count,
                        inline,
                    )
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmActiveBibliographyCaptureSnapshot {
    pub key: String,
    pub label_hint: Option<String>,
    pub source: SourceProvenance,
    pub producer: EventProducer,
    #[serde(default)]
    pub execution_anchor: VmExecutionAnchor,
    pub visible_output_prefix: String,
    pub lossy_before_restore: bool,
    pub text_event_mark: u64,
    pub inline_event_mark: VmExecutedInlineEventMarkSnapshot,
    pub math_event_mark: u64,
    pub nested_semantics: VmBibliographyNestedSemanticSnapshot,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmBibliographyNestedSemanticSnapshot {
    pub caption: VmSemanticCaptionSnapshot,
    pub environment: VmSemanticEnvironmentSnapshot,
    pub footnote: VmSemanticFootnoteSnapshot,
    pub front_matter: VmSemanticFrontMatterSnapshot,
    pub graphic: VmSemanticGraphicSnapshot,
    pub heading: VmSemanticHeadingSnapshot,
    pub list: VmSemanticListSnapshot,
    pub table: VmSemanticTableSnapshot,
}

impl VmBibliographyNestedSemanticSnapshot {
    fn is_restorable(
        &self,
        text_event_count: u64,
        math_event_count: u64,
        inline: &VmSemanticInlineSnapshot,
    ) -> bool {
        let text_event_count = usize::try_from(text_event_count).unwrap_or(usize::MAX);
        let math_event_count = usize::try_from(math_event_count).unwrap_or(usize::MAX);
        self.caption
            .is_restorable(text_event_count, math_event_count, inline)
            && self.environment.is_restorable()
            && self
                .footnote
                .is_restorable(text_event_count, math_event_count, inline)
            && self.front_matter.is_restorable()
            && self.graphic.is_restorable()
            && self
                .heading
                .is_restorable(text_event_count, math_event_count, inline)
            && self.list.is_restorable()
            && self.table.is_restorable()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticFootnoteSnapshot {
    #[serde(default)]
    pub scanner_slots: Vec<VmScannerFootnoteSlotSnapshot>,
    #[serde(default)]
    pub completed_transactions: Vec<Vec<RenderEventEnvelope>>,
    #[serde(default)]
    pub active_actions: Vec<VmActiveFootnoteCaptureSnapshot>,
    #[serde(default)]
    pub next_marker_id: u64,
    #[serde(default = "default_next_footnote_id")]
    pub next_note_id: FootnoteId,
    #[serde(default)]
    pub pending_mark: Option<VmPendingFootnoteMarkSnapshot>,
}

impl VmSemanticFootnoteSnapshot {
    pub fn is_restorable(
        &self,
        text_event_count: usize,
        math_event_count: usize,
        inline: &VmSemanticInlineSnapshot,
    ) -> bool {
        let text_event_count = u64::try_from(text_event_count).unwrap_or(u64::MAX);
        let math_event_count = u64::try_from(math_event_count).unwrap_or(u64::MAX);
        let scanner_event_ids = self
            .scanner_slots
            .iter()
            .flat_map(|slot| slot.event_ids.iter().copied())
            .collect::<Vec<_>>();
        let completed_event_ids = self
            .completed_transactions
            .iter()
            .flatten()
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        let active_event_ids = self
            .active_actions
            .iter()
            .map(|capture| capture.begin_event.meta.sequence)
            .collect::<Vec<_>>();
        let mut executed_event_ids = completed_event_ids;
        executed_event_ids.extend(active_event_ids);
        let marker_names = self
            .active_actions
            .iter()
            .map(|capture| capture.control_sequence.clone())
            .collect::<Vec<_>>();
        let marker_ids = self
            .active_actions
            .iter()
            .map(|capture| capture.marker_id)
            .collect::<Vec<_>>();
        let note_ids = self
            .completed_transactions
            .iter()
            .flatten()
            .filter_map(|event| match &event.event {
                RenderEvent::BeginFootnote(begin) => Some(begin.note_id),
                RenderEvent::EndFootnote(end) => Some(end.note_id),
                RenderEvent::FootnoteMark(mark) => Some(mark.note_id),
                _ => None,
            })
            .chain(self.active_actions.iter().filter_map(
                |capture| match &capture.begin_event.event {
                    RenderEvent::BeginFootnote(begin) => Some(begin.note_id),
                    _ => None,
                },
            ))
            .chain(self.pending_mark.iter().map(|pending| pending.note_id))
            .collect::<Vec<_>>();
        let pending_mark_is_valid = self.pending_mark.as_ref().is_none_or(|pending| {
            let mut matching_marks =
                self.completed_transactions
                    .iter()
                    .flatten()
                    .filter_map(|event| match &event.event {
                        RenderEvent::FootnoteMark(mark) if mark.note_id == pending.note_id => {
                            Some(mark)
                        }
                        _ => None,
                    });
            pending.note_id != 0
                && matching_marks
                    .next()
                    .is_some_and(|mark| mark.marker == pending.marker)
                && matching_marks.next().is_none()
        });

        self.scanner_slots
            .iter()
            .all(VmScannerFootnoteSlotSnapshot::is_restorable)
            && values_are_unique_nonzero(&scanner_event_ids)
            && self
                .completed_transactions
                .iter()
                .all(|transaction| !transaction.is_empty())
            && values_are_unique_nonzero(&executed_event_ids)
            && marker_names.iter().all(|name| !name.is_empty())
            && values_are_unique(&marker_names)
            && values_are_unique(&marker_ids)
            && marker_ids
                .iter()
                .all(|marker_id| *marker_id < self.next_marker_id)
            && self.next_note_id >= 1
            && note_ids
                .iter()
                .all(|note_id| *note_id >= 1 && *note_id < self.next_note_id)
            && pending_mark_is_valid
            && self.active_actions.iter().all(|capture| {
                executed_capture_producer_is_restorable(capture.begin_event.meta.producer)
                    && capture.text_flow_mark.event_mark <= text_event_count
                    && capture.inline_event_mark.is_restorable(inline)
                    && capture.math_event_mark <= math_event_count
                    && capture.transaction_mark
                        <= u64::try_from(self.completed_transactions.len()).unwrap_or(u64::MAX)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmPendingFootnoteMarkSnapshot {
    pub note_id: FootnoteId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmScannerFootnoteSlotSnapshot {
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
    pub event_ids: Vec<EventSequence>,
}

impl VmScannerFootnoteSlotSnapshot {
    fn is_restorable(&self) -> bool {
        self.start_utf8 <= self.end_utf8 && values_are_unique_nonzero(&self.event_ids)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmActiveFootnoteCaptureSnapshot {
    pub control_sequence: String,
    pub marker_id: u64,
    pub begin_event: RenderEventEnvelope,
    pub text_flow_mark: VmExecutedTextFlowMarkSnapshot,
    pub inline_event_mark: VmExecutedInlineEventMarkSnapshot,
    pub math_event_mark: u64,
    pub transaction_mark: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmScannerTextSlotSnapshot {
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
    pub event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub execution_anchor: VmExecutionAnchor,
    #[serde(default)]
    pub preserve_leading_space: bool,
}

impl VmScannerTextSlotSnapshot {
    fn is_restorable(&self) -> bool {
        self.start_utf8 <= self.end_utf8
            && values_are_unique_nonzero(&self.event_ids)
            && self.execution_anchor.is_restorable()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExecutionAuthorityRangeSnapshot {
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
    #[serde(default)]
    pub execution_anchor: VmExecutionAnchor,
}

impl VmExecutionAuthorityRangeSnapshot {
    fn is_restorable(&self) -> bool {
        self.start_utf8 <= self.end_utf8
            && self.path == self.execution_anchor.path
            && self.execution_anchor.is_restorable()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSuppressedSourceRangeSnapshot {
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
    #[serde(default)]
    pub execution_anchor: VmExecutionAnchor,
}

impl VmSuppressedSourceRangeSnapshot {
    fn is_restorable(&self) -> bool {
        self.start_utf8 <= self.end_utf8
            && self.path == self.execution_anchor.path
            && self.execution_anchor.is_restorable()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExecutedTextCaptureSnapshot {
    pub text: String,
    pub source: SourceProvenance,
    pub producer: EventProducer,
    pub literal_path: Option<Utf8PathBuf>,
    pub end_utf8: u32,
    #[serde(default)]
    pub execution_anchor: VmExecutionAnchor,
}

impl VmExecutedTextCaptureSnapshot {
    fn is_restorable(&self) -> bool {
        let literal_span = self.literal_path.as_ref().and_then(|literal_path| {
            std::iter::once((None, &self.source.primary))
                .chain(
                    self.source
                        .related
                        .iter()
                        .map(|related| (Some(related.role), &related.span)),
                )
                .find_map(|(role, span)| match span {
                    ProvenanceSpan::File(span)
                        if &span.path == literal_path
                            && role.is_none_or(|role| role == SourceSpanRole::EmitSite) =>
                    {
                        Some(span)
                    }
                    ProvenanceSpan::File(_) | ProvenanceSpan::Generated(_) => None,
                })
        });
        executed_capture_producer_is_restorable(self.producer)
            && self.execution_anchor.is_restorable()
            && self.literal_path.as_ref().is_none_or(|_| {
                literal_span.is_some_and(|span| {
                    span.start_utf8 <= span.end_utf8 && span.end_utf8 == self.end_utf8
                })
            })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticGraphicSnapshot {
    #[serde(default)]
    pub scanner_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub overridden_invocations: Vec<VmGraphicInvocationRangeSnapshot>,
    #[serde(default)]
    pub scanner_default_options: Vec<VmScannerGraphicDefaultOptionsSnapshot>,
}

impl VmSemanticGraphicSnapshot {
    pub fn is_restorable(&self) -> bool {
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_event_ids)
            && values_are_unique_nonzero(&executed_event_ids)
            && self
                .overridden_invocations
                .iter()
                .all(VmGraphicInvocationRangeSnapshot::is_restorable)
            && self
                .scanner_default_options
                .iter()
                .all(VmScannerGraphicDefaultOptionsSnapshot::is_restorable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmGraphicInvocationRangeSnapshot {
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
}

impl VmGraphicInvocationRangeSnapshot {
    fn is_restorable(&self) -> bool {
        self.start_utf8 <= self.end_utf8
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmScannerGraphicDefaultOptionsSnapshot {
    pub options: String,
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
    pub first_event_sequence: EventSequence,
    #[serde(default)]
    pub execution_anchor: VmExecutionAnchor,
}

impl VmScannerGraphicDefaultOptionsSnapshot {
    fn is_restorable(&self) -> bool {
        !self.options.trim().is_empty()
            && self.start_utf8 <= self.end_utf8
            && self.first_event_sequence != 0
            && self.path == self.execution_anchor.path
            && self.execution_anchor.is_restorable()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticListSnapshot {
    #[serde(default)]
    pub scanner_item_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub executed_items: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub active_lists: Vec<ListKind>,
}

impl VmSemanticListSnapshot {
    pub fn is_restorable(&self) -> bool {
        let executed_event_ids = self
            .executed_items
            .iter()
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_item_event_ids)
            && values_are_unique_nonzero(&executed_event_ids)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticEnvironmentSnapshot {
    #[serde(default)]
    pub scanner_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub scanner_column_layout_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
    #[serde(default)]
    pub included_authorities: Vec<VmIncludedEnvironmentAuthoritySnapshot>,
}

impl VmSemanticEnvironmentSnapshot {
    pub fn is_restorable(&self) -> bool {
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_event_ids)
            && values_are_unique_nonzero(&self.scanner_column_layout_event_ids)
            && self
                .scanner_column_layout_event_ids
                .iter()
                .all(|event_id| self.scanner_event_ids.contains(event_id))
            && values_are_unique_nonzero(&executed_event_ids)
            && self
                .included_authorities
                .iter()
                .all(VmIncludedEnvironmentAuthoritySnapshot::is_restorable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmIncludedEnvironmentAuthoritySnapshot {
    pub environment: String,
    pub path: Utf8PathBuf,
    pub start_utf8: u32,
    #[serde(default)]
    pub execution_anchor: VmExecutionAnchor,
}

impl VmIncludedEnvironmentAuthoritySnapshot {
    fn is_restorable(&self) -> bool {
        !self.environment.is_empty()
            && self.path == self.execution_anchor.path
            && self.execution_anchor.is_restorable()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticFrontMatterSnapshot {
    #[serde(default)]
    pub scanner_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub executed_events: Vec<RenderEventEnvelope>,
}

impl VmSemanticFrontMatterSnapshot {
    pub fn is_restorable(&self) -> bool {
        let executed_event_ids = self
            .executed_events
            .iter()
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_event_ids)
            && values_are_unique_nonzero(&executed_event_ids)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSemanticTableSnapshot {
    #[serde(default)]
    pub scanner_event_ids: Vec<EventSequence>,
    #[serde(default)]
    pub open_tables: Vec<VmExecutedTableFrameSnapshot>,
    #[serde(default)]
    pub executed_tables: Vec<VmExecutedTableSnapshot>,
    #[serde(default)]
    pub structured_events: bool,
}

impl VmSemanticTableSnapshot {
    pub fn is_restorable(&self) -> bool {
        let native_event_ids = self
            .executed_tables
            .iter()
            .filter_map(|table| table.native_event.as_ref())
            .map(|event| event.meta.sequence)
            .collect::<Vec<_>>();
        values_are_unique_nonzero(&self.scanner_event_ids)
            && values_are_unique_nonzero(&native_event_ids)
            && self
                .open_tables
                .iter()
                .all(|frame| executed_capture_producer_is_restorable(frame.producer))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExecutedTableFrameSnapshot {
    pub environment: String,
    pub source: SourceProvenance,
    pub producer: EventProducer,
    pub width_spec: Option<String>,
    pub columns: Vec<TableColumnSpec>,
    pub rows: Vec<TableRowEvent>,
    pub current_cells: Vec<TableCellEvent>,
    pub current_text: String,
    pub current_source: Option<SourceProvenance>,
    pub row_source: Option<SourceProvenance>,
    pub row_started: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExecutedTableSnapshot {
    pub environment: String,
    pub source: SourceProvenance,
    pub native_event: Option<RenderEventEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyVmSnapshotV1 {
    #[serde(default)]
    pub continuation_safety: VmContinuationSafety,
    #[serde(default)]
    pub input_continuation: Option<VmInputContinuationSnapshot>,
    #[serde(default)]
    pub jobname_source_path: Option<Utf8PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_sink: Option<VmSemanticSinkSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_capture: Option<VmSemanticCaptureSnapshot>,
    #[serde(default)]
    pub diagnostics: Vec<VmDiagnostic>,
    #[serde(default)]
    pub transcript: Vec<String>,
    #[serde(default)]
    pub module_traces: Vec<VmModuleTrace>,
    #[serde(default)]
    pub module_boundaries: Vec<VmModuleBoundary>,
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
    #[serde(default = "default_hidden_environments")]
    pub hidden_environments: Vec<String>,
    #[serde(default)]
    pub included_comment_environments: Vec<String>,
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
    pub graphic_paths: Vec<Utf8PathBuf>,
    #[serde(default)]
    pub graphic_extensions: Vec<String>,
    #[serde(default)]
    pub graphic_default_options: Option<String>,
    #[serde(default)]
    pub epsf_pending_options: Option<String>,
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
    #[serde(default)]
    pub text_script_wrapper_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmCodeTableAssignmentV1 {
    pub character: u8,
    pub value: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmCodeTableStateV1 {
    pub layers: Vec<Vec<VmCodeTableAssignmentV1>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntegerParameterId {
    Tolerance,
}

impl IntegerParameterId {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Tolerance => "tolerance",
        }
    }

    pub(crate) const fn default_value(self) -> i32 {
        match self {
            Self::Tolerance => 10_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmIntegerParameterAssignmentV1 {
    pub parameter: IntegerParameterId,
    pub value: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmIntegerParameterStateV1 {
    pub layers: Vec<Vec<VmIntegerParameterAssignmentV1>>,
}

impl VmIntegerParameterStateV1 {
    pub(crate) fn validate(&self, expected_layers: usize) -> Result<(), String> {
        if self.layers.len() != expected_layers {
            return Err(format!(
                "integer-parameter layer count {} does not match VM scope depth {expected_layers}",
                self.layers.len()
            ));
        }
        if !self.layers.iter().any(|layer| !layer.is_empty()) {
            return Err("integer-parameter state must not be empty".to_string());
        }
        if self.layers[0]
            .iter()
            .any(|assignment| assignment.value == assignment.parameter.default_value())
        {
            return Err(
                "integer-parameter root assignments equal to their defaults must be omitted"
                    .to_string(),
            );
        }
        for (layer_index, layer) in self.layers.iter().enumerate() {
            let mut previous = None;
            for assignment in layer {
                if previous.is_some_and(|parameter| parameter >= assignment.parameter) {
                    return Err(format!(
                        "integer-parameter layer {layer_index} entries must be strictly increasing"
                    ));
                }
                previous = Some(assignment.parameter);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutIntegerParameterId {
    AdjDemerits,
    BinOpPenalty,
    BrokenPenalty,
    ClubPenalty,
    DisplayWidowPenalty,
    DoubleHyphenDemerits,
    ExHyphenPenalty,
    FinalHyphenDemerits,
    HangAfter,
    HyphenPenalty,
    InterlinePenalty,
    LinePenalty,
    Looseness,
    PostDisplayPenalty,
    PreDisplayPenalty,
    PreTolerance,
    RelPenalty,
    WidowPenalty,
}

impl LayoutIntegerParameterId {
    pub(crate) const ALL: [Self; 18] = [
        Self::AdjDemerits,
        Self::BinOpPenalty,
        Self::BrokenPenalty,
        Self::ClubPenalty,
        Self::DisplayWidowPenalty,
        Self::DoubleHyphenDemerits,
        Self::ExHyphenPenalty,
        Self::FinalHyphenDemerits,
        Self::HangAfter,
        Self::HyphenPenalty,
        Self::InterlinePenalty,
        Self::LinePenalty,
        Self::Looseness,
        Self::PostDisplayPenalty,
        Self::PreDisplayPenalty,
        Self::PreTolerance,
        Self::RelPenalty,
        Self::WidowPenalty,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AdjDemerits => "adjdemerits",
            Self::BinOpPenalty => "binoppenalty",
            Self::BrokenPenalty => "brokenpenalty",
            Self::ClubPenalty => "clubpenalty",
            Self::DisplayWidowPenalty => "displaywidowpenalty",
            Self::DoubleHyphenDemerits => "doublehyphendemerits",
            Self::ExHyphenPenalty => "exhyphenpenalty",
            Self::FinalHyphenDemerits => "finalhyphendemerits",
            Self::HangAfter => "hangafter",
            Self::HyphenPenalty => "hyphenpenalty",
            Self::InterlinePenalty => "interlinepenalty",
            Self::LinePenalty => "linepenalty",
            Self::Looseness => "looseness",
            Self::PostDisplayPenalty => "postdisplaypenalty",
            Self::PreDisplayPenalty => "predisplaypenalty",
            Self::PreTolerance => "pretolerance",
            Self::RelPenalty => "relpenalty",
            Self::WidowPenalty => "widowpenalty",
        }
    }

    pub(crate) fn from_source_name(name: &str) -> Option<Self> {
        match name {
            "adjdemerits" => Some(Self::AdjDemerits),
            "binoppenalty" => Some(Self::BinOpPenalty),
            "brokenpenalty" => Some(Self::BrokenPenalty),
            "clubpenalty" => Some(Self::ClubPenalty),
            "displaywidowpenalty" => Some(Self::DisplayWidowPenalty),
            "doublehyphendemerits" => Some(Self::DoubleHyphenDemerits),
            "exhyphenpenalty" => Some(Self::ExHyphenPenalty),
            "finalhyphendemerits" => Some(Self::FinalHyphenDemerits),
            "hangafter" => Some(Self::HangAfter),
            "hyphenpenalty" => Some(Self::HyphenPenalty),
            "interlinepenalty" => Some(Self::InterlinePenalty),
            "linepenalty" => Some(Self::LinePenalty),
            "looseness" => Some(Self::Looseness),
            "postdisplaypenalty" => Some(Self::PostDisplayPenalty),
            "predisplaypenalty" => Some(Self::PreDisplayPenalty),
            "pretolerance" => Some(Self::PreTolerance),
            "relpenalty" => Some(Self::RelPenalty),
            "widowpenalty" => Some(Self::WidowPenalty),
            _ => None,
        }
    }

    pub(crate) const fn default_value(self) -> i32 {
        match self {
            Self::HangAfter => 1,
            Self::AdjDemerits
            | Self::BinOpPenalty
            | Self::BrokenPenalty
            | Self::ClubPenalty
            | Self::DisplayWidowPenalty
            | Self::DoubleHyphenDemerits
            | Self::ExHyphenPenalty
            | Self::FinalHyphenDemerits
            | Self::HyphenPenalty
            | Self::InterlinePenalty
            | Self::LinePenalty
            | Self::Looseness
            | Self::PostDisplayPenalty
            | Self::PreDisplayPenalty
            | Self::PreTolerance
            | Self::RelPenalty
            | Self::WidowPenalty => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmLayoutIntegerParameterAssignmentV1 {
    pub parameter: LayoutIntegerParameterId,
    pub value: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmLayoutIntegerParameterStateV1 {
    pub layers: Vec<Vec<VmLayoutIntegerParameterAssignmentV1>>,
}

impl VmLayoutIntegerParameterStateV1 {
    pub(crate) fn validate(&self, expected_layers: usize) -> Result<(), String> {
        if self.layers.len() != expected_layers {
            return Err(format!(
                "layout-integer-parameter layer count {} does not match VM scope depth {expected_layers}",
                self.layers.len()
            ));
        }
        if !self.layers.iter().any(|layer| !layer.is_empty()) {
            return Err("layout-integer-parameter state must not be empty".to_string());
        }
        if self.layers[0]
            .iter()
            .any(|assignment| assignment.value == assignment.parameter.default_value())
        {
            return Err(
                "layout-integer-parameter root assignments equal to their defaults must be omitted"
                    .to_string(),
            );
        }
        for (layer_index, layer) in self.layers.iter().enumerate() {
            let mut previous = None;
            for assignment in layer {
                if previous.is_some_and(|parameter| parameter >= assignment.parameter) {
                    return Err(format!(
                        "layout-integer-parameter layer {layer_index} entries must be strictly increasing"
                    ));
                }
                previous = Some(assignment.parameter);
            }
        }
        Ok(())
    }
}

impl VmCodeTableStateV1 {
    pub(crate) fn validate_mathcode(&self, expected_layers: usize) -> Result<(), String> {
        self.validate_with(expected_layers, "mathcode", |value| {
            (0..=32_768).contains(&value)
        })
    }

    pub(crate) fn validate_delcode(&self, expected_layers: usize) -> Result<(), String> {
        self.validate_with(expected_layers, "delcode", |value| {
            (-2_147_483_647..=16_777_215).contains(&value)
        })
    }

    fn validate_with(
        &self,
        expected_layers: usize,
        family: &str,
        value_is_valid: impl Fn(i32) -> bool,
    ) -> Result<(), String> {
        if self.layers.len() != expected_layers {
            return Err(format!(
                "{family} layer count {} does not match VM scope depth {expected_layers}",
                self.layers.len()
            ));
        }
        if !self.layers.iter().any(|layer| !layer.is_empty()) {
            return Err(format!("{family} state must not be empty"));
        }
        for (layer_index, layer) in self.layers.iter().enumerate() {
            let mut previous = None;
            for assignment in layer {
                if previous.is_some_and(|character| character >= assignment.character) {
                    return Err(format!(
                        "{family} layer {layer_index} character entries must be strictly increasing"
                    ));
                }
                if !value_is_valid(assignment.value) {
                    return Err(format!(
                        "{family} value {} for character {} is outside the V1 domain",
                        assignment.value, assignment.character
                    ));
                }
                previous = Some(assignment.character);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmSnapshot {
    legacy: LegacyVmSnapshotV1,
    pub muskip_registers: BTreeMap<u32, i32>,
    pub next_muskip_register: u32,
    pub mathcode_state: Option<VmCodeTableStateV1>,
    pub delcode_state: Option<VmCodeTableStateV1>,
    pub integer_parameter_state: Option<VmIntegerParameterStateV1>,
    pub layout_integer_parameter_state: Option<VmLayoutIntegerParameterStateV1>,
    pub dimension_parameter_state: Option<VmDimensionParameterStateV1>,
}

impl VmSnapshot {
    pub(crate) fn from_parts(
        legacy: LegacyVmSnapshotV1,
        muskip_registers: BTreeMap<u32, i32>,
        next_muskip_register: u32,
        mathcode_state: Option<VmCodeTableStateV1>,
        delcode_state: Option<VmCodeTableStateV1>,
        integer_parameter_state: Option<VmIntegerParameterStateV1>,
        layout_integer_parameter_state: Option<VmLayoutIntegerParameterStateV1>,
        dimension_parameter_state: Option<VmDimensionParameterStateV1>,
    ) -> Self {
        Self {
            legacy,
            muskip_registers,
            next_muskip_register,
            mathcode_state,
            delcode_state,
            integer_parameter_state,
            layout_integer_parameter_state,
            dimension_parameter_state,
        }
    }

    pub fn required_capabilities(&self) -> BTreeSet<SnapshotCapability> {
        let mut capabilities = BTreeSet::new();
        if !self.muskip_registers.is_empty()
            || self.next_muskip_register != default_next_muskip_register()
        {
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_MUSKIP_SCALAR_V1_CAPABILITY,
            ));
        }
        if self.mathcode_state.is_some() {
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY,
            ));
        }
        if self.delcode_state.is_some() {
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY,
            ));
        }
        if self.integer_parameter_state.is_some() {
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
            ));
        }
        if self.layout_integer_parameter_state.is_some() {
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
            ));
        }
        if self.dimension_parameter_state.is_some() {
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY,
            ));
        }
        if self.dimension_parameter_command_v1().is_some() {
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY,
            ));
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY,
            ));
        }
        let token_requires_muskip_alias = |token: &SnapshotToken| {
            matches!(
                &token.kind,
                SnapshotTokenKind::ControlSequence { name }
                    if matches!(
                        name.as_str(),
                        "muskip"
                            | "newmuskip"
                            | "muskipdef"
                            | "csname"
                            | "ifcsname"
                            | "@nameuse"
                    )
            )
        };
        let tokens_require_muskip_alias =
            |tokens: &[SnapshotToken]| tokens.iter().any(&token_requires_muskip_alias);
        let token_requires_mathcode = |token: &SnapshotToken| {
            matches!(
                &token.kind,
                SnapshotTokenKind::ControlSequence { name }
                    if matches!(name.as_str(), "mathcode" | "csname" | "ifcsname" | "@nameuse")
            )
        };
        let tokens_require_mathcode =
            |tokens: &[SnapshotToken]| tokens.iter().any(&token_requires_mathcode);
        let token_requires_delcode = |token: &SnapshotToken| {
            matches!(
                &token.kind,
                SnapshotTokenKind::ControlSequence { name }
                    if matches!(name.as_str(), "delcode" | "csname" | "ifcsname" | "@nameuse")
            )
        };
        let tokens_require_delcode =
            |tokens: &[SnapshotToken]| tokens.iter().any(&token_requires_delcode);
        let token_requires_integer_parameter = |token: &SnapshotToken| {
            matches!(
                &token.kind,
                SnapshotTokenKind::ControlSequence { name }
                    if matches!(name.as_str(), "tolerance" | "csname" | "ifcsname" | "@nameuse")
            )
        };
        let tokens_require_integer_parameter =
            |tokens: &[SnapshotToken]| tokens.iter().any(&token_requires_integer_parameter);
        let token_requires_layout_integer_parameter = |token: &SnapshotToken| {
            matches!(
                &token.kind,
                SnapshotTokenKind::ControlSequence { name }
                    if LayoutIntegerParameterId::from_source_name(name).is_some()
                        || matches!(name.as_str(), "csname" | "ifcsname" | "@nameuse")
            )
        };
        let tokens_require_layout_integer_parameter =
            |tokens: &[SnapshotToken]| tokens.iter().any(&token_requires_layout_integer_parameter);
        let scopes_require_muskip_alias = self.scopes.iter().any(|scope| {
            scope.values().any(|meaning| match meaning {
                SnapshotMeaning::Macro {
                    parameter_text,
                    optional_first_argument_default,
                    body,
                    ..
                } => {
                    tokens_require_muskip_alias(parameter_text)
                        || optional_first_argument_default
                            .as_deref()
                            .is_some_and(&tokens_require_muskip_alias)
                        || tokens_require_muskip_alias(body)
                }
                SnapshotMeaning::Primitive { name } => {
                    matches!(
                        name.as_str(),
                        "muskip" | "newmuskip" | "muskipdef" | "csname" | "ifcsname" | "@nameuse"
                    )
                }
                SnapshotMeaning::Token { token } => token_requires_muskip_alias(token),
            })
        });
        let scopes_require_mathcode = self.scopes.iter().any(|scope| {
            scope.values().any(|meaning| match meaning {
                SnapshotMeaning::Macro {
                    parameter_text,
                    optional_first_argument_default,
                    body,
                    ..
                } => {
                    tokens_require_mathcode(parameter_text)
                        || optional_first_argument_default
                            .as_deref()
                            .is_some_and(&tokens_require_mathcode)
                        || tokens_require_mathcode(body)
                }
                SnapshotMeaning::Primitive { name } => {
                    matches!(
                        name.as_str(),
                        "mathcode" | "csname" | "ifcsname" | "@nameuse"
                    )
                }
                SnapshotMeaning::Token { token } => token_requires_mathcode(token),
            })
        });
        let scopes_require_delcode = self.scopes.iter().any(|scope| {
            scope.values().any(|meaning| match meaning {
                SnapshotMeaning::Macro {
                    parameter_text,
                    optional_first_argument_default,
                    body,
                    ..
                } => {
                    tokens_require_delcode(parameter_text)
                        || optional_first_argument_default
                            .as_deref()
                            .is_some_and(&tokens_require_delcode)
                        || tokens_require_delcode(body)
                }
                SnapshotMeaning::Primitive { name } => {
                    matches!(
                        name.as_str(),
                        "delcode" | "csname" | "ifcsname" | "@nameuse"
                    )
                }
                SnapshotMeaning::Token { token } => token_requires_delcode(token),
            })
        });
        let scopes_require_integer_parameter = self.scopes.iter().any(|scope| {
            scope.values().any(|meaning| match meaning {
                SnapshotMeaning::Macro {
                    parameter_text,
                    optional_first_argument_default,
                    body,
                    ..
                } => {
                    tokens_require_integer_parameter(parameter_text)
                        || optional_first_argument_default
                            .as_deref()
                            .is_some_and(&tokens_require_integer_parameter)
                        || tokens_require_integer_parameter(body)
                }
                SnapshotMeaning::Primitive { name } => {
                    matches!(
                        name.as_str(),
                        "tolerance" | "csname" | "ifcsname" | "@nameuse"
                    )
                }
                SnapshotMeaning::Token { token } => token_requires_integer_parameter(token),
            })
        });
        let scopes_require_layout_integer_parameter = self.scopes.iter().any(|scope| {
            scope.values().any(|meaning| match meaning {
                SnapshotMeaning::Macro {
                    parameter_text,
                    optional_first_argument_default,
                    body,
                    ..
                } => {
                    tokens_require_layout_integer_parameter(parameter_text)
                        || optional_first_argument_default
                            .as_deref()
                            .is_some_and(&tokens_require_layout_integer_parameter)
                        || tokens_require_layout_integer_parameter(body)
                }
                SnapshotMeaning::Primitive { name } => {
                    LayoutIntegerParameterId::from_source_name(name).is_some()
                        || matches!(name.as_str(), "csname" | "ifcsname" | "@nameuse")
                }
                SnapshotMeaning::Token { token } => token_requires_layout_integer_parameter(token),
            })
        });
        let continuation_requires_muskip_alias =
            self.input_continuation
                .as_ref()
                .is_some_and(|continuation| {
                    continuation.queue.iter().any(|item| {
                        matches!(
                            item,
                            VmQueueItemSnapshot::Token { token }
                                if token_requires_muskip_alias(token)
                        )
                    }) || continuation.source_stack.iter().any(|frame| {
                        frame
                            .end_hooks
                            .iter()
                            .any(|tokens| tokens_require_muskip_alias(tokens))
                            || frame.module_options.as_ref().is_some_and(|options| {
                                options
                                    .declared_options
                                    .values()
                                    .any(|tokens| tokens_require_muskip_alias(tokens))
                                    || options
                                        .default_option_body
                                        .as_deref()
                                        .is_some_and(&tokens_require_muskip_alias)
                            })
                    })
                });
        if scopes_require_muskip_alias
            || self
                .token_registers
                .values()
                .any(|tokens| tokens_require_muskip_alias(tokens))
            || self
                .aftergroup_tokens
                .iter()
                .any(|tokens| tokens_require_muskip_alias(tokens))
            || self
                .after_assignment_token
                .as_ref()
                .is_some_and(&token_requires_muskip_alias)
            || self
                .at_end_document_hooks
                .iter()
                .any(|tokens| tokens_require_muskip_alias(tokens))
            || continuation_requires_muskip_alias
        {
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_MUSKIP_ALIAS_V1_CAPABILITY,
            ));
        }
        let continuation_requires_mathcode =
            self.input_continuation
                .as_ref()
                .is_some_and(|continuation| {
                    continuation.queue.iter().any(|item| match item {
                        VmQueueItemSnapshot::Token { token } => token_requires_mathcode(token),
                        VmQueueItemSnapshot::CharacterSource { mouth } => {
                            ["mathcode", "csname", "ifcsname", "@nameuse"]
                                .iter()
                                .any(|name| mouth.input().contains(name))
                        }
                        VmQueueItemSnapshot::ModuleEnd { .. } => false,
                    }) || continuation.source_stack.iter().any(|frame| {
                        frame
                            .end_hooks
                            .iter()
                            .any(|tokens| tokens_require_mathcode(tokens))
                            || frame.module_options.as_ref().is_some_and(|options| {
                                options
                                    .declared_options
                                    .values()
                                    .any(|tokens| tokens_require_mathcode(tokens))
                                    || options
                                        .default_option_body
                                        .as_deref()
                                        .is_some_and(&tokens_require_mathcode)
                            })
                    })
                });
        if scopes_require_mathcode
            || self
                .token_registers
                .values()
                .any(|tokens| tokens_require_mathcode(tokens))
            || self
                .aftergroup_tokens
                .iter()
                .any(|tokens| tokens_require_mathcode(tokens))
            || self
                .after_assignment_token
                .as_ref()
                .is_some_and(&token_requires_mathcode)
            || self
                .at_end_document_hooks
                .iter()
                .any(|tokens| tokens_require_mathcode(tokens))
            || continuation_requires_mathcode
        {
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_MATHCODE_TABLE_V1_CAPABILITY,
            ));
        }
        let continuation_requires_delcode =
            self.input_continuation
                .as_ref()
                .is_some_and(|continuation| {
                    continuation.queue.iter().any(|item| match item {
                        VmQueueItemSnapshot::Token { token } => token_requires_delcode(token),
                        VmQueueItemSnapshot::CharacterSource { mouth } => {
                            ["delcode", "csname", "ifcsname", "@nameuse"]
                                .iter()
                                .any(|name| mouth.input().contains(name))
                        }
                        VmQueueItemSnapshot::ModuleEnd { .. } => false,
                    }) || continuation.source_stack.iter().any(|frame| {
                        frame
                            .end_hooks
                            .iter()
                            .any(|tokens| tokens_require_delcode(tokens))
                            || frame.module_options.as_ref().is_some_and(|options| {
                                options
                                    .declared_options
                                    .values()
                                    .any(|tokens| tokens_require_delcode(tokens))
                                    || options
                                        .default_option_body
                                        .as_deref()
                                        .is_some_and(&tokens_require_delcode)
                            })
                    })
                });
        if scopes_require_delcode
            || self
                .token_registers
                .values()
                .any(|tokens| tokens_require_delcode(tokens))
            || self
                .aftergroup_tokens
                .iter()
                .any(|tokens| tokens_require_delcode(tokens))
            || self
                .after_assignment_token
                .as_ref()
                .is_some_and(&token_requires_delcode)
            || self
                .at_end_document_hooks
                .iter()
                .any(|tokens| tokens_require_delcode(tokens))
            || continuation_requires_delcode
        {
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_DELCODE_TABLE_V1_CAPABILITY,
            ));
        }
        let continuation_requires_integer_parameter =
            self.input_continuation
                .as_ref()
                .is_some_and(|continuation| {
                    continuation.queue.iter().any(|item| match item {
                        VmQueueItemSnapshot::Token { token } => {
                            token_requires_integer_parameter(token)
                        }
                        VmQueueItemSnapshot::CharacterSource { mouth } => {
                            mouth.input().contains("tolerance")
                                || ["csname", "ifcsname", "@nameuse"]
                                    .iter()
                                    .any(|name| mouth.input().contains(name))
                        }
                        VmQueueItemSnapshot::ModuleEnd { .. } => false,
                    }) || continuation.source_stack.iter().any(|frame| {
                        frame
                            .end_hooks
                            .iter()
                            .any(|tokens| tokens_require_integer_parameter(tokens))
                            || frame.module_options.as_ref().is_some_and(|options| {
                                options
                                    .declared_options
                                    .values()
                                    .any(|tokens| tokens_require_integer_parameter(tokens))
                                    || options
                                        .default_option_body
                                        .as_deref()
                                        .is_some_and(&tokens_require_integer_parameter)
                            })
                    })
                });
        if scopes_require_integer_parameter
            || self
                .token_registers
                .values()
                .any(|tokens| tokens_require_integer_parameter(tokens))
            || self
                .aftergroup_tokens
                .iter()
                .any(|tokens| tokens_require_integer_parameter(tokens))
            || self
                .after_assignment_token
                .as_ref()
                .is_some_and(&token_requires_integer_parameter)
            || self
                .at_end_document_hooks
                .iter()
                .any(|tokens| tokens_require_integer_parameter(tokens))
            || continuation_requires_integer_parameter
        {
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
            ));
        }
        let continuation_requires_layout_integer_parameter = self
            .input_continuation
            .as_ref()
            .is_some_and(|continuation| {
                continuation.queue.iter().any(|item| match item {
                    VmQueueItemSnapshot::Token { token } => {
                        token_requires_layout_integer_parameter(token)
                    }
                    VmQueueItemSnapshot::CharacterSource { mouth } => {
                        LayoutIntegerParameterId::ALL
                            .iter()
                            .any(|parameter| mouth.input().contains(parameter.as_str()))
                            || ["csname", "ifcsname", "@nameuse"]
                                .iter()
                                .any(|name| mouth.input().contains(name))
                    }
                    VmQueueItemSnapshot::ModuleEnd { .. } => false,
                }) || continuation.source_stack.iter().any(|frame| {
                    frame
                        .end_hooks
                        .iter()
                        .any(|tokens| tokens_require_layout_integer_parameter(tokens))
                        || frame.module_options.as_ref().is_some_and(|options| {
                            options
                                .declared_options
                                .values()
                                .any(|tokens| tokens_require_layout_integer_parameter(tokens))
                                || options
                                    .default_option_body
                                    .as_deref()
                                    .is_some_and(&tokens_require_layout_integer_parameter)
                        })
                })
            });
        if scopes_require_layout_integer_parameter
            || self
                .token_registers
                .values()
                .any(|tokens| tokens_require_layout_integer_parameter(tokens))
            || self
                .aftergroup_tokens
                .iter()
                .any(|tokens| tokens_require_layout_integer_parameter(tokens))
            || self
                .after_assignment_token
                .as_ref()
                .is_some_and(&token_requires_layout_integer_parameter)
            || self
                .at_end_document_hooks
                .iter()
                .any(|tokens| tokens_require_layout_integer_parameter(tokens))
            || continuation_requires_layout_integer_parameter
        {
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_STATE_V1_CAPABILITY,
            ));
            capabilities.insert(SnapshotCapability::new(
                VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY,
            ));
        }
        capabilities
    }

    pub(crate) fn dimension_parameter_command_v1(&self) -> Option<DimensionParameterId> {
        self.scopes.iter().find_map(|scope| {
            scope.values().find_map(|meaning| match meaning {
                SnapshotMeaning::Primitive { name } => {
                    DimensionParameterId::from_snapshot_command_v1_name(name)
                }
                SnapshotMeaning::Macro { .. } | SnapshotMeaning::Token { .. } => None,
            })
        })
    }

    pub(crate) fn has_nonlegacy_layered_eqtb_state(&self) -> bool {
        self.mathcode_state.is_some()
            || self.delcode_state.is_some()
            || self.integer_parameter_state.is_some()
            || self.layout_integer_parameter_state.is_some()
            || self.dimension_parameter_state.is_some()
            || self.dimension_parameter_command_v1().is_some()
            || self.required_capabilities().iter().any(|capability| {
                capability.as_str() == VM_SNAPSHOT_LAYOUT_INTEGER_PARAMETER_COMMAND_V1_CAPABILITY
            })
    }
}

impl Deref for VmSnapshot {
    type Target = LegacyVmSnapshotV1;

    fn deref(&self) -> &Self::Target {
        &self.legacy
    }
}

impl DerefMut for VmSnapshot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.legacy
    }
}

impl Serialize for VmSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let capabilities = self.required_capabilities();
        if !capabilities.is_empty() {
            return Err(serde::ser::Error::custom(format!(
                "legacy VM snapshot cannot encode required capabilities: {}",
                capabilities
                    .iter()
                    .map(SnapshotCapability::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        self.legacy.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VmSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let snapshot = Self {
            legacy: LegacyVmSnapshotV1::deserialize(deserializer)?,
            muskip_registers: BTreeMap::new(),
            next_muskip_register: default_next_muskip_register(),
            mathcode_state: None,
            delcode_state: None,
            integer_parameter_state: None,
            layout_integer_parameter_state: None,
            dimension_parameter_state: None,
        };
        let capabilities = snapshot.required_capabilities();
        if !capabilities.is_empty() {
            return Err(serde::de::Error::custom(format!(
                "legacy VM snapshot contains state requiring unsupported capabilities: {}",
                capabilities
                    .iter()
                    .map(SnapshotCapability::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        Ok(snapshot)
    }
}

fn default_hidden_environments() -> Vec<String> {
    vec!["comment".to_string()]
}

fn default_next_render_event_sequence() -> EventSequence {
    1
}

fn default_next_footnote_id() -> FootnoteId {
    1
}

fn event_execution_anchors_are_restorable(
    anchors: &[VmEventExecutionAnchorSnapshot],
    event_ids: &[EventSequence],
) -> bool {
    let anchored_event_ids = anchors
        .iter()
        .map(|anchor| anchor.event_sequence)
        .collect::<Vec<_>>();
    let expected_event_ids = event_ids.iter().copied().collect::<BTreeSet<_>>();
    values_are_unique_nonzero(&anchored_event_ids)
        && anchored_event_ids.iter().copied().collect::<BTreeSet<_>>() == expected_event_ids
        && anchors
            .iter()
            .all(|anchor| anchor.execution_anchor.is_restorable())
}

fn event_execution_anchors_are_valid(anchors: &[VmEventExecutionAnchorSnapshot]) -> bool {
    let event_ids = anchors
        .iter()
        .map(|anchor| anchor.event_sequence)
        .collect::<Vec<_>>();
    values_are_unique_nonzero(&event_ids)
        && anchors
            .iter()
            .all(|anchor| anchor.execution_anchor.is_restorable())
}

fn values_are_unique_nonzero(values: &[EventSequence]) -> bool {
    values.iter().all(|value| *value >= 1) && values_are_unique(values)
}

fn executed_capture_producer_is_restorable(producer: EventProducer) -> bool {
    matches!(producer, EventProducer::Primitive | EventProducer::Macro)
}

fn values_are_unique<T: Ord + Clone>(values: &[T]) -> bool {
    values.iter().cloned().collect::<BTreeSet<_>>().len() == values.len()
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
            && self
                .queue
                .iter()
                .filter(|item| matches!(item, VmQueueItemSnapshot::CharacterSource { .. }))
                .count()
                == self.source_stack.len()
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
    #[serde(default)]
    pub output_start_utf8: u32,
    #[serde(default)]
    pub execution_anchor: Option<VmExecutionAnchor>,
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
        #[serde(default)]
        long: bool,
        #[serde(default)]
        outer: bool,
        #[serde(default)]
        protected: bool,
        parameter_count: u8,
        #[serde(default)]
        parameter_text: Vec<SnapshotToken>,
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

pub(crate) fn default_next_muskip_register() -> u32 {
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
