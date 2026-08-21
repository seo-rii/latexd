import hashlib
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from scripts.check_tfm_validity_oracle import (
    CASE_SPECS,
    EXPECTED_FIXTURE,
    main,
    mutate_tfm,
    run_oracle,
    validate_case_results,
)


EXPECTED_CASES = {
    "valid_cmr10",
    "short_np5",
    "short_np4",
    "short_np0",
    "trailing_word",
    "zero_width_table_consistent",
    "invalid_character_width_index",
    "charlist_self_cycle",
    "invalid_width_fix_word_sign",
    "nonzero_width_zero",
    "invalid_fontdimen2",
    "invalid_fontdimen5",
    "invalid_ligkern",
    "invalid_kern_fix_word",
    "invalid_extensible",
}


class TfmValidityOracleTests(unittest.TestCase):
    def test_matrix_and_fixture_freeze_full_tfm_validity_boundaries(self) -> None:
        fixture = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))

        self.assertEqual(set(CASE_SPECS), EXPECTED_CASES)
        self.assertEqual(set(fixture["case_results"]), EXPECTED_CASES)
        self.assertEqual(fixture["format"], "latexd.tfm-validity-oracle")
        self.assertEqual(fixture["schema_version"], 1)
        self.assertEqual(fixture["compatibility_target"], "TeX82 via pdfTeX INITEX")
        self.assertEqual(validate_case_results(fixture["case_results"], fixture), [])

        for case_id, expected in fixture["case_results"].items():
            self.assertEqual(len(expected["mutated_tfm_sha256"]), 64, case_id)
            self.assertEqual(len(expected["source_sha256"]), 64, case_id)
            self.assertEqual(expected["observations"]["sentinel"], 1, case_id)

    def test_mutations_are_byte_deterministic_and_leave_assets_unchanged(
        self,
    ) -> None:
        repository = Path(__file__).parents[2]
        for case_id, spec in CASE_SPECS.items():
            source_path = repository / spec["base_tfm"]
            source = source_path.read_bytes()
            mutated = mutate_tfm(case_id, source)
            expected = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))[
                "case_results"
            ][case_id]

            self.assertEqual(
                hashlib.sha256(mutated).hexdigest(),
                expected["mutated_tfm_sha256"],
                case_id,
            )
            self.assertEqual(source_path.read_bytes(), source, case_id)

    def test_validation_rejects_changed_missing_and_unexpected_cases(self) -> None:
        fixture = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))
        expected = fixture["case_results"]

        changed = json.loads(json.dumps(expected))
        changed["valid_cmr10"]["observations"]["quad"] += 1
        self.assertTrue(validate_case_results(changed, fixture))

        missing = json.loads(json.dumps(expected))
        del missing["invalid_ligkern"]
        self.assertTrue(validate_case_results(missing, fixture))

        unexpected = json.loads(json.dumps(expected))
        unexpected["future_case"] = unexpected["valid_cmr10"]
        self.assertTrue(validate_case_results(unexpected, fixture))

    def test_ci_runs_policy_and_native_oracle_after_tex_install(self) -> None:
        workflow = (Path(__file__).parents[2] / ".github/workflows/ci.yml").read_text(
            encoding="utf-8"
        )

        policy = "scripts.tests.test_check_tfm_validity_oracle"
        oracle = "python3 scripts/check_tfm_validity_oracle.py"
        artifact = "tfm-validity-oracle"
        tex_install = "Install Computer Modern test fonts"
        self.assertIn(policy, workflow)
        self.assertIn(oracle, workflow)
        self.assertIn(artifact, workflow)
        self.assertLess(workflow.index(tex_install), workflow.index(oracle))

    def test_contract_records_subset_gap_and_keeps_w3_blocked(self) -> None:
        contract = (
            Path(__file__).parents[2] / "docs/m13-3-dp1-scan-context.md"
        ).read_text(encoding="utf-8")

        self.assertIn("full TFM validity", contract)
        self.assertIn("dimension-subset", contract)
        self.assertIn("W3 remains blocked", contract)

    @unittest.skipUnless(shutil.which("pdftex"), "pdftex is required for the oracle")
    def test_pdftex_initex_matches_characterization(self) -> None:
        fixture = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(run_oracle("pdftex"), fixture["case_results"])

    @unittest.skipUnless(shutil.which("pdftex"), "pdftex is required for the oracle")
    def test_cli_records_reproducible_engine_input_and_environment_evidence(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temp:
            report_path = Path(temp) / "oracle.json"
            self.assertEqual(
                main(["--engine", "pdftex", "--report", str(report_path)]), 0
            )
            report = json.loads(report_path.read_text(encoding="utf-8"))

        self.assertEqual(report["format"], "latexd.tfm-validity-oracle")
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual(report["compatibility_target"], "TeX82 via pdfTeX INITEX")
        self.assertIn("pdfTeX", report["engine"]["version"])
        self.assertTrue(Path(report["engine"]["path"]).is_absolute())
        self.assertEqual(len(report["engine"]["sha256"]), 64)
        self.assertEqual(
            report["invocation"], ["pdftex", "-ini", "-interaction=nonstopmode"]
        )
        self.assertEqual(
            report["environment"], {"locale": "C.UTF-8", "timezone": "UTC"}
        )
        self.assertEqual(report["expected_processes"], len(CASE_SPECS))
        self.assertEqual(report["observed_processes"], len(CASE_SPECS))
        self.assertEqual(set(report["base_tfm_files"]), {"cmr10.tfm", "cmex10.tfm"})
        self.assertEqual(set(report["case_results"]), EXPECTED_CASES)
        for case_id, result in report["case_results"].items():
            self.assertIn("This is pdfTeX", result["raw_output"], case_id)
            self.assertEqual(
                result["source_sha256"],
                hashlib.sha256(result["source"].encode()).hexdigest(),
                case_id,
            )


if __name__ == "__main__":
    unittest.main()
