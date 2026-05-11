use std::{collections::BTreeMap, fs};

use anyhow::{Context, Result, bail};
use camino::Utf8PathBuf;
use tex_layout::{DocumentLayout, LayoutOptions, layout_text};
use tex_pdf::render_pdf;
use tex_tokens::ControlSequenceInterner;
use tex_vm::{
    Vm, VmDiagnostic, VmModuleCheckpoint, VmModuleCheckpointKind, VmModuleTrace, VmReplayFrame,
    VmSnapshot, compile_format_snapshot,
};
use tex_world::ProjectWorld;

pub const MINI_KERNEL_SOURCE: &str = r##"
\def\par{ }
\def\relax{}
\def\begin#1{}
\def\end#1{}
\def\title#1{}
\newcommand{\author}[2][]{}
\def\date#1{}
\def\maketitle{}
\newcommand{\thanks}[1]{}
\newcommand{\affil}[2][]{}
\newcommand{\institute}[1]{}
\newcommand{\email}[1]{}
\newcommand{\orcidID}[1]{}
\newcommand{\titlerunning}[1]{}
\newcommand{\authorrunning}[1]{}
\newcommand{\keywords}[1]{#1}
\def\chapter#1{#1}
\def\section#1{#1}
\def\subsection#1{#1}
\def\subsubsection#1{#1}
\def\paragraph#1{#1}
\def\subparagraph#1{#1}
\def\appendix{}
\def\phantomsection{}
\def\addcontentsline#1#2#3{}
\def\addtocontents#1#2{}
\def\textbf#1{#1}
\def\emph#1{#1}
\def\textit#1{#1}
\def\texttt#1{#1}
\def\textrm#1{#1}
\def\textmd#1{#1}
\def\textsc#1{#1}
\def\textsuperscript#1{#1}
\def\textsubscript#1{#1}
\newcommand{\textcolor}[3][]{#3}
\def\color#1{}
\def\ensuremath#1{#1}
\def\mbox#1{#1}
\def\fbox#1{#1}
\newcommand{\makebox}[2][]{#2}
\newcommand{\raisebox}[3][]{#3}
\def\normalfont{}
\def\rmfamily{}
\def\sffamily{}
\def\ttfamily{}
\def\bfseries{}
\def\itshape{}
\def\scshape{}
\def\mdseries{}
\def\rm{}
\def\bf{}
\def\it{}
\def\tt{}
\def\small{}
\def\footnotesize{}
\def\scriptsize{}
\def\tiny{}
\def\normalsize{}
\def\large{}
\def\Large{}
\def\LARGE{}
\def\huge{}
\def\Huge{}
\def\boldmath{}
\def\unboldmath{}
\def\frenchspacing{}
\def\centering{}
\def\raggedright{}
\def\raggedleft{}
\def\newpage{}
\def\clearpage{}
\def\pagebreak{}
\def\nopagebreak{}
\def\noindent{}
\def\indent{}
\def\hfill{}
\def\vfill{}
\def\hfil{}
\def\vfil{}
\def\hspace#1{}
\def\vspace#1{}
\def\vskip#1{}
\def\hskip#1{}
\def\kern#1{}
\def\medskip{}
\def\smallskip{}
\def\bigskip{}
\def\linebreak{}
\def\nolinebreak{}
\def\sloppy{}
\def\fussy{}
\def\label#1{}
\def\ref#1{#1}
\def\pageref#1{#1}
\def\eqref#1{(#1)}
\def\autoref#1{#1}
\def\nameref#1{#1}
\def\cref#1{#1}
\def\Cref#1{#1}
\newcommand{\cite}[2][]{#2}
\newcommand{\citep}[2][]{#2}
\newcommand{\citet}[2][]{#2}
\def\href#1#2{#2}
\def\url#1{#1}
\def\nolinkurl#1{#1}
\def\path#1{#1}
\def\hypersetup#1{}
\newdimen\linewidth
\newdimen\columnwidth
\newdimen\textwidth
\newdimen\textheight
\newdimen\parskip
\newdimen\hfuzz
\newdimen\vfuzz
\newdimen\overfullrule
\newskip\Urlmuskip
\newcount\hbadness
\newcount\vbadness
\def\textfraction{}
\def\floatpagefraction{}
\def\topfraction{}
\def\bottomfraction{}
\def\dblfloatpagefraction{}
\def\dbltopfraction{}
\def\arraystretch{1}
\def\labelenumii{}
\def\labelenumiii{}
\def\newcounter#1{}
\def\newtheorem#1#2{}
\def\theoremstyle#1{}
\def\numberwithin#1#2{}
\def\nonumber{}
\def\notag{}
\def\tag#1{}
\newcommand{\footnote}[2][]{}
\newcommand{\footnotemark}[1][]{}
\def\hline{}
\def\cline#1{}
\def\toprule{}
\def\midrule{}
\def\bottomrule{}
\def\cmidrule#1{}
\def\checkmark{x}
\providecommand{\subfigure}[2][]{#2}
\def\left{}
\def\right{}
\def\middle{}
\def\big#1{#1}
\def\Big#1{#1}
\def\bigg#1{#1}
\def\Bigg#1{#1}
\def\bigl#1{#1}
\def\bigr#1{#1}
\def\Bigl#1{#1}
\def\Bigr#1{#1}
\def\langle{<}
\def\rangle{>}
\def\lvert{|}
\def\rvert{|}
\def\lVert{||}
\def\rVert{||}
\def\vert{|}
\def\Vert{||}
\def\alpha{alpha}
\def\beta{beta}
\def\gamma{gamma}
\def\delta{delta}
\def\epsilon{epsilon}
\def\varepsilon{varepsilon}
\def\zeta{zeta}
\def\eta{eta}
\def\theta{theta}
\def\vartheta{vartheta}
\def\iota{iota}
\def\kappa{kappa}
\def\lambda{lambda}
\def\mu{mu}
\def\nu{nu}
\def\xi{xi}
\def\pi{pi}
\def\rho{rho}
\def\sigma{sigma}
\def\tau{tau}
\def\upsilon{upsilon}
\def\phi{phi}
\def\varphi{varphi}
\def\chi{chi}
\def\psi{psi}
\def\omega{omega}
\def\Gamma{Gamma}
\def\Delta{Delta}
\def\Theta{Theta}
\def\Lambda{Lambda}
\def\Xi{Xi}
\def\Pi{Pi}
\def\Sigma{Sigma}
\def\Phi{Phi}
\def\Psi{Psi}
\def\Omega{Omega}
\def\ell{ell}
\def\hbar{hbar}
\def\sum{sum}
\def\prod{prod}
\def\int{int}
\def\partial{partial}
\def\infty{infty}
\def\log{log}
\def\exp{exp}
\def\cos{cos}
\def\sin{sin}
\def\dim{dim}
\def\Pr{Pr}
\def\le{<=}
\def\leq{<=}
\def\ge{>=}
\def\geq{>=}
\def\neq{!=}
\def\npreceq{not <=}
\def\preceq{<=}
\def\prec{<}
\def\approx{approx}
\def\propto{propto}
\def\subseteq{subset}
\def\subset{subset}
\def\notin{not in}
\def\in{in}
\def\cap{cap}
\def\otimes{x}
\def\oplus{+}
\def\bigotimes{x}
\def\wedge{and}
\def\vee{or}
\def\cup{cup}
\def\setminus{minus}
\def\neg{not}
\def\lnot{not}
\def\not#1{#1}
\def\cdot{*}
\def\cdots{...}
\def\ldots{...}
\def\dots{...}
\def\times{x}
\def\pm{+-}
\def\circ{o}
\def\bigcirc{o}
\def\bigcircop{o}
\def\ast{*}
\def\star{*}
\def\dag{dag}
\def\dagger{dagger}
\def\sim{sim}
\def\triangleq{=}
\def\nabla{nabla}
\def\prime{prime}
\def\to{to}
\def\rightarrow{to}
\def\Rightarrow{to}
\def\Longrightarrow{to}
\def\leftrightarrow{to}
\def\nleftrightarrow{not to}
\def\forall{for all}
\def\equiv{=}
\def\gets{gets}
\def\backslash{backslash}
\def\arg{arg}
\def\min{min}
\def\max{max}
\def\ln{ln}
\def\mod{mod}
\def\bmod{mod}
\def\mid{|}
\def\colon{:}
\def\perp{perp}
\def\triangleright{>}
\def\lceil{ceil}
\def\rceil{ceil}
\def\lfloor{floor}
\def\rfloor{floor}
\def\displaystyle{}
\def\quad{ }
\def\qquad{ }
\def\!{}
\def\ { }
\def\,{ }
\def\;{ }
\def\:{ }
\def\|{|}
\def\{{}
\def\}{}
\def\[{}
\def\]{}
\def\({}
\def\){}
\def\&{and}
\def\_{_}
\def\%{percent}
\def\#{hash}
\def\"#1{#1}
\def\'#1{#1}
\def\c#1{#1}
\def\bar#1{#1}
\def\hat#1{#1}
\def\dot#1{#1}
\def\vec#1{#1}
\def\tilde#1{#1}
\def\widetilde#1{#1}
\def\widehat#1{#1}
\def\overline#1{#1}
\def\underline#1{#1}
\def\underset#1#2{#2}
\def\stackrel#1#2{#2}
\def\underbrace#1{#1}
\def\mathbbm#1{#1}
\def\binom#1#2{#1 #2}
\def\usetikzlibrary#1{}
\def\xspace{}
\makeatletter
\newcommand{\includegraphics}[2][]{[image]}
\newcommand{\caption}[2][]{#2}
\def\item{}
\newcommand{\bibitem}[2][]{}
\def\bibliographystyle#1{}
\def\bibliography#1{}
\def\DeclareMathOperator#1#2{\def#1{#2}}
\def\DeclareMathAlphabet#1#2#3#4#5{}
\def\mathrm#1{#1}
\def\mathbf#1{#1}
\def\mathsf#1{#1}
\def\mathit#1{#1}
\def\mathcal#1{#1}
\def\mathbb#1{#1}
\def\boldsymbol#1{#1}
\def\bm#1{#1}
\def\operatorname#1{#1}
\def\text#1{#1}
\def\frac#1#2{#1/#2}
\newcommand{\sqrt}[2][]{#2}
\def\multirow#1#2#3{#3}
\def\multicolumn#1#2#3{#3}
\def\shortstack#1{#1}
\def\parbox#1#2{#2}
\def\scalebox#1#2{#2}
\def\State{}
\def\Comment#1{}
\def\For#1{#1}
\def\EndFor{}
\def\If#1{#1}
\def\ElsIf#1{#1}
\def\Else{}
\def\EndIf{}
\def\While#1{#1}
\def\EndWhile{}
\def\Return#1{#1}
\def\Require#1{#1}
\def\Ensure#1{#1}
\def\algorithmicrequire{Input:}
\def\algorithmicensure{Output:}
\def\algrenewcommand#1#2{}
\def\\{ }
\def\@M{10000}
\def\@plus{ plus }
\def\@minus{ minus }
\def\@ne{1}
\def\z@{0}
\def\p@{1pt}
\def\m@ne{-1}
\def\@pnumwidth{1.55em}
\def\@tocrmarg{2.55em}
\def\@endpart{}
\def\@mkboth#1#2{}
\def\@startsection#1#2#3#4#5#6#7{#7}
\def\@dottedtocline#1#2#3#4#5{#4 #5}
\makeatother
\newcommand{\LaTeX}{LaTeX}
"##;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRunResult {
    pub toplevel: Utf8PathBuf,
    pub output: String,
    pub registers: BTreeMap<u32, i32>,
    pub transcript: Vec<String>,
    pub diagnostics: Vec<VmDiagnostic>,
    pub loaded_modules: Vec<Utf8PathBuf>,
    pub module_traces: Vec<VmModuleTrace>,
    pub module_checkpoints: Vec<VmModuleCheckpoint>,
    pub source_lengths: BTreeMap<Utf8PathBuf, usize>,
    pub body_source_start_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSourceSpan {
    pub file: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSyncSpan {
    pub file: Utf8PathBuf,
    pub start_utf8: u32,
    pub end_utf8: u32,
    pub output_start_utf8: u32,
    pub output_end_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectReplayCheckpoint {
    pub snapshot: VmSnapshot,
    pub resume_path: Utf8PathBuf,
    pub source_offset_utf8: u32,
    pub continuation_stack: Vec<VmReplayFrame>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectPageMeta {
    pub page_id: String,
    pub index: usize,
    pub width_pt: f32,
    pub height_pt: f32,
    pub content_hash: String,
    pub text_span: tex_layout::TextSpan,
    pub line_count: usize,
    pub source_spans: Vec<ProjectSourceSpan>,
    pub sync_spans: Vec<ProjectSyncSpan>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectPdfBuild {
    pub run: ProjectRunResult,
    pub layout: DocumentLayout,
    pub page_metadata: Vec<ProjectPageMeta>,
    pub pdf_bytes: Vec<u8>,
}

pub fn compile_mini_kernel_snapshot() -> VmSnapshot {
    let mut interner = ControlSequenceInterner::new();
    compile_format_snapshot(&mut interner, MINI_KERNEL_SOURCE)
}

pub fn run_project(world: &ProjectWorld) -> Result<ProjectRunResult> {
    let snapshot = compile_mini_kernel_snapshot();
    Ok(run_project_from_base_snapshot(world, &snapshot)?.0)
}

pub fn build_project_pdf(world: &ProjectWorld) -> Result<ProjectPdfBuild> {
    let snapshot = compile_mini_kernel_snapshot();
    Ok(run_project_pdf_from_base_snapshot(world, &snapshot)?.0)
}

pub fn build_project_pdf_with_snapshot(
    world: &ProjectWorld,
    snapshot: &VmSnapshot,
) -> Result<ProjectPdfBuild> {
    let run = run_project_with_snapshot(world, snapshot)?;
    Ok(build_project_pdf_from_run(run))
}

pub fn run_project_pdf_from_base_snapshot(
    world: &ProjectWorld,
    snapshot: &VmSnapshot,
) -> Result<(ProjectPdfBuild, ProjectReplayCheckpoint)> {
    run_project_pdf_from_base_snapshot_with_mounts(world, snapshot, &BTreeMap::new())
}

pub fn run_project_pdf_from_base_snapshot_with_mounts(
    world: &ProjectWorld,
    snapshot: &VmSnapshot,
    mounted_files: &BTreeMap<Utf8PathBuf, String>,
) -> Result<(ProjectPdfBuild, ProjectReplayCheckpoint)> {
    let (run, preamble_checkpoint) =
        run_project_from_base_snapshot_with_mounts(world, snapshot, mounted_files)?;
    Ok((build_project_pdf_from_run(run), preamble_checkpoint))
}

pub fn build_project_pdf_from_checkpoint(
    world: &ProjectWorld,
    checkpoint: &ProjectReplayCheckpoint,
    output_prefix: &str,
) -> Result<ProjectPdfBuild> {
    let run = run_project_from_checkpoint(world, checkpoint, output_prefix)?;
    Ok(build_project_pdf_from_run(run))
}

pub fn build_project_pdf_from_checkpoint_with_mounts(
    world: &ProjectWorld,
    checkpoint: &ProjectReplayCheckpoint,
    output_prefix: &str,
    mounted_files: &BTreeMap<Utf8PathBuf, String>,
) -> Result<ProjectPdfBuild> {
    let run =
        run_project_from_checkpoint_with_mounts(world, checkpoint, output_prefix, mounted_files)?;
    Ok(build_project_pdf_from_run(run))
}

pub fn capture_page_checkpoints(
    world: &ProjectWorld,
    checkpoint: &ProjectReplayCheckpoint,
    page_metadata: &[ProjectPageMeta],
) -> Result<Vec<ProjectReplayCheckpoint>> {
    let mut replay_frames = Vec::with_capacity(checkpoint.continuation_stack.len() + 1);
    let resume_source =
        fs::read_to_string(world.root.join(&checkpoint.resume_path)).with_context(|| {
            format!(
                "failed to read replay source {}",
                world.root.join(&checkpoint.resume_path)
            )
        })?;
    let resume_offset = align_char_boundary(&resume_source, checkpoint.source_offset_utf8 as usize);
    replay_frames.push((checkpoint.resume_path.clone(), resume_source, resume_offset));
    for frame in &checkpoint.continuation_stack {
        let source = fs::read_to_string(world.root.join(&frame.path)).with_context(|| {
            format!(
                "failed to read replay source {}",
                world.root.join(&frame.path)
            )
        })?;
        let offset = align_char_boundary(&source, frame.source_offset_utf8 as usize);
        replay_frames.push((frame.path.clone(), source, offset));
    }
    let mut checkpoints = Vec::with_capacity(page_metadata.len());
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::restore(&mut interner, &checkpoint.snapshot);
    vm.set_file_root(world.root.clone());
    let mut frame_index = 0usize;
    let mut last_resume_path = checkpoint.resume_path.clone();
    let mut last_source_offset_utf8 = checkpoint.source_offset_utf8;
    for (page_index, page) in page_metadata.iter().enumerate() {
        while frame_index < replay_frames.len() {
            let (path, source, current_offset) = &mut replay_frames[frame_index];
            let page_target = page
                .source_spans
                .iter()
                .filter(|span| span.file == *path)
                .map(|span| span.end_utf8)
                .max()
                .map(|target| align_char_boundary(source, target as usize));
            let future_target = page_metadata[page_index..]
                .iter()
                .flat_map(|candidate| candidate.source_spans.iter())
                .filter(|span| span.file == *path)
                .map(|span| span.end_utf8)
                .max()
                .map(|target| align_char_boundary(source, target as usize));
            if let Some(target_offset) = page_target.filter(|target| *target > *current_offset) {
                vm.set_entry_source_path(path.clone());
                let _ = vm.run_plain(&source[*current_offset..target_offset]);
                *current_offset = target_offset;
                last_resume_path = path.clone();
                last_source_offset_utf8 = *current_offset as u32;
            }
            let should_advance = match future_target {
                Some(target) => *current_offset >= target,
                None => true,
            };
            if !should_advance {
                break;
            }
            if *current_offset < source.len() {
                vm.set_entry_source_path(path.clone());
                let _ = vm.run_plain(&source[*current_offset..]);
                *current_offset = source.len();
                last_resume_path = path.clone();
                last_source_offset_utf8 = *current_offset as u32;
            }
            frame_index += 1;
        }
        let (resume_path, source_offset_utf8, continuation_stack) =
            if let Some((path, _, current_offset)) = replay_frames.get(frame_index) {
                (
                    path.clone(),
                    *current_offset as u32,
                    replay_frames[frame_index + 1..]
                        .iter()
                        .map(|(path, _, current_offset)| VmReplayFrame {
                            path: path.clone(),
                            source_offset_utf8: *current_offset as u32,
                        })
                        .collect(),
                )
            } else {
                (
                    last_resume_path.clone(),
                    last_source_offset_utf8,
                    Vec::new(),
                )
            };
        checkpoints.push(ProjectReplayCheckpoint {
            snapshot: vm.snapshot(),
            resume_path,
            source_offset_utf8,
            continuation_stack,
        });
    }
    Ok(checkpoints)
}

pub fn run_project_from_base_snapshot(
    world: &ProjectWorld,
    snapshot: &VmSnapshot,
) -> Result<(ProjectRunResult, ProjectReplayCheckpoint)> {
    run_project_from_base_snapshot_with_mounts(world, snapshot, &BTreeMap::new())
}

pub fn run_project_from_base_snapshot_with_mounts(
    world: &ProjectWorld,
    snapshot: &VmSnapshot,
    mounted_files: &BTreeMap<Utf8PathBuf, String>,
) -> Result<(ProjectRunResult, ProjectReplayCheckpoint)> {
    let (toplevel, source) = read_toplevel_source(world, mounted_files)?;
    let body_start = document_body_start(&source);
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::restore(&mut interner, snapshot);
    vm.set_file_root(world.root.clone());
    for (path, mounted_source) in mounted_files {
        vm.mount_file(path.clone(), mounted_source.clone());
    }
    vm.set_entry_source_path(toplevel.clone());
    let mut output = String::new();
    let mut transcript = Vec::new();
    let mut diagnostics = Vec::new();
    let mut module_traces = Vec::new();
    let mut module_checkpoints = Vec::new();
    if body_start > 0 {
        let preamble = vm.run_plain(&source[..body_start]);
        output.push_str(&preamble.output);
        transcript.extend(preamble.transcript);
        diagnostics.extend(preamble.diagnostics);
        module_traces.extend(preamble.module_traces);
        module_checkpoints.extend(preamble.module_checkpoints);
    }
    let preamble_checkpoint = ProjectReplayCheckpoint {
        snapshot: vm.snapshot(),
        resume_path: toplevel.clone(),
        source_offset_utf8: body_start as u32,
        continuation_stack: Vec::new(),
    };
    let outcome = vm.run_plain(&source[body_start..]);
    let output_prefix_len = output.len() as u32;
    output.push_str(&outcome.output);
    transcript.extend(outcome.transcript);
    diagnostics.extend(outcome.diagnostics);
    module_traces.extend(
        outcome
            .module_traces
            .into_iter()
            .map(|trace| VmModuleTrace {
                path: trace.path,
                source_start_utf8: trace.source_start_utf8,
                source_end_utf8: trace.source_end_utf8,
                output_start_utf8: trace.output_start_utf8 + output_prefix_len,
                output_end_utf8: trace.output_end_utf8 + output_prefix_len,
            }),
    );
    module_checkpoints.extend(outcome.module_checkpoints.into_iter().map(|checkpoint| {
        let resume_path = checkpoint.resume_path;
        let continuation_stack = checkpoint
            .continuation_stack
            .into_iter()
            .map(|frame| VmReplayFrame {
                path: frame.path.clone(),
                source_offset_utf8: if frame.path == toplevel {
                    frame.source_offset_utf8 + body_start as u32
                } else {
                    frame.source_offset_utf8
                },
            })
            .collect();
        VmModuleCheckpoint {
            kind: checkpoint.kind,
            module_path: checkpoint.module_path,
            resume_path: resume_path.clone(),
            source_offset_utf8: if resume_path.as_ref() == Some(&toplevel) {
                checkpoint.source_offset_utf8 + body_start as u32
            } else {
                checkpoint.source_offset_utf8
            },
            continuation_stack,
            output_start_utf8: checkpoint.output_start_utf8 + output_prefix_len,
            snapshot: checkpoint.snapshot,
        }
    }));
    let source_lengths = collect_source_lengths(
        world,
        mounted_files,
        &toplevel,
        &source,
        &outcome.loaded_modules,
    );

    Ok((
        ProjectRunResult {
            toplevel,
            output,
            registers: outcome.registers,
            transcript,
            diagnostics,
            loaded_modules: outcome.loaded_modules,
            module_traces,
            module_checkpoints,
            source_lengths,
            body_source_start_utf8: body_start as u32,
        },
        preamble_checkpoint,
    ))
}

pub fn run_project_from_checkpoint(
    world: &ProjectWorld,
    checkpoint: &ProjectReplayCheckpoint,
    output_prefix: &str,
) -> Result<ProjectRunResult> {
    run_project_from_checkpoint_with_mounts(world, checkpoint, output_prefix, &BTreeMap::new())
}

pub fn run_project_from_checkpoint_with_mounts(
    world: &ProjectWorld,
    checkpoint: &ProjectReplayCheckpoint,
    output_prefix: &str,
    mounted_files: &BTreeMap<Utf8PathBuf, String>,
) -> Result<ProjectRunResult> {
    let (toplevel, source) = read_toplevel_source(world, mounted_files)?;
    let body_start = document_body_start(&source);
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::restore(&mut interner, &checkpoint.snapshot);
    vm.set_file_root(world.root.clone());
    for (path, mounted_source) in mounted_files {
        vm.mount_file(path.clone(), mounted_source.clone());
    }
    let mut output = output_prefix.to_string();
    let mut registers = BTreeMap::new();
    let mut transcript = Vec::new();
    let mut diagnostics = Vec::new();
    let mut loaded_modules = Vec::new();
    let mut module_traces = Vec::new();
    let mut module_checkpoints = Vec::new();
    let mut replay_frames = Vec::with_capacity(checkpoint.continuation_stack.len() + 1);
    replay_frames.push(VmReplayFrame {
        path: checkpoint.resume_path.clone(),
        source_offset_utf8: checkpoint.source_offset_utf8,
    });
    replay_frames.extend(checkpoint.continuation_stack.clone());
    for frame in &replay_frames {
        let source_path = world.root.join(&frame.path);
        let source = if let Some(source) = mounted_files.get(&frame.path) {
            source.clone()
        } else {
            fs::read_to_string(source_path.as_std_path())
                .with_context(|| format!("failed to read replay source {source_path}"))?
        };
        let start_offset = align_char_boundary(&source, frame.source_offset_utf8 as usize);
        vm.set_entry_source_path(frame.path.clone());
        let output_prefix_len = output.len() as u32;
        let outcome = vm.run_plain(&source[start_offset..]);
        output.push_str(&outcome.output);
        let output_end_utf8 = output.len() as u32;
        registers = outcome.registers;
        transcript.extend(outcome.transcript);
        diagnostics.extend(outcome.diagnostics);
        loaded_modules = outcome.loaded_modules;
        if frame.path != toplevel && !loaded_modules.contains(&frame.path) {
            loaded_modules.push(frame.path.clone());
        }
        if frame.path != toplevel {
            module_traces.push(VmModuleTrace {
                path: frame.path.clone(),
                source_start_utf8: start_offset as u32,
                source_end_utf8: source.len() as u32,
                output_start_utf8: output_prefix_len,
                output_end_utf8,
            });
        }
        module_traces.extend(
            outcome
                .module_traces
                .into_iter()
                .map(|trace| VmModuleTrace {
                    path: trace.path,
                    source_start_utf8: trace.source_start_utf8,
                    source_end_utf8: trace.source_end_utf8,
                    output_start_utf8: trace.output_start_utf8 + output_prefix_len,
                    output_end_utf8: trace.output_end_utf8 + output_prefix_len,
                }),
        );
        module_checkpoints.extend(outcome.module_checkpoints.into_iter().map(|checkpoint| {
            VmModuleCheckpoint {
                kind: checkpoint.kind,
                module_path: checkpoint.module_path,
                resume_path: checkpoint.resume_path,
                source_offset_utf8: checkpoint.source_offset_utf8,
                continuation_stack: checkpoint.continuation_stack,
                output_start_utf8: checkpoint.output_start_utf8 + output_prefix_len,
                snapshot: checkpoint.snapshot,
            }
        }));
    }
    loaded_modules.sort();
    loaded_modules.dedup();
    let source_lengths =
        collect_source_lengths(world, mounted_files, &toplevel, &source, &loaded_modules);

    Ok(ProjectRunResult {
        toplevel,
        output,
        registers,
        transcript,
        diagnostics,
        loaded_modules,
        module_traces,
        module_checkpoints,
        source_lengths,
        body_source_start_utf8: body_start as u32,
    })
}

fn build_project_pdf_from_run(run: ProjectRunResult) -> ProjectPdfBuild {
    let layout = layout_text(&run.output, LayoutOptions::default());
    let output_len = run.output.len().max(1);
    let primary_source_len = run
        .source_lengths
        .get(&run.toplevel)
        .copied()
        .unwrap_or_default();
    let body_source_len = primary_source_len.saturating_sub(run.body_source_start_utf8 as usize);
    let mut attributed_segments = vec![(
        run.toplevel.clone(),
        run.body_source_start_utf8,
        primary_source_len as u32,
        0u32,
        run.output.len() as u32,
    )];
    for trace in &run.module_traces {
        let Some(length) = run.source_lengths.get(&trace.path) else {
            continue;
        };
        let trace_source_start = trace.source_start_utf8.min(*length as u32);
        let trace_source_end = if trace.source_end_utf8 > trace_source_start {
            trace.source_end_utf8.min(*length as u32)
        } else {
            *length as u32
        };
        if trace.output_end_utf8 > trace.output_start_utf8 && trace_source_end > trace_source_start
        {
            attributed_segments.push((
                trace.path.clone(),
                trace_source_start,
                trace_source_end,
                trace.output_start_utf8,
                trace.output_end_utf8,
            ));
        }
    }
    let mut child_intervals = BTreeMap::<Utf8PathBuf, Vec<(u32, u32, u32, u32)>>::new();
    let mut open_module_checkpoints = Vec::<&VmModuleCheckpoint>::new();
    for checkpoint in &run.module_checkpoints {
        match checkpoint.kind {
            VmModuleCheckpointKind::Enter => open_module_checkpoints.push(checkpoint),
            VmModuleCheckpointKind::Exit => {
                let Some(index) = open_module_checkpoints.iter().rposition(|candidate| {
                    candidate.module_path == checkpoint.module_path
                        && candidate.resume_path == checkpoint.resume_path
                        && candidate.continuation_stack == checkpoint.continuation_stack
                        && candidate.source_offset_utf8 <= checkpoint.source_offset_utf8
                        && candidate.output_start_utf8 <= checkpoint.output_start_utf8
                }) else {
                    continue;
                };
                let enter = open_module_checkpoints.remove(index);
                let Some(parent_path) = enter.resume_path.clone() else {
                    continue;
                };
                if checkpoint.source_offset_utf8 > enter.source_offset_utf8
                    && checkpoint.output_start_utf8 > enter.output_start_utf8
                {
                    child_intervals.entry(parent_path).or_default().push((
                        enter.source_offset_utf8,
                        checkpoint.source_offset_utf8,
                        enter.output_start_utf8,
                        checkpoint.output_start_utf8,
                    ));
                }
            }
        }
    }
    for intervals in child_intervals.values_mut() {
        intervals.sort_by_key(|(_, _, output_start_utf8, _)| *output_start_utf8);
    }
    let mut residual_segments = Vec::new();
    for (path, source_start, source_end, output_start, output_end) in attributed_segments {
        if source_end <= source_start || output_end <= output_start {
            continue;
        }
        let mut segments = vec![(source_start, source_end, output_start, output_end)];
        if let Some(intervals) = child_intervals.get(&path) {
            for &(child_source_start, child_source_end, child_output_start, child_output_end) in
                intervals
            {
                let mut next_segments = Vec::new();
                for (
                    segment_source_start,
                    segment_source_end,
                    segment_output_start,
                    segment_output_end,
                ) in segments
                {
                    if child_source_end <= segment_source_start
                        || child_source_start >= segment_source_end
                        || child_output_end <= segment_output_start
                        || child_output_start >= segment_output_end
                    {
                        next_segments.push((
                            segment_source_start,
                            segment_source_end,
                            segment_output_start,
                            segment_output_end,
                        ));
                        continue;
                    }
                    if child_source_start > segment_source_start
                        && child_output_start > segment_output_start
                    {
                        next_segments.push((
                            segment_source_start,
                            child_source_start,
                            segment_output_start,
                            child_output_start,
                        ));
                    }
                    if child_source_end < segment_source_end
                        && child_output_end < segment_output_end
                    {
                        next_segments.push((
                            child_source_end,
                            segment_source_end,
                            child_output_end,
                            segment_output_end,
                        ));
                    }
                }
                segments = next_segments;
            }
        }
        residual_segments.extend(
            segments
                .into_iter()
                .filter(|(source_start, source_end, output_start, output_end)| {
                    source_end > source_start && output_end > output_start
                })
                .map(|(source_start, source_end, output_start, output_end)| {
                    (
                        path.clone(),
                        source_start,
                        source_end,
                        output_start,
                        output_end,
                    )
                }),
        );
    }
    let page_metadata = layout
        .pages
        .iter()
        .map(|page| {
            let page_output_end = layout
                .pages
                .get(page.index + 1)
                .map(|next_page| next_page.text_span.start_utf8)
                .unwrap_or(run.output.len() as u32);
            let mut ordered_source_spans = Vec::<(u32, ProjectSourceSpan)>::new();
            let mut ordered_sync_spans = Vec::<ProjectSyncSpan>::new();
            for (path, source_start, source_end, output_start, output_end) in &residual_segments {
                if *output_end <= page.text_span.start_utf8 || *output_start >= page_output_end {
                    continue;
                }
                let overlap_start = (*output_start).max(page.text_span.start_utf8);
                let overlap_end = (*output_end).min(page_output_end);
                if overlap_start >= overlap_end {
                    continue;
                }
                let segment_source_len = source_end.saturating_sub(*source_start).max(1);
                let segment_output_len = output_end.saturating_sub(*output_start).max(1);
                let start_utf8 = *source_start
                    + ((segment_source_len as u64 * (overlap_start - *output_start) as u64)
                        / segment_output_len as u64) as u32;
                let end_utf8 = if overlap_end == *output_end {
                    *source_end
                } else {
                    *source_start
                        + ((segment_source_len as u64 * (overlap_end - *output_start) as u64)
                            / segment_output_len as u64) as u32
                };
                ordered_source_spans.push((
                    overlap_start,
                    ProjectSourceSpan {
                        file: path.clone(),
                        start_utf8,
                        end_utf8,
                    },
                ));
                ordered_sync_spans.push(ProjectSyncSpan {
                    file: path.clone(),
                    start_utf8,
                    end_utf8,
                    output_start_utf8: overlap_start - page.text_span.start_utf8,
                    output_end_utf8: overlap_end - page.text_span.start_utf8,
                });
            }
            for trace in &run.module_traces {
                let Some(length) = run.source_lengths.get(&trace.path) else {
                    continue;
                };
                let trace_source_start = trace.source_start_utf8.min(*length as u32);
                let trace_source_end = if trace.source_end_utf8 > trace_source_start {
                    trace.source_end_utf8.min(*length as u32)
                } else {
                    *length as u32
                };
                if trace.output_end_utf8 > trace.output_start_utf8
                    || trace.output_start_utf8 < page.text_span.start_utf8
                    || trace.output_start_utf8 >= page_output_end
                {
                    continue;
                }
                ordered_source_spans.push((
                    trace.output_start_utf8,
                    ProjectSourceSpan {
                        file: trace.path.clone(),
                        start_utf8: trace_source_start,
                        end_utf8: trace_source_end,
                    },
                ));
            }
            if ordered_source_spans.is_empty()
                && primary_source_len > run.body_source_start_utf8 as usize
            {
                let start_utf8 = run.body_source_start_utf8
                    + ((body_source_len * page.text_span.start_utf8 as usize) / output_len) as u32;
                let end_utf8 = if page.index + 1 == layout.pages.len() {
                    primary_source_len as u32
                } else {
                    run.body_source_start_utf8
                        + ((body_source_len * page.text_span.end_utf8 as usize) / output_len) as u32
                };
                ordered_source_spans.push((
                    page.text_span.start_utf8,
                    ProjectSourceSpan {
                        file: run.toplevel.clone(),
                        start_utf8,
                        end_utf8,
                    },
                ));
                ordered_sync_spans.push(ProjectSyncSpan {
                    file: run.toplevel.clone(),
                    start_utf8,
                    end_utf8,
                    output_start_utf8: 0,
                    output_end_utf8: page_output_end.saturating_sub(page.text_span.start_utf8),
                });
            }
            ordered_source_spans.sort_by_key(|(output_start, _)| *output_start);
            let mut source_spans = Vec::<ProjectSourceSpan>::new();
            for (_, span) in ordered_source_spans {
                if let Some(previous) = source_spans.last_mut() {
                    if previous.file == span.file && previous.end_utf8 >= span.start_utf8 {
                        previous.end_utf8 = previous.end_utf8.max(span.end_utf8);
                        continue;
                    }
                }
                source_spans.push(span);
            }
            ordered_sync_spans.sort_by_key(|span| span.output_start_utf8);
            let mut sync_spans = Vec::<ProjectSyncSpan>::new();
            for span in ordered_sync_spans {
                if let Some(previous) = sync_spans.last_mut() {
                    if previous.file == span.file
                        && previous.output_end_utf8 >= span.output_start_utf8
                        && previous.end_utf8 >= span.start_utf8
                    {
                        previous.end_utf8 = previous.end_utf8.max(span.end_utf8);
                        previous.output_end_utf8 =
                            previous.output_end_utf8.max(span.output_end_utf8);
                        continue;
                    }
                }
                sync_spans.push(span);
            }
            ProjectPageMeta {
                page_id: page.page_id.clone(),
                index: page.index,
                width_pt: page.width_pt,
                height_pt: page.height_pt,
                content_hash: page.content_hash.clone(),
                text_span: page.text_span.clone(),
                line_count: page.lines.len(),
                source_spans,
                sync_spans,
            }
        })
        .collect();
    let pdf_bytes = render_pdf(&layout);

    ProjectPdfBuild {
        run,
        layout,
        page_metadata,
        pdf_bytes,
    }
}

pub fn run_project_with_snapshot(
    world: &ProjectWorld,
    snapshot: &VmSnapshot,
) -> Result<ProjectRunResult> {
    let (toplevel, source) = read_toplevel_source(world, &BTreeMap::new())?;
    let mut interner = ControlSequenceInterner::new();
    let mut vm = Vm::restore(&mut interner, snapshot);
    vm.set_file_root(world.root.clone());
    vm.set_entry_source_path(toplevel.clone());
    let outcome = vm.run_plain(&source);
    let source_lengths = collect_source_lengths(
        world,
        &BTreeMap::new(),
        &toplevel,
        &source,
        &outcome.loaded_modules,
    );

    Ok(ProjectRunResult {
        toplevel,
        output: outcome.output,
        registers: outcome.registers,
        transcript: outcome.transcript,
        diagnostics: outcome.diagnostics,
        loaded_modules: outcome.loaded_modules,
        module_traces: outcome.module_traces,
        module_checkpoints: outcome.module_checkpoints,
        source_lengths,
        body_source_start_utf8: 0,
    })
}

fn read_toplevel_source(
    world: &ProjectWorld,
    mounted_files: &BTreeMap<Utf8PathBuf, String>,
) -> Result<(Utf8PathBuf, String)> {
    let Some(toplevel) = world.manifest.toplevels.first().cloned() else {
        bail!("project manifest does not declare a toplevel document");
    };
    let source = if let Some(source) = mounted_files.get(&toplevel) {
        source.clone()
    } else {
        fs::read_to_string(world.root.join(&toplevel))
            .with_context(|| format!("failed to read toplevel {}", world.root.join(&toplevel)))?
    };
    Ok((toplevel, source))
}

fn collect_source_lengths(
    world: &ProjectWorld,
    mounted_files: &BTreeMap<Utf8PathBuf, String>,
    toplevel: &Utf8PathBuf,
    source: &str,
    loaded_modules: &[Utf8PathBuf],
) -> BTreeMap<Utf8PathBuf, usize> {
    let mut source_lengths = BTreeMap::new();
    source_lengths.insert(toplevel.clone(), source.len());
    for module in loaded_modules {
        if let Some(text) = mounted_files.get(module) {
            source_lengths.insert(module.clone(), text.len());
        } else if let Ok(text) = fs::read_to_string(world.root.join(module)) {
            source_lengths.insert(module.clone(), text.len());
        }
    }
    source_lengths
}

fn document_body_start(source: &str) -> usize {
    source
        .find(r"\begin{document}")
        .map(|offset| offset + r"\begin{document}".len())
        .unwrap_or(0)
}

fn align_char_boundary(source: &str, requested_offset: usize) -> usize {
    let mut offset = requested_offset.min(source.len());
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use std::fs;

    use camino::Utf8PathBuf;
    use tempfile::tempdir;
    use tex_vm::{VmDiagnosticKind, VmReplayFrame};
    use tex_world::ProjectWorld;

    use super::{
        build_project_pdf, capture_page_checkpoints, compile_mini_kernel_snapshot, run_project,
        run_project_from_checkpoint, run_project_with_snapshot,
    };

    #[test]
    fn mini_kernel_snapshot_is_reusable_across_runs() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("article.cls"), "\\def\\classmark{class}").expect("class");
        fs::write(root.join("pkg.sty"), "\\def\\pkgmark{pkg}").expect("package");
        fs::write(
            root.join("main.tex"),
            "\\documentclass{article}\\usepackage{pkg}\\begin{document}\\classmark\\pkgmark\\section{Hi}\\end{document}",
        )
        .expect("main");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let snapshot = compile_mini_kernel_snapshot();
        let first = run_project_with_snapshot(&world, &snapshot).expect("first run");
        let second = run_project_with_snapshot(&world, &snapshot).expect("second run");

        assert_eq!(first.output, "classpkgHi");
        assert_eq!(first.output, second.output);
        assert_eq!(first.toplevel, Utf8PathBuf::from("main.tex"));
        assert!(first.diagnostics.is_empty());
        assert!(
            first
                .source_lengths
                .contains_key(&Utf8PathBuf::from("main.tex"))
        );
    }

    #[test]
    fn project_runner_loads_local_class_and_package_files() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - paper.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("local.cls"), "\\def\\classmark{C}").expect("class");
        fs::write(root.join("alpha.sty"), "\\def\\packmark{A}").expect("alpha");
        fs::write(root.join("beta.sty"), "\\def\\packmark{B}").expect("beta");
        fs::write(
            root.join("paper.tex"),
            "\\documentclass[11pt]{local}\\usepackage{alpha,beta}\\classmark\\packmark",
        )
        .expect("paper");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let result = run_project(&world).expect("project run");

        assert_eq!(result.output, "CB");
        assert!(result.diagnostics.is_empty());
        assert_eq!(
            result.transcript,
            vec![
                "class local.cls",
                "def \\classmark #0",
                "package alpha.sty",
                "package beta.sty",
                "def \\packmark #0",
                "def \\packmark #0",
            ]
        );
    }

    #[test]
    fn project_runner_reports_missing_local_package() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - paper.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("local.cls"), "\\def\\classmark{C}").expect("class");
        fs::write(
            root.join("paper.tex"),
            "\\documentclass{local}\\usepackage{ghost}\\classmark",
        )
        .expect("paper");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let result = run_project(&world).expect("project run");

        assert_eq!(result.output, "C");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].kind, VmDiagnosticKind::MissingFile);
        assert_eq!(result.diagnostics[0].detail, "package ghost.sty");
    }

    #[test]
    fn project_runner_reports_undefined_control_sequences() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - paper.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("paper.tex"), "\\LaTeX\\UnknownCommand").expect("paper");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let result = run_project(&world).expect("project run");

        assert_eq!(result.output, "LaTeX\\UnknownCommand");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics[0].kind,
            VmDiagnosticKind::UndefinedControlSequence
        );
        assert_eq!(result.diagnostics[0].detail, "UnknownCommand");
    }

    #[test]
    fn project_runner_can_render_internal_pdf() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - paper.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("paper.tex"), "\\LaTeX\\section{Hello world}").expect("paper");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let build = build_project_pdf(&world).expect("project pdf");
        let pdf_text = String::from_utf8_lossy(&build.pdf_bytes);

        assert_eq!(build.run.output, "LaTeXHello world");
        assert_eq!(build.layout.pages.len(), 1);
        assert_eq!(build.page_metadata.len(), 1);
        assert_eq!(
            build.page_metadata[0].source_spans[0].file,
            Utf8PathBuf::from("paper.tex")
        );
        assert_eq!(build.page_metadata[0].line_count, 1);
        assert!(pdf_text.starts_with("%PDF-1.4"));
        assert!(pdf_text.contains("/Type /Page"));
    }

    #[test]
    fn project_pdf_build_populates_source_spans_for_loaded_modules() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - paper.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("local.cls"), "\\def\\classmark{C}").expect("class");
        fs::write(root.join("pkg.sty"), "\\def\\pkgmark{P}").expect("package");
        fs::write(
            root.join("paper.tex"),
            "\\documentclass{local}\\usepackage{pkg}\\classmark\\pkgmark",
        )
        .expect("paper");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let build = build_project_pdf(&world).expect("project pdf");

        assert_eq!(build.page_metadata.len(), 1);
        assert_eq!(build.page_metadata[0].width_pt, 612.0);
        assert_eq!(build.page_metadata[0].height_pt, 792.0);
        assert_eq!(
            build.page_metadata[0]
                .source_spans
                .iter()
                .map(|span| span.file.clone())
                .collect::<Vec<_>>(),
            vec![
                Utf8PathBuf::from("paper.tex"),
                Utf8PathBuf::from("local.cls"),
                Utf8PathBuf::from("pkg.sty"),
            ]
        );
        assert!(!build.page_metadata[0].content_hash.is_empty());
    }

    #[test]
    fn input_source_spans_attach_to_the_page_where_input_starts() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - paper.tex\n",
        )
        .expect("manifest");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/tail.tex"), "tail-input-line").expect("tail input");
        let mut words = (0..1800)
            .map(|index| format!("word{index:04}"))
            .collect::<Vec<_>>();
        words.insert(1500, "\\input{sections/tail}".to_string());
        fs::write(root.join("paper.tex"), words.join(" ")).expect("paper");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let build = build_project_pdf(&world).expect("project pdf");

        assert!(build.page_metadata.len() > 2);
        assert!(
            !build.page_metadata[0]
                .source_spans
                .iter()
                .any(|span| span.file == Utf8PathBuf::from("sections/tail.tex"))
        );
        let input_page = build
            .page_metadata
            .iter()
            .find(|page| {
                page.source_spans
                    .iter()
                    .any(|span| span.file == Utf8PathBuf::from("sections/tail.tex"))
            })
            .expect("input source span page");
        assert!(input_page.index >= 2);
    }

    #[test]
    fn nested_input_source_spans_split_parent_ranges_around_child_output() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("main.tex"), "A\\input{parent}Z").expect("main");
        fs::write(root.join("parent.tex"), "B\\input{child}C").expect("parent");
        fs::write(root.join("child.tex"), "D").expect("child");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let build = build_project_pdf(&world).expect("project pdf");

        assert_eq!(build.page_metadata.len(), 1);
        assert_eq!(
            build.page_metadata[0]
                .source_spans
                .iter()
                .map(|span| span.file.clone())
                .collect::<Vec<_>>(),
            vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("parent.tex"),
                Utf8PathBuf::from("child.tex"),
                Utf8PathBuf::from("parent.tex"),
                Utf8PathBuf::from("main.tex"),
            ]
        );
        let main_spans = build.page_metadata[0]
            .source_spans
            .iter()
            .filter(|span| span.file == Utf8PathBuf::from("main.tex"))
            .collect::<Vec<_>>();
        let parent_spans = build.page_metadata[0]
            .source_spans
            .iter()
            .filter(|span| span.file == Utf8PathBuf::from("parent.tex"))
            .collect::<Vec<_>>();
        assert_eq!(main_spans.len(), 2);
        assert_eq!(parent_spans.len(), 2);
        assert!(main_spans[0].end_utf8 < main_spans[1].start_utf8);
        assert!(parent_spans[0].end_utf8 < parent_spans[1].start_utf8);
    }

    #[test]
    fn base_snapshot_run_preserves_file_local_offsets_for_nested_checkpoints() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("article.cls"), "").expect("class");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/child.tex"), "nested").expect("child");
        let parent_source = "before \\input{sections/child} after";
        fs::write(root.join("sections/parent.tex"), parent_source).expect("parent");
        let main_source = "\\documentclass{article}\\begin{document} prefix \\input{sections/parent} suffix \\end{document}";
        fs::write(root.join("main.tex"), main_source).expect("main");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let run = run_project(&world).expect("run");
        let child_exit = run
            .module_checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.kind == tex_vm::VmModuleCheckpointKind::Exit
                    && checkpoint.module_path == Utf8PathBuf::from("sections/child.tex")
            })
            .expect("child exit checkpoint");

        assert_eq!(
            child_exit.resume_path,
            Some(Utf8PathBuf::from("sections/parent.tex"))
        );
        assert_eq!(
            child_exit.source_offset_utf8,
            parent_source.find(" after").expect("parent after offset") as u32
        );
        assert_eq!(
            child_exit.continuation_stack,
            vec![VmReplayFrame {
                path: Utf8PathBuf::from("main.tex"),
                source_offset_utf8: main_source.find(" suffix").expect("main suffix offset") as u32,
            }]
        );
    }

    #[test]
    fn replay_checkpoint_can_resume_nested_input_exit() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - paper.tex\n",
        )
        .expect("manifest");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/child.tex"), "D").expect("child");
        fs::write(
            root.join("sections/parent.tex"),
            "B\\input{sections/child}C",
        )
        .expect("parent");
        fs::write(root.join("paper.tex"), "A\\input{sections/parent}Z").expect("paper");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let full = run_project(&world).expect("full run");
        let exit_checkpoint = full
            .module_checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.kind == tex_vm::VmModuleCheckpointKind::Exit
                    && checkpoint.module_path == Utf8PathBuf::from("sections/child.tex")
            })
            .expect("child exit checkpoint");

        let replayed = run_project_from_checkpoint(
            &world,
            &super::ProjectReplayCheckpoint {
                snapshot: exit_checkpoint.snapshot.clone(),
                resume_path: exit_checkpoint.resume_path.clone().expect("resume path"),
                source_offset_utf8: exit_checkpoint.source_offset_utf8,
                continuation_stack: exit_checkpoint.continuation_stack.clone(),
            },
            &full.output[..exit_checkpoint.output_start_utf8 as usize],
        )
        .expect("replayed run");

        assert_eq!(replayed.output, full.output);
        assert!(
            replayed
                .loaded_modules
                .contains(&Utf8PathBuf::from("sections/parent.tex"))
        );
        let resumed_parent_trace = replayed
            .module_traces
            .iter()
            .find(|trace| trace.path == Utf8PathBuf::from("sections/parent.tex"))
            .expect("resumed parent trace");
        assert_eq!(
            resumed_parent_trace.source_start_utf8,
            exit_checkpoint.source_offset_utf8
        );
        assert_eq!(
            resumed_parent_trace.source_end_utf8,
            "B\\input{sections/child}C".len() as u32
        );
    }

    #[test]
    fn capture_page_checkpoints_tracks_nested_replay_frames() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - paper.tex\n",
        )
        .expect("manifest");
        fs::create_dir_all(root.join("sections")).expect("sections dir");
        fs::write(root.join("sections/child.tex"), "D").expect("child");
        let mut parent_words = (0..1600)
            .map(|index| format!("word{index:04}"))
            .collect::<Vec<_>>();
        parent_words.insert(400, "\\input{sections/child}".to_string());
        fs::write(root.join("sections/parent.tex"), parent_words.join(" ")).expect("parent");
        fs::write(root.join("paper.tex"), "A \\input{sections/parent} Z").expect("paper");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let build = build_project_pdf(&world).expect("project pdf");
        let exit_checkpoint = build
            .run
            .module_checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.kind == tex_vm::VmModuleCheckpointKind::Exit
                    && checkpoint.module_path == Utf8PathBuf::from("sections/child.tex")
            })
            .expect("child exit checkpoint");
        let start_page_index = build
            .page_metadata
            .iter()
            .position(|page| page.text_span.end_utf8 > exit_checkpoint.output_start_utf8)
            .expect("suffix start page");
        let captured = capture_page_checkpoints(
            &world,
            &super::ProjectReplayCheckpoint {
                snapshot: exit_checkpoint.snapshot.clone(),
                resume_path: exit_checkpoint.resume_path.clone().expect("resume path"),
                source_offset_utf8: exit_checkpoint.source_offset_utf8,
                continuation_stack: exit_checkpoint.continuation_stack.clone(),
            },
            &build.page_metadata[start_page_index..],
        )
        .expect("captured checkpoints");

        assert_eq!(captured.len(), build.page_metadata.len() - start_page_index);
        assert_eq!(
            captured[0].resume_path,
            Utf8PathBuf::from("sections/parent.tex")
        );
    }

    #[test]
    fn multipage_project_pdf_has_monotonic_page_metadata() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - paper.tex\n",
        )
        .expect("manifest");
        let body = (0..1200)
            .map(|index| format!("line{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(root.join("paper.tex"), body).expect("paper");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let build = build_project_pdf(&world).expect("project pdf");

        assert!(build.page_metadata.len() > 1);
        for window in build.page_metadata.windows(2) {
            assert!(window[0].index < window[1].index);
            assert!(window[0].text_span.end_utf8 < window[1].text_span.start_utf8);
            assert_ne!(window[0].page_id, window[1].page_id);
        }
    }

    #[test]
    fn project_pdf_metadata_is_stable_for_same_input() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - paper.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("paper.tex"), "\\LaTeX\\section{Stable metadata}").expect("paper");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let left = build_project_pdf(&world).expect("left build");
        let right = build_project_pdf(&world).expect("right build");

        assert_eq!(left.page_metadata, right.page_metadata);
        assert_eq!(left.layout.pages[0].page_id, right.layout.pages[0].page_id);
    }

    #[test]
    fn loaded_modules_and_source_lengths_are_deterministic() {
        let tempdir = tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
        fs::write(
            root.join("00README.yaml"),
            "compiler: pdf_latex\ntoplevel:\n  - paper.tex\n",
        )
        .expect("manifest");
        fs::write(root.join("zeta.sty"), "\\def\\zeta{z}").expect("zeta");
        fs::write(root.join("alpha.sty"), "\\def\\alpha{a}").expect("alpha");
        fs::write(
            root.join("paper.tex"),
            "\\usepackage{zeta,alpha}\\zeta\\alpha",
        )
        .expect("paper");

        let world = ProjectWorld::load(root.clone()).expect("world");
        let result = run_project(&world).expect("project run");

        assert_eq!(
            result.loaded_modules,
            vec![
                Utf8PathBuf::from("alpha.sty"),
                Utf8PathBuf::from("zeta.sty")
            ]
        );
        assert_eq!(
            result.source_lengths[&Utf8PathBuf::from("alpha.sty")],
            "\\def\\alpha{a}".len()
        );
        assert_eq!(
            result.source_lengths[&Utf8PathBuf::from("zeta.sty")],
            "\\def\\zeta{z}".len()
        );
    }
}
