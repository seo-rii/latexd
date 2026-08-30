import json
import io
import unittest
from contextlib import redirect_stdout
from pathlib import Path

from scripts.check_tfm_validation_ledger import (
    main,
    validate_kern_source_contract,
    validate_rule_contract,
    validate_rule_ledger,
    validate_rule_transition,
)


ROOT = Path(__file__).parents[2]
LEDGER = ROOT / "docs/tex82-read-font-info-validation-rules.md"
CORPUS_MANIFEST = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validity-oracle-v2/manifest.json"
)
RULE_CONTRACT = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validation-rules-v1.json"
)
RULE_TRANSITION = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validation-rule-transition-v2.json"
)
KERN_SOURCE_CONTRACT = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-kern-source-contract-v1.json"
)
LIG_KERN_SOURCE_CONTRACT = ROOT / "docs/tex82-read-font-info-lig-kern.md"


def ledger_text() -> str:
    return LEDGER.read_text(encoding="utf-8")


def fixture_case_ids() -> set[str]:
    manifest = json.loads(CORPUS_MANIFEST.read_text(encoding="utf-8"))
    return {case["id"] for case in manifest["cases"]}


def rule_contract() -> dict[str, object]:
    return json.loads(RULE_CONTRACT.read_text(encoding="utf-8"))


def rule_transition() -> dict[str, object]:
    return json.loads(RULE_TRANSITION.read_text(encoding="utf-8"))


def kern_source_contract() -> dict[str, object]:
    return json.loads(KERN_SOURCE_CONTRACT.read_text(encoding="utf-8"))


class TfmValidationLedgerTests(unittest.TestCase):
    def test_kern_source_contract_pins_exact_successor_boundary(self) -> None:
        contract = kern_source_contract()
        self.assertEqual(
            validate_kern_source_contract(contract, rule_transition()), []
        )
        self.assertEqual(contract["schema_version"], 1)
        self.assertEqual(
            contract["predecessor"],
            {
                "path": "tfm-validation-rule-transition-v2.json",
                "schema_version": 2,
                "raw_sha256": "4a0bb1453055d12037fbbab0c77999feaf9b24f2d71b7e8afeb38453d2788316",
                "canonical_sha256": "773983c0d5a99c21067f79edd887db792092c4d59efb8d9c0af2c478fb5c00fc",
            },
        )
        self.assertEqual(
            contract["focused_source"],
            {
                "compatibility_source_sha256": "c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324",
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
            },
        )
        self.assertEqual(
            contract["proof_boundary"],
            {
                "input": "LigKernCheckedTfm",
                "output": "KernCheckedTfm",
                "owned_rule_ids": ["TFM-KERN-001"],
                "loop_cardinality": "nk",
                "reads": ["effective_size", "kerns"],
                "excluded_reads": ["extensibles", "parameters", "raw_suffix"],
                "entry_zero_check": False,
            },
        )

    def test_kern_source_contract_rejects_predecessor_source_and_scope_drift(
        self,
    ) -> None:
        for field, replacement in (
            ("predecessor", {}),
            ("focused_source", {}),
            ("proof_boundary", {}),
        ):
            changed = kern_source_contract()
            changed[field] = replacement
            self.assertTrue(
                validate_kern_source_contract(changed, rule_transition()),
                field,
            )

    def test_ligkern_replacement_review_and_kern_contract_are_documented(
        self,
    ) -> None:
        for relative_path in (
            "PLAN.md",
            "docs/m13-3-dp1-scan-context.md",
            "docs/tex82-read-font-info-validation-rules.md",
            "docs/tex82-read-font-info-lig-kern.md",
        ):
            document = (ROOT / relative_path).read_text(encoding="utf-8")
            for required in (
                "6a93bc49-6f74-83ee-b517-7f02fcebb9f9",
                "PROCEED_PRIVATE_TFM_KERN",
                "confidence 0.93",
                "tfm-kern-source-contract-v1.json",
                "lines 11108..11130",
                "306907b8734bfa4dc990546e1fb84d0158c2b9af2338faed18808a06c4bfa58e",
                "lines 11142..11148",
                "e4db0f873ddda4dc750831a8ddcb436bb44dae7cb41044314837a1895a9c1906",
                "lines 11173..11174",
                "d1b13b62579f82c3fec9ea7fbf275c751ea1e7eb31a02c2d703233c7c84760f1",
                "whole `nk` table",
                "no entry-zero check",
            ):
                self.assertIn(required, document, relative_path)

    def test_standalone_gate_reports_transitioned_proof_ownership(self) -> None:
        output = io.StringIO()
        with redirect_stdout(output):
            self.assertEqual(main(), 0)
        self.assertIn("'LigKernCheckedTfm': 8", output.getvalue())
        self.assertIn("'KernCheckedTfm': 1", output.getvalue())

    def test_v2_transition_pins_ligkern_source_and_splits_only_kern_owner(
        self,
    ) -> None:
        transition = rule_transition()
        self.assertEqual(validate_rule_transition(transition, rule_contract()), [])
        self.assertEqual(transition["schema_version"], 2)
        self.assertEqual(
            transition["focused_source"]["check_existence_section"],
            {
                "lines": "11150..11154",
                "sha256": "50b7893997fe98c90314983b83456c0fa15f577d02e91e2a03cf2a8034765c63",
            },
        )
        self.assertEqual(
            transition["focused_source"]["lig_kern_instruction_section"],
            {
                "lines": "11156..11172",
                "sha256": "a105c3b6349d6ad4c15e37f3cc0d8b64670c14ffc3f79cdd827da05043d28c5d",
            },
        )
        self.assertEqual(transition["proof_states_added"], ["KernCheckedTfm"])
        self.assertEqual(
            transition["ownership_changes"],
            [
                {
                    "rule_id": "TFM-KERN-001",
                    "from": "LigKernCheckedTfm",
                    "to": "KernCheckedTfm",
                }
            ],
        )
        self.assertEqual(
            transition["source_predicate_projections"],
            [
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
            ],
        )
        document = LIG_KERN_SOURCE_CONTRACT.read_text(encoding="utf-8")
        for required in (
            "lines 11150..11154",
            "50b7893997fe98c90314983b83456c0fa15f577d02e91e2a03cf2a8034765c63",
            "lines 11156..11172",
            "a105c3b6349d6ad4c15e37f3cc0d8b64670c14ffc3f79cdd827da05043d28c5d",
            "`LigKernCheckedTfm`",
            "`KernCheckedTfm`",
            "must not scale kern fix words",
        ):
            self.assertIn(required, document)
        for relative_path in (
            "PLAN.md",
            "docs/m13-3-dp1-scan-context.md",
            "docs/tex82-read-font-info-validation-rules.md",
        ):
            summary = (ROOT / relative_path).read_text(encoding="utf-8")
            self.assertIn("tfm-validation-rule-transition-v2.json", summary)
            self.assertIn(
                "a105c3b6349d6ad4c15e37f3cc0d8b64670c14ffc3f79cdd827da05043d28c5d",
                summary,
            )
            self.assertIn("docs/tex82-read-font-info-lig-kern.md", summary)

    def test_v2_transition_rejects_source_or_ownership_drift(self) -> None:
        for field, replacement in (
            ("focused_source", {}),
            ("proof_states_added", []),
            ("ownership_changes", []),
            ("source_predicate_projections", []),
        ):
            changed = rule_transition()
            changed[field] = replacement
            self.assertTrue(
                validate_rule_transition(changed, rule_contract()),
                field,
            )

    def test_private_lig_kern_implementation_evidence_is_documented(self) -> None:
        for relative_path in (
            "PLAN.md",
            "docs/m13-3-dp1-scan-context.md",
            "docs/tex82-read-font-info-validation-rules.md",
            "docs/tex82-read-font-info-lig-kern.md",
        ):
            document = (ROOT / relative_path).read_text(encoding="utf-8")
            for required in (
                "private `LigKernCheckedTfm` implementation",
                "83/83 persisted corpus phase outcomes",
                "8/8 exact lig/kern-owned rejections",
                "4,096 generated programs",
                "32,755-instruction absolute maximum",
                "kern words remain unread and unscaled",
                "exactly one production construction",
                "6a93b53b-e6b0-83ee-92f5-686badb00774",
                "REVISE_PRIVATE_TFM_LIGKERN",
                "unsafe `ptr::read`",
                "source_predicate_projections",
                "count-1/count",
            ):
                self.assertIn(required, document, relative_path)

    def test_header_closure_review_authorizes_only_private_character_phase(
        self,
    ) -> None:
        for path in (
            ROOT / "PLAN.md",
            ROOT / "docs/tex82-read-font-info-validation-rules.md",
        ):
            document = path.read_text(encoding="utf-8")
            self.assertIn("6a8e45fc-4bd0-83ee-b4f8-e2c948311ae1", document)
            self.assertIn("PROCEED_PRIVATE_TFM_CHARACTER", document)

    def test_private_character_implementation_evidence_is_documented(self) -> None:
        for path in (
            ROOT / "PLAN.md",
            ROOT / "docs/m13-3-dp1-scan-context.md",
            ROOT / "docs/tex82-read-font-info-validation-rules.md",
        ):
            document = path.read_text(encoding="utf-8")
            self.assertIn("private `CharacterCheckedTfm` implementation", document)
            self.assertIn("10/10 exact character-owned rejections", document)
            self.assertIn("domains `1..=5`", document)
            self.assertIn("lig/kern remains blocked", document)

    def test_character_closure_review_authorizes_only_private_box_phase(
        self,
    ) -> None:
        for path in (
            ROOT / "PLAN.md",
            ROOT / "docs/m13-3-dp1-scan-context.md",
            ROOT / "docs/tex82-read-font-info-validation-rules.md",
        ):
            document = path.read_text(encoding="utf-8")
            self.assertIn("6a939670-0fc8-83e8-923f-ebaed26b4c72", document)
            self.assertIn("PROCEED_PRIVATE_TFM_BOX", document)
            self.assertIn("private `BoxCheckedTfm` implementation", document)
            self.assertIn("lig/kern remains blocked", document)

    def test_character_closure_hardening_evidence_is_documented(self) -> None:
        for path in (
            ROOT / "PLAN.md",
            ROOT / "docs/m13-3-dp1-scan-context.md",
            ROOT / "docs/tex82-read-font-info-validation-rules.md",
        ):
            document = path.read_text(encoding="utf-8")
            self.assertIn("four adjacent metric precedence pairs", document)
            self.assertIn("`CharListTraversalLimit` remains unreachable", document)
            self.assertIn("AST negative mutants", document)
            self.assertIn(
                "db680c23a099b5b39c484d34c357116fc8d6967a9151db4108af0ddf4cfbb0be",
                document,
            )
            self.assertIn(
                "9df44bf4b157acfb65fa0d5cc7de4d42ba7f869bae460e07daf984e1fbca19b4",
                document,
            )
            self.assertIn("one required CI job", document)

    def test_contract_records_machine_ledger_gate(self) -> None:
        contract = (ROOT / "docs/m13-3-dp1-scan-context.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("check_tfm_validation_ledger.py", contract)
        self.assertIn("tfm-validation-rules-v1.json", contract)
        self.assertIn("33/33 semantic rule cells", contract)
        self.assertIn("83/83", contract)
        self.assertIn("source ordinal", contract)
        self.assertIn("proof ownership", contract)
        self.assertIn("6a8e358c-697c-83e8-a6ba-881a469553d7", contract)
        self.assertIn("REVISE_PRIVATE_TFM_HEADER", contract)

    def test_ci_runs_machine_ledger_policy(self) -> None:
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertIn("scripts.tests.test_check_tfm_validation_ledger", workflow)
        ledger_gate = "python3 scripts/check_tfm_validation_ledger.py"
        native_oracle = "python3 scripts/check_tfm_validity_oracle.py"
        rust_suite = "run: cargo test -q"
        self.assertIn(ledger_gate, workflow)
        self.assertLess(workflow.index(ledger_gate), workflow.index(native_oracle))
        self.assertLess(workflow.index(native_oracle), workflow.index(rust_suite))

    def test_repository_ledger_has_exact_order_unique_rules_and_complete_witnesses(
        self,
    ) -> None:
        self.assertEqual(validate_rule_ledger(ledger_text(), fixture_case_ids()), [])

    def test_duplicate_rule_id_is_rejected(self) -> None:
        lines = ledger_text().splitlines()
        row_index = next(
            index for index, line in enumerate(lines) if line.startswith("| `TFM-SIZE-001`")
        )
        lines.insert(row_index + 1, lines[row_index])

        errors = validate_rule_ledger("\n".join(lines), fixture_case_ids())
        self.assertTrue(any("duplicate rule ids" in error for error in errors))

    def test_source_order_swaps_are_rejected(self) -> None:
        lines = ledger_text().splitlines()
        count_index = next(
            index for index, line in enumerate(lines) if line.startswith("| `TFM-COUNT-001`")
        )
        header_index = next(
            index for index, line in enumerate(lines) if line.startswith("| `TFM-HEADER-001`")
        )
        lines[count_index], lines[header_index] = lines[header_index], lines[count_index]

        errors = validate_rule_ledger("\n".join(lines), fixture_case_ids())
        self.assertTrue(any("source rule order" in error for error in errors))

    def test_semantic_rule_cells_cannot_be_reassigned_between_ids(self) -> None:
        lines = ledger_text().splitlines()
        header_index = next(
            index for index, line in enumerate(lines) if line.startswith("| `TFM-HEADER-001`")
        )
        charlist_index = next(
            index
            for index, line in enumerate(lines)
            if line.startswith("| `TFM-CHARLIST-001`")
        )
        header_cells = [
            cell.strip() for cell in lines[header_index].strip().strip("|").split("|")
        ]
        charlist_cells = [
            cell.strip()
            for cell in lines[charlist_index].strip().strip("|").split("|")
        ]
        for cell_index in (1, 2, 3):
            header_cells[cell_index], charlist_cells[cell_index] = (
                charlist_cells[cell_index],
                header_cells[cell_index],
            )
        lines[header_index] = "| " + " | ".join(header_cells) + " |"
        lines[charlist_index] = "| " + " | ".join(charlist_cells) + " |"

        errors = validate_rule_ledger("\n".join(lines), fixture_case_ids())
        self.assertTrue(any("semantic contract" in error for error in errors))

    def test_nonempty_dependency_cells_cannot_be_swapped(self) -> None:
        lines = ledger_text().splitlines()
        first_index = next(
            index for index, line in enumerate(lines) if line.startswith("| `TFM-HEADER-001`")
        )
        second_index = next(
            index
            for index, line in enumerate(lines)
            if line.startswith("| `TFM-CHARLIST-001`")
        )
        first = [cell.strip() for cell in lines[first_index].strip().strip("|").split("|")]
        second = [cell.strip() for cell in lines[second_index].strip().strip("|").split("|")]
        first[2], second[2] = second[2], first[2]
        lines[first_index] = "| " + " | ".join(first) + " |"
        lines[second_index] = "| " + " | ".join(second) + " |"

        errors = validate_rule_ledger("\n".join(lines), fixture_case_ids())
        self.assertTrue(any("semantic contract" in error for error in errors))

    def test_complete_witness_cells_cannot_be_swapped(self) -> None:
        lines = ledger_text().splitlines()
        first_index = next(
            index for index, line in enumerate(lines) if line.startswith("| `TFM-HEADER-001`")
        )
        second_index = next(
            index
            for index, line in enumerate(lines)
            if line.startswith("| `TFM-CHARLIST-001`")
        )
        first = [cell.strip() for cell in lines[first_index].strip().strip("|").split("|")]
        second = [cell.strip() for cell in lines[second_index].strip().strip("|").split("|")]
        first[3], second[3] = second[3], first[3]
        lines[first_index] = "| " + " | ".join(first) + " |"
        lines[second_index] = "| " + " | ".join(second) + " |"

        errors = validate_rule_ledger("\n".join(lines), fixture_case_ids())
        self.assertTrue(any("semantic contract" in error for error in errors))

    def test_contract_pins_source_hashes_and_all_rule_semantics(self) -> None:
        contract = rule_contract()
        self.assertEqual(contract["schema_version"], 1)
        self.assertEqual(
            contract["compatibility_source"]["sha256"],
            "c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324",
        )
        self.assertEqual(
            contract["compatibility_source"]["loader_section_sha256"],
            "57f665ae4cc87c721d444fdde0a1817f194f44bab18388c42a1d26d830c6ddc8",
        )
        self.assertEqual(len(contract["rules"]), 33)
        for rule in contract["rules"]:
            self.assertIn("predicate_sha256", rule)
            self.assertIn("dependency_ids", rule)
            self.assertIn("witnesses", rule)
            self.assertIn("proof_state", rule)
        rust_bridge = (
            ROOT / "crates/tex-tfm-metrics/src/tfm_validation.rs"
        ).read_text(encoding="utf-8")
        self.assertIn(
            "cebc062f771f27c5c46e0e83a74ab7c7c9f6e3a172b2cf1fe01bce0a7f6f6c21",
            rust_bridge,
        )
        self.assertIn("canonical v1 rule contract", rust_bridge)

    def test_eof_rules_are_source_late_but_header_proven(self) -> None:
        rules = {rule["id"]: rule for rule in rule_contract()["rules"]}
        self.assertGreater(
            rules["TFM-EOF-001"]["source_ordinal"],
            rules["TFM-PARAM-003"]["source_ordinal"],
        )
        self.assertEqual(rules["TFM-EOF-001"]["proof_state"], "HeaderCheckedTfm")
        self.assertEqual(rules["TFM-EOF-002"]["proof_state"], "HeaderCheckedTfm")

    def test_header_proof_ownership_cannot_be_moved_to_a_late_phase(self) -> None:
        changed = json.loads(json.dumps(rule_contract()))
        rules = {rule["id"]: rule for rule in changed["rules"]}
        rules["TFM-EOF-001"]["proof_state"] = "TailCheckedTfm"
        rules["TFM-PARAM-003"]["proof_state"] = "HeaderCheckedTfm"

        errors = validate_rule_contract(changed, fixture_case_ids())
        self.assertTrue(any("proof ownership" in error for error in errors))

    def test_character_proof_ownership_cannot_be_moved_to_a_late_phase(self) -> None:
        changed = json.loads(json.dumps(rule_contract()))
        rules = {rule["id"]: rule for rule in changed["rules"]}
        rules["TFM-CHAR-001"]["proof_state"] = "TailCheckedTfm"

        errors = validate_rule_contract(changed, fixture_case_ids())
        self.assertTrue(any("CharacterCheckedTfm proof ownership" in error for error in errors))

    def test_box_proof_ownership_cannot_be_moved_to_a_late_phase(self) -> None:
        changed = json.loads(json.dumps(rule_contract()))
        rules = {rule["id"]: rule for rule in changed["rules"]}
        rules["TFM-BOX-001"]["proof_state"] = "TailCheckedTfm"

        errors = validate_rule_contract(changed, fixture_case_ids())
        self.assertTrue(any("BoxCheckedTfm proof ownership" in error for error in errors))

    def test_reviewed_v1_contract_semantics_require_a_version_transition(self) -> None:
        changed = json.loads(json.dumps(rule_contract()))
        rules = {rule["id"]: rule for rule in changed["rules"]}
        rules["TFM-PARAM-001"]["predicate_sha256"] = "0" * 64

        errors = validate_rule_contract(changed, fixture_case_ids())
        self.assertTrue(any("reviewed v1 contract digest" in error for error in errors))

    def test_unknown_native_witness_is_rejected(self) -> None:
        changed = ledger_text().replace(
            "`valid_cmr10`", "`unknown_native_case`", 1
        )

        errors = validate_rule_ledger(changed, fixture_case_ids())
        self.assertTrue(any("unknown native witnesses" in error for error in errors))

    def test_missing_dependency_is_rejected(self) -> None:
        changed = ledger_text().replace(
            "| Effective size; upstream scanner state. |",
            "| |",
            1,
        )

        errors = validate_rule_ledger(changed, fixture_case_ids())
        self.assertTrue(any("missing dependencies" in error for error in errors))

    def test_unmapped_fixture_case_is_rejected(self) -> None:
        changed = ledger_text().replace("`valid_cmr10`", "valid control", 1)

        errors = validate_rule_ledger(changed, fixture_case_ids())
        self.assertTrue(any("unmapped fixture cases" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
