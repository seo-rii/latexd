struct BibliographyFeaturesSemanticCaseFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
}

enum BibliographyFeaturesSemanticCase {
    CoreAux,
    NatexlabOutput,
}

fn prepare_bibliography_features_semantic_case_fixture(
    main_source: &str,
    refs_source: &str,
) -> BibliographyFeaturesSemanticCaseFixture {
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
    fs::write(root.join("refs.bbl"), refs_source).expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let build_root = root.join(".latexd/build");
    BibliographyFeaturesSemanticCaseFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
    }
}

async fn compile_bibliography_features_semantic_case_fixture(
    fixture: &BibliographyFeaturesSemanticCaseFixture,
) -> Result<CompileOutcome, latexd::compiler::CompileFailure> {
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let result = driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: fixture.build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await;
    if let Ok(outcome) = &result {
        assert!(
            outcome
                .dep_trace
                .inputs
                .contains(&Utf8PathBuf::from("main.tex")),
            "tracked inputs should include main.tex"
        );
        assert!(
            outcome
                .dep_trace
                .inputs
                .contains(&Utf8PathBuf::from("refs.bbl")),
            "tracked inputs should include refs.bbl"
        );
        let build_meta = serde_json::from_slice::<BuildMeta>(
            &fs::read(fixture.build_root.join("rev-1/build-meta.json")).expect("read build meta"),
        )
        .expect("parse build meta");
        assert!(build_meta.aux_sensitive);
        assert_eq!(
            build_meta.dirty_files,
            vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")]
        );
        assert_eq!(build_meta.start_checkpoint_id, None);
        assert_eq!(build_meta.start_page_index, 0);
        assert_eq!(build_meta.page_count, outcome.page_metadata.len());
        assert_eq!(build_meta.rebuilt_page_count, outcome.page_metadata.len());
        assert_eq!(build_meta.reused_page_count, 0);
        assert_eq!(build_meta.semantic_pass_count, 2);
        assert_eq!(build_meta.semantic_rerun_count, 1);
        assert!(build_meta.semantic_fixpoint_reached);
        assert!(!build_meta.semantic_aux_backdated);
    }
    result
}

async fn run_bibliography_features_semantic_case(case: BibliographyFeaturesSemanticCase) {
    match case {
        BibliographyFeaturesSemanticCase::CoreAux => {
            let fixture = prepare_bibliography_features_semantic_case_fixture(
                "\\documentclass{article}\\begin{document}\\tableofcontents\\section{Intro}\\label{sec:intro}See \\ref{sec:intro} on page \\pageref{sec:intro}. Cite \\cite{alpha}.\\bibliography{refs}\\end{document}",
                "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
            );
            compile_bibliography_features_semantic_case_fixture(&fixture)
                .await
                .expect("semantic aux build should succeed");
            let build_root = &fixture.build_root;

            let output =
                fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
            assert!(output.contains("Contents"));
            assert!(output.contains("1 Intro"));
            assert!(output.contains("See 1 on page 1."));
            assert!(output.contains("Cite [1]."));
            assert!(output.contains("Alpha entry."));

            let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
            let concrete_aux_path = build_root.join("rev-1/semantic.aux");
            let concrete_aux = load_semantic_aux(&concrete_aux_path).expect("load concrete aux");
            let concrete_aux_text =
                fs::read_to_string(concrete_aux_path).expect("read concrete aux");
            assert_eq!(aux.labels.len(), 1);
            assert_eq!(aux.labels[0].key, "sec:intro");
            assert_eq!(aux.labels[0].number, "1");
            assert_eq!(aux.labels[0].page, 1);
            assert_eq!(aux.toc.len(), 1);
            assert_eq!(aux.toc[0].title, "Intro");
            assert_eq!(aux.toc[0].page, 1);
            assert_eq!(aux.citation_keys, vec!["alpha".to_string()]);
            assert_eq!(aux.bibliography_inputs, vec![Utf8PathBuf::from("refs.bbl")]);
            assert_eq!(aux.bibliography.len(), 1);
            assert_eq!(aux.bibliography[0].key, "alpha");
            assert_eq!(concrete_aux, aux);
            assert!(concrete_aux_text.contains("\\newlabel{"));
            assert!(
                concrete_aux_text
                    .contains("\\@writefile{toc}{\\contentsline{section}{\\numberline{31}")
            );
            assert!(concrete_aux_text.contains("\\citation{616c706861}"));
            assert!(concrete_aux_text.contains("\\bibdata{726566732e62626c}"));
            assert!(!concrete_aux_text.contains("\\latexdtoc{"));
            let stored_sources = serde_json::from_slice::<StoredSources>(
                &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
            )
            .expect("parse sources");
            let main_tex = Utf8PathBuf::from("main.tex");
            let refs_bbl = Utf8PathBuf::from("refs.bbl");
            let raw_main = &stored_sources.files[&main_tex];
            let executed_main = &stored_sources.executed_files[&main_tex];
            assert!(raw_main.contains("\\ref{sec:intro}"));
            assert!(!executed_main.contains("\\ref{sec:intro}"));
            assert!(executed_main.contains("See 1 on page 1. Cite [1]."));
            assert!(executed_main.contains("Contents"));
            assert_eq!(stored_sources.executed_files[&refs_bbl], "[1] Alpha entry.");
        }
        BibliographyFeaturesSemanticCase::NatexlabOutput => {
            let fixture = prepare_bibliography_features_semantic_case_fixture(
                "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
                r"\begin{thebibliography}{1}\bibitem[Alpha 2024\natexlab{a}]{alpha} Alpha \newblock 2024\NAT@exlab{a}.\end{thebibliography}",
            );
            compile_bibliography_features_semantic_case_fixture(&fixture)
                .await
                .expect("semantic aux build should succeed");
            let build_root = &fixture.build_root;

            let output =
                fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
            assert!(output.contains("Alpha 2024a."));
            assert!(!output.contains("\\natexlab"));
            assert!(!output.contains("\\NAT@exlab"));

            let stored_sources = serde_json::from_slice::<StoredSources>(
                &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
            )
            .expect("parse sources");
            assert!(
                stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")]
                    .contains("Alpha 2024a.")
            );
        }
    }
}
