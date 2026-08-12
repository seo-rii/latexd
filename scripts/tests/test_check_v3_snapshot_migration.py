import unittest

from scripts.check_v3_snapshot_migration import validate_pre_reader_results


EXPECTED_RESULTS = {
    "raw_field_only": {
        "accepted": True,
        "muskip_field_preserved": False,
        "output": "R",
    },
    "checkpoint_versioned_only": {
        "accepted": True,
        "replay_safe": False,
    },
    "checkpoint_dual_lane": {
        "accepted": True,
        "replay_safe": True,
    },
    "raw_versioned_document": {
        "accepted": False,
    },
}


class V3SnapshotMigrationBaselineTests(unittest.TestCase):
    def test_accepts_exact_pre_reader_characterization(self) -> None:
        self.assertEqual(validate_pre_reader_results(EXPECTED_RESULTS), [])

    def test_rejects_field_only_probe_that_preserves_unknown_state(self) -> None:
        results = {name: result.copy() for name, result in EXPECTED_RESULTS.items()}
        results["raw_field_only"]["muskip_field_preserved"] = True

        violations = validate_pre_reader_results(results)

        self.assertTrue(any("raw_field_only" in item for item in violations))

    def test_rejects_versioned_only_checkpoint_that_old_reader_replays(self) -> None:
        results = {name: result.copy() for name, result in EXPECTED_RESULTS.items()}
        results["checkpoint_versioned_only"]["replay_safe"] = True

        violations = validate_pre_reader_results(results)

        self.assertTrue(any("checkpoint_versioned_only" in item for item in violations))

    def test_rejects_dual_lane_checkpoint_that_old_reader_does_not_replay(self) -> None:
        results = {name: result.copy() for name, result in EXPECTED_RESULTS.items()}
        results["checkpoint_dual_lane"]["replay_safe"] = False

        violations = validate_pre_reader_results(results)

        self.assertTrue(any("checkpoint_dual_lane" in item for item in violations))

    def test_rejects_versioned_document_that_old_raw_reader_accepts(self) -> None:
        results = {name: result.copy() for name, result in EXPECTED_RESULTS.items()}
        results["raw_versioned_document"]["accepted"] = True

        violations = validate_pre_reader_results(results)

        self.assertTrue(any("raw_versioned_document" in item for item in violations))


if __name__ == "__main__":
    unittest.main()
