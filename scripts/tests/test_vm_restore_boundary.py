import tomllib
import unittest
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]


class VmRestoreBoundaryPolicyTests(unittest.TestCase):
    def test_asserting_restore_is_disallowed_in_production_targets(self) -> None:
        config = tomllib.loads(
            (REPOSITORY_ROOT / "clippy.toml").read_text(encoding="utf-8")
        )
        rules = {
            rule["path"]: rule["reason"]
            for rule in config.get("disallowed-methods", [])
        }

        self.assertEqual(
            rules.get("tex_vm::Vm::restore"),
            "persisted or untrusted snapshots must use Vm::try_restore",
        )


if __name__ == "__main__":
    unittest.main()
