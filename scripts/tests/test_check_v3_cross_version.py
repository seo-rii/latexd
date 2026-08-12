import unittest

from scripts.check_v3_cross_version import validate_layered_matrix, validate_matrix


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


def valid_layered_result() -> dict[str, object]:
    return {
        "output": "LGZLGALGARGA",
        "diagnostic_count": 0,
        "scopes": [
            {
                "vthreex": {"kind": "macro"},
                "vthreey": {"kind": "macro"},
            },
            {"vthreex": {"kind": "macro"}},
            {},
            {
                "vthreex": {"kind": "macro"},
                "vthreez": {"kind": "macro"},
            },
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

    def test_accepts_exact_layered_bidirectional_results(self) -> None:
        result = valid_layered_result()

        self.assertEqual(validate_layered_matrix(result, result.copy()), [])

    def test_rejects_collapsed_or_stale_layered_results(self) -> None:
        old_to_new = valid_layered_result()
        new_to_old = valid_layered_result()
        new_to_old["scopes"] = [
            old_to_new["scopes"][0],
            old_to_new["scopes"][1],
            old_to_new["scopes"][3],
        ]

        violations = validate_layered_matrix(old_to_new, new_to_old)

        self.assertTrue(any("scope depth" in violation for violation in violations))
        self.assertTrue(any("directions differ" in violation for violation in violations))


if __name__ == "__main__":
    unittest.main()
