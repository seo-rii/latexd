#!/usr/bin/env python3
"""Run the V3 control-sequence snapshot contract across two real revisions."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import tempfile
from pathlib import Path
from typing import Any


DEFAULT_BASELINE = "f66cdbf"
EXPECTED_OUTPUT = "LRRMZ"
EXPECTED_MEANING_KINDS = {
    "vthreealias": "macro",
    "vthreeprimitive": "primitive",
    "vthreeroot": "macro",
    "vthreetoken": "token",
}
EXPECTED_LAYERED_OUTPUT = "LGZLGALGARGA"
EXPECTED_LAYERED_KEYS = [
    {"vthreex", "vthreey"},
    {"vthreex"},
    set(),
    {"vthreex", "vthreez"},
]

HARNESS_SOURCE = r'''use std::{collections::BTreeMap, env, fs, path::Path};

use serde_json::json;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{SnapshotMeaning, Vm, VmSnapshot};

fn produce(path: &Path, scenario: &str) {
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::new(&mut interner);
    match scenario {
        "basic" => {
            vm.run_plain(
                r"\def\vthreeroot{R}\let\vthreealias=\vthreeroot\let\vthreeprimitive=\def\let\vthreetoken=Z{\def\vthreeroot{L}",
            );
        }
        "layered" => {
            vm.run_plain(
                r"\def\vthreex{R}\def\vthreey{R}{\def\vthreex{L}\def\vthreey{A}{{\def\vthreex{L}\def\vthreez{Z}\def\vthreey{B}\global\def\vthreey{G}",
            );
        }
        other => panic!("unsupported scenario: {other}"),
    }
    fs::write(
        path,
        serde_json::to_vec(&vm.snapshot()).expect("serialize snapshot"),
    )
    .expect("write snapshot");
}

fn consume(path: &Path, scenario: &str) {
    let snapshot = serde_json::from_slice::<VmSnapshot>(&fs::read(path).expect("read snapshot"))
        .expect("deserialize snapshot");
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::restore(&mut interner, &snapshot);
    let projected_before_continuation = (scenario == "layered").then(|| vm.snapshot().scopes);
    let outcome = match scenario {
        "basic" => vm.run_plain(
            r"\vthreeroot}\vthreeroot\vthreealias\vthreeprimitive\vthreemade{M}\vthreemade\vthreetoken",
        ),
        "layered" => vm.run_plain(
            r"\vthreex\vthreey\vthreez}\vthreex\vthreey\ifdefined\vthreez BAD\else A\fi}\vthreex\vthreey\ifdefined\vthreez BAD\else A\fi}\vthreex\vthreey\ifdefined\vthreez BAD\else A\fi",
        ),
        other => panic!("unsupported scenario: {other}"),
    };
    let scopes = projected_before_continuation
        .unwrap_or_else(|| vm.snapshot().scopes)
        .into_iter()
        .map(|scope| {
            scope
                .into_iter()
                .filter(|(name, _)| name.starts_with("vthree"))
                .collect::<BTreeMap<String, SnapshotMeaning>>()
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string(&json!({
            "output": outcome.output,
            "diagnostic_count": outcome.diagnostics.len(),
            "scopes": scopes,
        }))
        .expect("serialize result")
    );
}

fn main() {
    let mut arguments = env::args_os().skip(1);
    let mode = arguments.next().expect("mode");
    let scenario = arguments.next().expect("scenario");
    let path = arguments.next().expect("snapshot path");
    assert!(arguments.next().is_none(), "unexpected argument");
    match mode.to_str().expect("UTF-8 mode") {
        "produce" => produce(Path::new(&path), scenario.to_str().expect("UTF-8 scenario")),
        "consume" => consume(Path::new(&path), scenario.to_str().expect("UTF-8 scenario")),
        other => panic!("unsupported mode: {other}"),
    }
}
'''


def validate_matrix(
    old_to_new: dict[str, Any], new_to_old: dict[str, Any]
) -> list[str]:
    violations: list[str] = []
    for direction, result in (
        ("old-to-new", old_to_new),
        ("new-to-old", new_to_old),
    ):
        if result.get("output") != EXPECTED_OUTPUT:
            violations.append(
                f"{direction} output mismatch: {result.get('output')!r}"
            )
        if result.get("diagnostic_count") != 0:
            violations.append(
                f"{direction} diagnostic count: {result.get('diagnostic_count')!r}"
            )
        scopes = result.get("scopes")
        if not isinstance(scopes, list) or len(scopes) != 1:
            violations.append(f"{direction} scope depth mismatch: {scopes!r}")
            continue
        root = scopes[0]
        if not isinstance(root, dict):
            violations.append(f"{direction} root scope is not an object")
            continue
        for name, expected_kind in EXPECTED_MEANING_KINDS.items():
            meaning = root.get(name)
            actual_kind = meaning.get("kind") if isinstance(meaning, dict) else None
            if actual_kind != expected_kind:
                violations.append(
                    f"{direction} scope meaning mismatch for {name}: {actual_kind!r}"
                )
    if old_to_new != new_to_old:
        violations.append("old-to-new and new-to-old directions differ")
    return violations


def validate_layered_matrix(
    old_to_new: dict[str, Any], new_to_old: dict[str, Any]
) -> list[str]:
    violations: list[str] = []
    for direction, result in (
        ("old-to-new layered", old_to_new),
        ("new-to-old layered", new_to_old),
    ):
        if result.get("output") != EXPECTED_LAYERED_OUTPUT:
            violations.append(
                f"{direction} output mismatch: {result.get('output')!r}"
            )
        if result.get("diagnostic_count") != 0:
            violations.append(
                f"{direction} diagnostic count: {result.get('diagnostic_count')!r}"
            )
        scopes = result.get("scopes")
        if not isinstance(scopes, list) or len(scopes) != len(EXPECTED_LAYERED_KEYS):
            violations.append(f"{direction} scope depth mismatch: {scopes!r}")
            continue
        for index, (scope, expected_keys) in enumerate(
            zip(scopes, EXPECTED_LAYERED_KEYS, strict=True)
        ):
            if not isinstance(scope, dict) or set(scope) != expected_keys:
                violations.append(
                    f"{direction} scope {index} keys mismatch: {scope!r}"
                )
                continue
            for name, meaning in scope.items():
                actual_kind = meaning.get("kind") if isinstance(meaning, dict) else None
                if actual_kind != "macro":
                    violations.append(
                        f"{direction} scope {index} meaning mismatch for {name}: {actual_kind!r}"
                    )
    if old_to_new != new_to_old:
        violations.append("old-to-new and new-to-old layered directions differ")
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


def _add_worktree(repo: Path, path: Path, revision: str) -> None:
    _run(
        ["git", "worktree", "add", "--detach", str(path), revision],
        cwd=repo,
    )


def _remove_worktree(repo: Path, path: Path) -> None:
    subprocess.run(
        ["git", "worktree", "remove", "--force", str(path)],
        cwd=repo,
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def _write_harness(root: Path, package_name: str, source_root: Path) -> Path:
    harness = root / package_name
    (harness / "src").mkdir(parents=True)
    manifest = f'''[package]
name = "{package_name}"
version = "0.1.0"
edition = "2024"

[dependencies]
serde_json = "1"
tex-tokens = {{ path = "{source_root / 'crates/tex-tokens'}" }}
tex-vm = {{ path = "{source_root / 'crates/tex-vm'}" }}

[profile.dev]
debug = 0
incremental = false
'''
    (harness / "Cargo.toml").write_text(manifest, encoding="utf-8")
    (harness / "src/main.rs").write_text(HARNESS_SOURCE, encoding="utf-8")
    return harness / "Cargo.toml"


def _build_harness(
    repo: Path,
    temp_root: Path,
    label: str,
    source_root: Path,
    target_dir: Path,
) -> Path:
    package_name = f"v3-cross-version-{label}"
    manifest = _write_harness(temp_root, package_name, source_root)
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
    return target_dir / "debug" / package_name


def _run_fixture(
    producer: Path,
    consumer: Path,
    scenario: str,
    snapshot: Path,
    cwd: Path,
) -> dict[str, Any]:
    _run([str(producer), "produce", scenario, str(snapshot)], cwd=cwd)
    completed = _run(
        [str(consumer), "consume", scenario, str(snapshot)],
        cwd=cwd,
        capture_output=True,
    )
    return json.loads(completed.stdout)


def run_matrix(repo: Path, baseline: str, candidate: str) -> list[str]:
    baseline_commit = _resolve_revision(repo, baseline)
    candidate_commit = _resolve_revision(repo, candidate)
    with tempfile.TemporaryDirectory(prefix="latexd-v3-cross-version-") as temp:
        temp_root = Path(temp)
        baseline_root = temp_root / "baseline"
        candidate_root = temp_root / "candidate"
        added_worktrees: list[Path] = []
        try:
            _add_worktree(repo, baseline_root, baseline_commit)
            added_worktrees.append(baseline_root)
            _add_worktree(repo, candidate_root, candidate_commit)
            added_worktrees.append(candidate_root)
            target_dir = temp_root / "target"
            baseline_binary = _build_harness(
                repo, temp_root, "baseline", baseline_root, target_dir
            )
            candidate_binary = _build_harness(
                repo, temp_root, "candidate", candidate_root, target_dir
            )

            violations: list[str] = []
            for scenario, validator in (
                ("basic", validate_matrix),
                ("layered", validate_layered_matrix),
            ):
                old_to_new = _run_fixture(
                    baseline_binary,
                    candidate_binary,
                    scenario,
                    temp_root / f"old-{scenario}-snapshot.json",
                    repo,
                )
                new_to_old = _run_fixture(
                    candidate_binary,
                    baseline_binary,
                    scenario,
                    temp_root / f"new-{scenario}-snapshot.json",
                    repo,
                )
                violations.extend(validator(old_to_new, new_to_old))
            return violations
        finally:
            for worktree in reversed(added_worktrees):
                _remove_worktree(repo, worktree)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", default=DEFAULT_BASELINE)
    parser.add_argument("--candidate", default="HEAD")
    parser.add_argument(
        "--repo",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
    )
    args = parser.parse_args(argv)

    violations = run_matrix(args.repo.resolve(), args.baseline, args.candidate)
    if violations:
        print("V3 cross-version matrix failed:")
        for violation in violations:
            print(f"- {violation}")
        return 1
    print(
        "V3 cross-version matrix passed "
        f"(baseline={args.baseline}, candidate={args.candidate})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
