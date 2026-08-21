#!/usr/bin/env python3
"""Characterize TeX82 font and magnification context needed by dimension scans."""

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
    / "crates/tex-vm/tests/fixtures/dimension-scan-context-oracle-v1.json"
)
REQUIRED_METRIC_FILES = ("cmr10.tfm", "cmr7.tfm")
CASE_SPECS = {
    "fresh_current_font": "fresh INITEX current-font identity and em/ex metrics",
    "cmr10_metrics": "cmr10 identity, quad, x-height, em, and ex",
    "second_font_metrics": "distinct cmr7 identity and metrics",
    "grouped_font_switch": "current-font selection restores across a group",
    "repeated_font_selection": "repeated same-level selection remains stable",
    "scaled_font_selection": "scaled and at-size font-relative dimensions",
    "font_alias_dynamic_lookup": "font aliases and csname selection",
    "missing_metric_file": "missing TFM diagnostics, retention, and progress",
    "invalid_font_definition": "invalid at-size recovery and progress",
    "font_magnification_interaction": "font-relative units under non-default mag",
    "magnification_fresh_query": "fresh INITEX mag value and query",
    "magnification_direct_assignments": "several direct mag assignments",
    "magnification_optional_equals_signs": "optional equals and repeated signs",
    "magnification_scope_globaldefs": "local, global, and globaldefs semantics",
    "magnification_alias_dynamic_lookup": "mag aliases and csname lookup",
    "magnification_afterassignment_success": "afterassignment on valid mag input",
    "magnification_afterassignment_error": "afterassignment on invalid mag input",
    "magnification_true_units": "true units below nominal magnification",
    "magnification_ordinary_vs_true_units": "ordinary and true units at mag 2000",
    "magnification_reassignment_after_use": "mag reassignment after true-unit use",
    "magnification_missing_number": "missing-number recovery and progress",
    "magnification_range_error": "out-of-range mag recovery and progress",
}

OBSERVATION_PATTERN = re.compile(
    r"LATEXD-SCANCTX:([A-Za-z0-9_]+)=([A-Za-z0-9_.:/+-]+)"
)


def _observation(name: str, expression: str) -> str:
    return f"\\message{{^^JLATEXD-SCANCTX:{name}={expression}}}"


def _dimension_observation(name: str, expression: str) -> list[str]:
    return [
        rf"\dimen254={expression}",
        _observation(name, r"\number\dimen254"),
    ]


def build_case_source(case_id: str) -> str:
    lines = [r"\catcode123=1", r"\catcode125=2"]

    if case_id == "fresh_current_font":
        lines.extend(
            [
                _observation("font", r"\fontname\font"),
                *_dimension_observation("quad", r"\fontdimen6\font"),
                *_dimension_observation("x_height", r"\fontdimen5\font"),
                *_dimension_observation("em", "1em"),
                *_dimension_observation("ex", "1ex"),
            ]
        )
    elif case_id == "cmr10_metrics":
        lines.extend(
            [
                r"\font\primary=cmr10",
                r"\primary",
                _observation("font", r"\fontname\font"),
                *_dimension_observation("quad", r"\fontdimen6\font"),
                *_dimension_observation("x_height", r"\fontdimen5\font"),
                *_dimension_observation("em", "1em"),
                *_dimension_observation("ex", "1ex"),
            ]
        )
    elif case_id == "second_font_metrics":
        lines.extend(
            [
                r"\font\secondary=cmr7",
                r"\secondary",
                _observation("font", r"\fontname\font"),
                *_dimension_observation("quad", r"\fontdimen6\font"),
                *_dimension_observation("x_height", r"\fontdimen5\font"),
                *_dimension_observation("em", "1em"),
                *_dimension_observation("ex", "1ex"),
            ]
        )
    elif case_id == "grouped_font_switch":
        lines.extend(
            [
                r"\font\primary=cmr10",
                r"\font\secondary=cmr7",
                r"\primary",
                _observation("before_font", r"\fontname\font"),
                *_dimension_observation("before_em", "1em"),
                r"\begingroup",
                r"\secondary",
                _observation("inside_font", r"\fontname\font"),
                *_dimension_observation("inside_em", "1em"),
                r"\endgroup",
                _observation("after_font", r"\fontname\font"),
                *_dimension_observation("after_em", "1em"),
            ]
        )
    elif case_id == "repeated_font_selection":
        lines.extend(
            [
                r"\font\primary=cmr10",
                r"\primary\primary\primary",
                _observation("font", r"\fontname\font"),
                *_dimension_observation("em", "1em"),
                *_dimension_observation("ex", "1ex"),
            ]
        )
    elif case_id == "scaled_font_selection":
        lines.extend(
            [
                r"\font\scaledfont=cmr10 scaled 1200",
                r"\scaledfont",
                _observation("scaled_font", r"\fontname\font"),
                *_dimension_observation("scaled_quad", r"\fontdimen6\font"),
                *_dimension_observation("scaled_x_height", r"\fontdimen5\font"),
                *_dimension_observation("scaled_em", "1em"),
                *_dimension_observation("scaled_ex", "1ex"),
                r"\font\atfont=cmr10 at 12pt",
                r"\atfont",
                _observation("at_font", r"\fontname\font"),
                *_dimension_observation("at_em", "1em"),
                *_dimension_observation("at_ex", "1ex"),
            ]
        )
    elif case_id == "font_alias_dynamic_lookup":
        lines.extend(
            [
                r"\font\primary=cmr10",
                r"\font\secondary=cmr7",
                r"\let\fontalias=\primary",
                r"\fontalias",
                _observation("alias_font", r"\fontname\font"),
                *_dimension_observation("alias_em", "1em"),
                r"\csname secondary\endcsname",
                _observation("dynamic_font", r"\fontname\font"),
                *_dimension_observation("dynamic_em", "1em"),
            ]
        )
    elif case_id == "missing_metric_file":
        lines.extend(
            [
                r"\font\primary=cmr10",
                r"\primary",
                r"\font\missing=latexdmissingmetric",
                _observation("retained_font", r"\fontname\font"),
                *_dimension_observation("retained_em", "1em"),
                _observation("sentinel", "1"),
            ]
        )
    elif case_id == "invalid_font_definition":
        lines.extend(
            [
                r"\font\invalid=cmr10 at -1pt",
                r"\invalid",
                _observation("font", r"\fontname\font"),
                *_dimension_observation("em", "1em"),
                _observation("sentinel", "1"),
            ]
        )
    elif case_id == "font_magnification_interaction":
        lines.extend(
            [
                r"\mag=2000",
                r"\font\primary=cmr10",
                r"\primary",
                _observation("mag", r"\number\mag"),
                _observation("font", r"\fontname\font"),
                *_dimension_observation("quad", r"\fontdimen6\font"),
                *_dimension_observation("x_height", r"\fontdimen5\font"),
                *_dimension_observation("em", "1em"),
                *_dimension_observation("ex", "1ex"),
            ]
        )
    elif case_id == "magnification_fresh_query":
        lines.append(_observation("mag", r"\number\mag"))
    elif case_id == "magnification_direct_assignments":
        lines.extend(
            [
                r"\mag=500",
                _observation("first", r"\number\mag"),
                r"\mag=1000",
                _observation("second", r"\number\mag"),
                r"\mag=2000",
                _observation("third", r"\number\mag"),
            ]
        )
    elif case_id == "magnification_optional_equals_signs":
        lines.extend(
            [
                r"\mag --+2000",
                _observation("without_equals", r"\number\mag"),
                r"\mag = ---1500",
                _observation("with_equals", r"\number\mag"),
            ]
        )
    elif case_id == "magnification_scope_globaldefs":
        lines.extend(
            [
                r"\begingroup\mag=1200",
                _observation("local", r"\number\mag"),
                r"\endgroup",
                _observation("restored", r"\number\mag"),
                r"\begingroup\global\mag=1300\endgroup",
                _observation("global", r"\number\mag"),
                r"\globaldefs=-1",
                r"\begingroup\global\mag=1400",
                _observation("negative_globaldefs_local", r"\number\mag"),
                r"\endgroup",
                _observation("negative_globaldefs_restored", r"\number\mag"),
                r"\globaldefs=1",
                r"\begingroup\mag=1500\endgroup",
                r"\globaldefs=0",
                _observation("positive_globaldefs", r"\number\mag"),
            ]
        )
    elif case_id == "magnification_alias_dynamic_lookup":
        lines.extend(
            [
                r"\let\savedmag=\mag",
                r"\savedmag=1200",
                _observation("alias", r"\number\savedmag"),
                r"\csname mag\endcsname=1300",
                _observation("dynamic", r"\number\mag"),
            ]
        )
    elif case_id == "magnification_afterassignment_success":
        lines.extend(
            [
                rf"\def\mark{{{_observation('afterassignment', '1')}}}",
                r"\afterassignment\mark\mag=2000",
                _observation("value", r"\number\mag"),
                _observation("sentinel", "1"),
            ]
        )
    elif case_id == "magnification_afterassignment_error":
        lines.extend(
            [
                r"\mag=2000",
                rf"\def\mark{{{_observation('afterassignment', '1')}}}",
                r"\afterassignment\mark\mag=\relax",
                _observation("value", r"\number\mag"),
                _observation("sentinel", "1"),
            ]
        )
    elif case_id == "magnification_true_units":
        lines.extend(
            [
                r"\mag=500",
                *_dimension_observation("pt", "1pt"),
                *_dimension_observation("truept", "1truept"),
                *_dimension_observation("truein", "1truein"),
                _observation("mag", r"\number\mag"),
            ]
        )
    elif case_id == "magnification_ordinary_vs_true_units":
        lines.extend(
            [
                r"\mag=2000",
                *_dimension_observation("pt", "1pt"),
                *_dimension_observation("inch", "1in"),
                *_dimension_observation("truept", "1truept"),
                *_dimension_observation("truein", "1truein"),
                _observation("mag", r"\number\mag"),
            ]
        )
    elif case_id == "magnification_reassignment_after_use":
        lines.extend(
            [
                r"\mag=2000",
                *_dimension_observation("before", "1truept"),
                r"\mag=1000",
                _observation("retained_mag", r"\number\mag"),
                *_dimension_observation("after", "1truept"),
                _observation("sentinel", "1"),
            ]
        )
    elif case_id == "magnification_missing_number":
        lines.extend(
            [
                r"\mag=2000",
                r"\mag=\relax",
                _observation("assigned_value", r"\number\mag"),
                *_dimension_observation("truept", "1truept"),
                _observation("value_after_use", r"\number\mag"),
                _observation("sentinel", "1"),
            ]
        )
    elif case_id == "magnification_range_error":
        lines.extend(
            [
                r"\mag=40000",
                _observation("assigned_value", r"\number\mag"),
                *_dimension_observation("truept", "1truept"),
                _observation("value_after_use", r"\number\mag"),
                _observation("sentinel", "1"),
            ]
        )
    else:
        raise ValueError(f"unknown dimension scan-context case: {case_id}")

    lines.append(r"\end")
    return "\n" + "\n".join(lines) + "\n"


def parse_observations(output: str) -> dict[str, int | str]:
    observations: dict[str, int | str] = {}
    for name, raw_value in OBSERVATION_PATTERN.findall(output):
        if name in observations:
            raise ValueError(f"duplicate dimension scan-context observation: {name}")
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


def _collect_case_results(
    engine: str, *, include_raw_output: bool
) -> dict[str, dict[str, object]]:
    results = {}
    process_environment = os.environ.copy()
    process_environment.update({"LC_ALL": "C.UTF-8", "TZ": "UTC"})
    for case_id in CASE_SPECS:
        source = build_case_source(case_id)
        with tempfile.TemporaryDirectory(prefix="latexd-scan-context-oracle-") as temp:
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
            "diagnostics": parse_diagnostics(completed.stdout),
            "exit_status": completed.returncode,
            "observations": parse_observations(completed.stdout),
            "source": source,
            "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
        }
        if include_raw_output:
            result["raw_output"] = completed.stdout
        results[case_id] = result
    return results


def _fixture_result(result: dict[str, object]) -> dict[str, object]:
    return {
        "diagnostics": result["diagnostics"],
        "exit_status": result["exit_status"],
        "observations": result["observations"],
        "source_sha256": result["source_sha256"],
    }


def validate_case_results(
    results: dict[str, dict[str, object]], expected: dict[str, dict[str, object]]
) -> list[str]:
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


def _resolve_metric_provenance(
    kpsewhich_path: str, process_environment: dict[str, str]
) -> tuple[list[dict[str, object]], str]:
    metrics = []
    for requested_name in REQUIRED_METRIC_FILES:
        lookup = subprocess.run(
            [kpsewhich_path, requested_name],
            check=True,
            env=process_environment,
            text=True,
            stdout=subprocess.PIPE,
        ).stdout.strip()
        if not lookup:
            raise RuntimeError(f"TeX font metric not found: {requested_name}")
        lookup_path = Path(lookup).absolute()
        resolved_path = lookup_path.resolve(strict=True)
        metrics.append(
            {
                "requested_name": requested_name,
                "lookup_path": str(lookup_path),
                "resolved_path": str(resolved_path),
                "lookup_path_is_symlink": lookup_path.is_symlink(),
                "sha256": hashlib.sha256(resolved_path.read_bytes()).hexdigest(),
            }
        )
    texmf_search_path = subprocess.run(
        [kpsewhich_path, "--var-value=TEXMF"],
        check=True,
        env=process_environment,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout.strip()
    return metrics, texmf_search_path


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", default="pdftex")
    parser.add_argument(
        "--report",
        type=Path,
        default=Path("target/dimension-scan-context-oracle.json"),
    )
    args = parser.parse_args(argv)

    fixture = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))
    expected = fixture["case_results"]
    case_results = _collect_case_results(args.engine, include_raw_output=True)
    violations = validate_case_results(case_results, expected)
    if violations:
        print("TeX82 dimension scan-context oracle failed:")
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
    kpsewhich_path = shutil.which("kpsewhich")
    if kpsewhich_path is None:
        raise RuntimeError("TeX font resolver not found: kpsewhich")
    process_environment = os.environ.copy()
    process_environment.update({"LC_ALL": "C.UTF-8", "TZ": "UTC"})
    metric_files, texmf_search_path = _resolve_metric_provenance(
        kpsewhich_path, process_environment
    )
    report = {
        "format": "latexd.dimension-scan-context-oracle",
        "schema_version": 1,
        "compatibility_target": "TeX82 via pdfTeX INITEX",
        "engine": {
            "path": engine_path,
            "sha256": hashlib.sha256(Path(engine_path).read_bytes()).hexdigest(),
            "version": engine_version,
        },
        "invocation": [args.engine, "-ini", "-interaction=nonstopmode"],
        "environment": {"locale": "C.UTF-8", "timezone": "UTC"},
        "font_resolver": {
            "kpsewhich_path": str(Path(kpsewhich_path).resolve()),
            "texmf_search_path": texmf_search_path,
            "metric_files": metric_files,
        },
        "expected_processes": len(CASE_SPECS),
        "observed_processes": len(case_results),
        "normalization": {
            "diagnostics": (
                "lines beginning !; semicolon continuation joined; "
                "one trailing period removed"
            ),
            "observations": "LATEXD-SCANCTX:<name>=<integer-or-identity>",
        },
        "case_descriptions": CASE_SPECS,
        "case_results": case_results,
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"TeX82 dimension scan-context oracle passed ({args.engine} -ini); "
        f"report: {args.report}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
