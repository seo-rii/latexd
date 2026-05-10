struct InternalBaselinesFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
}

enum InternalBaselinesMetadataAndReuseCase {
    PreambleReuse,
    MultipageMetadata,
}

enum InternalBaselinesBibliographyAndTocCase {
    BibliographyStemOrder,
    StarredSectionToc,
}

enum InternalBaselinesSourceAndFailureCase {
    SourceSpans,
    VmDiagnostics,
}

fn prepare_internal_baselines_fixture(
    main_source: &str,
    extra_files: &[(&str, &str)],
) -> InternalBaselinesFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    fs::write(
        root.join("00README.yaml"),
        r#"
compiler: pdf_latex
toplevel:
  - main.tex
"#,
    )
    .expect("write manifest");
    fs::write(root.join("article.cls"), "").expect("write class");
    fs::write(root.join("main.tex"), main_source).expect("write main");
    for (path, contents) in extra_files {
        fs::write(root.join(path), contents).expect("write extra file");
    }

    let world = ProjectWorld::load(root.clone()).expect("world");
    let build_root = root.join(".latexd/build");
    InternalBaselinesFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
    }
}

async fn compile_internal_baselines_fixture(
    fixture: &InternalBaselinesFixture,
    changed_files: &[&str],
) -> Result<CompileOutcome, latexd::compiler::CompileFailure> {
    compile_internal_baselines_fixture_rev(fixture, 1, changed_files).await
}

fn internal_baselines_changed_files(paths: &[&str]) -> Vec<Utf8PathBuf> {
    paths.iter().map(|path| Utf8PathBuf::from(*path)).collect()
}

async fn compile_internal_baselines_fixture_rev(
    fixture: &InternalBaselinesFixture,
    rev: u64,
    changed_files: &[&str],
) -> Result<CompileOutcome, latexd::compiler::CompileFailure> {
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev,
            build_root: fixture.build_root.clone(),
            changed_files: internal_baselines_changed_files(changed_files),
        })
        .await
}

async fn run_internal_baselines_source_and_failure(case: InternalBaselinesSourceAndFailureCase) {
    match case {
        InternalBaselinesSourceAndFailureCase::SourceSpans => {
            let fixture = prepare_internal_baselines_fixture(
                "A\\input{parent}Z",
                &[("parent.tex", "B\\input{child}C"), ("child.tex", "D")],
            );
            let outcome = compile_internal_baselines_fixture(&fixture, &["main.tex"])
                .await
                .expect("internal build should succeed");

            assert_eq!(outcome.page_metadata.len(), 1);
            assert_eq!(
                outcome.page_metadata[0]
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
        }
        InternalBaselinesSourceAndFailureCase::VmDiagnostics => {
            let fixture = prepare_internal_baselines_fixture("\\UnknownCommand", &[]);
            let failure = compile_internal_baselines_fixture(&fixture, &["main.tex"])
                .await
                .expect_err("internal build should fail on VM diagnostics");

            assert_eq!(failure.diagnostics.len(), 1);
            assert!(failure.diagnostics[0].message.contains("UnknownCommand"));
        }
    }
}

type BaseSource = InternalBaselinesSourceAndFailureCase;
type BaseMeta = InternalBaselinesMetadataAndReuseCase;
type BaseBibToc = InternalBaselinesBibliographyAndTocCase;

async fn run_base_source(case: BaseSource) {
    run_internal_baselines_source_and_failure(case).await;
}

async fn run_base_meta(case: BaseMeta) {
    run_internal_baselines_metadata_and_reuse(case).await;
}

async fn run_base_bib_toc(case: BaseBibToc) {
    run_internal_baselines_bibliography_and_toc(case).await;
}

async fn run_internal_baselines_metadata_and_reuse(case: InternalBaselinesMetadataAndReuseCase) {
    match case {
        InternalBaselinesMetadataAndReuseCase::PreambleReuse => {
            let fixture = prepare_internal_baselines_fixture(
                "\\documentclass{article}\\title{A}\\begin{document}\\classmark first body\\end{document}",
                &[("article.cls", "\\def\\classmark{class}")],
            );

            let first = compile_internal_baselines_fixture(&fixture, &["main.tex"])
                .await
                .expect("first internal build should succeed");
            assert!(first.reused_checkpoint_id.is_none());
            let first_checkpoints =
                load_checkpoint_bundle(&fixture.build_root.join("rev-1/checkpoints.json"))
                    .expect("load rev1 checkpoints");
            let first_preamble_id = first_checkpoints.checkpoints[0].meta.checkpoint_id.clone();
            let assert_plain_rebuild_meta = |rev, start_checkpoint_id| {
                let path = fixture
                    .build_root
                    .join(format!("rev-{rev}/build-meta.json"));
                let build_meta =
                    serde_json::from_slice::<BuildMeta>(&fs::read(path).expect("read build meta"))
                        .expect("parse build meta");
                assert_eq!(build_meta.dirty_files, vec![Utf8PathBuf::from("main.tex")]);
                assert_eq!(build_meta.start_checkpoint_id, start_checkpoint_id);
                assert_eq!(build_meta.start_page_index, 0);
                assert_eq!(build_meta.semantic_pass_count, 0);
                assert_eq!(build_meta.semantic_rerun_count, 0);
                assert!(build_meta.semantic_fixpoint_reached);
                assert!(!build_meta.semantic_aux_backdated);
            };

            fs::write(
                fixture.root.join("main.tex"),
                "\\documentclass{article}\\title{A}\\begin{document}\\classmark second body\\end{document}",
            )
            .expect("rewrite main tex body");
            let second = compile_internal_baselines_fixture_rev(&fixture, 2, &["main.tex"])
                .await
                .expect("body-only rebuild should succeed");
            assert_eq!(second.reused_checkpoint_id, Some(first_preamble_id.clone()));
            assert_plain_rebuild_meta(2, Some(first_preamble_id.clone()));

            fs::write(
                fixture.root.join("main.tex"),
                "\\documentclass{article}\\title{B}\\begin{document}\\classmark third body\\end{document}",
            )
            .expect("rewrite main tex preamble");
            let third = compile_internal_baselines_fixture_rev(&fixture, 3, &["main.tex"])
                .await
                .expect("preamble rebuild should succeed");
            assert!(third.reused_checkpoint_id.is_none());
            assert_plain_rebuild_meta(3, None);
        }
        InternalBaselinesMetadataAndReuseCase::MultipageMetadata => {
            let source = (0..1200)
                .map(|index| format!("line{index}"))
                .collect::<Vec<_>>()
                .join("\n");
            let fixture = prepare_internal_baselines_fixture(&source, &[]);

            let outcome = compile_internal_baselines_fixture_rev(&fixture, 3, &["main.tex"])
                .await
                .expect("internal build should succeed");

            assert!(outcome.page_metadata.len() > 1);
            for window in outcome.page_metadata.windows(2) {
                assert!(window[0].index < window[1].index);
                assert!(window[0].text_end_utf8 < window[1].text_start_utf8);
                assert_ne!(window[0].page_id, window[1].page_id);
            }

            let checkpoints =
                load_checkpoint_bundle(&fixture.build_root.join("rev-3/checkpoints.json"))
                    .expect("load checkpoints");
            assert_eq!(
                checkpoints.checkpoints.len(),
                outcome.page_metadata.len() + 1
            );
        }
    }
}

async fn run_internal_baselines_bibliography_and_toc(
    case: InternalBaselinesBibliographyAndTocCase,
) {
    match case {
        InternalBaselinesBibliographyAndTocCase::BibliographyStemOrder => {
            let fixture = prepare_internal_baselines_fixture(
                "\\documentclass{article}\\begin{document}See \\cite{beta} then \\cite{alpha}.\\bibliography{refsb,refsa}\\end{document}",
                &[
                    (
                        "refsb.bbl",
                        "\\begin{thebibliography}{1}\\bibitem{beta} Beta entry.\\end{thebibliography}",
                    ),
                    (
                        "refsa.bbl",
                        "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
                    ),
                ],
            );
            compile_internal_baselines_fixture(&fixture, &["main.tex", "refsb.bbl", "refsa.bbl"])
                .await
                .expect("semantic aux build should succeed");

            let output = fs::read_to_string(fixture.build_root.join("rev-1/output.txt"))
                .expect("read output");
            assert!(output.contains("See [1] then [2]."));

            let aux =
                load_semantic_aux(&fixture.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(aux.bibliography.len(), 2);
            assert_eq!(aux.bibliography[0].key, "beta");
            assert_eq!(aux.bibliography[1].key, "alpha");
        }
        InternalBaselinesBibliographyAndTocCase::StarredSectionToc => {
            let fixture = prepare_internal_baselines_fixture(
                "\\documentclass{article}\\begin{document}\\tableofcontents\\section*{Prelude}\\section{Intro}\\label{sec:intro}See \\ref{sec:intro}.\\end{document}",
                &[],
            );
            compile_internal_baselines_fixture(&fixture, &["main.tex"])
                .await
                .expect("semantic aux build should succeed");

            let output = fs::read_to_string(fixture.build_root.join("rev-1/output.txt"))
                .expect("read output");
            assert!(output.contains("Contents"));
            assert!(output.contains("Prelude"));
            assert!(output.contains("1 Intro"));
            assert!(!output.contains("Prelude ...."));
            assert!(output.contains("See 1."));

            let aux =
                load_semantic_aux(&fixture.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(aux.toc.len(), 1);
            assert_eq!(aux.toc[0].title, "Intro");
            assert_eq!(aux.toc[0].number, "1");

            let stored_sources = serde_json::from_slice::<StoredSources>(
                &fs::read(fixture.build_root.join("rev-1/sources.json")).expect("read sources"),
            )
            .expect("parse sources");
            let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
            assert!(executed_main.contains("Prelude"));
            assert!(!executed_main.contains("\\section*"));
        }
    }
}
