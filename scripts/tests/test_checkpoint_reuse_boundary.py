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


if __name__ == "__main__":
    unittest.main()
