#!/usr/bin/env python3
"""Check the next TeX82 layout-only integer-parameter bundle for M13.3."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import tempfile
from pathlib import Path


PARAMETER_DEFAULTS = {
    "adjdemerits": 0,
    "binoppenalty": 0,
    "brokenpenalty": 0,
    "clubpenalty": 0,
    "displaywidowpenalty": 0,
    "doublehyphendemerits": 0,
    "exhyphenpenalty": 0,
    "finalhyphendemerits": 0,
    "hangafter": 1,
    "hyphenpenalty": 0,
    "interlinepenalty": 0,
    "linepenalty": 0,
    "looseness": 0,
    "postdisplaypenalty": 0,
    "predisplaypenalty": 0,
    "pretolerance": 0,
    "relpenalty": 0,
    "widowpenalty": 0,
}


def _observation(name: str, expression: str) -> str:
    return f"\\message{{^^JLATEXD-LAYOUT-INT:{name}={expression}}}"


def _build_oracle_source() -> str:
    lines = [r"\catcode123=1", r"\catcode125=2"]
    for name in PARAMETER_DEFAULTS:
        parameter = f"\\{name}"
        lines.extend(
            [
                _observation(f"default_{name}", f"\\the{parameter}"),
                r"\begingroup",
                f"{parameter}=123",
                _observation(f"local_{name}", f"\\the{parameter}"),
                r"\endgroup",
                _observation(f"restored_{name}", f"\\the{parameter}"),
                f"{parameter}=2147483647",
                _observation(f"max_{name}", f"\\the{parameter}"),
                f"{parameter}=-2147483647",
                _observation(f"min_{name}", f"\\the{parameter}"),
            ]
        )

    lines.extend(
        [
            r"\pretolerance=0",
            r"\begingroup",
            r"\global\pretolerance=321",
            r"\endgroup",
            _observation("global", r"\the\pretolerance"),
            r"\globaldefs=-1",
            r"\begingroup",
            r"\global\pretolerance=777",
            _observation("negative_globaldefs_local", r"\the\pretolerance"),
            r"\endgroup",
            _observation("negative_globaldefs_restored", r"\the\pretolerance"),
            r"\globaldefs=1",
            r"\begingroup",
            r"\pretolerance=111",
            r"\endgroup",
            r"\globaldefs=0",
            _observation("positive_globaldefs", r"\the\pretolerance"),
            r"\pretolerance='123",
            _observation("octal", r"\the\pretolerance"),
            "\\pretolerance=\"1234",
            _observation("hexadecimal", r"\the\pretolerance"),
            r"\pretolerance=`A",
            _observation("character", r"\the\pretolerance"),
            r"\pretolerance=--+17",
            _observation("repeated_signs", r"\the\pretolerance"),
            r"\pretolerance=12",
            r"\advance\pretolerance by 5",
            _observation("advanced", r"\the\pretolerance"),
            r"\multiply\pretolerance by -3",
            _observation("multiplied", r"\the\pretolerance"),
            r"\divide\pretolerance by 2",
            _observation("divided", r"\the\pretolerance"),
            r"\pretolerance=2147483647",
            r"\advance\pretolerance by 1",
            _observation("advance_wraps", r"\the\pretolerance"),
            _observation("number", r"\number\pretolerance"),
            r"\ifnum\pretolerance<0",
            _observation("ifnum", "1"),
            r"\else",
            _observation("ifnum", "0"),
            r"\fi",
            rf"\def\aftermarker{{{_observation('afterassignment', '1')}}}",
            r"\afterassignment\aftermarker\pretolerance=44",
            _observation("afterassignment_value", r"\the\pretolerance"),
            r"\let\savedpretolerance=\pretolerance",
            r"\savedpretolerance=45",
            _observation("alias_value", r"\the\savedpretolerance"),
            r"\begingroup",
            r"\def\pretolerance{46}",
            _observation("explicit_redefinition", r"\pretolerance"),
            r"\endgroup",
            _observation("restored_builtin", r"\the\pretolerance"),
            r"\end",
        ]
    )
    return "\n" + "\n".join(lines) + "\n"


ORACLE_SOURCE = _build_oracle_source()

EXPECTED_OBSERVATIONS: dict[str, int] = {}
for _parameter_name, _default in PARAMETER_DEFAULTS.items():
    EXPECTED_OBSERVATIONS.update(
        {
            f"default_{_parameter_name}": _default,
            f"local_{_parameter_name}": 123,
            f"restored_{_parameter_name}": _default,
            f"max_{_parameter_name}": 2_147_483_647,
            f"min_{_parameter_name}": -2_147_483_647,
        }
    )
EXPECTED_OBSERVATIONS.update(
    {
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
    }
)

OBSERVATION_PATTERN = re.compile(
    r"LATEXD-LAYOUT-INT:([A-Za-z0-9_]+)=(-?[0-9]+)"
)
ERROR_PATTERN = re.compile(r"^! (.+)\.$", re.MULTILINE)

REJECTION_CASES = {
    "positive_number_too_big": {
        "probe": (
            rf"\def\mark{{{_observation('afterassignment', '1')}}}"
            r"\afterassignment\mark\pretolerance=2147483648"
        ),
        "diagnostics": ["Number too big"],
        "observations": {"afterassignment": 1, "value": 2_147_483_647},
    },
    "negative_number_too_big": {
        "probe": r"\pretolerance=-2147483648",
        "diagnostics": ["Number too big"],
        "observations": {"value": -2_147_483_647},
    },
    "missing_number": {
        "probe": (
            rf"\def\mark{{{_observation('afterassignment', '1')}}}"
            r"\afterassignment\mark\pretolerance=\relax"
        ),
        "diagnostics": ["Missing number, treated as zero"],
        "observations": {"afterassignment": 1, "value": 0},
    },
    "multiply_overflow": {
        "probe": r"\pretolerance=1073741824\multiply\pretolerance by2",
        "diagnostics": ["Arithmetic overflow"],
        "observations": {"value": 1_073_741_824},
    },
    "divide_by_zero": {
        "probe": r"\pretolerance=17\divide\pretolerance by0",
        "diagnostics": ["Arithmetic overflow"],
        "observations": {"value": 17},
    },
}

REJECTION_VALUE_OBSERVATION = _observation("value", r"\the\pretolerance")
EXPECTED_REJECTIONS = {}
for _rejection_name, _case in REJECTION_CASES.items():
    _source = (
        "\\catcode123=1\\catcode125=2"
        f"{_case['probe']}{REJECTION_VALUE_OBSERVATION}\\end\n"
    )
    EXPECTED_REJECTIONS[_rejection_name] = {
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
            raise ValueError(f"duplicate TeX layout-integer observation: {name}")
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
    with tempfile.TemporaryDirectory(prefix="latexd-layout-integer-oracle-") as temp:
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
            f"TeX layout-integer oracle exited with status {completed.returncode}:\n"
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
            f"{case['probe']}{REJECTION_VALUE_OBSERVATION}\\end\n"
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
        default=Path("target/layout-integer-parameters-oracle.json"),
    )
    args = parser.parse_args(argv)

    valid_probe = run_oracle(args.engine)
    observations = valid_probe["observations"]
    assert isinstance(observations, dict)
    violations = validate_observations(observations)
    rejections = run_rejection_oracle(args.engine)
    violations.extend(validate_rejections(rejections))
    if violations:
        print("TeX82 layout-integer-parameters oracle failed:")
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
        "format": "latexd.layout-integer-parameters-oracle",
        "schema_version": 1,
        "compatibility_target": "TeX82 via pdfTeX INITEX",
        "planned_source_activation_set": list(PARAMETER_DEFAULTS),
        "parameter_defaults": PARAMETER_DEFAULTS,
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
        "TeX82 layout-integer-parameters oracle passed "
        f"({args.engine} -ini); report: {args.report}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
