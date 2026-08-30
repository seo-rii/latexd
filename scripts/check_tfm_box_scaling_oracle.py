#!/usr/bin/env python3
"""Check TeX82 fix-word box scaling at every normalization boundary."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).parents[1]
BASE_TFM = ROOT / "crates/tex-fonts/assets/classic/tfm/cmr10.tfm"
EXPECTED_FIXTURE = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-box-scaling-oracle-v1.json"
)
REVIEWED_FIXTURE_SHA256 = (
    "287f3c33038b05279239f0836af5e03a306f4589d41127eb3aec2af88f051eb4"
)
COMPATIBILITY_SOURCE = {
    "url": "https://tug.ctan.org/systems/knuth/dist/tex/tex.web",
    "sha256": "c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324",
    "box_scaling_lines": [11108, 11148],
    "box_scaling_sha256": "fe78fd68fc804b9a567500e5c403865bf9e945b0e4e930f9de6bdc5813a68462",
}

CASE_SIZES_SP = {
    "size_1": 1,
    "size_2": 2,
    "size_15": 15,
    "size_16": 16,
    "size_17": 17,
    "size_65535": 65_535,
    "size_65536": 65_536,
    "size_65537": 65_537,
    "size_8388607": 8_388_607,
    "size_8388608": 8_388_608,
    "size_8388609": 8_388_609,
    "size_16777215": 16_777_215,
    "size_16777216": 16_777_216,
    "size_16777217": 16_777_217,
    "size_33554431": 33_554_431,
    "size_33554432": 33_554_432,
    "size_33554433": 33_554_433,
    "size_67108863": 67_108_863,
    "size_67108864": 67_108_864,
    "size_67108865": 67_108_865,
    "size_134217727": 134_217_727,
}

FIX_WORD_CASES = {
    "zero": bytes.fromhex("00 00 00 00"),
    "least_positive": bytes.fromhex("00 00 00 01"),
    "sub_byte_carry": bytes.fromhex("00 00 00 ff"),
    "byte_carry": bytes.fromhex("00 00 01 00"),
    "below_one": bytes.fromhex("00 0f ff ff"),
    "one": bytes.fromhex("00 10 00 00"),
    "max_positive": bytes.fromhex("00 ff ff ff"),
    "least_negative": bytes.fromhex("ff ff ff ff"),
    "negative_one": bytes.fromhex("ff f0 00 00"),
    "negative_sixteen": bytes.fromhex("ff 00 00 00"),
}

OBSERVATION_PATTERN = re.compile(
    r"LATEXD-TFMBOX:([A-Za-z0-9_]+)=(-?[0-9]+)"
)
ERROR_PATTERN = re.compile(r"^! (.+)\.$", re.MULTILINE)


def build_mutated_tfm(base: bytes) -> bytes:
    if len(base) < 24:
        raise ValueError("base TFM is shorter than its size fields")
    counts = [
        int.from_bytes(base[offset : offset + 2], "big")
        for offset in range(0, 24, 2)
    ]
    _, lh, bc, ec, nw, nh, nd, ni, nl, _, _, _ = counts
    if (bc, ec, nw, nh, nd, ni, nl) != (0, 127, 36, 16, 10, 5, 88):
        raise ValueError("box oracle requires the reviewed cmr10 table geometry")

    mutated = bytearray(base)
    counts[6:9] = [11, 11, 81]
    for index, count in enumerate(counts):
        mutated[index * 2 : index * 2 + 2] = count.to_bytes(2, "big")

    character_start = 4 * (6 + lh)
    character_count = ec - bc + 1
    width_start = character_start + 4 * character_count
    height_start = width_start + 4 * nw
    depth_start = height_start + 4 * nh
    italic_start = depth_start + 4 * counts[6]

    for character in range(character_count):
        mutated[character_start + 4 * character + 2] &= 0xFC
    for table_start in (width_start, height_start, depth_start, italic_start):
        mutated[table_start : table_start + 4] = bytes(4)

    for character, word in enumerate(FIX_WORD_CASES.values()):
        metric_index = character + 1
        record = character_start + 4 * (character - bc)
        mutated[record : record + 4] = bytes(
            [
                metric_index,
                metric_index << 4 | metric_index,
                metric_index << 2,
                0,
            ]
        )
        for table_start in (width_start, height_start, depth_start, italic_start):
            offset = table_start + 4 * metric_index
            mutated[offset : offset + 4] = word
    return bytes(mutated)


def build_probe_source(size_sp: int) -> str:
    lines = [
        r"\catcode123=1",
        r"\catcode125=2",
        rf"\font\probe=latexdprobe at {size_sp}sp",
    ]
    for character, case_id in enumerate(FIX_WORD_CASES):
        lines.extend(
            [
                rf"\setbox0=\hbox{{\probe\char{character}}}",
                rf"\message{{^^JLATEXD-TFMBOX:{case_id}_width=\number\wd0}}",
                rf"\message{{^^JLATEXD-TFMBOX:{case_id}_height=\number\ht0}}",
                rf"\message{{^^JLATEXD-TFMBOX:{case_id}_depth=\number\dp0}}",
                (
                    rf"\setbox2=\hbox{{\probe\char{character}\/{{"
                    rf"\message{{^^JLATEXD-TFMBOX:{case_id}_italic=\number\lastkern}}}}}}"
                ),
            ]
        )
    lines.append(r"\end")
    return "\n" + "\n".join(lines) + "\n"


def parse_observations(output: str) -> dict[str, int]:
    observations = {}
    for name, raw_value in OBSERVATION_PATTERN.findall(output):
        if name in observations:
            raise ValueError(f"duplicate TFM box observation: {name}")
        observations[name] = int(raw_value)
    return observations


def run_oracle(engine: str) -> dict[str, dict[str, object]]:
    mutated = build_mutated_tfm(BASE_TFM.read_bytes())
    environment = os.environ.copy()
    environment.update({"LC_ALL": "C.UTF-8", "TZ": "UTC"})
    results = {}
    for case_id, size_sp in CASE_SIZES_SP.items():
        source = build_probe_source(size_sp)
        with tempfile.TemporaryDirectory(prefix="latexd-tfm-box-oracle-") as temp:
            root = Path(temp)
            (root / "latexdprobe.tfm").write_bytes(mutated)
            (root / "probe.tex").write_text(source, encoding="utf-8")
            case_environment = environment.copy()
            case_environment["TEXFONTS"] = f"{root}{os.pathsep}"
            completed = subprocess.run(
                [engine, "-ini", "-interaction=nonstopmode", "probe.tex"],
                cwd=root,
                env=case_environment,
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
            )
        results[case_id] = {
            "diagnostics": ERROR_PATTERN.findall(completed.stdout),
            "exit_status": completed.returncode,
            "mutated_tfm_sha256": hashlib.sha256(mutated).hexdigest(),
            "observations": parse_observations(completed.stdout),
            "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
        }
    return results


def validate_results(
    results: dict[str, dict[str, object]],
    fixture: dict[str, object],
    *,
    base_tfm: bytes | None = None,
) -> list[str]:
    expected = fixture.get("case_results")
    if not isinstance(expected, dict):
        raise ValueError("TFM box scaling fixture case_results must be an object")
    violations = []
    base_tfm = BASE_TFM.read_bytes() if base_tfm is None else base_tfm
    actual_contract = {
        "format": "latexd.tfm-box-scaling-oracle",
        "schema_version": 1,
        "compatibility_target": "TeX82 via pdfTeX INITEX",
        "compatibility_source": COMPATIBILITY_SOURCE,
        "base_tfm_sha256": hashlib.sha256(base_tfm).hexdigest(),
        "case_sizes_sp": CASE_SIZES_SP,
        "fix_word_cases": {
            case_id: word.hex() for case_id, word in FIX_WORD_CASES.items()
        },
        "native_observation_projection": {
            "width": "exact_scaled_sp",
            "height": "max_zero_exact_scaled_sp",
            "depth": "max_zero_exact_scaled_sp",
            "italic": "exact_scaled_sp",
        },
    }
    for field, actual in actual_contract.items():
        if fixture.get(field) != actual:
            label = "base TFM SHA-256" if field == "base_tfm_sha256" else field
            violations.append(
                f"{label} mismatch: expected {fixture.get(field)!r}, observed {actual!r}"
            )
    for case_id, expected_result in expected.items():
        actual = results.get(case_id)
        if actual != expected_result:
            violations.append(
                f"{case_id} mismatch: expected {expected_result!r}, observed {actual!r}"
            )
    unexpected = set(results).difference(expected)
    if unexpected:
        violations.append(f"unexpected cases: {sorted(unexpected)!r}")
    return violations


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", default="pdftex")
    parser.add_argument(
        "--report",
        type=Path,
        default=Path("target/tfm-box-scaling-oracle.json"),
    )
    parser.add_argument("--write-fixture", action="store_true")
    args = parser.parse_args(argv)

    if not args.write_fixture:
        fixture_sha256 = hashlib.sha256(EXPECTED_FIXTURE.read_bytes()).hexdigest()
        if fixture_sha256 != REVIEWED_FIXTURE_SHA256:
            print(
                "TeX82 TFM box scaling fixture differs from the reviewed v1 "
                f"fixture: {fixture_sha256}"
            )
            return 1

    results = run_oracle(args.engine)
    fixture = {
        "format": "latexd.tfm-box-scaling-oracle",
        "schema_version": 1,
        "compatibility_target": "TeX82 via pdfTeX INITEX",
        "compatibility_source": COMPATIBILITY_SOURCE,
        "base_tfm_sha256": hashlib.sha256(BASE_TFM.read_bytes()).hexdigest(),
        "case_sizes_sp": CASE_SIZES_SP,
        "fix_word_cases": {
            case_id: word.hex() for case_id, word in FIX_WORD_CASES.items()
        },
        "native_observation_projection": {
            "width": "exact_scaled_sp",
            "height": "max_zero_exact_scaled_sp",
            "depth": "max_zero_exact_scaled_sp",
            "italic": "exact_scaled_sp",
        },
        "case_results": results,
    }
    if args.write_fixture:
        EXPECTED_FIXTURE.parent.mkdir(parents=True, exist_ok=True)
        EXPECTED_FIXTURE.write_text(
            json.dumps(fixture, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        return 0

    expected = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))
    violations = validate_results(results, expected)
    if violations:
        print("TeX82 TFM box scaling oracle failed:")
        for violation in violations:
            print(f"- {violation}")
        return 1

    engine_path = shutil.which(args.engine)
    if engine_path is None:
        raise RuntimeError(f"TeX oracle engine not found: {args.engine}")
    engine_path = str(Path(engine_path).resolve())
    engine_version = subprocess.run(
        [engine_path, "--version"],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout.strip()
    report = expected | {
        "engine": {
            "path": engine_path,
            "sha256": hashlib.sha256(Path(engine_path).read_bytes()).hexdigest(),
            "version": engine_version,
        },
        "invocation": [args.engine, "-ini", "-interaction=nonstopmode"],
        "environment": {"locale": "C.UTF-8", "timezone": "UTC"},
        "base_tfm": {
            "repository_path": str(BASE_TFM.relative_to(ROOT)),
            "sha256": hashlib.sha256(BASE_TFM.read_bytes()).hexdigest(),
        },
        "expected_processes": len(CASE_SIZES_SP),
        "observed_processes": len(results),
        "probe_sources": {
            case_id: build_probe_source(size_sp)
            for case_id, size_sp in CASE_SIZES_SP.items()
        },
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        "TeX82 TFM box scaling oracle passed: "
        f"sizes={len(CASE_SIZES_SP)}, fix_words={len(FIX_WORD_CASES)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
