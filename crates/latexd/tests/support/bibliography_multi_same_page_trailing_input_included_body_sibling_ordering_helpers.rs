struct SamePageIncludedBodyTrailingInputSiblingOrderingFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    bibliography_checkpoint_id: String,
    bibliography_page_index_after: usize,
}

#[derive(Clone, Copy)]
enum SamePageIncludedBodyTrailingInputSiblingOrderingCase {
    Baseline,
    Reversed,
    Interleaved,
    OtherInterleaved,
}

type BodySiblingOrderCase = SamePageIncludedBodyTrailingInputSiblingOrderingCase;

async fn run_body_sibling_order_case(case: BodySiblingOrderCase) {
    run_same_page_included_body_trailing_input_sibling_ordering_replay(case).await;
}

async fn prepare_same_page_included_body_trailing_input_sibling_ordering_fixture()
-> SamePageIncludedBodyTrailingInputSiblingOrderingFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let body_filler = (0..80)
        .map(|index| format!("body{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
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
        "\\documentclass{article}\\begin{document}\\input{sections/body}\\bibliography{refsa,refsb}\\input{sections/tail}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("sections/body.tex"),
        format!("Cite \\cite{{alpha}} and \\cite{{beta}}. {body_filler}"),
    )
    .expect("write body");
    fs::write(root.join("sections/tail.tex"), "Tail A.").expect("write tail");
    fs::write(
        root.join("refsa.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[A 2024]{alpha} Alpha entry.\n\\end{thebibliography}\n",
    )
    .expect("write first bibliography");
    fs::write(
        root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta} Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("write second bibliography");

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
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/body.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("refsb.bbl"),
                Utf8PathBuf::from("sections/tail.tex"),
            ],
        })
        .await
        .expect("first semantic aux build should succeed");
    assert_eq!(
        first.page_metadata.len(),
        1,
        "fixture should keep included body, both bibliography files, and tail on the same page"
    );
    let first_bundle =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let bibliography_checkpoint = first_bundle
        .checkpoints
        .iter()
        .filter(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.input_boundary_kind == Some(VmModuleCheckpointKind::Enter)
                && checkpoint.meta.module_path.as_ref().is_some_and(|path| {
                    path == &Utf8PathBuf::from("refsa.bbl")
                        || path == &Utf8PathBuf::from("refsb.bbl")
                })
        })
        .min_by_key(|checkpoint| checkpoint.meta.output_start_utf8)
        .expect("same-page bibliography checkpoint");

    SamePageIncludedBodyTrailingInputSiblingOrderingFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        bibliography_checkpoint_id: bibliography_checkpoint.meta.checkpoint_id.clone(),
        bibliography_page_index_after: bibliography_checkpoint.meta.page_index_after,
    }
}

fn rewrite_same_page_included_body_trailing_input_sibling_ordering(
    fixture: &SamePageIncludedBodyTrailingInputSiblingOrderingFixture,
) {
    fs::write(fixture.root.join("sections/tail.tex"), "Tail B.").expect("rewrite tail");
    fs::write(
        fixture.root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta}  Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite second bibliography");
}

async fn compile_same_page_included_body_trailing_input_sibling_ordering_second_pass(
    fixture: &SamePageIncludedBodyTrailingInputSiblingOrderingFixture,
    dirty_files: &[Utf8PathBuf],
) -> CompileOutcome {
    fixture
        .driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: fixture.build_root.clone(),
            changed_files: dirty_files.to_vec(),
        })
        .await
        .expect("second semantic aux build should succeed")
}

fn assert_same_page_included_body_trailing_input_sibling_ordering_replay(
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
    expected_checkpoint_id: &str,
    expected_page_index: usize,
) {
    let second_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-2/sources.json")).expect("read second sources"),
    )
    .expect("parse second sources");
    assert!(
        second_sources.executed_files[&Utf8PathBuf::from("sections/tail.tex")].contains("Tail B."),
        "executed tail.tex should reflect the later tracked change"
    );
    assert_eq!(
        second.reused_checkpoint_id.as_deref(),
        Some(expected_checkpoint_id)
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files.to_vec());
    assert_eq!(
        build_meta.start_checkpoint_id.as_deref(),
        Some(expected_checkpoint_id)
    );
    assert_eq!(build_meta.start_page_index, expected_page_index);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert!(build_meta.rebuilt_page_count >= 1);
    assert_eq!(
        build_meta.rebuilt_page_count + build_meta.reused_page_count,
        build_meta.page_count
    );
    assert_eq!(build_meta.semantic_pass_count, 1);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(build_meta.semantic_aux_backdated);
}

async fn run_same_page_included_body_trailing_input_sibling_ordering_replay(
    case: SamePageIncludedBodyTrailingInputSiblingOrderingCase,
) {
    let fixture = prepare_same_page_included_body_trailing_input_sibling_ordering_fixture().await;
    rewrite_same_page_included_body_trailing_input_sibling_ordering(&fixture);

    let dirty_files = match case {
        SamePageIncludedBodyTrailingInputSiblingOrderingCase::Baseline => vec![
            Utf8PathBuf::from("refsb.bbl"),
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("refsa.bbl"),
        ],
        SamePageIncludedBodyTrailingInputSiblingOrderingCase::Reversed => vec![
            Utf8PathBuf::from("refsa.bbl"),
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("refsb.bbl"),
        ],
        SamePageIncludedBodyTrailingInputSiblingOrderingCase::Interleaved => vec![
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("refsb.bbl"),
            Utf8PathBuf::from("refsa.bbl"),
        ],
        SamePageIncludedBodyTrailingInputSiblingOrderingCase::OtherInterleaved => vec![
            Utf8PathBuf::from("sections/tail.tex"),
            Utf8PathBuf::from("refsa.bbl"),
            Utf8PathBuf::from("refsb.bbl"),
        ],
    };
    let second = compile_same_page_included_body_trailing_input_sibling_ordering_second_pass(
        &fixture,
        &dirty_files,
    )
    .await;

    assert_same_page_included_body_trailing_input_sibling_ordering_replay(
        &fixture.build_root,
        &second,
        &dirty_files,
        &fixture.bibliography_checkpoint_id,
        fixture.bibliography_page_index_after,
    );
}
