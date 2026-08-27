#!/usr/bin/env python3
"""Validate the TeX82 TFM source-rule ledger as a fail-closed policy."""

from __future__ import annotations

import hashlib
import json
import re
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).parents[1]
RULE_CONTRACT_PATH = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validation-rules-v1.json"
)
RULE_CONTRACT = json.loads(RULE_CONTRACT_PATH.read_text(encoding="utf-8"))
PINNED_SOURCE = {
    "url": "https://tug.ctan.org/systems/knuth/dist/tex/tex.web",
    "sha256": "c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324",
    "loader_section": "lines 10870..11210",
    "loader_section_sha256": "57f665ae4cc87c721d444fdde0a1817f194f44bab18388c42a1d26d830c6ddc8",
}
HEADER_CHECKED_RULES = {
    "TFM-COUNT-001",
    "TFM-RANGE-001",
    "TFM-RANGE-002",
    "TFM-RANGE-003",
    "TFM-GEOMETRY-001",
    "TFM-GEOMETRY-002",
    "TFM-HEADER-001",
    "TFM-HEADER-002",
    "TFM-EOF-001",
    "TFM-EOF-002",
}
CHARACTER_CHECKED_RULES = {
    "TFM-CHAR-001",
    "TFM-CHAR-002",
    "TFM-CHARLIST-001",
    "TFM-CHARLIST-002",
}
REVIEWED_V1_CONTRACT_CANONICAL_SHA256 = (
    "cebc062f771f27c5c46e0e83a74ab7c7c9f6e3a172b2cf1fe01bce0a7f6f6c21"
)
REVIEWED_V1_RULE_IDS = (
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


def validate_rule_contract(
    contract: dict[str, object], fixture_case_ids: set[str]
) -> list[str]:
    errors: list[str] = []
    canonical_contract = json.dumps(
        contract,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode()
    if hashlib.sha256(canonical_contract).hexdigest() != (
        REVIEWED_V1_CONTRACT_CANONICAL_SHA256
    ):
        errors.append("reviewed v1 contract digest differs; create a version transition")
    if contract.get("format") != "latexd.tfm-validation-rule-contract":
        errors.append("semantic contract format is invalid")
    if contract.get("schema_version") != 1:
        errors.append("semantic contract schema version is invalid")
    if contract.get("compatibility_source") != PINNED_SOURCE:
        errors.append("semantic contract compatibility source pins differ")

    rules = contract.get("rules")
    invariants = contract.get("invariants")
    proof_states = contract.get("proof_states")
    if not isinstance(rules, list) or not isinstance(invariants, list) or not isinstance(
        proof_states, list
    ):
        return errors + ["semantic contract collections are invalid"]

    rule_ids = [rule.get("id") for rule in rules if isinstance(rule, dict)]
    if tuple(rule_ids) != REVIEWED_V1_RULE_IDS:
        errors.append("reviewed v1 ordered rule ids differ; create a version transition")
    duplicates = sorted(
        rule_id
        for rule_id, count in Counter(rule_ids).items()
        if count > 1 and isinstance(rule_id, str)
    )
    if duplicates:
        errors.append("semantic contract duplicate rule ids: " + ", ".join(duplicates))
    ordinals = [rule.get("source_ordinal") for rule in rules if isinstance(rule, dict)]
    if ordinals != list(range(1, len(rules) + 1)):
        errors.append("semantic contract source ordinals are not exact and contiguous")
    anchors = [rule.get("source_anchor") for rule in rules if isinstance(rule, dict)]
    if any(not isinstance(anchor, str) or not anchor for anchor in anchors) or len(
        set(anchors)
    ) != len(anchors):
        errors.append("semantic contract source anchors are missing or duplicated")

    known_dependencies = set(rule_ids) | set(invariants)
    known_proof_states = set(proof_states)
    referenced_cases: set[str] = set()
    for rule in rules:
        if not isinstance(rule, dict):
            errors.append("semantic contract rule entry is not an object")
            continue
        rule_id = rule.get("id")
        dependencies = rule.get("dependency_ids")
        witnesses = rule.get("witnesses")
        proof_state = rule.get("proof_state")
        if not isinstance(dependencies, list) or any(
            dependency not in known_dependencies for dependency in dependencies
        ):
            errors.append(f"semantic contract dependencies are invalid for {rule_id}")
        if not isinstance(witnesses, list):
            errors.append(f"semantic contract witnesses are invalid for {rule_id}")
        else:
            unknown = sorted(set(witnesses) - fixture_case_ids)
            if unknown:
                errors.append(
                    f"semantic contract unknown native witnesses for {rule_id}: "
                    + ", ".join(unknown)
                )
            referenced_cases.update(witnesses)
        if proof_state not in known_proof_states:
            errors.append(f"semantic contract proof state is invalid for {rule_id}")
        for field in (
            "predicate_sha256",
            "dependency_text_sha256",
            "future_phase_sha256",
        ):
            value = rule.get(field)
            if not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{64}", value) is None:
                errors.append(f"semantic contract {field} is invalid for {rule_id}")

    header_claims = {
        rule.get("id")
        for rule in rules
        if isinstance(rule, dict) and rule.get("proof_state") == "HeaderCheckedTfm"
    }
    if header_claims != HEADER_CHECKED_RULES:
        errors.append("semantic contract HeaderCheckedTfm proof ownership differs")
    character_claims = {
        rule.get("id")
        for rule in rules
        if isinstance(rule, dict) and rule.get("proof_state") == "CharacterCheckedTfm"
    }
    if character_claims != CHARACTER_CHECKED_RULES:
        errors.append("semantic contract CharacterCheckedTfm proof ownership differs")
    unmapped_cases = sorted(fixture_case_ids - referenced_cases)
    if unmapped_cases:
        errors.append("semantic contract unmapped fixture cases: " + ", ".join(unmapped_cases))
    return errors


def validate_rule_ledger(
    ledger: str,
    fixture_case_ids: set[str],
    contract: dict[str, object] | None = None,
) -> list[str]:
    contract = RULE_CONTRACT if contract is None else contract
    errors = validate_rule_contract(contract, fixture_case_ids)
    lines = ledger.splitlines()
    try:
        header_index = next(
            index for index, line in enumerate(lines) if line.startswith("| Rule id |")
        )
    except StopIteration:
        return ["source rule table header is missing"]

    rows: list[tuple[str, str, str, str, str]] = []
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
        rows.append((match.group(1), cells[1], cells[2], cells[3], cells[4]))

    rule_ids = [row[0] for row in rows]
    duplicates = sorted(
        rule_id for rule_id, count in Counter(rule_ids).items() if count > 1
    )
    if duplicates:
        errors.append(f"duplicate rule ids: {', '.join(duplicates)}")
    contract_rules = contract.get("rules", [])
    expected_rule_ids = [rule.get("id") for rule in contract_rules]
    if rule_ids != expected_rule_ids:
        errors.append(
            "source rule order differs from the pinned read_font_info order: "
            + ", ".join(rule_ids)
        )

    missing_dependencies = [
        rule_id for rule_id, _, dependency, _, _ in rows if not dependency
    ]
    if missing_dependencies:
        errors.append("missing dependencies: " + ", ".join(missing_dependencies))

    referenced_cases: set[str] = set()
    unknown_witnesses: set[str] = set()
    expected_by_id = {rule.get("id"): rule for rule in contract_rules}
    for rule_id, predicate, dependency, native_evidence, future_phase in rows:
        witnesses = re.findall(r"`([a-z][a-z0-9_]*)`", native_evidence)
        for token in witnesses:
            if token in fixture_case_ids:
                referenced_cases.add(token)
            else:
                unknown_witnesses.add(token)
        expected = expected_by_id.get(rule_id)
        if expected is None:
            continue
        mismatches = []
        for name, value, field in (
            ("predicate", predicate, "predicate_sha256"),
            ("dependency", dependency, "dependency_text_sha256"),
            ("future phase", future_phase, "future_phase_sha256"),
        ):
            if hashlib.sha256(value.encode()).hexdigest() != expected.get(field):
                mismatches.append(name)
        if witnesses != expected.get("witnesses"):
            mismatches.append("witnesses")
        if mismatches:
            errors.append(
                f"semantic contract mismatch for {rule_id}: " + ", ".join(mismatches)
            )

    if unknown_witnesses:
        errors.append("unknown native witnesses: " + ", ".join(sorted(unknown_witnesses)))

    unmapped_cases = sorted(fixture_case_ids - referenced_cases)
    if unmapped_cases:
        errors.append("unmapped fixture cases: " + ", ".join(unmapped_cases))
    return errors


def main() -> int:
    ledger = (ROOT / "docs/tex82-read-font-info-validation-rules.md").read_text(
        encoding="utf-8"
    )
    corpus_manifest = json.loads(
        (
            ROOT
            / "crates/tex-tfm-metrics/tests/fixtures/tfm-validity-oracle-v2/manifest.json"
        ).read_text(encoding="utf-8")
    )
    corpus_case_ids = {case["id"] for case in corpus_manifest["cases"]}
    errors = validate_rule_ledger(ledger, corpus_case_ids)
    if errors:
        for error in errors:
            print(error)
        return 1
    proof_counts = Counter(rule["proof_state"] for rule in RULE_CONTRACT["rules"])
    print(
        "TeX82 TFM validation source-rule ledger passed: "
        f"rules={len(RULE_CONTRACT['rules'])}, witnesses={len(corpus_case_ids)}, "
        f"proof_states={dict(sorted(proof_counts.items()))}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
