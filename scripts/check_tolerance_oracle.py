#!/usr/bin/env python3
"""Check the TeX82 tolerance storage/query/arithmetic contract used by M13.3."""

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
    "default": 10000,
    "local": 123,
    "restored": 10000,
    "global": 321,
    "negative_globaldefs_local": 777,
    "negative_globaldefs_restored": 321,
    "positive_globaldefs": 111,
    "octal": 83,
    "hexadecimal": 4660,
    "character": 65,
    "repeated_signs": 17,
    "advanced": 17,
    "multiplied": -51,
    "divided": -25,
    "advance_wraps": -2_147_483_648,
    "number": -2_147_483_648,
    "ifnum": 1,
    "afterassignment": 1,
    "afterassignment_value": 44,
    "alias_value": 45,
    "explicit_redefinition": 46,
    "restored_builtin": 45,
    "max": 2_147_483_647,
    "min": -2_147_483_647,
}

ORACLE_SOURCE = r"""
\catcode123=1
\catcode125=2
\message{^^JLATEXD-TOLERANCE:default=\the\tolerance}
\begingroup
\tolerance=123
\message{^^JLATEXD-TOLERANCE:local=\the\tolerance}
\endgroup
\message{^^JLATEXD-TOLERANCE:restored=\the\tolerance}
\begingroup
\global\tolerance=321
\endgroup
\message{^^JLATEXD-TOLERANCE:global=\the\tolerance}
\globaldefs=-1
\begingroup
\global\tolerance=777
\message{^^JLATEXD-TOLERANCE:negative_globaldefs_local=\the\tolerance}
\endgroup
\message{^^JLATEXD-TOLERANCE:negative_globaldefs_restored=\the\tolerance}
\globaldefs=1
\begingroup
\tolerance=111
\endgroup
\globaldefs=0
\message{^^JLATEXD-TOLERANCE:positive_globaldefs=\the\tolerance}
\tolerance '123
\message{^^JLATEXD-TOLERANCE:octal=\the\tolerance}
\tolerance="1234
\message{^^JLATEXD-TOLERANCE:hexadecimal=\the\tolerance}
\tolerance=`A
\message{^^JLATEXD-TOLERANCE:character=\the\tolerance}
\tolerance --+17
\message{^^JLATEXD-TOLERANCE:repeated_signs=\the\tolerance}
\tolerance = 12
\advance\tolerance by 5
\message{^^JLATEXD-TOLERANCE:advanced=\the\tolerance}
\multiply\tolerance by -3
\message{^^JLATEXD-TOLERANCE:multiplied=\the\tolerance}
\divide\tolerance by 2
\message{^^JLATEXD-TOLERANCE:divided=\the\tolerance}
\tolerance=2147483647
\advance\tolerance by1
\message{^^JLATEXD-TOLERANCE:advance_wraps=\the\tolerance}
\message{^^JLATEXD-TOLERANCE:number=\number\tolerance}
\ifnum\tolerance<0
\message{^^JLATEXD-TOLERANCE:ifnum=1}
\else
\message{^^JLATEXD-TOLERANCE:ifnum=0}
\fi
\def\aftermarker{\message{^^JLATEXD-TOLERANCE:afterassignment=1}}
\afterassignment\aftermarker\tolerance=44
\message{^^JLATEXD-TOLERANCE:afterassignment_value=\the\tolerance}
\let\savedtolerance=\tolerance
\savedtolerance=45
\message{^^JLATEXD-TOLERANCE:alias_value=\the\savedtolerance}
\begingroup
\def\tolerance{46}
\message{^^JLATEXD-TOLERANCE:explicit_redefinition=\tolerance}
\endgroup
\message{^^JLATEXD-TOLERANCE:restored_builtin=\the\tolerance}
\tolerance=2147483647
\message{^^JLATEXD-TOLERANCE:max=\the\tolerance}
\tolerance=-2147483647
\message{^^JLATEXD-TOLERANCE:min=\the\tolerance}
\end
"""

OBSERVATION_PATTERN = re.compile(r"LATEXD-TOLERANCE:([A-Za-z0-9_]+)=(-?[0-9]+)")
ERROR_PATTERN = re.compile(r"^! (.+)\.$", re.MULTILINE)

REJECTION_CASES = {
    "positive_number_too_big": {
        "probe": (
            r"\def\mark{\message{^^JLATEXD-TOLERANCE:afterassignment=1}}"
            r"\afterassignment\mark\tolerance=2147483648"
        ),
        "query": r"\message{^^JLATEXD-TOLERANCE:value=\the\tolerance}",
        "diagnostics": ["Number too big"],
        "observations": {"afterassignment": 1, "value": 2_147_483_647},
    },
    "negative_number_too_big": {
        "probe": r"\tolerance=-2147483648",
        "query": r"\message{^^JLATEXD-TOLERANCE:value=\the\tolerance}",
        "diagnostics": ["Number too big"],
        "observations": {"value": -2_147_483_647},
    },
    "missing_number": {
        "probe": (
            r"\def\mark{\message{^^JLATEXD-TOLERANCE:afterassignment=1}}"
            r"\afterassignment\mark\tolerance=\relax"
        ),
        "query": r"\message{^^JLATEXD-TOLERANCE:value=\the\tolerance}",
        "diagnostics": ["Missing number, treated as zero"],
        "observations": {"afterassignment": 1, "value": 0},
    },
    "multiply_overflow": {
        "probe": r"\tolerance=1073741824\multiply\tolerance by2",
        "query": r"\message{^^JLATEXD-TOLERANCE:value=\the\tolerance}",
        "diagnostics": ["Arithmetic overflow"],
        "observations": {"value": 1_073_741_824},
    },
    "divide_by_zero": {
        "probe": r"\tolerance=17\divide\tolerance by0",
        "query": r"\message{^^JLATEXD-TOLERANCE:value=\the\tolerance}",
        "diagnostics": ["Arithmetic overflow"],
        "observations": {"value": 17},
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
            raise ValueError(f"duplicate TeX tolerance observation: {name}")
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
    with tempfile.TemporaryDirectory(prefix="latexd-tolerance-oracle-") as temp:
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
            f"TeX tolerance oracle exited with status {completed.returncode}:\n"
            f"{completed.stdout}"
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
        default=Path("target/tolerance-oracle.json"),
    )
    args = parser.parse_args(argv)

    valid_probe = run_oracle(args.engine)
    observations = valid_probe["observations"]
    assert isinstance(observations, dict)
    violations = validate_observations(observations)
    rejections = run_rejection_oracle(args.engine)
    violations.extend(validate_rejections(rejections))
    if violations:
        print("TeX82 tolerance oracle failed:")
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
        "format": "latexd.tolerance-oracle",
        "schema_version": 1,
        "compatibility_target": "TeX82 via pdfTeX INITEX",
        "planned_source_activation_set": ["tolerance"],
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
        f"TeX82 tolerance oracle passed ({args.engine} -ini); report: {args.report}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
