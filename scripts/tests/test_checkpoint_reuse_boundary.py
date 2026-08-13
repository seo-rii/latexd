import re
import unittest
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]


class CheckpointReuseBoundaryPolicyTests(unittest.TestCase):
    def test_compiler_does_not_bypass_checkpoint_reuse_loader(self) -> None:
        source = (REPOSITORY_ROOT / "crates/latexd/src/compiler.rs").read_text(
            encoding="utf-8"
        )
        production_source = source.split("#[cfg(test)]", maxsplit=1)[0]

        self.assertFalse(
            "load_checkpoint_bundle(" in production_source,
            "compiler production code must use the checkpoint reuse loader",
        )

    def test_compiler_reads_stored_snapshots_through_attachment_boundary(self) -> None:
        source = (REPOSITORY_ROOT / "crates/latexd/src/compiler.rs").read_text(
            encoding="utf-8"
        )
        production_source = source.split("#[cfg(test)]", maxsplit=1)[0]

        self.assertIsNone(
            re.search(r"reused_checkpoint\s*\.snapshot(?:\s|\.)", production_source),
            "reusable preamble must use the snapshot attachment accessor",
        )
        replay_helper = production_source.split(
            "fn replay_checkpoint_from_stored", maxsplit=1
        )[1].split("fn select_shipout_replay_plan", maxsplit=1)[0]
        self.assertFalse(
            ".snapshot\n        .as_ref()" in replay_helper,
            "stored replay helper must use the snapshot attachment accessor",
        )
        self.assertGreaterEqual(
            production_source.count("snapshot_for_restore()"),
            3,
            "stored checkpoint consumers must support either snapshot lane",
        )

    def test_checkpoint_capture_checks_write_lane_eligibility(self) -> None:
        source = (REPOSITORY_ROOT / "crates/tex-checkpoint/src/lib.rs").read_text(
            encoding="utf-8"
        )
        builder = source.split(
            "pub fn build_checkpoint_bundle_with_shipouts", maxsplit=1
        )[1].split("pub fn save_checkpoint_bundle", maxsplit=1)[0]

        self.assertGreaterEqual(
            builder.count(".lane_for("),
            3,
            "every checkpoint category must suppress attachments outside its write lane",
        )


if __name__ == "__main__":
    unittest.main()
