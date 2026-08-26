#!/usr/bin/env python3
"""Validate the TeX82 TFM source-rule ledger as a fail-closed policy."""

from __future__ import annotations

import json
import re
from collections import Counter
from pathlib import Path


EXPECTED_RULE_ORDER = (
    "TFM-SIZE-001",
    "TFM-COUNT-001",
    "TFM-RANGE-001",
    "TFM-RANGE-002",
    "TFM-RANGE-003",
    "TFM-GEOMETRY-001",
    "TFM-GEOMETRY-002",
    "TFM-RESOURCE-001",
    "TFM-HEADER-001",
    "TFM-HEADER-002",
    "TFM-CHAR-001",
    "TFM-CHAR-002",
    "TFM-CHARLIST-001",
    "TFM-CHARLIST-002",
    "TFM-BOX-001",
    "TFM-BOX-002",
    "TFM-BOX-003",
    "TFM-LIGKERN-001",
    "TFM-LIGKERN-002",
    "TFM-LIGKERN-003",
    "TFM-LIGKERN-004",
    "TFM-LIGKERN-005",
    "TFM-LIGKERN-006",
    "TFM-LIGKERN-007",
    "TFM-LIGKERN-008",
    "TFM-KERN-001",
    "TFM-EXT-001",
    "TFM-EXT-002",
    "TFM-PARAM-001",
    "TFM-PARAM-002",
    "TFM-PARAM-003",
    "TFM-EOF-001",
    "TFM-EOF-002",
)

RULE_PHASE = {
    rule_id: phase
    for phase, rule_ids in enumerate(
        (
            ("TFM-SIZE-001",),
            (
                "TFM-COUNT-001",
                "TFM-RANGE-001",
                "TFM-RANGE-002",
                "TFM-RANGE-003",
                "TFM-GEOMETRY-001",
                "TFM-GEOMETRY-002",
                "TFM-RESOURCE-001",
            ),
            ("TFM-HEADER-001", "TFM-HEADER-002"),
            (
                "TFM-CHAR-001",
                "TFM-CHAR-002",
                "TFM-CHARLIST-001",
                "TFM-CHARLIST-002",
            ),
            ("TFM-BOX-001", "TFM-BOX-002", "TFM-BOX-003"),
            (
                "TFM-LIGKERN-001",
                "TFM-LIGKERN-002",
                "TFM-LIGKERN-003",
                "TFM-LIGKERN-004",
                "TFM-LIGKERN-005",
                "TFM-LIGKERN-006",
                "TFM-LIGKERN-007",
                "TFM-LIGKERN-008",
                "TFM-KERN-001",
            ),
            (
                "TFM-EXT-001",
                "TFM-EXT-002",
                "TFM-PARAM-001",
                "TFM-PARAM-002",
                "TFM-PARAM-003",
                "TFM-EOF-001",
                "TFM-EOF-002",
            ),
        )
    )
    for rule_id in rule_ids
}

PHASE_MARKERS = {
    0: ("Precondition before byte decoding",),
    1: (
        "Private preamble",
        "Private normalization",
        "Private checked layout",
        "Explicitly excluded",
    ),
    2: ("Private header",),
    3: ("Private character", "Private charlist", "Private bounded graph"),
    4: ("Private exact-scaling", "Private at-size"),
    5: ("Private lig/kern", "Private state-machine", "Private kern"),
    6: ("Private extensible", "Private parameter", "Private frame"),
}


def validate_rule_ledger(ledger: str, fixture_case_ids: set[str]) -> list[str]:
    errors: list[str] = []
    lines = ledger.splitlines()
    try:
        header_index = next(
            index for index, line in enumerate(lines) if line.startswith("| Rule id |")
        )
    except StopIteration:
        return ["source rule table header is missing"]

    rows: list[tuple[str, str, str, str]] = []
    for line in lines[header_index + 2 :]:
        if not line.startswith("|"):
            break
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) != 5:
            errors.append(f"source rule row has {len(cells)} cells instead of 5: {line}")
            continue
        match = re.fullmatch(r"`([A-Z0-9-]+)`", cells[0])
        if match is None:
            errors.append(f"invalid source rule id cell: {cells[0]}")
            continue
        rows.append((match.group(1), cells[2], cells[3], cells[4]))

    rule_ids = [row[0] for row in rows]
    duplicates = sorted(
        rule_id for rule_id, count in Counter(rule_ids).items() if count > 1
    )
    if duplicates:
        errors.append(f"duplicate rule ids: {', '.join(duplicates)}")
    if tuple(rule_ids) != EXPECTED_RULE_ORDER:
        errors.append(
            "source rule order differs from the pinned read_font_info order: "
            + ", ".join(rule_ids)
        )

    phases = [RULE_PHASE.get(rule_id, -1) for rule_id in rule_ids]
    if any(current > following for current, following in zip(phases, phases[1:])):
        errors.append("private implementation phase order is not monotonic")

    missing_dependencies = [rule_id for rule_id, dependency, _, _ in rows if not dependency]
    if missing_dependencies:
        errors.append("missing dependencies: " + ", ".join(missing_dependencies))

    misplaced_phases = []
    referenced_cases: set[str] = set()
    unknown_witnesses: set[str] = set()
    for rule_id, _, native_evidence, future_phase in rows:
        phase = RULE_PHASE.get(rule_id)
        if phase is None or not any(
            marker in future_phase for marker in PHASE_MARKERS[phase]
        ):
            misplaced_phases.append(rule_id)
        for token in re.findall(r"`([a-z][a-z0-9_]*)`", native_evidence):
            if token in fixture_case_ids:
                referenced_cases.add(token)
            else:
                unknown_witnesses.add(token)

    if misplaced_phases:
        errors.append("rules have missing or incorrect phase cells: " + ", ".join(misplaced_phases))
    if unknown_witnesses:
        errors.append("unknown native witnesses: " + ", ".join(sorted(unknown_witnesses)))

    unmapped_cases = sorted(fixture_case_ids - referenced_cases)
    if unmapped_cases:
        errors.append("unmapped fixture cases: " + ", ".join(unmapped_cases))
    return errors


def main() -> int:
    root = Path(__file__).parents[1]
    ledger = (root / "docs/tex82-read-font-info-validation-rules.md").read_text(
        encoding="utf-8"
    )
    fixture = json.loads(
        (
            root
            / "crates/tex-tfm-metrics/tests/fixtures/tfm-validity-oracle-v1.json"
        ).read_text(encoding="utf-8")
    )
    errors = validate_rule_ledger(ledger, set(fixture["case_results"]))
    if errors:
        for error in errors:
            print(error)
        return 1
    print("TeX82 TFM validation source-rule ledger passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
