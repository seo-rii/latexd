#!/usr/bin/env python3
"""Characterize TeX82 TFM validity boundaries needed before DP1 font loading."""

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


REPOSITORY = Path(__file__).resolve().parents[1]
EXPECTED_FIXTURE = (
    REPOSITORY
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validity-oracle-v1.json"
)

TEX82_READ_FONT_INFO_SOURCE = {
    "url": "https://tug.ctan.org/systems/knuth/dist/tex/tex.web",
    "sha256": "c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324",
    "loader_section_lines": [10870, 11210],
    "loader_section_sha256": "57f665ae4cc87c721d444fdde0a1817f194f44bab18388c42a1d26d830c6ddc8",
}

CMR10_TFM = "crates/tex-fonts/assets/classic/tfm/cmr10.tfm"
CMEX10_TFM = "crates/tex-fonts/assets/classic/tfm/cmex10.tfm"

CASE_SPECS = {
    "valid_cmr10": {
        "base_tfm": CMR10_TFM,
        "description": "unmodified cmr10 control",
    },
    "size_field_high_bit": {
        "base_tfm": CMR10_TFM,
        "description": "a TFM size halfword uses the forbidden high bit",
    },
    "invalid_character_range": {
        "base_tfm": CMR10_TFM,
        "description": "bc is greater than ec plus one",
    },
    "aggregate_length_mismatch": {
        "base_tfm": CMR10_TFM,
        "description": "lf disagrees with the aggregate table counts",
    },
    "short_np5": {
        "base_tfm": CMR10_TFM,
        "description": "fontdimen6 absent from an otherwise valid parameter table",
    },
    "short_np4": {
        "base_tfm": CMR10_TFM,
        "description": "fontdimen5 and fontdimen6 both absent",
    },
    "short_np0": {
        "base_tfm": CMR10_TFM,
        "description": "empty parameter table receives TeX82 zero defaults",
    },
    "trailing_word": {
        "base_tfm": CMR10_TFM,
        "description": "one complete word follows the declared TFM length",
    },
    "zero_width_table_consistent": {
        "base_tfm": CMR10_TFM,
        "description": "zero width-table count with a compensating table count",
    },
    "zero_height_table_consistent": {
        "base_tfm": CMR10_TFM,
        "description": "zero height-table count with a compensating table count",
    },
    "zero_depth_table_consistent": {
        "base_tfm": CMR10_TFM,
        "description": "zero depth-table count with a compensating table count",
    },
    "zero_italic_table_consistent": {
        "base_tfm": CMR10_TFM,
        "description": "zero italic-table count with a compensating table count",
    },
    "short_header": {
        "base_tfm": CMR10_TFM,
        "description": "the header contains fewer than checksum and design-size words",
    },
    "design_size_below_one_pt": {
        "base_tfm": CMR10_TFM,
        "description": "the design size is one fix-word unit below 1pt",
    },
    "invalid_character_width_index": {
        "base_tfm": CMR10_TFM,
        "description": "character info addresses width index nw",
    },
    "charlist_self_cycle": {
        "base_tfm": CMR10_TFM,
        "description": "character list points back to the same character",
    },
    "invalid_width_fix_word_sign": {
        "base_tfm": CMR10_TFM,
        "description": "unselected width fix word has a forbidden sign byte",
    },
    "nonzero_width_zero": {
        "base_tfm": CMR10_TFM,
        "description": "width table entry zero scales to a nonzero dimension",
    },
    "invalid_fontdimen2": {
        "base_tfm": CMR10_TFM,
        "description": "unselected fontdimen2 has a forbidden fix-word sign byte",
    },
    "invalid_fontdimen5": {
        "base_tfm": CMR10_TFM,
        "description": "selected fontdimen5 has a forbidden fix-word sign byte",
    },
    "invalid_ligkern": {
        "base_tfm": CMR10_TFM,
        "description": "ligature/kern instruction jumps outside its table",
    },
    "invalid_kern_fix_word": {
        "base_tfm": CMR10_TFM,
        "description": "kern fix word has a forbidden sign byte",
    },
    "invalid_extensible": {
        "base_tfm": CMEX10_TFM,
        "description": "extensible recipe references a character outside the range",
    },
    "signed_slant_parameter": {
        "base_tfm": CMR10_TFM,
        "description": "fontdimen1 uses a sign byte that is valid for the pure-number slant",
    },
    "premature_eof": {
        "base_tfm": CMR10_TFM,
        "description": "the final byte declared by lf is missing",
    },
}

OBSERVATION_PATTERN = re.compile(
    r"LATEXD-TFMV:([A-Za-z0-9_]+)=([A-Za-z0-9_.:/+-]+)"
)

PROBE_SOURCE = r"""
\catcode123=1
\catcode125=2
\font\baseline=cmr10
\let\probe=\baseline
\font\probe=latexdprobe
\probe
\dimen0=\fontdimen6\font
\dimen2=\fontdimen5\font
\message{^^JLATEXD-TFMV:font=\fontname\font}
\message{^^JLATEXD-TFMV:quad=\number\dimen0}
\message{^^JLATEXD-TFMV:xheight=\number\dimen2}
\message{^^JLATEXD-TFMV:sentinel=1}
\end
"""


def _counts(tfm: bytes) -> list[int]:
    if len(tfm) < 24:
        raise ValueError("base TFM is shorter than its size fields")
    return [
        int.from_bytes(tfm[offset : offset + 2], "big")
        for offset in range(0, 24, 2)
    ]


def _table_offsets(tfm: bytes) -> dict[str, int]:
    _, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, _ = _counts(tfm)
    character_count = ec - bc + 1 if bc <= ec else 0
    return {
        "character": 4 * (6 + lh),
        "width": 4 * (6 + lh + character_count),
        "ligkern": 4 * (6 + lh + character_count + nw + nh + nd + ni),
        "kern": 4 * (6 + lh + character_count + nw + nh + nd + ni + nl),
        "extensible": 4
        * (6 + lh + character_count + nw + nh + nd + ni + nl + nk),
        "parameter": 4
        * (6 + lh + character_count + nw + nh + nd + ni + nl + nk + ne),
    }


def _short_parameter_table(tfm: bytes, parameter_count: int) -> bytes:
    counts = _counts(tfm)
    original_count = counts[11]
    if original_count != 7 or not 0 <= parameter_count < original_count:
        raise ValueError("short-parameter mutations require a seven-parameter base")
    removed_words = original_count - parameter_count
    mutated = bytearray(tfm[: len(tfm) - removed_words * 4])
    mutated[0:2] = (counts[0] - removed_words).to_bytes(2, "big")
    mutated[22:24] = parameter_count.to_bytes(2, "big")
    return bytes(mutated)


def mutate_tfm(case_id: str, base_tfm: bytes) -> bytes:
    if case_id == "valid_cmr10":
        return bytes(base_tfm)
    if case_id == "short_np5":
        return _short_parameter_table(base_tfm, 5)
    if case_id == "short_np4":
        return _short_parameter_table(base_tfm, 4)
    if case_id == "short_np0":
        return _short_parameter_table(base_tfm, 0)
    if case_id == "trailing_word":
        return bytes(base_tfm) + bytes(4)
    if case_id == "premature_eof":
        return bytes(base_tfm[:-1])

    mutated = bytearray(base_tfm)
    offsets = _table_offsets(base_tfm)
    if case_id == "size_field_high_bit":
        mutated[8] = 128
    elif case_id == "invalid_character_range":
        mutated[4:6] = (2).to_bytes(2, "big")
        mutated[6:8] = (0).to_bytes(2, "big")
    elif case_id == "aggregate_length_mismatch":
        mutated[0:2] = (_counts(base_tfm)[0] + 1).to_bytes(2, "big")
    elif case_id == "zero_width_table_consistent":
        counts = _counts(base_tfm)
        mutated[8:10] = (0).to_bytes(2, "big")
        mutated[16:18] = (counts[8] + counts[4]).to_bytes(2, "big")
    elif case_id == "zero_height_table_consistent":
        counts = _counts(base_tfm)
        mutated[10:12] = (0).to_bytes(2, "big")
        mutated[18:20] = (counts[9] + counts[5]).to_bytes(2, "big")
    elif case_id == "zero_depth_table_consistent":
        counts = _counts(base_tfm)
        mutated[12:14] = (0).to_bytes(2, "big")
        mutated[18:20] = (counts[9] + counts[6]).to_bytes(2, "big")
    elif case_id == "zero_italic_table_consistent":
        counts = _counts(base_tfm)
        mutated[14:16] = (0).to_bytes(2, "big")
        mutated[18:20] = (counts[9] + counts[7]).to_bytes(2, "big")
    elif case_id == "short_header":
        counts = _counts(base_tfm)
        mutated[2:4] = (1).to_bytes(2, "big")
        mutated[18:20] = (counts[9] + counts[1] - 1).to_bytes(2, "big")
    elif case_id == "design_size_below_one_pt":
        mutated[28:32] = ((1 << 20) - 1).to_bytes(4, "big")
    elif case_id == "invalid_character_width_index":
        mutated[offsets["character"]] = _counts(base_tfm)[4]
    elif case_id == "charlist_self_cycle":
        mutated[offsets["character"] + 2] = 2
        mutated[offsets["character"] + 3] = _counts(base_tfm)[2]
    elif case_id == "invalid_width_fix_word_sign":
        mutated[offsets["width"] + 4] = 1
    elif case_id == "nonzero_width_zero":
        mutated[offsets["width"] : offsets["width"] + 4] = (1 << 16).to_bytes(4, "big")
    elif case_id == "invalid_fontdimen2":
        mutated[offsets["parameter"] + 4] = 1
    elif case_id == "invalid_fontdimen5":
        mutated[offsets["parameter"] + 16] = 1
    elif case_id == "invalid_ligkern":
        mutated[offsets["ligkern"] : offsets["ligkern"] + 4] = bytes([129, 0, 1, 0])
    elif case_id == "invalid_kern_fix_word":
        mutated[offsets["kern"]] = 1
    elif case_id == "invalid_extensible":
        if _counts(base_tfm)[10] == 0:
            raise ValueError("extensible mutation requires a nonempty recipe table")
        mutated[offsets["extensible"] + 3] = 255
    elif case_id == "signed_slant_parameter":
        mutated[offsets["parameter"]] = 1
    else:
        raise ValueError(f"unknown TFM validity case: {case_id}")
    return bytes(mutated)


def parse_observations(output: str) -> dict[str, int | str]:
    observations: dict[str, int | str] = {}
    for name, raw_value in OBSERVATION_PATTERN.findall(output):
        if name in observations:
            raise ValueError(f"duplicate TFM validity observation: {name}")
        observations[name] = (
            int(raw_value) if re.fullmatch(r"[+-]?[0-9]+", raw_value) else raw_value
        )
    return observations


def parse_diagnostics(output: str) -> list[str]:
    lines = output.splitlines()
    diagnostics = []
    for index, line in enumerate(lines):
        if not line.startswith("! "):
            continue
        diagnostic = line[2:]
        if diagnostic.endswith(";") and index + 1 < len(lines):
            continuation = lines[index + 1]
            if continuation.startswith(" "):
                diagnostic = f"{diagnostic} {continuation.strip()}"
        diagnostics.append(diagnostic.removesuffix("."))
    return diagnostics


def _fixture_result(result: dict[str, object]) -> dict[str, object]:
    return {
        "diagnostics": result["diagnostics"],
        "exit_status": result["exit_status"],
        "mutated_tfm_sha256": result["mutated_tfm_sha256"],
        "observations": result["observations"],
        "source_sha256": result["source_sha256"],
    }


def _collect_case_results(
    engine: str, *, include_raw_output: bool
) -> dict[str, dict[str, object]]:
    results = {}
    process_environment = os.environ.copy()
    process_environment.update({"LC_ALL": "C.UTF-8", "TZ": "UTC"})
    source_sha256 = hashlib.sha256(PROBE_SOURCE.encode()).hexdigest()
    for case_id, spec in CASE_SPECS.items():
        base_path = REPOSITORY / spec["base_tfm"]
        mutated = mutate_tfm(case_id, base_path.read_bytes())
        with tempfile.TemporaryDirectory(prefix="latexd-tfm-validity-oracle-") as temp:
            root = Path(temp)
            (root / "latexdprobe.tfm").write_bytes(mutated)
            (root / "probe.tex").write_text(PROBE_SOURCE, encoding="utf-8")
            case_environment = process_environment.copy()
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
        result: dict[str, object] = {
            "diagnostics": parse_diagnostics(completed.stdout),
            "exit_status": completed.returncode,
            "mutated_tfm_sha256": hashlib.sha256(mutated).hexdigest(),
            "observations": parse_observations(completed.stdout),
            "source_sha256": source_sha256,
        }
        if include_raw_output:
            result["raw_output"] = completed.stdout
            result["source"] = PROBE_SOURCE
        results[case_id] = result
    return results


def run_oracle(engine: str) -> dict[str, dict[str, object]]:
    return _collect_case_results(engine, include_raw_output=False)


def validate_case_results(
    results: dict[str, dict[str, object]], fixture: dict[str, object]
) -> list[str]:
    expected = fixture["case_results"]
    if not isinstance(expected, dict):
        raise ValueError("TFM validity fixture case_results must be an object")
    violations = []
    for case_id, expected_result in expected.items():
        actual = results.get(case_id)
        semantic_actual = None if actual is None else _fixture_result(actual)
        if semantic_actual != expected_result:
            violations.append(
                f"{case_id} mismatch: expected {expected_result!r}, "
                f"observed {semantic_actual!r}"
            )
    unexpected = set(results).difference(expected)
    if unexpected:
        violations.append(f"unexpected cases: {sorted(unexpected)!r}")
    return violations


def _base_tfm_evidence() -> dict[str, dict[str, object]]:
    evidence = {}
    for relative_path in sorted({spec["base_tfm"] for spec in CASE_SPECS.values()}):
        path = (REPOSITORY / relative_path).resolve(strict=True)
        evidence[path.name] = {
            "repository_path": relative_path,
            "resolved_path": str(path),
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        }
    return evidence


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", default="pdftex")
    parser.add_argument(
        "--report",
        type=Path,
        default=Path("target/tfm-validity-oracle.json"),
    )
    args = parser.parse_args(argv)

    fixture = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))
    case_results = _collect_case_results(args.engine, include_raw_output=True)
    violations = validate_case_results(case_results, fixture)
    if violations:
        print("TeX82 TFM validity oracle failed:")
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
    report = {
        "format": fixture["format"],
        "schema_version": fixture["schema_version"],
        "compatibility_target": fixture["compatibility_target"],
        "compatibility_source": TEX82_READ_FONT_INFO_SOURCE,
        "engine": {
            "path": engine_path,
            "sha256": hashlib.sha256(Path(engine_path).read_bytes()).hexdigest(),
            "version": engine_version,
        },
        "invocation": [args.engine, "-ini", "-interaction=nonstopmode"],
        "environment": {"locale": "C.UTF-8", "timezone": "UTC"},
        "base_tfm_files": _base_tfm_evidence(),
        "expected_processes": len(CASE_SPECS),
        "observed_processes": len(case_results),
        "case_descriptions": {
            case_id: spec["description"] for case_id, spec in CASE_SPECS.items()
        },
        "normalization": {
            "diagnostics": (
                "lines beginning !; semicolon continuation joined; "
                "one trailing period removed"
            ),
            "observations": "LATEXD-TFMV:<name>=<integer-or-font-name>",
        },
        "case_results": case_results,
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"TeX82 TFM validity oracle passed ({args.engine} -ini); "
        f"report: {args.report}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
