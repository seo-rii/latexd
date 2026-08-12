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
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    SnapshotCapability, Vm, VmContinuationBlocker, VmContinuationSafety, VmModuleCheckpointKind,
    VmReplayFrame, VmSnapshot, VmSnapshotDocument, decode_vm_snapshot_document,
};

pub const CHECKPOINT_UNSAFE_STATE: &str = "CHECKPOINT_UNSAFE_STATE";

const CHECKPOINT_DISK_SCHEMA_VERSION: u32 = 2;
const CHECKPOINT_DISK_ENCODING: &str = "gzip+base64";
const MAX_CHECKPOINT_UNCOMPRESSED_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const CHECKPOINT_ENVELOPE_PREFIX: &[u8] =
    b"{\"schema_version\":2,\"encoding\":\"gzip+base64\",\"payload\":\"";

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
}

impl<W> IntegrityWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            hasher: blake3::Hasher::new(),
            bytes_written: 0,
        }
    }

    fn into_parts(self) -> (W, u64, blake3::Hash) {
        (self.inner, self.bytes_written, self.hasher.finalize())
    }
}

impl<W: Write> Write for IntegrityWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionedSnapshotSlotWire {
    document: serde_json::Value,
}

impl<'de> Deserialize<'de> for VersionedSnapshotSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = VersionedSnapshotSlotWire::deserialize(deserializer)?;
        let encoded = serde_json::to_vec(&wire.document).map_err(serde::de::Error::custom)?;
        let document = decode_vm_snapshot_document(&encoded).map_err(serde::de::Error::custom)?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotWritePolicy {
    LegacyOnly,
}

const SNAPSHOT_WRITE_POLICY: SnapshotWritePolicy = SnapshotWritePolicy::LegacyOnly;

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
        let required_capabilities = snapshot.required_capabilities();
        if !required_capabilities.is_empty() {
            anyhow::bail!(
                "legacy snapshot writer cannot encode required capabilities: {}",
                required_capabilities
                    .iter()
                    .map(SnapshotCapability::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Ok(Self(snapshot))
    }
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
}

impl Serialize for StoredCheckpoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let snapshot = match &self.attachment {
            StoredSnapshotAttachment::None => None,
            StoredSnapshotAttachment::Legacy(snapshot) => Some(snapshot),
            StoredSnapshotAttachment::Versioned(_) => {
                return Err(serde::ser::Error::custom(
                    "versioned snapshot writer is disabled",
                ));
            }
        };
        StoredCheckpointWriteWire {
            meta: &self.meta,
            snapshot,
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize)]
struct StoredCheckpointWire {
    meta: CheckpointMeta,
    snapshot: Option<VmSnapshot>,
    #[serde(default)]
    versioned_snapshot: Option<serde_json::Value>,
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
                serde_json::from_value(slot).map_err(serde::de::Error::custom)?,
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
    fn with_legacy_snapshot(meta: CheckpointMeta, snapshot: Option<VmSnapshot>) -> Result<Self> {
        let attachment = match snapshot {
            Some(snapshot) => {
                StoredSnapshotAttachment::Legacy(LegacySnapshotForWrite::try_from(snapshot)?.0)
            }
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
    pub checkpoints: Vec<StoredCheckpoint>,
    #[serde(default)]
    pub pages: Vec<CheckpointPage>,
}

#[derive(Serialize)]
struct CheckpointBundleWriteWire<'a> {
    checkpoints: &'a [StoredCheckpoint],
    pages: &'a [CheckpointPage],
}

impl CheckpointBundle {
    fn ensure_legacy_writable(&self) -> Result<()> {
        match SNAPSHOT_WRITE_POLICY {
            SnapshotWritePolicy::LegacyOnly => {
                if self.checkpoints.iter().any(|checkpoint| {
                    matches!(
                        checkpoint.snapshot_attachment(),
                        SnapshotAttachment::Versioned(_)
                    )
                }) {
                    anyhow::bail!("versioned snapshot writer is disabled");
                }
            }
        }
        Ok(())
    }
}

impl Serialize for CheckpointBundle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.ensure_legacy_writable()
            .map_err(serde::ser::Error::custom)?;
        CheckpointBundleWriteWire {
            checkpoints: &self.checkpoints,
            pages: &self.pages,
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
    if !shipout_checkpoints.is_empty() && shipout_checkpoints.len() != pages.len() {
        anyhow::bail!("shipout snapshot/page length mismatch");
    }
    let snapshot_json =
        serde_json::to_vec(preamble_snapshot).context("failed to serialize preamble snapshot")?;
    let vm_state_hash = blake3::hash(&snapshot_json).to_hex().to_string();
    let preamble_continuation_safety = preamble_snapshot.continuation_safety.clone();
    let preamble_snapshot_attached = preamble_continuation_safety.is_safe();
    let mut checkpoints = vec![StoredCheckpoint::with_legacy_snapshot(
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
    )?];

    for (index, page) in pages.iter().enumerate() {
        let boundary_hash = page_boundary_hash(page);
        let shipout_checkpoint = shipout_checkpoints.get(index);
        let source_offset_utf8 = shipout_checkpoint
            .map(|checkpoint| checkpoint.source_offset_utf8)
            .unwrap_or(0);
        let vm_state_hash = shipout_checkpoint
            .map(|checkpoint| serde_json::to_vec(&checkpoint.snapshot))
            .transpose()
            .context("failed to serialize shipout snapshot")?
            .map(|json| blake3::hash(&json).to_hex().to_string())
            .unwrap_or_else(|| vm_state_hash.clone());
        let continuation_safety = shipout_checkpoint
            .map(|checkpoint| checkpoint.snapshot.continuation_safety.clone())
            .unwrap_or_default();
        let snapshot_attached = shipout_checkpoint.is_some() && continuation_safety.is_safe();
        checkpoints.push(StoredCheckpoint::with_legacy_snapshot(
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
        )?);
    }

    for boundary in input_boundaries {
        let snapshot_json = serde_json::to_vec(&boundary.snapshot)
            .context("failed to serialize input-boundary snapshot")?;
        let vm_state_hash = blake3::hash(&snapshot_json).to_hex().to_string();
        let continuation_safety = boundary.snapshot.continuation_safety.clone();
        let snapshot_attached = continuation_safety.is_safe()
            && boundary
                .snapshot
                .input_continuation
                .as_ref()
                .is_none_or(tex_vm::VmInputContinuationSnapshot::is_restorable);
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
        checkpoints.push(StoredCheckpoint::with_legacy_snapshot(
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
        )?);
    }

    Ok(CheckpointBundle {
        checkpoints,
        pages: pages.to_vec(),
    })
}

pub fn save_checkpoint_bundle(path: &Utf8Path, bundle: &CheckpointBundle) -> Result<()> {
    bundle.ensure_legacy_writable()?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("checkpoint bundle path has no parent: {path}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temporary checkpoint bundle beside {path}"))?;
    temporary
        .write_all(CHECKPOINT_ENVELOPE_PREFIX)
        .with_context(|| format!("failed to write checkpoint envelope header for {path}"))?;
    let (uncompressed_len, uncompressed_hash) = {
        let encoded = EncoderWriter::new(temporary.as_file_mut(), &BASE64_STANDARD);
        let compressed = GzEncoder::new(encoded, Compression::fast());
        let mut integrity = IntegrityWriter::new(compressed);
        serde_json::to_writer(&mut integrity, bundle)
            .context("failed to serialize checkpoint bundle")?;
        let (compressed, uncompressed_len, uncompressed_hash) = integrity.into_parts();
        let mut encoded = compressed
            .finish()
            .context("failed to finish checkpoint compression")?;
        encoded
            .finish()
            .context("failed to finish checkpoint base64 encoding")?;
        (uncompressed_len, uncompressed_hash)
    };
    writeln!(
        temporary,
        "\",\"uncompressed_len\":{uncompressed_len},\"uncompressed_blake3\":\"{}\"}}",
        uncompressed_hash.to_hex()
    )
    .with_context(|| format!("failed to write checkpoint envelope footer for {path}"))?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("failed to sync temporary checkpoint bundle for {path}"))?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to replace checkpoint bundle {path}"))?;
    Ok(())
}

pub fn load_checkpoint_bundle(path: &Utf8Path) -> Result<CheckpointBundle> {
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
    if envelope.uncompressed_len > MAX_CHECKPOINT_UNCOMPRESSED_BYTES {
        anyhow::bail!(
            "checkpoint payload declares {} uncompressed bytes, exceeding the {} byte limit in {path}",
            envelope.uncompressed_len,
            MAX_CHECKPOINT_UNCOMPRESSED_BYTES
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
        Ok(bundle) => CheckpointBundleReuse::Hit(bundle),
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
    use std::fs;

    use camino::Utf8PathBuf;
    use tempfile::tempdir;
    use tex_tokens::ControlSequenceInterner;
    use tex_vm::{VmModuleCheckpointKind, VmReplayFrame, compile_format_snapshot};

    use super::{
        CheckpointBundleReuse, CheckpointCacheMissReason, CheckpointKind, CheckpointPage,
        InputBoundaryCheckpoint, ShipoutCheckpoint, build_checkpoint_bundle,
        build_checkpoint_bundle_with_shipouts, build_checkpoint_bundle_with_snapshots,
        can_reuse_preamble, find_unchanged_tail, load_checkpoint_bundle,
        load_checkpoint_bundle_for_reuse, load_latest_reusable_preamble, preamble_key_for_source,
        save_checkpoint_bundle, select_reusable_preamble,
    };

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
