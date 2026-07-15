async fn prepare_late_multi_bibliography_semantic_change_workspace(
    root: &Utf8Path,
    driver: &CompilerDriver,
    build_root: &Utf8Path,
) -> ProjectWorld {
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
        "\\documentclass{article}\\begin{document}Order check. \\cite{beta} and \\citeyear{alpha}.\\bibliography{refsb,refsa}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("write first bibliography");
    fs::write(
        root.join("refsa.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[A 2024]{alpha} Alpha entry.\n\\end{thebibliography}\n",
    )
    .expect("write second bibliography");

    let world = ProjectWorld::load(root.to_owned()).expect("world");
    let first = driver
        .compile(CompileRequest {
            root: root.to_owned(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.to_owned(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("refsa.bbl"),
            ],
        })
        .await
        .expect("first semantic aux build should succeed");
    assert_eq!(first.page_metadata.len(), 1);

    fs::write(
        root.join("refsa.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[A 2026]{alpha} Alpha revised entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite later bibliography");
    world
}

fn assert_late_multi_bibliography_semantic_change_rebuild(
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: Vec<Utf8PathBuf>,
) {
    let second_output =
        fs::read_to_string(build_root.join("rev-2/output.txt")).expect("read second output");
    assert!(second_output.contains("Alpha revised entry."));
    assert!(
        second_output.contains("2026"),
        "executed citation output should reflect the semantic year change"
    );
    assert_eq!(
        second.reused_checkpoint_id, None,
        "semantic-changing later bibliography edits should rebuild from the base snapshot even when skip-shipout conditions also hold"
    );

    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, None);
    assert_eq!(build_meta.start_page_index, 0);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(build_meta.rebuilt_page_count, second.page_metadata.len());
    assert_eq!(build_meta.reused_page_count, 0);
    assert_eq!(build_meta.semantic_pass_count, 2);
    assert_eq!(build_meta.semantic_rerun_count, 1);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}

#[derive(Clone, Copy)]
enum LateMultiNoise {
    NoExtraDirty,
    Untracked,
    Unreadable,
}

enum LateMultiRootCase {
    Baseline,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

enum LateMultiIncludedBodyCase {
    Baseline,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

fn apply_late_multi_bibliography_noise(root: &Utf8Path, noise: LateMultiNoise) {
    match noise {
        LateMultiNoise::NoExtraDirty => {}
        LateMultiNoise::Untracked => {
            fs::write(root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        LateMultiNoise::Unreadable => {
            fs::create_dir_all(root.join("notes.txt")).expect("create unreadable dirty dir");
        }
    }
}

struct LateMultiBibliographySemanticChangeRun {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    second: CompileOutcome,
}

async fn compile_late_multi_bibliography_semantic_change_with_root_noise(
    dirty_files: Vec<Utf8PathBuf>,
    noise: LateMultiNoise,
) -> LateMultiBibliographySemanticChangeRun {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let world =
        prepare_late_multi_bibliography_semantic_change_workspace(&root, &driver, &build_root)
            .await;

    apply_late_multi_bibliography_noise(&root, noise);

    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: dirty_files,
        })
        .await
        .expect("second semantic aux build should succeed");

    LateMultiBibliographySemanticChangeRun {
        _tempdir: tempdir,
        root,
        build_root,
        second,
    }
}

async fn run_late_multi_root_rebuild(dirty_files: Vec<Utf8PathBuf>, noise: LateMultiNoise) {
    let run =
        compile_late_multi_bibliography_semantic_change_with_root_noise(dirty_files.clone(), noise)
            .await;
    assert_late_multi_bibliography_semantic_change_rebuild(
        &run.build_root,
        &run.second,
        dirty_files,
    );
}

async fn run_late_multi_root_case(case: LateMultiRootCase) {
    let (dirty_files, noise) = match case {
        LateMultiRootCase::Baseline => (
            vec![Utf8PathBuf::from("refsa.bbl")],
            LateMultiNoise::NoExtraDirty,
        ),
        LateMultiRootCase::UntrackedFollows => (
            vec![
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
            LateMultiNoise::Untracked,
        ),
        LateMultiRootCase::UntrackedPrecedes => (
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("refsa.bbl"),
            ],
            LateMultiNoise::Untracked,
        ),
        LateMultiRootCase::UnreadableFollows => (
            vec![
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
            LateMultiNoise::Unreadable,
        ),
        LateMultiRootCase::UnreadablePrecedes => (
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("refsa.bbl"),
            ],
            LateMultiNoise::Unreadable,
        ),
    };

    run_late_multi_root_rebuild(dirty_files, noise).await;
}

async fn prepare_late_multi_bibliography_semantic_change_with_included_body_workspace(
    root: &Utf8Path,
    driver: &CompilerDriver,
    build_root: &Utf8Path,
) -> ProjectWorld {
    let filler = "late multi bibliography replay filler text ".repeat(220);
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
        "\\documentclass{article}\\begin{document}\\input{sections/body}\\bibliography{refsa,refsb}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("sections/body.tex"),
        format!("Early cite \\cite{{alpha}}. {filler} Late year \\citeyear{{beta}}."),
    )
    .expect("write body");
    fs::write(
        root.join("refsa.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[A 2024]{alpha} Alpha entry.\n\\end{thebibliography}\n",
    )
    .expect("write first bibliography");
    fs::write(
        root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2024]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("write second bibliography");

    let world = ProjectWorld::load(root.to_owned()).expect("world");
    let _first = driver
        .compile(CompileRequest {
            root: root.to_owned(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.to_owned(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/body.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("refsb.bbl"),
            ],
        })
        .await
        .expect("first semantic aux build should succeed");
    fs::write(
        root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite second bibliography");
    world
}

async fn compile_late_multi_bibliography_semantic_change_with_included_body_noise(
    dirty_files: Vec<Utf8PathBuf>,
    noise: LateMultiNoise,
) -> LateMultiBibliographySemanticChangeRun {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let world = prepare_late_multi_bibliography_semantic_change_with_included_body_workspace(
        &root,
        &driver,
        &build_root,
    )
    .await;

    apply_late_multi_bibliography_noise(&root, noise);

    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: dirty_files,
        })
        .await
        .expect("second semantic aux build should succeed");

    LateMultiBibliographySemanticChangeRun {
        _tempdir: tempdir,
        root,
        build_root,
        second,
    }
}

fn assert_late_multi_bibliography_semantic_change_with_included_body_rebuild(
    root: &Utf8Path,
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: Vec<Utf8PathBuf>,
) {
    let second_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-2/sources.json")).expect("read second sources"),
    )
    .expect("parse second sources");
    assert!(
        second_sources.executed_files[&Utf8PathBuf::from("sections/body.tex")]
            .contains("Late year 2025."),
        "executed body.tex should reflect the late bibliography change"
    );
    assert_eq!(second.reused_checkpoint_id, None);
    let replace_indexes = second
        .page_patches
        .iter()
        .filter_map(|patch| match patch {
            PagePatchOp::ReplacePage { index, .. } => Some(*index),
            _ => None,
        })
        .collect::<Vec<_>>();
    let body_source =
        fs::read_to_string(root.join("sections/body.tex")).expect("read included body source");
    let late_citation_offset = body_source
        .find(r"\citeyear{beta}")
        .expect("late citation source offset");
    let changed_page_index = renderer_page_index_covering_source_offset(
        second,
        Utf8Path::new("sections/body.tex"),
        late_citation_offset,
    );
    assert_eq!(replace_indexes, vec![changed_page_index]);
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, None);
    assert_eq!(build_meta.start_page_index, 0);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert!(build_meta.rebuilt_page_count >= 1);
    assert_eq!(
        build_meta.rebuilt_page_count + build_meta.reused_page_count,
        build_meta.page_count
    );
    assert_eq!(build_meta.semantic_pass_count, 2);
    assert_eq!(build_meta.semantic_rerun_count, 1);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}

async fn run_late_multi_included_body_rebuild(
    dirty_files: Vec<Utf8PathBuf>,
    noise: LateMultiNoise,
) {
    let run = compile_late_multi_bibliography_semantic_change_with_included_body_noise(
        dirty_files.clone(),
        noise,
    )
    .await;
    assert_late_multi_bibliography_semantic_change_with_included_body_rebuild(
        &run.root,
        &run.build_root,
        &run.second,
        dirty_files,
    );
}

async fn run_late_multi_included_body_case(case: LateMultiIncludedBodyCase) {
    let (dirty_files, noise) = match case {
        LateMultiIncludedBodyCase::Baseline => (
            vec![Utf8PathBuf::from("refsb.bbl")],
            LateMultiNoise::NoExtraDirty,
        ),
        LateMultiIncludedBodyCase::UntrackedFollows => (
            vec![
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
            LateMultiNoise::Untracked,
        ),
        LateMultiIncludedBodyCase::UntrackedPrecedes => (
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("refsb.bbl"),
            ],
            LateMultiNoise::Untracked,
        ),
        LateMultiIncludedBodyCase::UnreadableFollows => (
            vec![
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ],
            LateMultiNoise::Unreadable,
        ),
        LateMultiIncludedBodyCase::UnreadablePrecedes => (
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("refsb.bbl"),
            ],
            LateMultiNoise::Unreadable,
        ),
    };

    run_late_multi_included_body_rebuild(dirty_files, noise).await;
}
