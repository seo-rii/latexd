struct ExternalOracleInternalCompilerCompatFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
}

enum ExternalOracleInternalCompilerCompatCase {
    Cleveref,
    Fontspec,
    Minted,
    Revtex,
}

type CompatCase = ExternalOracleInternalCompilerCompatCase;

async fn run_compat(case: CompatCase) {
    run_external_oracle_internal_compiler_compat_case(case).await;
}

fn prepare_external_oracle_internal_compiler_compat_fixture(
    compiler: &str,
    main_source: &str,
    extra_files: &[(&str, &str)],
) -> ExternalOracleInternalCompilerCompatFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    fs::write(
        root.join("00README.yaml"),
        format!(
            r#"
compiler: {compiler}
toplevel:
  - main.tex
"#
        ),
    )
    .expect("write manifest");
    fs::write(root.join("main.tex"), main_source).expect("write main");
    for (relative_path, contents) in extra_files {
        let target = root.join(relative_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent.as_std_path()).expect("create parent dir");
        }
        fs::write(target, contents).expect("write extra file");
    }
    let build_root = root.join(".latexd/build");
    ExternalOracleInternalCompilerCompatFixture {
        _tempdir: tempdir,
        root,
        build_root,
    }
}

async fn compile_external_oracle_internal_compiler_compat_output(
    fixture: &ExternalOracleInternalCompilerCompatFixture,
    changed_files: &[&str],
) -> String {
    let world = ProjectWorld::load(fixture.root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: fixture.build_root.clone(),
            changed_files: changed_files
                .iter()
                .map(|path| Utf8PathBuf::from(*path))
                .collect(),
        })
        .await
        .expect("internal compat build should succeed");

    fs::read_to_string(fixture.build_root.join("rev-1/output.txt")).expect("read output")
}

async fn run_external_oracle_internal_compiler_compat_case(
    case: ExternalOracleInternalCompilerCompatCase,
) {
    let (compiler, main_source, extra_files, changed_files, expected_output) = match case {
        ExternalOracleInternalCompilerCompatCase::Cleveref => (
            "pdf_latex",
            "\\documentclass{article}\\begin{document}\\usepackage{cleveref}\\hyperdriver\\cleverefready\\cref{sec:intro}\\end{document}",
            vec![
                ("article.cls", ""),
                (
                    "hyperref.sty",
                    r"\ProvidesPackage{hyperref}[2024/01/01]\def\hyperdriver{hyperref}",
                ),
                (
                    "cleveref.sty",
                    r"\ProvidesPackage{cleveref}[2024/01/01]\RequirePackage{hyperref}\AtBeginDocument{\DeclareRobustCommand{\cref}[1]{CRef #1}\def\cleverefready{ready}}",
                ),
            ],
            vec!["main.tex", "hyperref.sty", "cleveref.sty"],
            "hyperrefreadyCRef sec:intro",
        ),
        ExternalOracleInternalCompilerCompatCase::Fontspec => (
            "xe_latex",
            "\\documentclass{article}\\begin{document}\\usepackage{fontspec}\\setmainfont{Example Font.otf}\\fontready\\end{document}",
            vec![
                ("article.cls", ""),
                (
                    "fontspec.sty",
                    r"\ProvidesPackage{fontspec}[2024/01/01]\def\setmainfont#1{\IfFileExists{#1}{\def\fontready{font-found}}{\def\fontready{font-missing}}}",
                ),
                ("Example Font.otf", "fake font payload"),
            ],
            vec!["main.tex", "fontspec.sty", "Example Font.otf"],
            "font-found",
        ),
        ExternalOracleInternalCompilerCompatCase::Minted => (
            "pdf_latex",
            "\\documentclass{article}\\begin{document}\\usepackage{minted}\\inputminted{python}{main.py}\\end{document}",
            vec![
                ("article.cls", ""),
                (
                    "minted.sty",
                    r"\ProvidesPackage{minted}[2024/01/01]\def\inputminted#1#2{\IfFileExists{_minted-main/code.pygtex}{\input{_minted-main/code.pygtex}}{cache-miss}}",
                ),
                ("_minted-main/code.pygtex", "cached minted output"),
            ],
            vec!["main.tex", "minted.sty", "_minted-main/code.pygtex"],
            "cached minted output",
        ),
        ExternalOracleInternalCompilerCompatCase::Revtex => (
            "pdf_latex",
            "\\documentclass{revtex4-2}\\begin{document}\\articleclass\\revtexclass\\arrayloaded\\end{document}",
            vec![
                (
                    "article.cls",
                    r"\ProvidesClass{article}[2024/01/01]\def\articleclass{article}",
                ),
                (
                    "array.sty",
                    r"\ProvidesPackage{array}[2024/01/01]\def\arrayloaded{array}",
                ),
                (
                    "revtex4-2.cls",
                    r"\NeedsTeXFormat{LaTeX2e}\ProvidesClass{revtex4-2}[2024/01/01]\LoadClassWithOptions{article}\RequirePackage{array}\def\revtexclass{revtex}",
                ),
            ],
            vec!["main.tex", "article.cls", "array.sty", "revtex4-2.cls"],
            "articlerevtexarray",
        ),
    };

    let fixture = prepare_external_oracle_internal_compiler_compat_fixture(
        compiler,
        main_source,
        &extra_files,
    );
    let output =
        compile_external_oracle_internal_compiler_compat_output(&fixture, &changed_files).await;
    assert!(output.contains(expected_output));
}
