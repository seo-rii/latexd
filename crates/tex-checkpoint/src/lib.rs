use std::fs;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use tex_vm::{VmModuleCheckpointKind, VmReplayFrame, VmSnapshot};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCheckpoint {
    pub meta: CheckpointMeta,
    pub snapshot: Option<VmSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointBundle {
    pub checkpoints: Vec<StoredCheckpoint>,
    #[serde(default)]
    pub pages: Vec<CheckpointPage>,
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
    let mut checkpoints = vec![StoredCheckpoint {
        meta: CheckpointMeta {
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
            snapshot_attached: true,
            source_offset_utf8: preamble_source_offset_utf8,
            resume_path: None,
            continuation_stack: Vec::new(),
            module_path: None,
            input_boundary_kind: None,
            output_start_utf8: 0,
        },
        snapshot: Some(preamble_snapshot.clone()),
    }];

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
        checkpoints.push(StoredCheckpoint {
            meta: CheckpointMeta {
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
                snapshot_attached: shipout_checkpoint.is_some(),
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
            snapshot: shipout_checkpoint.map(|checkpoint| checkpoint.snapshot.clone()),
        });
    }

    for boundary in input_boundaries {
        let snapshot_json = serde_json::to_vec(&boundary.snapshot)
            .context("failed to serialize input-boundary snapshot")?;
        let vm_state_hash = blake3::hash(&snapshot_json).to_hex().to_string();
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
        checkpoints.push(StoredCheckpoint {
            meta: CheckpointMeta {
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
                snapshot_attached: true,
                source_offset_utf8: boundary.source_offset_utf8,
                resume_path: boundary.resume_path.clone(),
                continuation_stack: boundary.continuation_stack.clone(),
                module_path: Some(boundary.module_path.clone()),
                input_boundary_kind: Some(boundary.kind),
                output_start_utf8: boundary.output_start_utf8,
            },
            snapshot: Some(boundary.snapshot.clone()),
        });
    }

    Ok(CheckpointBundle {
        checkpoints,
        pages: pages.to_vec(),
    })
}

pub fn save_checkpoint_bundle(path: &Utf8Path, bundle: &CheckpointBundle) -> Result<()> {
    let contents =
        serde_json::to_vec_pretty(bundle).context("failed to serialize checkpoint bundle")?;
    fs::write(path, contents).with_context(|| format!("failed to write checkpoint bundle {path}"))
}

pub fn load_checkpoint_bundle(path: &Utf8Path) -> Result<CheckpointBundle> {
    let contents =
        fs::read(path).with_context(|| format!("failed to read checkpoint bundle {path}"))?;
    serde_json::from_slice(&contents)
        .with_context(|| format!("failed to parse checkpoint bundle {path}"))
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
        })
        .cloned()
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
        let bundle = load_checkpoint_bundle(&path)?;
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
        CheckpointKind, CheckpointPage, InputBoundaryCheckpoint, ShipoutCheckpoint,
        build_checkpoint_bundle, build_checkpoint_bundle_with_shipouts,
        build_checkpoint_bundle_with_snapshots, can_reuse_preamble, find_unchanged_tail,
        load_checkpoint_bundle, load_latest_reusable_preamble, preamble_key_for_source,
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
        assert!(bundle.checkpoints[0].snapshot.is_some());
        assert_eq!(bundle.checkpoints[1].meta.kind, CheckpointKind::Shipout);
        assert!(bundle.checkpoints[1].snapshot.is_none());
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
        assert!(bundle.checkpoints[0].snapshot.is_some());
        assert!(bundle.checkpoints[1].meta.snapshot_attached);
        assert_eq!(bundle.checkpoints[1].meta.source_offset_utf8, 47);
        assert_eq!(
            bundle.checkpoints[1].snapshot.as_ref(),
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
        assert_eq!(input_checkpoint.snapshot.as_ref(), Some(&input_snapshot));
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
        assert!(selected.snapshot.is_some());
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
