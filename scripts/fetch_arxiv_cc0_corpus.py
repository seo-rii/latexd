#!/usr/bin/env python3
from __future__ import annotations

import argparse
import bz2
import gzip
import json
import lzma
import shutil
import tarfile
import tempfile
import time
import urllib.request
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "fixtures" / "arxiv-oracle" / "cc0-smoke.json"
DEFAULT_OUTPUT = Path.home() / ".cache" / "latexd" / "arxiv-cc0"
USER_AGENT = "latexd-local-corpus/0.1 (mailto:none@example.com)"
MAX_DOWNLOAD_BYTES = 256 * 1024 * 1024
MAX_SOURCE_BYTES = 512 * 1024 * 1024
MAX_SOURCE_FILE_BYTES = 128 * 1024 * 1024
MAX_SOURCE_MEMBERS = 10_000


def read_manifest(path: Path) -> list[dict[str, object]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    cases = payload.get("cases")
    if not isinstance(cases, list):
        raise ValueError(f"{path} does not contain a cases array")
    return cases


def download(url: str, target: Path, force: bool) -> None:
    if target.exists() and target.stat().st_size > 0 and not force:
        if target.stat().st_size > MAX_DOWNLOAD_BYTES:
            raise ValueError("cached corpus download exceeds byte budget")
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    tmp = target.with_suffix(target.suffix + ".tmp")
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            with tmp.open("wb") as handle:
                total = 0
                while chunk := response.read(1024 * 1024):
                    total += len(chunk)
                    if total > MAX_DOWNLOAD_BYTES:
                        raise ValueError("corpus download exceeds byte budget")
                    handle.write(chunk)
        tmp.replace(target)
    finally:
        tmp.unlink(missing_ok=True)


def safe_extract_tar(source: Path, target: Path) -> bool:
    # Bound the entire decoded stream before tarfile sees a PAX/GNU header: its
    # hidden metadata is consumed before the iterator yields a regular member.
    with source.open("rb") as raw:
        magic = raw.read(6)
    opener = (gzip.open if magic.startswith(b"\x1f\x8b") else
              bz2.open if magic.startswith(b"BZh") else
              lzma.open if magic.startswith(b"\xfd7zXZ\x00") else open)
    with tempfile.TemporaryFile() as decoded, opener(source, "rb") as stream:
        total = 0
        while chunk := stream.read(min(1024 * 1024, MAX_SOURCE_BYTES - total + 1)):
            total += len(chunk)
            if total > MAX_SOURCE_BYTES:
                raise ValueError("decoded source archive exceeds extraction budget")
            decoded.write(chunk)
        decoded.seek(0)
        return extract_decoded_tar(decoded, target)


def extract_decoded_tar(decoded, target: Path) -> bool:
    try:
        with tarfile.open(fileobj=decoded, mode="r:") as archive:
            members = []
            names = set()
            total = 0
            target_root = target.resolve()
            for member in archive:
                path = PurePosixPath(member.name)
                if path.is_absolute() or ".." in path.parts or "\\" in member.name:
                    raise ValueError(f"unsafe tar path: {member.name}")
                if not (member.isfile() or member.isdir()):
                    raise ValueError(f"unsupported tar entry: {member.name}")
                if path in names and not member.isdir():
                    raise ValueError(f"duplicate tar file: {member.name}")
                names.add(path)
                total += member.size
                if (len(members) >= MAX_SOURCE_MEMBERS or member.size < 0
                        or member.size > MAX_SOURCE_FILE_BYTES or total > MAX_SOURCE_BYTES):
                    raise ValueError("source archive exceeds extraction budget")
                member_path = (target / member.name).resolve()
                if target_root != member_path and target_root not in member_path.parents:
                    raise ValueError(f"tar member escapes output directory: {member.name}")
                members.append(member)
            # Validate the whole archive before writing; never materialize links,
            # devices, executable permissions, or archive ownership metadata.
            for member in members:
                destination = target / member.name
                if member.isdir():
                    destination.mkdir(parents=True, exist_ok=True)
                else:
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    with archive.extractfile(member) as source_file, destination.open("wb") as output:
                        shutil.copyfileobj(source_file, output)
            return True
    except tarfile.TarError:
        return False


def extract_source(source: Path, target: Path, toplevel: str, force: bool) -> None:
    if target.exists() and any(target.iterdir()) and not force:
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    # A rejected or interrupted archive must never become a reusable project.
    with tempfile.TemporaryDirectory(prefix=".latexd-source-", dir=target.parent) as staging:
        prepared = Path(staging) / "project"
        prepared.mkdir()
        if not safe_extract_tar(source, prepared):
            try:
                with gzip.open(source, "rb") as compressed:
                    payload = compressed.read(MAX_SOURCE_FILE_BYTES + 1)
            except OSError:
                with source.open("rb") as raw:
                    payload = raw.read(MAX_SOURCE_FILE_BYTES + 1)
            if len(payload) > MAX_SOURCE_FILE_BYTES:
                raise ValueError("source file exceeds extraction budget")
            output_name = Path(toplevel).name or "main.tex"
            (prepared / output_name).write_bytes(payload)
        if target.exists():
            shutil.rmtree(target)
        prepared.replace(target)


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
