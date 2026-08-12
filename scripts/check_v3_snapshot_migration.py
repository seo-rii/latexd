#!/usr/bin/env python3
"""Characterize the exact pre-reader V3 snapshot compatibility boundary."""

from __future__ import annotations

import argparse
import copy
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
}

HARNESS_SOURCE = r'''use std::{env, fs, path::Path};

use camino::Utf8Path;
use serde_json::json;
use tex_checkpoint::{
    build_checkpoint_bundle, checkpoint_is_replay_safe, load_checkpoint_bundle,
};
use tex_tokens::ControlSequenceInterner;
use tex_vm::{Vm, VmSnapshot};

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

fn main() {
    let mut arguments = env::args_os().skip(1);
    let mode = arguments.next().expect("mode");
    let path = arguments.next().expect("path");
    assert!(arguments.next().is_none(), "unexpected argument");
    match mode.to_str().expect("UTF-8 mode") {
        "produce-raw" => produce_raw(Path::new(&path)),
        "consume-raw" => consume_raw(Path::new(&path)),
        "produce-bundle" => produce_bundle(Path::new(&path)),
        "consume-bundle" => consume_bundle(Path::new(&path)),
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


def _write_harness(root: Path, source_root: Path) -> Path:
    harness = root / "v3-snapshot-migration-baseline"
    (harness / "src").mkdir(parents=True)
    manifest = f'''[package]
name = "v3-snapshot-migration-baseline"
version = "0.1.0"
edition = "2024"

[dependencies]
camino = "1"
serde_json = "1"
tex-checkpoint = {{ path = "{source_root / 'crates/tex-checkpoint'}" }}
tex-tokens = {{ path = "{source_root / 'crates/tex-tokens'}" }}
tex-vm = {{ path = "{source_root / 'crates/tex-vm'}" }}

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
            manifest = _write_harness(temp_root, baseline_root)
            target_dir = temp_root / "target"
            cargo_env = os.environ.copy()
            cargo_env.update(
                {
                    "CARGO_INCREMENTAL": "0",
                    "CARGO_PROFILE_DEV_DEBUG": "0",
                    "CARGO_TARGET_DIR": str(target_dir),
                    "RUSTFLAGS": "-C debuginfo=0",
                }
            )
            _run(
                ["cargo", "build", "--quiet", "--manifest-path", str(manifest)],
                cwd=repo,
                env=cargo_env,
            )
            binary = target_dir / "debug" / "v3-snapshot-migration-baseline"

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
                "state_hash": "characterization-only",
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
