#!/usr/bin/env python3
"""Materialize the content-addressed TeX82 TFM validity corpus."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

if __package__:
    from scripts.check_tfm_validity_oracle import (
        CORPUS_ROOT,
        build_case_inputs,
        build_corpus_manifest,
    )
else:
    from check_tfm_validity_oracle import (
        CORPUS_ROOT,
        build_case_inputs,
        build_corpus_manifest,
    )


def write_corpus(destination: Path) -> None:
    if destination.exists():
        raise FileExistsError(f"refusing to replace existing corpus: {destination}")
    case_inputs = build_case_inputs()
    manifest = build_corpus_manifest(case_inputs)
    blob_root = destination / "blobs"
    blob_root.mkdir(parents=True)
    cases = {case["id"]: case for case in manifest["cases"]}
    for case_id, raw in case_inputs.items():
        path = blob_root / f"{cases[case_id]['blob_sha256']}.tfm"
        if not path.exists():
            path.write_bytes(raw)
    (destination / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=CORPUS_ROOT)
    args = parser.parse_args(argv)
    write_corpus(args.output)
    print(f"wrote content-addressed TFM validity corpus: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
