import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from scripts.check_tfm_validity_oracle import (
    CASE_SIZES,
    CASE_SPECS,
    CORPUS_MANIFEST,
    CORPUS_ROOT,
    EXPECTED_FIXTURE,
    TEX82_READ_FONT_INFO_SOURCE,
    build_case_inputs,
    load_corpus_case_inputs,
    main,
    mutate_tfm,
    run_oracle,
    validate_case_results,
    validate_corpus_manifest,
)


EXPECTED_CASES = {
    "aggregate_length_mismatch",
    "charlist_self_cycle",
    "charlist_out_of_range",
    "charlist_target_in_range_absent",
    "charlist_three_node_cycle",
    "charlist_two_node_cycle",
    "valid_charlist_acyclic_chain",
    "character_range_ec256",
    "design_size_below_one_pt",
    "design_size_exactly_one_pt",
    "empty_range_2_1",
    "empty_range_256_255",
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
    "extensible_bottom_in_range_absent",
    "extensible_middle_in_range_absent",
    "extensible_repeat_in_range_absent",
    "extensible_top_in_range_absent",
    "invalid_depth_fix_word_sign",
    "invalid_fontdimen2",
    "invalid_fontdimen5",
    "invalid_height_fix_word_sign",
    "invalid_italic_fix_word_sign",
    "invalid_at_size_limit",
    "invalid_at_size_zero",
    "invalid_kern_fix_word",
    "invalid_ligkern",
    "invalid_ligkern_kern_index",
    "invalid_ligkern_next_character",
    "invalid_ligkern_skip",
    "invalid_ligature_target",
    "invalid_boundary_label",
    "ligature_target_in_range_absent",
    "ligkern_next_in_range_absent",
    "valid_boundary_character_absent_next_bypass",
    "valid_boundary_label",
    "valid_ligkern_restart",
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
    "parameter_8_invalid_fix_word",
    "parameter_count_8_valid",
    "short_header",
    "minimal_header_lh2",
    "short_np0",
    "short_np4",
    "short_np5",
    "signed_slant_parameter",
    "size_field_high_bit",
    "trailing_word",
    "trailing_1_byte_nonzero",
    "trailing_2_bytes_nonzero",
    "trailing_3_bytes_nonzero",
    "trailing_long_nonzero",
    "valid_cmr10",
    "valid_cmr10_at_1sp",
    "valid_cmr10_at_16sp",
    "valid_cmr10_at_max_sp",
    "zero_depth_table_consistent",
    "zero_height_table_consistent",
    "zero_italic_table_consistent",
    "zero_width_table_consistent",
}

EXPLICIT_CASE_SIZES = {
    "invalid_at_size_zero": {"mode": "at_sp", "value": 0},
    "invalid_at_size_limit": {"mode": "at_sp", "value": 1 << 27},
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
    "valid_cmr10_at_max_sp": {"mode": "at_sp", "value": (1 << 27) - 1},
}


class TfmValidityOracleTests(unittest.TestCase):
    def test_v2_corpus_generator_is_executable_from_repository_root(self) -> None:
        repository = Path(__file__).parents[2]
        completed = subprocess.run(
            [
                sys.executable,
                "scripts/generate_tfm_validity_corpus.py",
                "--help",
            ],
            cwd=repository,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )

        self.assertEqual(completed.returncode, 0, completed.stdout)
        self.assertIn("content-addressed TeX82 TFM validity corpus", completed.stdout)

    def test_v2_corpus_freezes_exact_bytes_and_three_way_classification(
        self,
    ) -> None:
        manifest = json.loads(CORPUS_MANIFEST.read_text(encoding="utf-8"))
        self.assertEqual(validate_corpus_manifest(manifest, CORPUS_ROOT), [])
        self.assertEqual(manifest["format"], "latexd.tfm-validity-corpus")
        self.assertEqual(manifest["schema_version"], 2)
        self.assertEqual(len(manifest["cases"]), 82)

        expected_case_keys = {
            "blob_sha256",
            "expected_classification",
            "first_rejecting_rule",
            "id",
            "requested_size",
            "resolved_effective_size_sp",
            "supports_rules",
            "validator_input_size_sp",
        }
        cases = {case["id"]: case for case in manifest["cases"]}
        self.assertEqual(set(cases), EXPECTED_CASES)
        self.assertEqual(
            {case["expected_classification"] for case in cases.values()},
            {
                "AcceptedByNativeLoader",
                "InvalidEffectiveSize",
                "MalformedTfm",
            },
        )
        for case_id, case in cases.items():
            self.assertEqual(set(case), expected_case_keys, case_id)
            self.assertEqual(case["requested_size"], CASE_SIZES[case_id], case_id)

        generated = build_case_inputs()
        loaded = load_corpus_case_inputs()
        self.assertEqual(set(generated), EXPECTED_CASES)
        self.assertEqual(loaded, generated)
        for case_id, blob in loaded.items():
            self.assertEqual(
                hashlib.sha256(blob).hexdigest(),
                cases[case_id]["blob_sha256"],
                case_id,
            )

        manifest_hashes = {case["blob_sha256"] for case in cases.values()}
        blob_files = {path.stem for path in (CORPUS_ROOT / "blobs").glob("*.tfm")}
        self.assertEqual(blob_files, manifest_hashes)

    def test_v2_corpus_normalizes_size_and_first_rejection_semantics(self) -> None:
        manifest = json.loads(CORPUS_MANIFEST.read_text(encoding="utf-8"))
        cases = {case["id"]: case for case in manifest["cases"]}

        for case_id, requested in (
            ("invalid_at_size_zero", 0),
            ("invalid_at_size_limit", 1 << 27),
        ):
            case = cases[case_id]
            self.assertEqual(case["expected_classification"], "InvalidEffectiveSize")
            self.assertEqual(case["first_rejecting_rule"], "TFM-SIZE-001")
            self.assertEqual(case["resolved_effective_size_sp"], 655_360)
            self.assertEqual(case["validator_input_size_sp"], requested)

        self.assertEqual(cases["valid_cmr10"]["resolved_effective_size_sp"], 655_360)
        self.assertEqual(
            cases["design_size_exactly_one_pt"]["resolved_effective_size_sp"],
            65_536,
        )
        for case_id, rule_id in (
            ("aggregate_length_mismatch", "TFM-GEOMETRY-001"),
            ("short_header", "TFM-HEADER-001"),
            ("design_size_below_one_pt", "TFM-HEADER-002"),
        ):
            case = cases[case_id]
            self.assertEqual(case["expected_classification"], "MalformedTfm")
            self.assertEqual(case["first_rejecting_rule"], rule_id)
            self.assertIsNone(case["resolved_effective_size_sp"])
            self.assertEqual(case["validator_input_size_sp"], 655_360)

        parameter_tail = cases["parameter_8_invalid_fix_word"]
        self.assertEqual(parameter_tail["first_rejecting_rule"], "TFM-PARAM-002")
        self.assertEqual(
            parameter_tail["supports_rules"],
            ["TFM-PARAM-002", "TFM-PARAM-003"],
        )
        self.assertIsNone(cases["trailing_3_bytes_nonzero"]["first_rejecting_rule"])

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
        plan = (Path(__file__).parents[2] / "PLAN.md").read_text(encoding="utf-8")

        self.assertIn("full TFM validity", contract)
        self.assertIn("dimension-subset", contract)
        self.assertIn("dimension_subset::extract_exact_frame", contract)
        self.assertIn("ExactFrameLengthMismatch", contract)
        self.assertIn("REVISE_BOUNDARY", contract)
        self.assertIn("6a8db2a7-dd74-83ee-851b-4749f3f3fbd4", contract)
        self.assertIn("complete font-load validation remains open", contract)
        self.assertIn("82 byte-frozen mutations", contract)
        self.assertIn(
            "bb48c1a684727289ff254c394faa5285595b3f5aed7663e28d0b717c45d7a4aa",
            contract,
        )
        self.assertIn("tex82-read-font-info-validation-rules.md", contract)
        self.assertIn(TEX82_READ_FONT_INFO_SOURCE["sha256"], contract)
        self.assertIn("Phase 1", contract)
        self.assertIn("Phase 2", contract)
        self.assertIn("Phase 3", contract)
        self.assertIn("Phase 4", contract)
        self.assertIn("targeted natural-size", contract)
        self.assertIn("6a8ddef5-7b84-83e9-a8ff-b24a2c752739", contract)
        self.assertIn("REVISE_TFM_PLAN", contract)
        self.assertIn("6a8e2bef-e164-83e8-99ee-be8002ced80f", contract)
        self.assertIn("PROCEED_PRIVATE_TFM_VALIDATOR", contract)
        self.assertIn("HeaderCheckedTfm", contract)
        self.assertIn("content-addressed v2 corpus", contract)
        self.assertIn("69 unique SHA-256 blobs", contract)
        self.assertIn("AcceptedByNativeLoader", contract)
        self.assertIn("InvalidEffectiveSize", contract)
        self.assertIn("MalformedTfm", contract)
        self.assertIn("validator_input_size_sp", contract)
        self.assertIn("same persisted bytes", contract)
        self.assertNotIn("rejection and acceptance inventory is complete", contract)
        self.assertIn("W3 remains blocked", contract)
        for evidence in (
            "6a8e358c-697c-83e8-a6ba-881a469553d7",
            "REVISE_PRIVATE_TFM_HEADER",
            "33/33 semantic rule cells",
            "content-addressed v2 corpus",
            "69 unique SHA-256 blobs",
        ):
            self.assertIn(evidence, plan)

    def test_source_rule_ledger_maps_stateful_and_size_dependent_gates(self) -> None:
        ledger = (
            Path(__file__).parents[2]
            / "docs/tex82-read-font-info-validation-rules.md"
        ).read_text(encoding="utf-8")

        required_evidence = (
            TEX82_READ_FONT_INFO_SOURCE["sha256"],
            TEX82_READ_FONT_INFO_SOURCE["loader_section_sha256"],
            "6a8ddef5-7b84-83e9-a8ff-b24a2c752739",
            "TFM-SIZE-001",
            "TFM-RANGE-003",
            "TFM-BOX-003",
            "TFM-LIGKERN-004",
            "TFM-EXT-002",
            "TFM-PARAM-003",
            "TFM-EOF-002",
            "nonzero_width_zero_at_1sp",
            "nonzero_width_zero_at_16sp",
            "empty_range_256_255",
            "valid_boundary_character_absent_next_bypass",
            "ligkern_next_in_range_absent",
            "charlist_target_in_range_absent",
            "extensible_repeat_in_range_absent",
            "parameter_count_8_valid",
            "trailing_3_bytes_nonzero",
            "private-first",
            "InvalidEffectiveSize",
            "MalformedTfm",
            "6a8e2bef-e164-83e8-99ee-be8002ced80f",
            "PROCEED_PRIVATE_TFM_VALIDATOR",
            "HeaderCheckedTfm",
        )
        for evidence in required_evidence:
            self.assertIn(evidence, ledger)

        self.assertNotIn("ValidatedTfmAtSize is implemented", ledger)

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
