#!/usr/bin/env python3
from __future__ import annotations

import argparse
import gzip
import json
import shutil
import tarfile
import time
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "fixtures" / "arxiv-oracle" / "cc0-smoke.json"
DEFAULT_OUTPUT = Path.home() / ".cache" / "latexd" / "arxiv-cc0"
USER_AGENT = "latexd-local-corpus/0.1 (mailto:none@example.com)"


def read_manifest(path: Path) -> list[dict[str, object]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    cases = payload.get("cases")
    if not isinstance(cases, list):
        raise ValueError(f"{path} does not contain a cases array")
    return cases


def download(url: str, target: Path, force: bool) -> None:
    if target.exists() and target.stat().st_size > 0 and not force:
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    tmp = target.with_suffix(target.suffix + ".tmp")
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=60) as response:
        with tmp.open("wb") as handle:
            shutil.copyfileobj(response, handle)
    tmp.replace(target)


def safe_extract_tar(source: Path, target: Path) -> bool:
    try:
        with tarfile.open(source, mode="r:*") as archive:
            members = archive.getmembers()
            target_root = target.resolve()
            for member in members:
                member_path = (target / member.name).resolve()
                if target_root != member_path and target_root not in member_path.parents:
                    raise ValueError(f"tar member escapes output directory: {member.name}")
            archive.extractall(target)
            return True
    except tarfile.TarError:
        return False


def extract_source(source: Path, target: Path, toplevel: str, force: bool) -> None:
    if target.exists() and any(target.iterdir()) and not force:
        return
    if target.exists():
        shutil.rmtree(target)
    target.mkdir(parents=True, exist_ok=True)
    if safe_extract_tar(source, target):
        return
    try:
        payload = gzip.decompress(source.read_bytes())
    except OSError:
        payload = source.read_bytes()
    output_name = Path(toplevel).name or "main.tex"
    (target / output_name).write_bytes(payload)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Fetch local CC0 arXiv PDF/source corpus for latexd oracle tests."
    )
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--case", action="append", default=[], help="arXiv id to fetch")
    parser.add_argument("--limit", type=int, default=0, help="maximum number of cases to fetch")
    parser.add_argument("--force", action="store_true", help="redownload and re-extract")
    parser.add_argument("--delay", type=float, default=3.0, help="delay between arXiv requests")
    args = parser.parse_args()

    cases = read_manifest(args.manifest)
    selected = set(args.case)
    if selected:
        cases = [case for case in cases if str(case["arxiv_id"]) in selected]
    if args.limit > 0:
        cases = cases[: args.limit]

    args.output.mkdir(parents=True, exist_ok=True)
    for index, case in enumerate(cases):
        arxiv_id = str(case["arxiv_id"])
        toplevel = str(case["toplevel"])
        print(f"fetching {arxiv_id} ({case['title']})")
        raw_source = args.output / "raw" / f"{arxiv_id}.src"
        pdf = args.output / "pdfs" / f"{arxiv_id}.pdf"
        download(str(case["source_url"]), raw_source, args.force)
        if args.delay and index + 1 < len(cases):
            time.sleep(args.delay)
        download(str(case["pdf_url"]), pdf, args.force)
        extract_source(
            raw_source,
            args.output / "sources" / arxiv_id,
            toplevel,
            args.force,
        )
        if args.delay and index + 1 < len(cases):
            time.sleep(args.delay)

    print(f"corpus ready: {args.output}")


if __name__ == "__main__":
    main()
