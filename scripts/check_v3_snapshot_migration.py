#!/usr/bin/env python3
"""Characterize the exact pre-reader V3 snapshot compatibility boundary."""

from __future__ import annotations

import argparse
import base64
import copy
import gzip
import json
import os
import subprocess
import tempfile
from pathlib import Path
from typing import Any


DEFAULT_BASELINE = "00c8ee3"
EXPECTED_PRE_READER_RESULTS = {
    "raw_field_only": {
        "accepted": True,
        "muskip_field_preserved": False,
        "output": "R",
    },
    "checkpoint_versioned_only": {
        "accepted": True,
        "replay_safe": False,
    },
    "checkpoint_dual_lane": {
        "accepted": True,
        "replay_safe": True,
    },
    "raw_versioned_document": {
        "accepted": False,
    },
    "canonical_muskip_document_to_pre_reader": {
        "accepted": False,
    },
    "candidate_legacy_bundle_to_pre_reader": {
        "accepted": True,
        "replay_safe": True,
        "versioned_field_present": False,
        "muskip_field_present": False,
    },
    "candidate_envelope_to_pre_reader": {
        "accepted": True,
        "replay_safe": True,
        "output": "R",
        "versioned_field_present": False,
        "muskip_field_present": False,
    },
    "pre_reader_envelope_to_candidate": {
        "accepted": True,
        "replay_safe": True,
        "output": "R",
    },
    "candidate_versioned_envelope": {
        "reuse": "hit",
        "replay_safe": True,
        "output": "R",
    },
    "candidate_supported_muskip_capability_envelope": {
        "reuse": "hit",
        "replay_safe": True,
        "output": "[2.5mu][3mu][3mu]",
    },
    "supported_muskip_envelope_to_pre_reader": {
        "accepted": True,
        "replay_safe": False,
        "output": None,
    },
    "candidate_duplicate_muskip_member_envelope": {
        "reuse": "miss",
        "reason": "unreadable",
    },
    "candidate_dual_lane_envelope": {"reuse": "miss", "reason": "unreadable"},
    "candidate_unsupported_capability_envelope": {
        "reuse": "miss",
        "reason": "unreadable",
    },
    "candidate_malformed_document_envelope": {
        "reuse": "miss",
        "reason": "unreadable",
    },
}

HARNESS_SOURCE = r'''use std::{env, fs, io::Write, path::Path};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use camino::Utf8Path;
use flate2::{Compression, write::GzEncoder};
use serde_json::json;
use tex_checkpoint::{
    build_checkpoint_bundle, checkpoint_is_replay_safe, load_checkpoint_bundle,
    save_checkpoint_bundle,
};
#[cfg(feature = "candidate")]
use tex_checkpoint::{
    CheckpointBundleReuse, CheckpointCacheMissReason, load_checkpoint_bundle_for_reuse,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmSnapshot};
#[cfg(feature = "candidate")]
use tex_vm::VmSnapshotDocument;

fn produce_raw(path: &Path) {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(r"\def\vthreemigrationprobe{R}");
    fs::write(
        path,
        serde_json::to_vec(&vm.snapshot()).expect("serialize raw snapshot"),
    )
    .expect("write raw snapshot");
}

fn consume_raw(path: &Path) {
    let bytes = fs::read(path).expect("read raw snapshot");
    let result = match serde_json::from_slice::<VmSnapshot>(&bytes) {
        Ok(snapshot) => {
            let mut interner = ControlSequenceInterner::new();
            let mut vm = Vm::restore(&mut interner, &snapshot);
            let outcome = vm.run_plain(r"\vthreemigrationprobe");
            let projected = serde_json::to_value(vm.snapshot()).expect("serialize projection");
            json!({
                "accepted": true,
                "muskip_field_preserved": projected.get("muskip_registers").is_some(),
                "output": outcome.output,
            })
        }
        Err(_) => json!({ "accepted": false }),
    };
    println!("{}", serde_json::to_string(&result).expect("serialize raw result"));
}

#[cfg(feature = "candidate")]
fn produce_muskip_document(path: &Path) {
    let mut interner = ControlSequenceInterner::new();
    let vm = Vm::new(&mut interner);
    let mut snapshot = vm.snapshot();
    snapshot.muskip_registers.insert(17, 163840);
    snapshot.muskip_registers.insert(300, 458752);
    snapshot.next_muskip_register = 301;
    let document = VmSnapshotDocument::from_snapshot(snapshot);
    fs::write(
        path,
        serde_json::to_vec(&document).expect("serialize canonical muskip document"),
    )
    .expect("write canonical muskip document");
}

fn produce_bundle(path: &Path) {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(r"\def\vthreemigrationprobe{R}");
    let bundle = build_checkpoint_bundle(1, &vm.snapshot(), "baseline", &[])
        .expect("build checkpoint bundle");
    fs::write(
        path,
        serde_json::to_vec(&bundle).expect("serialize legacy bundle"),
    )
    .expect("write legacy bundle");
}

fn produce_envelope(path: &Path) {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    vm.run_plain(r"\def\vthreemigrationprobe{R}");
    let bundle = build_checkpoint_bundle(1, &vm.snapshot(), "baseline", &[])
        .expect("build checkpoint bundle");
    save_checkpoint_bundle(Utf8Path::from_path(path).expect("UTF-8 envelope path"), &bundle)
        .expect("save production envelope");
}

fn consume_bundle(path: &Path) {
    let utf8_path = Utf8Path::from_path(path).expect("UTF-8 bundle path");
    let result = match load_checkpoint_bundle(utf8_path) {
        Ok(bundle) => json!({
            "accepted": true,
            "replay_safe": bundle
                .checkpoints
                .first()
                .is_some_and(checkpoint_is_replay_safe),
        }),
        Err(_) => json!({ "accepted": false }),
    };
    println!("{}", serde_json::to_string(&result).expect("serialize bundle result"));
}

fn replay_output(snapshot: &VmSnapshot) -> String {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::restore(&mut interner, snapshot);
    vm.run_plain(r"\vthreemigrationprobe").output
}

#[cfg(feature = "candidate")]
fn replay_muskip_output(snapshot: &VmSnapshot) -> String {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::restore(&mut interner, snapshot);
    vm.run_plain(
        r"\newmuskip\dynamic\dynamic=3mu[\the\muskip17][\the\dynamic][\the\muskip301]",
    )
    .output
}

#[cfg(not(feature = "candidate"))]
fn consume_envelope(path: &Path) {
    let utf8_path = Utf8Path::from_path(path).expect("UTF-8 envelope path");
    let result = match load_checkpoint_bundle(utf8_path) {
        Ok(bundle) => {
            let checkpoint = bundle.checkpoints.first().expect("checkpoint");
            json!({
                "accepted": true,
                "replay_safe": checkpoint_is_replay_safe(checkpoint),
                "output": checkpoint.snapshot.as_ref().map(replay_output),
            })
        }
        Err(_) => json!({ "accepted": false }),
    };
    println!("{}", serde_json::to_string(&result).expect("serialize envelope result"));
}

#[cfg(feature = "candidate")]
fn consume_envelope(path: &Path) {
    let utf8_path = Utf8Path::from_path(path).expect("UTF-8 envelope path");
    let result = match load_checkpoint_bundle(utf8_path) {
        Ok(bundle) => {
            let checkpoint = bundle.checkpoints.first().expect("checkpoint");
            json!({
                "accepted": true,
                "replay_safe": checkpoint_is_replay_safe(checkpoint),
                "output": checkpoint
                    .snapshot_for_restore()
                    .map(|restore| replay_output(restore.state())),
            })
        }
        Err(_) => json!({ "accepted": false }),
    };
    println!("{}", serde_json::to_string(&result).expect("serialize envelope result"));
}

#[cfg(feature = "candidate")]
fn consume_reuse(path: &Path) {
    let utf8_path = Utf8Path::from_path(path).expect("UTF-8 envelope path");
    let result = match load_checkpoint_bundle_for_reuse(utf8_path) {
        CheckpointBundleReuse::Hit(bundle) => {
            let checkpoint = bundle.checkpoints.first().expect("checkpoint");
            json!({
                "reuse": "hit",
                "replay_safe": checkpoint_is_replay_safe(checkpoint),
                "output": checkpoint
                    .snapshot_for_restore()
                    .map(|restore| replay_output(restore.state())),
            })
        }
        CheckpointBundleReuse::Miss(reason) => json!({
            "reuse": "miss",
            "reason": match reason {
                CheckpointCacheMissReason::NotFound => "not_found",
                CheckpointCacheMissReason::Unreadable => "unreadable",
            },
        }),
    };
    println!("{}", serde_json::to_string(&result).expect("serialize reuse result"));
}

#[cfg(feature = "candidate")]
fn consume_muskip_reuse(path: &Path) {
    let utf8_path = Utf8Path::from_path(path).expect("UTF-8 envelope path");
    let result = match load_checkpoint_bundle_for_reuse(utf8_path) {
        CheckpointBundleReuse::Hit(bundle) => {
            let checkpoint = bundle.checkpoints.first().expect("checkpoint");
            json!({
                "reuse": "hit",
                "replay_safe": checkpoint_is_replay_safe(checkpoint),
                "output": checkpoint
                    .snapshot_for_restore()
                    .map(|restore| replay_muskip_output(restore.state())),
            })
        }
        CheckpointBundleReuse::Miss(reason) => json!({
            "reuse": "miss",
            "reason": match reason {
                CheckpointCacheMissReason::NotFound => "not_found",
                CheckpointCacheMissReason::Unreadable => "unreadable",
            },
        }),
    };
    println!("{}", serde_json::to_string(&result).expect("serialize reuse result"));
}

fn wrap_raw_payload(raw_path: &Path, envelope_path: &Path) {
    let payload = fs::read(raw_path).expect("read raw checkpoint payload");
    let mut compressor = GzEncoder::new(Vec::new(), Compression::fast());
    compressor.write_all(&payload).expect("compress checkpoint payload");
    let compressed = compressor.finish().expect("finish checkpoint compression");
    let envelope = json!({
        "schema_version": 2,
        "encoding": "gzip+base64",
        "payload": BASE64_STANDARD.encode(compressed),
        "uncompressed_len": payload.len(),
        "uncompressed_blake3": blake3::hash(&payload).to_hex().to_string(),
    });
    fs::write(
        envelope_path,
        serde_json::to_vec(&envelope).expect("serialize checkpoint envelope"),
    )
    .expect("write checkpoint envelope");
}

fn main() {
    let mut arguments = env::args_os().skip(1);
    let mode = arguments.next().expect("mode");
    let path = arguments.next().expect("path");
    let second_path = arguments.next();
    assert!(arguments.next().is_none(), "unexpected argument");
    match mode.to_str().expect("UTF-8 mode") {
        "produce-raw" => produce_raw(Path::new(&path)),
        "consume-raw" => consume_raw(Path::new(&path)),
        #[cfg(feature = "candidate")]
        "produce-muskip-document" => produce_muskip_document(Path::new(&path)),
        "produce-bundle" => produce_bundle(Path::new(&path)),
        "consume-bundle" => consume_bundle(Path::new(&path)),
        "produce-envelope" => produce_envelope(Path::new(&path)),
        "consume-envelope" => consume_envelope(Path::new(&path)),
        #[cfg(feature = "candidate")]
        "consume-reuse" => consume_reuse(Path::new(&path)),
        #[cfg(feature = "candidate")]
        "consume-muskip-reuse" => consume_muskip_reuse(Path::new(&path)),
        "wrap-raw" => wrap_raw_payload(
            Path::new(&path),
            Path::new(&second_path.expect("envelope output path")),
        ),
        other => panic!("unsupported mode: {other}"),
    }
}
'''


def validate_pre_reader_results(results: dict[str, Any]) -> list[str]:
    violations = []
    for scenario, expected in EXPECTED_PRE_READER_RESULTS.items():
        actual = results.get(scenario)
        if actual != expected:
            violations.append(
                f"{scenario} mismatch: expected {expected!r}, observed {actual!r}"
            )
    unexpected = set(results).difference(EXPECTED_PRE_READER_RESULTS)
    if unexpected:
        violations.append(f"unexpected scenarios: {sorted(unexpected)!r}")
    return violations


def _run(
    arguments: list[str],
    *,
    cwd: Path,
    env: dict[str, str] | None = None,
    capture_output: bool = False,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        arguments,
        cwd=cwd,
        env=env,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture_output else None,
    )


def _resolve_revision(repo: Path, revision: str) -> str:
    completed = _run(
        ["git", "rev-parse", "--verify", f"{revision}^{{commit}}"],
        cwd=repo,
        capture_output=True,
    )
    return completed.stdout.strip()


def _write_harness(root: Path, source_root: Path, package_name: str) -> Path:
    harness = root / package_name
    (harness / "src").mkdir(parents=True)
    manifest = f'''[package]
name = "{package_name}"
version = "0.1.0"
edition = "2024"

[dependencies]
base64 = "0.22"
blake3 = "1"
camino = "1"
flate2 = "1"
serde_json = "1"
tex-checkpoint = {{ path = "{source_root / 'crates/tex-checkpoint'}" }}
tex-tokens = {{ path = "{source_root / 'crates/tex-tokens'}" }}
tex-vm = {{ path = "{source_root / 'crates/tex-vm'}" }}

[features]
candidate = []

[profile.dev]
debug = 0
incremental = false
'''
    (harness / "Cargo.toml").write_text(manifest, encoding="utf-8")
    (harness / "src/main.rs").write_text(HARNESS_SOURCE, encoding="utf-8")
    return harness / "Cargo.toml"


def _read_result(binary: Path, mode: str, path: Path, cwd: Path) -> dict[str, Any]:
    completed = _run(
        [str(binary), mode, str(path)],
        cwd=cwd,
        capture_output=True,
    )
    return json.loads(completed.stdout)


def _read_envelope_payload(path: Path) -> dict[str, Any]:
    envelope = json.loads(path.read_text(encoding="utf-8"))
    compressed = base64.b64decode(envelope["payload"], validate=True)
    return json.loads(gzip.decompress(compressed))


def _write_wrapped_payload(
    binary: Path,
    payload: dict[str, Any],
    raw_path: Path,
    envelope_path: Path,
    cwd: Path,
) -> None:
    raw_path.write_text(json.dumps(payload), encoding="utf-8")
    _run(
        [str(binary), "wrap-raw", str(raw_path), str(envelope_path)],
        cwd=cwd,
    )


def characterize_pre_reader(repo: Path, baseline: str) -> dict[str, Any]:
    baseline_commit = _resolve_revision(repo, baseline)
    with tempfile.TemporaryDirectory(prefix="latexd-v3-snapshot-migration-") as temp:
        temp_root = Path(temp)
        baseline_root = temp_root / "baseline"
        added_worktree = False
        try:
            _run(
                ["git", "worktree", "add", "--detach", str(baseline_root), baseline_commit],
                cwd=repo,
            )
            added_worktree = True
            baseline_package = "v3-snapshot-migration-baseline"
            manifest = _write_harness(temp_root, baseline_root, baseline_package)
            baseline_target_dir = temp_root / "baseline-target"
            baseline_cargo_env = os.environ.copy()
            baseline_cargo_env.update(
                {
                    "CARGO_INCREMENTAL": "0",
                    "CARGO_PROFILE_DEV_DEBUG": "0",
                    "CARGO_TARGET_DIR": str(baseline_target_dir),
                    "RUSTFLAGS": "-C debuginfo=0",
                }
            )
            _run(
                ["cargo", "build", "--quiet", "--manifest-path", str(manifest)],
                cwd=repo,
                env=baseline_cargo_env,
            )
            binary = baseline_target_dir / "debug" / baseline_package

            candidate_package = "v3-snapshot-migration-candidate"
            candidate_manifest = _write_harness(temp_root, repo, candidate_package)
            candidate_target_dir = temp_root / "candidate-target"
            candidate_cargo_env = os.environ.copy()
            candidate_cargo_env.update(
                {
                    "CARGO_INCREMENTAL": "0",
                    "CARGO_PROFILE_DEV_DEBUG": "0",
                    "CARGO_TARGET_DIR": str(candidate_target_dir),
                    "RUSTFLAGS": "-C debuginfo=0",
                }
            )
            _run(
                [
                    "cargo",
                    "build",
                    "--quiet",
                    "--manifest-path",
                    str(candidate_manifest),
                    "--features",
                    "candidate",
                ],
                cwd=repo,
                env=candidate_cargo_env,
            )
            candidate_binary = candidate_target_dir / "debug" / candidate_package

            raw_path = temp_root / "raw.json"
            _run([str(binary), "produce-raw", str(raw_path)], cwd=repo)
            raw = json.loads(raw_path.read_text(encoding="utf-8"))

            field_only = copy.deepcopy(raw)
            field_only["muskip_registers"] = {"0": 123}
            field_only["next_muskip_register"] = 1
            field_only_path = temp_root / "raw-field-only.json"
            field_only_path.write_text(json.dumps(field_only), encoding="utf-8")

            versioned_document = {
                "format": "latexd.vm-snapshot",
                "schema_version": 1,
                "required_capabilities": ["eqtb.muskip.scalar-v1"],
                "state": field_only,
            }
            versioned_document_path = temp_root / "raw-versioned-document.json"
            versioned_document_path.write_text(
                json.dumps(versioned_document), encoding="utf-8"
            )

            bundle_path = temp_root / "bundle.json"
            _run([str(binary), "produce-bundle", str(bundle_path)], cwd=repo)
            bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
            legacy_snapshot = bundle["checkpoints"][0]["snapshot"]
            versioned_slot = {
                "document": versioned_document,
            }

            versioned_only = copy.deepcopy(bundle)
            versioned_only["checkpoints"][0]["snapshot"] = None
            versioned_only["checkpoints"][0]["versioned_snapshot"] = versioned_slot
            versioned_only_path = temp_root / "checkpoint-versioned-only.json"
            versioned_only_path.write_text(
                json.dumps(versioned_only), encoding="utf-8"
            )

            dual_lane = copy.deepcopy(bundle)
            dual_lane["checkpoints"][0]["snapshot"] = legacy_snapshot
            dual_lane["checkpoints"][0]["versioned_snapshot"] = versioned_slot
            dual_lane_path = temp_root / "checkpoint-dual-lane.json"
            dual_lane_path.write_text(json.dumps(dual_lane), encoding="utf-8")

            candidate_bundle_path = temp_root / "candidate-bundle.json"
            _run(
                [str(candidate_binary), "produce-bundle", str(candidate_bundle_path)],
                cwd=repo,
            )
            candidate_bundle = json.loads(
                candidate_bundle_path.read_text(encoding="utf-8")
            )
            candidate_checkpoint = candidate_bundle["checkpoints"][0]
            candidate_old_result = _read_result(
                binary, "consume-bundle", candidate_bundle_path, repo
            )

            candidate_envelope_path = temp_root / "candidate-envelope.json"
            _run(
                [
                    str(candidate_binary),
                    "produce-envelope",
                    str(candidate_envelope_path),
                ],
                cwd=repo,
            )
            candidate_envelope_bundle = _read_envelope_payload(candidate_envelope_path)
            candidate_envelope_checkpoint = candidate_envelope_bundle["checkpoints"][0]
            candidate_envelope_old_result = _read_result(
                binary, "consume-envelope", candidate_envelope_path, repo
            )

            baseline_envelope_path = temp_root / "baseline-envelope.json"
            _run(
                [str(binary), "produce-envelope", str(baseline_envelope_path)],
                cwd=repo,
            )
            baseline_envelope_candidate_result = _read_result(
                candidate_binary, "consume-envelope", baseline_envelope_path, repo
            )

            candidate_legacy_snapshot = candidate_checkpoint["snapshot"]
            candidate_document = {
                "format": "latexd.vm-snapshot",
                "schema_version": 1,
                "required_capabilities": [],
                "state": candidate_legacy_snapshot,
            }
            candidate_slot = {"document": candidate_document}

            candidate_versioned_bundle = copy.deepcopy(candidate_bundle)
            candidate_versioned_bundle["checkpoints"][0]["snapshot"] = None
            candidate_versioned_bundle["checkpoints"][0][
                "versioned_snapshot"
            ] = candidate_slot
            candidate_versioned_envelope = temp_root / "candidate-versioned-envelope.json"
            _write_wrapped_payload(
                candidate_binary,
                candidate_versioned_bundle,
                temp_root / "candidate-versioned-payload.json",
                candidate_versioned_envelope,
                repo,
            )

            candidate_muskip_document_path = temp_root / "candidate-muskip-document.json"
            _run(
                [
                    str(candidate_binary),
                    "produce-muskip-document",
                    str(candidate_muskip_document_path),
                ],
                cwd=repo,
            )
            candidate_muskip_document = json.loads(
                candidate_muskip_document_path.read_text(encoding="utf-8")
            )
            candidate_muskip_bundle = copy.deepcopy(candidate_versioned_bundle)
            candidate_muskip_bundle["checkpoints"][0]["versioned_snapshot"] = {
                "document": candidate_muskip_document
            }
            candidate_muskip_envelope = temp_root / "candidate-muskip-envelope.json"
            _write_wrapped_payload(
                candidate_binary,
                candidate_muskip_bundle,
                temp_root / "candidate-muskip-payload.json",
                candidate_muskip_envelope,
                repo,
            )

            duplicate_muskip_payload = json.dumps(
                candidate_muskip_bundle, separators=(",", ":")
            )
            cursor_member = '"next_muskip_register":301'
            if duplicate_muskip_payload.count(cursor_member) != 1:
                raise AssertionError("expected one muskip cursor member in fixture")
            duplicate_muskip_payload = duplicate_muskip_payload.replace(
                cursor_member,
                '"next_muskip_register":"invalid",' + cursor_member,
            )
            duplicate_muskip_payload_path = (
                temp_root / "candidate-duplicate-muskip-payload.json"
            )
            duplicate_muskip_payload_path.write_text(
                duplicate_muskip_payload, encoding="utf-8"
            )
            duplicate_muskip_envelope = (
                temp_root / "candidate-duplicate-muskip-envelope.json"
            )
            _run(
                [
                    str(candidate_binary),
                    "wrap-raw",
                    str(duplicate_muskip_payload_path),
                    str(duplicate_muskip_envelope),
                ],
                cwd=repo,
            )

            candidate_dual_bundle = copy.deepcopy(candidate_bundle)
            candidate_dual_bundle["checkpoints"][0]["versioned_snapshot"] = candidate_slot
            candidate_dual_envelope = temp_root / "candidate-dual-envelope.json"
            _write_wrapped_payload(
                candidate_binary,
                candidate_dual_bundle,
                temp_root / "candidate-dual-payload.json",
                candidate_dual_envelope,
                repo,
            )

            unsupported_bundle = copy.deepcopy(candidate_versioned_bundle)
            unsupported_bundle["checkpoints"][0]["versioned_snapshot"] = {
                "document": {
                    "format": "latexd.vm-snapshot",
                    "schema_version": 1,
                    "required_capabilities": ["future.capability-v1"],
                    "state": "must not be decoded",
                }
            }
            unsupported_envelope = temp_root / "candidate-unsupported-envelope.json"
            _write_wrapped_payload(
                candidate_binary,
                unsupported_bundle,
                temp_root / "candidate-unsupported-payload.json",
                unsupported_envelope,
                repo,
            )

            malformed_bundle = copy.deepcopy(candidate_versioned_bundle)
            malformed_bundle["checkpoints"][0]["versioned_snapshot"] = {
                "document": {
                    "format": "latexd.vm-snapshot",
                    "schema_version": 1,
                    "required_capabilities": [],
                    "state": "not a VM snapshot",
                }
            }
            malformed_envelope = temp_root / "candidate-malformed-envelope.json"
            _write_wrapped_payload(
                candidate_binary,
                malformed_bundle,
                temp_root / "candidate-malformed-payload.json",
                malformed_envelope,
                repo,
            )

            return {
                "raw_field_only": _read_result(
                    binary, "consume-raw", field_only_path, repo
                ),
                "checkpoint_versioned_only": _read_result(
                    binary, "consume-bundle", versioned_only_path, repo
                ),
                "checkpoint_dual_lane": _read_result(
                    binary, "consume-bundle", dual_lane_path, repo
                ),
                "raw_versioned_document": _read_result(
                    binary, "consume-raw", versioned_document_path, repo
                ),
                "canonical_muskip_document_to_pre_reader": _read_result(
                    binary, "consume-raw", candidate_muskip_document_path, repo
                ),
                "candidate_legacy_bundle_to_pre_reader": {
                    **candidate_old_result,
                    "versioned_field_present": "versioned_snapshot"
                    in candidate_checkpoint,
                    "muskip_field_present": any(
                        field in candidate_checkpoint["snapshot"]
                        for field in ("muskip_registers", "next_muskip_register")
                    ),
                },
                "candidate_envelope_to_pre_reader": {
                    **candidate_envelope_old_result,
                    "versioned_field_present": "versioned_snapshot"
                    in candidate_envelope_checkpoint,
                    "muskip_field_present": any(
                        field in candidate_envelope_checkpoint["snapshot"]
                        for field in ("muskip_registers", "next_muskip_register")
                    ),
                },
                "pre_reader_envelope_to_candidate": baseline_envelope_candidate_result,
                "candidate_versioned_envelope": _read_result(
                    candidate_binary, "consume-reuse", candidate_versioned_envelope, repo
                ),
                "candidate_supported_muskip_capability_envelope": _read_result(
                    candidate_binary,
                    "consume-muskip-reuse",
                    candidate_muskip_envelope,
                    repo,
                ),
                "supported_muskip_envelope_to_pre_reader": _read_result(
                    binary, "consume-envelope", candidate_muskip_envelope, repo
                ),
                "candidate_duplicate_muskip_member_envelope": _read_result(
                    candidate_binary,
                    "consume-reuse",
                    duplicate_muskip_envelope,
                    repo,
                ),
                "candidate_dual_lane_envelope": _read_result(
                    candidate_binary, "consume-reuse", candidate_dual_envelope, repo
                ),
                "candidate_unsupported_capability_envelope": _read_result(
                    candidate_binary, "consume-reuse", unsupported_envelope, repo
                ),
                "candidate_malformed_document_envelope": _read_result(
                    candidate_binary, "consume-reuse", malformed_envelope, repo
                ),
            }
        finally:
            if added_worktree:
                subprocess.run(
                    ["git", "worktree", "remove", "--force", str(baseline_root)],
                    cwd=repo,
                    check=False,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", default=DEFAULT_BASELINE)
    parser.add_argument(
        "--repo",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
    )
    args = parser.parse_args(argv)

    results = characterize_pre_reader(args.repo.resolve(), args.baseline)
    print(json.dumps(results, sort_keys=True))
    violations = validate_pre_reader_results(results)
    if violations:
        print("V3 snapshot migration pre-reader characterization failed:")
        for violation in violations:
            print(f"- {violation}")
        return 1
    print(f"V3 snapshot migration pre-reader characterization passed ({args.baseline})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
