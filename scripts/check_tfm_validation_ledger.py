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
RULE_TRANSITION_PATH = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validation-rule-transition-v2.json"
)
RULE_TRANSITION_V3_PATH = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validation-rule-transition-v3.json"
)
KERN_SOURCE_CONTRACT_PATH = (
    ROOT / "crates/tex-tfm-metrics/tests/fixtures/tfm-kern-source-contract-v1.json"
)
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
BOX_CHECKED_RULES = {
    "TFM-BOX-001",
    "TFM-BOX-002",
    "TFM-BOX-003",
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

PINNED_V2_FOCUSED_SOURCE = {
    "compatibility_source_sha256": PINNED_SOURCE["sha256"],
    "check_existence_section": {
        "lines": "11150..11154",
        "sha256": "50b7893997fe98c90314983b83456c0fa15f577d02e91e2a03cf2a8034765c63",
    },
    "lig_kern_instruction_section": {
        "lines": "11156..11172",
        "sha256": "a105c3b6349d6ad4c15e37f3cc0d8b64670c14ffc3f79cdd827da05043d28c5d",
    },
}
PINNED_V2_OWNERSHIP_CHANGES = [
    {
        "rule_id": "TFM-KERN-001",
        "from": "LigKernCheckedTfm",
        "to": "KernCheckedTfm",
    }
]
PINNED_V2_SOURCE_PREDICATE_PROJECTIONS = [
    {
        "source_predicate": "empty_instruction_table",
        "runtime_projection": "AcceptedEmptyInstructionTable",
        "rule_ids": ["TFM-LIGKERN-001"],
    },
    {
        "source_predicate": "restart_target_range_check",
        "runtime_projection": "RestartTargetOutOfRange",
        "rule_ids": ["TFM-LIGKERN-002", "TFM-LIGKERN-008"],
    },
    {
        "source_predicate": "first_boundary_character_marker",
        "runtime_projection": "AcceptedBoundaryCharacter",
        "rule_ids": ["TFM-LIGKERN-003"],
    },
    {
        "source_predicate": "ordinary_next_character_existence",
        "runtime_projection": "NextCharacterMissing",
        "rule_ids": ["TFM-LIGKERN-004"],
    },
    {
        "source_predicate": "ligature_replacement_existence",
        "runtime_projection": "LigatureTargetMissing",
        "rule_ids": ["TFM-LIGKERN-005"],
    },
    {
        "source_predicate": "kern_index_range_check",
        "runtime_projection": "KernIndexOutOfRange",
        "rule_ids": ["TFM-LIGKERN-006"],
    },
    {
        "source_predicate": "forward_skip_range_check",
        "runtime_projection": "ForwardSkipOutOfRange",
        "rule_ids": ["TFM-LIGKERN-007"],
    },
]
REVIEWED_V2_TRANSITION_RAW_SHA256 = (
    "4a0bb1453055d12037fbbab0c77999feaf9b24f2d71b7e8afeb38453d2788316"
)
REVIEWED_V2_TRANSITION_CANONICAL_SHA256 = (
    "773983c0d5a99c21067f79edd887db792092c4d59efb8d9c0af2c478fb5c00fc"
)
PINNED_V3_OWNERSHIP_CHANGES = [
    {
        "rule_id": "TFM-EXT-001",
        "from": "TailCheckedTfm",
        "to": "ExtensibleCheckedTfm",
    },
    {
        "rule_id": "TFM-EXT-002",
        "from": "TailCheckedTfm",
        "to": "ExtensibleCheckedTfm",
    },
]
PINNED_V3_SOURCE_PREDICATE_PROJECTIONS = [
    {
        "source_predicate": "optional_part_character_existence",
        "runtime_projection": "OptionalPartMissing",
        "rule_ids": ["TFM-EXT-001"],
    },
    {
        "source_predicate": "repeat_character_existence",
        "runtime_projection": "RepeatMissing",
        "rule_ids": ["TFM-EXT-002"],
    },
]
REVIEWED_V3_TRANSITION_RAW_SHA256 = (
    "5929817fa92f3f8ead2a05ba33476281bb16ab5661eef5926730fe6fa27ce09d"
)
REVIEWED_V3_TRANSITION_CANONICAL_SHA256 = (
    "3206379d5f6f6748c2d532da83df565a187aee2077e936a67672336d10569ccf"
)
PINNED_KERN_FOCUSED_SOURCE = {
    "compatibility_source_sha256": PINNED_SOURCE["sha256"],
    "fix_word_scaling_section": {
        "lines": "11108..11130",
        "sha256": "306907b8734bfa4dc990546e1fb84d0158c2b9af2338faed18808a06c4bfa58e",
    },
    "scale_normalization_section": {
        "lines": "11142..11148",
        "sha256": "e4db0f873ddda4dc750831a8ddcb436bb44dae7cb41044314837a1895a9c1906",
    },
    "kern_loop_section": {
        "lines": "11173..11174",
        "sha256": "d1b13b62579f82c3fec9ea7fbf275c751ea1e7eb31a02c2d703233c7c84760f1",
    },
}
PINNED_KERN_PROOF_BOUNDARY = {
    "input": "LigKernCheckedTfm",
    "output": "KernCheckedTfm",
    "owned_rule_ids": ["TFM-KERN-001"],
    "loop_cardinality": "nk",
    "reads": ["effective_size", "kerns"],
    "excluded_reads": ["extensibles", "parameters", "raw_suffix"],
    "entry_zero_check": False,
}


def validate_kern_source_contract(
    contract: dict[str, object], predecessor: dict[str, object]
) -> list[str]:
    errors: list[str] = []
    expected_keys = {
        "format",
        "schema_version",
        "predecessor",
        "focused_source",
        "proof_boundary",
    }
    if set(contract) != expected_keys:
        errors.append("kern source contract fields differ")
    if contract.get("format") != "latexd.tfm-kern-source-contract":
        errors.append("kern source contract format is invalid")
    if contract.get("schema_version") != 1:
        errors.append("kern source contract schema version is invalid")

    raw_predecessor = RULE_TRANSITION_PATH.read_bytes()
    canonical_predecessor = json.dumps(
        predecessor,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode()
    expected_predecessor = {
        "path": "tfm-validation-rule-transition-v2.json",
        "schema_version": 2,
        "raw_sha256": REVIEWED_V2_TRANSITION_RAW_SHA256,
        "canonical_sha256": REVIEWED_V2_TRANSITION_CANONICAL_SHA256,
    }
    if contract.get("predecessor") != expected_predecessor:
        errors.append("kern source contract predecessor pin differs")
    if hashlib.sha256(raw_predecessor).hexdigest() != (
        REVIEWED_V2_TRANSITION_RAW_SHA256
    ):
        errors.append("kern source contract predecessor raw content differs")
    if hashlib.sha256(canonical_predecessor).hexdigest() != (
        REVIEWED_V2_TRANSITION_CANONICAL_SHA256
    ):
        errors.append("kern source contract predecessor canonical content differs")
    if contract.get("focused_source") != PINNED_KERN_FOCUSED_SOURCE:
        errors.append("kern source contract focused source pins differ")
    if contract.get("proof_boundary") != PINNED_KERN_PROOF_BOUNDARY:
        errors.append("kern source contract proof boundary differs")
    return errors


def validate_rule_transition(
    transition: dict[str, object], predecessor: dict[str, object]
) -> list[str]:
    errors: list[str] = []
    expected_keys = {
        "format",
        "schema_version",
        "predecessor",
        "focused_source",
        "proof_states_added",
        "ownership_changes",
        "source_predicate_projections",
    }
    if set(transition) != expected_keys:
        errors.append("v2 transition fields differ")
    if transition.get("format") != "latexd.tfm-validation-rule-contract-transition":
        errors.append("v2 transition format is invalid")
    if transition.get("schema_version") != 2:
        errors.append("v2 transition schema version is invalid")

    canonical_predecessor = json.dumps(
        predecessor,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode()
    expected_predecessor = {
        "path": "tfm-validation-rules-v1.json",
        "schema_version": 1,
        "canonical_sha256": REVIEWED_V1_CONTRACT_CANONICAL_SHA256,
    }
    if transition.get("predecessor") != expected_predecessor:
        errors.append("v2 transition predecessor pin differs")
    if hashlib.sha256(canonical_predecessor).hexdigest() != (
        REVIEWED_V1_CONTRACT_CANONICAL_SHA256
    ):
        errors.append("v2 transition predecessor content differs")
    if transition.get("focused_source") != PINNED_V2_FOCUSED_SOURCE:
        errors.append("v2 transition focused source pins differ")
    if transition.get("proof_states_added") != ["KernCheckedTfm"]:
        errors.append("v2 transition added proof states differ")
    if transition.get("ownership_changes") != PINNED_V2_OWNERSHIP_CHANGES:
        errors.append("v2 transition proof ownership changes differ")
    projections = transition.get("source_predicate_projections")
    if projections != PINNED_V2_SOURCE_PREDICATE_PROJECTIONS:
        errors.append("v2 transition source predicate projections differ")
    if isinstance(projections, list):
        projected_rule_ids = [
            rule_id
            for projection in projections
            if isinstance(projection, dict)
            for rule_id in projection.get("rule_ids", [])
        ]
        expected_lig_kern_rule_ids = [
            rule_id
            for rule_id in REVIEWED_V1_RULE_IDS
            if rule_id.startswith("TFM-LIGKERN-")
        ]
        if Counter(projected_rule_ids) != Counter(expected_lig_kern_rule_ids):
            errors.append("v2 transition lig/kern projection coverage differs")
        if "TFM-KERN-001" in projected_rule_ids:
            errors.append("v2 transition lig/kern projection includes kern scaling")
    else:
        errors.append("v2 transition source predicate projections are invalid")

    rules = predecessor.get("rules")
    proof_states = predecessor.get("proof_states")
    if isinstance(rules, list) and isinstance(proof_states, list):
        kern_rules = [
            rule
            for rule in rules
            if isinstance(rule, dict) and rule.get("id") == "TFM-KERN-001"
        ]
        if len(kern_rules) != 1 or kern_rules[0].get("proof_state") != (
            "LigKernCheckedTfm"
        ):
            errors.append("v2 transition source ownership does not match predecessor")
        if "KernCheckedTfm" in proof_states:
            errors.append("v2 transition target proof state already exists")
    else:
        errors.append("v2 transition predecessor collections are invalid")
    return errors


def validate_rule_transition_v3(
    transition: dict[str, object], predecessor: dict[str, object]
) -> list[str]:
    errors: list[str] = []
    expected_keys = {
        "format",
        "schema_version",
        "predecessor",
        "proof_states_added",
        "ownership_changes",
        "source_predicate_projections",
    }
    if set(transition) != expected_keys:
        errors.append("v3 transition fields differ")
    if transition.get("format") != "latexd.tfm-validation-rule-contract-transition":
        errors.append("v3 transition format is invalid")
    if transition.get("schema_version") != 3:
        errors.append("v3 transition schema version is invalid")

    raw_predecessor = RULE_TRANSITION_PATH.read_bytes()
    canonical_predecessor = json.dumps(
        predecessor,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode()
    expected_predecessor = {
        "path": "tfm-validation-rule-transition-v2.json",
        "schema_version": 2,
        "raw_sha256": REVIEWED_V2_TRANSITION_RAW_SHA256,
        "canonical_sha256": REVIEWED_V2_TRANSITION_CANONICAL_SHA256,
    }
    if transition.get("predecessor") != expected_predecessor:
        errors.append("v3 transition predecessor pin differs")
    if hashlib.sha256(raw_predecessor).hexdigest() != (
        REVIEWED_V2_TRANSITION_RAW_SHA256
    ):
        errors.append("v3 transition predecessor raw content differs")
    if hashlib.sha256(canonical_predecessor).hexdigest() != (
        REVIEWED_V2_TRANSITION_CANONICAL_SHA256
    ):
        errors.append("v3 transition predecessor canonical content differs")
    if transition.get("proof_states_added") != ["ExtensibleCheckedTfm"]:
        errors.append("v3 transition added proof states differ")
    if transition.get("ownership_changes") != PINNED_V3_OWNERSHIP_CHANGES:
        errors.append("v3 transition proof ownership changes differ")
    projections = transition.get("source_predicate_projections")
    if projections != PINNED_V3_SOURCE_PREDICATE_PROJECTIONS:
        errors.append("v3 transition source predicate projections differ")
    if isinstance(projections, list):
        projected_rule_ids = [
            rule_id
            for projection in projections
            if isinstance(projection, dict)
            for rule_id in projection.get("rule_ids", [])
        ]
        if any(not isinstance(rule_id, str) for rule_id in projected_rule_ids):
            errors.append("v3 transition projected rule ids are invalid")
        else:
            if Counter(projected_rule_ids) != Counter(
                ["TFM-EXT-001", "TFM-EXT-002"]
            ):
                errors.append("v3 transition extensible projection coverage differs")
            if any(
                rule_id.startswith("TFM-PARAM-") for rule_id in projected_rule_ids
            ):
                errors.append("v3 transition extensible projection includes parameters")
    else:
        errors.append("v3 transition source predicate projections are invalid")

    canonical_transition = json.dumps(
        transition,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode()
    if hashlib.sha256(RULE_TRANSITION_V3_PATH.read_bytes()).hexdigest() != (
        REVIEWED_V3_TRANSITION_RAW_SHA256
    ):
        errors.append("reviewed v3 transition raw content differs")
    if hashlib.sha256(canonical_transition).hexdigest() != (
        REVIEWED_V3_TRANSITION_CANONICAL_SHA256
    ):
        errors.append("reviewed v3 transition canonical content differs")
    return errors


def validate_transition_chain(
    transitions: list[dict[str, object]], predecessor: dict[str, object]
) -> list[str]:
    errors: list[str] = []
    schema_versions = [transition.get("schema_version") for transition in transitions]
    if schema_versions != [2, 3]:
        errors.append("transition chain schema order differs; expected v2->v3")
    if transitions:
        errors.extend(validate_rule_transition(transitions[0], predecessor))
    if len(transitions) >= 2:
        errors.extend(validate_rule_transition_v3(transitions[1], transitions[0]))

    proof_states = predecessor.get("proof_states")
    rules = predecessor.get("rules")
    if not isinstance(proof_states, list) or not isinstance(rules, list):
        return errors + ["transition chain predecessor collections are invalid"]
    known_states = set(proof_states)
    current_owners = {
        rule.get("id"): rule.get("proof_state")
        for rule in rules
        if isinstance(rule, dict)
    }
    moved_rule_ids: set[str] = set()
    for transition in transitions:
        schema_version = transition.get("schema_version")
        added_states = transition.get("proof_states_added")
        if not isinstance(added_states, list):
            errors.append(f"v{schema_version} transition added proof states are invalid")
            continue
        for state in added_states:
            if not isinstance(state, str):
                errors.append(f"v{schema_version} transition added proof state is invalid")
            elif state in known_states:
                errors.append(
                    f"v{schema_version} transition proof state already exists: {state}"
                )
            else:
                known_states.add(state)

        ownership_changes = transition.get("ownership_changes")
        if not isinstance(ownership_changes, list):
            errors.append(f"v{schema_version} transition ownership changes are invalid")
            continue
        seen_in_transition: set[str] = set()
        for change in ownership_changes:
            if not isinstance(change, dict):
                errors.append(f"v{schema_version} transition ownership move is invalid")
                continue
            rule_id = change.get("rule_id")
            source = change.get("from")
            target = change.get("to")
            if not all(isinstance(value, str) for value in (rule_id, source, target)):
                errors.append(
                    f"v{schema_version} transition ownership move fields are invalid"
                )
                continue
            if rule_id in seen_in_transition:
                errors.append(
                    f"v{schema_version} transition duplicate ownership move: {rule_id}"
                )
                continue
            seen_in_transition.add(rule_id)
            if rule_id in moved_rule_ids:
                errors.append(
                    f"v{schema_version} transition rule already moved by an earlier "
                    f"transition: {rule_id}"
                )
                continue
            if rule_id not in current_owners:
                errors.append(f"v{schema_version} transition rule is unknown: {rule_id}")
                continue
            current_owner = current_owners[rule_id]
            if source != current_owner:
                errors.append(
                    f"v{schema_version} transition current effective owner differs for "
                    f"{rule_id}: expected {current_owner}, got {source}"
                )
            if target not in known_states:
                errors.append(
                    f"v{schema_version} transition target proof state is unknown for "
                    f"{rule_id}: {target}"
                )
            if source == current_owner and target in known_states:
                current_owners[rule_id] = target
            moved_rule_ids.add(rule_id)
    return errors


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
    box_claims = {
        rule.get("id")
        for rule in rules
        if isinstance(rule, dict) and rule.get("proof_state") == "BoxCheckedTfm"
    }
    if box_claims != BOX_CHECKED_RULES:
        errors.append("semantic contract BoxCheckedTfm proof ownership differs")
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
    transition = json.loads(RULE_TRANSITION_PATH.read_text(encoding="utf-8"))
    transition_v3 = json.loads(
        RULE_TRANSITION_V3_PATH.read_text(encoding="utf-8")
    )
    transitions = [transition, transition_v3]
    errors.extend(validate_transition_chain(transitions, RULE_CONTRACT))
    kern_source_contract = json.loads(
        KERN_SOURCE_CONTRACT_PATH.read_text(encoding="utf-8")
    )
    errors.extend(validate_kern_source_contract(kern_source_contract, transition))
    if errors:
        for error in errors:
            print(error)
        return 1
    proof_counts = Counter(rule["proof_state"] for rule in RULE_CONTRACT["rules"])
    for current_transition in transitions:
        for change in current_transition["ownership_changes"]:
            proof_counts[change["from"]] -= 1
            proof_counts[change["to"]] += 1
    print(
        "TeX82 TFM validation source-rule ledger passed: "
        f"rules={len(RULE_CONTRACT['rules'])}, witnesses={len(corpus_case_ids)}, "
        f"proof_states={dict(sorted(proof_counts.items()))}, "
        "transition_chain=v2->v3, "
        "kern_source_contract=v1"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
