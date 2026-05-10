fn write_internal_compiler_manifest(root: &Utf8Path) {
    fs::write(
        root.join("00README.yaml"),
        r#"
compiler: pdf_latex
toplevel:
  - main.tex
"#,
    )
    .expect("write manifest");
}

fn write_internal_compiler_article_class(root: &Utf8Path, class_source: &str) {
    fs::write(root.join("article.cls"), class_source).expect("write class");
}

struct InternalCompilerCoreFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
}

fn prepare_internal_compiler_core_fixture(class_source: &str) -> InternalCompilerCoreFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    write_internal_compiler_manifest(&root);
    write_internal_compiler_article_class(&root, class_source);
    let build_root = root.join(".latexd/build");
    InternalCompilerCoreFixture {
        _tempdir: tempdir,
        root,
        build_root,
    }
}

fn internal_compiler_core_changed_files(paths: &[&str]) -> Vec<Utf8PathBuf> {
    paths.iter().map(|path| Utf8PathBuf::from(*path)).collect()
}

async fn compile_internal_compiler_core_fixture(
    fixture: &InternalCompilerCoreFixture,
    changed_files: &[&str],
) -> CompileOutcome {
    compile_internal_compiler_main(
        &fixture.root,
        1,
        fixture.build_root.clone(),
        internal_compiler_core_changed_files(changed_files),
    )
    .await
}

async fn compile_internal_compiler_main(
    root: &Utf8Path,
    rev: u64,
    build_root: Utf8PathBuf,
    changed_files: Vec<Utf8PathBuf>,
) -> CompileOutcome {
    let world = ProjectWorld::load(root.to_path_buf()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    driver
        .compile(CompileRequest {
            root: root.to_path_buf(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev,
            build_root,
            changed_files,
        })
        .await
        .expect("internal compiler build should succeed")
}

fn assert_first_page_artifact_urls(root: &Utf8Path, outcome: &CompileOutcome) {
    assert!(
        root.join(format!(
            ".latexd/build/rev-1/pages/{}.pdf",
            outcome.page_metadata[0].page_id
        ))
        .exists()
    );
    assert_eq!(
        outcome.page_artifacts[0].pdf_url,
        format!(
            "/artifacts/rev/1/pages/{}.pdf",
            outcome.page_metadata[0].page_id
        )
    );
    let expected_svg_url = format!(
        "/artifacts/rev/1/pages/{}.svg",
        outcome.page_metadata[0].page_id
    );
    assert_eq!(
        outcome.page_artifacts[0].svg_url.as_deref(),
        Some(expected_svg_url.as_str())
    );
}

fn assert_internal_compiler_dep_inputs(outcome: &CompileOutcome, expected_inputs: &[&str]) {
    for expected_input in expected_inputs {
        assert!(
            outcome
                .dep_trace
                .inputs
                .contains(&Utf8PathBuf::from(*expected_input)),
            "tracked inputs should include {expected_input}"
        );
    }
}

fn assert_internal_compiler_single_page_artifacts(
    root: &Utf8Path,
    build_root: &Utf8Path,
    outcome: &CompileOutcome,
) {
    assert!(outcome.pdf_path.exists());
    assert_eq!(outcome.page_metadata.len(), 1);
    assert_eq!(outcome.page_metadata[0].index, 0);
    assert_eq!(outcome.page_metadata[0].line_count, 1);
    assert_eq!(outcome.page_metadata[0].width_pt, 612);
    assert_eq!(outcome.page_metadata[0].height_pt, 792);
    assert_eq!(outcome.page_metadata[0].text_start_utf8, 0);
    assert!(!outcome.page_metadata[0].content_hash.is_empty());
    assert_first_page_artifact_urls(root, outcome);
    assert!(
        build_root
            .join(format!(
                "rev-1/pages/{}.svg",
                outcome.page_metadata[0].page_id
            ))
            .exists()
    );
    let syncmap: Vec<PageSyncMapArtifact> = serde_json::from_slice(
        &fs::read(build_root.join("rev-1/page-syncmap.json")).expect("read page syncmap"),
    )
    .expect("decode page syncmap");
    assert_eq!(syncmap.len(), 1);
    assert_eq!(syncmap[0].page_id, outcome.page_metadata[0].page_id);
    assert_eq!(syncmap[0].width_pt, outcome.page_metadata[0].width_pt);
    assert!(
        syncmap[0]
            .items
            .iter()
            .all(|item| item.right_px > item.left_px)
    );
    assert!(
        syncmap[0]
            .items
            .iter()
            .all(|item| item.bottom_px > item.top_px)
    );
    assert!(
        syncmap[0]
            .items
            .iter()
            .any(|item| item.left_px > 0 && item.right_px < syncmap[0].width_pt)
    );
}

async fn assert_internal_compiler_single_page_output_contains(
    fixture: &InternalCompilerCoreFixture,
    changed_files: &[&str],
    expected_inputs: &[&str],
    expected_output: &str,
) {
    let outcome = compile_internal_compiler_core_fixture(fixture, changed_files).await;
    assert_internal_compiler_single_page_artifacts(&fixture.root, &fixture.build_root, &outcome);
    assert_internal_compiler_dep_inputs(&outcome, expected_inputs);
    let output =
        fs::read_to_string(fixture.build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains(expected_output));
}

enum InternalCompilerCoreCase {
    PageMetadata,
    GroupedUsepackage,
    HyperxmpStyleLoadOrder,
}

async fn run_internal_compiler_core_case(case: InternalCompilerCoreCase) {
    match case {
        InternalCompilerCoreCase::PageMetadata => {
            let fixture = prepare_internal_compiler_core_fixture("\\def\\classmark{class}");
            fs::write(fixture.root.join("pkg.sty"), "\\def\\pkgmark{pkg}").expect("write package");
            fs::write(
                fixture.root.join("main.tex"),
                "\\documentclass{article}\\usepackage{pkg}\\begin{document}\\classmark\\pkgmark\\section{Hi}\\end{document}",
            )
            .expect("write main tex");

            let outcome = compile_internal_compiler_core_fixture(&fixture, &["main.tex"]).await;

            assert_internal_compiler_single_page_artifacts(
                &fixture.root,
                &fixture.build_root,
                &outcome,
            );
            assert_eq!(
                outcome.page_metadata[0]
                    .source_spans
                    .iter()
                    .map(|span| span.file.clone())
                    .collect::<Vec<_>>(),
                vec![
                    Utf8PathBuf::from("main.tex"),
                    Utf8PathBuf::from("article.cls"),
                    Utf8PathBuf::from("pkg.sty")
                ]
            );
            assert_internal_compiler_dep_inputs(&outcome, &["main.tex", "article.cls", "pkg.sty"]);
            let checkpoints =
                load_checkpoint_bundle(&fixture.build_root.join("rev-1/checkpoints.json"))
                    .expect("load checkpoints");
            assert_eq!(checkpoints.checkpoints.len(), 2);
        }
        InternalCompilerCoreCase::GroupedUsepackage => {
            let fixture = prepare_internal_compiler_core_fixture("");
            fs::write(fixture.root.join("setspace.sty"), "\\input{setspace-defs}")
                .expect("write package");
            fs::write(
                fixture.root.join("setspace-defs.tex"),
                "\\def\\singlespacing{Single Spacing}",
            )
            .expect("write package defs");
            fs::write(
                fixture.root.join("main.tex"),
                "\\documentclass{article}\\begin{document}{\\usepackage{setspace}}\\singlespacing\\end{document}",
            )
            .expect("write main");

            assert_internal_compiler_single_page_output_contains(
                &fixture,
                &["main.tex", "setspace.sty", "setspace-defs.tex"],
                &[
                    "main.tex",
                    "article.cls",
                    "setspace.sty",
                    "setspace-defs.tex",
                ],
                "Single Spacing",
            )
            .await;
        }
        InternalCompilerCoreCase::HyperxmpStyleLoadOrder => {
            let fixture = prepare_internal_compiler_core_fixture("");
            fs::write(
                fixture.root.join("hyperref.sty"),
                r"\NeedsTeXFormat{LaTeX2e}\ProvidesPackage{hyperref}[2024/01/01]\DeclareOption{unicode}{\def\hyperunicode{unicode}}\ProcessOptions\relax\def\hypersetup#1{}\def\hyperdriver{hyperref}",
            )
            .expect("write hyperref");
            fs::write(
                fixture.root.join("hyperxmp.sty"),
                r"\NeedsTeXFormat{LaTeX2e}\ProvidesPackage{hyperxmp}[2024/01/01]\PassOptionsToPackage{unicode}{hyperref}\RequirePackage{hyperref}\def\hyperxmploaded{hyperxmp}\hypersetup{pdfauthor=Author}",
            )
            .expect("write hyperxmp");
            fs::write(
                fixture.root.join("main.tex"),
                "\\documentclass{article}\\begin{document}\\usepackage{hyperxmp}\\hyperdriver\\hyperxmploaded\\end{document}",
            )
            .expect("write main");

            assert_internal_compiler_single_page_output_contains(
                &fixture,
                &["main.tex", "hyperref.sty", "hyperxmp.sty"],
                &["main.tex", "article.cls", "hyperref.sty", "hyperxmp.sty"],
                "hyperrefhyperxmp",
            )
            .await;
        }
    }
}
