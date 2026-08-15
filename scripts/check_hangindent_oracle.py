#!/usr/bin/env python3
"""Characterize TeX82 hangindent against dimen0 before M13.3 activation."""

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


PLANNED_SOURCE_ACTIVATION_SET = ["hangindent"]
OWNERS = ("hangindent", "dimen0")

CASE_SPECS = {
    "default": "fresh INITEX default in scaled points",
    "direct_and_physical_units": "ordinary dimensions, signs, rounding, and units",
    "relative_units": "current-font em and ex units",
    "true_units": "true units under non-default magnification",
    "internal_and_query": "internal dimensions, the, and ifdim",
    "dimexpr_unavailable": "pdfTeX INITEX keeps the e-TeX dimexpr primitive absent",
    "scope_and_globaldefs": "local/global assignment and both globaldefs polarities",
    "arithmetic_and_odd_division": "advance, multiply, divide, and signed truncation",
    "afterassignment_alias_shadow_dynamic": (
        "afterassignment, stable aliases, local shadowing, and csname lookup"
    ),
    "dimension_too_large": "maximum-dimension recovery",
    "missing_number": "missing-number recovery and afterassignment",
    "illegal_unit": "illegal-unit recovery and trailing-token progress",
    "arithmetic_overflow": "failed multiply preserves the previous value",
    "divide_by_zero": "failed division preserves the previous value",
}

OBSERVATION_PATTERN = re.compile(
    r"LATEXD-HANGINDENT:([A-Za-z0-9_]+)=(-?[0-9]+)"
)
ERROR_PATTERN = re.compile(r"^! (.+)\.$", re.MULTILINE)


def _observation(name: str, expression: str) -> str:
    return f"\\message{{^^JLATEXD-HANGINDENT:{name}={expression}}}"


def build_case_source(case_id: str, owner: str) -> str:
    if owner not in OWNERS:
        raise ValueError(f"unsupported oracle owner: {owner}")
    target = r"\hangindent" if owner == "hangindent" else r"\dimen0"
    alias_setup = (
        r"\let\saved=\hangindent"
        if owner == "hangindent"
        else r"\dimendef\saved=0"
    )
    dynamic_target = (
        r"\csname hangindent\endcsname"
        if owner == "hangindent"
        else r"\csname dimen\endcsname0"
    )
    lines = [r"\catcode123=1", r"\catcode125=2"]

    if case_id == "default":
        lines.append(_observation("value", rf"\number{target}"))
    elif case_id == "direct_and_physical_units":
        assignments = (
            ("optional_equals", " = 1pt"),
            ("repeated_signs", "=--+1.5pt"),
            ("positive_half_sp", "=.5sp"),
            ("negative_half_sp", "=-.5sp"),
            ("max", "=16383.99998pt"),
            ("in", "=1in"),
            ("pc", "=1pc"),
            ("cm", "=1cm"),
            ("mm", "=1mm"),
            ("bp", "=1bp"),
            ("dd", "=1dd"),
            ("cc", "=1cc"),
        )
        for name, assignment in assignments:
            lines.extend(
                [f"{target}{assignment}", _observation(name, rf"\number{target}")]
            )
    elif case_id == "relative_units":
        lines.extend(
            [
                r"\font\probe=cmr10",
                r"\probe",
                f"{target}=1em",
                _observation("em", rf"\number{target}"),
                f"{target}=1ex",
                _observation("ex", rf"\number{target}"),
            ]
        )
    elif case_id == "true_units":
        lines.extend(
            [
                r"\mag=2000",
                f"{target}=1pt",
                _observation("pt", rf"\number{target}"),
                f"{target}=1truept",
                _observation("truept", rf"\number{target}"),
                f"{target}=1truein",
                _observation("truein", rf"\number{target}"),
            ]
        )
    elif case_id == "internal_and_query":
        lines.extend(
            [
                r"\dimen2=1.25pt",
                f"{target}=\dimen2",
                _observation("internal", rf"\number{target}"),
                rf"\dimen4=\the{target}",
                _observation("the", r"\number\dimen4"),
                rf"\ifdim{target}=1.25pt",
                _observation("ifdim", "1"),
                r"\else",
                _observation("ifdim", "0"),
                r"\fi",
            ]
        )
    elif case_id == "dimexpr_unavailable":
        lines.extend(
            [
                r"\dimen2=1.25pt",
                f"{target}=\dimexpr\dimen2+.5pt\relax",
                _observation("value", rf"\number{target}"),
                _observation("sentinel", "1"),
            ]
        )
    elif case_id == "scope_and_globaldefs":
        lines.extend(
            [
                f"{target}=0pt",
                r"\begingroup",
                f"{target}=1pt",
                _observation("local", rf"\number{target}"),
                r"\endgroup",
                _observation("restored", rf"\number{target}"),
                r"\begingroup",
                rf"\global{target}=2pt",
                r"\endgroup",
                _observation("global", rf"\number{target}"),
                r"\globaldefs=-1",
                r"\begingroup",
                rf"\global{target}=3pt",
                _observation("negative_globaldefs_local", rf"\number{target}"),
                r"\endgroup",
                _observation("negative_globaldefs_restored", rf"\number{target}"),
                r"\globaldefs=1",
                r"\begingroup",
                f"{target}=4pt",
                r"\endgroup",
                r"\globaldefs=0",
                _observation("positive_globaldefs", rf"\number{target}"),
                f"{target}=0pt",
                r"\begingroup",
                f"{target}=0pt",
                _observation("local_default", rf"\number{target}"),
                r"\endgroup",
                _observation("local_default_restored", rf"\number{target}"),
            ]
        )
    elif case_id == "arithmetic_and_odd_division":
        lines.extend(
            [
                f"{target}=1.25pt",
                rf"\advance{target} by .5pt",
                _observation("advanced", rf"\number{target}"),
                rf"\multiply{target} by -3",
                _observation("multiplied", rf"\number{target}"),
                rf"\divide{target} by 2",
                _observation("divided", rf"\number{target}"),
                f"{target}=5sp",
                rf"\divide{target} by 2",
                _observation("positive_odd_division", rf"\number{target}"),
                f"{target}=-5sp",
                rf"\divide{target} by 2",
                _observation("negative_odd_division", rf"\number{target}"),
                f"{target}=16383.99998pt",
                rf"\advance{target} by 16383.99998pt",
                rf"\advance{target} by 2sp",
                _observation("advance_wraps", rf"\number{target}"),
            ]
        )
    elif case_id == "afterassignment_alias_shadow_dynamic":
        lines.extend(
            [
                rf"\def\mark{{{_observation('afterassignment', '1')}}}",
                rf"\afterassignment\mark{target}=1pt",
                _observation("afterassignment_value", rf"\number{target}"),
                alias_setup,
                r"\saved=2pt",
                _observation("alias", r"\number\saved"),
                r"\begingroup",
                r"\def\saved{46}",
                _observation("shadow", r"\saved"),
                r"\endgroup",
                _observation("restored_alias", r"\number\saved"),
                f"{dynamic_target}=3pt",
                _observation("dynamic", rf"\number{target}"),
            ]
        )
    elif case_id == "dimension_too_large":
        lines.extend(
            [
                rf"\def\mark{{{_observation('afterassignment', '1')}}}",
                f"{target}=1pt",
                rf"\afterassignment\mark{target}=16384pt",
                _observation("value", rf"\number{target}"),
                _observation("sentinel", "1"),
            ]
        )
    elif case_id == "missing_number":
        lines.extend(
            [
                rf"\def\mark{{{_observation('afterassignment', '1')}}}",
                f"{target}=1pt",
                rf"\afterassignment\mark{target}=\relax",
                _observation("value", rf"\number{target}"),
                _observation("sentinel", "1"),
            ]
        )
    elif case_id == "illegal_unit":
        lines.extend(
            [
                f"{target}=1pt",
                f"{target}=2qu\relax",
                _observation("value", rf"\number{target}"),
                _observation("sentinel", "1"),
            ]
        )
    elif case_id == "arithmetic_overflow":
        lines.extend(
            [
                f"{target}=16383.99998pt",
                rf"\multiply{target} by 3",
                _observation("value", rf"\number{target}"),
                _observation("sentinel", "1"),
            ]
        )
    elif case_id == "divide_by_zero":
        lines.extend(
            [
                f"{target}=17sp",
                rf"\divide{target} by 0",
                _observation("value", rf"\number{target}"),
                _observation("sentinel", "1"),
            ]
        )
    else:
        raise ValueError(f"unknown oracle case: {case_id}")

    lines.append(r"\end")
    return "\n" + "\n".join(lines) + "\n"


EXPECTED_SEMANTICS = {
    "default": (0, [], {"value": 0}),
    "direct_and_physical_units": (
        0,
        [],
        {
            "optional_equals": 65536,
            "repeated_signs": 98304,
            "positive_half_sp": 0,
            "negative_half_sp": 0,
            "max": 1073741823,
            "in": 4736286,
            "pc": 786432,
            "cm": 1864679,
            "mm": 186467,
            "bp": 65781,
            "dd": 70124,
            "cc": 841489,
        },
    ),
    "relative_units": (0, [], {"em": 655361, "ex": 282168}),
    "true_units": (0, [], {"pt": 65536, "truept": 32768, "truein": 2368143}),
    "internal_and_query": (
        0,
        [],
        {"internal": 81920, "the": 81920, "ifdim": 1},
    ),
    "dimexpr_unavailable": (
        1,
        ["Undefined control sequence"],
        {"value": 81920, "sentinel": 1},
    ),
    "scope_and_globaldefs": (
        0,
        [],
        {
            "local": 65536,
            "restored": 0,
            "global": 131072,
            "negative_globaldefs_local": 196608,
            "negative_globaldefs_restored": 131072,
            "positive_globaldefs": 262144,
            "local_default": 0,
            "local_default_restored": 0,
        },
    ),
    "arithmetic_and_odd_division": (
        0,
        [],
        {
            "advanced": 114688,
            "multiplied": -344064,
            "divided": -172032,
            "positive_odd_division": 2,
            "negative_odd_division": -2,
            "advance_wraps": -2147483648,
        },
    ),
    "afterassignment_alias_shadow_dynamic": (
        0,
        [],
        {
            "afterassignment": 1,
            "afterassignment_value": 65536,
            "alias": 131072,
            "shadow": 46,
            "restored_alias": 131072,
            "dynamic": 196608,
        },
    ),
    "dimension_too_large": (
        1,
        ["Dimension too large"],
        {"afterassignment": 1, "value": 1073741823, "sentinel": 1},
    ),
    "missing_number": (
        1,
        ["Missing number, treated as zero", "Illegal unit of measure (pt inserted)"],
        {"afterassignment": 1, "value": 0, "sentinel": 1},
    ),
    "illegal_unit": (
        1,
        ["Illegal unit of measure (pt inserted)"],
        {"value": 131072, "sentinel": 1},
    ),
    "arithmetic_overflow": (
        1,
        ["Arithmetic overflow"],
        {"value": 1073741823, "sentinel": 1},
    ),
    "divide_by_zero": (
        1,
        ["Arithmetic overflow"],
        {"value": 17, "sentinel": 1},
    ),
}


def _build_expected_case_results() -> dict[str, dict[str, dict[str, object]]]:
    expected = {}
    for case_id, (exit_status, diagnostics, observations) in EXPECTED_SEMANTICS.items():
        owners = {}
        for owner in OWNERS:
            source = build_case_source(case_id, owner)
            owners[owner] = {
                "diagnostics": diagnostics,
                "exit_status": exit_status,
                "observations": observations,
                "source": source,
                "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
            }
        expected[case_id] = owners
    return expected


EXPECTED_CASE_RESULTS = _build_expected_case_results()


def parse_observations(output: str) -> dict[str, int]:
    observations = {}
    for name, raw_value in OBSERVATION_PATTERN.findall(output):
        if name in observations:
            raise ValueError(f"duplicate hangindent oracle observation: {name}")
        observations[name] = int(raw_value)
    return observations


def _collect_case_results(
    engine: str, *, include_raw_output: bool
) -> dict[str, dict[str, dict[str, object]]]:
    results = {}
    process_environment = os.environ.copy()
    process_environment.update({"LC_ALL": "C.UTF-8", "TZ": "UTC"})
    for case_id in CASE_SPECS:
        owners = {}
        for owner in OWNERS:
            source = build_case_source(case_id, owner)
            with tempfile.TemporaryDirectory(prefix="latexd-hangindent-oracle-") as temp:
                root = Path(temp)
                source_path = root / "probe.tex"
                source_path.write_text(source, encoding="utf-8")
                completed = subprocess.run(
                    [engine, "-ini", "-interaction=nonstopmode", source_path.name],
                    cwd=root,
                    env=process_environment,
                    check=False,
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                )
            result = {
                "diagnostics": ERROR_PATTERN.findall(completed.stdout),
                "exit_status": completed.returncode,
                "observations": parse_observations(completed.stdout),
                "source": source,
                "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
            }
            if include_raw_output:
                result["raw_output"] = completed.stdout
            owners[owner] = result
        results[case_id] = owners
    return results


def run_oracle(engine: str) -> dict[str, dict[str, dict[str, object]]]:
    return _collect_case_results(engine, include_raw_output=False)


def validate_case_results(
    results: dict[str, dict[str, dict[str, object]]],
) -> list[str]:
    violations = []
    for case_id, expected in EXPECTED_CASE_RESULTS.items():
        actual = results.get(case_id)
        if actual != expected:
            violations.append(
                f"{case_id} mismatch: expected {expected!r}, observed {actual!r}"
            )
    unexpected = set(results).difference(EXPECTED_CASE_RESULTS)
    if unexpected:
        violations.append(f"unexpected cases: {sorted(unexpected)!r}")
    return violations


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", default="pdftex")
    parser.add_argument(
        "--report", type=Path, default=Path("target/hangindent-oracle.json")
    )
    args = parser.parse_args(argv)

    case_results = _collect_case_results(args.engine, include_raw_output=True)
    semantic_results = json.loads(json.dumps(case_results))
    for owners in semantic_results.values():
        for result in owners.values():
            result.pop("raw_output")
    violations = validate_case_results(semantic_results)
    if violations:
        print("TeX82 hangindent oracle failed:")
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
        "format": "latexd.hangindent-oracle",
        "schema_version": 1,
        "compatibility_target": "TeX82 via pdfTeX INITEX",
        "planned_source_activation_set": PLANNED_SOURCE_ACTIVATION_SET,
        "engine": {
            "path": engine_path,
            "sha256": hashlib.sha256(Path(engine_path).read_bytes()).hexdigest(),
            "version": engine_version,
        },
        "invocation": [args.engine, "-ini", "-interaction=nonstopmode"],
        "environment": {"locale": "C.UTF-8", "timezone": "UTC"},
        "normalization": {
            "diagnostics": "lines matching ^! (.+)\\.$",
            "observations": "LATEXD-HANGINDENT:<name>=<signed integer sp>",
        },
        "case_descriptions": CASE_SPECS,
        "case_results": case_results,
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"TeX82 hangindent oracle passed ({args.engine} -ini); report: {args.report}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
