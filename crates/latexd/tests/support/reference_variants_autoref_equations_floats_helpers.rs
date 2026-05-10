struct ReferenceVariantsAutorefEquationsFloatsFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
}

fn prepare_reference_variants_autoref_equations_floats_fixture(
    main_source: &str,
) -> ReferenceVariantsAutorefEquationsFloatsFixture {
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

    let world = ProjectWorld::load(root.clone()).expect("world");
    let build_root = root.join(".latexd/build");
    ReferenceVariantsAutorefEquationsFloatsFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
    }
}

async fn compile_reference_variants_autoref_equations_floats_fixture(
    fixture: &ReferenceVariantsAutorefEquationsFloatsFixture,
) -> Result<CompileOutcome, latexd::compiler::CompileFailure> {
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: fixture.build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
}

async fn assert_autoref_equations_floats_output_and_sources(
    fixture: &ReferenceVariantsAutorefEquationsFloatsFixture,
    expected_output_texts: &[&str],
    absent_output_texts: &[&str],
    expected_source_texts: &[&str],
    absent_source_texts: &[&str],
) {
    compile_reference_variants_autoref_equations_floats_fixture(fixture)
        .await
        .expect("semantic aux build should succeed");

    let output =
        fs::read_to_string(fixture.build_root.join("rev-1/output.txt")).expect("read output");
    for expected_text in expected_output_texts {
        assert!(output.contains(expected_text));
    }
    for absent_text in absent_output_texts {
        assert!(!output.contains(absent_text));
    }

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(fixture.build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    for expected_text in expected_source_texts {
        assert!(executed_main.contains(expected_text));
    }
    for absent_text in absent_source_texts {
        assert!(!executed_main.contains(absent_text));
    }
}

enum ReferenceVariantsAutorefKindCase {
    EquationKinds,
    AlignLabels,
    FloatKinds,
}

async fn run_reference_variants_autoref_kind(case: ReferenceVariantsAutorefKindCase) {
    match case {
        ReferenceVariantsAutorefKindCase::EquationKinds => {
            let fixture = prepare_reference_variants_autoref_equations_floats_fixture(
                "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{equation}\\label{eq:first}a\\end{equation}\\begin{equation}\\label{eq:second}b\\end{equation}See \\autoref{eq:first}, \\cref{eq:first,eq:second}, \\namecref{eq:first}, \\vref{eq:first}, and \\crefrange{eq:first}{eq:second}.\\end{document}",
            );

            assert_autoref_equations_floats_output_and_sources(
                &fixture,
                &["See Equation 1, Equations 1, 2, equation, equation 1 on page 1, and Equations 1 to 2."],
                &[],
                &["See Equation 1, Equations 1, 2, equation, equation 1 on page 1, and Equations 1 to 2."],
                &["\\autoref", "\\cref", "\\namecref", "\\vref", "\\crefrange"],
            )
            .await;
        }
        ReferenceVariantsAutorefKindCase::AlignLabels => {
            let fixture = prepare_reference_variants_autoref_equations_floats_fixture(
                "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{align}\\label{eq:first}a\\end{align}\\begin{gather}\\label{eq:second}b\\end{gather}See \\eqref{eq:first}, \\autoref{eq:first}, and \\crefrange{eq:first}{eq:second}.\\end{document}",
            );

            assert_autoref_equations_floats_output_and_sources(
                &fixture,
                &["See (1), Equation 1, and Equations 1 to 2."],
                &[],
                &["See (1), Equation 1, and Equations 1 to 2."],
                &["\\eqref", "\\autoref", "\\crefrange"],
            )
            .await;
        }
        ReferenceVariantsAutorefKindCase::FloatKinds => {
            let fixture = prepare_reference_variants_autoref_equations_floats_fixture(
                "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{figure}\\label{fig:first}a\\end{figure}\\begin{table}\\label{tab:first}b\\end{table}See \\autoref{fig:first}, \\cref{tab:first}, \\namecref{fig:first}, and \\vref{tab:first}.\\end{document}",
            );

            assert_autoref_equations_floats_output_and_sources(
                &fixture,
                &["See Figure 1, Table 1, figure, and table 1 on page 1."],
                &[],
                &["See Figure 1, Table 1, figure, and table 1 on page 1."],
                &["\\autoref", "\\cref", "\\namecref", "\\vref"],
            )
            .await;
        }
    }
}

type RefFloatCap = ReferenceVariantsAutorefFloatCaptionCase;

async fn run_ref_float_cap(case: RefFloatCap) {
    run_reference_variants_autoref_float_caption(case).await;
}

enum ReferenceVariantsAutorefFloatCaptionCase {
    CaptionsAndLists,
    CaptionOf,
    StarredCaption,
}

async fn run_reference_variants_autoref_float_caption(
    case: ReferenceVariantsAutorefFloatCaptionCase,
) {
    match case {
        ReferenceVariantsAutorefFloatCaptionCase::CaptionsAndLists => {
            let fixture = prepare_reference_variants_autoref_equations_floats_fixture(
                "\\documentclass{article}\\begin{document}\\listoffigures\\listoftables\\begin{figure}\\caption[Short Figure]{Long Figure Title}\\label{fig:first}a\\end{figure}\\begin{table}\\caption{Long Table Title}\\label{tab:first}b\\end{table}\\end{document}",
            );

            assert_autoref_equations_floats_output_and_sources(
                &fixture,
                &[
                    "List of Figures",
                    "Short Figure",
                    "List of Tables",
                    "Long Table Title",
                    "Figure 1: Long Figure Title",
                    "Table 1: Long Table Title",
                ],
                &[],
                &[
                    "List of Figures\n1 Short Figure .... 1\n",
                    "Figure 1: Long Figure Title",
                ],
                &["\\listoffigures", "\\listoftables", "\\caption"],
            )
            .await;

            let aux =
                load_semantic_aux(&fixture.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(aux.float_captions.len(), 2);
            assert_eq!(aux.float_captions[0].kind, "figure");
            assert_eq!(aux.float_captions[0].title, "Short Figure");
            assert_eq!(aux.float_captions[1].kind, "table");
        }
        ReferenceVariantsAutorefFloatCaptionCase::CaptionOf => {
            let fixture = prepare_reference_variants_autoref_equations_floats_fixture(
                "\\documentclass{article}\\begin{document}\\listoffigures\\captionof{figure}[Short Figure]{Long Figure Title}\\label{fig:first}See \\autoref{fig:first}.\\end{document}",
            );

            assert_autoref_equations_floats_output_and_sources(
                &fixture,
                &[
                    "List of Figures",
                    "Short Figure",
                    "Figure 1: Long Figure Title",
                    "See Figure 1.",
                ],
                &[],
                &[
                    "List of Figures\n1 Short Figure .... 1\n",
                    "Figure 1: Long Figure Title",
                    "See Figure 1.",
                ],
                &["\\captionof"],
            )
            .await;

            let aux =
                load_semantic_aux(&fixture.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(aux.float_captions.len(), 1);
            assert_eq!(aux.float_captions[0].kind, "figure");
            assert_eq!(aux.labels[0].number, "1");
        }
        ReferenceVariantsAutorefFloatCaptionCase::StarredCaption => {
            let fixture = prepare_reference_variants_autoref_equations_floats_fixture(
                "\\documentclass{article}\\begin{document}\\listoffigures\\begin{figure}\\caption*{Hidden Figure Title}\\end{figure}\\captionof*{figure}{Detached Hidden Figure}\\end{document}",
            );

            assert_autoref_equations_floats_output_and_sources(
                &fixture,
                &["Hidden Figure Title", "Detached Hidden Figure"],
                &["List of Figures"],
                &["Hidden Figure Title", "Detached Hidden Figure"],
                &["List of Figures", "\\caption*", "\\captionof*"],
            )
            .await;

            let aux =
                load_semantic_aux(&fixture.build_root.join("rev-1/aux.json")).expect("load aux");
            assert_eq!(aux.float_captions.len(), 2);
            assert!(
                aux.float_captions
                    .iter()
                    .all(|caption| caption.number.is_empty())
            );
        }
    }
}
