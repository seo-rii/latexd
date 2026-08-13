#!/usr/bin/env python3
"""Check the TeX82 mathcode/delcode behavior used by the V3 migration."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import tempfile
from pathlib import Path


EXPECTED_OBSERVATIONS = {
    "math_A_default": 28993,
    "math_a_default": 29025,
    "math_0_default": 28720,
    "math_plus_default": 43,
    "del_A_default": -1,
    "math_A_local": 123,
    "del_A_local": 456,
    "math_A_restored": 28993,
    "del_A_restored": -1,
    "math_A_global": 321,
    "del_A_global": 654,
    "math_A_negative_globaldefs_local": 777,
    "del_A_negative_globaldefs_local": 888,
    "math_A_negative_globaldefs_restored": 321,
    "del_A_negative_globaldefs_restored": 654,
    "math_A_positive_globaldefs": 111,
    "del_A_positive_globaldefs": 222,
    "mathcode_max": 32767,
    "mathcode_active": 32768,
    "delcode_min": -1,
    "delcode_negative_two": -2,
    "delcode_integer_min": -2147483647,
    "delcode_max": 16777215,
    "mathchardef_max": 32767,
    "mathcode_octal": 83,
    "mathcode_hexadecimal": 4660,
    "mathcode_backtick_lhs": 234,
    "mathcode_optional_equals": 235,
    "delcode_octal": 83,
    "delcode_hexadecimal": 4660,
    "delcode_backtick_lhs": 234,
    "delcode_optional_equals": 235,
    "number_mathcode": 83,
    "number_delcode": 83,
}
for _character in range(256):
    _mathcode = _character
    if ord("0") <= _character <= ord("9"):
        _mathcode += 0x7000
    elif ord("A") <= _character <= ord("Z") or ord("a") <= _character <= ord("z"):
        _mathcode += 0x7100
    EXPECTED_OBSERVATIONS[f"math_default_{_character:03d}"] = _mathcode
    EXPECTED_OBSERVATIONS[f"del_default_{_character:03d}"] = (
        0 if _character == ord(".") else -1
    )

_DEFAULT_TABLE_SOURCE = "".join(
    f"\\message{{^^JLATEXD-ORACLE:math_default_{character:03d}=\\the\\mathcode{character}}}\n"
    f"\\message{{^^JLATEXD-ORACLE:del_default_{character:03d}=\\the\\delcode{character}}}\n"
    for character in range(256)
)

_ORACLE_BEHAVIOR_SOURCE = r"""
\catcode123=1
\catcode125=2
\message{LATEXD-ORACLE:math_A_default=\the\mathcode65}
\message{LATEXD-ORACLE:math_a_default=\the\mathcode97}
\message{LATEXD-ORACLE:math_0_default=\the\mathcode48}
\message{LATEXD-ORACLE:math_plus_default=\the\mathcode43}
\message{LATEXD-ORACLE:del_A_default=\the\delcode65}
\begingroup
\mathcode65=123
\delcode65=456
\message{LATEXD-ORACLE:math_A_local=\the\mathcode65}
\message{LATEXD-ORACLE:del_A_local=\the\delcode65}
\endgroup
\message{LATEXD-ORACLE:math_A_restored=\the\mathcode65}
\message{LATEXD-ORACLE:del_A_restored=\the\delcode65}
\begingroup
\global\mathcode65=321
\global\delcode65=654
\endgroup
\message{LATEXD-ORACLE:math_A_global=\the\mathcode65}
\message{LATEXD-ORACLE:del_A_global=\the\delcode65}
\globaldefs=-1
\begingroup
\global\mathcode65=777
\global\delcode65=888
\message{LATEXD-ORACLE:math_A_negative_globaldefs_local=\the\mathcode65}
\message{LATEXD-ORACLE:del_A_negative_globaldefs_local=\the\delcode65}
\endgroup
\message{LATEXD-ORACLE:math_A_negative_globaldefs_restored=\the\mathcode65}
\message{LATEXD-ORACLE:del_A_negative_globaldefs_restored=\the\delcode65}
\globaldefs=1
\begingroup
\mathcode65=111
\delcode65=222
\endgroup
\globaldefs=0
\message{LATEXD-ORACLE:math_A_positive_globaldefs=\the\mathcode65}
\message{LATEXD-ORACLE:del_A_positive_globaldefs=\the\delcode65}
\mathcode66=32767
\message{LATEXD-ORACLE:mathcode_max=\the\mathcode66}
\mathcode66=32768
\message{LATEXD-ORACLE:mathcode_active=\the\mathcode66}
\delcode66=-1
\message{LATEXD-ORACLE:delcode_min=\the\delcode66}
\delcode66=-2
\message{LATEXD-ORACLE:delcode_negative_two=\the\delcode66}
\delcode66=-2147483647
\message{LATEXD-ORACLE:delcode_integer_min=\the\delcode66}
\delcode66=16777215
\message{LATEXD-ORACLE:delcode_max=\the\delcode66}
\mathchardef\latexdmathchar=32767
\message{LATEXD-ORACLE:mathchardef_max=\the\latexdmathchar}
\mathcode67='123
\message{LATEXD-ORACLE:mathcode_octal=\the\mathcode67}
\mathcode68="1234
\message{LATEXD-ORACLE:mathcode_hexadecimal=\the\mathcode68}
\mathcode`E=234
\message{LATEXD-ORACLE:mathcode_backtick_lhs=\the\mathcode69}
\mathcode70 235
\message{LATEXD-ORACLE:mathcode_optional_equals=\the\mathcode70}
\delcode67='123
\message{LATEXD-ORACLE:delcode_octal=\the\delcode67}
\delcode68="1234
\message{LATEXD-ORACLE:delcode_hexadecimal=\the\delcode68}
\delcode`E=234
\message{LATEXD-ORACLE:delcode_backtick_lhs=\the\delcode69}
\delcode70 235
\message{LATEXD-ORACLE:delcode_optional_equals=\the\delcode70}
\message{LATEXD-ORACLE:number_mathcode=\number\mathcode67}
\message{LATEXD-ORACLE:number_delcode=\number\delcode67}
\end
"""
ORACLE_SOURCE = _ORACLE_BEHAVIOR_SOURCE.replace(
    "\\catcode125=2\n", "\\catcode125=2\n" + _DEFAULT_TABLE_SOURCE, 1
)

OBSERVATION_PATTERN = re.compile(r"LATEXD-ORACLE:([A-Za-z0-9_]+)=(-?[0-9]+)")
ERROR_PATTERN = re.compile(r"^! (.+)\.$", re.MULTILINE)

REJECTION_CASES = {
    "mathcode_character_too_large": {
        "probe": r"\mathcode0=42\mathcode65=123\mathcode256=456",
        "query": r"\message{LATEXD-ORACLE:mathcode_zero_after_bad_character=\the\mathcode0}\message{LATEXD-ORACLE:mathcode_A_after_bad_character=\the\mathcode65}",
        "diagnostics": ["Bad character code (256)"],
        "observations": {
            "mathcode_zero_after_bad_character": 456,
            "mathcode_A_after_bad_character": 123,
        },
    },
    "mathcode_character_negative": {
        "probe": r"\mathcode0=42\mathcode-1=456",
        "query": r"\message{LATEXD-ORACLE:mathcode_zero_after_negative_character=\the\mathcode0}",
        "diagnostics": ["Bad character code (-1)"],
        "observations": {"mathcode_zero_after_negative_character": 456},
    },
    "delcode_character_too_large": {
        "probe": r"\delcode0=42\delcode65=123\delcode256=456",
        "query": r"\message{LATEXD-ORACLE:delcode_zero_after_bad_character=\the\delcode0}\message{LATEXD-ORACLE:delcode_A_after_bad_character=\the\delcode65}",
        "diagnostics": ["Bad character code (256)"],
        "observations": {
            "delcode_zero_after_bad_character": 456,
            "delcode_A_after_bad_character": 123,
        },
    },
    "delcode_character_negative": {
        "probe": r"\delcode0=42\delcode-1=456",
        "query": r"\message{LATEXD-ORACLE:delcode_zero_after_negative_character=\the\delcode0}",
        "diagnostics": ["Bad character code (-1)"],
        "observations": {"delcode_zero_after_negative_character": 456},
    },
    "mathcode_negative": {
        "probe": r"\mathcode65=123\mathcode65=-1",
        "query": r"\message{LATEXD-ORACLE:mathcode_after_negative=\the\mathcode65}",
        "diagnostics": ["Invalid code (-1), should be in the range 0..32768"],
        "observations": {"mathcode_after_negative": 0},
    },
    "mathcode_too_large": {
        "probe": r"\mathcode65=123\mathcode65=32769",
        "query": r"\message{LATEXD-ORACLE:mathcode_after_too_large=\the\mathcode65}",
        "diagnostics": [
            "Invalid code (32769), should be in the range 0..32768"
        ],
        "observations": {"mathcode_after_too_large": 0},
    },
    "delcode_too_large": {
        "probe": r"\delcode65=123\delcode65=16777216",
        "query": r"\message{LATEXD-ORACLE:delcode_after_too_large=\the\delcode65}",
        "diagnostics": ["Invalid code (16777216), should be at most 16777215"],
        "observations": {"delcode_after_too_large": 0},
    },
    "mathchar_active": {
        "probe": "\\catcode36=3\n$\\mathchar32768$",
        "query": "",
        "diagnostics": [
            "Bad mathchar (32768)",
            "Math formula deleted: Insufficient symbol fonts",
        ],
        "observations": {},
    },
    "mathchardef_active": {
        "probe": r"\mathchardef\latexdmathchar=32768",
        "query": r"\message{LATEXD-ORACLE:mathchardef_after_active=\the\latexdmathchar}",
        "diagnostics": ["Bad mathchar (32768)"],
        "observations": {"mathchardef_after_active": 0},
    },
}

EXPECTED_REJECTIONS = {}
for _name, _case in REJECTION_CASES.items():
    _source = (
        "\\catcode123=1\\catcode125=2"
        f"{_case['probe']}{_case['query']}\\end\n"
    )
    EXPECTED_REJECTIONS[_name] = {
        "diagnostics": _case["diagnostics"],
        "exit_status": 1,
        "observations": _case["observations"],
        "source": _source,
        "source_sha256": hashlib.sha256(_source.encode()).hexdigest(),
    }


def parse_observations(output: str) -> dict[str, int]:
    observations: dict[str, int] = {}
    for name, raw_value in OBSERVATION_PATTERN.findall(output):
        if name in observations:
            raise ValueError(f"duplicate TeX oracle observation: {name}")
        observations[name] = int(raw_value)
    return observations


def validate_observations(observations: dict[str, int]) -> list[str]:
    violations = []
    for name, expected in EXPECTED_OBSERVATIONS.items():
        actual = observations.get(name)
        if actual != expected:
            violations.append(
                f"{name} mismatch: expected {expected}, observed {actual!r}"
            )
    unexpected = set(observations).difference(EXPECTED_OBSERVATIONS)
    if unexpected:
        violations.append(f"unexpected observations: {sorted(unexpected)!r}")
    return violations


def validate_rejections(rejections: dict[str, dict[str, object]]) -> list[str]:
    violations = []
    for name, expected in EXPECTED_REJECTIONS.items():
        actual = rejections.get(name)
        if actual != expected:
            violations.append(
                f"{name} mismatch: expected {expected!r}, observed {actual!r}"
            )
    unexpected = set(rejections).difference(EXPECTED_REJECTIONS)
    if unexpected:
        violations.append(f"unexpected rejections: {sorted(unexpected)!r}")
    return violations


def _run_tex_source(engine: str, source: str) -> subprocess.CompletedProcess[str]:
    with tempfile.TemporaryDirectory(prefix="latexd-mathcode-delcode-oracle-") as temp:
        root = Path(temp)
        source_path = root / "probe.tex"
        source_path.write_text(source, encoding="utf-8")
        return subprocess.run(
            [engine, "-ini", "-interaction=nonstopmode", source_path.name],
            cwd=root,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )


def run_oracle(engine: str) -> dict[str, object]:
    completed = _run_tex_source(engine, ORACLE_SOURCE)
    if completed.returncode != 0:
        raise RuntimeError(
            f"TeX oracle exited with status {completed.returncode}:\n{completed.stdout}"
        )
    return {
        "exit_status": completed.returncode,
        "observations": parse_observations(completed.stdout),
        "source": ORACLE_SOURCE,
        "source_sha256": hashlib.sha256(ORACLE_SOURCE.encode()).hexdigest(),
    }


def run_rejection_oracle(engine: str) -> dict[str, dict[str, object]]:
    rejections = {}
    for name, case in REJECTION_CASES.items():
        source = (
            "\\catcode123=1\\catcode125=2"
            f"{case['probe']}{case['query']}\\end\n"
        )
        completed = _run_tex_source(engine, source)
        errors = ERROR_PATTERN.findall(completed.stdout)
        if completed.returncode == 0 or not errors:
            raise RuntimeError(
                f"TeX rejection oracle `{name}` did not report an error:\n"
                f"{completed.stdout}"
            )
        rejections[name] = {
            "diagnostics": errors,
            "exit_status": completed.returncode,
            "observations": parse_observations(completed.stdout),
            "source": source,
            "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
        }
    return rejections


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", default="pdftex")
    parser.add_argument(
        "--report",
        type=Path,
        default=Path("target/mathcode-delcode-oracle.json"),
    )
    args = parser.parse_args(argv)

    valid_probe = run_oracle(args.engine)
    observations = valid_probe["observations"]
    assert isinstance(observations, dict)
    violations = validate_observations(observations)
    rejections = run_rejection_oracle(args.engine)
    violations.extend(validate_rejections(rejections))
    if violations:
        print("TeX82 mathcode/delcode oracle failed:")
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
        "format": "latexd.mathcode-delcode-oracle",
        "schema_version": 1,
        "compatibility_target": "TeX82 via pdfTeX INITEX",
        "engine": {
            "path": engine_path,
            "sha256": hashlib.sha256(Path(engine_path).read_bytes()).hexdigest(),
            "version": engine_version,
        },
        "valid_probe": valid_probe,
        "rejection_probes": rejections,
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"TeX82 mathcode/delcode oracle passed ({args.engine} -ini); "
        f"report: {args.report}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
