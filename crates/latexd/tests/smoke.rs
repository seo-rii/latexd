use std::{collections::BTreeMap, fs};

use camino::Utf8PathBuf;
use hmr_protocol::PagePatchOp;
use latexd::{
    PreviewSnapshot,
    compiler::{CompileRequest, CompilerDriver, PageSyncMapArtifact},
};
use tempfile::tempdir;
use tex_aux::{MaterializedRewriteSpan, SemanticAuxIndex, load_semantic_aux};
use tex_checkpoint::{CheckpointKind, load_checkpoint_bundle};
use tex_world::ProjectWorld;

#[derive(Debug, serde::Deserialize)]
struct BuildMeta {
    aux_sensitive: bool,
    dirty_files: Vec<Utf8PathBuf>,
    start_checkpoint_id: Option<String>,
    start_page_index: usize,
    page_count: usize,
    rebuilt_page_count: usize,
    reused_page_count: usize,
    semantic_pass_count: usize,
    semantic_rerun_count: usize,
    semantic_fixpoint_reached: bool,
    semantic_aux_backdated: bool,
}

#[derive(Debug, serde::Deserialize)]
struct StoredSources {
    files: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    executed_files: BTreeMap<Utf8PathBuf, String>,
    #[serde(default)]
    rewrite_spans: BTreeMap<Utf8PathBuf, Vec<MaterializedRewriteSpan>>,
}

#[tokio::test]
async fn mock_compiler_builds_pdf_and_failure_keeps_last_good_preview() {
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
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    fs::write(root.join("sections/intro.tex"), "Intro section").expect("write intro tex");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\n\\input{sections/intro}\nHello latexd\n\\end{document}\n",
    )
    .expect("write main tex");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(
        Some(env!("CARGO_BIN_EXE_latexd").to_string()),
        vec![
            "mock-compiler".to_string(),
            "--input".to_string(),
            "{main}".to_string(),
            "--output".to_string(),
            "{out_pdf}".to_string(),
            "--depfile".to_string(),
            "{depfile}".to_string(),
            "--fail-if-contains".to_string(),
            "\\broken".to_string(),
        ],
    );
    let build_root = root.join(".latexd/build");

    let first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("first build should succeed");

    assert!(first.pdf_path.exists());
    assert_eq!(
        first.dep_trace.inputs,
        vec![
            Utf8PathBuf::from("main.tex"),
            Utf8PathBuf::from("sections/intro.tex")
        ]
    );

    let mut snapshot = PreviewSnapshot::default();
    snapshot.apply_started(1, vec!["main.tex".to_string()]);
    snapshot.apply_success(
        1,
        first.diagnostics.clone(),
        "/artifacts/rev/1/main.pdf".to_string(),
        first.page_metadata.len(),
        first
            .page_metadata
            .iter()
            .map(|page| page.page_id.clone())
            .collect(),
        first.page_artifacts.clone(),
    );
    let last_good = snapshot.pdf_url.clone();

    fs::write(root.join("main.tex"), "\\broken").expect("write broken source");
    let failure = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root,
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect_err("second build should fail");

    snapshot.apply_started(2, vec!["main.tex".to_string()]);
    snapshot.apply_failure(2, failure.diagnostics.clone());

    assert_eq!(snapshot.pdf_url, last_good);
    assert_eq!(snapshot.last_build_succeeded, Some(false));
    assert!(!failure.diagnostics.is_empty());
}

#[tokio::test]
async fn internal_compiler_builds_pdf_and_emits_page_metadata() {
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
    fs::write(root.join("article.cls"), "\\def\\classmark{class}").expect("write class");
    fs::write(root.join("pkg.sty"), "\\def\\pkgmark{pkg}").expect("write package");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\usepackage{pkg}\\begin{document}\\classmark\\pkgmark\\section{Hi}\\end{document}",
    )
    .expect("write main tex");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let outcome = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: root.join(".latexd/build"),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("internal build should succeed");

    assert!(outcome.pdf_path.exists());
    assert_eq!(outcome.page_metadata.len(), 1);
    assert_eq!(outcome.page_metadata[0].index, 0);
    assert_eq!(outcome.page_metadata[0].line_count, 1);
    assert_eq!(outcome.page_metadata[0].width_pt, 612);
    assert_eq!(outcome.page_metadata[0].height_pt, 792);
    assert_eq!(outcome.page_metadata[0].text_start_utf8, 0);
    assert!(!outcome.page_metadata[0].content_hash.is_empty());
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
    assert!(
        outcome
            .dep_trace
            .inputs
            .contains(&Utf8PathBuf::from("main.tex"))
    );
    assert!(
        outcome
            .dep_trace
            .inputs
            .contains(&Utf8PathBuf::from("article.cls"))
    );
    assert!(
        outcome
            .dep_trace
            .inputs
            .contains(&Utf8PathBuf::from("pkg.sty"))
    );
    let checkpoints = load_checkpoint_bundle(&root.join(".latexd/build/rev-1/checkpoints.json"))
        .expect("load checkpoints");
    assert_eq!(checkpoints.checkpoints.len(), 2);
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
    assert!(
        root.join(format!(
            ".latexd/build/rev-1/pages/{}.svg",
            outcome.page_metadata[0].page_id
        ))
        .exists()
    );
    let syncmap: Vec<PageSyncMapArtifact> = serde_json::from_slice(
        &fs::read(root.join(".latexd/build/rev-1/page-syncmap.json")).expect("read page syncmap"),
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

#[tokio::test]
async fn bundled_arxiv_basic_fixture_builds_with_internal_compiler() {
    let fixture_root =
        Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/arxiv-basic");
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 tempdir");
    let mut copy_dirs = vec![(fixture_root.clone(), root.clone())];
    while let Some((source_dir, target_dir)) = copy_dirs.pop() {
        fs::create_dir_all(target_dir.as_std_path()).expect("create target dir");
        for entry in fs::read_dir(source_dir.as_std_path())
            .expect("read source dir")
            .filter_map(|entry| entry.ok())
        {
            let source_path =
                Utf8PathBuf::from_path_buf(entry.path()).expect("fixture path should be utf8");
            let target_path = target_dir.join(entry.file_name().to_string_lossy().as_ref());
            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                copy_dirs.push((source_path, target_path));
            } else {
                fs::copy(source_path.as_std_path(), target_path.as_std_path())
                    .expect("copy fixture file");
            }
        }
    }

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let outcome = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: root.join(".latexd/build"),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("bundled arxiv-basic fixture should build with the internal compiler");

    assert!(outcome.pdf_path.exists());
    assert!(outcome.diagnostics.is_empty());
    assert_eq!(outcome.page_metadata.len(), 1);
    assert!(
        outcome
            .dep_trace
            .inputs
            .contains(&Utf8PathBuf::from("main.tex"))
    );
    assert!(
        outcome
            .dep_trace
            .inputs
            .contains(&Utf8PathBuf::from("article.cls"))
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

#[tokio::test]
async fn internal_compiler_keeps_grouped_usepackage_definitions_visible() {
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
    fs::write(root.join("setspace.sty"), "\\input{setspace-defs}").expect("write package");
    fs::write(
        root.join("setspace-defs.tex"),
        "\\def\\singlespacing{Single Spacing}",
    )
    .expect("write package defs");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}{\\usepackage{setspace}}\\singlespacing\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("setspace.sty"),
                Utf8PathBuf::from("setspace-defs.tex"),
            ],
        })
        .await
        .expect("grouped usepackage build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Single Spacing"));
}

#[tokio::test]
async fn internal_compiler_supports_hyperxmp_style_package_load_order() {
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
    fs::write(
        root.join("hyperref.sty"),
        r"\NeedsTeXFormat{LaTeX2e}\ProvidesPackage{hyperref}[2024/01/01]\DeclareOption{unicode}{\def\hyperunicode{unicode}}\ProcessOptions\relax\def\hypersetup#1{}\def\hyperdriver{hyperref}",
    )
    .expect("write hyperref");
    fs::write(
        root.join("hyperxmp.sty"),
        r"\NeedsTeXFormat{LaTeX2e}\ProvidesPackage{hyperxmp}[2024/01/01]\PassOptionsToPackage{unicode}{hyperref}\RequirePackage{hyperref}\def\hyperxmploaded{hyperxmp}\hypersetup{pdfauthor=Author}",
    )
    .expect("write hyperxmp");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\usepackage{hyperxmp}\\hyperdriver\\hyperxmploaded\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("hyperref.sty"),
                Utf8PathBuf::from("hyperxmp.sty"),
            ],
        })
        .await
        .expect("hyperxmp-style build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("hyperrefhyperxmp"));
}

#[tokio::test]
async fn internal_compiler_supports_revtex_style_class_load_order() {
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
    fs::write(
        root.join("article.cls"),
        r"\ProvidesClass{article}[2024/01/01]\def\articleclass{article}",
    )
    .expect("write article");
    fs::write(
        root.join("array.sty"),
        r"\ProvidesPackage{array}[2024/01/01]\def\arrayloaded{array}",
    )
    .expect("write array");
    fs::write(
        root.join("revtex4-2.cls"),
        r"\NeedsTeXFormat{LaTeX2e}\ProvidesClass{revtex4-2}[2024/01/01]\LoadClassWithOptions{article}\RequirePackage{array}\def\revtexclass{revtex}",
    )
    .expect("write revtex");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{revtex4-2}\\begin{document}\\articleclass\\revtexclass\\arrayloaded\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("article.cls"),
                Utf8PathBuf::from("array.sty"),
                Utf8PathBuf::from("revtex4-2.cls"),
            ],
        })
        .await
        .expect("revtex-style build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("articlerevtexarray"));
}

#[tokio::test]
async fn internal_compiler_supports_cleveref_hyperref_style_package_hooks() {
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
    fs::write(
        root.join("hyperref.sty"),
        r"\ProvidesPackage{hyperref}[2024/01/01]\def\hyperdriver{hyperref}",
    )
    .expect("write hyperref");
    fs::write(
        root.join("cleveref.sty"),
        r"\ProvidesPackage{cleveref}[2024/01/01]\RequirePackage{hyperref}\AtBeginDocument{\DeclareRobustCommand{\cref}[1]{CRef #1}\def\cleverefready{ready}}",
    )
    .expect("write cleveref");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\usepackage{cleveref}\\hyperdriver\\cleverefready\\cref{sec:intro}\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("hyperref.sty"),
                Utf8PathBuf::from("cleveref.sty"),
            ],
        })
        .await
        .expect("cleveref-style build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("hyperrefreadyCRef sec:intro"));
}

#[tokio::test]
async fn internal_compiler_supports_minted_style_cached_pygtex_input() {
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
    fs::write(
        root.join("minted.sty"),
        r"\ProvidesPackage{minted}[2024/01/01]\def\inputminted#1#2{\IfFileExists{_minted-main/code.pygtex}{\input{_minted-main/code.pygtex}}{cache-miss}}",
    )
    .expect("write minted");
    fs::create_dir_all(root.join("_minted-main")).expect("create cache dir");
    fs::write(
        root.join("_minted-main/code.pygtex"),
        "cached minted output",
    )
    .expect("write pygtex");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\usepackage{minted}\\inputminted{python}{main.py}\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("minted.sty"),
                Utf8PathBuf::from("_minted-main/code.pygtex"),
            ],
        })
        .await
        .expect("minted-style build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("cached minted output"));
}

#[tokio::test]
async fn internal_compiler_supports_xelatex_style_local_font_filename_lookup() {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    fs::write(
        root.join("00README.yaml"),
        r#"
compiler: xe_latex
toplevel:
  - main.tex
"#,
    )
    .expect("write manifest");
    fs::write(root.join("article.cls"), "").expect("write class");
    fs::write(
        root.join("fontspec.sty"),
        r"\ProvidesPackage{fontspec}[2024/01/01]\def\setmainfont#1{\IfFileExists{#1}{\def\fontready{font-found}}{\def\fontready{font-missing}}}",
    )
    .expect("write fontspec");
    fs::write(root.join("Example Font.otf"), "fake font payload").expect("write font file");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\usepackage{fontspec}\\setmainfont{Example Font.otf}\\fontready\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("fontspec.sty"),
                Utf8PathBuf::from("Example Font.otf"),
            ],
        })
        .await
        .expect("xelatex-style build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("font-found"));
}

#[tokio::test]
async fn internal_compiler_stabilizes_semantic_aux_and_ingests_bbl() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\section{Intro}\\label{sec:intro}See \\ref{sec:intro} on page \\pageref{sec:intro}. Cite \\cite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let outcome = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

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

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Contents"));
    assert!(output.contains("1 Intro"));
    assert!(output.contains("See 1 on page 1."));
    assert!(output.contains("Cite [1]."));
    assert!(output.contains("Alpha entry."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    let concrete_aux =
        load_semantic_aux(&build_root.join("rev-1/semantic.aux")).expect("load concrete aux");
    let concrete_aux_text =
        fs::read_to_string(build_root.join("rev-1/semantic.aux")).expect("read concrete aux");
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
        concrete_aux_text.contains("\\@writefile{toc}{\\contentsline{section}{\\numberline{31}")
    );
    assert!(concrete_aux_text.contains("\\citation{616c706861}"));
    assert!(concrete_aux_text.contains("\\bibdata{726566732e62626c}"));
    assert!(!concrete_aux_text.contains("\\latexdtoc{"));
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-1/build-meta.json")).expect("read build meta"),
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
    assert!(build_root.join("rev-1/sources.json").exists());
    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert!(stored_sources.files[&Utf8PathBuf::from("main.tex")].contains("\\ref{sec:intro}"));
    assert!(
        !stored_sources.executed_files[&Utf8PathBuf::from("main.tex")].contains("\\ref{sec:intro}")
    );
    assert!(
        stored_sources.executed_files[&Utf8PathBuf::from("main.tex")]
            .contains("See 1 on page 1. Cite [1].")
    );
    assert!(stored_sources.executed_files[&Utf8PathBuf::from("main.tex")].contains("Contents"));
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] Alpha entry."
    );
}

#[tokio::test]
async fn internal_compiler_supports_starred_refs_optional_cite_notes_and_bibitem_labels() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}See \\ref*{sec:intro} on page \\pageref*{sec:intro}. Cite \\cite[see][chap.~2]{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem[Alpha 2024]{alpha} Alpha {Entry} \\emph{Title}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See 1 on page 1."));
    assert!(output.contains("Cite [see 1, chap.~2]."));
    assert!(output.contains("Alpha Entry Title."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See 1 on page 1. Cite [see 1, chap.~2]."));
    assert!(!executed_main.contains("\\ref*"));
    assert!(!executed_main.contains("\\pageref*"));
    assert!(!executed_main.contains("\\cite[see][chap.~2]"));
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] Alpha Entry Title."
    );
}

#[tokio::test]
async fn internal_compiler_supports_citeauthor_and_citeyear_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citeauthor{alpha} (\\citeyear{alpha}) and \\citeauthor*{beta} (\\citeyear*{beta}).\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Alpha (2024) and Beta and Gamma (2023)."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Alpha (2024) and Beta and Gamma (2023)."));
    assert!(!executed_main.contains("\\citeauthor"));
    assert!(!executed_main.contains("\\citeyear"));
}

#[tokio::test]
async fn internal_compiler_supports_capitalized_citeauthor_and_citeyear_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\Citeauthor{alpha} (\\Citeyear{alpha}) and \\Citeauthor*{beta} (\\Citeyear*{beta}).\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[alpha 2024]{alpha} Alpha entry.\\bibitem[beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Alpha (2024) and Beta and Gamma (2023)."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Alpha (2024) and Beta and Gamma (2023)."));
    assert!(!executed_main.contains("\\Citeauthor"));
    assert!(!executed_main.contains("\\Citeyear"));
}

#[tokio::test]
async fn internal_compiler_supports_citetitle_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citetitle{alpha} and \\Citetitle{beta}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} alpha entry title.\\bibitem[Beta 2023]{beta} beta study heading.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See alpha entry title and Beta study heading."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See alpha entry title and Beta study heading."));
    assert!(!executed_main.contains("\\citetitle"));
    assert!(!executed_main.contains("\\Citetitle"));
}

#[tokio::test]
async fn internal_compiler_supports_citefield_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citefield{alpha}{author}, \\citefield{alpha}{year}, \\citefield{alpha}{title}, and \\citefield{alpha}{label}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem[Alpha 2024]{alpha} alpha entry title.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Alpha, 2024, alpha entry title, and Alpha 2024."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Alpha, 2024, alpha entry title, and Alpha 2024."));
    assert!(!executed_main.contains("\\citefield"));
}

#[tokio::test]
async fn internal_compiler_supports_citeurl_and_url_field_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citeurl{alpha} and \\citefield{alpha}{url}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem[Alpha 2024]{alpha} Alpha \\href{https://example.test/paper}{Paper Link}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See https://example.test/paper and https://example.test/paper."));

    let stored_aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(
        stored_aux.citation_url("alpha"),
        Some("https://example.test/paper")
    );

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(
        executed_main.contains("See https://example.test/paper and https://example.test/paper.")
    );
    assert!(!executed_main.contains("\\citeurl"));
    assert!(!executed_main.contains("\\citefield"));
}

#[tokio::test]
async fn internal_compiler_prefers_bibfield_url_for_citeurl_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citeurl{alpha} and \\citefield{alpha}{url}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibfield{url}{https://example.test/bibfield}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(
        output.contains("See https://example.test/bibfield and https://example.test/bibfield.")
    );

    let stored_aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(
        stored_aux.citation_url("alpha"),
        Some("https://example.test/bibfield")
    );

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(
        executed_main
            .contains("See https://example.test/bibfield and https://example.test/bibfield.")
    );
    assert!(!executed_main.contains("\\citeurl"));
    assert!(!executed_main.contains("\\citefield"));
}

#[tokio::test]
async fn internal_compiler_supports_citenum_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citenum{alpha} and \\citenum{alpha,beta}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta 2023]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See 1 and 1, 2."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See 1 and 1, 2."));
    assert!(!executed_main.contains("\\citenum"));
}

#[tokio::test]
async fn internal_compiler_supports_doi_and_eprint_citefield_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citefield{alpha}{doi} and \\citefield{alpha}{eprint}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem[Alpha 2024]{alpha} Alpha entry. \\doi{10.1000/example}. \\eprint{arXiv:2401.00001}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See 10.1000/example and arXiv:2401.00001."));

    let stored_aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(stored_aux.citation_doi("alpha"), Some("10.1000/example"));
    assert_eq!(
        stored_aux.citation_eprint("alpha"),
        Some("arXiv:2401.00001")
    );

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See 10.1000/example and arXiv:2401.00001."));
    assert!(!executed_main.contains("\\citefield"));
}

#[tokio::test]
async fn internal_compiler_supports_direct_identifier_citation_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citedoi{alpha}, \\citeeprint{alpha}, \\citeisbn{alpha}, and \\citeissn{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibinfo{doi}{10.1000/example}. \\bibinfo{eprint}{arXiv:2401.00001}. \\bibfield{isbn}{978-1-4028-9462-6}. \\bibfield{issn}{2049-3630}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(
        output.contains("See 10.1000/example, arXiv:2401.00001, 978-1-4028-9462-6, and 2049-3630.")
    );

    let stored_aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(stored_aux.citation_doi("alpha"), Some("10.1000/example"));
    assert_eq!(
        stored_aux.citation_eprint("alpha"),
        Some("arXiv:2401.00001")
    );
    assert_eq!(
        stored_aux.citation_field("alpha", "isbn"),
        Some("978-1-4028-9462-6")
    );
    assert_eq!(
        stored_aux.citation_field("alpha", "issn"),
        Some("2049-3630")
    );

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(
        executed_main
            .contains("See 10.1000/example, arXiv:2401.00001, 978-1-4028-9462-6, and 2049-3630.")
    );
    assert!(!executed_main.contains("\\citedoi"));
    assert!(!executed_main.contains("\\citeeprint"));
    assert!(!executed_main.contains("\\citeisbn"));
    assert!(!executed_main.contains("\\citeissn"));
}

#[tokio::test]
async fn internal_compiler_supports_citedate_and_citeurldate_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citedate{alpha}, \\Citedate{beta}, \\citeurldate{alpha}, and \\Citeurldate{beta}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem{alpha}\\bibinfo{date}{March 2024}. \\bibfield{urldate}{2024-03-01}.\\bibitem{beta}\\bibinfo{year}{2023}. \\bibfield{urldate}{2023-08-15}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See March 2024, 2023, 2024-03-01, and 2023-08-15."));

    let stored_aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(
        stored_aux.citation_field("alpha", "date"),
        Some("March 2024")
    );
    assert_eq!(stored_aux.citation_year("beta"), Some("2023".to_string()));
    assert_eq!(
        stored_aux.citation_field("alpha", "urldate"),
        Some("2024-03-01")
    );
    assert_eq!(
        stored_aux.citation_field("beta", "urldate"),
        Some("2023-08-15")
    );

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See March 2024, 2023, 2024-03-01, and 2023-08-15."));
    assert!(!executed_main.contains("\\citedate"));
    assert!(!executed_main.contains("\\Citedate"));
    assert!(!executed_main.contains("\\citeurldate"));
    assert!(!executed_main.contains("\\Citeurldate"));
}

#[tokio::test]
async fn internal_compiler_prefers_bibinfo_metadata_for_citation_fields() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citeauthor{alpha} (\\citeyear{alpha}), \\citetitle{alpha}, \\citefield{alpha}{doi}, and \\citefield{alpha}{eprint}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibinfo{author}{Alpha and Beta}. \\bibinfo{year}{2024}. \\bibinfo{title}{Exact Title}. \\bibinfo{doi}{10.1000/example}. \\bibinfo{eprint}{arXiv:2401.00001}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains(
        "See Alpha and Beta (2024), Exact Title, 10.1000/example, and arXiv:2401.00001."
    ));

    let stored_aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(
        stored_aux.citation_author("alpha"),
        Some("Alpha and Beta".to_string())
    );
    assert_eq!(stored_aux.citation_year("alpha"), Some("2024".to_string()));
    assert_eq!(stored_aux.citation_title("alpha"), Some("Exact Title"));
    assert_eq!(stored_aux.citation_doi("alpha"), Some("10.1000/example"));
    assert_eq!(
        stored_aux.citation_eprint("alpha"),
        Some("arXiv:2401.00001")
    );

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains(
        "See Alpha and Beta (2024), Exact Title, 10.1000/example, and arXiv:2401.00001."
    ));
    assert!(!executed_main.contains("\\citeauthor"));
    assert!(!executed_main.contains("\\citeyear"));
    assert!(!executed_main.contains("\\citetitle"));
    assert!(!executed_main.contains("\\citefield"));
}

#[tokio::test]
async fn internal_compiler_supports_generic_bibinfo_citefield_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citefield{alpha}{journal} and \\citefield{alpha}{pages}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibinfo{journal}{Journal of Testing}. \\bibinfo{pages}{10--20}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Journal of Testing and 10--20."));

    let stored_aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(
        stored_aux.citation_field("alpha", "journal"),
        Some("Journal of Testing")
    );
    assert_eq!(stored_aux.citation_field("alpha", "pages"), Some("10--20"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Journal of Testing and 10--20."));
    assert!(!executed_main.contains("\\citefield"));
}

#[tokio::test]
async fn internal_compiler_prefers_bibfield_metadata_for_citation_fields() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citeauthor{alpha} (\\citeyear{alpha}), \\citetitle{alpha}, and \\citefield{alpha}{journal}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibfield{author}{Alpha and Beta}. \\bibfield{year}{2024}. \\bibfield{title}{Field Title}. \\bibfield{journal}{Journal of Fields}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Alpha and Beta (2024), Field Title, and Journal of Fields."));

    let stored_aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(
        stored_aux.citation_author("alpha"),
        Some("Alpha and Beta".to_string())
    );
    assert_eq!(stored_aux.citation_year("alpha"), Some("2024".to_string()));
    assert_eq!(stored_aux.citation_title("alpha"), Some("Field Title"));
    assert_eq!(
        stored_aux.citation_field("alpha", "journal"),
        Some("Journal of Fields")
    );

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(
        executed_main.contains("See Alpha and Beta (2024), Field Title, and Journal of Fields.")
    );
    assert!(!executed_main.contains("\\citeauthor"));
    assert!(!executed_main.contains("\\citeyear"));
    assert!(!executed_main.contains("\\citetitle"));
    assert!(!executed_main.contains("\\citefield"));
}

#[tokio::test]
async fn internal_compiler_supports_textual_natbib_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citet{alpha} and \\citealt{beta} / \\citealp{beta} / \\onlinecite{beta}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(
        output.contains(
            "See Alpha (2024) and Beta et al. 2023 / Beta et al. 2023 / Beta et al. 2023."
        )
    );

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(
        executed_main.contains(
            "See Alpha (2024) and Beta et al. 2023 / Beta et al. 2023 / Beta et al. 2023."
        )
    );
    assert!(!executed_main.contains("\\citet"));
    assert!(!executed_main.contains("\\citealt"));
    assert!(!executed_main.contains("\\citealp"));
    assert!(!executed_main.contains("\\onlinecite"));
}

#[tokio::test]
async fn internal_compiler_supports_capitalized_textual_citation_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\Citet{alpha} and \\Citealt{beta} / \\Citealp{beta}. \\Textcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[alpha 2024]{alpha} Alpha entry.\\bibitem[beta et al. 2023]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(
        output.contains("See Alpha (2024) and Beta et al. 2023 / Beta et al. 2023. Alpha (2024).")
    );

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(
        executed_main
            .contains("See Alpha (2024) and Beta et al. 2023 / Beta et al. 2023. Alpha (2024).")
    );
    assert!(!executed_main.contains("\\Citet"));
    assert!(!executed_main.contains("\\Citealt"));
    assert!(!executed_main.contains("\\Citealp"));
    assert!(!executed_main.contains("\\Textcite"));
}

#[tokio::test]
async fn internal_compiler_supports_textual_natbib_notes() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\citet[see][chap.~2]{alpha} and \\citealt[e.g.][pp.~1--2]{beta}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("see Alpha (2024, chap.~2) and e.g. Beta et al. 2023, pp.~1--2."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(
        executed_main.contains("see Alpha (2024, chap.~2) and e.g. Beta et al. 2023, pp.~1--2.")
    );
    assert!(!executed_main.contains("\\citet"));
    assert!(!executed_main.contains("\\citealt"));
}

#[tokio::test]
async fn internal_compiler_supports_citetext_with_nested_citations() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citetext{compare \\citealp{beta} with \\citeyearpar{alpha}}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See (compare Beta et al. 2023 with (2024))."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See (compare Beta et al. 2023 with (2024))."));
    assert!(!executed_main.contains("\\citetext"));
    assert!(!executed_main.contains("\\citealp"));
    assert!(!executed_main.contains("\\citeyearpar"));
}

#[tokio::test]
async fn internal_compiler_supports_starred_textual_natbib_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citet*{beta}, \\citep*{beta}, \\citealt*{beta} / \\citealp*{beta}, and \\Textcite*{beta}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains(
        "See Beta and Gamma (2023), (Beta and Gamma, 2023), Beta and Gamma 2023 / Beta and Gamma 2023, and Beta and Gamma (2023)."
    ));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains(
        "See Beta and Gamma (2023), (Beta and Gamma, 2023), Beta and Gamma 2023 / Beta and Gamma 2023, and Beta and Gamma (2023)."
    ));
    assert!(!executed_main.contains("\\citet*"));
    assert!(!executed_main.contains("\\citep*"));
    assert!(!executed_main.contains("\\citealt*"));
    assert!(!executed_main.contains("\\citealp*"));
    assert!(!executed_main.contains("\\Textcite*"));
}

#[tokio::test]
async fn internal_compiler_supports_parenthetical_natbib_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citep[see][chap.~2]{alpha} and \\Citep{beta}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[beta et al. 2023]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See (see Alpha, 2024, chap.~2) and (Beta et al., 2023)."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See (see Alpha, 2024, chap.~2) and (Beta et al., 2023)."));
    assert!(!executed_main.contains("\\citep"));
    assert!(!executed_main.contains("\\Citep"));
}

#[tokio::test]
async fn internal_compiler_supports_biblatex_textual_parenthetical_and_printbibliography() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\addbibresource{refs.bib}\\textcite{alpha} and \\parencite[see][pp.~1--2]{beta}.\\printbibliography\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Alpha (2024) and (see Beta et al., 2023, pp.~1--2)."));
    assert!(output.contains("[1] Alpha entry."));
    assert!(output.contains("[2] Beta entry."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("Alpha (2024) and (see Beta et al., 2023, pp.~1--2)."));
    assert!(executed_main.contains("\\input{refs.bbl}"));
    assert!(!executed_main.contains("\\textcite"));
    assert!(!executed_main.contains("\\parencite"));
    assert!(!executed_main.contains("\\printbibliography"));
    assert!(!executed_main.contains("\\addbibresource"));
}

#[tokio::test]
async fn internal_compiler_recovers_split_preamble_biblatex_paper_family_without_stale_tail() {
    let tempdir = tempdir().expect("tempdir");
    let fixture_root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/arxiv-smoke/split-preamble-biblatex-paper-family-workflow");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 tempdir");
    fs::create_dir_all(root.as_std_path()).expect("create project root");

    let mut copy_dirs = vec![(fixture_root.clone(), root.clone())];
    while let Some((source_dir, target_dir)) = copy_dirs.pop() {
        fs::create_dir_all(target_dir.as_std_path()).expect("create target dir");
        for entry in fs::read_dir(source_dir.as_std_path())
            .expect("read source dir")
            .filter_map(|entry| entry.ok())
        {
            let source_path =
                Utf8PathBuf::from_path_buf(entry.path()).expect("fixture path should be utf8");
            let target_path = target_dir.join(entry.file_name().to_string_lossy().as_ref());
            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                copy_dirs.push((source_path, target_path));
            } else {
                fs::copy(source_path.as_std_path(), target_path.as_std_path())
                    .expect("copy fixture file");
            }
        }
    }

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");

    for rev in 1..=8u64 {
        let mut changed_files = Vec::new();
        if rev > 1 {
            let overlay_root = fixture_root.join(format!("rev{rev}"));
            if overlay_root.exists() {
                let mut overlay_dirs = vec![overlay_root.clone()];
                while let Some(source_dir) = overlay_dirs.pop() {
                    for entry in fs::read_dir(source_dir.as_std_path())
                        .expect("read overlay dir")
                        .filter_map(|entry| entry.ok())
                    {
                        let source_path = Utf8PathBuf::from_path_buf(entry.path())
                            .expect("overlay path should be utf8");
                        if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                            overlay_dirs.push(source_path);
                            continue;
                        }
                        let relative_path = source_path
                            .strip_prefix(&overlay_root)
                            .expect("overlay path should be relative to overlay root");
                        let target_path = root.join(relative_path);
                        if let Some(parent) = target_path.parent() {
                            fs::create_dir_all(parent.as_std_path()).expect("create overlay dir");
                        }
                        fs::copy(source_path.as_std_path(), target_path.as_std_path())
                            .expect("copy overlay file");
                        changed_files.push(relative_path.to_owned());
                    }
                }
            }
            let delete_path = fixture_root.join(format!("REV{rev}-DELETE.txt"));
            if delete_path.exists() {
                let deletes =
                    fs::read_to_string(delete_path.as_std_path()).expect("read delete list");
                for relative_path in deletes
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                {
                    let relative_path = Utf8PathBuf::from(relative_path);
                    let target_path = root.join(&relative_path);
                    if target_path.exists() {
                        if target_path.as_std_path().is_dir() {
                            fs::remove_dir_all(target_path.as_std_path())
                                .expect("remove directory");
                        } else {
                            fs::remove_file(target_path.as_std_path()).expect("remove file");
                        }
                    }
                    changed_files.push(relative_path);
                }
            }
            changed_files.sort();
        } else {
            changed_files.push(Utf8PathBuf::from("main.tex"));
        }

        driver
            .compile(CompileRequest {
                root: root.clone(),
                manifest: world.manifest.clone(),
                toplevel: Utf8PathBuf::from("main.tex"),
                rev,
                build_root: build_root.clone(),
                changed_files,
            })
            .await
            .unwrap_or_else(|error| panic!("rev {rev} should succeed: {error:?}"));
    }

    let output = fs::read_to_string(build_root.join("rev-8/output.txt")).expect("read output");
    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-8/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let previous_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-7/sources.json")).expect("read previous sources"),
    )
    .expect("parse previous sources");
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-8/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    let checkpoint_bundle =
        load_checkpoint_bundle(&build_root.join("rev-7/checkpoints.json")).expect("load bundle");
    let selected_checkpoint = build_meta
        .start_checkpoint_id
        .as_ref()
        .and_then(|checkpoint_id| {
            checkpoint_bundle
                .checkpoints
                .iter()
                .find(|checkpoint| &checkpoint.meta.checkpoint_id == checkpoint_id)
                .cloned()
        });
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(
        executed_main.contains(
            "See Section 1, Figure 1, and Observation Lemma A; compare Alpha (2025) and (see Beta et al., 2023, pp.~1--2)."
        ),
        "executed main: {executed_main}"
    );
    assert!(
        !executed_main.contains("and (see beta)."),
        "executed main should not retain stale degraded cite fallback: {executed_main}"
    );
    assert!(
        output.contains(
            "See Section 1, Figure 1, and Observation Lemma A; compare Alpha (2025) and (see Beta et al., 2023, pp.~1--2)."
        ),
        "compiler output: {output}\nexecuted main: {executed_main}\nselected checkpoint: {selected_checkpoint:?}\nprevious spans: {:?}\ncurrent spans: {:?}",
        previous_sources.rewrite_spans.get(&Utf8PathBuf::from("main.tex")),
        stored_sources.rewrite_spans.get(&Utf8PathBuf::from("main.tex"))
    );
    assert!(
        !output.contains(
            "References and Observation Lemma A; compare Alpha (2025) and (see Beta et al., 2023, pp.~1--2). References"
        ),
        "compiler output should not contain stale duplicated tail: {output}\nexecuted main: {executed_main}\nselected checkpoint: {selected_checkpoint:?}\nprevious spans: {:?}\ncurrent spans: {:?}",
        previous_sources.rewrite_spans.get(&Utf8PathBuf::from("main.tex")),
        stored_sources.rewrite_spans.get(&Utf8PathBuf::from("main.tex"))
    );
    assert!(
        !output.contains("and (see beta)."),
        "compiler output should not contain stale degraded cite fallback: {output}\nexecuted main: {executed_main}\nselected checkpoint: {selected_checkpoint:?}\nprevious spans: {:?}\ncurrent spans: {:?}",
        previous_sources
            .rewrite_spans
            .get(&Utf8PathBuf::from("main.tex")),
        stored_sources
            .rewrite_spans
            .get(&Utf8PathBuf::from("main.tex"))
    );
}

#[tokio::test]
async fn internal_compiler_supports_smartcite_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\smartcite{alpha} and \\smartcites[see][chap.~2]{alpha}[cf.][pp.~1--2]{beta}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains(
        "(Alpha, 2024) and (see Alpha, 2024, chap.~2; cf. Beta et al., 2023, pp.~1--2)."
    ));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains(
        "(Alpha, 2024) and (see Alpha, 2024, chap.~2; cf. Beta et al., 2023, pp.~1--2)."
    ));
    assert!(!executed_main.contains("\\smartcite"));
    assert!(!executed_main.contains("\\smartcites"));
}

#[tokio::test]
async fn internal_compiler_supports_supercite_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\supercite{alpha} and \\supercites[see]{alpha}[cf.][pp.~1--2]{beta}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("^1 and ^see 1; cf. 2, pp.~1--2."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("^1 and ^see 1; cf. 2, pp.~1--2."));
    assert!(!executed_main.contains("\\supercite"));
    assert!(!executed_main.contains("\\supercites"));
}

#[tokio::test]
async fn internal_compiler_supports_printbibliography_bibintoc_heading() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\addbibresource{refs.bib}\\printbibliography[heading=bibintoc,title={References}]\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("References"));
    assert!(output.contains("[1] Alpha entry."));
    assert!(output.contains("Contents"));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert!(aux.toc.iter().any(|entry| entry.title == "References"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("References"));
    assert!(executed_main.contains("\\input{refs.bbl}"));
    assert!(!executed_main.contains("\\printbibliography"));
    assert!(!executed_main.contains("\\addbibresource"));
}

#[tokio::test]
async fn internal_compiler_supports_printbibliography_bibnumbered_heading() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\addbibresource{refs.bib}\\printbibliography[heading=bibnumbered,title={References}]\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("2 References"));
    assert!(output.contains("[1] Alpha entry."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert!(
        aux.toc
            .iter()
            .any(|entry| entry.number == "2" && entry.title == "References")
    );

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("2 References"));
    assert!(executed_main.contains("\\input{refs.bbl}"));
    assert!(!executed_main.contains("\\printbibliography"));
}

#[tokio::test]
async fn internal_compiler_supports_printbibheading_bibnumbered_heading() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\printbibheading[heading=bibnumbered,title={References}]\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("2 References"));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert!(
        aux.toc
            .iter()
            .any(|entry| entry.number == "2" && entry.title == "References")
    );

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("2 References"));
    assert!(!executed_main.contains("\\printbibheading"));
}

#[tokio::test]
async fn internal_compiler_supports_fullcite_and_bibentry_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha} and \\bibentry{beta}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta 2023]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Alpha entry. and Beta entry.."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("Alpha entry. and Beta entry.."));
    assert!(!executed_main.contains("\\fullcite"));
    assert!(!executed_main.contains("\\bibentry"));
}

#[tokio::test]
async fn internal_compiler_supports_biblatex_multicite_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\textcites[see][chap.~2]{alpha}[cf.][pp.~1--2]{beta} and \\parencites{alpha}[cf.]{beta}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[Alpha 2024]{alpha} Alpha entry.\\bibitem[Beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains(
        "see Alpha (2024, chap.~2); cf. Beta et al. (2023, pp.~1--2) and (Alpha, 2024; cf. Beta et al., 2023)."
    ));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains(
        "see Alpha (2024, chap.~2); cf. Beta et al. (2023, pp.~1--2) and (Alpha, 2024; cf. Beta et al., 2023)."
    ));
    assert!(!executed_main.contains("\\textcites"));
    assert!(!executed_main.contains("\\parencites"));
}

#[tokio::test]
async fn internal_compiler_supports_citeyearpar_and_year_suffixes() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citeyear{alpha} and \\citeyearpar*{beta}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem[Alpha 2024a]{alpha} Alpha entry.\\bibitem[Beta et al., 2023b]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See 2024a and (2023b)."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See 2024a and (2023b)."));
    assert!(!executed_main.contains("\\citeyear"));
    assert!(!executed_main.contains("\\citeyearpar"));
}

#[tokio::test]
async fn internal_compiler_supports_citeyear_suffixes_from_natexlab_markup() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citeyear{alpha} and \\citeyearpar{beta}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        r"\begin{thebibliography}{2}\bibitem[Alpha 2024\natexlab{a}]{alpha} Alpha entry.\bibitem[Beta et al., 2023\NAT@exlab{b}]{beta} Beta entry.\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See 2024a and (2023b)."));
    assert!(!output.contains("\\natexlab"));
    assert!(!output.contains("\\NAT@exlab"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See 2024a and (2023b)."));
}

#[tokio::test]
async fn internal_compiler_supports_citefullauthor_and_capitalized_citeyearpar_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\citefullauthor{alpha} and \\Citefullauthor*{beta} in \\Citeyearpar{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\\bibitem{alpha}\\bibinfo{author}{Alpha and Beta}. \\bibinfo{year}{2024}.\\bibitem[beta et al.(2023)Beta and Gamma]{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Alpha and Beta and Beta and Gamma in (2024)."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Alpha and Beta and Beta and Gamma in (2024)."));
    assert!(!executed_main.contains("\\citefullauthor"));
    assert!(!executed_main.contains("\\Citefullauthor"));
    assert!(!executed_main.contains("\\Citeyearpar"));
}

#[tokio::test]
async fn internal_compiler_supports_citation_alias_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\defcitealias{alpha}{Paper I}See \\citetalias{alpha}, \\citepalias{alpha}, and \\Citetalias{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Paper I, (Paper I), and Paper I."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.citation_alias_text("alpha"), Some("Paper I"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Paper I, (Paper I), and Paper I."));
    assert!(!executed_main.contains("\\citetalias"));
    assert!(!executed_main.contains("\\citepalias"));
    assert!(!executed_main.contains("\\Citetalias"));
}

#[tokio::test]
async fn internal_compiler_supports_eqref_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{equation}\\label{eq:first}a\\end{equation}\\begin{equation}\\label{eq:second}b\\end{equation}See \\eqref{eq:first} and \\eqref*{eq:second}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See (1) and (2)."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See (1) and (2)."));
    assert!(!executed_main.contains("\\eqref"));
}

#[tokio::test]
async fn internal_compiler_supports_subref_and_subeqref_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\begin{figure}\\label{fig:panel}a\\end{figure}\\begin{equation}\\label{eq:panel}b\\end{equation}See \\subref{fig:panel} and \\subeqref{eq:panel}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See 1 and (1)."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See 1 and (1)."));
    assert!(!executed_main.contains("\\subref"));
    assert!(!executed_main.contains("\\subeqref"));
}

#[tokio::test]
async fn internal_compiler_supports_nameref_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}See \\nameref{sec:intro} and \\nameref*{sec:intro}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Intro and Intro."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Intro and Intro."));
    assert!(!executed_main.contains("\\nameref"));
}

#[tokio::test]
async fn internal_compiler_supports_titleref_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\begin{theorem}[Pythagoras]\\label{thm:first}a\\end{theorem}See \\titleref{sec:intro} and \\Titleref{thm:first}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Intro and Pythagoras."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Intro and Pythagoras."));
    assert!(!executed_main.contains("\\titleref"));
    assert!(!executed_main.contains("\\Titleref"));
}

#[tokio::test]
async fn internal_compiler_prefers_long_title_for_nameref_even_with_short_toc_title() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\section[Short Intro]{Long Introduction}\\label{sec:intro}See \\nameref{sec:intro}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("1 Short Intro"));
    assert!(output.contains("Long Introduction"));
    assert!(output.contains("See Long Introduction."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Long Introduction."));
    assert!(!executed_main.contains("\\nameref"));
}

#[tokio::test]
async fn internal_compiler_prefers_float_caption_title_for_nameref() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{figure}\\caption{Long Figure Title}\\label{fig:first}a\\end{figure}See \\nameref{fig:first}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Long Figure Title."));
    assert!(!output.contains("See Intro."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Long Figure Title."));
    assert!(!executed_main.contains("See Intro."));
    assert!(!executed_main.contains("\\nameref"));
}

#[tokio::test]
async fn internal_compiler_supports_autoref_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}See \\autoref{sec:intro} and \\autoref*{sec:intro}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Section 1 and Section 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Section 1 and Section 1."));
    assert!(!executed_main.contains("\\autoref"));
}

#[tokio::test]
async fn internal_compiler_supports_autoref_for_chapter_and_appendix_labels() {
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
    fs::write(root.join("book.cls"), "").expect("write class");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{book}\\begin{document}\\chapter{Intro}\\label{chap:intro}\\appendix\\chapter{Proofs}\\section{Lemma}\\label{sec:lemma}See \\autoref{chap:intro}, \\autoref*{sec:lemma}, and \\autoref{chap:proof}.\\label{chap:proof}\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Chapter 1, Appendix A.1, and Appendix A."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Chapter 1, Appendix A.1, and Appendix A."));
    assert!(!executed_main.contains("\\autoref"));
}

#[tokio::test]
async fn internal_compiler_supports_autoref_for_subsection_depth() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\subsection{Scope}\\label{sub:scope}\\subsubsection{Detail}\\label{subsub:detail}See \\autoref{sub:scope} and \\autoref{subsub:detail}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Subsection 1.1 and Subsubsection 1.1.1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Subsection 1.1 and Subsubsection 1.1.1."));
    assert!(!executed_main.contains("\\autoref"));
}

#[tokio::test]
async fn internal_compiler_supports_autoref_for_paragraph_depth() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\subsection{Scope}\\subsubsection{Detail}\\paragraph{Claim}\\label{par:claim}\\subparagraph{Case}\\label{subpar:case}See \\autoref{par:claim} and \\autoref{subpar:case}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Paragraph 1.1.1.1 and Subparagraph 1.1.1.1.1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Paragraph 1.1.1.1 and Subparagraph 1.1.1.1.1."));
    assert!(!executed_main.contains("\\autoref"));
}

#[tokio::test]
async fn internal_compiler_supports_equation_kinds_for_reference_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{equation}\\label{eq:first}a\\end{equation}\\begin{equation}\\label{eq:second}b\\end{equation}See \\autoref{eq:first}, \\cref{eq:first,eq:second}, \\namecref{eq:first}, \\vref{eq:first}, and \\crefrange{eq:first}{eq:second}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains(
        "See Equation 1, Equations 1, 2, equation, equation 1 on page 1, and Equations 1 to 2."
    ));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains(
        "See Equation 1, Equations 1, 2, equation, equation 1 on page 1, and Equations 1 to 2."
    ));
    assert!(!executed_main.contains("\\autoref"));
    assert!(!executed_main.contains("\\cref"));
    assert!(!executed_main.contains("\\namecref"));
    assert!(!executed_main.contains("\\vref"));
    assert!(!executed_main.contains("\\crefrange"));
}

#[tokio::test]
async fn internal_compiler_supports_align_labels_for_equation_reference_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{align}\\label{eq:first}a\\end{align}\\begin{gather}\\label{eq:second}b\\end{gather}See \\eqref{eq:first}, \\autoref{eq:first}, and \\crefrange{eq:first}{eq:second}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See (1), Equation 1, and Equations 1 to 2."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See (1), Equation 1, and Equations 1 to 2."));
    assert!(!executed_main.contains("\\eqref"));
    assert!(!executed_main.contains("\\autoref"));
    assert!(!executed_main.contains("\\crefrange"));
}

#[tokio::test]
async fn internal_compiler_supports_float_kinds_for_reference_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{figure}\\label{fig:first}a\\end{figure}\\begin{table}\\label{tab:first}b\\end{table}See \\autoref{fig:first}, \\cref{tab:first}, \\namecref{fig:first}, and \\vref{tab:first}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Figure 1, Table 1, figure, and table 1 on page 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Figure 1, Table 1, figure, and table 1 on page 1."));
    assert!(!executed_main.contains("\\autoref"));
    assert!(!executed_main.contains("\\cref"));
    assert!(!executed_main.contains("\\namecref"));
    assert!(!executed_main.contains("\\vref"));
}

#[tokio::test]
async fn internal_compiler_supports_float_captions_and_lists() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\listoffigures\\listoftables\\begin{figure}\\caption[Short Figure]{Long Figure Title}\\label{fig:first}a\\end{figure}\\begin{table}\\caption{Long Table Title}\\label{tab:first}b\\end{table}\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("List of Figures"));
    assert!(output.contains("Short Figure"));
    assert!(output.contains("List of Tables"));
    assert!(output.contains("Long Table Title"));
    assert!(output.contains("Figure 1: Long Figure Title"));
    assert!(output.contains("Table 1: Long Table Title"));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.float_captions.len(), 2);
    assert_eq!(aux.float_captions[0].kind, "figure");
    assert_eq!(aux.float_captions[0].title, "Short Figure");
    assert_eq!(aux.float_captions[1].kind, "table");

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("List of Figures\n1 Short Figure .... 1\n"));
    assert!(executed_main.contains("Figure 1: Long Figure Title"));
    assert!(!executed_main.contains("\\listoffigures"));
    assert!(!executed_main.contains("\\listoftables"));
    assert!(!executed_main.contains("\\caption"));
}

#[tokio::test]
async fn internal_compiler_supports_captionof_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\listoffigures\\captionof{figure}[Short Figure]{Long Figure Title}\\label{fig:first}See \\autoref{fig:first}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("List of Figures"));
    assert!(output.contains("Short Figure"));
    assert!(output.contains("Figure 1: Long Figure Title"));
    assert!(output.contains("See Figure 1."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.float_captions.len(), 1);
    assert_eq!(aux.float_captions[0].kind, "figure");
    assert_eq!(aux.labels[0].number, "1");

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("List of Figures\n1 Short Figure .... 1\n"));
    assert!(executed_main.contains("Figure 1: Long Figure Title"));
    assert!(executed_main.contains("See Figure 1."));
    assert!(!executed_main.contains("\\captionof"));
}

#[tokio::test]
async fn internal_compiler_keeps_starred_captions_out_of_float_lists() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\listoffigures\\begin{figure}\\caption*{Hidden Figure Title}\\end{figure}\\captionof*{figure}{Detached Hidden Figure}\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(!output.contains("List of Figures"));
    assert!(output.contains("Hidden Figure Title"));
    assert!(output.contains("Detached Hidden Figure"));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.float_captions.len(), 2);
    assert!(
        aux.float_captions
            .iter()
            .all(|caption| caption.number.is_empty())
    );

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(!executed_main.contains("List of Figures"));
    assert!(executed_main.contains("Hidden Figure Title"));
    assert!(executed_main.contains("Detached Hidden Figure"));
    assert!(!executed_main.contains("\\caption*"));
    assert!(!executed_main.contains("\\captionof*"));
}

#[tokio::test]
async fn internal_compiler_supports_algorithm_kinds_for_reference_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{algorithm}\\label{alg:first}a\\end{algorithm}See \\autoref{alg:first}, \\namecref{alg:first}, and \\vref{alg:first}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Algorithm 1, algorithm, and algorithm 1 on page 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Algorithm 1, algorithm, and algorithm 1 on page 1."));
    assert!(!executed_main.contains("\\autoref"));
    assert!(!executed_main.contains("\\namecref"));
    assert!(!executed_main.contains("\\vref"));
}

#[tokio::test]
async fn internal_compiler_supports_theorem_kinds_for_reference_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{theorem}\\label{thm:first}a\\end{theorem}\\begin{lemma}\\label{lem:first}b\\end{lemma}See \\autoref{thm:first}, \\cref{lem:first}, \\namecref{thm:first}, and \\vref{lem:first}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Theorem 1, Lemma 1, theorem, and lemma 1 on page 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Theorem 1, Lemma 1, theorem, and lemma 1 on page 1."));
    assert!(!executed_main.contains("\\autoref"));
    assert!(!executed_main.contains("\\cref"));
    assert!(!executed_main.contains("\\namecref"));
    assert!(!executed_main.contains("\\vref"));
}

#[tokio::test]
async fn internal_compiler_supports_newtheorem_defined_environments() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\newtheorem{oblemma}{Observation Lemma}\\section{Intro}\\begin{oblemma}\\label{obs:first}a\\end{oblemma}\\begin{oblemma}[Second]\\label{obs:second}b\\end{oblemma}See \\autoref{obs:first}, \\cref{obs:first,obs:second}, \\namecrefs{obs:first,obs:second}, and \\nameref{obs:second}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Observation Lemma 1. a"));
    assert!(output.contains("Observation Lemma 2 (Second). b"));
    assert!(output.contains(
        "See Observation Lemma 1, Observation Lemmas 1, 2, observation lemmas, and Second."
    ));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("Observation Lemma 1. a"));
    assert!(executed_main.contains("Observation Lemma 2 (Second). b"));
    assert!(executed_main.contains(
        "See Observation Lemma 1, Observation Lemmas 1, 2, observation lemmas, and Second."
    ));
    assert!(!executed_main.contains("\\newtheorem"));
    assert!(!executed_main.contains("\\begin{oblemma}"));
    assert!(!executed_main.contains("\\end{oblemma}"));
    assert!(!executed_main.contains("\\autoref"));
    assert!(!executed_main.contains("\\cref"));
    assert!(!executed_main.contains("\\namecrefs"));
    assert!(!executed_main.contains("\\nameref"));
}

#[tokio::test]
async fn internal_compiler_supports_newtheorem_shared_counters() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\newtheorem{oblemma}{Observation Lemma}\\newtheorem{obcor}[oblemma]{Observation Corollary}\\section{Intro}\\begin{oblemma}\\label{obs:first}a\\end{oblemma}\\begin{obcor}\\label{obs:second}b\\end{obcor}See \\autoref{obs:first} and \\autoref{obs:second}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Observation Lemma 1. a"));
    assert!(output.contains("Observation Corollary 2. b"));
    assert!(output.contains("See Observation Lemma 1 and Observation Corollary 2."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("Observation Lemma 1. a"));
    assert!(executed_main.contains("Observation Corollary 2. b"));
    assert!(executed_main.contains("See Observation Lemma 1 and Observation Corollary 2."));
    assert!(!executed_main.contains("\\newtheorem"));
    assert!(!executed_main.contains("\\begin{oblemma}"));
    assert!(!executed_main.contains("\\begin{obcor}"));
    assert!(!executed_main.contains("\\autoref"));
}

#[tokio::test]
async fn internal_compiler_supports_newtheorem_section_scoped_counters() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\newtheorem{oblemma}{Observation Lemma}[section]\\section{Intro}\\begin{oblemma}\\label{obs:first}a\\end{oblemma}\\section{Next}\\begin{oblemma}\\label{obs:second}b\\end{oblemma}See \\autoref{obs:first} and \\autoref{obs:second}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Observation Lemma 1.1. a"));
    assert!(output.contains("Observation Lemma 2.1. b"));
    assert!(output.contains("See Observation Lemma 1.1 and Observation Lemma 2.1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("Observation Lemma 1.1. a"));
    assert!(executed_main.contains("Observation Lemma 2.1. b"));
    assert!(executed_main.contains("See Observation Lemma 1.1 and Observation Lemma 2.1."));
    assert!(!executed_main.contains("\\newtheorem"));
    assert!(!executed_main.contains("\\autoref"));
}

#[tokio::test]
async fn internal_compiler_prefers_newtheorem_override_for_builtin_env_and_shared_scope() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\newtheorem{theorem}{Theorem}[section]\\newtheorem{cor}[theorem]{Corollary}\\section{Intro}\\begin{theorem}\\label{thm:first}a\\end{theorem}\\begin{cor}\\label{cor:first}b\\end{cor}\\section{Next}\\begin{theorem}\\label{thm:second}c\\end{theorem}\\begin{cor}\\label{cor:second}d\\end{cor}See \\autoref{thm:first}, \\autoref{cor:first}, \\autoref{thm:second}, and \\autoref{cor:second}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Theorem 1.1. a"));
    assert!(output.contains("Corollary 1.2. b"));
    assert!(output.contains("Theorem 2.1. c"));
    assert!(output.contains("Corollary 2.2. d"));
    assert!(output.contains("See Theorem 1.1, Corollary 1.2, Theorem 2.1, and Corollary 2.2."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("Theorem 1.1. a"));
    assert!(executed_main.contains("Corollary 1.2. b"));
    assert!(executed_main.contains("Theorem 2.1. c"));
    assert!(executed_main.contains("Corollary 2.2. d"));
    assert!(!executed_main.contains("\\newtheorem"));
    assert!(!executed_main.contains("\\autoref"));
}

#[tokio::test]
async fn internal_compiler_strips_theoremstyle_declarations() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\theoremstyle{definition}\\newtheoremstyle{tight}{}{}{}{}{}{}{ }{}\\swapnumbers\\newtheorem{oblemma}{Observation Lemma}\\section{Intro}\\begin{oblemma}\\label{obs:first}a\\end{oblemma}See \\autoref{obs:first}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Observation Lemma 1. a"));
    assert!(output.contains("See Observation Lemma 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("Observation Lemma 1. a"));
    assert!(executed_main.contains("See Observation Lemma 1."));
    assert!(!executed_main.contains("\\theoremstyle"));
    assert!(!executed_main.contains("\\newtheoremstyle"));
    assert!(!executed_main.contains("\\swapnumbers"));
}

#[tokio::test]
async fn internal_compiler_materializes_theorem_and_proof_environment_headers() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{theorem}[Pythagoras]\\label{thm:first}a\\end{theorem}\\begin{proof}[Sketch]See \\nameref{thm:first}.\\end{proof}\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Theorem 1 (Pythagoras). aProof (Sketch). See Pythagoras."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("Theorem 1 (Pythagoras). aProof (Sketch). See Pythagoras."));
    assert!(!executed_main.contains("\\begin{theorem}"));
    assert!(!executed_main.contains("\\end{theorem}"));
    assert!(!executed_main.contains("\\begin{proof}"));
    assert!(!executed_main.contains("\\end{proof}"));
    assert!(!executed_main.contains("\\nameref"));
}

#[tokio::test]
async fn internal_compiler_supports_claim_and_example_reference_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{claim}\\label{clm:first}a\\end{claim}\\begin{example}\\label{ex:first}b\\end{example}See \\autoref{clm:first}, \\cref{ex:first}, \\namecref{clm:first}, and \\vref{ex:first}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Claim 1, Example 1, claim, and example 1 on page 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Claim 1, Example 1, claim, and example 1 on page 1."));
    assert!(!executed_main.contains("\\autoref"));
    assert!(!executed_main.contains("\\cref"));
    assert!(!executed_main.contains("\\namecref"));
    assert!(!executed_main.contains("\\vref"));
}

#[tokio::test]
async fn internal_compiler_supports_axiom_fact_and_observation_reference_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{axiom}\\label{ax:first}a\\end{axiom}\\begin{fact}\\label{fact:first}b\\end{fact}\\begin{observation}\\label{obs:first}c\\end{observation}See \\thmref{ax:first}, \\cref{fact:first}, \\namecref{obs:first}, and \\vref{fact:first}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Axiom 1, Fact 1, observation, and fact 1 on page 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Axiom 1, Fact 1, observation, and fact 1 on page 1."));
    assert!(!executed_main.contains("\\thmref"));
    assert!(!executed_main.contains("\\cref"));
    assert!(!executed_main.contains("\\namecref"));
    assert!(!executed_main.contains("\\vref"));
}

#[tokio::test]
async fn internal_compiler_supports_problem_exercise_question_and_notation_reference_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{problem}\\label{prob:first}a\\end{problem}\\begin{exercise}\\label{ex:first}b\\end{exercise}\\begin{question}\\label{q:first}c\\end{question}\\begin{notation}\\label{not:first}d\\end{notation}See \\thmref{prob:first}, \\cref{ex:first}, \\namecref{not:first}, and \\vref{q:first}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("internal compile should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Problem 1, Exercise 1, notation, and question 1 on page 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(
        executed_main.contains("See Problem 1, Exercise 1, notation, and question 1 on page 1.")
    );
    assert!(!executed_main.contains("\\thmref"));
    assert!(!executed_main.contains("\\cref"));
    assert!(!executed_main.contains("\\namecref"));
    assert!(!executed_main.contains("\\vref"));
}

#[tokio::test]
async fn internal_compiler_supports_thmref_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\begin{theorem}\\label{thm:first}a\\end{theorem}\\begin{claim}\\label{clm:first}b\\end{claim}See \\thmref{thm:first} and \\Thmref{clm:first}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Theorem 1 and Claim 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Theorem 1 and Claim 1."));
    assert!(!executed_main.contains("\\thmref"));
    assert!(!executed_main.contains("\\Thmref"));
}

#[tokio::test]
async fn internal_compiler_supports_fullref_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\begin{theorem}[Pythagoras]\\label{thm:first}a\\end{theorem}See \\fullref{sec:intro} and \\Fullref{thm:first}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Section 1 (Intro) and Theorem 1 (Pythagoras)."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Section 1 (Intro) and Theorem 1 (Pythagoras)."));
    assert!(!executed_main.contains("\\fullref"));
    assert!(!executed_main.contains("\\Fullref"));
}

#[tokio::test]
async fn internal_compiler_supports_plural_namecref_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Scope}\\label{sub:scope}\\subsection{Detail}\\label{sub:detail}\\paragraph{Claim}\\label{par:claim}\\paragraph{Case}\\label{par:case}\\begin{theorem}\\label{thm:first}a\\end{theorem}\\begin{lemma}\\label{lem:first}b\\end{lemma}See \\namecrefs{sub:scope,sub:detail}, \\nameCrefs{par:claim,par:case}, and \\lcnamecrefs{thm:first,lem:first}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See section; subsection, Paragraphs, and theorem; lemma."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See section; subsection, Paragraphs, and theorem; lemma."));
    assert!(!executed_main.contains("\\namecrefs"));
    assert!(!executed_main.contains("\\nameCrefs"));
    assert!(!executed_main.contains("\\lcnamecrefs"));
}

#[tokio::test]
async fn internal_compiler_supports_labelcref_and_labelcpageref_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\begin{equation}\\label{eq:first}a\\end{equation}\\subsection{Detail}\\label{sub:detail}See \\labelcref{sec:intro,eq:first} and \\labelcpageref{sec:intro,sub:detail}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See 1, (1) and 1, 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See 1, (1) and 1, 1."));
    assert!(!executed_main.contains("\\labelcref"));
    assert!(!executed_main.contains("\\labelcpageref"));
}

#[tokio::test]
async fn internal_compiler_strips_float_and_equation_environment_wrappers() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\begin{figure}\\caption{Long Figure Title}\\label{fig:first}a\\end{figure}\\begin{equation}\\label{eq:first}b\\end{equation}\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Figure 1: Long Figure Titleab"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("Figure 1: Long Figure Titleab"));
    assert!(!executed_main.contains("\\begin{figure}"));
    assert!(!executed_main.contains("\\end{figure}"));
    assert!(!executed_main.contains("\\begin{equation}"));
    assert!(!executed_main.contains("\\end{equation}"));
}

#[tokio::test]
async fn internal_compiler_supports_cleveref_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}See \\cref{sec:intro} and \\Cref*{sec:intro}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Section 1 and Section 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Section 1 and Section 1."));
    assert!(!executed_main.contains("\\cref"));
    assert!(!executed_main.contains("\\Cref"));
}

#[tokio::test]
async fn internal_compiler_supports_namecref_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\subsection{Scope}\\label{sub:scope}\\subsubsection{Detail}\\label{subsub:detail}See \\namecref{sec:intro}, \\nameCref{sub:scope}, and \\lcnamecref{subsub:detail}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See section, Subsection, and subsubsection."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See section, Subsection, and subsubsection."));
    assert!(!executed_main.contains("\\namecref"));
    assert!(!executed_main.contains("\\nameCref"));
    assert!(!executed_main.contains("\\lcnamecref"));
}

#[tokio::test]
async fn internal_compiler_supports_crefrange_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\subsection{Scope}\\label{sub:scope}\\subsection{Detail}\\label{sub:detail}\\paragraph{Claim}\\label{par:claim}\\paragraph{Case}\\label{par:case}See \\crefrange{sub:scope}{sub:detail} and \\Crefrange{par:claim}{par:case}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Subsections 1.1 to 1.2 and Paragraphs 1.2.1 to 1.2.2."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See Subsections 1.1 to 1.2 and Paragraphs 1.2.1 to 1.2.2."));
    assert!(!executed_main.contains("\\crefrange"));
    assert!(!executed_main.contains("\\Crefrange"));
}

#[tokio::test]
async fn internal_compiler_supports_page_oriented_ref_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\subsection{Scope}\\label{sub:scope}See \\cpageref{sec:intro}, \\Cpageref{sub:scope}, \\vpageref{sub:scope}, \\autopageref{sec:intro}, \\vref{sec:intro}, and \\Vref{sub:scope}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains(
        "See page 1, Page 1, page 1, page 1, section 1 on page 1, and Subsection 1.1 on page 1."
    ));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains(
        "See page 1, Page 1, page 1, page 1, section 1 on page 1, and Subsection 1.1 on page 1."
    ));
    assert!(!executed_main.contains("\\cpageref"));
    assert!(!executed_main.contains("\\Cpageref"));
    assert!(!executed_main.contains("\\vpageref"));
    assert!(!executed_main.contains("\\autopageref"));
    assert!(!executed_main.contains("\\vref"));
    assert!(!executed_main.contains("\\Vref"));
}

#[tokio::test]
async fn internal_compiler_supports_pagerefrange_variant() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\section{Scope}\\label{sub:scope}See \\pagerefrange{sec:intro}{sub:scope}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See pages 1 to 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See pages 1 to 1."));
    assert!(!executed_main.contains("\\pagerefrange"));
}

#[tokio::test]
async fn internal_compiler_supports_varioref_range_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\section{Scope}\\label{sub:scope}See \\vpagerefrange{sec:intro}{sub:scope} and \\vrefrange{sec:intro}{sub:scope}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See pages 1 to 1 and section 1 on page 1 to section 2 on page 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(
        executed_main.contains("See pages 1 to 1 and section 1 on page 1 to section 2 on page 1.")
    );
    assert!(!executed_main.contains("\\vpagerefrange"));
    assert!(!executed_main.contains("\\vrefrange"));
}

#[tokio::test]
async fn internal_compiler_supports_cpagerefrange_variants() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\label{sec:intro}\\section{Scope}\\label{sub:scope}See \\cpagerefrange{sec:intro}{sub:scope} and \\Cpagerefrange{sec:intro}{sub:scope}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See pages 1 to 1 and Pages 1 to 1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("See pages 1 to 1 and Pages 1 to 1."));
    assert!(!executed_main.contains("\\cpagerefrange"));
    assert!(!executed_main.contains("\\Cpagerefrange"));
}

#[tokio::test]
async fn internal_compiler_supports_multi_label_cleveref_for_subsection_pluralization() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\section{Intro}\\subsection{Scope}\\label{sub:scope}\\subsection{Detail}\\label{sub:detail}\\subsubsection{Inner}\\label{subsub:detail}See \\cref{sub:scope,sub:detail} and \\cref{sub:scope,subsub:detail}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Subsections 1.1, 1.2 and Subsection 1.1; Subsubsection 1.2.1."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(
        executed_main.contains("See Subsections 1.1, 1.2 and Subsection 1.1; Subsubsection 1.2.1.")
    );
    assert!(!executed_main.contains("\\cref"));
}

#[tokio::test]
async fn internal_compiler_supports_multi_label_cleveref_lists() {
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
    fs::write(root.join("book.cls"), "").expect("write class");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{book}\\begin{document}\\chapter{Intro}\\section{Setup}\\label{sec:setup}\\section{Scope}\\label{sec:scope}\\appendix\\chapter{Proofs}\\label{chap:proof}See \\cref{sec:setup,sec:scope}, \\Cref{chap:proof}, and \\cref{sec:scope,chap:proof}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See Sections 1.1, 1.2, Appendix A, and Section 1.2; Appendix A."));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(
        executed_main.contains("See Sections 1.1, 1.2, Appendix A, and Section 1.2; Appendix A.")
    );
    assert!(!executed_main.contains("\\cref"));
    assert!(!executed_main.contains("\\Cref"));
}

#[tokio::test]
async fn internal_compiler_supports_subsubsection_toc_and_label_numbering() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\section{Intro}\\subsection{Scope}\\subsubsection{Detail}\\label{sec:detail}See \\ref{sec:detail}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Contents"));
    assert!(output.contains("1 Intro"));
    assert!(output.contains("1.1 Scope"));
    assert!(output.contains("1.1.1 Detail"));
    assert!(output.contains("See 1.1.1."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.toc.len(), 3);
    assert_eq!(aux.toc[2].title, "Detail");
    assert_eq!(aux.toc[2].level, 3);
    assert_eq!(aux.toc[2].number, "1.1.1");
    assert_eq!(aux.labels[0].number, "1.1.1");

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert!(stored_sources.executed_files[&Utf8PathBuf::from("main.tex")].contains("See 1.1.1."));
}

#[tokio::test]
async fn internal_compiler_supports_chapter_toc_and_label_numbering() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\chapter{Intro}\\section{Scope}\\label{sec:scope}See \\ref{sec:scope}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Contents"));
    assert!(output.contains("1 Intro"));
    assert!(output.contains("1.1 Scope"));
    assert!(output.contains("See 1.1."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.toc.len(), 2);
    assert_eq!(aux.toc[0].level, 0);
    assert_eq!(aux.toc[0].number, "1");
    assert_eq!(aux.toc[1].number, "1.1");
    assert_eq!(aux.labels[0].number, "1.1");
}

#[tokio::test]
async fn internal_compiler_uses_optional_section_title_for_toc_and_long_title_for_body() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\section[Short Intro]{Long Introduction}\\label{sec:intro}See \\ref{sec:intro}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Contents"));
    assert!(output.contains("1 Short Intro"));
    assert!(output.contains("Long Introduction"));
    assert!(output.contains("See 1."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.toc.len(), 1);
    assert_eq!(aux.toc[0].title, "Short Intro");
    assert_eq!(aux.labels[0].number, "1");

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("Long Introduction"));
    assert!(!executed_main.contains("\\section[Short Intro]"));
}

#[tokio::test]
async fn internal_compiler_switches_to_appendix_letter_numbering() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\section{Intro}\\appendix\\section{Proofs}\\subsection{Lemma}\\label{sec:lemma}See \\ref{sec:lemma}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("1 Intro"));
    assert!(output.contains("A Proofs"));
    assert!(output.contains("A.1 Lemma"));
    assert!(output.contains("See A.1."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.toc.len(), 3);
    assert_eq!(aux.toc[1].number, "A");
    assert_eq!(aux.toc[2].number, "A.1");
    assert_eq!(aux.labels[0].number, "A.1");

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert!(stored_sources.executed_files[&Utf8PathBuf::from("main.tex")].contains("See A.1."));
}

#[tokio::test]
async fn internal_compiler_switches_to_appendices_letter_numbering() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\section{Intro}\\appendices\\section{Proofs}\\subsection{Lemma}\\label{sec:lemma}See \\ref{sec:lemma}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("A Proofs"));
    assert!(output.contains("A.1 Lemma"));
    assert!(output.contains("See A.1."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.toc[1].number, "A");
    assert_eq!(aux.toc[2].number, "A.1");
    assert_eq!(aux.labels[0].number, "A.1");
}

#[tokio::test]
async fn internal_compiler_switches_chapter_appendices_to_letter_numbering() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\chapter{Intro}\\appendix\\chapter{Proofs}\\section{Lemma}\\label{sec:lemma}See \\ref{sec:lemma}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("1 Intro"));
    assert!(output.contains("A Proofs"));
    assert!(output.contains("A.1 Lemma"));
    assert!(output.contains("See A.1."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.toc.len(), 3);
    assert_eq!(aux.toc[1].number, "A");
    assert_eq!(aux.toc[2].number, "A.1");
    assert_eq!(aux.labels[0].number, "A.1");
}

#[tokio::test]
async fn internal_compiler_preserves_bibliography_stem_order_across_multiple_bbl_files() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\cite{beta} then \\cite{alpha}.\\bibliography{refsb,refsa}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{beta} Beta entry.\\end{thebibliography}",
    )
    .expect("write refsb");
    fs::write(
        root.join("refsa.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
    )
    .expect("write refsa");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("refsa.bbl"),
            ],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See [1] then [2]."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.bibliography.len(), 2);
    assert_eq!(aux.bibliography[0].key, "beta");
    assert_eq!(aux.bibliography[1].key, "alpha");
}

#[tokio::test]
async fn internal_compiler_keeps_starred_section_title_out_of_toc() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\section*{Prelude}\\section{Intro}\\label{sec:intro}See \\ref{sec:intro}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Contents"));
    assert!(output.contains("Prelude"));
    assert!(output.contains("1 Intro"));
    assert!(!output.contains("Prelude ...."));
    assert!(output.contains("See 1."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.toc.len(), 1);
    assert_eq!(aux.toc[0].title, "Intro");
    assert_eq!(aux.toc[0].number, "1");

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(executed_main.contains("Prelude"));
    assert!(!executed_main.contains("\\section*"));
}

#[tokio::test]
async fn internal_compiler_strips_href_targets_from_bibliography_output() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}See \\cite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha \\href{https://example.test/paper}{Paper Link}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("See [1]."));
    assert!(output.contains("Alpha Paper Link."));
    assert!(!output.contains("https://example.test/paper"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] Alpha Paper Link."
    );
}

#[tokio::test]
async fn internal_compiler_strips_urlprefix_and_renders_bibnamedash_in_bibliography_output() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}\\bibnamedash. \\urlprefix\\url{https://example.test/paper}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("---. https://example.test/paper."));
    assert!(!output.contains("\\urlprefix"));
    assert!(!output.contains("\\bibnamedash"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] ---. https://example.test/paper."
    );
}

#[tokio::test]
async fn internal_compiler_strips_common_biblatex_bibliography_wrappers() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}\\mkbibquote{Alpha Title}. \\mkbibparens{2024}. \\mkbibbrackets{note}. \\mkbibemph{Emph}. \\mkbibbold{Bold}. \\mkbibitalic{Italic}. \\enquote{Nested}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("\"Alpha Title\". (2024). [note]. Emph. Bold. Italic. \"Nested\"."));
    assert!(!output.contains("\\mkbibquote"));
    assert!(!output.contains("\\mkbibparens"));
    assert!(!output.contains("\\mkbibbrackets"));
    assert!(!output.contains("\\mkbibemph"));
    assert!(!output.contains("\\mkbibbold"));
    assert!(!output.contains("\\mkbibitalic"));
    assert!(!output.contains("\\enquote"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] \"Alpha Title\". (2024). [note]. Emph. Bold. Italic. \"Nested\"."
    );
}

#[tokio::test]
async fn internal_compiler_strips_newunit_finentry_and_renders_addpunct_wrappers() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}Alpha\\addcomma\\addspace Beta\\newunit Gamma\\addcolon\\addspace Delta\\addsemicolon\\addspace Epsilon\\adddot\\finentry\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Alpha, Beta Gamma: Delta; Epsilon."));
    assert!(!output.contains("\\newunit"));
    assert!(!output.contains("\\finentry"));
    assert!(!output.contains("\\addcomma"));
    assert!(!output.contains("\\addspace"));
    assert!(!output.contains("\\addcolon"));
    assert!(!output.contains("\\addsemicolon"));
    assert!(!output.contains("\\adddot"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] Alpha, Beta Gamma: Delta; Epsilon."
    );
}

#[tokio::test]
async fn internal_compiler_strips_bibstring_and_mkbibacro_wrappers() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}\\mkbibnamefamily{Alpha} \\bibstring{andothers}. \\mkbibacro{URL}: \\url{https://example.test/paper}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Alpha et al. URL: https://example.test/paper."));
    assert!(!output.contains("\\bibstring"));
    assert!(!output.contains("\\mkbibacro"));
    assert!(!output.contains("\\mkbibnamefamily"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] Alpha et al. URL: https://example.test/paper."
    );
}

#[tokio::test]
async fn internal_compiler_strips_parentext_and_spacing_wrappers() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}Alpha\\addabbrvspace Beta\\addnbspace Gamma\\addthinspace Delta\\addlowpenspace Epsilon\\addhighpenspace Zeta\\parentext{Supplement}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Alpha Beta Gamma Delta Epsilon Zeta (Supplement)."));
    assert!(!output.contains("\\addabbrvspace"));
    assert!(!output.contains("\\addnbspace"));
    assert!(!output.contains("\\addthinspace"));
    assert!(!output.contains("\\addlowpenspace"));
    assert!(!output.contains("\\addhighpenspace"));
    assert!(!output.contains("\\parentext"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] Alpha Beta Gamma Delta Epsilon Zeta (Supplement)."
    );
}

#[tokio::test]
async fn internal_compiler_strips_dash_and_slash_wrappers() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}Pages 10\\bibrangedash20\\addcomma\\addspace Vol\\adddot 2\\addslash Issue 3\\addhyphen4\\textendash5\\textemdash appendix.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(
        output.contains("Pages 10-20, Vol. 2/Issue 3-4-5--- appendix."),
        "compiler output: {output}"
    );
    assert!(!output.contains("\\bibrangedash"));
    assert!(!output.contains("\\addslash"));
    assert!(!output.contains("\\addhyphen"));
    assert!(!output.contains("\\textendash"));
    assert!(!output.contains("\\textemdash"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] Pages 10-20, Vol. 2/Issue 3-4-5--- appendix."
    );
}

#[tokio::test]
async fn internal_compiler_strips_low_level_punctuation_helpers() {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let refs = "\\begin{thebibliography}{1}\\bibitem{alpha}Alpha\\adddotspace Beta\\unspace\\isdot\\nopunct Gamma\\isdot \\bibopenparen Delta\\bibcloseparen \\bibopenbracket Epsilon\\bibclosebracket \\bibopenbrace Zeta\\bibclosebrace\\end{thebibliography}";
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(root.join("refs.bbl"), refs).expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Alpha. Beta. Gamma. (Delta) [Epsilon] Zeta"));
    assert!(!output.contains("\\adddotspace"));
    assert!(!output.contains("\\unspace"));
    assert!(!output.contains("\\isdot"));
    assert!(!output.contains("\\nopunct"));
    assert!(!output.contains("\\bibopenparen"));
    assert!(!output.contains("\\bibcloseparen"));
    assert!(!output.contains("\\bibopenbracket"));
    assert!(!output.contains("\\bibclosebracket"));
    assert!(!output.contains("\\bibopenbrace"));
    assert!(!output.contains("\\bibclosebrace"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] Alpha. Beta. Gamma. (Delta) [Epsilon] {Zeta}"
    );
}

#[tokio::test]
async fn internal_compiler_strips_superscript_subscript_and_braces_wrappers() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}Edition\\mkbibsuperscript{2}\\mkbibsubscript{a} \\mkbibbraces{Supplement}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(
        output.contains("Edition2a Supplement."),
        "compiler output: {output}"
    );
    assert!(!output.contains("\\mkbibsuperscript"));
    assert!(!output.contains("\\mkbibsubscript"));
    assert!(!output.contains("\\mkbibbraces"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] Edition2a {Supplement}."
    );
}

#[tokio::test]
async fn internal_compiler_strips_nolinkurl_path_and_detokenize_wrappers() {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let refs = "\\begin{thebibliography}{1}\\bibitem{alpha}Source: \\nolinkurl{https://example.test/paper} at \\path{/tmp/archive} via \\detokenize{arXiv:2401.01234}.\\end{thebibliography}";
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(root.join("refs.bbl"), refs).expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(
        output.contains("Source: https://example.test/paper at /tmp/archive via arXiv:2401.01234.")
    );
    assert!(!output.contains("\\nolinkurl"));
    assert!(!output.contains("\\path"));
    assert!(!output.contains("\\detokenize"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] Source: https://example.test/paper at /tmp/archive via arXiv:2401.01234."
    );
}

#[tokio::test]
async fn internal_compiler_strips_case_textstyle_and_textsuper_wrappers() {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let refs = "\\begin{thebibliography}{1}\\bibitem{alpha}\\NoCaseChange{NASA}. \\MakeSentenceCase{alpha title}. \\MakeTitleCase{beta title}. \\protect\\relax\\leavevmode\\ignorespaces   \\emph{Emph}. Trimmed \\unskip. \\phantom{Ghost}\\hphantom{Wide}\\vphantom{Tall}Visible. Tight\\!Join. Soft\\,Gap. Wide\\;Gap. Colon\\:Gap. Named\\space Gap. Backslash\\ Gap. Quote\\textquotesingle s. Double\\textquotedbl q. Angles\\textless x\\textgreater. Pipe\\textbar join. Path\\slash name. \\mbox{Stable}. \\hbox{Fixed}. \\fbox{Framed}. \\framebox[2em][c]{Wide}. \\raisebox{0.5ex}[1ex][0ex]{Raised}. \\parbox[t]{4em}{Paragraph}. \\makebox[3em][l]{Inline}. \\texttt{Code}. \\textsf{Sans}. \\textsc{Caps}. \\textbf{Bold}. \\textit{Italic}. \\textrm{Roman}. \\textup{Upright}. \\textmd{Medium}. \\textnormal{Normal}. Edition\\textsuperscript{2}\\textsubscript{a}.\\end{thebibliography}";
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(root.join("refs.bbl"), refs).expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("NASA. alpha title. beta title. Emph. Trimmed. Visible. TightJoin. Soft Gap. Wide Gap. Colon Gap. Named Gap. Backslash Gap. Quote's. Double\"q. Angles<x>. Pipe|join. Path/name. Stable. Fixed. Framed. Wide. Raised. Paragraph. Inline. Code. Sans. Caps. Bold. Italic. Roman. Upright. Medium. Normal. Edition2a."));
    assert!(!output.contains("\\NoCaseChange"));
    assert!(!output.contains("\\MakeSentenceCase"));
    assert!(!output.contains("\\MakeTitleCase"));
    assert!(!output.contains("\\protect"));
    assert!(!output.contains("\\relax"));
    assert!(!output.contains("\\leavevmode"));
    assert!(!output.contains("\\ignorespaces"));
    assert!(!output.contains("\\unskip"));
    assert!(!output.contains("\\emph"));
    assert!(!output.contains("\\mbox"));
    assert!(!output.contains("\\hbox"));
    assert!(!output.contains("\\fbox"));
    assert!(!output.contains("\\framebox"));
    assert!(!output.contains("\\raisebox"));
    assert!(!output.contains("\\parbox"));
    assert!(!output.contains("\\makebox"));
    assert!(!output.contains("\\phantom"));
    assert!(!output.contains("\\hphantom"));
    assert!(!output.contains("\\vphantom"));
    assert!(!output.contains("\\!"));
    assert!(!output.contains("\\,"));
    assert!(!output.contains("\\;"));
    assert!(!output.contains("\\:"));
    assert!(!output.contains("\\space"));
    assert!(!output.contains("\\ Gap"));
    assert!(!output.contains("\\textquotesingle"));
    assert!(!output.contains("\\textquotedbl"));
    assert!(!output.contains("\\textless"));
    assert!(!output.contains("\\textgreater"));
    assert!(!output.contains("\\textbar"));
    assert!(!output.contains("\\slash"));
    assert!(!output.contains("\\texttt"));
    assert!(!output.contains("\\textsf"));
    assert!(!output.contains("\\textsc"));
    assert!(!output.contains("\\textbf"));
    assert!(!output.contains("\\textit"));
    assert!(!output.contains("\\textrm"));
    assert!(!output.contains("\\textup"));
    assert!(!output.contains("\\textmd"));
    assert!(!output.contains("\\textnormal"));
    assert!(!output.contains("\\textsuperscript"));
    assert!(!output.contains("\\textsubscript"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] NASA. alpha title. beta title. Emph. Trimmed. Visible. TightJoin. Soft Gap. Wide Gap. Colon Gap. Named Gap. Backslash Gap. Quote's. Double\"q. Angles<x>. Pipe|join. Path/name. Stable. Fixed. Framed. Wide. Raised. Paragraph. Inline. Code. Sans. Caps. Bold. Italic. Roman. Upright. Medium. Normal. Edition2a."
    );
}

#[tokio::test]
async fn internal_compiler_strips_urlstyle_wrapper() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}\\urlstyle{same}\\url{https://example.test/paper}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("https://example.test/paper."));
    assert!(!output.contains("\\urlstyle"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] https://example.test/paper."
    );
}

#[tokio::test]
async fn internal_compiler_strips_starred_case_wrappers() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}\\MakeSentenceCase*{alpha title}. \\MakeTitleCase*{beta title}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("alpha title. beta title."));
    assert!(!output.contains("\\MakeSentenceCase*"));
    assert!(!output.contains("\\MakeTitleCase*"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] alpha title. beta title."
    );
}

#[tokio::test]
async fn internal_compiler_strips_starred_formatting_wrappers() {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let refs = "\\begin{thebibliography}{1}\\bibitem{alpha}\\mkbibquote*{Alpha Title}. \\mkbibparens*{2024}. \\mkbibbrackets*{note}. \\mkbibbraces*{Supplement}. \\mkbibemph*{Emph}. \\mkbibbold*{Bold}. \\mkbibitalic*{Italic}.\\end{thebibliography}";
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(root.join("refs.bbl"), refs).expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("\"Alpha Title\". (2024). [note]. Supplement. Emph. Bold. Italic."));
    assert!(!output.contains("\\mkbibquote*"));
    assert!(!output.contains("\\mkbibparens*"));
    assert!(!output.contains("\\mkbibbrackets*"));
    assert!(!output.contains("\\mkbibbraces*"));
    assert!(!output.contains("\\mkbibemph*"));
    assert!(!output.contains("\\mkbibbold*"));
    assert!(!output.contains("\\mkbibitalic*"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] \"Alpha Title\". (2024). [note]. {Supplement}. Emph. Bold. Italic."
    );
}

#[tokio::test]
async fn internal_compiler_strips_name_affix_wrapper() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha}\\mkbibnamefamily{Doe}, \\mkbibnameaffix{Jr.}.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Doe, Jr.."));
    assert!(!output.contains("\\mkbibnamefamily"));
    assert!(!output.contains("\\mkbibnameaffix"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert_eq!(
        stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")],
        "[1] Doe, Jr.."
    );
}

#[tokio::test]
async fn internal_compiler_strips_natexlab_suffix_markup_in_bibliography_output() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\fullcite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        r"\begin{thebibliography}{1}\bibitem[Alpha 2024\natexlab{a}]{alpha} Alpha \newblock 2024\NAT@exlab{a}.\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Alpha 2024a."));
    assert!(!output.contains("\\natexlab"));
    assert!(!output.contains("\\NAT@exlab"));

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    assert!(stored_sources.executed_files[&Utf8PathBuf::from("refs.bbl")].contains("Alpha 2024a."));
}

#[tokio::test]
async fn internal_compiler_follows_include_files_for_semantic_aux_sections_and_labels() {
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
    fs::create_dir_all(root.join("chapters")).expect("chapters dir");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\include{chapters/intro}See \\ref{sec:intro}.\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("chapters/intro.tex"),
        "\\section{Intro}\\label{sec:intro}",
    )
    .expect("write intro");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let outcome = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("chapters/intro.tex"),
            ],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Contents"));
    assert!(output.contains("1 Intro"));
    assert!(output.contains("See 1."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.toc.len(), 1);
    assert_eq!(aux.toc[0].title, "Intro");
    assert_eq!(aux.labels.len(), 1);
    assert_eq!(aux.labels[0].key, "sec:intro");
    assert_eq!(aux.labels[0].number, "1");
    assert!(
        outcome
            .dep_trace
            .inputs
            .contains(&Utf8PathBuf::from("chapters/intro.tex"))
    );
}

#[tokio::test]
async fn internal_compiler_honors_includeonly_for_include_files() {
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
    fs::create_dir_all(root.join("chapters")).expect("chapters dir");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\includeonly{chapters/intro}\\include{chapters/intro}\\include{chapters/extra}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("chapters/intro.tex"),
        "\\section{Intro}\\label{sec:intro}",
    )
    .expect("write intro");
    fs::write(
        root.join("chapters/extra.tex"),
        "\\section{Extra}\\label{sec:extra}",
    )
    .expect("write extra");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let outcome = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("chapters/intro.tex"),
                Utf8PathBuf::from("chapters/extra.tex"),
            ],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("1 Intro"));
    assert!(!output.contains("Extra"));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.toc.len(), 1);
    assert_eq!(aux.toc[0].title, "Intro");
    assert_eq!(aux.labels.len(), 1);
    assert_eq!(aux.labels[0].key, "sec:intro");
    assert!(
        outcome
            .dep_trace
            .inputs
            .contains(&Utf8PathBuf::from("chapters/intro.tex"))
    );
    assert!(
        !outcome
            .dep_trace
            .inputs
            .contains(&Utf8PathBuf::from("chapters/extra.tex"))
    );
}

#[tokio::test]
async fn internal_compiler_supports_manual_toc_entries_for_starred_sections() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\section*{Prelude}\\phantomsection\\addcontentsline{toc}{section}{Prelude}\\section{Intro}\\label{sec:intro}See \\ref{sec:intro}.\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Contents"));
    assert!(output.contains("Prelude .... 1"));
    assert!(output.contains("1 Intro .... 1"));
    assert!(output.contains("Prelude"));
    assert!(output.contains("See 1."));

    let aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    assert_eq!(aux.toc.len(), 2);
    assert_eq!(aux.toc[0].title, "Prelude");
    assert_eq!(aux.toc[0].number, "");
    assert_eq!(aux.toc[1].title, "Intro");
    assert_eq!(aux.toc[1].number, "1");

    let stored_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read sources"),
    )
    .expect("parse sources");
    let executed_main = &stored_sources.executed_files[&Utf8PathBuf::from("main.tex")];
    assert!(!executed_main.contains("\\phantomsection"));
    assert!(!executed_main.contains("\\addcontentsline"));
}

#[tokio::test]
async fn internal_compiler_persists_semantic_index_artifact() {
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
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\input{sections/intro}\\cite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("sections/intro.tex"),
        "\\section{Intro}\\label{sec:intro}",
    )
    .expect("write intro");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/intro.tex"),
                Utf8PathBuf::from("refs.bbl"),
            ],
        })
        .await
        .expect("semantic aux build should succeed");

    let index = serde_json::from_slice::<SemanticAuxIndex>(
        &fs::read(build_root.join("rev-1/semantic-index.json")).expect("read semantic index"),
    )
    .expect("parse semantic index");
    assert!(index.has_table_of_contents);
    assert!(!index.has_bibliography_heading);
    assert_eq!(index.label_count, 1);
    assert_eq!(index.toc_count, 1);
    assert_eq!(index.citation_key_count, 1);
    assert_eq!(index.bibliography_entry_count, 1);
    let main = index
        .files
        .iter()
        .find(|file| file.path == Utf8PathBuf::from("main.tex"))
        .expect("main summary");
    assert_eq!(main.citation_keys, vec![String::from("alpha")]);
    let intro = index
        .files
        .iter()
        .find(|file| file.path == Utf8PathBuf::from("sections/intro.tex"))
        .expect("intro summary");
    assert_eq!(intro.label_keys, vec![String::from("sec:intro")]);
    assert_eq!(intro.toc.len(), 1);
    assert_eq!(intro.toc[0].number, "1");
    assert_eq!(intro.toc[0].title, "Intro");
    let bibliography = index
        .files
        .iter()
        .find(|file| file.path == Utf8PathBuf::from("refs.bbl"))
        .expect("bibliography summary");
    assert_eq!(bibliography.bibliography_keys, vec![String::from("alpha")]);
}

#[tokio::test]
async fn internal_compiler_persists_semantic_index_for_printbibheading_bibintoc() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\printbibheading[heading=bibintoc,title={References}]\\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("semantic aux build should succeed");

    let output = fs::read_to_string(build_root.join("rev-1/output.txt")).expect("read output");
    assert!(output.contains("Contents"));
    assert!(output.contains("References .... 1"));
    assert!(output.contains("References"));

    let index = serde_json::from_slice::<SemanticAuxIndex>(
        &fs::read(build_root.join("rev-1/semantic-index.json")).expect("read semantic index"),
    )
    .expect("parse semantic index");
    assert!(index.has_table_of_contents);
    assert!(index.has_bibliography_heading);
    assert_eq!(index.toc_count, 1);
}

#[tokio::test]
async fn internal_compiler_backdates_semantic_aux_payload_from_previous_revision() {
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
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\section{Intro}\\label{sec:intro}See \\ref{sec:intro} on page \\pageref{sec:intro}. Cite \\cite{alpha}.\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\\bibitem{alpha} Alpha entry.\\end{thebibliography}",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");

    let _first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("first semantic aux build should succeed");
    let first_bundle = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load first checkpoint bundle");

    let previous_aux = load_semantic_aux(&build_root.join("rev-1/aux.json")).expect("load aux");
    let previous_payload = serde_json::to_vec(&previous_aux).expect("serialize prior aux");
    fs::write(
        build_root.join("rev-1/aux.json").as_std_path(),
        &previous_payload,
    )
    .expect("rewrite aux payload");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\tableofcontents\\section{Intro}\\label{sec:intro}See \\ref{sec:intro} on page \\pageref{sec:intro}. Cite \\cite{alpha}.\\bibliography{refs}\\end{document}\n",
    )
    .expect("rewrite main");

    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("second semantic aux build should succeed");
    let replay_checkpoint = first_bundle
        .checkpoints
        .iter()
        .find(|checkpoint| {
            Some(&checkpoint.meta.checkpoint_id) == second.reused_checkpoint_id.as_ref()
        })
        .expect("reused rev-1 checkpoint");
    let tail = second.unchanged_tail.as_ref().expect("unchanged tail");
    assert_eq!(tail.previous_rev, 1);
    assert_eq!(tail.current_page_start, 0);
    assert_eq!(tail.page_count, second.page_metadata.len());
    assert!(second.page_patches.is_empty());
    assert_eq!(
        fs::read(build_root.join("rev-2/aux.json").as_std_path()).expect("read backdated aux"),
        previous_payload
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, vec![Utf8PathBuf::from("main.tex")]);
    assert_eq!(build_meta.start_checkpoint_id, second.reused_checkpoint_id);
    assert_eq!(
        build_meta.start_page_index,
        replay_checkpoint
            .meta
            .page_index_after
            .min(second.page_metadata.len())
    );
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(build_meta.rebuilt_page_count, 0);
    assert_eq!(build_meta.reused_page_count, second.page_metadata.len());
    assert_eq!(build_meta.semantic_pass_count, 1);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(build_meta.semantic_aux_backdated);
}

#[tokio::test]
async fn internal_compiler_replays_from_bibliography_input_checkpoint_after_semantic_fixpoint() {
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
    let filler = "bibliography replay filler text ".repeat(220);
    fs::write(
        root.join("main.tex"),
        format!(
            "\\documentclass{{article}}\\begin{{document}}Cite \\cite{{alpha}}.\\section{{Intro}} {filler}\\bibliography{{refs}}\\end{{document}}"
        ),
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem{alpha} Alpha entry.\n\\end{thebibliography}\n",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("first semantic aux build should succeed");
    assert!(
        first.page_metadata.len() >= 2,
        "fixture should push bibliography onto a later page"
    );
    let first_bundle =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let expected_checkpoint = first_bundle
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("refs.bbl"))
        })
        .expect("refs.bbl input boundary");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem{alpha} Alpha   entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite bbl");

    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("second semantic aux build should succeed");

    assert_eq!(
        second.reused_checkpoint_id,
        Some(expected_checkpoint.meta.checkpoint_id.clone())
    );
    assert!(second.page_patches.is_empty());
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, vec![Utf8PathBuf::from("refs.bbl")]);
    assert_eq!(
        build_meta.start_checkpoint_id,
        Some(expected_checkpoint.meta.checkpoint_id.clone())
    );
    assert_eq!(
        build_meta.start_page_index,
        expected_checkpoint.meta.page_index_after
    );
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(build_meta.rebuilt_page_count, 0);
    assert_eq!(build_meta.reused_page_count, second.page_metadata.len());
    assert_eq!(build_meta.semantic_pass_count, 1);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(build_meta.semantic_aux_backdated);
}

#[tokio::test]
async fn internal_compiler_falls_back_to_cp0_when_bibliography_edit_changes_earlier_citation_output()
 {
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
    let filler = "bibliography replay filler text ".repeat(220);
    fs::write(
        root.join("main.tex"),
        format!(
            "\\documentclass{{article}}\\begin{{document}}\\section{{Intro}} {filler} Cite \\cite{{alpha}}.\\bibliography{{refs}}\\end{{document}}"
        ),
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem{alpha} Alpha entry.\n\\end{thebibliography}\n",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let _first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("first semantic aux build should succeed");
    let first_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-1/sources.json")).expect("read first sources"),
    )
    .expect("parse first sources");
    assert!(
        first_sources.executed_files[&Utf8PathBuf::from("main.tex")].contains("Cite [1]."),
        "first executed main.tex should materialize citation before the filler block"
    );
    let first_bundle =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let bibliography_checkpoint = first_bundle
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("refs.bbl"))
        })
        .cloned()
        .expect("refs.bbl input boundary");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\n\\bibitem{beta} Beta entry.\n\\bibitem{alpha} Alpha entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite bbl");

    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("second semantic aux build should succeed");
    let second_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-2/sources.json")).expect("read second sources"),
    )
    .expect("parse second sources");
    assert!(
        second_sources.files[&Utf8PathBuf::from("main.tex")].contains("\\cite{alpha}"),
        "raw main.tex should remain unchanged"
    );
    assert!(
        second_sources.executed_files[&Utf8PathBuf::from("main.tex")].contains("Cite [2]."),
        "second executed main.tex should reflect earlier citation renumbering"
    );
    let earliest_changed_rewrite_output_start = first_sources.rewrite_spans
        [&Utf8PathBuf::from("main.tex")]
        .iter()
        .zip(&second_sources.rewrite_spans[&Utf8PathBuf::from("main.tex")])
        .find_map(|(previous, current)| {
            (previous.start_utf8 == current.start_utf8
                && previous.end_utf8 == current.end_utf8
                && previous.rendered != current.rendered)
                .then_some(previous.output_start_utf8)
        })
        .expect("changed cite rewrite span");
    assert_eq!(
        second.reused_checkpoint_id, None,
        "semantic-changing bibliography edits should rebuild from the base snapshot"
    );
    assert_eq!(
        first_bundle.checkpoints[0].meta.output_start_utf8, 0,
        "rev-1 cp0 should still mark the earliest output boundary"
    );
    assert!(
        first_bundle.checkpoints[0].meta.output_start_utf8 <= earliest_changed_rewrite_output_start,
        "base-snapshot rebuild should restart before the earliest changed citation output"
    );
    assert!(
        first_bundle.checkpoints[0].meta.output_start_utf8
            < bibliography_checkpoint.meta.output_start_utf8,
        "base-snapshot rebuild should restart before the refs.bbl input boundary"
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.start_checkpoint_id, None);
    assert_eq!(build_meta.start_page_index, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert_eq!(build_meta.semantic_pass_count, 2);
}

#[tokio::test]
async fn internal_compiler_uses_semantic_rewrite_span_for_late_bibliography_only_change() {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let filler = "late bibliography replay filler text ".repeat(220);
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
    fs::write(
        root.join("main.tex"),
        format!(
            "\\documentclass{{article}}\\begin{{document}}Early cite \\cite{{alpha}}. {filler} Late year \\citeyear{{beta}}.\\bibliography{{refs}}\\end{{document}}"
        ),
    )
    .expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\n\\bibitem[Alpha 2024]{alpha} Alpha entry.\n\\bibitem[Beta 2024]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("write bbl");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("first semantic aux build should succeed");
    assert!(
        first.page_metadata.len() >= 2,
        "fixture should push late cite onto a later page"
    );
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{2}\n\\bibitem[Alpha 2024]{alpha} Alpha entry.\n\\bibitem[Beta 2025]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite bbl");

    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("second semantic aux build should succeed");

    let second_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-2/sources.json")).expect("read second sources"),
    )
    .expect("parse second sources");
    assert!(
        second_sources.executed_files[&Utf8PathBuf::from("main.tex")].contains("Late year 2025."),
        "executed main.tex should reflect only the late citeyear change"
    );
    assert_eq!(
        second.reused_checkpoint_id, None,
        "semantic-changing bibliography edits should rebuild from the base snapshot"
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, vec![Utf8PathBuf::from("refs.bbl")]);
    assert_eq!(build_meta.start_page_index, 0);
    assert_eq!(build_meta.start_checkpoint_id, None);
    assert!(build_meta.semantic_fixpoint_reached);
}

#[tokio::test]
async fn internal_compiler_avoids_shipout_replay_corruption_for_semantically_equal_multi_bibliography_edit()
 {
    let fixture_root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/arxiv-smoke/optioned-bibliography-order-stack");
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 tempdir");
    let mut copy_dirs = vec![(fixture_root.clone(), root.clone())];
    while let Some((source_dir, target_dir)) = copy_dirs.pop() {
        fs::create_dir_all(target_dir.as_std_path()).expect("create target dir");
        for entry in fs::read_dir(source_dir.as_std_path())
            .expect("read source dir")
            .filter_map(|entry| entry.ok())
        {
            let source_path = Utf8PathBuf::from_path_buf(entry.path()).expect("utf8 source path");
            let target_path = target_dir.join(entry.file_name().to_string_lossy().as_ref());
            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                copy_dirs.push((source_path, target_path));
                continue;
            }
            fs::copy(source_path.as_std_path(), target_path.as_std_path())
                .expect("copy fixture file");
        }
    }

    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let mut third_preamble_checkpoint = None;
    let mut third = None;
    let mut fourth = None;
    for rev in 1..=4u64 {
        let mut changed_files = Vec::new();
        if rev > 1 {
            let overlay_root = fixture_root.join(format!("rev{rev}"));
            if overlay_root.exists() {
                let mut overlay_dirs = vec![overlay_root.clone()];
                while let Some(source_dir) = overlay_dirs.pop() {
                    for entry in fs::read_dir(source_dir.as_std_path())
                        .expect("read overlay dir")
                        .filter_map(|entry| entry.ok())
                    {
                        let source_path =
                            Utf8PathBuf::from_path_buf(entry.path()).expect("utf8 overlay path");
                        if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                            overlay_dirs.push(source_path);
                            continue;
                        }
                        let relative_path = source_path
                            .strip_prefix(&overlay_root)
                            .expect("overlay path should be relative to overlay root");
                        let target_path = root.join(relative_path);
                        if let Some(parent) = target_path.parent() {
                            fs::create_dir_all(parent.as_std_path()).expect("create parent dir");
                        }
                        fs::copy(source_path.as_std_path(), target_path.as_std_path())
                            .expect("copy overlay file");
                        changed_files.push(relative_path.to_owned());
                    }
                }
            }
        }
        let world = ProjectWorld::load(root.clone()).expect("world");
        let outcome = driver
            .compile(CompileRequest {
                root: root.clone(),
                manifest: world.manifest.clone(),
                toplevel: Utf8PathBuf::from("main.tex"),
                rev,
                build_root: build_root.clone(),
                changed_files,
            })
            .await
            .expect("build should succeed");
        if rev == 3 {
            let third_bundle = load_checkpoint_bundle(&build_root.join("rev-3/checkpoints.json"))
                .expect("load third bundle");
            third_preamble_checkpoint = Some(
                third_bundle
                    .checkpoints
                    .first()
                    .expect("third preamble checkpoint")
                    .meta
                    .checkpoint_id
                    .clone(),
            );
            third = Some(outcome);
        } else if rev == 4 {
            fourth = Some(outcome);
        }
    }

    let third = third.expect("third outcome");
    let third_output =
        fs::read_to_string(build_root.join("rev-3/output.txt")).expect("read third output");
    assert!(third_output.contains("Order check. [2] and [1]"));
    assert!(third_output.contains("[1] Beta entry."));
    assert!(third_output.contains("[2] Alpha entry."));
    let third_build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-3/build-meta.json")).expect("read third build meta"),
    )
    .expect("parse third build meta");
    assert!(third_build_meta.aux_sensitive);
    assert_eq!(
        third_build_meta.dirty_files,
        vec![Utf8PathBuf::from("main.tex")]
    );
    assert_eq!(third_build_meta.start_page_index, 0);
    assert_eq!(third_build_meta.page_count, third.page_metadata.len());
    assert_eq!(third_build_meta.semantic_pass_count, 2);
    assert_eq!(third_build_meta.semantic_rerun_count, 1);
    assert!(third_build_meta.semantic_fixpoint_reached);
    assert!(!third_build_meta.semantic_aux_backdated);

    let fourth = fourth.expect("fourth outcome");
    let fourth_output =
        fs::read_to_string(build_root.join("rev-4/output.txt")).expect("read fourth output");
    assert_eq!(
        fourth_output
            .matches("wrapperarticletwocolumnunicode")
            .count(),
        1
    );
    assert!(!fourth_output.contains("column,unicode]wrapper"));
    assert!(!fourth_output.contains("icleOrder check"));
    assert!(fourth_output.contains("Order check. [2] and [1]"));
    assert!(fourth_output.contains("[1] Beta entry."));
    assert!(fourth_output.contains("[2] Alpha entry."));
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-4/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, vec![Utf8PathBuf::from("refsa.bbl")]);
    assert_eq!(
        build_meta.start_checkpoint_id,
        third_preamble_checkpoint.clone()
    );
    assert_eq!(build_meta.start_page_index, 0);
    assert_eq!(build_meta.page_count, fourth.page_metadata.len());
    assert_eq!(build_meta.rebuilt_page_count, 0);
    assert_eq!(build_meta.reused_page_count, 1);
    assert_eq!(build_meta.semantic_pass_count, 1);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(build_meta.semantic_aux_backdated);
    assert_eq!(fourth.reused_checkpoint_id, third_preamble_checkpoint);
}

#[tokio::test]
async fn internal_compiler_splits_nested_input_source_spans_in_output_order() {
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
    fs::write(root.join("main.tex"), "A\\input{parent}Z").expect("write main");
    fs::write(root.join("parent.tex"), "B\\input{child}C").expect("write parent");
    fs::write(root.join("child.tex"), "D").expect("write child");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let outcome = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: root.join(".latexd/build"),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
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

#[tokio::test]
async fn internal_compiler_turns_vm_diagnostics_into_failure() {
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
    fs::write(root.join("main.tex"), "\\UnknownCommand").expect("write main tex");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let failure = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: root.join(".latexd/build"),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect_err("internal build should fail on VM diagnostics");

    assert_eq!(failure.diagnostics.len(), 1);
    assert!(failure.diagnostics[0].message.contains("UnknownCommand"));
}

#[tokio::test]
async fn internal_compiler_emits_multipage_metadata_and_checkpoints() {
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
    let source = (0..1200)
        .map(|index| format!("line{index}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(root.join("main.tex"), source).expect("write main tex");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let outcome = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 3,
            build_root: root.join(".latexd/build"),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("internal build should succeed");

    assert!(outcome.page_metadata.len() > 1);
    for window in outcome.page_metadata.windows(2) {
        assert!(window[0].index < window[1].index);
        assert!(window[0].text_end_utf8 < window[1].text_start_utf8);
        assert_ne!(window[0].page_id, window[1].page_id);
    }

    let checkpoints = load_checkpoint_bundle(&root.join(".latexd/build/rev-3/checkpoints.json"))
        .expect("load checkpoints");
    assert_eq!(
        checkpoints.checkpoints.len(),
        outcome.page_metadata.len() + 1
    );
}

#[tokio::test]
async fn internal_compiler_reuses_only_matching_preamble_checkpoints() {
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
    fs::write(root.join("article.cls"), "\\def\\classmark{class}").expect("write class");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\title{A}\\begin{document}\\classmark first body\\end{document}",
    )
    .expect("write main tex");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("first internal build should succeed");
    assert!(first.reused_checkpoint_id.is_none());
    let first_checkpoints = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load rev1 checkpoints");
    let first_preamble_id = first_checkpoints.checkpoints[0].meta.checkpoint_id.clone();

    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\title{A}\\begin{document}\\classmark second body\\end{document}",
    )
    .expect("rewrite main tex body");
    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("body-only rebuild should succeed");
    assert_eq!(second.reused_checkpoint_id, Some(first_preamble_id));

    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\title{B}\\begin{document}\\classmark third body\\end{document}",
    )
    .expect("rewrite main tex preamble");
    let third = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 3,
            build_root,
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("preamble rebuild should succeed");
    assert!(third.reused_checkpoint_id.is_none());
}

#[tokio::test]
async fn internal_compiler_reports_shifted_unchanged_tail_from_prior_shipout() {
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
    let original_body = (0..1536)
        .map(|index| format!("w{index:07}"))
        .collect::<Vec<_>>()
        .join(" ");
    let original_source = format!(
        "\\documentclass{{article}}\\begin{{document}}\n{}\n\\end{{document}}",
        original_body
    );
    fs::write(root.join("main.tex"), &original_source).expect("write main tex");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("first build should succeed");
    assert_eq!(first.page_metadata.len(), 4);

    let inserted_page = (0..384)
        .map(|index| format!("x{index:07}"))
        .collect::<Vec<_>>()
        .join(" ");
    let shifted_body = format!(
        "{} {} {}",
        (0..384)
            .map(|index| format!("w{index:07}"))
            .collect::<Vec<_>>()
            .join(" "),
        inserted_page,
        (384..1536)
            .map(|index| format!("w{index:07}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    fs::write(
        root.join("main.tex"),
        format!(
            "\\documentclass{{article}}\\begin{{document}}\n{}\n\\end{{document}}",
            shifted_body
        ),
    )
    .expect("rewrite main tex");

    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root,
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("second build should succeed");

    let tail = second.unchanged_tail.expect("unchanged tail");
    assert_eq!(tail.previous_rev, 1);
    assert_eq!(tail.previous_page_start, 1);
    assert_eq!(tail.current_page_start, 2);
    assert_eq!(tail.page_count, 3);
    assert_eq!(
        second.page_patches,
        vec![PagePatchOp::InsertPage {
            index: 1,
            page_id: second.page_metadata[1].page_id.clone(),
            pdf_url: format!(
                "/artifacts/rev/2/pages/{}.pdf",
                second.page_metadata[1].page_id
            ),
            svg_url: Some(format!(
                "/artifacts/rev/2/pages/{}.svg",
                second.page_metadata[1].page_id
            )),
        }]
    );
    assert_eq!(
        second.page_artifacts[0].pdf_url,
        format!(
            "/artifacts/rev/1/pages/{}.pdf",
            second.page_metadata[0].page_id
        )
    );
    assert_eq!(
        second.page_artifacts[1].pdf_url,
        format!(
            "/artifacts/rev/2/pages/{}.pdf",
            second.page_metadata[1].page_id
        )
    );
    assert!(
        second.page_artifacts[2..]
            .iter()
            .all(|page| page.pdf_url.starts_with("/artifacts/rev/1/pages/"))
    );
    let first_checkpoints =
        load_checkpoint_bundle(&root.join(".latexd/build/rev-1/checkpoints.json"))
            .expect("load rev1 checkpoints");
    let second_checkpoints =
        load_checkpoint_bundle(&root.join(".latexd/build/rev-2/checkpoints.json"))
            .expect("load rev2 checkpoints");
    let current_source = fs::read_to_string(root.join("main.tex")).expect("read current main");
    let source_delta = current_source.len() as i64 - original_source.len() as i64;
    let shared_prefix = original_source
        .bytes()
        .zip(current_source.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    for offset in 0..tail.page_count {
        let previous_page_index = tail.previous_page_start + offset;
        let current_page_index = tail.current_page_start + offset;
        let previous_checkpoint = first_checkpoints
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == tex_checkpoint::CheckpointKind::Shipout
                    && checkpoint.meta.page_index_after == previous_page_index + 1
            })
            .expect("previous shipout checkpoint");
        let current_checkpoint = second_checkpoints
            .checkpoints
            .iter()
            .find(|checkpoint| {
                checkpoint.meta.kind == tex_checkpoint::CheckpointKind::Shipout
                    && checkpoint.meta.page_index_after == current_page_index + 1
            })
            .expect("current shipout checkpoint");
        let mut rebased_offset = previous_checkpoint.meta.source_offset_utf8 as usize;
        if rebased_offset > shared_prefix {
            rebased_offset = (rebased_offset as i64 + source_delta)
                .clamp(0, current_source.len() as i64) as usize;
        } else {
            rebased_offset = rebased_offset.min(current_source.len());
        }
        let page_floor = second.page_metadata[..=current_page_index]
            .iter()
            .flat_map(|page| page.source_spans.iter())
            .filter(|span| span.file == Utf8PathBuf::from("main.tex"))
            .map(|span| span.end_utf8 as usize)
            .max()
            .unwrap_or_default();
        assert_eq!(
            current_checkpoint.meta.source_offset_utf8 as usize,
            rebased_offset.max(page_floor)
        );
    }
}

#[tokio::test]
async fn internal_compiler_replays_from_nearest_shipout_checkpoint_for_late_edit() {
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
    let original_body = (0..1536)
        .map(|index| format!("w{index:07}"))
        .collect::<Vec<_>>()
        .join(" ");
    let original_source = format!(
        "\\documentclass{{article}}\\begin{{document}}\n{}\n\\end{{document}}",
        original_body
    );
    fs::write(root.join("main.tex"), &original_source).expect("write main tex");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("first build should succeed");
    assert_eq!(first.page_metadata.len(), 4);
    let first_checkpoints = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load rev1 checkpoints");
    assert!(
        first_checkpoints.checkpoints[1..]
            .iter()
            .all(|checkpoint| checkpoint.snapshot.is_some())
    );
    assert!(
        first_checkpoints.checkpoints[1..]
            .windows(2)
            .all(|window| window[0].meta.source_offset_utf8 <= window[1].meta.source_offset_utf8)
    );

    let late_edit_body = format!(
        "{} {}",
        (0..1152)
            .map(|index| format!("w{index:07}"))
            .collect::<Vec<_>>()
            .join(" "),
        (1152..1536)
            .map(|index| format!("z{index:07}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let late_edit_source = format!(
        "\\documentclass{{article}}\\begin{{document}}\n{}\n\\end{{document}}",
        late_edit_body
    );
    fs::write(root.join("main.tex"), &late_edit_source).expect("rewrite main tex");
    let diff_offset = original_source
        .bytes()
        .zip(late_edit_source.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    let expected_checkpoint_id = first_checkpoints
        .checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.meta.page_index_after > 0)
        .take_while(|checkpoint| checkpoint.meta.source_offset_utf8 <= diff_offset as u32)
        .last()
        .and_then(|offset_checkpoint| {
            let span_start_page = first.page_metadata.iter().find_map(|page| {
                page.source_spans
                    .iter()
                    .find(|span| span.file == Utf8PathBuf::from("main.tex"))
                    .and_then(|span| {
                        if (diff_offset as u32) < span.end_utf8 {
                            Some(page.index)
                        } else {
                            None
                        }
                    })
            })?;
            let expected_page_index_after =
                offset_checkpoint.meta.page_index_after.min(span_start_page);
            first_checkpoints
                .checkpoints
                .iter()
                .find(|checkpoint| checkpoint.meta.page_index_after == expected_page_index_after)
                .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
        })
        .expect("expected shipout checkpoint");

    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("second build should succeed");

    assert_eq!(
        second.reused_checkpoint_id,
        Some(expected_checkpoint_id.clone())
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert_eq!(build_meta.start_checkpoint_id, Some(expected_checkpoint_id));
    assert!(build_meta.start_page_index >= 1);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert!(build_meta.reused_page_count >= 1);
    assert!(build_meta.rebuilt_page_count >= 1);
    assert!(
        second.page_artifacts[0]
            .pdf_url
            .starts_with("/artifacts/rev/1/pages/")
    );
    assert!(
        second
            .page_artifacts
            .last()
            .expect("last page artifact")
            .pdf_url
            .starts_with("/artifacts/rev/2/pages/")
    );
    assert!(second.page_patches.iter().any(|patch| matches!(
        patch,
        PagePatchOp::ReplacePage { index, .. } if *index >= 2
    )));
    assert!(build_root.join("rev-2/output.txt").exists());
    assert!(build_root.join("rev-2/sources.json").exists());
}

#[tokio::test]
async fn internal_compiler_replays_from_late_input_file_page() {
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
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    let mut words = (0..1800)
        .map(|index| format!("word{index:04}"))
        .collect::<Vec<_>>();
    words.insert(1500, "\\input{sections/tail}".to_string());
    fs::write(root.join("sections/tail.tex"), "tail-A").expect("write tail input");
    fs::write(root.join("main.tex"), words.join(" ")).expect("write main tex");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("first build should succeed");

    let input_page = first
        .page_metadata
        .iter()
        .find(|page| {
            page.source_spans
                .iter()
                .any(|span| span.file == Utf8PathBuf::from("sections/tail.tex"))
        })
        .expect("input page");
    assert!(input_page.index > 0);
    let first_checkpoints = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load rev1 checkpoints");
    assert!(first_checkpoints.checkpoints.iter().any(|checkpoint| {
        checkpoint.meta.kind == tex_checkpoint::CheckpointKind::InputBoundary
            && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("sections/tail.tex"))
    }));

    fs::write(root.join("sections/tail.tex"), "tail-B").expect("rewrite tail input");
    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root,
            changed_files: vec![Utf8PathBuf::from("sections/tail.tex")],
        })
        .await
        .expect("second build should succeed");

    let expected_checkpoint_id = first_checkpoints
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.meta.kind == tex_checkpoint::CheckpointKind::InputBoundary
                && checkpoint.meta.module_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/tail.tex"))
        })
        .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
        .expect("input boundary checkpoint");
    assert_eq!(second.reused_checkpoint_id, Some(expected_checkpoint_id));
}

#[tokio::test]
async fn internal_compiler_replays_from_toplevel_input_exit_boundary() {
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
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    fs::write(root.join("sections/tail.tex"), "tail-body").expect("write tail");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document} intro \\input{sections/tail} after-old \\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("first build should succeed");

    let first_checkpoints = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load rev1 checkpoints");
    let expected_checkpoint_id = first_checkpoints
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.meta.kind == tex_checkpoint::CheckpointKind::InputBoundary
                && checkpoint.meta.input_boundary_kind == Some(tex_vm::VmModuleCheckpointKind::Exit)
                && checkpoint.meta.resume_path.as_ref() == Some(&Utf8PathBuf::from("main.tex"))
                && checkpoint.meta.module_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/tail.tex"))
        })
        .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
        .expect("toplevel input exit checkpoint");

    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document} intro \\input{sections/tail} after-new \\end{document}",
    )
    .expect("rewrite main");
    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root,
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("second build should succeed");

    assert_eq!(second.reused_checkpoint_id, Some(expected_checkpoint_id));
}

#[tokio::test]
async fn internal_compiler_prefers_earliest_repeated_include_occurrence_for_same_file_edit() {
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
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    fs::write(root.join("sections/child.tex"), "nested").expect("write child");
    fs::write(
        root.join("sections/tail.tex"),
        "before \\input{sections/child} after-old",
    )
    .expect("write tail");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document} A \\input{sections/tail} B \\input{sections/tail} C \\end{document}",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("first build should succeed");

    let first_checkpoints = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load rev1 checkpoints");
    let expected_checkpoint_id = first_checkpoints
        .checkpoints
        .iter()
        .filter(|checkpoint| {
            checkpoint.meta.kind == tex_checkpoint::CheckpointKind::InputBoundary
                && checkpoint.meta.input_boundary_kind == Some(tex_vm::VmModuleCheckpointKind::Exit)
                && checkpoint.meta.resume_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/tail.tex"))
                && checkpoint.meta.module_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/child.tex"))
        })
        .min_by_key(|checkpoint| checkpoint.meta.output_start_utf8)
        .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
        .expect("first repeated occurrence checkpoint");

    fs::write(
        root.join("sections/tail.tex"),
        "before \\input{sections/child} after-new",
    )
    .expect("rewrite tail");
    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root,
            changed_files: vec![Utf8PathBuf::from("sections/tail.tex")],
        })
        .await
        .expect("second build should succeed");

    assert_eq!(second.reused_checkpoint_id, Some(expected_checkpoint_id));
}

#[tokio::test]
async fn internal_compiler_replays_from_nested_input_exit_boundary() {
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
    fs::create_dir_all(root.join("sections")).expect("sections dir");
    let mut words = (0..1600)
        .map(|index| format!("word{index:04}"))
        .collect::<Vec<_>>();
    words.insert(900, "\\input{sections/tail}".to_string());
    words.push("after-old".to_string());
    fs::write(root.join("sections/tail.tex"), "tail-body").expect("write tail");
    fs::write(root.join("sections/parent.tex"), words.join(" ")).expect("write parent");
    fs::write(
        root.join("main.tex"),
        "alpha \\input{sections/parent} omega",
    )
    .expect("write main");

    let world = ProjectWorld::load(root.clone()).expect("world");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("first build should succeed");

    let first_checkpoints = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load rev1 checkpoints");
    let expected_checkpoint_id = first_checkpoints
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.meta.kind == tex_checkpoint::CheckpointKind::InputBoundary
                && checkpoint.meta.input_boundary_kind == Some(tex_vm::VmModuleCheckpointKind::Exit)
                && checkpoint.meta.resume_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/parent.tex"))
                && checkpoint.meta.module_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/tail.tex"))
        })
        .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
        .expect("nested exit checkpoint");
    assert!(first_checkpoints.checkpoints.iter().any(|checkpoint| {
        checkpoint.meta.kind == tex_checkpoint::CheckpointKind::Shipout
            && checkpoint.meta.resume_path.is_some()
    }));

    let mut edited_words = (0..1600)
        .map(|index| format!("word{index:04}"))
        .collect::<Vec<_>>();
    edited_words.insert(900, "\\input{sections/tail}".to_string());
    edited_words.push("after-new".to_string());
    fs::write(root.join("sections/parent.tex"), edited_words.join(" ")).expect("rewrite parent");
    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root,
            changed_files: vec![Utf8PathBuf::from("sections/parent.tex")],
        })
        .await
        .expect("second build should succeed");

    assert_eq!(second.reused_checkpoint_id, Some(expected_checkpoint_id));
}
