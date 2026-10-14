import unittest
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]


class CiPythonTestListTests(unittest.TestCase):
    def test_ci_runs_every_scripts_test_module(self) -> None:
        workflow_tokens = set(
            (REPOSITORY_ROOT / ".github/workflows/ci.yml")
            .read_text(encoding="utf-8")
            .split()
        )
        modules = sorted(
            f"scripts.tests.{path.stem}"
            for path in (REPOSITORY_ROOT / "scripts/tests").glob("test_*.py")
        )

        self.assertEqual(
            [module for module in modules if module not in workflow_tokens], []
        )


if __name__ == "__main__":
    unittest.main()
