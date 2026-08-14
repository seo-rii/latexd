import copy
import hashlib
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from scripts.check_layout_integer_parameters_oracle import (
    EXPECTED_OBSERVATIONS,
    EXPECTED_REJECTIONS,
    ORACLE_SOURCE,
    PARAMETER_DEFAULTS,
    main,
    parse_observations,
    run_oracle,
    run_rejection_oracle,
    validate_observations,
    validate_rejections,
)


class LayoutIntegerParametersOracleTests(unittest.TestCase):
    def test_ci_runs_policy_test_and_native_oracle_after_tex_install(self) -> None:
        workflow = (Path(__file__).parents[2] / ".github/workflows/ci.yml").read_text(
            encoding="utf-8"
        )

        policy_test = "scripts.tests.test_check_layout_integer_parameters_oracle"
        oracle = "python3 scripts/check_layout_integer_parameters_oracle.py"
        artifact = "layout-integer-parameters-oracle"
        tex_install = "Install Computer Modern test fonts"
        self.assertIn(policy_test, workflow)
        self.assertIn(oracle, workflow)
        self.assertIn(artifact, workflow)
        self.assertLess(workflow.index(tex_install), workflow.index(oracle))

    def test_freezes_exact_layout_only_parameter_set_and_initex_defaults(self) -> None:
        self.assertEqual(
            PARAMETER_DEFAULTS,
            {
                "adjdemerits": 0,
                "binoppenalty": 0,
                "brokenpenalty": 0,
                "clubpenalty": 0,
                "displaywidowpenalty": 0,
                "doublehyphendemerits": 0,
                "exhyphenpenalty": 0,
                "finalhyphendemerits": 0,
                "hangafter": 1,
                "hyphenpenalty": 0,
                "interlinepenalty": 0,
                "linepenalty": 0,
                "looseness": 0,
                "postdisplaypenalty": 0,
                "predisplaypenalty": 0,
                "pretolerance": 0,
                "relpenalty": 0,
                "widowpenalty": 0,
            },
        )
        self.assertEqual(list(PARAMETER_DEFAULTS), sorted(PARAMETER_DEFAULTS))
        self.assertNotIn("tolerance", PARAMETER_DEFAULTS)

    def test_accepts_exact_per_name_storage_and_integer_domain_contract(self) -> None:
        self.assertEqual(validate_observations(EXPECTED_OBSERVATIONS), [])

        for name, default in PARAMETER_DEFAULTS.items():
            self.assertEqual(EXPECTED_OBSERVATIONS[f"default_{name}"], default)
            self.assertEqual(EXPECTED_OBSERVATIONS[f"local_{name}"], 123)
            self.assertEqual(EXPECTED_OBSERVATIONS[f"restored_{name}"], default)
            self.assertEqual(EXPECTED_OBSERVATIONS[f"max_{name}"], 2_147_483_647)
            self.assertEqual(EXPECTED_OBSERVATIONS[f"min_{name}"], -2_147_483_647)

    def test_accepts_representative_scope_query_arithmetic_and_alias_contract(self) -> None:
        expected = {
            "global": 321,
            "negative_globaldefs_local": 777,
            "negative_globaldefs_restored": 321,
            "positive_globaldefs": 111,
            "octal": 83,
            "hexadecimal": 4660,
            "character": 65,
            "repeated_signs": 17,
            "advanced": 17,
            "multiplied": -51,
            "divided": -25,
            "advance_wraps": -2_147_483_648,
            "number": -2_147_483_648,
            "ifnum": 1,
            "afterassignment": 1,
            "afterassignment_value": 44,
            "alias_value": 45,
            "explicit_redefinition": 46,
            "restored_builtin": 45,
        }
        for name, value in expected.items():
            self.assertEqual(EXPECTED_OBSERVATIONS[name], value)

    def test_parses_wrapped_tex_message_observations(self) -> None:
        output = """This is pdfTeX
LATEXD-LAYOUT-INT:default_pretolerance=0
LATEXD-LAYOUT-INT:local_pretolerance=123 LATEXD-LAYOUT-INT:restored_pretolerance=0
No pages of output.
"""

        self.assertEqual(
            parse_observations(output),
            {
                "default_pretolerance": 0,
                "local_pretolerance": 123,
                "restored_pretolerance": 0,
            },
        )

    def test_rejects_changed_missing_and_unexpected_observations(self) -> None:
        changed = EXPECTED_OBSERVATIONS.copy()
        changed["default_hangafter"] = 0
        self.assertTrue(
            any("default_hangafter" in item for item in validate_observations(changed))
        )

        missing = EXPECTED_OBSERVATIONS.copy()
        del missing["max_widowpenalty"]
        self.assertTrue(
            any("max_widowpenalty" in item for item in validate_observations(missing))
        )

        unexpected = EXPECTED_OBSERVATIONS | {"future_behavior": 1}
        self.assertTrue(
            any(
                "unexpected observations" in item
                for item in validate_observations(unexpected)
            )
        )

    def test_accepts_exact_shared_scanner_and_arithmetic_recovery_contract(self) -> None:
        self.assertEqual(validate_rejections(EXPECTED_REJECTIONS), [])
        self.assertEqual(
            set(EXPECTED_REJECTIONS),
            {
                "positive_number_too_big",
                "negative_number_too_big",
                "missing_number",
                "multiply_overflow",
                "divide_by_zero",
            },
        )
        self.assertEqual(
            EXPECTED_REJECTIONS["positive_number_too_big"]["observations"],
            {"afterassignment": 1, "value": 2_147_483_647},
        )
        self.assertEqual(
            EXPECTED_REJECTIONS["negative_number_too_big"]["observations"],
            {"value": -2_147_483_647},
        )
        self.assertEqual(
            EXPECTED_REJECTIONS["multiply_overflow"]["observations"],
            {"value": 1_073_741_824},
        )

    def test_rejects_changed_recovery_diagnostics(self) -> None:
        rejections = copy.deepcopy(EXPECTED_REJECTIONS)
        rejections["multiply_overflow"]["diagnostics"] = ["Number too big"]

        self.assertTrue(
            any("multiply_overflow" in item for item in validate_rejections(rejections))
        )

    @unittest.skipUnless(shutil.which("pdftex"), "pdftex is required for the oracle")
    def test_pdftex_initex_matches_characterization(self) -> None:
        run = run_oracle("pdftex")

        self.assertEqual(run["exit_status"], 0)
        self.assertEqual(run["observations"], EXPECTED_OBSERVATIONS)
        self.assertEqual(
            run["source_sha256"], hashlib.sha256(ORACLE_SOURCE.encode()).hexdigest()
        )

    @unittest.skipUnless(shutil.which("pdftex"), "pdftex is required for the oracle")
    def test_pdftex_initex_matches_recovery_characterization(self) -> None:
        self.assertEqual(run_rejection_oracle("pdftex"), EXPECTED_REJECTIONS)

    @unittest.skipUnless(shutil.which("pdftex"), "pdftex is required for the oracle")
    def test_cli_records_reproducible_engine_and_probe_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            report_path = Path(temp) / "oracle.json"

            self.assertEqual(
                main(["--engine", "pdftex", "--report", str(report_path)]), 0
            )
            report = json.loads(report_path.read_text(encoding="utf-8"))

        self.assertEqual(report["format"], "latexd.layout-integer-parameters-oracle")
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual(report["compatibility_target"], "TeX82 via pdfTeX INITEX")
        self.assertEqual(
            report["planned_source_activation_set"], list(PARAMETER_DEFAULTS)
        )
        self.assertEqual(report["parameter_defaults"], PARAMETER_DEFAULTS)
        self.assertNotIn("source_enabled_set", report)
        self.assertIn("pdfTeX", report["engine"]["version"])
        self.assertTrue(Path(report["engine"]["path"]).is_absolute())
        self.assertEqual(len(report["engine"]["sha256"]), 64)
        self.assertEqual(report["valid_probe"]["source"], ORACLE_SOURCE)
        self.assertEqual(report["valid_probe"]["observations"], EXPECTED_OBSERVATIONS)
        self.assertEqual(report["rejection_probes"], EXPECTED_REJECTIONS)


if __name__ == "__main__":
    unittest.main()
