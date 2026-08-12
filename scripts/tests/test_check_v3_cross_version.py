import unittest

from scripts.check_v3_cross_version import validate_matrix


def valid_result() -> dict[str, object]:
    return {
        "output": "LRRMZ",
        "diagnostic_count": 0,
        "scopes": [
            {
                "vthreealias": {"kind": "macro"},
                "vthreeprimitive": {"kind": "primitive"},
                "vthreeroot": {"kind": "macro"},
                "vthreetoken": {"kind": "token"},
            }
        ],
    }


class V3CrossVersionMatrixTests(unittest.TestCase):
    def test_accepts_equivalent_bidirectional_results(self) -> None:
        result = valid_result()

        self.assertEqual(validate_matrix(result, result.copy()), [])

    def test_rejects_behavior_or_projection_drift(self) -> None:
        old_to_new = valid_result()
        new_to_old = valid_result()
        new_to_old["output"] = "LRMZ"
        new_to_old["diagnostic_count"] = 1
        new_to_old["scopes"] = []

        violations = validate_matrix(old_to_new, new_to_old)

        self.assertTrue(any("output" in violation for violation in violations))
        self.assertTrue(any("diagnostic" in violation for violation in violations))
        self.assertTrue(any("scope" in violation for violation in violations))
        self.assertTrue(any("directions differ" in violation for violation in violations))


if __name__ == "__main__":
    unittest.main()
