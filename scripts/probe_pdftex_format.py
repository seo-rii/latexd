#!/usr/bin/env python3
"""Bounded, local pdfTeX preamble dump/restore experiment (not an engine adapter).

Produces machine-readable evidence in .codex/engine-poc. It never installs TeX
packages, invokes a shell, changes source corpus files, or reuses a live input
directory. Six optional corpus cases probe preamble dumping only, not whole-paper
equivalence. Run from the repository root; use a fresh output directory per run.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import shutil
import signal
import subprocess
import time


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def inventory(root: Path) -> dict[str, str]:
    return {str(p.relative_to(root)): digest(p.read_bytes())
            for p in sorted(root.rglob('*')) if p.is_file() and not p.is_symlink()}


def limits() -> None:
    resource.setrlimit(resource.RLIMIT_CPU, (20, 20))
    resource.setrlimit(resource.RLIMIT_AS, (1024 * 1024 * 1024,) * 2)
    resource.setrlimit(resource.RLIMIT_FSIZE, (32 * 1024 * 1024,) * 2)
    resource.setrlimit(resource.RLIMIT_NOFILE, (128, 128))


class Probe:
    def __init__(self, output: Path, timeout: int):
        self.output = output
        self.timeout = timeout
        self.pdftex = shutil.which('pdflatex')
        if not self.pdftex:
            raise RuntimeError('pdflatex is required; this probe does not install it')
        self.commands: list[dict] = []
        self.environment = dict(os.environ, SOURCE_DATE_EPOCH='1788825600',
                                FORCE_SOURCE_DATE='1', TZ='UTC',
                                openin_any='p', openout_any='p', MKTEXPK='0', MKTEXTFM='0', MKTEXFMT='0')
        self.read_files: dict[str, dict] = {}

    def run(self, command: list[str], cwd: Path, label: str) -> dict:
        log = cwd / f'probe-{label}.log'
        started = time.monotonic()
        with log.open('xb') as stream:
            os.chmod(log, 0o600)
            process = subprocess.Popen(command, cwd=cwd, env=self.environment,
                                       stdout=stream, stderr=subprocess.STDOUT,
                                       start_new_session=True, preexec_fn=limits)
            timeout = False
            try:
                code = process.wait(timeout=self.timeout)
            except subprocess.TimeoutExpired:
                timeout = True
                os.killpg(process.pid, signal.SIGKILL)
                code = process.wait()
        raw = log.read_text(errors='replace')
        result = {'command': command, 'cwd': str(cwd.relative_to(self.output)),
                  'pid': process.pid, 'exit_status': code, 'timeout': timeout,
                  'wall_us': round((time.monotonic() - started) * 1_000_000),
                  'log': str(log.relative_to(self.output)),
                  'diagnostics': [line[:400] for line in raw.splitlines()
                                  if line.startswith('!') or 'Fatal error' in line
                                  or 'not found' in line][:8]}
        self.commands.append(result)
        self.readset(cwd)
        return result

    def readset(self, cwd: Path) -> None:
        for recorder in cwd.glob('*.fls'):
            for line in recorder.read_text(errors='replace').splitlines():
                if not line.startswith('INPUT '):
                    continue
                candidate = Path(line[6:])
                path = candidate if candidate.is_absolute() else cwd / candidate
                path = path.resolve()
                if path.is_file() and str(path) not in self.read_files:
                    self.read_files[str(path)] = {
                        'sha256': digest(path.read_bytes()), 'bytes': path.stat().st_size,
                        'system': not path.is_relative_to(self.output)}

    def tex(self, cwd: Path, source: str, label: str, *, dump: bool = False) -> dict:
        command = [self.pdftex, '-no-shell-escape', '-no-mktex=pk', '-no-mktex=tfm', '-interaction=nonstopmode',
                   '-halt-on-error', '-recorder', '-jobname=main']
        if dump:
            command += ['-ini', '&pdflatex']
        elif (cwd / 'main.fmt').exists():
            command += ['-fmt=./main.fmt']
        return self.run(command + [source], cwd, label)

    def observe(self, cwd: Path) -> dict:
        pdf = cwd / 'main.pdf'
        if not pdf.exists():
            return {'pdf_exists': False}
        text = self.run(['pdftotext', '-layout', 'main.pdf', 'text.txt'], cwd, 'text')
        raster = self.run(['pdftoppm', '-r', '72', 'main.pdf', 'page'], cwd, 'raster')
        effects = {p.name: digest(p.read_bytes()) for p in sorted(cwd.iterdir())
                   if p.suffix in {'.aux', '.toc', '.out', '.effect', '.idx'}}
        return {'pdf_exists': True, 'pdf_bytes': pdf.stat().st_size,
                'pdf_sha256': digest(pdf.read_bytes()),
                'text_ok': text['exit_status'] == 0,
                'text_sha256': digest((cwd / 'text.txt').read_bytes()) if text['exit_status'] == 0 else None,
                'raster_ok': raster['exit_status'] == 0,
                'page_raster_sha256': [digest(p.read_bytes()) for p in sorted(cwd.glob('page-*.ppm'))],
                'effects': effects}


def fixtures() -> list[dict]:
    article = '\\documentclass{article}\n'
    cases: list[dict] = []

    def add(name: str, preamble: str = '', body: str = 'Alpha. Stable body.',
            *, cls: str = article, files: dict | None = None, boundary: bool = False,
            already_begun: bool = False):
        cases.append({'name': name, 'preamble': cls + preamble,
                      'body': ('' if already_begun else '\\begin{document}\n') + body + '\n\\end{document}\n',
                      'files': files or {}, 'expected_equivalent': not boundary})

    add('article')
    add('amsart', cls='\\documentclass{amsart}\n', body='\\section{Proof} Let $x=1$.')
    add('twocolumn', cls='\\documentclass[twocolumn]{article}\n', body=('A sentence in two columns. ' * 100))
    add('amsmath', '\\usepackage{amsmath,amssymb}\n',
        r'Inline $x_i^2$. \begin{align} a&=\frac{b}{c}\\d&=\sqrt{x+1}\end{align}')
    add('graphics', '\\usepackage{graphicx}\n', r'\includegraphics[width=80pt]{asset.pdf}')
    add('references', body=r'Forward \ref{later}, page \pageref{later}.\newpage\section{Later}\label{later} Stable.')
    add('table_of_contents', body=r'\tableofcontents\section{First} Alpha.\section{Second} Bravo.')
    add('nested_macros', r'\def\word{Alpha}\newcommand\twice[1]{#1, #1}\def\phrase{\twice{\word}}' + '\n', r'\phrase.')
    add('catcodes', r'\makeatletter\def\private@name{Private}\newcommand\public{\private@name}\makeatother' + '\n', r'\public.')
    add('groups_global', r'\def\local{Outer}\begingroup\def\local{Inner}\gdef\globalword{Global}\endgroup' + '\n', r'\local, \globalword.')
    add('registers', r'\newcount\probeCount\probeCount=37\newdimen\probeDim\probeDim=12pt' + '\n', r'Count \the\probeCount; dimension \the\probeDim.')
    add('included_preamble', '\\input{defs.tex}\n', r'\fromInput.', files={'defs.tex': r'\def\fromInput{Frozen input ALPHA}' + '\n'})
    add('font_encoding', '\\usepackage[T1]{fontenc}\n', r'Accents: \'e, \"o. Bold \textbf{ABC}.')
    add('hyperref', '\\usepackage{hyperref}\n', r'\section{Target}\label{target}\hyperref[target]{Jump}.')
    add('theorem', '\\usepackage{amsthm}\n\\newtheorem{theorem}{Theorem}\n', r'\begin{theorem}One equals one.\end{theorem}')
    add('table', '\\usepackage{array}\n', r'\begin{tabular}{lr}Name&Value\\Alpha&1\\Bravo&2\end{tabular}')
    add('preamble_immediate_write', '\\newwrite\\effect\n\\immediate\\openout\\effect=main.effect\n\\immediate\\write\\effect{preamble effect}\n\\immediate\\closeout\\effect\n', boundary=True)
    add('open_write_stream', '\\newwrite\\effect\n\\immediate\\openout\\effect=main.effect\n', r'\immediate\write\effect{body effect}\immediate\closeout\effect Stable.', boundary=True)
    add('open_group', '\\begingroup\n', r'\endgroup Stable.', boundary=True)
    add('after_shipout', '\\begin{document}\nFirst page.\\newpage\n', 'Second page.', boundary=True, already_begun=True)
    return cases


def run_fixture(probe: Probe, case: dict, asset: Path) -> dict:
    root = probe.output / 'fixtures' / case['name']
    source = root / 'snapshot'
    source.mkdir(parents=True)
    (source / 'preamble.tex').write_text(case['preamble'])
    (source / 'body.tex').write_text(case['body'])
    for name, data in case['files'].items():
        (source / name).write_text(data)
    if case['name'] == 'graphics':
        shutil.copyfile(asset, source / 'asset.pdf')
    snapshot_hashes = inventory(source)
    lanes = {}
    for lane in ['clean', 'dump', 'restored']:
        lanes[lane] = root / lane
        shutil.copytree(source, lanes[lane])
    (lanes['clean'] / 'main.tex').write_text('\\input{preamble.tex}\n\\input{body.tex}\n')
    (lanes['dump'] / 'dump.tex').write_text('\\input{preamble.tex}\n\\dump\n')
    clean_runs = []
    for index in range(3):
        result = probe.tex(lanes['clean'], 'main.tex', f'clean-{index + 1}')
        clean_runs.append(result)
        if result['exit_status']:
            break
    clean = probe.observe(lanes['clean']) if clean_runs[-1]['exit_status'] == 0 else None
    dump = probe.tex(lanes['dump'], 'dump.tex', 'dump', dump=True)
    format_file = lanes['dump'] / 'main.fmt'
    restored_runs = []
    restored = None
    if dump['exit_status'] == 0 and format_file.exists():
        shutil.copyfile(format_file, lanes['restored'] / 'main.fmt')
        for index in range(3):
            result = probe.tex(lanes['restored'], 'body.tex', f'restored-{index + 1}')
            restored_runs.append(result)
            if result['exit_status']:
                break
        if restored_runs[-1]['exit_status'] == 0:
            restored = probe.observe(lanes['restored'])
    checks = {
        'text_equal': bool(clean and restored and clean['text_ok'] and restored['text_ok']
                           and clean['text_sha256'] == restored['text_sha256']),
        'all_page_pixels_equal': bool(clean and restored and clean['raster_ok'] and restored['raster_ok']
                                     and clean['page_raster_sha256'] and clean['page_raster_sha256'] == restored['page_raster_sha256']),
        'aux_and_file_effects_equal': bool(clean and restored and clean['effects'] == restored['effects']),
        'input_snapshot_unchanged': inventory(source) == snapshot_hashes,
    }
    return {'fixture': case['name'], 'kind': 'controlled_fixture',
            'expected_equivalent': case['expected_equivalent'], 'checks': checks,
            'equivalent': all(checks.values()), 'source_sha256': snapshot_hashes,
            'clean': clean, 'restored': restored,
            'clean_exit': clean_runs[-1]['exit_status'], 'dump_exit': dump['exit_status'],
            'restored_exit': restored_runs[-1]['exit_status'] if restored_runs else None,
            'format_bytes': format_file.stat().st_size if format_file.exists() else None,
            'format_sha256': digest(format_file.read_bytes()) if format_file.exists() else None,
            'dump_us': dump['wall_us'],
            'clean_pass_us': [result['wall_us'] for result in clean_runs],
            'restored_pass_us': [result['wall_us'] for result in restored_runs],
            'dump_side_effects': sorted(p.name for p in lanes['dump'].glob('*.effect'))}


def run_corpus_preambles(probe: Probe, corpus: Path) -> list[dict]:
    lock = json.loads((corpus / 'corpus-lock.json').read_text())
    results = []
    for case in lock['cases'][:6]:
        original = corpus / 'sources' / case['arxiv_id']
        target = probe.output / 'corpus-preambles' / case['arxiv_id']
        source_bytes = sum(p.stat().st_size for p in original.rglob('*') if p.is_file())
        if source_bytes > 100 * 1024 * 1024 or any(p.is_symlink() for p in original.rglob('*')):
            results.append({'arxiv_id': case['arxiv_id'], 'status': 'source_budget_or_symlink'})
            continue
        before = inventory(original)
        shutil.copytree(original, target)
        text = (target / case['toplevel']).read_text(errors='replace')
        boundary = re.search(r'^\s*\\begin\s*\{document\}', text, re.MULTILINE)
        if not boundary:
            results.append({'arxiv_id': case['arxiv_id'], 'status': 'no_literal_document_boundary'})
            continue
        prefix = text[:boundary.start()]
        (target / 'preamble-probe.tex').write_text(prefix + '\n\\dump\n')
        dump = probe.tex(target, 'preamble-probe.tex', 'dump', dump=True)
        fmt = target / 'main.fmt'
        results.append({'arxiv_id': case['arxiv_id'], 'version': case['version'],
                        'kind': 'real_paper_preamble_only',
                        'archive_sha256': case['source_sha256'],
                        'entrypoint_sha256': before[case['toplevel']],
                        'prefix_sha256': digest(prefix.encode()), 'prefix_bytes': len(prefix.encode()),
                        'dump_exit': dump['exit_status'], 'dump_us': dump['wall_us'],
                        'format_bytes': fmt.stat().st_size if fmt.exists() else None,
                        'diagnostics': dump['diagnostics'],
                        'source_original_unchanged': before == inventory(original),
                        'whole_document_restore_tested': False})
    return results


def run_dependency_mutation(probe: Probe) -> dict:
    """A wrapper snapshot isolates input; a format does not invalidate itself."""
    root = probe.output / 'dependency-mutation'
    live = root / 'live'
    live.mkdir(parents=True)
    (live / 'defs.tex').write_text('\\def\\messageword{ALPHA}\n')
    (live / 'preamble.tex').write_text('\\documentclass{article}\n\\input{defs.tex}\n')
    (live / 'body.tex').write_text('\\begin{document}\\messageword\\end{document}\n')
    captured = root / 'captured'
    shutil.copytree(live, captured)
    original_hash = digest((captured / 'defs.tex').read_bytes())
    dump_dir = root / 'dump'
    shutil.copytree(captured, dump_dir)
    (dump_dir / 'dump.tex').write_text('\\input{preamble.tex}\n\\dump\n')
    dumped = probe.tex(dump_dir, 'dump.tex', 'dump', dump=True)
    if dumped['exit_status']:
        return {'status': 'dump_failed', 'dump_exit': dumped['exit_status']}
    # The live project changes only after the wrapper captured and compiled A.
    (live / 'defs.tex').write_text('\\def\\messageword{BRAVO}\n')
    observed = {}
    for lane, source, reuse in [('captured_restore', captured, True),
                                ('new_revision_clean', live, False),
                                ('unsafe_new_revision_restore', live, True)]:
        target = root / lane
        shutil.copytree(source, target)
        if reuse:
            shutil.copyfile(dump_dir / 'main.fmt', target / 'main.fmt')
            entry = 'body.tex'
        else:
            (target / 'main.tex').write_text('\\input{preamble.tex}\n\\input{body.tex}\n')
            entry = 'main.tex'
        runs = [probe.tex(target, entry, f'pass-{index}') for index in range(2)]
        observed[lane] = probe.observe(target) if all(run['exit_status'] == 0 for run in runs) else None
    captured_result = observed['captured_restore']
    new_result = observed['new_revision_clean']
    unsafe_result = observed['unsafe_new_revision_restore']
    checks = {
        'captured_bytes_stayed_immutable': digest((captured / 'defs.tex').read_bytes()) == original_hash,
        'desired_dependency_bytes_changed': digest((live / 'defs.tex').read_bytes()) != original_hash,
        'unsafe_restore_preserves_old_text': bool(captured_result and unsafe_result and
                                                 captured_result['text_sha256'] == unsafe_result['text_sha256']),
        'unsafe_restore_differs_from_new_clean': bool(new_result and unsafe_result and
                                                     new_result['text_sha256'] != unsafe_result['text_sha256']),
    }
    return {'status': 'observed', 'kind': 'dependency_revision_mutation', 'checks': checks,
            'captured_dependency_sha256': original_hash,
            'desired_dependency_sha256': digest((live / 'defs.tex').read_bytes()),
            'observations': observed,
            'interpretation': 'The wrapper supplies immutable bytes; reuse must validate the preamble readset before selecting the format.'}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=Path('.codex/engine-poc'))
    parser.add_argument('--corpus', type=Path)
    parser.add_argument('--timeout-seconds', type=int, default=25)
    args = parser.parse_args()
    output = args.output.resolve()
    if (output / 'report.json').exists() or (output / 'fixtures').exists():
        parser.error('output contains a prior run; choose a fresh directory')
    output.mkdir(parents=True, exist_ok=True)
    os.chmod(output, 0o700)
    started = time.monotonic()
    probe = Probe(output, args.timeout_seconds)
    cases = fixtures()
    (output / 'fixture-manifest.json').write_text(json.dumps(cases, indent=2) + '\n')
    asset_dir = output / 'asset'
    asset_dir.mkdir()
    (asset_dir / 'asset.tex').write_text(r'\documentclass{article}\usepackage[paperwidth=80pt,paperheight=80pt,margin=5pt]{geometry}\pagestyle{empty}\begin{document}\noindent\rule{20pt}{20pt}\end{document}' + '\n')
    asset_run = probe.tex(asset_dir, 'asset.tex', 'asset')
    if asset_run['exit_status']:
        raise RuntimeError('failed to create controlled graphics asset')
    results = []
    for case in cases:
        results.append(run_fixture(probe, case, asset_dir / 'main.pdf'))
        used = sum(p.stat().st_size for p in output.rglob('*') if p.is_file())
        if used > 500 * 1024 * 1024:
            raise RuntimeError('500 MiB experiment output budget exceeded')
    dependency_mutation = run_dependency_mutation(probe)
    corpus_results = run_corpus_preambles(probe, args.corpus.resolve()) if args.corpus else []
    version = subprocess.check_output([probe.pdftex, '--version'], text=True)
    report = {'schema_version': 1, 'experiment': 'native_pdftex_preamble_format_dump',
              'engine_version': version, 'engine_binary': str(Path(probe.pdftex).resolve()),
              'engine_binary_sha256': digest(Path(probe.pdftex).read_bytes()),
              'probe_script_sha256': digest(Path(__file__).read_bytes()),
              'controlled_fixtures': results, 'real_paper_preambles': corpus_results,
              'dependency_mutation': dependency_mutation,
              'environment': {key: probe.environment[key] for key in
                              ['SOURCE_DATE_EPOCH', 'FORCE_SOURCE_DATE', 'TZ', 'openin_any', 'openout_any', 'MKTEXPK', 'MKTEXTFM', 'MKTEXFMT']},
              'context_contract': {'jobname': 'main', 'shell_escape': False,
                                   'same_engine_package_tree': True,
                                   'input_snapshot_copies': True,
                                   'native_effect_journal': False,
                                   'mid_document_continuation_api': False},
              'budgets': {'per_process_wall_seconds': args.timeout_seconds,
                          'per_process_cpu_seconds': 20, 'per_process_address_bytes': 1024**3,
                          'per_file_bytes': 32 * 1024**2, 'fixture_output_bytes': 500 * 1024**2},
              'elapsed_us': round((time.monotonic() - started) * 1_000_000),
              'output_bytes_before_report': sum(p.stat().st_size for p in output.rglob('*') if p.is_file()),
              'files_used': probe.read_files, 'commands': probe.commands}
    (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    good = [case for case in results if case['expected_equivalent']]
    print(json.dumps({'report': str(output / 'report.json'), 'controlled_fixtures': len(results),
                      'supported_fixture_equal': sum(case['equivalent'] for case in good),
                      'supported_fixture_total': len(good),
                      'boundary_observations': [{key: case[key] for key in
                          ['fixture', 'equivalent', 'clean_exit', 'dump_exit', 'restored_exit', 'checks']}
                          for case in results if not case['expected_equivalent']],
                      'real_paper_preamble_dumps': sum(case.get('dump_exit') == 0 for case in corpus_results),
                      'real_paper_preambles_total': len(corpus_results)}))
    return 0 if (all(case['equivalent'] for case in good)
                 and all(dependency_mutation.get('checks', {'missing': False}).values())) else 1


if __name__ == '__main__':
    raise SystemExit(main())
