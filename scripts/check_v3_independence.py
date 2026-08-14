#!/usr/bin/env python3
"""Guard a bounded V3 ownership migration against unrelated identity work."""

from __future__ import annotations

import argparse
import re
import shlex
import subprocess
import sys
from pathlib import Path


ALLOWED_PRODUCTION_PATHS = frozenset(
    {
        "crates/tex-vm/src/control_sequence_scopes.rs",
        "crates/tex-vm/src/eqtb.rs",
        "crates/tex-vm/src/lib.rs",
        "crates/tex-vm/src/save_stack.rs",
        "crates/tex-vm/src/semantic_front_matter.rs",
    }
)
NON_PRODUCTION_PREFIXES = (
    "crates/tex-vm/tests/",
    "docs/",
)
NON_PRODUCTION_PATHS = frozenset({"PLAN.md"})
V3_MIGRATION_MARKERS = frozenset(
    {
        "crates/tex-vm/src/control_sequence_scopes.rs",
        "crates/tex-vm/src/eqtb.rs",
        "crates/tex-vm/src/save_stack.rs",
    }
)
V3_OWNER_SYMBOL_MARKERS = ("ControlSequence", "control_sequence")
FORBIDDEN_IDENTIFIERS = (
    "DependencyId",
    "DepTrace",
    "EventMeta",
    "EventSequence",
    "ExecutedSourceSlice",
    "ExpansionId",
    "FileId",
    "IdentityContextId",
    "ProvenanceSpan",
    "RevisionId",
    "ScopedControlSequenceId",
    "SourceProvenance",
    "SourceRevision",
    "SourceSpan",
    "StableEventId",
)


def _is_allowed_path(path: str) -> bool:
    return (
        path in ALLOWED_PRODUCTION_PATHS
        or path in NON_PRODUCTION_PATHS
        or path.startswith(NON_PRODUCTION_PREFIXES)
    )


def _diff_path(line: str) -> str | None:
    try:
        fields = shlex.split(line)
    except ValueError:
        return None
    if len(fields) != 4 or fields[:2] != ["diff", "--git"]:
        return None
    path = fields[3]
    return path[2:] if path.startswith("b/") else path


def _added_line_violations(path: str, line_number: int, source: str) -> list[str]:
    violations = []
    source = re.sub(r'"(?:\\.|[^"\\])*"', '""', source)
    source = source.split("//", 1)[0]
    for identifier in FORBIDDEN_IDENTIFIERS:
        if re.search(rf"\b{re.escape(identifier)}\b", source):
            violations.append(f"{path}:{line_number}: added forbidden symbol {identifier}")

    if re.search(r"\bpub(?:\s*\([^)]*\))?\s+.*\bControlSequenceId\b", source):
        violations.append(
            f"{path}:{line_number}: added durable/public ControlSequenceId surface"
        )

    if path in ALLOWED_PRODUCTION_PATHS and re.search(
        r"\b(?:Serialize|Deserialize)\b", source
    ):
        violations.append(
            f"{path}:{line_number}: added snapshot persistence to the V3 owner surface"
        )
    return violations


def check_patch(patch: str) -> list[str]:
    """Return policy violations found in a zero-context unified diff."""

    violations: list[str] = []
    current_path: str | None = None
    current_path_allowed = False
    new_line_number = 0

    for line in patch.splitlines():
        if line.startswith("diff --git "):
            current_path = _diff_path(line)
            current_path_allowed = current_path is not None and _is_allowed_path(current_path)
            if current_path is None:
                violations.append("could not parse a diff path")
            elif not current_path_allowed:
                violations.append(f"{current_path}: path is outside the bounded V3 migration")
            continue

        if line.startswith("@@ "):
            match = re.search(r"\+(\d+)(?:,\d+)?", line)
            new_line_number = int(match.group(1)) - 1 if match else 0
            continue

        if line.startswith("+++") or current_path is None:
            continue
        if line.startswith("+"):
            new_line_number += 1
            if current_path_allowed and current_path in ALLOWED_PRODUCTION_PATHS:
                violations.extend(
                    _added_line_violations(current_path, new_line_number, line[1:])
                )
        elif not line.startswith("-"):
            new_line_number += 1

    return violations


def check_migration_patch(patch: str) -> list[str]:
    """Check a commit only when it changes V3 control-sequence owner symbols."""

    current_path: str | None = None
    touches_v3_owner = False
    for line in patch.splitlines():
        if line.startswith("diff --git "):
            current_path = _diff_path(line)
            if current_path == "crates/tex-vm/src/control_sequence_scopes.rs":
                touches_v3_owner = True
            continue
        if (
            current_path in V3_MIGRATION_MARKERS
            and line[:1] in {"+", "-"}
            and not line.startswith(("+++", "---"))
            and any(marker in line[1:] for marker in V3_OWNER_SYMBOL_MARKERS)
        ):
            touches_v3_owner = True

    if not touches_v3_owner:
        return []
    return check_patch(patch)


def _git_diff(base: str, head: str) -> str:
    completed = subprocess.run(
        ["git", "diff", "--unified=0", "--no-ext-diff", f"{base}..{head}", "--"],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    )
    return completed.stdout


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--base", help="base Git revision for the migration diff")
    source.add_argument("--patch-file", type=Path, help="precomputed unified diff")
    parser.add_argument("--head", default="HEAD", help="head Git revision (default: HEAD)")
    parser.add_argument(
        "--only-if-v3-touched",
        action="store_true",
        help="skip commits that do not change V3 control-sequence ownership symbols",
    )
    args = parser.parse_args(argv)

    patch = (
        args.patch_file.read_text(encoding="utf-8")
        if args.patch_file is not None
        else _git_diff(args.base, args.head)
    )
    violations = (
        check_migration_patch(patch) if args.only_if_v3_touched else check_patch(patch)
    )
    if violations:
        print("V3 independence guard failed:", file=sys.stderr)
        for violation in violations:
            print(f"- {violation}", file=sys.stderr)
        return 1

    print("V3 independence guard passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
