#!/usr/bin/env python3
"""Check the TeX82 mathcode/delcode behavior used by the V3 migration."""

from __future__ import annotations

import argparse
import json
import re
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
}

EXPECTED_REJECTIONS = {
    "mathcode_character_too_large": "Bad character code (256)",
    "delcode_character_too_large": "Bad character code (256)",
    "mathcode_negative": "Invalid code (-1), should be in the range 0..32768",
    "mathcode_too_large": "Invalid code (32769), should be in the range 0..32768",
    "delcode_too_large": "Invalid code (16777216), should be at most 16777215",
    "mathchardef_active": "Bad mathchar (32768)",
}

ORACLE_SOURCE = r"""
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
\end
"""

OBSERVATION_PATTERN = re.compile(r"LATEXD-ORACLE:([A-Za-z0-9_]+)=(-?[0-9]+)")
ERROR_PATTERN = re.compile(r"^! (.+)\.$", re.MULTILINE)

REJECTION_SOURCES = {
    "mathcode_character_too_large": r"\mathcode256=1",
    "delcode_character_too_large": r"\delcode256=1",
    "mathcode_negative": r"\mathcode65=-1",
    "mathcode_too_large": r"\mathcode65=32769",
    "delcode_too_large": r"\delcode65=16777216",
    "mathchardef_active": r"\mathchardef\latexdmathchar=32768",
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


def validate_rejections(rejections: dict[str, str]) -> list[str]:
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


def run_oracle(engine: str) -> dict[str, int]:
    completed = _run_tex_source(engine, ORACLE_SOURCE)
    if completed.returncode != 0:
        raise RuntimeError(
            f"TeX oracle exited with status {completed.returncode}:\n{completed.stdout}"
        )
    return parse_observations(completed.stdout)


def run_rejection_oracle(engine: str) -> dict[str, str]:
    rejections = {}
    for name, probe in REJECTION_SOURCES.items():
        source = f"\\catcode123=1\\catcode125=2{probe}\\end\n"
        completed = _run_tex_source(engine, source)
        errors = ERROR_PATTERN.findall(completed.stdout)
        if completed.returncode == 0 or len(errors) != 1:
            raise RuntimeError(
                f"TeX rejection oracle `{name}` did not report one error:\n"
                f"{completed.stdout}"
            )
        rejections[name] = errors[0]
    return rejections


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", default="pdftex")
    args = parser.parse_args(argv)

    observations = run_oracle(args.engine)
    print(json.dumps(observations, sort_keys=True))
    violations = validate_observations(observations)
    rejections = run_rejection_oracle(args.engine)
    print(json.dumps(rejections, sort_keys=True))
    violations.extend(validate_rejections(rejections))
    if violations:
        print("TeX82 mathcode/delcode oracle failed:")
        for violation in violations:
            print(f"- {violation}")
        return 1
    print(f"TeX82 mathcode/delcode oracle passed ({args.engine} -ini)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
