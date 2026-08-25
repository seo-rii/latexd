import hashlib
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from scripts.check_tfm_validity_oracle import (
    CASE_SIZES,
    CASE_SPECS,
    EXPECTED_FIXTURE,
    TEX82_READ_FONT_INFO_SOURCE,
    main,
    mutate_tfm,
    run_oracle,
    validate_case_results,
)


EXPECTED_CASES = {
    "aggregate_length_mismatch",
    "charlist_self_cycle",
    "charlist_out_of_range",
    "design_size_below_one_pt",
    "invalid_character_range",
    "invalid_character_depth_index",
    "invalid_character_extensible_index",
    "invalid_character_height_index",
    "invalid_character_italic_index",
    "invalid_character_ligature_index",
    "invalid_character_width_index",
    "invalid_extensible",
    "invalid_extensible_bottom",
    "invalid_extensible_middle",
    "invalid_extensible_top",
    "invalid_depth_fix_word_sign",
    "invalid_fontdimen2",
    "invalid_fontdimen5",
    "invalid_height_fix_word_sign",
    "invalid_italic_fix_word_sign",
    "invalid_kern_fix_word",
    "invalid_ligkern",
    "invalid_ligkern_kern_index",
    "invalid_ligkern_next_character",
    "invalid_ligkern_skip",
    "invalid_ligature_target",
    "invalid_width_fix_word_sign",
    "nonzero_width_zero",
    "nonzero_width_zero_at_1sp",
    "nonzero_width_zero_at_16sp",
    "nonzero_depth_zero",
    "nonzero_depth_zero_at_1sp",
    "nonzero_depth_zero_at_16sp",
    "nonzero_height_zero",
    "nonzero_height_zero_at_1sp",
    "nonzero_height_zero_at_16sp",
    "nonzero_italic_zero",
    "nonzero_italic_zero_at_1sp",
    "nonzero_italic_zero_at_16sp",
    "premature_eof",
    "short_header",
    "short_np0",
    "short_np4",
    "short_np5",
    "signed_slant_parameter",
    "size_field_high_bit",
    "trailing_word",
    "valid_cmr10",
    "valid_cmr10_at_1sp",
    "valid_cmr10_at_16sp",
    "zero_depth_table_consistent",
    "zero_height_table_consistent",
    "zero_italic_table_consistent",
    "zero_width_table_consistent",
}

EXPLICIT_CASE_SIZES = {
    "nonzero_width_zero_at_1sp": {"mode": "at_sp", "value": 1},
    "nonzero_width_zero_at_16sp": {"mode": "at_sp", "value": 16},
    "nonzero_depth_zero_at_1sp": {"mode": "at_sp", "value": 1},
    "nonzero_depth_zero_at_16sp": {"mode": "at_sp", "value": 16},
    "nonzero_height_zero_at_1sp": {"mode": "at_sp", "value": 1},
    "nonzero_height_zero_at_16sp": {"mode": "at_sp", "value": 16},
    "nonzero_italic_zero_at_1sp": {"mode": "at_sp", "value": 1},
    "nonzero_italic_zero_at_16sp": {"mode": "at_sp", "value": 16},
    "valid_cmr10_at_1sp": {"mode": "at_sp", "value": 1},
    "valid_cmr10_at_16sp": {"mode": "at_sp", "value": 16},
}


class TfmValidityOracleTests(unittest.TestCase):
    def test_matrix_and_fixture_freeze_full_tfm_validity_boundaries(self) -> None:
        fixture = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))

        self.assertEqual(set(CASE_SPECS), EXPECTED_CASES)
        self.assertEqual(set(CASE_SIZES), EXPECTED_CASES)
        self.assertEqual(set(fixture["case_results"]), EXPECTED_CASES)
        self.assertEqual(set(fixture["case_sizes"]), EXPECTED_CASES)
        self.assertEqual(fixture["format"], "latexd.tfm-validity-oracle")
        self.assertEqual(fixture["schema_version"], 1)
        self.assertEqual(fixture["compatibility_target"], "TeX82 via pdfTeX INITEX")
        self.assertEqual(
            TEX82_READ_FONT_INFO_SOURCE,
            {
                "url": "https://tug.ctan.org/systems/knuth/dist/tex/tex.web",
                "sha256": "c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324",
                "loader_section_lines": [10870, 11210],
                "loader_section_sha256": "57f665ae4cc87c721d444fdde0a1817f194f44bab18388c42a1d26d830c6ddc8",
            },
        )
        self.assertEqual(validate_case_results(fixture["case_results"], fixture), [])

        for case_id in EXPECTED_CASES:
            expected_size = EXPLICIT_CASE_SIZES.get(case_id, {"mode": "natural"})
            self.assertEqual(CASE_SIZES[case_id], expected_size, case_id)
            self.assertEqual(fixture["case_sizes"][case_id], expected_size, case_id)

        for case_id, expected in fixture["case_results"].items():
            self.assertEqual(len(expected["mutated_tfm_sha256"]), 64, case_id)
            self.assertEqual(len(expected["source_sha256"]), 64, case_id)
            self.assertEqual(expected["observations"]["sentinel"], 1, case_id)

        for table in ("width", "height", "depth", "italic"):
            accepted = fixture["case_results"][f"nonzero_{table}_zero_at_1sp"]
            rejected = fixture["case_results"][f"nonzero_{table}_zero_at_16sp"]
            self.assertEqual(
                accepted["mutated_tfm_sha256"],
                rejected["mutated_tfm_sha256"],
                table,
            )
            self.assertEqual(accepted["diagnostics"], [], table)
            self.assertEqual(rejected["observations"]["font"], "nullfont", table)

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
        self.assertIn("dimension_subset::extract_exact_frame", contract)
        self.assertIn("ExactFrameLengthMismatch", contract)
        self.assertIn("REVISE_BOUNDARY", contract)
        self.assertIn("6a8db2a7-dd74-83ee-851b-4749f3f3fbd4", contract)
        self.assertIn("complete font-load validation remains open", contract)
        self.assertIn("54 byte-frozen mutations", contract)
        self.assertIn(TEX82_READ_FONT_INFO_SOURCE["sha256"], contract)
        self.assertIn("Phase 1", contract)
        self.assertIn("Phase 2", contract)
        self.assertIn("Phase 3", contract)
        self.assertIn("Phase 4", contract)
        self.assertIn("targeted natural-size", contract)
        self.assertIn("6a8ddef5-7b84-83e9-a8ff-b24a2c752739", contract)
        self.assertIn("REVISE_TFM_PLAN", contract)
        self.assertNotIn("rejection and acceptance inventory is complete", contract)
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
        self.assertEqual(report["compatibility_source"], TEX82_READ_FONT_INFO_SOURCE)
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
            self.assertEqual(result["size"], CASE_SIZES[case_id], case_id)
            self.assertIn("This is pdfTeX", result["raw_output"], case_id)
            self.assertEqual(
                result["source_sha256"],
                hashlib.sha256(result["source"].encode()).hexdigest(),
                case_id,
            )


if __name__ == "__main__":
    unittest.main()
