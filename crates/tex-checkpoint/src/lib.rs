use std::fs;
use std::io::{self, Read, Write};

use anyhow::{Context, Result, anyhow};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::read::DecoderReader;
use base64::write::EncoderWriter;
use camino::{Utf8Path, Utf8PathBuf};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    LegacyVmSnapshotV1, SnapshotCapability, Vm, VmContinuationBlocker, VmContinuationSafety,
    VmModuleCheckpointKind, VmReplayFrame, VmSnapshot, VmSnapshotDocument,
    decode_vm_snapshot_document,
};

pub const CHECKPOINT_UNSAFE_STATE: &str = "CHECKPOINT_UNSAFE_STATE";
pub const CHECKPOINT_VM_SEMANTIC_EPOCH: u32 = 2;

const CHECKPOINT_DISK_SCHEMA_VERSION: u32 = 2;
const CHECKPOINT_DISK_ENCODING: &str = "gzip+base64";
const MAX_CHECKPOINT_UNCOMPRESSED_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const CHECKPOINT_ENVELOPE_PREFIX: &[u8] =
    b"{\"schema_version\":2,\"encoding\":\"gzip+base64\",\"payload\":\"";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointUncompressedSizeLimitExceeded {
    pub attempted: u64,
    pub limit: u64,
}

impl std::fmt::Display for CheckpointUncompressedSizeLimitExceeded {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "checkpoint payload contains at least {} uncompressed bytes, exceeding the {} byte limit",
            self.attempted, self.limit
        )
    }
}

impl std::error::Error for CheckpointUncompressedSizeLimitExceeded {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Low-cardinality checkpoint persistence failure classes for fleet reporting.
pub enum CheckpointWriteFailureReason {
    LaneMismatch,
    InvalidDocument,
    BundlePreflight,
    SizeLimit,
    Serialization,
    IntegrityEnvelope,
    Tempfile,
    Persist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Byte counts produced by a successfully persisted compact checkpoint envelope.
pub struct CheckpointWriteStats {
    pub uncompressed_bytes: u64,
    pub persisted_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
/// A stable artifact representation of whether checkpoint persistence ran.
pub enum CheckpointWriteOutcome {
    #[default]
    NotAttempted,
    Success {
        uncompressed_bytes: u64,
        persisted_bytes: u64,
    },
    Failure {
        reason: CheckpointWriteFailureReason,
    },
}

impl From<CheckpointWriteStats> for CheckpointWriteOutcome {
    fn from(stats: CheckpointWriteStats) -> Self {
        Self::Success {
            uncompressed_bytes: stats.uncompressed_bytes,
            persisted_bytes: stats.persisted_bytes,
        }
    }
}

#[derive(Debug)]
/// A checkpoint save error with a stable reason and its detailed source chain.
pub struct CheckpointWriteError {
    reason: CheckpointWriteFailureReason,
    source: anyhow::Error,
}

impl CheckpointWriteError {
    fn new(reason: CheckpointWriteFailureReason, source: impl Into<anyhow::Error>) -> Self {
        Self {
            reason,
            source: source.into(),
        }
    }

    pub fn reason(&self) -> CheckpointWriteFailureReason {
        self.reason
    }
}

impl std::fmt::Display for CheckpointWriteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(formatter)
    }
}

impl std::error::Error for CheckpointWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Debug)]
struct CheckpointLaneMismatch(String);

impl std::fmt::Display for CheckpointLaneMismatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CheckpointLaneMismatch {}

#[derive(Debug, Deserialize)]
struct CheckpointBundleEnvelope<'a> {
    schema_version: u32,
    encoding: &'a str,
    #[serde(borrow)]
    payload: &'a str,
    uncompressed_len: u64,
    uncompressed_blake3: &'a str,
}

#[derive(Debug, Deserialize)]
struct CheckpointBundleProbe {
    #[serde(default)]
    schema_version: Option<u32>,
}

struct IntegrityWriter<W> {
    inner: W,
    hasher: blake3::Hasher,
    bytes_written: u64,
    max_bytes: u64,
    limit_exceeded: Option<CheckpointUncompressedSizeLimitExceeded>,
}

impl<W> IntegrityWriter<W> {
    fn new(inner: W, max_bytes: u64) -> Self {
        Self {
            inner,
            hasher: blake3::Hasher::new(),
            bytes_written: 0,
            max_bytes,
            limit_exceeded: None,
        }
    }

    fn limit_exceeded(&self) -> Option<CheckpointUncompressedSizeLimitExceeded> {
        self.limit_exceeded
    }

    fn into_parts(self) -> (W, u64, blake3::Hash) {
        (self.inner, self.bytes_written, self.hasher.finalize())
    }
}

impl<W: Write> Write for IntegrityWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let attempted = self
            .bytes_written
            .checked_add(u64::try_from(buffer.len()).unwrap_or(u64::MAX))
            .unwrap_or(u64::MAX);
        if attempted > self.max_bytes {
            let error = CheckpointUncompressedSizeLimitExceeded {
                attempted,
                limit: self.max_bytes,
            };
            self.limit_exceeded = Some(error);
            return Err(io::Error::new(io::ErrorKind::InvalidData, error));
        }
        let written = self.inner.write(buffer)?;
        self.hasher.update(&buffer[..written]);
        self.bytes_written = self.bytes_written.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

struct IntegrityReader<R> {
    inner: R,
    hasher: blake3::Hasher,
    bytes_read: u64,
    expected_len: u64,
}

impl<R> IntegrityReader<R> {
    fn new(inner: R, expected_len: u64) -> Self {
        Self {
            inner,
            hasher: blake3::Hasher::new(),
            bytes_read: 0,
            expected_len,
        }
    }

    fn finish(mut self) -> io::Result<(u64, blake3::Hash)>
    where
        R: Read,
    {
        let mut buffer = [0; 8 * 1024];
        while self.read(&mut buffer)? != 0 {}
        Ok((self.bytes_read, self.hasher.finalize()))
    }
}

impl<R: Read> Read for IntegrityReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.bytes_read == self.expected_len {
            let mut extra = [0];
            return match self.inner.read(&mut extra)? {
                0 => Ok(0),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "checkpoint payload exceeds declared uncompressed length",
                )),
            };
        }
        let remaining = self.expected_len.saturating_sub(self.bytes_read);
        let limit = remaining.min(buffer.len() as u64) as usize;
        let read = self.inner.read(&mut buffer[..limit])?;
        self.hasher.update(&buffer[..read]);
        self.bytes_read = self.bytes_read.saturating_add(read as u64);
        Ok(read)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointKind {
    Preamble,
    Shipout,
    InputBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointPage {
    pub page_id: String,
    pub index: usize,
    pub content_hash: String,
    pub text_start_utf8: u32,
    pub text_end_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointMeta {
    pub checkpoint_id: String,
    pub kind: CheckpointKind,
    pub rev: u64,
    pub page_index_after: usize,
    pub boundary_hash: String,
    pub vm_state_hash: String,
    pub snapshot_attached: bool,
    #[serde(default)]
    pub continuation_safety: VmContinuationSafety,
    #[serde(default)]
    pub source_offset_utf8: u32,
    #[serde(default)]
    pub resume_path: Option<Utf8PathBuf>,
    #[serde(default)]
    pub continuation_stack: Vec<VmReplayFrame>,
    #[serde(default)]
    pub module_path: Option<Utf8PathBuf>,
    #[serde(default)]
    pub input_boundary_kind: Option<VmModuleCheckpointKind>,
    #[serde(default)]
    pub output_start_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedSnapshotSlot {
    document: VmSnapshotDocument,
}

impl VersionedSnapshotSlot {
    pub fn document(&self) -> &VmSnapshotDocument {
        &self.document
    }

    fn from_snapshot(snapshot: VmSnapshot) -> Self {
        Self {
            document: VmSnapshotDocument::from_snapshot(snapshot),
        }
    }
}

#[derive(Serialize)]
struct VersionedSnapshotSlotWriteWire<'a> {
    document: &'a VmSnapshotDocument,
}

impl Serialize for VersionedSnapshotSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.document
            .validate_for_write()
            .map_err(serde::ser::Error::custom)?;
        VersionedSnapshotSlotWriteWire {
            document: &self.document,
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionedSnapshotSlotWire {
    document: Box<RawValue>,
}

impl<'de> Deserialize<'de> for VersionedSnapshotSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = VersionedSnapshotSlotWire::deserialize(deserializer)?;
        let document = decode_vm_snapshot_document(wire.document.get().as_bytes())
            .map_err(serde::de::Error::custom)?;
        Ok(Self { document })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotAttachment<'a> {
    None,
    Legacy(&'a VmSnapshot),
    Versioned(&'a VmSnapshotDocument),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotForRestore<'a> {
    Legacy(&'a VmSnapshot),
    Versioned(&'a VmSnapshotDocument),
}

impl<'a> SnapshotForRestore<'a> {
    pub fn state(self) -> &'a VmSnapshot {
        match self {
            Self::Legacy(snapshot) => snapshot,
            Self::Versioned(document) => &document.state,
        }
    }

    pub fn is_versioned(self) -> bool {
        matches!(self, Self::Versioned(_))
    }

    pub fn required_capabilities(self) -> impl Iterator<Item = &'a SnapshotCapability> {
        match self {
            Self::Legacy(_) => None,
            Self::Versioned(document) => Some(&document.required_capabilities),
        }
        .into_iter()
        .flatten()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// The production checkpoint writer policy intentionally exposes no versioned
/// selection surface.
///
/// ```compile_fail
/// use tex_checkpoint::SnapshotWritePolicy;
///
/// let _ = SnapshotWritePolicy::Versioned {
///     enabled_capabilities: &[],
/// };
/// ```
pub enum SnapshotWritePolicy {
    #[default]
    LegacyOnly,
}

/// A forward-compatible report of the writer policy recorded in artifacts.
///
/// This value is observation only. Checkpoint builders and save functions do
/// not accept it, so an [`Other`](Self::Other) value cannot authorize a lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotWritePolicyObservation {
    LegacyOnly,
    Other(String),
}

impl Default for SnapshotWritePolicyObservation {
    fn default() -> Self {
        Self::LegacyOnly
    }
}

impl SnapshotWritePolicyObservation {
    pub fn as_str(&self) -> &str {
        match self {
            Self::LegacyOnly => "legacy_only",
            Self::Other(value) => value,
        }
    }
}

impl From<SnapshotWritePolicy> for SnapshotWritePolicyObservation {
    fn from(policy: SnapshotWritePolicy) -> Self {
        match policy {
            SnapshotWritePolicy::LegacyOnly => Self::LegacyOnly,
        }
    }
}

impl Serialize for SnapshotWritePolicyObservation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SnapshotWritePolicyObservation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "legacy_only" => Self::LegacyOnly,
            _ => Self::Other(value),
        })
    }
}

impl SnapshotWritePolicy {
    fn allows(
        self,
        required_capabilities: &std::collections::BTreeSet<SnapshotCapability>,
    ) -> bool {
        SnapshotWriteMode::from(self)
            .lane_for(required_capabilities)
            .is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotWriteMode {
    LegacyOnly,
    #[allow(dead_code)] // Compiled now for candidate validation; no production constructor exists.
    Versioned {
        enabled_capabilities: &'static [&'static str],
    },
}

impl From<SnapshotWritePolicy> for SnapshotWriteMode {
    fn from(policy: SnapshotWritePolicy) -> Self {
        match policy {
            SnapshotWritePolicy::LegacyOnly => Self::LegacyOnly,
        }
    }
}

impl SnapshotWriteMode {
    fn lane_for(
        self,
        required_capabilities: &std::collections::BTreeSet<SnapshotCapability>,
    ) -> Option<SnapshotWriteLane> {
        if required_capabilities.is_empty() {
            return Some(SnapshotWriteLane::Legacy);
        }
        match self {
            Self::LegacyOnly => None,
            Self::Versioned {
                enabled_capabilities,
            } => required_capabilities
                .iter()
                .all(|required| enabled_capabilities.contains(&required.as_str()))
                .then_some(SnapshotWriteLane::Versioned),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotWriteLane {
    Legacy,
    Versioned,
}

pub const SNAPSHOT_WRITE_POLICY: SnapshotWritePolicy = SnapshotWritePolicy::LegacyOnly;

#[derive(Debug, Clone, PartialEq, Eq)]
enum StoredSnapshotAttachment {
    None,
    Legacy(VmSnapshot),
    Versioned(VersionedSnapshotSlot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LegacySnapshotForWrite(VmSnapshot);

impl TryFrom<VmSnapshot> for LegacySnapshotForWrite {
    type Error = anyhow::Error;

    fn try_from(snapshot: VmSnapshot) -> Result<Self> {
        ensure_legacy_snapshot_writable(&snapshot)?;
        Ok(Self(snapshot))
    }
}

fn ensure_legacy_snapshot_writable(
    snapshot: &VmSnapshot,
) -> std::result::Result<(), CheckpointLaneMismatch> {
    let required_capabilities = snapshot.required_capabilities();
    if !SNAPSHOT_WRITE_POLICY.allows(&required_capabilities) {
        return Err(CheckpointLaneMismatch(format!(
            "legacy snapshot writer cannot encode required capabilities: {}",
            required_capabilities
                .iter()
                .map(SnapshotCapability::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCheckpoint {
    pub meta: CheckpointMeta,
    attachment: StoredSnapshotAttachment,
}

#[derive(Serialize)]
struct StoredCheckpointWriteWire<'a> {
    meta: &'a CheckpointMeta,
    snapshot: Option<&'a VmSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    versioned_snapshot: Option<&'a VersionedSnapshotSlot>,
}

impl StoredCheckpoint {
    fn write_wire(&self, policy: SnapshotWriteMode) -> Result<StoredCheckpointWriteWire<'_>> {
        let (snapshot, versioned_snapshot) = match &self.attachment {
            StoredSnapshotAttachment::None => (None, None),
            StoredSnapshotAttachment::Legacy(snapshot) => {
                ensure_legacy_snapshot_writable(snapshot)?;
                (Some(snapshot), None)
            }
            StoredSnapshotAttachment::Versioned(slot) => {
                let required_capabilities = &slot.document.required_capabilities;
                if !matches!(
                    policy.lane_for(required_capabilities),
                    Some(SnapshotWriteLane::Versioned)
                ) {
                    match policy {
                        SnapshotWriteMode::LegacyOnly => {
                            return Err(CheckpointLaneMismatch(
                                "versioned snapshot writer is disabled".to_string(),
                            )
                            .into());
                        }
                        SnapshotWriteMode::Versioned { .. } => {
                            return Err(CheckpointLaneMismatch(format!(
                                "versioned snapshot writer does not enable required capabilities: {}",
                                required_capabilities
                                    .iter()
                                    .map(SnapshotCapability::as_str)
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ))
                            .into());
                        }
                    }
                }
                slot.document.validate_for_write()?;
                (None, Some(slot))
            }
        };
        Ok(StoredCheckpointWriteWire {
            meta: &self.meta,
            snapshot,
            versioned_snapshot,
        })
    }
}

impl Serialize for StoredCheckpoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.write_wire(SNAPSHOT_WRITE_POLICY.into())
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

#[derive(Deserialize)]
struct StoredCheckpointWire {
    meta: CheckpointMeta,
    snapshot: Option<VmSnapshot>,
    #[serde(default)]
    versioned_snapshot: Option<Box<RawValue>>,
}

impl<'de> Deserialize<'de> for StoredCheckpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = StoredCheckpointWire::deserialize(deserializer)?;
        let attachment = match (wire.snapshot, wire.versioned_snapshot) {
            (None, None) => StoredSnapshotAttachment::None,
            (Some(snapshot), None) => StoredSnapshotAttachment::Legacy(snapshot),
            (None, Some(slot)) => StoredSnapshotAttachment::Versioned(
                serde_json::from_str(slot.get()).map_err(serde::de::Error::custom)?,
            ),
            (Some(_), Some(_)) => {
                return Err(serde::de::Error::custom(
                    "checkpoint contains both snapshot lanes",
                ));
            }
        };
        Ok(Self {
            meta: wire.meta,
            attachment,
        })
    }
}

impl StoredCheckpoint {
    fn with_snapshot(
        meta: CheckpointMeta,
        snapshot: Option<VmSnapshot>,
        policy: SnapshotWriteMode,
    ) -> Result<Self> {
        let attachment = match snapshot {
            Some(snapshot) => match policy.lane_for(&snapshot.required_capabilities()) {
                Some(SnapshotWriteLane::Legacy) => {
                    StoredSnapshotAttachment::Legacy(LegacySnapshotForWrite::try_from(snapshot)?.0)
                }
                Some(SnapshotWriteLane::Versioned) => StoredSnapshotAttachment::Versioned(
                    VersionedSnapshotSlot::from_snapshot(snapshot),
                ),
                None => anyhow::bail!("snapshot is not enabled by the write policy"),
            },
            None => StoredSnapshotAttachment::None,
        };
        Ok(Self { meta, attachment })
    }

    pub fn snapshot_attachment(&self) -> SnapshotAttachment<'_> {
        match &self.attachment {
            StoredSnapshotAttachment::None => SnapshotAttachment::None,
            StoredSnapshotAttachment::Legacy(snapshot) => SnapshotAttachment::Legacy(snapshot),
            StoredSnapshotAttachment::Versioned(slot) => {
                SnapshotAttachment::Versioned(slot.document())
            }
        }
    }

    pub fn snapshot_for_restore(&self) -> Option<SnapshotForRestore<'_>> {
        match self.snapshot_attachment() {
            SnapshotAttachment::None => None,
            SnapshotAttachment::Legacy(snapshot) => Some(SnapshotForRestore::Legacy(snapshot)),
            SnapshotAttachment::Versioned(document) => {
                Some(SnapshotForRestore::Versioned(document))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CheckpointBundle {
    #[serde(default)]
    pub vm_semantic_epoch: u32,
    pub checkpoints: Vec<StoredCheckpoint>,
    #[serde(default)]
    pub pages: Vec<CheckpointPage>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointAttachmentCounts {
    pub none: usize,
    pub legacy: usize,
    pub versioned: usize,
}

#[derive(Serialize)]
struct CheckpointBundleWriteWire<'a> {
    vm_semantic_epoch: u32,
    checkpoints: Vec<StoredCheckpointWriteWire<'a>>,
    pages: &'a [CheckpointPage],
}

struct CheckpointBundleWriteWithPolicy<'a> {
    bundle: &'a CheckpointBundle,
    policy: SnapshotWriteMode,
}

impl Serialize for CheckpointBundleWriteWithPolicy<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.bundle
            .ensure_writable(self.policy)
            .map_err(serde::ser::Error::custom)?;
        let checkpoints = self
            .bundle
            .checkpoints
            .iter()
            .map(|checkpoint| checkpoint.write_wire(self.policy))
            .collect::<Result<Vec<_>>>()
            .map_err(serde::ser::Error::custom)?;
        CheckpointBundleWriteWire {
            vm_semantic_epoch: self.bundle.vm_semantic_epoch,
            checkpoints,
            pages: &self.bundle.pages,
        }
        .serialize(serializer)
    }
}

impl CheckpointBundle {
    fn ensure_writable(&self, policy: SnapshotWriteMode) -> Result<()> {
        for checkpoint in &self.checkpoints {
            checkpoint.write_wire(policy)?;
        }
        Ok(())
    }

    pub fn attachment_counts(&self) -> CheckpointAttachmentCounts {
        let mut counts = CheckpointAttachmentCounts::default();
        for checkpoint in &self.checkpoints {
            match checkpoint.snapshot_attachment() {
                SnapshotAttachment::None => counts.none += 1,
                SnapshotAttachment::Legacy(_) => counts.legacy += 1,
                SnapshotAttachment::Versioned(_) => counts.versioned += 1,
            }
        }
        counts
    }
}

impl Serialize for CheckpointBundle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        CheckpointBundleWriteWithPolicy {
            bundle: self,
            policy: SNAPSHOT_WRITE_POLICY.into(),
        }
        .serialize(serializer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointCacheMissReason {
    NotFound,
    Unreadable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointBundleReuse {
    Hit(CheckpointBundle),
    Miss(CheckpointCacheMissReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailRealignment {
    pub previous_rev: u64,
    pub resume_checkpoint_id: String,
    pub previous_page_start: usize,
    pub current_page_start: usize,
    pub page_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointReuseDiagnostic {
    pub code: &'static str,
    pub checkpoint_id: String,
    pub blockers: Vec<VmContinuationBlocker>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputBoundaryCheckpoint {
    pub kind: VmModuleCheckpointKind,
    pub module_path: Utf8PathBuf,
    pub resume_path: Option<Utf8PathBuf>,
    pub source_offset_utf8: u32,
    pub continuation_stack: Vec<VmReplayFrame>,
    pub output_start_utf8: u32,
    pub page_index_after: usize,
    pub snapshot: VmSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShipoutCheckpoint {
    pub snapshot: VmSnapshot,
    pub source_offset_utf8: u32,
    pub resume_path: Option<Utf8PathBuf>,
    pub continuation_stack: Vec<VmReplayFrame>,
}

pub fn build_checkpoint_bundle(
    rev: u64,
    preamble_snapshot: &VmSnapshot,
    preamble_key: &str,
    pages: &[CheckpointPage],
) -> Result<CheckpointBundle> {
    build_checkpoint_bundle_with_snapshots(
        rev,
        preamble_snapshot,
        preamble_key,
        0,
        pages,
        &[],
        &[],
        &[],
    )
}

pub fn build_checkpoint_bundle_with_snapshots(
    rev: u64,
    preamble_snapshot: &VmSnapshot,
    preamble_key: &str,
    preamble_source_offset_utf8: u32,
    pages: &[CheckpointPage],
    shipout_snapshots: &[VmSnapshot],
    shipout_source_offsets_utf8: &[u32],
    input_boundaries: &[InputBoundaryCheckpoint],
) -> Result<CheckpointBundle> {
    if shipout_snapshots.len() != shipout_source_offsets_utf8.len() {
        anyhow::bail!("shipout snapshot/source-offset length mismatch");
    }
    let shipout_checkpoints = shipout_snapshots
        .iter()
        .cloned()
        .zip(shipout_source_offsets_utf8.iter().copied())
        .map(|(snapshot, source_offset_utf8)| ShipoutCheckpoint {
            snapshot,
            source_offset_utf8,
            resume_path: None,
            continuation_stack: Vec::new(),
        })
        .collect::<Vec<_>>();
    build_checkpoint_bundle_with_shipouts(
        rev,
        preamble_snapshot,
        preamble_key,
        preamble_source_offset_utf8,
        pages,
        &shipout_checkpoints,
        input_boundaries,
    )
}

pub fn build_checkpoint_bundle_with_shipouts(
    rev: u64,
    preamble_snapshot: &VmSnapshot,
    preamble_key: &str,
    preamble_source_offset_utf8: u32,
    pages: &[CheckpointPage],
    shipout_checkpoints: &[ShipoutCheckpoint],
    input_boundaries: &[InputBoundaryCheckpoint],
) -> Result<CheckpointBundle> {
    Ok(build_checkpoint_bundle_with_shipouts_and_stats(
        rev,
        preamble_snapshot,
        preamble_key,
        preamble_source_offset_utf8,
        pages,
        shipout_checkpoints,
        input_boundaries,
    )?
    .bundle)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointSuppressionCounts {
    pub unsafe_continuation: usize,
    pub unsupported_capabilities: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointBundleBuild {
    pub bundle: CheckpointBundle,
    pub suppression_counts: CheckpointSuppressionCounts,
}

pub fn build_checkpoint_bundle_with_shipouts_and_stats(
    rev: u64,
    preamble_snapshot: &VmSnapshot,
    preamble_key: &str,
    preamble_source_offset_utf8: u32,
    pages: &[CheckpointPage],
    shipout_checkpoints: &[ShipoutCheckpoint],
    input_boundaries: &[InputBoundaryCheckpoint],
) -> Result<CheckpointBundleBuild> {
    build_checkpoint_bundle_with_shipouts_and_policy_and_stats(
        rev,
        preamble_snapshot,
        preamble_key,
        preamble_source_offset_utf8,
        pages,
        shipout_checkpoints,
        input_boundaries,
        SNAPSHOT_WRITE_POLICY.into(),
    )
}

fn build_checkpoint_bundle_with_shipouts_and_policy_and_stats(
    rev: u64,
    preamble_snapshot: &VmSnapshot,
    preamble_key: &str,
    preamble_source_offset_utf8: u32,
    pages: &[CheckpointPage],
    shipout_checkpoints: &[ShipoutCheckpoint],
    input_boundaries: &[InputBoundaryCheckpoint],
    policy: SnapshotWriteMode,
) -> Result<CheckpointBundleBuild> {
    if !shipout_checkpoints.is_empty() && shipout_checkpoints.len() != pages.len() {
        anyhow::bail!("shipout snapshot/page length mismatch");
    }
    let mut suppression_counts = CheckpointSuppressionCounts::default();
    let preamble_continuation_safety = preamble_snapshot.continuation_safety.clone();
    let preamble_is_safe = preamble_continuation_safety.is_safe();
    let preamble_is_enabled = policy
        .lane_for(&preamble_snapshot.required_capabilities())
        .is_some();
    let preamble_snapshot_attached = preamble_is_safe && preamble_is_enabled;
    if !preamble_is_safe {
        suppression_counts.unsafe_continuation += 1;
    } else if !preamble_is_enabled {
        suppression_counts.unsupported_capabilities += 1;
    }
    let vm_state_hash = checkpoint_vm_semantic_hash(preamble_snapshot)
        .context("failed to fingerprint preamble snapshot")?;
    let mut checkpoints = vec![StoredCheckpoint::with_snapshot(
        CheckpointMeta {
            checkpoint_id: checkpoint_id(
                CheckpointKind::Preamble,
                rev,
                0,
                preamble_key,
                &vm_state_hash,
            ),
            kind: CheckpointKind::Preamble,
            rev,
            page_index_after: 0,
            boundary_hash: preamble_key.to_string(),
            vm_state_hash: vm_state_hash.clone(),
            snapshot_attached: preamble_snapshot_attached,
            continuation_safety: preamble_continuation_safety,
            source_offset_utf8: preamble_source_offset_utf8,
            resume_path: None,
            continuation_stack: Vec::new(),
            module_path: None,
            input_boundary_kind: None,
            output_start_utf8: 0,
        },
        preamble_snapshot_attached.then(|| preamble_snapshot.clone()),
        policy,
    )?];

    for (index, page) in pages.iter().enumerate() {
        let boundary_hash = page_boundary_hash(page);
        let shipout_checkpoint = shipout_checkpoints.get(index);
        let source_offset_utf8 = shipout_checkpoint
            .map(|checkpoint| checkpoint.source_offset_utf8)
            .unwrap_or(0);
        let vm_state_hash = shipout_checkpoint
            .map(|checkpoint| checkpoint_vm_semantic_hash(&checkpoint.snapshot))
            .transpose()
            .context("failed to fingerprint shipout snapshot")?
            .unwrap_or_else(|| vm_state_hash.clone());
        let continuation_safety = shipout_checkpoint
            .map(|checkpoint| checkpoint.snapshot.continuation_safety.clone())
            .unwrap_or_default();
        let snapshot_attached = shipout_checkpoint.is_some_and(|checkpoint| {
            let is_safe = continuation_safety.is_safe();
            let is_enabled = policy
                .lane_for(&checkpoint.snapshot.required_capabilities())
                .is_some();
            if !is_safe {
                suppression_counts.unsafe_continuation += 1;
            } else if !is_enabled {
                suppression_counts.unsupported_capabilities += 1;
            }
            is_safe && is_enabled
        });
        checkpoints.push(StoredCheckpoint::with_snapshot(
            CheckpointMeta {
                checkpoint_id: checkpoint_id(
                    CheckpointKind::Shipout,
                    rev,
                    page.index + 1,
                    &boundary_hash,
                    &vm_state_hash,
                ),
                kind: CheckpointKind::Shipout,
                rev,
                page_index_after: page.index + 1,
                boundary_hash,
                vm_state_hash: vm_state_hash.clone(),
                snapshot_attached,
                continuation_safety,
                source_offset_utf8,
                resume_path: shipout_checkpoint
                    .and_then(|checkpoint| checkpoint.resume_path.clone()),
                continuation_stack: shipout_checkpoint
                    .map(|checkpoint| checkpoint.continuation_stack.clone())
                    .unwrap_or_default(),
                module_path: None,
                input_boundary_kind: None,
                output_start_utf8: page.text_start_utf8,
            },
            snapshot_attached.then(|| {
                shipout_checkpoint
                    .expect("attached shipout snapshot")
                    .snapshot
                    .clone()
            }),
            policy,
        )?);
    }

    for boundary in input_boundaries {
        let continuation_safety = boundary.snapshot.continuation_safety.clone();
        let is_safe = continuation_safety.is_safe()
            && boundary
                .snapshot
                .input_continuation
                .as_ref()
                .is_none_or(tex_vm::VmInputContinuationSnapshot::is_restorable);
        let is_enabled = policy
            .lane_for(&boundary.snapshot.required_capabilities())
            .is_some();
        let snapshot_attached = is_safe && is_enabled;
        if !is_safe {
            suppression_counts.unsafe_continuation += 1;
        } else if !is_enabled {
            suppression_counts.unsupported_capabilities += 1;
        }
        let vm_state_hash = checkpoint_vm_semantic_hash(&boundary.snapshot)
            .context("failed to fingerprint input-boundary snapshot")?;
        let boundary_hash = blake3::hash(
            format!(
                "{}:{}:{}:{}:{}:{}",
                match boundary.kind {
                    VmModuleCheckpointKind::Enter => "enter",
                    VmModuleCheckpointKind::Exit => "exit",
                },
                boundary.module_path,
                boundary.resume_path.as_deref().unwrap_or(Utf8Path::new("")),
                boundary.source_offset_utf8,
                boundary.output_start_utf8,
                boundary.page_index_after
            )
            .as_bytes(),
        )
        .to_hex()
        .to_string();
        checkpoints.push(StoredCheckpoint::with_snapshot(
            CheckpointMeta {
                checkpoint_id: checkpoint_id(
                    CheckpointKind::InputBoundary,
                    rev,
                    boundary.page_index_after,
                    &boundary_hash,
                    &vm_state_hash,
                ),
                kind: CheckpointKind::InputBoundary,
                rev,
                page_index_after: boundary.page_index_after,
                boundary_hash,
                vm_state_hash,
                snapshot_attached,
                continuation_safety,
                source_offset_utf8: boundary.source_offset_utf8,
                resume_path: boundary.resume_path.clone(),
                continuation_stack: boundary.continuation_stack.clone(),
                module_path: Some(boundary.module_path.clone()),
                input_boundary_kind: Some(boundary.kind),
                output_start_utf8: boundary.output_start_utf8,
            },
            snapshot_attached.then(|| boundary.snapshot.clone()),
            policy,
        )?);
    }

    Ok(CheckpointBundleBuild {
        bundle: CheckpointBundle {
            vm_semantic_epoch: CHECKPOINT_VM_SEMANTIC_EPOCH,
            checkpoints,
            pages: pages.to_vec(),
        },
        suppression_counts,
    })
}

// Persisted in checkpoint metadata and folded into checkpoint ids. This is a
// semantic state identity: write-lane policy and attachment representation must
// not change it. Capability-free state retains the legacy hash domain, while
// capability-bearing state uses the complete domain-separated fingerprint.
fn checkpoint_vm_semantic_hash(snapshot: &VmSnapshot) -> Result<String> {
    let required_capabilities = snapshot.required_capabilities();
    let legacy: &LegacyVmSnapshotV1 = snapshot;
    let legacy_json = serde_json::to_vec(&serde_json::to_value(legacy)?)?;
    if required_capabilities.is_empty() {
        return Ok(blake3::hash(&legacy_json).to_hex().to_string());
    }

    let muskip_json = serde_json::to_vec(&serde_json::to_value(&snapshot.muskip_registers)?)?;
    let mut fingerprint = blake3::Hasher::new();
    fingerprint.update(b"latexd:complete-vm-snapshot-fingerprint:v1\0");
    fingerprint.update(
        &u64::try_from(legacy_json.len())
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    fingerprint.update(&legacy_json);
    for capability in required_capabilities {
        fingerprint.update(
            &u64::try_from(capability.as_str().len())
                .unwrap_or(u64::MAX)
                .to_le_bytes(),
        );
        fingerprint.update(capability.as_str().as_bytes());
    }
    fingerprint.update(
        &u64::try_from(muskip_json.len())
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    fingerprint.update(&muskip_json);
    fingerprint.update(&snapshot.next_muskip_register.to_le_bytes());
    for state in [&snapshot.mathcode_state, &snapshot.delcode_state]
        .into_iter()
        .flatten()
    {
        let state_json = serde_json::to_vec(state)?;
        fingerprint.update(
            &u64::try_from(state_json.len())
                .unwrap_or(u64::MAX)
                .to_le_bytes(),
        );
        fingerprint.update(&state_json);
    }
    Ok(fingerprint.finalize().to_hex().to_string())
}

pub fn save_checkpoint_bundle(path: &Utf8Path, bundle: &CheckpointBundle) -> Result<()> {
    save_checkpoint_bundle_with_stats(path, bundle)
        .map(|_| ())
        .map_err(anyhow::Error::new)
}

pub fn save_checkpoint_bundle_with_stats(
    path: &Utf8Path,
    bundle: &CheckpointBundle,
) -> std::result::Result<CheckpointWriteStats, CheckpointWriteError> {
    save_checkpoint_bundle_with_policy_and_limit_and_stats(
        path,
        bundle,
        SNAPSHOT_WRITE_POLICY.into(),
        MAX_CHECKPOINT_UNCOMPRESSED_BYTES,
    )
}

#[cfg(test)]
fn save_checkpoint_bundle_with_policy(
    path: &Utf8Path,
    bundle: &CheckpointBundle,
    policy: SnapshotWriteMode,
) -> Result<()> {
    save_checkpoint_bundle_with_policy_and_limit_and_stats(
        path,
        bundle,
        policy,
        MAX_CHECKPOINT_UNCOMPRESSED_BYTES,
    )
    .map(|_| ())
    .map_err(anyhow::Error::new)
}

#[cfg(test)]
fn save_checkpoint_bundle_with_policy_and_limit(
    path: &Utf8Path,
    bundle: &CheckpointBundle,
    policy: SnapshotWriteMode,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    save_checkpoint_bundle_with_policy_and_limit_and_stats(
        path,
        bundle,
        policy,
        max_uncompressed_bytes,
    )
    .map(|_| ())
    .map_err(anyhow::Error::new)
}

fn save_checkpoint_bundle_with_policy_and_limit_and_stats(
    path: &Utf8Path,
    bundle: &CheckpointBundle,
    policy: SnapshotWriteMode,
    max_uncompressed_bytes: u64,
) -> std::result::Result<CheckpointWriteStats, CheckpointWriteError> {
    bundle.ensure_writable(policy).map_err(|error| {
        let reason = if error.downcast_ref::<CheckpointLaneMismatch>().is_some() {
            CheckpointWriteFailureReason::LaneMismatch
        } else if error
            .downcast_ref::<tex_vm::VmSnapshotDocumentError>()
            .is_some()
        {
            CheckpointWriteFailureReason::InvalidDocument
        } else {
            CheckpointWriteFailureReason::BundlePreflight
        };
        CheckpointWriteError::new(reason, error)
    })?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("checkpoint bundle path has no parent: {path}"))
        .map_err(|error| {
            CheckpointWriteError::new(CheckpointWriteFailureReason::Tempfile, error)
        })?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temporary checkpoint bundle beside {path}"))
        .map_err(|error| {
            CheckpointWriteError::new(CheckpointWriteFailureReason::Tempfile, error)
        })?;
    temporary
        .write_all(CHECKPOINT_ENVELOPE_PREFIX)
        .with_context(|| format!("failed to write checkpoint envelope header for {path}"))
        .map_err(|error| {
            CheckpointWriteError::new(CheckpointWriteFailureReason::IntegrityEnvelope, error)
        })?;
    let (uncompressed_len, uncompressed_hash) = {
        let encoded = EncoderWriter::new(temporary.as_file_mut(), &BASE64_STANDARD);
        let compressed = GzEncoder::new(encoded, Compression::fast());
        let mut integrity = IntegrityWriter::new(compressed, max_uncompressed_bytes);
        if let Err(error) = serde_json::to_writer(
            &mut integrity,
            &CheckpointBundleWriteWithPolicy { bundle, policy },
        ) {
            if let Some(limit_error) = integrity.limit_exceeded() {
                return Err(CheckpointWriteError::new(
                    CheckpointWriteFailureReason::SizeLimit,
                    limit_error,
                ));
            }
            return Err(CheckpointWriteError::new(
                CheckpointWriteFailureReason::Serialization,
                anyhow::Error::new(error).context("failed to serialize checkpoint bundle"),
            ));
        }
        let (compressed, uncompressed_len, uncompressed_hash) = integrity.into_parts();
        let mut encoded = compressed
            .finish()
            .context("failed to finish checkpoint compression")
            .map_err(|error| {
                CheckpointWriteError::new(CheckpointWriteFailureReason::IntegrityEnvelope, error)
            })?;
        encoded
            .finish()
            .context("failed to finish checkpoint base64 encoding")
            .map_err(|error| {
                CheckpointWriteError::new(CheckpointWriteFailureReason::IntegrityEnvelope, error)
            })?;
        (uncompressed_len, uncompressed_hash)
    };
    writeln!(
        temporary,
        "\",\"uncompressed_len\":{uncompressed_len},\"uncompressed_blake3\":\"{}\"}}",
        uncompressed_hash.to_hex()
    )
    .with_context(|| format!("failed to write checkpoint envelope footer for {path}"))
    .map_err(|error| {
        CheckpointWriteError::new(CheckpointWriteFailureReason::IntegrityEnvelope, error)
    })?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("failed to sync temporary checkpoint bundle for {path}"))
        .map_err(|error| {
            CheckpointWriteError::new(CheckpointWriteFailureReason::IntegrityEnvelope, error)
        })?;
    let persisted_bytes = temporary
        .as_file()
        .metadata()
        .with_context(|| format!("failed to inspect temporary checkpoint bundle for {path}"))
        .map_err(|error| {
            CheckpointWriteError::new(CheckpointWriteFailureReason::IntegrityEnvelope, error)
        })?
        .len();
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to replace checkpoint bundle {path}"))
        .map_err(|error| CheckpointWriteError::new(CheckpointWriteFailureReason::Persist, error))?;
    Ok(CheckpointWriteStats {
        uncompressed_bytes: uncompressed_len,
        persisted_bytes,
    })
}

pub fn load_checkpoint_bundle(path: &Utf8Path) -> Result<CheckpointBundle> {
    load_checkpoint_bundle_with_limit(path, MAX_CHECKPOINT_UNCOMPRESSED_BYTES)
}

fn load_checkpoint_bundle_with_limit(
    path: &Utf8Path,
    max_uncompressed_bytes: u64,
) -> Result<CheckpointBundle> {
    let contents =
        fs::read(path).with_context(|| format!("failed to read checkpoint bundle {path}"))?;
    let without_bom = contents
        .strip_prefix(&[0xef, 0xbb, 0xbf])
        .unwrap_or(&contents);
    let first_content = without_bom
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(without_bom.len());
    let trimmed = &without_bom[first_content..];
    let first_key = trimmed.strip_prefix(b"{").map(|object| {
        let first_key_start = object
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
            .unwrap_or(object.len());
        &object[first_key_start..]
    });
    let is_standard_legacy = first_key.is_some_and(|key| key.starts_with(b"\"checkpoints\""));
    let is_envelope = !is_standard_legacy
        && serde_json::from_slice::<CheckpointBundleProbe>(trimmed)
            .is_ok_and(|probe| probe.schema_version.is_some());
    if !is_envelope {
        return serde_json::from_slice(&contents)
            .with_context(|| format!("failed to parse legacy checkpoint bundle {path}"));
    }

    let envelope = serde_json::from_slice::<CheckpointBundleEnvelope<'_>>(trimmed)
        .with_context(|| format!("failed to parse checkpoint envelope {path}"))?;
    if envelope.schema_version != CHECKPOINT_DISK_SCHEMA_VERSION {
        anyhow::bail!(
            "unsupported checkpoint envelope schema version {} in {path}",
            envelope.schema_version
        );
    }
    if envelope.encoding != CHECKPOINT_DISK_ENCODING {
        anyhow::bail!(
            "unsupported checkpoint envelope encoding {:?} in {path}",
            envelope.encoding
        );
    }
    if envelope.uncompressed_len > max_uncompressed_bytes {
        anyhow::bail!(
            "checkpoint payload declares {} uncompressed bytes, exceeding the {} byte limit in {path}",
            envelope.uncompressed_len,
            max_uncompressed_bytes
        );
    }

    let decoded = DecoderReader::new(envelope.payload.as_bytes(), &BASE64_STANDARD);
    let decompressed = GzDecoder::new(decoded);
    let mut integrity = IntegrityReader::new(decompressed, envelope.uncompressed_len);
    let bundle = {
        let mut deserializer = serde_json::Deserializer::from_reader(&mut integrity);
        let bundle = CheckpointBundle::deserialize(&mut deserializer)
            .with_context(|| format!("failed to decode checkpoint bundle {path}"))?;
        deserializer
            .end()
            .with_context(|| format!("checkpoint bundle has trailing JSON data in {path}"))?;
        bundle
    };
    let (uncompressed_len, uncompressed_hash) = integrity
        .finish()
        .with_context(|| format!("failed to verify checkpoint payload {path}"))?;
    if uncompressed_len != envelope.uncompressed_len {
        anyhow::bail!(
            "checkpoint payload length mismatch in {path}: expected {}, read {uncompressed_len}",
            envelope.uncompressed_len
        );
    }
    if uncompressed_hash.to_hex().as_str() != envelope.uncompressed_blake3 {
        anyhow::bail!("checkpoint integrity hash mismatch in {path}");
    }
    Ok(bundle)
}

pub fn load_checkpoint_bundle_for_reuse(path: &Utf8Path) -> CheckpointBundleReuse {
    if !path.exists() {
        return CheckpointBundleReuse::Miss(CheckpointCacheMissReason::NotFound);
    }
    match load_checkpoint_bundle(path) {
        Ok(bundle) if bundle.vm_semantic_epoch == CHECKPOINT_VM_SEMANTIC_EPOCH => {
            CheckpointBundleReuse::Hit(bundle)
        }
        Ok(_) => CheckpointBundleReuse::Miss(CheckpointCacheMissReason::Unreadable),
        Err(_) => CheckpointBundleReuse::Miss(CheckpointCacheMissReason::Unreadable),
    }
}

pub fn can_reuse_preamble(changed_files: &[Utf8PathBuf]) -> bool {
    !changed_files.iter().any(|path| {
        path.file_name()
            .is_some_and(|name| name == "00README" || name.starts_with("00README."))
            || matches!(path.extension(), Some("cls" | "sty" | "cfg" | "def"))
    })
}

pub fn select_reusable_preamble(
    bundle: &CheckpointBundle,
    changed_files: &[Utf8PathBuf],
    current_preamble_key: &str,
) -> Option<StoredCheckpoint> {
    if !can_reuse_preamble(changed_files) {
        return None;
    }

    bundle
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::Preamble
                && checkpoint.meta.boundary_hash == current_preamble_key
                && checkpoint_is_replay_safe(checkpoint)
        })
        .cloned()
}

pub fn checkpoint_is_replay_safe(checkpoint: &StoredCheckpoint) -> bool {
    checkpoint.meta.snapshot_attached
        && checkpoint.meta.continuation_safety.is_safe()
        && checkpoint.snapshot_for_restore().is_some_and(|restore| {
            let snapshot = restore.state();
            let mut interner = ControlSequenceInterner::new();
            snapshot.continuation_safety.is_safe()
                && (checkpoint.meta.kind != CheckpointKind::InputBoundary
                    || snapshot
                        .input_continuation
                        .as_ref()
                        .is_none_or(tex_vm::VmInputContinuationSnapshot::is_restorable))
                && Vm::try_restore(&mut interner, snapshot).is_ok()
        })
}

pub fn checkpoint_reuse_diagnostic(
    checkpoint: &StoredCheckpoint,
) -> Option<CheckpointReuseDiagnostic> {
    if checkpoint_is_replay_safe(checkpoint) {
        return None;
    }
    let blockers = if checkpoint.meta.continuation_safety.blockers.is_empty() {
        checkpoint
            .snapshot_for_restore()
            .map(|restore| restore.state().continuation_safety.blockers.clone())
            .unwrap_or_else(|| vec![VmContinuationBlocker::UnverifiedSnapshot])
    } else {
        checkpoint.meta.continuation_safety.blockers.clone()
    };
    Some(CheckpointReuseDiagnostic {
        code: CHECKPOINT_UNSAFE_STATE,
        checkpoint_id: checkpoint.meta.checkpoint_id.clone(),
        blockers,
    })
}

pub fn load_latest_reusable_preamble(
    build_root: &Utf8Path,
    current_rev: u64,
    changed_files: &[Utf8PathBuf],
    current_preamble_key: &str,
) -> Result<Option<StoredCheckpoint>> {
    if current_rev <= 1 || !can_reuse_preamble(changed_files) {
        return Ok(None);
    }

    for rev in (1..current_rev).rev() {
        let path = build_root.join(format!("rev-{rev}/checkpoints.json"));
        if !path.exists() {
            continue;
        }
        let CheckpointBundleReuse::Hit(bundle) = load_checkpoint_bundle_for_reuse(&path) else {
            continue;
        };
        if let Some(checkpoint) =
            select_reusable_preamble(&bundle, changed_files, current_preamble_key)
        {
            return Ok(Some(checkpoint));
        }
    }

    Ok(None)
}

pub fn preamble_key_for_source(source: &str) -> String {
    blake3::hash(normalize_preamble(source).as_bytes())
        .to_hex()
        .to_string()
}

pub fn find_unchanged_tail(
    bundle: &CheckpointBundle,
    current_pages: &[CheckpointPage],
) -> Option<TailRealignment> {
    if bundle.pages.is_empty() || current_pages.is_empty() {
        return None;
    }

    let mut matched_pages = 0usize;
    while matched_pages < bundle.pages.len() && matched_pages < current_pages.len() {
        let previous = &bundle.pages[bundle.pages.len() - 1 - matched_pages];
        let current = &current_pages[current_pages.len() - 1 - matched_pages];
        if previous.content_hash != current.content_hash {
            break;
        }
        matched_pages += 1;
    }

    if matched_pages == 0 {
        return None;
    }

    let previous_page_start = bundle.pages.len() - matched_pages;
    let current_page_start = current_pages.len() - matched_pages;
    let resume_checkpoint = bundle
        .checkpoints
        .iter()
        .find(|checkpoint| checkpoint.meta.page_index_after == previous_page_start)?;
    let previous_rev = bundle
        .checkpoints
        .first()
        .map(|checkpoint| checkpoint.meta.rev)
        .unwrap_or_default();

    Some(TailRealignment {
        previous_rev,
        resume_checkpoint_id: resume_checkpoint.meta.checkpoint_id.clone(),
        previous_page_start,
        current_page_start,
        page_count: matched_pages,
    })
}

fn normalize_preamble(source: &str) -> String {
    source
        .split(r"\begin{document}")
        .next()
        .unwrap_or(source)
        .replace("\r\n", "\n")
}

fn checkpoint_id(
    kind: CheckpointKind,
    rev: u64,
    page_index_after: usize,
    boundary_hash: &str,
    vm_state_hash: &str,
) -> String {
    blake3::hash(
        format!("{kind:?}:{rev}:{page_index_after}:{boundary_hash}:{vm_state_hash}").as_bytes(),
    )
    .to_hex()
    .to_string()
}

fn page_boundary_hash(page: &CheckpointPage) -> String {
    blake3::hash(
        format!(
            "{}:{}:{}:{}:{}",
            page.page_id, page.index, page.content_hash, page.text_start_utf8, page.text_end_utf8
        )
        .as_bytes(),
    )
    .to_hex()
    .to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;

    use camino::Utf8PathBuf;
    use tempfile::tempdir;
    use tex_tokens::ControlSequenceInterner;
    use tex_vm::{
        SnapshotCapability, Vm, VmCodeTableAssignmentV1, VmCodeTableStateV1, VmContinuationBlocker,
        VmModuleCheckpointKind, VmReplayFrame, compile_format_snapshot,
    };

    use super::{
        CheckpointAttachmentCounts, CheckpointBundle, CheckpointBundleReuse,
        CheckpointBundleWriteWithPolicy, CheckpointCacheMissReason, CheckpointKind, CheckpointPage,
        CheckpointSuppressionCounts, CheckpointUncompressedSizeLimitExceeded,
        CheckpointWriteFailureReason, CheckpointWriteOutcome, InputBoundaryCheckpoint,
        SNAPSHOT_WRITE_POLICY, ShipoutCheckpoint, SnapshotAttachment, SnapshotWriteMode,
        SnapshotWritePolicy, SnapshotWritePolicyObservation, StoredSnapshotAttachment,
        VersionedSnapshotSlot, build_checkpoint_bundle, build_checkpoint_bundle_with_shipouts,
        build_checkpoint_bundle_with_shipouts_and_policy_and_stats,
        build_checkpoint_bundle_with_shipouts_and_stats, build_checkpoint_bundle_with_snapshots,
        can_reuse_preamble, find_unchanged_tail, load_checkpoint_bundle,
        load_checkpoint_bundle_for_reuse, load_checkpoint_bundle_with_limit,
        load_latest_reusable_preamble, preamble_key_for_source, save_checkpoint_bundle,
        save_checkpoint_bundle_with_policy, save_checkpoint_bundle_with_policy_and_limit,
        save_checkpoint_bundle_with_policy_and_limit_and_stats, save_checkpoint_bundle_with_stats,
        select_reusable_preamble,
    };

    const MUSKIP_ALIAS_V1_CAPABILITY: &str = "eqtb.muskip.alias-v1";
    const MUSKIP_SCALAR_V1_CAPABILITY: &str = "eqtb.muskip.scalar-v1";

    #[test]
    fn legacy_only_write_policy_rejects_required_capabilities() {
        let required_capabilities =
            BTreeSet::from([SnapshotCapability::new("eqtb.muskip.scalar-v1")]);

        assert!(
            !SnapshotWritePolicy::LegacyOnly.allows(&required_capabilities),
            "capability-bearing state must not enter the legacy lane"
        );
        assert_eq!(
            serde_json::to_string(&SNAPSHOT_WRITE_POLICY).expect("serialize production policy"),
            r#""legacy_only""#
        );
        assert_eq!(
            serde_json::to_string(&SnapshotWritePolicyObservation::from(SNAPSHOT_WRITE_POLICY))
                .expect("serialize production policy observation"),
            r#""legacy_only""#
        );
        let future: SnapshotWritePolicyObservation =
            serde_json::from_str(r#""future_versioned_muskip""#)
                .expect("decode future writer policy observation");
        assert_eq!(
            future,
            SnapshotWritePolicyObservation::Other("future_versioned_muskip".to_string())
        );
        assert_eq!(
            serde_json::to_string(&future).expect("re-encode future policy observation"),
            r#""future_versioned_muskip""#
        );
    }

    #[test]
    fn versioned_write_policy_builds_a_canonical_attachment_without_enabling_default_writes() {
        let mut interner = ControlSequenceInterner::new();
        let mut vm = Vm::new(&mut interner);
        let outcome = vm.run_plain(r"\newmuskip\first\first=2.5mu");
        assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
        let snapshot = vm.snapshot();
        let policy = SnapshotWriteMode::Versioned {
            enabled_capabilities: &[MUSKIP_ALIAS_V1_CAPABILITY, MUSKIP_SCALAR_V1_CAPABILITY],
        };
        let bundle = build_checkpoint_bundle_with_shipouts_and_policy_and_stats(
            1,
            &snapshot,
            "preamble",
            0,
            &[],
            &[],
            &[],
            policy,
        )
        .expect("build versioned-policy checkpoint")
        .bundle;

        assert!(matches!(
            bundle.checkpoints[0].snapshot_attachment(),
            SnapshotAttachment::Versioned(_)
        ));
        let wire = serde_json::to_value(CheckpointBundleWriteWithPolicy {
            bundle: &bundle,
            policy,
        })
        .expect("serialize versioned-policy checkpoint");
        assert!(wire["checkpoints"][0]["snapshot"].is_null());
        assert!(wire["checkpoints"][0]["versioned_snapshot"].is_object());
        let decoded = serde_json::from_value::<super::CheckpointBundle>(wire)
            .expect("decode canonical versioned checkpoint");
        let restore = decoded.checkpoints[0]
            .snapshot_for_restore()
            .expect("versioned restore state");
        assert!(restore.is_versioned());
        assert_eq!(restore.state(), &snapshot);

        let mut default_output = Vec::new();
        let error = serde_json::to_writer(&mut default_output, &bundle)
            .expect_err("default writer policy must remain LegacyOnly");
        assert!(
            error
                .to_string()
                .contains("versioned snapshot writer is disabled")
        );
        assert!(default_output.is_empty());
    }

    #[test]
    fn versioned_write_policy_suppresses_capabilities_outside_its_allowlist() {
        let mut interner = ControlSequenceInterner::new();
        let mut vm = Vm::new(&mut interner);
        let outcome = vm.run_plain(r"\newmuskip\first\first=2.5mu");
        assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
        let snapshot = vm.snapshot();
        let build = build_checkpoint_bundle_with_shipouts_and_policy_and_stats(
            1,
            &snapshot,
            "preamble",
            0,
            &[],
            &[],
            &[],
            SnapshotWriteMode::Versioned {
                enabled_capabilities: &[MUSKIP_SCALAR_V1_CAPABILITY],
            },
        )
        .expect("build partially enabled checkpoint");
        assert_eq!(build.suppression_counts.unsafe_continuation, 0);
        assert_eq!(build.suppression_counts.unsupported_capabilities, 1);
        let bundle = build.bundle;

        assert!(!bundle.checkpoints[0].meta.snapshot_attached);
        assert!(matches!(
            bundle.checkpoints[0].snapshot_attachment(),
            SnapshotAttachment::None
        ));
    }

    #[test]
    fn production_stats_do_not_count_missing_shipout_candidates_as_suppression() {
        let mut interner = ControlSequenceInterner::new();
        let mut snapshot = compile_format_snapshot(&mut interner, r"\def\snapshotword{R}");
        snapshot
            .continuation_safety
            .blockers
            .push(VmContinuationBlocker::OpenGroup);
        let pages = [CheckpointPage {
            page_id: "page-1".to_string(),
            index: 0,
            content_hash: "page-hash".to_string(),
            text_start_utf8: 0,
            text_end_utf8: 1,
        }];

        let build = build_checkpoint_bundle_with_shipouts_and_stats(
            1,
            &snapshot,
            "preamble",
            0,
            &pages,
            &[],
            &[],
        )
        .expect("build unsafe preamble without shipout candidate");

        assert_eq!(build.suppression_counts.unsafe_continuation, 1);
        assert_eq!(build.suppression_counts.unsupported_capabilities, 0);
        assert_eq!(build.bundle.checkpoints.len(), 2);
        assert!(build.bundle.checkpoints.iter().all(|checkpoint| {
            matches!(checkpoint.snapshot_attachment(), SnapshotAttachment::None)
        }));
    }

    #[test]
    fn versioned_write_policy_saves_and_reloads_the_production_envelope() {
        let mut source_interner = ControlSequenceInterner::new();
        let mut source = Vm::new(&mut source_interner);
        let outcome = source.run_plain(r"\newmuskip\first\first=2.5mu");
        assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
        let snapshot = source.snapshot();
        let policy = SnapshotWriteMode::Versioned {
            enabled_capabilities: &[MUSKIP_ALIAS_V1_CAPABILITY, MUSKIP_SCALAR_V1_CAPABILITY],
        };
        let bundle = build_checkpoint_bundle_with_shipouts_and_policy_and_stats(
            1,
            &snapshot,
            "preamble",
            0,
            &[],
            &[],
            &[],
            policy,
        )
        .expect("build versioned-policy checkpoint")
        .bundle;
        drop(source);
        let tempdir = tempdir().expect("tempdir");
        let path = Utf8PathBuf::from_path_buf(tempdir.path().join("checkpoints.json"))
            .expect("UTF-8 checkpoint path");

        save_checkpoint_bundle_with_policy(&path, &bundle, policy)
            .expect("save versioned-policy checkpoint");
        let reloaded = load_checkpoint_bundle(&path).expect("load versioned-policy checkpoint");
        let restore = reloaded.checkpoints[0]
            .snapshot_for_restore()
            .expect("versioned restore state");
        let mut restored_interner = ControlSequenceInterner::new();
        let mut restored = Vm::try_restore(&mut restored_interner, restore.state())
            .expect("restore versioned-policy checkpoint");
        let replay = restored.run_plain(r"[\the\first]");

        assert!(restore.is_versioned());
        assert_eq!(replay.output, "[2.5mu]");
        assert!(replay.diagnostics.is_empty(), "{:#?}", replay.diagnostics);
    }

    #[test]
    fn versioned_write_policy_round_trips_shipout_and_input_boundary_categories() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot =
            compile_format_snapshot(&mut interner, r"\def\preambleword{legacy}");
        let mut source = Vm::new(&mut interner);
        let outcome =
            source.run_plain(r"\newmuskip\first\first=2.5mu\def\checkpointword{category}");
        assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
        let category_snapshot = source.snapshot();
        let policy = SnapshotWriteMode::Versioned {
            enabled_capabilities: &[MUSKIP_ALIAS_V1_CAPABILITY, MUSKIP_SCALAR_V1_CAPABILITY],
        };
        let pages = [CheckpointPage {
            page_id: "page-1".to_string(),
            index: 0,
            content_hash: "page-hash".to_string(),
            text_start_utf8: 0,
            text_end_utf8: 24,
        }];
        let build = build_checkpoint_bundle_with_shipouts_and_policy_and_stats(
            7,
            &preamble_snapshot,
            "preamble",
            11,
            &pages,
            &[ShipoutCheckpoint {
                snapshot: category_snapshot.clone(),
                source_offset_utf8: 37,
                resume_path: Some(Utf8PathBuf::from("sections/tail.tex")),
                continuation_stack: vec![VmReplayFrame {
                    path: Utf8PathBuf::from("main.tex"),
                    source_offset_utf8: 41,
                }],
            }],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/body.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 53,
                continuation_stack: vec![VmReplayFrame {
                    path: Utf8PathBuf::from("outer.tex"),
                    source_offset_utf8: 59,
                }],
                output_start_utf8: 17,
                page_index_after: 1,
                snapshot: category_snapshot,
            }],
            policy,
        )
        .expect("build category-complete versioned checkpoint");

        assert_eq!(
            build.suppression_counts,
            CheckpointSuppressionCounts::default()
        );
        assert_eq!(
            build.bundle.attachment_counts(),
            CheckpointAttachmentCounts {
                none: 0,
                legacy: 1,
                versioned: 2,
            }
        );
        let tempdir = tempdir().expect("tempdir");
        let path = Utf8PathBuf::from_path_buf(tempdir.path().join("checkpoints.json"))
            .expect("UTF-8 checkpoint path");
        save_checkpoint_bundle_with_policy(&path, &build.bundle, policy)
            .expect("save category-complete versioned checkpoint");
        let reloaded = load_checkpoint_bundle(&path).expect("reload versioned checkpoints");
        assert_eq!(
            reloaded.attachment_counts(),
            CheckpointAttachmentCounts {
                none: 0,
                legacy: 1,
                versioned: 2,
            }
        );

        let shipout = reloaded
            .checkpoints
            .iter()
            .find(|checkpoint| checkpoint.meta.kind == CheckpointKind::Shipout)
            .expect("shipout checkpoint");
        assert_eq!(shipout.meta.source_offset_utf8, 37);
        assert_eq!(
            shipout.meta.resume_path.as_deref(),
            Some(camino::Utf8Path::new("sections/tail.tex"))
        );
        let input = reloaded
            .checkpoints
            .iter()
            .find(|checkpoint| checkpoint.meta.kind == CheckpointKind::InputBoundary)
            .expect("input-boundary checkpoint");
        assert_eq!(input.meta.source_offset_utf8, 53);
        assert_eq!(input.meta.output_start_utf8, 17);
        assert_eq!(
            input.meta.module_path.as_deref(),
            Some(camino::Utf8Path::new("sections/body.tex"))
        );

        for checkpoint in [shipout, input] {
            let restore = checkpoint
                .snapshot_for_restore()
                .expect("versioned category restore state");
            assert!(restore.is_versioned());
            let mut restored_interner = ControlSequenceInterner::new();
            let mut restored = Vm::try_restore(&mut restored_interner, restore.state())
                .expect("restore versioned category checkpoint");
            let replay = restored.run_plain(r"[\the\first][\checkpointword]");
            assert_eq!(replay.output, "[2.5mu][category]");
            assert!(replay.diagnostics.is_empty(), "{:#?}", replay.diagnostics);
        }
    }

    #[test]
    fn versioned_write_policy_reports_category_capability_suppression() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot =
            compile_format_snapshot(&mut interner, r"\def\preambleword{legacy}");
        let mut source = Vm::new(&mut interner);
        let outcome = source.run_plain(r"\newmuskip\first\first=2.5mu");
        assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
        let category_snapshot = source.snapshot();
        let pages = [CheckpointPage {
            page_id: "page-1".to_string(),
            index: 0,
            content_hash: "page-hash".to_string(),
            text_start_utf8: 0,
            text_end_utf8: 24,
        }];
        let build = build_checkpoint_bundle_with_shipouts_and_policy_and_stats(
            8,
            &preamble_snapshot,
            "preamble",
            0,
            &pages,
            &[ShipoutCheckpoint {
                snapshot: category_snapshot.clone(),
                source_offset_utf8: 37,
                resume_path: None,
                continuation_stack: Vec::new(),
            }],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/body.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 53,
                continuation_stack: Vec::new(),
                output_start_utf8: 17,
                page_index_after: 1,
                snapshot: category_snapshot,
            }],
            SnapshotWriteMode::Versioned {
                enabled_capabilities: &[MUSKIP_SCALAR_V1_CAPABILITY],
            },
        )
        .expect("build partially enabled category checkpoints");

        assert_eq!(build.suppression_counts.unsafe_continuation, 0);
        assert_eq!(build.suppression_counts.unsupported_capabilities, 2);
        assert_eq!(
            build.bundle.attachment_counts(),
            CheckpointAttachmentCounts {
                none: 2,
                legacy: 1,
                versioned: 0,
            }
        );
        for kind in [CheckpointKind::Shipout, CheckpointKind::InputBoundary] {
            let checkpoint = build
                .bundle
                .checkpoints
                .iter()
                .find(|checkpoint| checkpoint.meta.kind == kind)
                .expect("category checkpoint");
            assert!(!checkpoint.meta.snapshot_attached);
            assert!(matches!(
                checkpoint.snapshot_attachment(),
                SnapshotAttachment::None
            ));
        }
    }

    #[test]
    fn versioned_write_policy_reports_category_continuation_suppression() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot =
            compile_format_snapshot(&mut interner, r"\def\preambleword{legacy}");
        let mut source = Vm::new(&mut interner);
        let outcome = source.run_plain(r"\newmuskip\first\first=2.5mu");
        assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
        let mut category_snapshot = source.snapshot();
        category_snapshot
            .continuation_safety
            .blockers
            .push(VmContinuationBlocker::OpenGroup);
        let pages = [CheckpointPage {
            page_id: "page-1".to_string(),
            index: 0,
            content_hash: "page-hash".to_string(),
            text_start_utf8: 0,
            text_end_utf8: 24,
        }];
        let build = build_checkpoint_bundle_with_shipouts_and_policy_and_stats(
            9,
            &preamble_snapshot,
            "preamble",
            0,
            &pages,
            &[ShipoutCheckpoint {
                snapshot: category_snapshot.clone(),
                source_offset_utf8: 37,
                resume_path: None,
                continuation_stack: Vec::new(),
            }],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/body.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 53,
                continuation_stack: Vec::new(),
                output_start_utf8: 17,
                page_index_after: 1,
                snapshot: category_snapshot,
            }],
            SnapshotWriteMode::Versioned {
                enabled_capabilities: &[MUSKIP_ALIAS_V1_CAPABILITY, MUSKIP_SCALAR_V1_CAPABILITY],
            },
        )
        .expect("build unsafe category checkpoints");

        assert_eq!(build.suppression_counts.unsafe_continuation, 2);
        assert_eq!(build.suppression_counts.unsupported_capabilities, 0);
        assert_eq!(
            build.bundle.attachment_counts(),
            CheckpointAttachmentCounts {
                none: 2,
                legacy: 1,
                versioned: 0,
            }
        );
        for kind in [CheckpointKind::Shipout, CheckpointKind::InputBoundary] {
            let checkpoint = build
                .bundle
                .checkpoints
                .iter()
                .find(|checkpoint| checkpoint.meta.kind == kind)
                .expect("category checkpoint");
            assert!(!checkpoint.meta.snapshot_attached);
            assert!(matches!(
                checkpoint.snapshot_attachment(),
                SnapshotAttachment::None
            ));
        }
    }

    #[test]
    fn capability_free_snapshot_cannot_be_forced_into_the_versioned_lane() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\snapshotword{R}");
        assert!(snapshot.required_capabilities().is_empty());
        let policy = SnapshotWriteMode::Versioned {
            enabled_capabilities: &[MUSKIP_ALIAS_V1_CAPABILITY, MUSKIP_SCALAR_V1_CAPABILITY],
        };
        let normal_bundle = build_checkpoint_bundle_with_shipouts_and_policy_and_stats(
            1,
            &snapshot,
            "preamble",
            0,
            &[],
            &[],
            &[],
            policy,
        )
        .expect("build capability-free checkpoint")
        .bundle;
        assert!(matches!(
            normal_bundle.checkpoints[0].snapshot_attachment(),
            SnapshotAttachment::Legacy(_)
        ));

        let mut unauthorized = normal_bundle.checkpoints[0].clone();
        unauthorized.attachment =
            StoredSnapshotAttachment::Versioned(VersionedSnapshotSlot::from_snapshot(snapshot));
        let error = match unauthorized.write_wire(policy) {
            Ok(_) => panic!("private policy must enforce exact stored-lane equality"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("does not enable required capabilities")
        );

        let mut checkpoint_output = Vec::new();
        serde_json::to_writer(&mut checkpoint_output, &unauthorized)
            .expect_err("default checkpoint writer must reject the versioned lane");
        assert!(checkpoint_output.is_empty());

        let bundle = CheckpointBundle {
            vm_semantic_epoch: super::CHECKPOINT_VM_SEMANTIC_EPOCH,
            checkpoints: vec![
                normal_bundle.checkpoints[0].clone(),
                unauthorized,
                normal_bundle.checkpoints[0].clone(),
            ],
            pages: Vec::new(),
        };
        let mut default_bundle_output = Vec::new();
        serde_json::to_writer(&mut default_bundle_output, &bundle)
            .expect_err("late unauthorized child must fail parent preflight");
        assert!(default_bundle_output.is_empty());

        let mut private_bundle_output = Vec::new();
        serde_json::to_writer(
            &mut private_bundle_output,
            &CheckpointBundleWriteWithPolicy {
                bundle: &bundle,
                policy,
            },
        )
        .expect_err("private full policy must reject a capability-free versioned lane");
        assert!(private_bundle_output.is_empty());

        let tempdir = tempdir().expect("tempdir");
        let path = Utf8PathBuf::from_path_buf(tempdir.path().join("checkpoints.json"))
            .expect("UTF-8 checkpoint path");
        fs::write(&path, b"sentinel").expect("write sentinel");
        let entries_before = fs::read_dir(tempdir.path())
            .expect("read tempdir before save")
            .count();
        let classified = save_checkpoint_bundle_with_policy_and_limit_and_stats(
            &path,
            &bundle,
            policy,
            super::MAX_CHECKPOINT_UNCOMPRESSED_BYTES,
        )
        .expect_err("classify exact-lane mismatch");
        assert_eq!(
            classified.reason(),
            CheckpointWriteFailureReason::LaneMismatch
        );
        save_checkpoint_bundle(&path, &bundle)
            .expect_err("public save must reject before touching the filesystem");
        assert_eq!(fs::read(&path).expect("read sentinel"), b"sentinel");
        assert_eq!(
            fs::read_dir(tempdir.path())
                .expect("read tempdir after save")
                .count(),
            entries_before
        );
    }

    #[test]
    fn versioned_slot_rejects_an_invalid_document_before_output() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\snapshotword{R}");
        let mut slot = VersionedSnapshotSlot::from_snapshot(snapshot);
        slot.document
            .required_capabilities
            .insert(SnapshotCapability::new(MUSKIP_SCALAR_V1_CAPABILITY));

        let mut output = Vec::new();
        serde_json::to_writer(&mut output, &slot)
            .expect_err("slot must validate its document before output");
        assert!(output.is_empty());

        let mut checkpoint = build_checkpoint_bundle(
            1,
            &compile_format_snapshot(&mut interner, r"\def\snapshotword{R}"),
            "preamble",
            &[],
        )
        .expect("base checkpoint")
        .checkpoints
        .remove(0);
        checkpoint.attachment = StoredSnapshotAttachment::Versioned(slot);
        let bundle = CheckpointBundle {
            vm_semantic_epoch: super::CHECKPOINT_VM_SEMANTIC_EPOCH,
            checkpoints: vec![checkpoint],
            pages: Vec::new(),
        };
        let tempdir = tempdir().expect("tempdir");
        let path = Utf8PathBuf::from_path_buf(tempdir.path().join("invalid.json"))
            .expect("UTF-8 invalid path");
        let classified = save_checkpoint_bundle_with_policy_and_limit_and_stats(
            &path,
            &bundle,
            SnapshotWriteMode::Versioned {
                enabled_capabilities: &[MUSKIP_SCALAR_V1_CAPABILITY],
            },
            super::MAX_CHECKPOINT_UNCOMPRESSED_BYTES,
        )
        .expect_err("classify invalid versioned document");
        assert_eq!(
            classified.reason(),
            CheckpointWriteFailureReason::InvalidDocument
        );
        assert!(!path.exists());
    }

    #[test]
    fn builds_preamble_and_shipout_checkpoints() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            7,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[CheckpointPage {
                page_id: "p1".to_string(),
                index: 0,
                content_hash: "hash".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
            }],
        )
        .expect("checkpoint bundle");

        assert_eq!(bundle.checkpoints.len(), 2);
        assert_eq!(bundle.pages.len(), 1);
        assert_eq!(bundle.checkpoints[0].meta.kind, CheckpointKind::Preamble);
        assert!(bundle.checkpoints[0].snapshot_for_restore().is_some());
        assert_eq!(bundle.checkpoints[1].meta.kind, CheckpointKind::Shipout);
        assert!(bundle.checkpoints[1].snapshot_for_restore().is_none());
    }

    #[test]
    fn saves_and_loads_checkpoint_bundle() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            1,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[],
        )
        .expect("checkpoint bundle");
        let tempdir = tempdir().expect("tempdir");
        let path =
            Utf8PathBuf::from_path_buf(tempdir.path().join("checkpoints.json")).expect("utf8");

        save_checkpoint_bundle(&path, &bundle).expect("save");
        let loaded = load_checkpoint_bundle(&path).expect("load");

        assert_eq!(loaded, bundle);
    }

    #[test]
    fn save_and_read_share_the_uncompressed_payload_limit() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            1,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[],
        )
        .expect("checkpoint bundle");
        let policy = SnapshotWriteMode::LegacyOnly;
        let exact_limit = u64::try_from(
            serde_json::to_vec(&CheckpointBundleWriteWithPolicy {
                bundle: &bundle,
                policy,
            })
            .expect("serialize checkpoint payload")
            .len(),
        )
        .expect("payload length fits u64");
        let tempdir = tempdir().expect("tempdir");
        let exact_path = Utf8PathBuf::from_path_buf(tempdir.path().join("exact.json"))
            .expect("UTF-8 exact path");

        save_checkpoint_bundle_with_policy_and_limit(&exact_path, &bundle, policy, exact_limit)
            .expect("save payload exactly at limit");
        assert_eq!(
            load_checkpoint_bundle_with_limit(&exact_path, exact_limit)
                .expect("read payload exactly at limit"),
            bundle
        );

        let rejected_path = Utf8PathBuf::from_path_buf(tempdir.path().join("rejected.json"))
            .expect("UTF-8 rejected path");
        fs::write(&rejected_path, b"sentinel").expect("write sentinel target");
        let entries_before = fs::read_dir(tempdir.path())
            .expect("read tempdir before rejected save")
            .count();
        let error = save_checkpoint_bundle_with_policy_and_limit(
            &rejected_path,
            &bundle,
            policy,
            exact_limit - 1,
        )
        .expect_err("reject payload one byte above limit");
        let error_message = format!("{error:#}");
        assert!(
            error_message.contains("uncompressed") && error_message.contains("exceeding"),
            "{error_message}"
        );
        assert_eq!(
            fs::read(&rejected_path).expect("read preserved sentinel"),
            b"sentinel"
        );
        assert_eq!(
            fs::read_dir(tempdir.path())
                .expect("read tempdir after rejected save")
                .count(),
            entries_before,
            "rejected size admission leaked a temporary file"
        );
        let classified = save_checkpoint_bundle_with_policy_and_limit_and_stats(
            &rejected_path,
            &bundle,
            policy,
            exact_limit - 1,
        )
        .expect_err("classify payload one byte above limit");
        assert_eq!(classified.reason(), CheckpointWriteFailureReason::SizeLimit);
        assert!(
            classified
                .source
                .downcast_ref::<CheckpointUncompressedSizeLimitExceeded>()
                .is_some()
        );
    }

    #[test]
    fn checkpoint_save_reports_typed_success_and_failure_outcomes() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle =
            build_checkpoint_bundle(1, &snapshot, "preamble", &[]).expect("checkpoint bundle");
        let tempdir = tempdir().expect("tempdir");
        let success_path = Utf8PathBuf::from_path_buf(tempdir.path().join("success.json"))
            .expect("UTF-8 success path");

        let stats = save_checkpoint_bundle_with_stats(&success_path, &bundle)
            .expect("save with typed success stats");
        assert!(stats.uncompressed_bytes > 0);
        assert!(stats.persisted_bytes > 0);
        assert_eq!(
            serde_json::to_value(CheckpointWriteOutcome::from(stats))
                .expect("serialize successful write outcome"),
            serde_json::json!({
                "status": "success",
                "uncompressed_bytes": stats.uncompressed_bytes,
                "persisted_bytes": stats.persisted_bytes,
            })
        );

        let missing_parent =
            Utf8PathBuf::from_path_buf(tempdir.path().join("missing-parent/checkpoints.json"))
                .expect("UTF-8 missing-parent path");
        let tempfile_error = save_checkpoint_bundle_with_stats(&missing_parent, &bundle)
            .expect_err("classify temporary-file creation failure");
        assert_eq!(
            tempfile_error.reason(),
            CheckpointWriteFailureReason::Tempfile
        );

        let directory_target = Utf8PathBuf::from_path_buf(tempdir.path().join("directory-target"))
            .expect("UTF-8 directory target");
        fs::create_dir(&directory_target).expect("create persist-failure target");
        let persist_error = save_checkpoint_bundle_with_stats(&directory_target, &bundle)
            .expect_err("classify atomic persist failure");
        assert_eq!(
            persist_error.reason(),
            CheckpointWriteFailureReason::Persist
        );
        assert_eq!(
            serde_json::to_value(CheckpointWriteOutcome::Failure {
                reason: persist_error.reason(),
            })
            .expect("serialize failed write outcome"),
            serde_json::json!({
                "status": "failure",
                "reason": "persist",
            })
        );
    }

    #[test]
    fn saves_checkpoint_bundle_in_compact_versioned_envelope() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(
            &mut interner,
            r"\def\foo{a deliberately repeated checkpoint payload}",
        );
        let mut bundle = build_checkpoint_bundle(
            1,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[],
        )
        .expect("checkpoint bundle");
        let prototype = bundle.checkpoints[0].clone();
        for index in 1..=8 {
            let mut checkpoint = prototype.clone();
            checkpoint.meta.checkpoint_id = format!("repeated-{index}");
            bundle.checkpoints.push(checkpoint);
        }
        let uncompressed = serde_json::to_vec_pretty(&bundle).expect("serialize raw bundle");
        let tempdir = tempdir().expect("tempdir");
        let path =
            Utf8PathBuf::from_path_buf(tempdir.path().join("checkpoints.json")).expect("utf8");

        save_checkpoint_bundle(&path, &bundle).expect("save");

        let persisted = fs::read(&path).expect("read envelope");
        let envelope =
            serde_json::from_slice::<serde_json::Value>(&persisted).expect("parse envelope");
        assert_eq!(envelope["schema_version"], 2);
        assert_eq!(envelope["encoding"], "gzip+base64");
        assert!(
            envelope["payload"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        assert!(
            envelope["uncompressed_len"]
                .as_u64()
                .is_some_and(|value| value > 0)
        );
        assert!(
            envelope["uncompressed_blake3"]
                .as_str()
                .is_some_and(|value| value.len() == 64)
        );
        assert!(persisted.len() * 2 < uncompressed.len());
        assert_eq!(load_checkpoint_bundle(&path).expect("load"), bundle);
    }

    #[test]
    fn rejects_checkpoint_envelope_with_wrong_integrity_hash() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            1,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[],
        )
        .expect("checkpoint bundle");
        let tempdir = tempdir().expect("tempdir");
        let path =
            Utf8PathBuf::from_path_buf(tempdir.path().join("checkpoints.json")).expect("utf8");
        save_checkpoint_bundle(&path, &bundle).expect("save");
        let mut envelope =
            serde_json::from_slice::<serde_json::Value>(&fs::read(&path).expect("read envelope"))
                .expect("parse envelope");
        envelope["uncompressed_blake3"] = serde_json::Value::String("0".repeat(64));
        fs::write(
            &path,
            serde_json::to_vec(&envelope).expect("serialize corrupt envelope"),
        )
        .expect("write corrupt envelope");

        let error = load_checkpoint_bundle(&path).expect_err("integrity mismatch");

        assert!(error.to_string().contains("integrity hash mismatch"));
    }

    #[test]
    fn rejects_checkpoint_envelope_over_uncompressed_size_limit() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            1,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[],
        )
        .expect("checkpoint bundle");
        let tempdir = tempdir().expect("tempdir");
        let path =
            Utf8PathBuf::from_path_buf(tempdir.path().join("checkpoints.json")).expect("utf8");
        save_checkpoint_bundle(&path, &bundle).expect("save");
        let mut envelope =
            serde_json::from_slice::<serde_json::Value>(&fs::read(&path).expect("read envelope"))
                .expect("parse envelope");
        envelope["uncompressed_len"] = serde_json::Value::from(u64::MAX);
        fs::write(
            &path,
            serde_json::to_vec(&envelope).expect("serialize oversized envelope"),
        )
        .expect("write oversized envelope");

        let error = load_checkpoint_bundle(&path).expect_err("oversized payload");

        assert!(error.to_string().contains("exceeding"));
    }

    #[test]
    fn loads_legacy_checkpoint_bundle_without_pages() {
        let tempdir = tempdir().expect("tempdir");
        let path = Utf8PathBuf::from_path_buf(tempdir.path().join("legacy-checkpoints.json"))
            .expect("utf8");
        fs::write(
            &path,
            r#"{
  "checkpoints": [
    {
      "meta": {
        "checkpoint_id": "cp0",
        "kind": "preamble",
        "rev": 1,
        "page_index_after": 0,
        "boundary_hash": "legacy",
        "vm_state_hash": "vm",
        "snapshot_attached": false
      },
      "snapshot": null
    }
  ]
}"#,
        )
        .expect("write legacy json");

        let bundle = load_checkpoint_bundle(&path).expect("load legacy bundle");

        assert_eq!(bundle.checkpoints.len(), 1);
        assert!(bundle.pages.is_empty());
    }

    #[test]
    fn checkpoint_ids_are_stable_for_same_input() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let page = CheckpointPage {
            page_id: "p1".to_string(),
            index: 0,
            content_hash: "hash".to_string(),
            text_start_utf8: 0,
            text_end_utf8: 10,
        };

        let left = build_checkpoint_bundle(
            9,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            std::slice::from_ref(&page),
        )
        .expect("left");
        let right = build_checkpoint_bundle(
            9,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            std::slice::from_ref(&page),
        )
        .expect("right");

        assert_eq!(left, right);
    }

    #[test]
    fn vm_semantic_hash_is_stable_policy_independent_and_state_complete() {
        let mut first_interner = ControlSequenceInterner::new();
        let first = compile_format_snapshot(
            &mut first_interner,
            r"\def\zeta{Z}\def\alpha{A}\def\middle{M}",
        );
        let mut second_interner = ControlSequenceInterner::new();
        let second = compile_format_snapshot(
            &mut second_interner,
            r"\def\zeta{Z}\def\alpha{A}\def\middle{M}",
        );
        assert_eq!(first, second);
        let first_hash = super::checkpoint_vm_semantic_hash(&first).expect("hash first snapshot");
        let second_hash =
            super::checkpoint_vm_semantic_hash(&second).expect("hash second snapshot");
        assert_eq!(first_hash, second_hash);

        let mut first_muskip_interner = ControlSequenceInterner::new();
        let mut first_muskip_vm = Vm::new(&mut first_muskip_interner);
        let outcome = first_muskip_vm
            .run_plain(r"\def\zeta{Z}\def\alpha{A}\newmuskip\first\first=2.5mu\muskipdef\fixed=17");
        assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
        let first_muskip = first_muskip_vm.snapshot();
        let mut second_muskip_interner = ControlSequenceInterner::new();
        let mut second_muskip_vm = Vm::new(&mut second_muskip_interner);
        let outcome = second_muskip_vm
            .run_plain(r"\def\zeta{Z}\def\alpha{A}\newmuskip\first\first=2.5mu\muskipdef\fixed=17");
        assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
        let second_muskip = second_muskip_vm.snapshot();
        assert_eq!(first_muskip, second_muskip);
        let first_muskip_hash =
            super::checkpoint_vm_semantic_hash(&first_muskip).expect("hash first muskip snapshot");
        let second_muskip_hash = super::checkpoint_vm_semantic_hash(&second_muskip)
            .expect("hash second muskip snapshot");
        assert_eq!(first_muskip_hash, second_muskip_hash);
        assert_eq!(
            (first_hash.as_str(), first_muskip_hash.as_str()),
            (
                "55d9206986dbe96fb89a79e06b363b5d4d104f3b77e28b07d512015cdbd29b06",
                "ed142f79e88c2bcff4311ad59c5fce98f18b64ec594800e91c783cf3ab5bd696",
            )
        );

        let mut scalar_changed = first_muskip.clone();
        *scalar_changed
            .muskip_registers
            .values_mut()
            .next()
            .expect("allocated muskip scalar") += 1;
        let mut cursor_changed = first_muskip.clone();
        cursor_changed.next_muskip_register += 1;
        let mut legacy_changed = first_muskip.clone();
        legacy_changed.registers.insert(404, 7);
        for changed in [scalar_changed, cursor_changed, legacy_changed] {
            assert_ne!(
                first_muskip_hash,
                super::checkpoint_vm_semantic_hash(&changed).expect("hash changed snapshot")
            );
        }
        let mut alias_changed_interner = ControlSequenceInterner::new();
        let mut alias_changed_vm = Vm::new(&mut alias_changed_interner);
        let outcome = alias_changed_vm
            .run_plain(r"\def\zeta{Z}\def\alpha{A}\newmuskip\first\first=2.5mu\muskipdef\fixed=18");
        assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
        assert_ne!(
            first_muskip_hash,
            super::checkpoint_vm_semantic_hash(&alias_changed_vm.snapshot())
                .expect("hash alias-changed snapshot")
        );

        let production = build_checkpoint_bundle(10, &first_muskip, "preamble", &[])
            .expect("build suppressed production checkpoint");
        let candidate = build_checkpoint_bundle_with_shipouts_and_policy_and_stats(
            10,
            &first_muskip,
            "preamble",
            0,
            &[],
            &[],
            &[],
            SnapshotWriteMode::Versioned {
                enabled_capabilities: &[MUSKIP_ALIAS_V1_CAPABILITY, MUSKIP_SCALAR_V1_CAPABILITY],
            },
        )
        .expect("build private versioned checkpoint")
        .bundle;
        assert_eq!(
            production.checkpoints[0].meta.vm_state_hash,
            candidate.checkpoints[0].meta.vm_state_hash,
            "writer routing changed semantic identity"
        );
    }

    #[test]
    fn vm_semantic_hash_distinguishes_passive_code_table_state() {
        let mut interner = ControlSequenceInterner::new();
        let mut first = Vm::new(&mut interner).snapshot();
        first.mathcode_state = Some(VmCodeTableStateV1 {
            layers: vec![vec![VmCodeTableAssignmentV1 {
                character: b'A',
                value: 100,
            }]],
        });
        first.delcode_state = Some(VmCodeTableStateV1 {
            layers: vec![vec![VmCodeTableAssignmentV1 {
                character: b'.',
                value: -1,
            }]],
        });

        let mut math_changed = first.clone();
        math_changed
            .mathcode_state
            .as_mut()
            .expect("mathcode state")
            .layers[0][0]
            .value += 1;
        let mut del_changed = first.clone();
        del_changed
            .delcode_state
            .as_mut()
            .expect("delcode state")
            .layers[0][0]
            .value -= 1;

        let first_hash = super::checkpoint_vm_semantic_hash(&first).expect("hash code tables");
        assert_ne!(
            first_hash,
            super::checkpoint_vm_semantic_hash(&math_changed).expect("hash changed mathcode")
        );
        assert_ne!(
            first_hash,
            super::checkpoint_vm_semantic_hash(&del_changed).expect("hash changed delcode")
        );
    }

    #[test]
    fn shipout_boundary_hash_changes_with_page_content() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let left = build_checkpoint_bundle(
            9,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[CheckpointPage {
                page_id: "p1".to_string(),
                index: 0,
                content_hash: "hash-a".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
            }],
        )
        .expect("left");
        let right = build_checkpoint_bundle(
            9,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[CheckpointPage {
                page_id: "p1".to_string(),
                index: 0,
                content_hash: "hash-b".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
            }],
        )
        .expect("right");

        assert_ne!(
            left.checkpoints[1].meta.boundary_hash,
            right.checkpoints[1].meta.boundary_hash
        );
        assert_ne!(
            left.checkpoints[1].meta.checkpoint_id,
            right.checkpoints[1].meta.checkpoint_id
        );
    }

    #[test]
    fn stores_shipout_snapshots_with_source_offsets() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let shipout_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{page}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            5,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            19,
            &[CheckpointPage {
                page_id: "p0".to_string(),
                index: 0,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
            }],
            std::slice::from_ref(&shipout_snapshot),
            &[47],
            &[],
        )
        .expect("bundle");

        assert_eq!(bundle.checkpoints[0].meta.source_offset_utf8, 19);
        assert!(bundle.checkpoints[0].snapshot_for_restore().is_some());
        assert!(bundle.checkpoints[1].meta.snapshot_attached);
        assert_eq!(bundle.checkpoints[1].meta.source_offset_utf8, 47);
        assert_eq!(
            bundle.checkpoints[1]
                .snapshot_for_restore()
                .map(|restore| restore.state()),
            Some(&shipout_snapshot)
        );
    }

    #[test]
    fn stores_shipout_resume_metadata() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let shipout_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{page}");
        let bundle = build_checkpoint_bundle_with_shipouts(
            5,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            19,
            &[CheckpointPage {
                page_id: "p0".to_string(),
                index: 0,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
            }],
            &[ShipoutCheckpoint {
                snapshot: shipout_snapshot.clone(),
                source_offset_utf8: 47,
                resume_path: Some(Utf8PathBuf::from("sections/tail.tex")),
                continuation_stack: vec![VmReplayFrame {
                    path: Utf8PathBuf::from("main.tex"),
                    source_offset_utf8: 61,
                }],
            }],
            &[],
        )
        .expect("bundle");

        assert_eq!(
            bundle.checkpoints[1].meta.resume_path.as_ref(),
            Some(&Utf8PathBuf::from("sections/tail.tex"))
        );
        assert_eq!(
            bundle.checkpoints[1].meta.continuation_stack,
            vec![VmReplayFrame {
                path: Utf8PathBuf::from("main.tex"),
                source_offset_utf8: 61,
            }]
        );
    }

    #[test]
    fn stores_input_boundary_checkpoints_with_module_path() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{pre}");
        let input_snapshot = compile_format_snapshot(&mut interner, r"\def\fmt{input}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            6,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[CheckpointPage {
                page_id: "p0".to_string(),
                index: 0,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
            }],
            &[],
            &[],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/tail.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 14,
                continuation_stack: vec![VmReplayFrame {
                    path: Utf8PathBuf::from("outer.tex"),
                    source_offset_utf8: 28,
                }],
                output_start_utf8: 12,
                page_index_after: 0,
                snapshot: input_snapshot.clone(),
            }],
        )
        .expect("bundle");

        let input_checkpoint = bundle
            .checkpoints
            .iter()
            .find(|checkpoint| checkpoint.meta.kind == CheckpointKind::InputBoundary)
            .expect("input checkpoint");
        assert_eq!(
            input_checkpoint.meta.module_path.as_ref(),
            Some(&Utf8PathBuf::from("sections/tail.tex"))
        );
        assert_eq!(
            input_checkpoint.meta.resume_path.as_ref(),
            Some(&Utf8PathBuf::from("main.tex"))
        );
        assert_eq!(
            input_checkpoint.meta.input_boundary_kind,
            Some(VmModuleCheckpointKind::Enter)
        );
        assert_eq!(
            input_checkpoint.meta.continuation_stack,
            vec![VmReplayFrame {
                path: Utf8PathBuf::from("outer.tex"),
                source_offset_utf8: 28,
            }]
        );
        assert_eq!(input_checkpoint.meta.source_offset_utf8, 14);
        assert_eq!(input_checkpoint.meta.output_start_utf8, 12);
        assert_eq!(
            input_checkpoint
                .snapshot_for_restore()
                .map(|restore| restore.state()),
            Some(&input_snapshot)
        );
    }

    #[test]
    fn preamble_reuse_policy_rejects_style_and_manifest_changes() {
        assert!(can_reuse_preamble(&[Utf8PathBuf::from("main.tex")]));
        assert!(can_reuse_preamble(&[Utf8PathBuf::from(
            "sections/body.tex"
        )]));
        assert!(!can_reuse_preamble(&[Utf8PathBuf::from("article.cls")]));
        assert!(!can_reuse_preamble(&[Utf8PathBuf::from("pkg.sty")]));
        assert!(!can_reuse_preamble(&[Utf8PathBuf::from("00README.yaml")]));
    }

    #[test]
    fn selects_preamble_checkpoint_when_changes_are_body_only() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let preamble_key = preamble_key_for_source(r"\documentclass{article}");
        let bundle = build_checkpoint_bundle(3, &snapshot, &preamble_key, &[]).expect("bundle");

        let selected =
            select_reusable_preamble(&bundle, &[Utf8PathBuf::from("main.tex")], &preamble_key)
                .expect("selected checkpoint");
        assert_eq!(selected.meta.kind, CheckpointKind::Preamble);
        assert!(selected.snapshot_for_restore().is_some());
    }

    #[test]
    fn preamble_key_ignores_body_changes() {
        let left = preamble_key_for_source("\\documentclass{article}\\begin{document}left body");
        let right = preamble_key_for_source("\\documentclass{article}\\begin{document}right body");

        assert_eq!(left, right);
    }

    #[test]
    fn preamble_key_changes_when_preamble_changes() {
        let left = preamble_key_for_source("\\documentclass{article}\\title{A}\\begin{document}");
        let right = preamble_key_for_source("\\documentclass{article}\\title{B}\\begin{document}");

        assert_ne!(left, right);
    }

    #[test]
    fn loads_latest_reusable_preamble_from_previous_revision() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let tempdir = tempdir().expect("tempdir");
        let build_root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8");
        let preamble_key = preamble_key_for_source(r"\documentclass{article}");
        fs::create_dir_all(build_root.join("rev-1")).expect("rev-1");
        fs::create_dir_all(build_root.join("rev-2")).expect("rev-2");
        save_checkpoint_bundle(
            &build_root.join("rev-1/checkpoints.json"),
            &build_checkpoint_bundle(1, &snapshot, &preamble_key, &[]).expect("bundle 1"),
        )
        .expect("save rev1");
        save_checkpoint_bundle(
            &build_root.join("rev-2/checkpoints.json"),
            &build_checkpoint_bundle(2, &snapshot, &preamble_key, &[]).expect("bundle 2"),
        )
        .expect("save rev2");

        let selected = load_latest_reusable_preamble(
            &build_root,
            3,
            &[Utf8PathBuf::from("main.tex")],
            &preamble_key,
        )
        .expect("load latest")
        .expect("selected");

        assert_eq!(selected.meta.rev, 2);
        assert_eq!(selected.meta.kind, CheckpointKind::Preamble);
    }

    #[test]
    fn skips_corrupt_newer_checkpoint_bundle_when_loading_reusable_preamble() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let tempdir = tempdir().expect("tempdir");
        let build_root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8");
        let preamble_key = preamble_key_for_source(r"\documentclass{article}");
        fs::create_dir_all(build_root.join("rev-1")).expect("rev-1");
        fs::create_dir_all(build_root.join("rev-2")).expect("rev-2");
        save_checkpoint_bundle(
            &build_root.join("rev-1/checkpoints.json"),
            &build_checkpoint_bundle(1, &snapshot, &preamble_key, &[]).expect("bundle 1"),
        )
        .expect("save rev1");
        fs::write(build_root.join("rev-2/checkpoints.json"), b"{truncated")
            .expect("write corrupt rev2");

        let selected = load_latest_reusable_preamble(
            &build_root,
            3,
            &[Utf8PathBuf::from("main.tex")],
            &preamble_key,
        )
        .expect("load older valid bundle")
        .expect("selected");

        assert_eq!(selected.meta.rev, 1);
    }

    #[test]
    fn classifies_missing_checkpoint_bundle_as_reuse_miss() {
        let tempdir = tempdir().expect("tempdir");
        let missing =
            Utf8PathBuf::from_path_buf(tempdir.path().join("missing.json")).expect("utf8 path");

        assert_eq!(
            load_checkpoint_bundle_for_reuse(&missing),
            CheckpointBundleReuse::Miss(CheckpointCacheMissReason::NotFound)
        );
    }

    #[test]
    fn classifies_corrupt_checkpoint_bundle_as_reuse_miss() {
        let tempdir = tempdir().expect("tempdir");
        let corrupt =
            Utf8PathBuf::from_path_buf(tempdir.path().join("corrupt.json")).expect("utf8 path");
        fs::write(&corrupt, b"{truncated").expect("write corrupt checkpoint");

        assert_eq!(
            load_checkpoint_bundle_for_reuse(&corrupt),
            CheckpointBundleReuse::Miss(CheckpointCacheMissReason::Unreadable)
        );
    }

    #[test]
    fn rejects_reuse_when_current_preamble_key_differs() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let tempdir = tempdir().expect("tempdir");
        let build_root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8");
        let old_key = preamble_key_for_source(r"\documentclass{article}\title{A}");
        let new_key = preamble_key_for_source(r"\documentclass{article}\title{B}");
        fs::create_dir_all(build_root.join("rev-1")).expect("rev-1");
        save_checkpoint_bundle(
            &build_root.join("rev-1/checkpoints.json"),
            &build_checkpoint_bundle(1, &snapshot, &old_key, &[]).expect("bundle 1"),
        )
        .expect("save rev1");

        let selected = load_latest_reusable_preamble(
            &build_root,
            2,
            &[Utf8PathBuf::from("main.tex")],
            &new_key,
        )
        .expect("load latest");

        assert!(selected.is_none());
    }

    #[test]
    fn finds_shifted_unchanged_tail_against_previous_pages() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            4,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "p2".to_string(),
                    index: 2,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
                CheckpointPage {
                    page_id: "p3".to_string(),
                    index: 3,
                    content_hash: "old-3".to_string(),
                    text_start_utf8: 30,
                    text_end_utf8: 40,
                },
            ],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "inserted".to_string(),
                    index: 1,
                    content_hash: "inserted".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "new-2".to_string(),
                    index: 2,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
                CheckpointPage {
                    page_id: "new-3".to_string(),
                    index: 3,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 30,
                    text_end_utf8: 40,
                },
                CheckpointPage {
                    page_id: "new-4".to_string(),
                    index: 4,
                    content_hash: "old-3".to_string(),
                    text_start_utf8: 40,
                    text_end_utf8: 50,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 4);
        assert_eq!(tail.previous_page_start, 1);
        assert_eq!(tail.current_page_start, 2);
        assert_eq!(tail.page_count, 3);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[1].meta.checkpoint_id
        );
    }

    #[test]
    fn returns_none_when_no_tail_pages_match() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            1,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[CheckpointPage {
                page_id: "p0".to_string(),
                index: 0,
                content_hash: "old-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
            }],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[CheckpointPage {
                page_id: "new-0".to_string(),
                index: 0,
                content_hash: "new-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
            }],
        );

        assert!(tail.is_none());
    }

    #[test]
    fn returns_none_when_previous_bundle_has_no_pages() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            6,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[CheckpointPage {
                page_id: "new-0".to_string(),
                index: 0,
                content_hash: "hash-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
            }],
        );

        assert!(tail.is_none());
    }

    #[test]
    fn returns_none_when_current_document_has_no_pages() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            7,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[CheckpointPage {
                page_id: "p0".to_string(),
                index: 0,
                content_hash: "old-0".to_string(),
                text_start_utf8: 0,
                text_end_utf8: 10,
            }],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(&bundle, &[]);

        assert!(tail.is_none());
    }

    #[test]
    fn returns_none_when_current_document_only_preserves_prefix() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            9,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "new-2".to_string(),
                    index: 2,
                    content_hash: "appended".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
        );

        assert!(tail.is_none());
    }

    #[test]
    fn returns_full_tail_when_all_pages_match() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            2,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 2);
        assert_eq!(tail.previous_page_start, 0);
        assert_eq!(tail.current_page_start, 0);
        assert_eq!(tail.page_count, 2);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[0].meta.checkpoint_id
        );
    }

    #[test]
    fn prefers_preamble_resume_checkpoint_over_page_zero_input_boundary_for_full_tail() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{input}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            21,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\foo{a}"),
                compile_format_snapshot(&mut interner, r"\def\foo{b}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/frontmatter.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 0,
                continuation_stack: Vec::new(),
                output_start_utf8: 0,
                page_index_after: 0,
                snapshot: input_snapshot,
            }],
        )
        .expect("bundle");
        let page_zero_input_checkpoint_id = bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.page_index_after == 0
            })
            .expect("page-zero input boundary checkpoint")
            .meta
            .checkpoint_id
            .clone();

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 21);
        assert_eq!(tail.previous_page_start, 0);
        assert_eq!(tail.current_page_start, 0);
        assert_eq!(tail.page_count, 2);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[0].meta.checkpoint_id
        );
        assert_ne!(tail.resume_checkpoint_id, page_zero_input_checkpoint_id);
    }

    #[test]
    fn falls_back_to_page_zero_input_boundary_when_preamble_checkpoint_is_missing() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{input}");
        let mut bundle = build_checkpoint_bundle_with_snapshots(
            22,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\foo{a}"),
                compile_format_snapshot(&mut interner, r"\def\foo{b}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/frontmatter.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 0,
                continuation_stack: Vec::new(),
                output_start_utf8: 0,
                page_index_after: 0,
                snapshot: input_snapshot,
            }],
        )
        .expect("bundle");
        let expected_checkpoint_id = bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.page_index_after == 0
            })
            .expect("page-zero input boundary checkpoint")
            .meta
            .checkpoint_id
            .clone();
        bundle
            .checkpoints
            .retain(|checkpoint| checkpoint.meta.kind != CheckpointKind::Preamble);

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 22);
        assert_eq!(tail.previous_page_start, 0);
        assert_eq!(tail.current_page_start, 0);
        assert_eq!(tail.page_count, 2);
        assert_eq!(tail.resume_checkpoint_id, expected_checkpoint_id);
    }

    #[test]
    fn prefers_earlier_page_zero_input_boundary_when_preamble_checkpoint_is_missing() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let first_input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{first}");
        let second_input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{second}");
        let mut bundle = build_checkpoint_bundle_with_snapshots(
            24,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\foo{a}"),
                compile_format_snapshot(&mut interner, r"\def\foo{b}"),
            ],
            &[10, 20],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/frontmatter.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 0,
                    page_index_after: 0,
                    snapshot: first_input_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/abstract.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 3,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 4,
                    page_index_after: 0,
                    snapshot: second_input_snapshot,
                },
            ],
        )
        .expect("bundle");
        let expected_checkpoint_id = bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.page_index_after == 0
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/frontmatter.tex"))
            })
            .expect("first page-zero input boundary checkpoint")
            .meta
            .checkpoint_id
            .clone();
        let later_checkpoint_id = bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.page_index_after == 0
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/abstract.tex"))
            })
            .expect("second page-zero input boundary checkpoint")
            .meta
            .checkpoint_id
            .clone();
        bundle
            .checkpoints
            .retain(|checkpoint| checkpoint.meta.kind != CheckpointKind::Preamble);

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 24);
        assert_eq!(tail.previous_page_start, 0);
        assert_eq!(tail.current_page_start, 0);
        assert_eq!(tail.page_count, 2);
        assert_eq!(tail.resume_checkpoint_id, expected_checkpoint_id);
        assert_ne!(tail.resume_checkpoint_id, later_checkpoint_id);
    }

    #[test]
    fn falls_back_to_later_page_zero_input_boundary_when_earlier_one_is_missing() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let first_input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{first}");
        let second_input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{second}");
        let mut bundle = build_checkpoint_bundle_with_snapshots(
            25,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\foo{a}"),
                compile_format_snapshot(&mut interner, r"\def\foo{b}"),
            ],
            &[10, 20],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/frontmatter.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 0,
                    page_index_after: 0,
                    snapshot: first_input_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/abstract.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 3,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 4,
                    page_index_after: 0,
                    snapshot: second_input_snapshot,
                },
            ],
        )
        .expect("bundle");
        let expected_checkpoint_id = bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.page_index_after == 0
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/abstract.tex"))
            })
            .expect("second page-zero input boundary checkpoint")
            .meta
            .checkpoint_id
            .clone();
        bundle.checkpoints.retain(|checkpoint| {
            checkpoint.meta.kind != CheckpointKind::Preamble
                && !(checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.page_index_after == 0
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/frontmatter.tex")))
        });

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 25);
        assert_eq!(tail.previous_page_start, 0);
        assert_eq!(tail.current_page_start, 0);
        assert_eq!(tail.page_count, 2);
        assert_eq!(tail.resume_checkpoint_id, expected_checkpoint_id);
    }

    #[test]
    fn keeps_first_page_zero_input_boundary_in_bundle_order_when_preamble_is_missing() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let later_input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{later}");
        let earlier_input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{earlier}");
        let mut bundle = build_checkpoint_bundle_with_snapshots(
            26,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\foo{a}"),
                compile_format_snapshot(&mut interner, r"\def\foo{b}"),
            ],
            &[10, 20],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/abstract.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 3,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 4,
                    page_index_after: 0,
                    snapshot: later_input_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/frontmatter.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 0,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 0,
                    page_index_after: 0,
                    snapshot: earlier_input_snapshot,
                },
            ],
        )
        .expect("bundle");
        let expected_checkpoint_id = bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.page_index_after == 0
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/abstract.tex"))
            })
            .expect("first bundle-order page-zero checkpoint")
            .meta
            .checkpoint_id
            .clone();
        let other_checkpoint_id = bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.page_index_after == 0
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/frontmatter.tex"))
            })
            .expect("second bundle-order page-zero checkpoint")
            .meta
            .checkpoint_id
            .clone();
        bundle
            .checkpoints
            .retain(|checkpoint| checkpoint.meta.kind != CheckpointKind::Preamble);

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 26);
        assert_eq!(tail.previous_page_start, 0);
        assert_eq!(tail.current_page_start, 0);
        assert_eq!(tail.page_count, 2);
        assert_eq!(tail.resume_checkpoint_id, expected_checkpoint_id);
        assert_ne!(tail.resume_checkpoint_id, other_checkpoint_id);
    }

    #[test]
    fn returns_none_for_full_tail_when_page_zero_resume_checkpoint_is_missing() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{input}");
        let mut bundle = build_checkpoint_bundle_with_snapshots(
            23,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\foo{a}"),
                compile_format_snapshot(&mut interner, r"\def\foo{b}"),
            ],
            &[10, 20],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/frontmatter.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 0,
                continuation_stack: Vec::new(),
                output_start_utf8: 0,
                page_index_after: 0,
                snapshot: input_snapshot,
            }],
        )
        .expect("bundle");
        bundle
            .checkpoints
            .retain(|checkpoint| checkpoint.meta.page_index_after != 0);

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        );

        assert!(tail.is_none());
    }

    #[test]
    fn returns_single_page_tail_when_only_last_page_matches() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            3,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "p2".to_string(),
                    index: 2,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "changed-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "changed-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "new-2".to_string(),
                    index: 2,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 3);
        assert_eq!(tail.previous_page_start, 2);
        assert_eq!(tail.current_page_start, 2);
        assert_eq!(tail.page_count, 1);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[2].meta.checkpoint_id
        );
    }

    #[test]
    fn returns_tail_when_current_document_is_shorter() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            5,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "p2".to_string(),
                    index: 2,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 5);
        assert_eq!(tail.previous_page_start, 1);
        assert_eq!(tail.current_page_start, 0);
        assert_eq!(tail.page_count, 2);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[1].meta.checkpoint_id
        );
    }

    #[test]
    fn returns_tail_when_current_document_gains_front_pages() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            8,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "front-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "new-2".to_string(),
                    index: 2,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 8);
        assert_eq!(tail.previous_page_start, 0);
        assert_eq!(tail.current_page_start, 1);
        assert_eq!(tail.page_count, 2);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[0].meta.checkpoint_id
        );
    }

    #[test]
    fn returns_only_last_page_when_current_document_appends_duplicate_tail_page() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            9,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "new-2".to_string(),
                    index: 2,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 9);
        assert_eq!(tail.previous_page_start, 1);
        assert_eq!(tail.current_page_start, 2);
        assert_eq!(tail.page_count, 1);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[1].meta.checkpoint_id
        );
    }

    #[test]
    fn returns_only_last_two_pages_when_current_document_appends_duplicate_two_page_tail() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            16,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "head-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "tail-0".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "p2".to_string(),
                    index: 2,
                    content_hash: "tail-1".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "changed-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "tail-0".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "new-2".to_string(),
                    index: 2,
                    content_hash: "tail-1".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
                CheckpointPage {
                    page_id: "new-3".to_string(),
                    index: 3,
                    content_hash: "tail-0".to_string(),
                    text_start_utf8: 30,
                    text_end_utf8: 40,
                },
                CheckpointPage {
                    page_id: "new-4".to_string(),
                    index: 4,
                    content_hash: "tail-1".to_string(),
                    text_start_utf8: 40,
                    text_end_utf8: 50,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 16);
        assert_eq!(tail.previous_page_start, 1);
        assert_eq!(tail.current_page_start, 3);
        assert_eq!(tail.page_count, 2);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[1].meta.checkpoint_id
        );
    }

    #[test]
    fn returns_last_two_pages_when_current_document_repeats_entire_two_page_document() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            17,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "tail-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "tail-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "tail-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "tail-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "new-2".to_string(),
                    index: 2,
                    content_hash: "tail-0".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
                CheckpointPage {
                    page_id: "new-3".to_string(),
                    index: 3,
                    content_hash: "tail-1".to_string(),
                    text_start_utf8: 30,
                    text_end_utf8: 40,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 17);
        assert_eq!(tail.previous_page_start, 0);
        assert_eq!(tail.current_page_start, 2);
        assert_eq!(tail.page_count, 2);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[0].meta.checkpoint_id
        );
    }

    #[test]
    fn returns_none_when_matching_tail_lacks_resume_checkpoint() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let mut bundle = build_checkpoint_bundle(
            10,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "p2".to_string(),
                    index: 2,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
        )
        .expect("bundle");
        bundle
            .checkpoints
            .retain(|checkpoint| checkpoint.meta.page_index_after != 2);

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "changed-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        );

        assert!(tail.is_none());
    }

    #[test]
    fn prefers_shipout_resume_checkpoint_over_later_input_boundary_with_same_page_index() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{input}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            11,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "p2".to_string(),
                    index: 2,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\foo{a}"),
                compile_format_snapshot(&mut interner, r"\def\foo{b}"),
                compile_format_snapshot(&mut interner, r"\def\foo{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/tail.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 12,
                continuation_stack: Vec::new(),
                output_start_utf8: 20,
                page_index_after: 2,
                snapshot: input_snapshot,
            }],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "changed-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 11);
        assert_eq!(tail.previous_page_start, 2);
        assert_eq!(tail.current_page_start, 1);
        assert_eq!(tail.page_count, 1);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[2].meta.checkpoint_id
        );
        assert_ne!(
            tail.resume_checkpoint_id,
            bundle
                .checkpoints
                .last()
                .expect("input boundary checkpoint")
                .meta
                .checkpoint_id
        );
    }

    #[test]
    fn prefers_shipout_resume_checkpoint_over_multiple_input_boundaries_with_same_page_index() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let first_input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{first}");
        let second_input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{second}");
        let bundle = build_checkpoint_bundle_with_snapshots(
            20,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "p2".to_string(),
                    index: 2,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\foo{a}"),
                compile_format_snapshot(&mut interner, r"\def\foo{b}"),
                compile_format_snapshot(&mut interner, r"\def\foo{c}"),
            ],
            &[10, 20, 30],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/first.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 11,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 18,
                    page_index_after: 2,
                    snapshot: first_input_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/second.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 12,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 22,
                    page_index_after: 2,
                    snapshot: second_input_snapshot,
                },
            ],
        )
        .expect("bundle");
        let input_checkpoint_ids = bundle
            .checkpoints
            .iter()
            .filter(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.page_index_after == 2
            })
            .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
            .collect::<Vec<_>>();

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "changed-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 20);
        assert_eq!(tail.previous_page_start, 2);
        assert_eq!(tail.current_page_start, 1);
        assert_eq!(tail.page_count, 1);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[2].meta.checkpoint_id
        );
        assert!(
            input_checkpoint_ids
                .iter()
                .all(|checkpoint_id| checkpoint_id != &tail.resume_checkpoint_id)
        );
    }

    #[test]
    fn keeps_first_matching_resume_checkpoint_in_bundle_order_when_input_precedes_shipout() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{input}");
        let mut bundle = build_checkpoint_bundle_with_snapshots(
            27,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "p2".to_string(),
                    index: 2,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\foo{a}"),
                compile_format_snapshot(&mut interner, r"\def\foo{b}"),
                compile_format_snapshot(&mut interner, r"\def\foo{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/tail.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 12,
                continuation_stack: Vec::new(),
                output_start_utf8: 20,
                page_index_after: 2,
                snapshot: input_snapshot,
            }],
        )
        .expect("bundle");
        let shipout_index = bundle
            .checkpoints
            .iter()
            .position(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::Shipout
                    && checkpoint.meta.page_index_after == 2
            })
            .expect("shipout checkpoint");
        let shipout_checkpoint_id = bundle.checkpoints[shipout_index].meta.checkpoint_id.clone();
        let input_index = bundle
            .checkpoints
            .iter()
            .position(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.page_index_after == 2
            })
            .expect("input checkpoint");
        let input_checkpoint_id = bundle.checkpoints[input_index].meta.checkpoint_id.clone();
        let input_checkpoint = bundle.checkpoints.remove(input_index);
        bundle.checkpoints.insert(shipout_index, input_checkpoint);

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "changed-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 27);
        assert_eq!(tail.previous_page_start, 2);
        assert_eq!(tail.current_page_start, 1);
        assert_eq!(tail.page_count, 1);
        assert_eq!(tail.resume_checkpoint_id, input_checkpoint_id);
        assert_ne!(tail.resume_checkpoint_id, shipout_checkpoint_id);
    }

    #[test]
    fn falls_back_to_input_boundary_resume_checkpoint_when_shipout_is_missing() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{input}");
        let mut bundle = build_checkpoint_bundle_with_snapshots(
            12,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "p2".to_string(),
                    index: 2,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\foo{a}"),
                compile_format_snapshot(&mut interner, r"\def\foo{b}"),
                compile_format_snapshot(&mut interner, r"\def\foo{c}"),
            ],
            &[10, 20, 30],
            &[InputBoundaryCheckpoint {
                kind: VmModuleCheckpointKind::Enter,
                module_path: Utf8PathBuf::from("sections/tail.tex"),
                resume_path: Some(Utf8PathBuf::from("main.tex")),
                source_offset_utf8: 12,
                continuation_stack: Vec::new(),
                output_start_utf8: 20,
                page_index_after: 2,
                snapshot: input_snapshot,
            }],
        )
        .expect("bundle");
        let expected_checkpoint_id = bundle
            .checkpoints
            .last()
            .expect("input boundary checkpoint")
            .meta
            .checkpoint_id
            .clone();
        bundle.checkpoints.retain(|checkpoint| {
            !(checkpoint.meta.kind == CheckpointKind::Shipout
                && checkpoint.meta.page_index_after == 2)
        });

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "changed-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 12);
        assert_eq!(tail.previous_page_start, 2);
        assert_eq!(tail.current_page_start, 1);
        assert_eq!(tail.page_count, 1);
        assert_eq!(tail.resume_checkpoint_id, expected_checkpoint_id);
    }

    #[test]
    fn prefers_earlier_input_boundary_resume_checkpoint_when_multiple_inputs_share_page_index() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let first_input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{first}");
        let second_input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{second}");
        let mut bundle = build_checkpoint_bundle_with_snapshots(
            18,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "p2".to_string(),
                    index: 2,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\foo{a}"),
                compile_format_snapshot(&mut interner, r"\def\foo{b}"),
                compile_format_snapshot(&mut interner, r"\def\foo{c}"),
            ],
            &[10, 20, 30],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/first.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 11,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 18,
                    page_index_after: 2,
                    snapshot: first_input_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/second.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 12,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 19,
                    page_index_after: 2,
                    snapshot: second_input_snapshot,
                },
            ],
        )
        .expect("bundle");
        let expected_checkpoint_id = bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.page_index_after == 2
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/first.tex"))
            })
            .expect("first input boundary checkpoint")
            .meta
            .checkpoint_id
            .clone();
        bundle.checkpoints.retain(|checkpoint| {
            !(checkpoint.meta.kind == CheckpointKind::Shipout
                && checkpoint.meta.page_index_after == 2)
        });

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "changed-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 18);
        assert_eq!(tail.previous_page_start, 2);
        assert_eq!(tail.current_page_start, 1);
        assert_eq!(tail.page_count, 1);
        assert_eq!(tail.resume_checkpoint_id, expected_checkpoint_id);
    }

    #[test]
    fn falls_back_to_later_input_boundary_resume_checkpoint_when_earlier_one_is_missing() {
        let mut interner = ControlSequenceInterner::new();
        let preamble_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let first_input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{first}");
        let second_input_snapshot = compile_format_snapshot(&mut interner, r"\def\foo{second}");
        let mut bundle = build_checkpoint_bundle_with_snapshots(
            19,
            &preamble_snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            0,
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "p2".to_string(),
                    index: 2,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
            &[
                compile_format_snapshot(&mut interner, r"\def\foo{a}"),
                compile_format_snapshot(&mut interner, r"\def\foo{b}"),
                compile_format_snapshot(&mut interner, r"\def\foo{c}"),
            ],
            &[10, 20, 30],
            &[
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/first.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 11,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 18,
                    page_index_after: 2,
                    snapshot: first_input_snapshot,
                },
                InputBoundaryCheckpoint {
                    kind: VmModuleCheckpointKind::Enter,
                    module_path: Utf8PathBuf::from("sections/second.tex"),
                    resume_path: Some(Utf8PathBuf::from("main.tex")),
                    source_offset_utf8: 12,
                    continuation_stack: Vec::new(),
                    output_start_utf8: 19,
                    page_index_after: 2,
                    snapshot: second_input_snapshot,
                },
            ],
        )
        .expect("bundle");
        let expected_checkpoint_id = bundle
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.page_index_after == 2
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/second.tex"))
            })
            .expect("second input boundary checkpoint")
            .meta
            .checkpoint_id
            .clone();
        bundle.checkpoints.retain(|checkpoint| {
            !((checkpoint.meta.kind == CheckpointKind::Shipout
                && checkpoint.meta.page_index_after == 2)
                || (checkpoint.meta.kind == CheckpointKind::InputBoundary
                    && checkpoint.meta.page_index_after == 2
                    && checkpoint.meta.module_path.as_ref()
                        == Some(&Utf8PathBuf::from("sections/first.tex"))))
        });

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "changed-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 19);
        assert_eq!(tail.previous_page_start, 2);
        assert_eq!(tail.current_page_start, 1);
        assert_eq!(tail.page_count, 1);
        assert_eq!(tail.resume_checkpoint_id, expected_checkpoint_id);
    }

    #[test]
    fn matches_tail_by_content_hash_even_when_text_offsets_shift() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            13,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
                CheckpointPage {
                    page_id: "p2".to_string(),
                    index: 2,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 20,
                    text_end_utf8: 30,
                },
            ],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "new-0".to_string(),
                    index: 0,
                    content_hash: "front-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 11,
                },
                CheckpointPage {
                    page_id: "new-1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 11,
                    text_end_utf8: 25,
                },
                CheckpointPage {
                    page_id: "new-2".to_string(),
                    index: 2,
                    content_hash: "old-2".to_string(),
                    text_start_utf8: 25,
                    text_end_utf8: 44,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 13);
        assert_eq!(tail.previous_page_start, 1);
        assert_eq!(tail.current_page_start, 1);
        assert_eq!(tail.page_count, 2);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[1].meta.checkpoint_id
        );
    }

    #[test]
    fn matches_tail_by_content_hash_even_when_page_ids_and_indexes_change() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            14,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "front".to_string(),
                    index: 4,
                    content_hash: "front-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 12,
                },
                CheckpointPage {
                    page_id: "shifted-tail".to_string(),
                    index: 7,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 12,
                    text_end_utf8: 25,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 14);
        assert_eq!(tail.previous_page_start, 1);
        assert_eq!(tail.current_page_start, 1);
        assert_eq!(tail.page_count, 1);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[1].meta.checkpoint_id
        );
    }

    #[test]
    fn returns_full_tail_when_all_hashes_match_despite_page_id_index_and_offset_drift() {
        let mut interner = ControlSequenceInterner::new();
        let snapshot = compile_format_snapshot(&mut interner, r"\def\foo{bar}");
        let bundle = build_checkpoint_bundle(
            15,
            &snapshot,
            &preamble_key_for_source(r"\documentclass{article}"),
            &[
                CheckpointPage {
                    page_id: "p0".to_string(),
                    index: 0,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 0,
                    text_end_utf8: 10,
                },
                CheckpointPage {
                    page_id: "p1".to_string(),
                    index: 1,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 10,
                    text_end_utf8: 20,
                },
            ],
        )
        .expect("bundle");

        let tail = find_unchanged_tail(
            &bundle,
            &[
                CheckpointPage {
                    page_id: "shifted-0".to_string(),
                    index: 8,
                    content_hash: "old-0".to_string(),
                    text_start_utf8: 3,
                    text_end_utf8: 17,
                },
                CheckpointPage {
                    page_id: "shifted-1".to_string(),
                    index: 9,
                    content_hash: "old-1".to_string(),
                    text_start_utf8: 17,
                    text_end_utf8: 34,
                },
            ],
        )
        .expect("tail");

        assert_eq!(tail.previous_rev, 15);
        assert_eq!(tail.previous_page_start, 0);
        assert_eq!(tail.current_page_start, 0);
        assert_eq!(tail.page_count, 2);
        assert_eq!(
            tail.resume_checkpoint_id,
            bundle.checkpoints[0].meta.checkpoint_id
        );
    }
}
