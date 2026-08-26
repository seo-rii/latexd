import json
import unittest
from pathlib import Path

from scripts.check_tfm_validation_ledger import validate_rule_ledger


ROOT = Path(__file__).parents[2]
LEDGER = ROOT / "docs/tex82-read-font-info-validation-rules.md"
FIXTURE = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validity-oracle-v1.json"
)


def ledger_text() -> str:
    return LEDGER.read_text(encoding="utf-8")


def fixture_case_ids() -> set[str]:
    fixture = json.loads(FIXTURE.read_text(encoding="utf-8"))
    return set(fixture["case_results"])


class TfmValidationLedgerTests(unittest.TestCase):
    def test_contract_records_machine_ledger_gate(self) -> None:
        contract = (ROOT / "docs/m13-3-dp1-scan-context.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("check_tfm_validation_ledger.py", contract)
        self.assertIn("82/82", contract)

    def test_ci_runs_machine_ledger_policy(self) -> None:
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertIn("scripts.tests.test_check_tfm_validation_ledger", workflow)

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

    def test_source_and_phase_order_swaps_are_rejected(self) -> None:
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
        self.assertTrue(any("phase order" in error for error in errors))

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
