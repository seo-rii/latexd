enum BetweenNoiseUnreadableDirtyOrder {
    Interleaved,
    OtherInterleaved,
}

type InputBetweenUnreadableCase = BetweenNoiseUnreadableDirtyOrder;

async fn run_input_between_unreadable_case(case: InputBetweenUnreadableCase) {
    run_unchanged_tail_multi_input_between_noise_unreadable_reuse(case).await;
}

struct UnchangedTailMultiInputBetweenNoiseUnreadableFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first: CompileOutcome,
    body_a: String,
    body_b: String,
}

async fn prepare_unchanged_tail_multi_input_between_noise_unreadable_fixture()
-> UnchangedTailMultiInputBetweenNoiseUnreadableFixture {
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
    fs::create_dir_all(root.join("sections")).expect("create sections dir");
    let body_a = (0..768)
        .map(|index| format!("a{index:07}"))
        .collect::<Vec<_>>()
        .join(" ");
    let body_b = (0..768)
        .map(|index| format!("b{index:07}"))
        .collect::<Vec<_>>()
        .join(" ");
    fs::write(root.join("sections/body-a.tex"), &body_a).expect("write body a");
    fs::write(root.join("sections/body-b.tex"), &body_b).expect("write body b");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\n\\input{sections/body-a}\n\\input{sections/body-b}\n\\end{document}\n",
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
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/body-a.tex"),
                Utf8PathBuf::from("sections/body-b.tex"),
            ],
        })
        .await
        .expect("first build should succeed");
    assert!(
        !first.page_metadata.is_empty(),
        "fixture should render at least one page"
    );

    UnchangedTailMultiInputBetweenNoiseUnreadableFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        first,
        body_a,
        body_b,
    }
}

fn rewrite_unchanged_tail_multi_input_between_noise_unreadable(
    fixture: &UnchangedTailMultiInputBetweenNoiseUnreadableFixture,
) {
    fs::write(
        fixture.root.join("sections/body-a.tex"),
        format!("{}% body-a trailing comment\n", fixture.body_a),
    )
    .expect("rewrite body a");
    fs::write(
        fixture.root.join("sections/body-b.tex"),
        format!("{}% body-b trailing comment\n", fixture.body_b),
    )
    .expect("rewrite body b");
    fs::create_dir_all(fixture.root.join("notes.txt")).expect("create unreadable dirty dir");
}

async fn run_unchanged_tail_multi_input_between_noise_unreadable_reuse(
    dirty_order: BetweenNoiseUnreadableDirtyOrder,
) {
    let fixture = prepare_unchanged_tail_multi_input_between_noise_unreadable_fixture().await;
    rewrite_unchanged_tail_multi_input_between_noise_unreadable(&fixture);
    let changed_files = match dirty_order {
        BetweenNoiseUnreadableDirtyOrder::Interleaved => vec![
            Utf8PathBuf::from("sections/body-a.tex"),
            Utf8PathBuf::from("notes.txt"),
            Utf8PathBuf::from("sections/body-b.tex"),
        ],
        BetweenNoiseUnreadableDirtyOrder::OtherInterleaved => vec![
            Utf8PathBuf::from("sections/body-b.tex"),
            Utf8PathBuf::from("notes.txt"),
            Utf8PathBuf::from("sections/body-a.tex"),
        ],
    };
    let second = compile_unchanged_tail_multi_input_between_noise_unreadable_second_pass(
        &fixture,
        &changed_files,
    )
    .await;
    assert_unchanged_tail_multi_input_between_noise_unreadable_reuse(
        &fixture,
        &second,
        &changed_files,
    );
}

async fn compile_unchanged_tail_multi_input_between_noise_unreadable_second_pass(
    fixture: &UnchangedTailMultiInputBetweenNoiseUnreadableFixture,
    changed_files: &[Utf8PathBuf],
) -> CompileOutcome {
    fixture
        .driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: fixture.build_root.clone(),
            changed_files: changed_files.to_vec(),
        })
        .await
        .expect("second build should succeed")
}

fn assert_unchanged_tail_multi_input_between_noise_unreadable_reuse(
    fixture: &UnchangedTailMultiInputBetweenNoiseUnreadableFixture,
    second: &CompileOutcome,
    changed_files: &[Utf8PathBuf],
) {
    let tail = second.unchanged_tail.as_ref().expect("unchanged tail");
    assert_eq!(tail.previous_rev, 1);
    assert_eq!(tail.previous_page_start, 0);
    assert_eq!(tail.current_page_start, 0);
    assert_eq!(tail.page_count, fixture.first.page_metadata.len());
    assert_eq!(tail.page_count, second.page_metadata.len());
    assert_eq!(
        second
            .page_metadata
            .iter()
            .map(|page| page.page_id.as_str())
            .collect::<Vec<_>>(),
        fixture
            .first
            .page_metadata
            .iter()
            .map(|page| page.page_id.as_str())
            .collect::<Vec<_>>()
    );
    assert!(second.reused_checkpoint_id.is_some());
    assert!(second.page_patches.is_empty());
    assert!(
        second
            .page_artifacts
            .iter()
            .all(|page| page.pdf_url.starts_with("/artifacts/rev/1/pages/"))
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(fixture.build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(!build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, changed_files.to_vec());
    assert_eq!(build_meta.start_checkpoint_id, second.reused_checkpoint_id);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(build_meta.rebuilt_page_count, 0);
    assert_eq!(build_meta.reused_page_count, build_meta.page_count);
    assert!(build_meta.start_page_index <= build_meta.page_count);
    assert_eq!(build_meta.semantic_pass_count, 0);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}
