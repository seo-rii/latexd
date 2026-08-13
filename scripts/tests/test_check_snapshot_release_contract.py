import unittest
from pathlib import Path

from scripts.check_snapshot_release_contract import (
    CARGO_FEATURE_MODE,
    EXPECTED_SERDE_JSON_FEATURES,
    RELEASE_TARGET,
    RUST_TOOLCHAIN,
    build_release_report,
    cargo_feature_graph_command,
    release_commands,
    validate_serde_json_features,
)


FEATURE_GRAPH = '''
├── serde_json feature "default"
│   ├── serde_json v1.0.149
│   └── serde_json feature "std"
│       └── serde_json v1.0.149 (*)
├── serde_json feature "raw_value"
│   └── serde_json v1.0.149 (*)
'''


class SnapshotReleaseContractTests(unittest.TestCase):
    def test_report_records_exact_release_inputs(self) -> None:
        report = build_release_report(
            feature_graph="serde graph",
            rustc="rustc 1.94.0\ncommit-hash: abc\n",
            revision="deadbeef",
            cargo_lock=b"locked dependencies",
            commands=[["cargo", "build"]],
            skip_migration=False,
        )

        self.assertEqual(report["cargo_feature_mode"], "default")
        self.assertEqual(report["cargo_feature_graph"], "serde graph")
        self.assertEqual(report["repository_revision"], "deadbeef")
        self.assertEqual(report["rustc_version"], "rustc 1.94.0\ncommit-hash: abc")
        self.assertEqual(
            report["cargo_lock_sha256"],
            "be44b1fe7b37130063887b70e33638dbae5a5e67aa2051f83abe47b2c98d0560",
        )

    def test_feature_graph_is_resolved_from_the_release_binary(self) -> None:
        self.assertEqual(
            cargo_feature_graph_command(),
            ["cargo", "tree", "--locked", "-e", "features", "-p", "latexd"],
        )

    def test_ci_runs_and_publishes_pinned_release_contract(self) -> None:
        workflow = (
            Path(__file__).resolve().parents[2] / ".github/workflows/ci.yml"
        ).read_text(encoding="utf-8")

        self.assertIn("toolchain: 1.94.0", workflow)
        self.assertIn("targets: x86_64-unknown-linux-gnu", workflow)
        self.assertIn("python3 scripts/check_snapshot_release_contract.py", workflow)
        self.assertIn("name: snapshot-release-contract", workflow)

    def test_accepts_exact_serde_json_feature_graph(self) -> None:
        self.assertEqual(validate_serde_json_features(FEATURE_GRAPH), [])
        self.assertEqual(
            EXPECTED_SERDE_JSON_FEATURES,
            {"default", "raw_value", "std"},
        )

    def test_rejects_ordering_affecting_feature(self) -> None:
        violations = validate_serde_json_features(
            FEATURE_GRAPH + '\n└── serde_json feature "preserve_order"\n'
        )

        self.assertTrue(any("preserve_order" in item for item in violations))

    def test_rejects_missing_raw_value_feature(self) -> None:
        violations = validate_serde_json_features(
            FEATURE_GRAPH.replace('├── serde_json feature "raw_value"\n', "")
        )

        self.assertTrue(any("raw_value" in item for item in violations))

    def test_release_commands_pin_profile_lock_target_and_clean_golden_runs(self) -> None:
        commands = release_commands("/tmp/release-a", "/tmp/release-b")
        rendered = [" ".join(command) for command in commands]

        self.assertEqual(RUST_TOOLCHAIN, "1.94.0")
        self.assertEqual(RELEASE_TARGET, "x86_64-unknown-linux-gnu")
        self.assertEqual(CARGO_FEATURE_MODE, "default")
        self.assertTrue(all("--release" in command for command in rendered))
        self.assertTrue(all("--locked" in command for command in rendered))
        self.assertTrue(all(f"--target {RELEASE_TARGET}" in command for command in rendered))
        golden_commands = [
            command
            for command in rendered
            if "v3_snapshot_document_contract" in command
        ]
        self.assertEqual(len(golden_commands), 2)
        self.assertTrue(any("/tmp/release-a" in command for command in golden_commands))
        self.assertTrue(any("/tmp/release-b" in command for command in golden_commands))


if __name__ == "__main__":
    unittest.main()
