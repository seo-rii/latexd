#!/usr/bin/env python3
"""Run the pinned native release contract for versioned checkpoint snapshots."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import tempfile
from pathlib import Path


RUST_TOOLCHAIN = "1.94.0"
RELEASE_TARGET = "x86_64-unknown-linux-gnu"
CARGO_FEATURE_MODE = "default"
EXPECTED_SERDE_JSON_FEATURES = {"default", "raw_value", "std"}


def validate_serde_json_features(feature_graph: str) -> list[str]:
    actual = set(re.findall(r'serde_json feature "([^"]+)"', feature_graph))
    violations: list[str] = []
    for missing in sorted(EXPECTED_SERDE_JSON_FEATURES - actual):
        violations.append(f"missing serde_json feature {missing!r}")
    for unexpected in sorted(actual - EXPECTED_SERDE_JSON_FEATURES):
        violations.append(f"unexpected serde_json feature {unexpected!r}")
    return violations


def cargo_feature_graph_command() -> list[str]:
    return ["cargo", "tree", "--locked", "-e", "features", "-p", "latexd"]


def release_commands(target_a: str, target_b: str) -> list[list[str]]:
    common = ["--locked", "--release", "--target", RELEASE_TARGET]
    return [
        [
            "cargo",
            "build",
            *common,
            "--target-dir",
            target_a,
            "-p",
            "latexd",
        ],
        [
            "cargo",
            "test",
            *common,
            "--target-dir",
            target_a,
            "-p",
            "tex-vm",
            "--test",
            "v3_snapshot_document_contract",
        ],
        [
            "cargo",
            "test",
            *common,
            "--target-dir",
            target_b,
            "-p",
            "tex-vm",
            "--test",
            "v3_snapshot_document_contract",
        ],
        [
            "cargo",
            "test",
            *common,
            "--target-dir",
            target_a,
            "-p",
            "tex-checkpoint",
        ],
        [
            "cargo",
            "test",
            *common,
            "--target-dir",
            target_a,
            "-p",
            "latexd",
            "--lib",
            "internal_compiler_rebuilds_source_when_muskip_snapshots_are_suppressed",
        ],
    ]


def build_release_report(
    *,
    feature_graph: str,
    rustc: str,
    revision: str,
    cargo_lock: bytes,
    commands: list[list[str]],
    skip_migration: bool,
) -> dict[str, object]:
    return {
        "rust_toolchain": RUST_TOOLCHAIN,
        "rustc_version": rustc.strip(),
        "target": RELEASE_TARGET,
        "profile": "release",
        "locked": True,
        "cargo_lock_sha256": hashlib.sha256(cargo_lock).hexdigest(),
        "cargo_feature_mode": CARGO_FEATURE_MODE,
        "cargo_feature_graph": feature_graph,
        "serde_json_features": sorted(EXPECTED_SERDE_JSON_FEATURES),
        "serde_json_feature_graph_sha256": hashlib.sha256(
            feature_graph.encode("utf-8")
        ).hexdigest(),
        "repository_revision": revision,
        "commands": commands,
        "migration_profile": None if skip_migration else "release",
    }


def _run(
    command: list[str],
    *,
    cwd: Path,
    env: dict[str, str] | None = None,
    capture_output: bool = False,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=cwd,
        env=env,
        check=True,
        text=True,
        capture_output=capture_output,
    )


def run_release_contract(repo: Path, output: Path, *, skip_migration: bool) -> None:
    rustc = _run(["rustc", "-Vv"], cwd=repo, capture_output=True).stdout
    if f"release: {RUST_TOOLCHAIN}" not in rustc:
        raise RuntimeError(f"release contract requires Rust {RUST_TOOLCHAIN}\n{rustc}")
    if f"host: {RELEASE_TARGET}" not in rustc:
        raise RuntimeError(f"release contract requires host {RELEASE_TARGET}\n{rustc}")

    feature_graph = _run(
        cargo_feature_graph_command(),
        cwd=repo,
        capture_output=True,
    ).stdout
    violations = validate_serde_json_features(feature_graph)
    if violations:
        raise RuntimeError("\n".join(violations))

    release_env = os.environ.copy()
    release_env.update(
        {
            "CARGO_INCREMENTAL": "0",
            "RUSTFLAGS": "-C debuginfo=0",
        }
    )
    with tempfile.TemporaryDirectory(prefix="latexd-snapshot-release-a-") as target_a:
        with tempfile.TemporaryDirectory(prefix="latexd-snapshot-release-b-") as target_b:
            commands = release_commands(target_a, target_b)
            for command in commands:
                _run(command, cwd=repo, env=release_env)
            if not skip_migration:
                _run(
                    [
                        "python3",
                        "scripts/check_v3_snapshot_migration.py",
                        "--cargo-profile",
                        "release",
                        "--target",
                        RELEASE_TARGET,
                    ],
                    cwd=repo,
                    env=release_env,
                )

    revision = _run(
        ["git", "rev-parse", "HEAD"], cwd=repo, capture_output=True
    ).stdout.strip()
    report = build_release_report(
        feature_graph=feature_graph,
        rustc=rustc,
        revision=revision,
        cargo_lock=(repo / "Cargo.lock").read_bytes(),
        commands=commands,
        skip_migration=skip_migration,
    )
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(report, sort_keys=True))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("target/snapshot-release-contract.json"),
    )
    parser.add_argument("--skip-migration", action="store_true")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    output = args.output if args.output.is_absolute() else repo / args.output
    run_release_contract(repo, output, skip_migration=args.skip_migration)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
