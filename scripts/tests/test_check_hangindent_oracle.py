import hashlib
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from scripts.check_hangindent_oracle import (
    CASE_SPECS,
    EXPECTED_CASE_RESULTS,
    PLANNED_SOURCE_ACTIVATION_SET,
    main,
    parse_observations,
    run_oracle,
    validate_case_results,
)


class HangIndentOracleTests(unittest.TestCase):
    def test_ci_runs_policy_test_and_native_oracle_after_tex_install(self) -> None:
        workflow = (Path(__file__).parents[2] / ".github/workflows/ci.yml").read_text(
            encoding="utf-8"
        )

        policy_test = "scripts.tests.test_check_hangindent_oracle"
        oracle = "python3 scripts/check_hangindent_oracle.py"
        artifact = "hangindent-oracle"
        tex_install = "Install Computer Modern test fonts"
        self.assertIn(policy_test, workflow)
        self.assertIn(oracle, workflow)
        self.assertIn(artifact, workflow)
        self.assertLess(workflow.index(tex_install), workflow.index(oracle))

    def test_freezes_one_planned_source_name_and_paired_native_owners(self) -> None:
        self.assertEqual(PLANNED_SOURCE_ACTIVATION_SET, ["hangindent"])
        self.assertEqual(
            set(EXPECTED_CASE_RESULTS["default"]), {"hangindent", "dimen0"}
        )
        self.assertEqual(
            EXPECTED_CASE_RESULTS["default"]["hangindent"]["observations"],
            {"value": 0},
        )
        self.assertEqual(
            EXPECTED_CASE_RESULTS["default"]["dimen0"]["observations"],
            {"value": 0},
        )

    def test_production_runtime_has_no_hangindent_primitive_or_builtin_yet(self) -> None:
        repository = Path(__file__).parents[2]
        for relative_path in (
            "crates/tex-vm/src/command.rs",
            "crates/tex-vm/src/lib.rs",
        ):
            source = (repository / relative_path).read_text(encoding="utf-8")
            self.assertNotIn("Primitive::HangIndent", source, relative_path)
            self.assertNotIn('"hangindent" => Some(Primitive::', source, relative_path)

    def test_matrix_covers_scanning_scope_arithmetic_aliases_and_recovery(self) -> None:
        required = {
            "default",
            "direct_and_physical_units",
            "relative_units",
            "true_units",
            "internal_and_query",
            "dimexpr_unavailable",
            "scope_and_globaldefs",
            "arithmetic_and_odd_division",
            "afterassignment_alias_shadow_dynamic",
            "dimension_too_large",
            "negative_dimension_too_large",
            "missing_number",
            "illegal_unit",
            "arithmetic_overflow",
            "divide_by_zero",
        }
        self.assertEqual(set(CASE_SPECS), required)
        self.assertEqual(validate_case_results(EXPECTED_CASE_RESULTS), [])

        for case_id in required:
            hangindent = EXPECTED_CASE_RESULTS[case_id]["hangindent"]
            dimen0 = EXPECTED_CASE_RESULTS[case_id]["dimen0"]
            self.assertEqual(hangindent["exit_status"], dimen0["exit_status"])
            self.assertEqual(hangindent["diagnostics"], dimen0["diagnostics"])
            self.assertEqual(hangindent["observations"], dimen0["observations"])

        self.assertEqual(
            EXPECTED_CASE_RESULTS["internal_and_query"]["hangindent"][
                "observations"
            ],
            {"internal": 81920, "the": 81920, "ifdim": 1},
        )
        self.assertEqual(
            EXPECTED_CASE_RESULTS["arithmetic_and_odd_division"]["hangindent"][
                "observations"
            ]["advance_wraps"],
            -2147483648,
        )

    def test_gate1_contract_separates_full_i32_state_from_passive_command_identity(
        self,
    ) -> None:
        document = (
            Path(__file__).parents[2] / "docs/m13-3-dp1-hangindent.md"
        ).read_text(encoding="utf-8")

        self.assertIn("-2,147,483,648..=2,147,483,647", document)
        self.assertIn("passive identity and owner linkage only", document)
        self.assertIn("VmLayoutIntegerParameterStateV1", document)

    def test_parser_rejects_duplicate_markers(self) -> None:
        output = "LATEXD-HANGINDENT:value=1 LATEXD-HANGINDENT:value=2"
        with self.assertRaisesRegex(ValueError, "duplicate"):
            parse_observations(output)

    def test_validation_rejects_changed_missing_and_unexpected_cases(self) -> None:
        changed = json.loads(json.dumps(EXPECTED_CASE_RESULTS))
        changed["default"]["hangindent"]["observations"]["value"] = 1
        self.assertTrue(any("default" in item for item in validate_case_results(changed)))

        missing = json.loads(json.dumps(EXPECTED_CASE_RESULTS))
        del missing["missing_number"]
        self.assertTrue(
            any("missing_number" in item for item in validate_case_results(missing))
        )

        unexpected = json.loads(json.dumps(EXPECTED_CASE_RESULTS))
        unexpected["future_case"] = unexpected["default"]
        self.assertTrue(
            any("unexpected cases" in item for item in validate_case_results(unexpected))
        )

    @unittest.skipUnless(shutil.which("pdftex"), "pdftex is required for the oracle")
    def test_pdftex_initex_matches_characterization(self) -> None:
        self.assertEqual(run_oracle("pdftex"), EXPECTED_CASE_RESULTS)

    @unittest.skipUnless(shutil.which("pdftex"), "pdftex is required for the oracle")
    def test_cli_records_reproducible_engine_case_and_environment_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            report_path = Path(temp) / "oracle.json"

            self.assertEqual(
                main(["--engine", "pdftex", "--report", str(report_path)]), 0
            )
            report = json.loads(report_path.read_text(encoding="utf-8"))

        self.assertEqual(report["format"], "latexd.hangindent-oracle")
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual(report["compatibility_target"], "TeX82 via pdfTeX INITEX")
        self.assertEqual(report["planned_source_activation_set"], ["hangindent"])
        self.assertNotIn("source_enabled_set", report)
        self.assertIn("pdfTeX", report["engine"]["version"])
        self.assertTrue(Path(report["engine"]["path"]).is_absolute())
        self.assertEqual(len(report["engine"]["sha256"]), 64)
        self.assertEqual(report["invocation"], ["pdftex", "-ini", "-interaction=nonstopmode"])
        self.assertEqual(report["environment"]["locale"], "C.UTF-8")
        self.assertEqual(report["environment"]["timezone"], "UTC")
        self.assertEqual(report["expected_processes"], len(CASE_SPECS) * 2)
        self.assertEqual(report["observed_processes"], report["expected_processes"])
        self.assertEqual(report["font_metrics"]["requested_name"], "cmr10.tfm")
        self.assertTrue(Path(report["font_metrics"]["lookup_path"]).is_absolute())
        self.assertTrue(Path(report["font_metrics"]["resolved_path"]).is_absolute())
        self.assertEqual(len(report["font_metrics"]["sha256"]), 64)
        self.assertTrue(report["font_metrics"]["texmf_search_path"])
        semantic_results = json.loads(json.dumps(report["case_results"]))
        for case_id, owners in semantic_results.items():
            for owner, result in owners.items():
                raw_output = result.pop("raw_output")
                self.assertEqual(
                    result["source_sha256"],
                    hashlib.sha256(result["source"].encode()).hexdigest(),
                )
                self.assertIn("This is pdfTeX", raw_output)
                self.assertEqual(result, EXPECTED_CASE_RESULTS[case_id][owner])


if __name__ == "__main__":
    unittest.main()
