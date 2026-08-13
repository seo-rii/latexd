import shutil
import unittest
from pathlib import Path

from scripts.check_mathcode_delcode_oracle import (
    EXPECTED_OBSERVATIONS,
    EXPECTED_REJECTIONS,
    parse_observations,
    run_oracle,
    run_rejection_oracle,
    validate_observations,
    validate_rejections,
)


class MathcodeDelcodeOracleTests(unittest.TestCase):
    def test_ci_runs_policy_tests_and_native_oracle_after_tex_install(self) -> None:
        workflow = (Path(__file__).parents[2] / ".github/workflows/ci.yml").read_text(
            encoding="utf-8"
        )

        policy_test = "scripts.tests.test_check_mathcode_delcode_oracle"
        oracle = "python3 scripts/check_mathcode_delcode_oracle.py"
        tex_install = "Install Computer Modern test fonts"
        self.assertIn(policy_test, workflow)
        self.assertIn(oracle, workflow)
        self.assertLess(workflow.index(tex_install), workflow.index(oracle))

    def test_parses_wrapped_tex_message_observations(self) -> None:
        output = """This is pdfTeX
LATEXD-ORACLE:math_A_default=28993
LATEXD-ORACLE:math_A_local=123 LATEXD-ORACLE:math_A_restored=28993
No pages of output.
"""

        self.assertEqual(
            parse_observations(output),
            {
                "math_A_default": 28993,
                "math_A_local": 123,
                "math_A_restored": 28993,
            },
        )

    def test_accepts_exact_tex82_observations(self) -> None:
        self.assertEqual(validate_observations(EXPECTED_OBSERVATIONS), [])

    def test_rejects_changed_default(self) -> None:
        observations = EXPECTED_OBSERVATIONS.copy()
        observations["math_A_default"] = 65

        violations = validate_observations(observations)

        self.assertTrue(any("math_A_default" in item for item in violations))

    def test_rejects_missing_observation(self) -> None:
        observations = EXPECTED_OBSERVATIONS.copy()
        del observations["del_A_default"]

        violations = validate_observations(observations)

        self.assertTrue(any("del_A_default" in item for item in violations))

    def test_rejects_unexpected_observation(self) -> None:
        observations = EXPECTED_OBSERVATIONS | {"future_engine_value": 1}

        violations = validate_observations(observations)

        self.assertTrue(any("unexpected observations" in item for item in violations))

    def test_accepts_exact_tex82_rejections(self) -> None:
        self.assertEqual(validate_rejections(EXPECTED_REJECTIONS), [])

    def test_rejects_changed_numeric_boundary(self) -> None:
        rejections = EXPECTED_REJECTIONS.copy()
        rejections["mathcode_too_large"] = "Bad mathchar (65536)"

        violations = validate_rejections(rejections)

        self.assertTrue(any("mathcode_too_large" in item for item in violations))

    @unittest.skipUnless(shutil.which("pdftex"), "pdftex is required for the oracle")
    def test_pdftex_initex_matches_characterization(self) -> None:
        self.assertEqual(run_oracle("pdftex"), EXPECTED_OBSERVATIONS)

    @unittest.skipUnless(shutil.which("pdftex"), "pdftex is required for the oracle")
    def test_pdftex_initex_rejects_out_of_range_values(self) -> None:
        self.assertEqual(run_rejection_oracle("pdftex"), EXPECTED_REJECTIONS)


if __name__ == "__main__":
    unittest.main()
