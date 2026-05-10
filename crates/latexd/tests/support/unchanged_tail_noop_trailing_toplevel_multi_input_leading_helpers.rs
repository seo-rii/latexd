struct UnchangedTailTrailingToplevelMultiInputLeadingFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first: CompileOutcome,
    body_a: String,
    body_b: String,
}

async fn prepare_unchanged_tail_trailing_toplevel_multi_input_leading_fixture()
-> UnchangedTailTrailingToplevelMultiInputLeadingFixture {
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

    UnchangedTailTrailingToplevelMultiInputLeadingFixture {
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

fn rewrite_unchanged_tail_trailing_toplevel_multi_input_leading(
    root: &Utf8Path,
    body_a: &str,
    body_b: &str,
) {
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\n\\input{sections/body-a}\n\\input{sections/body-b}\n\\end{document}\n% trailing comment in main\n",
    )
    .expect("rewrite main tex");
    fs::write(
        root.join("sections/body-a.tex"),
        format!("% leading comment before body-a content\n{}\n", body_a),
    )
    .expect("rewrite body a");
    fs::write(
        root.join("sections/body-b.tex"),
        format!("% leading comment before body-b content\n{}\n", body_b),
    )
    .expect("rewrite body b");
}

fn rewrite_unchanged_tail_trailing_toplevel_mixed_multi_input(
    root: &Utf8Path,
    body_a: &str,
    body_b: &str,
) {
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\n\\input{sections/body-a}\n\\input{sections/body-b}\n\\end{document}\n% trailing comment in main\n",
    )
    .expect("rewrite main tex");
    fs::write(
        root.join("sections/body-a.tex"),
        format!("% leading comment before body-a content\n{}\n", body_a),
    )
    .expect("rewrite body a");
    fs::write(
        root.join("sections/body-b.tex"),
        format!("{}% body-b trailing comment\n", body_b),
    )
    .expect("rewrite body b");
}

fn write_unchanged_tail_trailing_toplevel_multi_input_leading_untracked(root: &Utf8Path) {
    fs::write(root.join("notes.txt"), "scratch notes").expect("write notes");
}

fn write_unchanged_tail_trailing_toplevel_multi_input_leading_unreadable(root: &Utf8Path) {
    fs::create_dir_all(root.join("notes.txt")).expect("create unreadable dirty dir");
}

#[derive(Clone, Copy)]
enum UnchangedTailMultiInputNoise {
    NoExtraDirty,
    Untracked,
    Unreadable,
}

#[derive(Clone, Copy)]
enum UnchangedTailMultiInputRewrite {
    Leading,
    Mixed,
}

#[derive(Clone, Copy)]
enum UnchangedTailMultiInputDirtyOrder {
    InterleavedPrecedes,
    InterleavedFollows,
    OtherInterleavedPrecedes,
    OtherInterleavedFollows,
}

enum UnchangedTailMultiInputLeadingCase {
    Baseline,
    ReversedDirtyOrder,
    InterleavedDirtyOrder,
    OtherInterleavedDirtyOrder,
    InterleavedUntrackedPrecedes,
    InterleavedUntrackedFollows,
    InterleavedUnreadablePrecedes,
    InterleavedUnreadableFollows,
    OtherInterleavedUntrackedPrecedes,
    OtherInterleavedUntrackedFollows,
    OtherInterleavedUnreadablePrecedes,
    OtherInterleavedUnreadableFollows,
}

enum UnchangedTailMultiInputMixedCase {
    Baseline,
    ReversedDirtyOrder,
    InterleavedDirtyOrder,
    OtherInterleavedDirtyOrder,
    InterleavedUntrackedPrecedes,
    InterleavedUntrackedFollows,
    InterleavedUnreadablePrecedes,
    InterleavedUnreadableFollows,
    OtherInterleavedUntrackedPrecedes,
    OtherInterleavedUntrackedFollows,
    OtherInterleavedUnreadablePrecedes,
    OtherInterleavedUnreadableFollows,
}

type MultiLeadCase = UnchangedTailMultiInputLeadingCase;
type MultiMixCase = UnchangedTailMultiInputMixedCase;

async fn run_multi_lead_case(case: MultiLeadCase) {
    run_unchanged_tail_multi_input_leading_case(case).await;
}

async fn run_multi_mix_case(case: MultiMixCase) {
    run_unchanged_tail_multi_input_mixed_case(case).await;
}

fn unchanged_tail_multi_input_dirty_order_files(
    dirty_order: UnchangedTailMultiInputDirtyOrder,
) -> [&'static str; 4] {
    match dirty_order {
        UnchangedTailMultiInputDirtyOrder::InterleavedPrecedes => [
            "notes.txt",
            "sections/body-a.tex",
            "main.tex",
            "sections/body-b.tex",
        ],
        UnchangedTailMultiInputDirtyOrder::InterleavedFollows => [
            "sections/body-a.tex",
            "main.tex",
            "sections/body-b.tex",
            "notes.txt",
        ],
        UnchangedTailMultiInputDirtyOrder::OtherInterleavedPrecedes => [
            "notes.txt",
            "sections/body-b.tex",
            "main.tex",
            "sections/body-a.tex",
        ],
        UnchangedTailMultiInputDirtyOrder::OtherInterleavedFollows => [
            "sections/body-b.tex",
            "main.tex",
            "sections/body-a.tex",
            "notes.txt",
        ],
    }
}

async fn run_unchanged_tail_multi_input_leading_reuse(
    changed_files: &[&str],
    noise: UnchangedTailMultiInputNoise,
) {
    run_unchanged_tail_multi_input_reuse(
        changed_files,
        noise,
        UnchangedTailMultiInputRewrite::Leading,
    )
    .await;
}

async fn run_unchanged_tail_multi_input_leading_order_reuse(
    dirty_order: UnchangedTailMultiInputDirtyOrder,
    noise: UnchangedTailMultiInputNoise,
) {
    let changed_files = unchanged_tail_multi_input_dirty_order_files(dirty_order);
    run_unchanged_tail_multi_input_leading_reuse(&changed_files, noise).await;
}

async fn run_unchanged_tail_multi_input_leading_case(case: UnchangedTailMultiInputLeadingCase) {
    match case {
        UnchangedTailMultiInputLeadingCase::Baseline => {
            run_unchanged_tail_multi_input_leading_reuse(
                &["sections/body-a.tex", "sections/body-b.tex", "main.tex"],
                UnchangedTailMultiInputNoise::NoExtraDirty,
            )
            .await;
        }
        UnchangedTailMultiInputLeadingCase::ReversedDirtyOrder => {
            run_unchanged_tail_multi_input_leading_reuse(
                &["main.tex", "sections/body-b.tex", "sections/body-a.tex"],
                UnchangedTailMultiInputNoise::NoExtraDirty,
            )
            .await;
        }
        UnchangedTailMultiInputLeadingCase::InterleavedDirtyOrder => {
            run_unchanged_tail_multi_input_leading_reuse(
                &["sections/body-a.tex", "main.tex", "sections/body-b.tex"],
                UnchangedTailMultiInputNoise::NoExtraDirty,
            )
            .await;
        }
        UnchangedTailMultiInputLeadingCase::OtherInterleavedDirtyOrder => {
            run_unchanged_tail_multi_input_leading_reuse(
                &["sections/body-b.tex", "main.tex", "sections/body-a.tex"],
                UnchangedTailMultiInputNoise::NoExtraDirty,
            )
            .await;
        }
        UnchangedTailMultiInputLeadingCase::InterleavedUntrackedPrecedes => {
            run_unchanged_tail_multi_input_leading_order_reuse(
                UnchangedTailMultiInputDirtyOrder::InterleavedPrecedes,
                UnchangedTailMultiInputNoise::Untracked,
            )
            .await;
        }
        UnchangedTailMultiInputLeadingCase::InterleavedUntrackedFollows => {
            run_unchanged_tail_multi_input_leading_order_reuse(
                UnchangedTailMultiInputDirtyOrder::InterleavedFollows,
                UnchangedTailMultiInputNoise::Untracked,
            )
            .await;
        }
        UnchangedTailMultiInputLeadingCase::InterleavedUnreadablePrecedes => {
            run_unchanged_tail_multi_input_leading_order_reuse(
                UnchangedTailMultiInputDirtyOrder::InterleavedPrecedes,
                UnchangedTailMultiInputNoise::Unreadable,
            )
            .await;
        }
        UnchangedTailMultiInputLeadingCase::InterleavedUnreadableFollows => {
            run_unchanged_tail_multi_input_leading_order_reuse(
                UnchangedTailMultiInputDirtyOrder::InterleavedFollows,
                UnchangedTailMultiInputNoise::Unreadable,
            )
            .await;
        }
        UnchangedTailMultiInputLeadingCase::OtherInterleavedUntrackedPrecedes => {
            run_unchanged_tail_multi_input_leading_order_reuse(
                UnchangedTailMultiInputDirtyOrder::OtherInterleavedPrecedes,
                UnchangedTailMultiInputNoise::Untracked,
            )
            .await;
        }
        UnchangedTailMultiInputLeadingCase::OtherInterleavedUntrackedFollows => {
            run_unchanged_tail_multi_input_leading_order_reuse(
                UnchangedTailMultiInputDirtyOrder::OtherInterleavedFollows,
                UnchangedTailMultiInputNoise::Untracked,
            )
            .await;
        }
        UnchangedTailMultiInputLeadingCase::OtherInterleavedUnreadablePrecedes => {
            run_unchanged_tail_multi_input_leading_order_reuse(
                UnchangedTailMultiInputDirtyOrder::OtherInterleavedPrecedes,
                UnchangedTailMultiInputNoise::Unreadable,
            )
            .await;
        }
        UnchangedTailMultiInputLeadingCase::OtherInterleavedUnreadableFollows => {
            run_unchanged_tail_multi_input_leading_order_reuse(
                UnchangedTailMultiInputDirtyOrder::OtherInterleavedFollows,
                UnchangedTailMultiInputNoise::Unreadable,
            )
            .await;
        }
    }
}

async fn run_unchanged_tail_multi_input_mixed_reuse(
    changed_files: &[&str],
    noise: UnchangedTailMultiInputNoise,
) {
    run_unchanged_tail_multi_input_reuse(
        changed_files,
        noise,
        UnchangedTailMultiInputRewrite::Mixed,
    )
    .await;
}

async fn run_unchanged_tail_multi_input_mixed_order_reuse(
    dirty_order: UnchangedTailMultiInputDirtyOrder,
    noise: UnchangedTailMultiInputNoise,
) {
    let changed_files = unchanged_tail_multi_input_dirty_order_files(dirty_order);
    run_unchanged_tail_multi_input_mixed_reuse(&changed_files, noise).await;
}

async fn run_unchanged_tail_multi_input_mixed_case(case: UnchangedTailMultiInputMixedCase) {
    match case {
        UnchangedTailMultiInputMixedCase::Baseline => {
            run_unchanged_tail_multi_input_mixed_reuse(
                &["sections/body-a.tex", "sections/body-b.tex", "main.tex"],
                UnchangedTailMultiInputNoise::NoExtraDirty,
            )
            .await;
        }
        UnchangedTailMultiInputMixedCase::ReversedDirtyOrder => {
            run_unchanged_tail_multi_input_mixed_reuse(
                &["main.tex", "sections/body-b.tex", "sections/body-a.tex"],
                UnchangedTailMultiInputNoise::NoExtraDirty,
            )
            .await;
        }
        UnchangedTailMultiInputMixedCase::InterleavedDirtyOrder => {
            run_unchanged_tail_multi_input_mixed_reuse(
                &["sections/body-a.tex", "main.tex", "sections/body-b.tex"],
                UnchangedTailMultiInputNoise::NoExtraDirty,
            )
            .await;
        }
        UnchangedTailMultiInputMixedCase::OtherInterleavedDirtyOrder => {
            run_unchanged_tail_multi_input_mixed_reuse(
                &["sections/body-b.tex", "main.tex", "sections/body-a.tex"],
                UnchangedTailMultiInputNoise::NoExtraDirty,
            )
            .await;
        }
        UnchangedTailMultiInputMixedCase::InterleavedUntrackedPrecedes => {
            run_unchanged_tail_multi_input_mixed_order_reuse(
                UnchangedTailMultiInputDirtyOrder::InterleavedPrecedes,
                UnchangedTailMultiInputNoise::Untracked,
            )
            .await;
        }
        UnchangedTailMultiInputMixedCase::InterleavedUntrackedFollows => {
            run_unchanged_tail_multi_input_mixed_order_reuse(
                UnchangedTailMultiInputDirtyOrder::InterleavedFollows,
                UnchangedTailMultiInputNoise::Untracked,
            )
            .await;
        }
        UnchangedTailMultiInputMixedCase::InterleavedUnreadablePrecedes => {
            run_unchanged_tail_multi_input_mixed_order_reuse(
                UnchangedTailMultiInputDirtyOrder::InterleavedPrecedes,
                UnchangedTailMultiInputNoise::Unreadable,
            )
            .await;
        }
        UnchangedTailMultiInputMixedCase::InterleavedUnreadableFollows => {
            run_unchanged_tail_multi_input_mixed_order_reuse(
                UnchangedTailMultiInputDirtyOrder::InterleavedFollows,
                UnchangedTailMultiInputNoise::Unreadable,
            )
            .await;
        }
        UnchangedTailMultiInputMixedCase::OtherInterleavedUntrackedPrecedes => {
            run_unchanged_tail_multi_input_mixed_order_reuse(
                UnchangedTailMultiInputDirtyOrder::OtherInterleavedPrecedes,
                UnchangedTailMultiInputNoise::Untracked,
            )
            .await;
        }
        UnchangedTailMultiInputMixedCase::OtherInterleavedUntrackedFollows => {
            run_unchanged_tail_multi_input_mixed_order_reuse(
                UnchangedTailMultiInputDirtyOrder::OtherInterleavedFollows,
                UnchangedTailMultiInputNoise::Untracked,
            )
            .await;
        }
        UnchangedTailMultiInputMixedCase::OtherInterleavedUnreadablePrecedes => {
            run_unchanged_tail_multi_input_mixed_order_reuse(
                UnchangedTailMultiInputDirtyOrder::OtherInterleavedPrecedes,
                UnchangedTailMultiInputNoise::Unreadable,
            )
            .await;
        }
        UnchangedTailMultiInputMixedCase::OtherInterleavedUnreadableFollows => {
            run_unchanged_tail_multi_input_mixed_order_reuse(
                UnchangedTailMultiInputDirtyOrder::OtherInterleavedFollows,
                UnchangedTailMultiInputNoise::Unreadable,
            )
            .await;
        }
    }
}

async fn run_unchanged_tail_multi_input_reuse(
    changed_files: &[&str],
    noise: UnchangedTailMultiInputNoise,
    rewrite: UnchangedTailMultiInputRewrite,
) {
    let fixture = prepare_unchanged_tail_trailing_toplevel_multi_input_leading_fixture().await;
    match rewrite {
        UnchangedTailMultiInputRewrite::Leading => {
            rewrite_unchanged_tail_trailing_toplevel_multi_input_leading(
                &fixture.root,
                &fixture.body_a,
                &fixture.body_b,
            );
        }
        UnchangedTailMultiInputRewrite::Mixed => {
            rewrite_unchanged_tail_trailing_toplevel_mixed_multi_input(
                &fixture.root,
                &fixture.body_a,
                &fixture.body_b,
            );
        }
    }
    match noise {
        UnchangedTailMultiInputNoise::NoExtraDirty => {}
        UnchangedTailMultiInputNoise::Untracked => {
            write_unchanged_tail_trailing_toplevel_multi_input_leading_untracked(&fixture.root);
        }
        UnchangedTailMultiInputNoise::Unreadable => {
            write_unchanged_tail_trailing_toplevel_multi_input_leading_unreadable(&fixture.root);
        }
    }

    let changed_files = changed_files
        .iter()
        .map(|changed_file| Utf8PathBuf::from(*changed_file))
        .collect::<Vec<_>>();
    let second = compile_unchanged_tail_trailing_toplevel_multi_input_leading_second_pass(
        &fixture,
        changed_files.clone(),
    )
    .await;

    assert_unchanged_tail_trailing_toplevel_multi_input_leading_reuse(
        &fixture,
        &second,
        changed_files,
    );
}

async fn compile_unchanged_tail_trailing_toplevel_multi_input_leading_second_pass(
    fixture: &UnchangedTailTrailingToplevelMultiInputLeadingFixture,
    changed_files: Vec<Utf8PathBuf>,
) -> CompileOutcome {
    fixture
        .driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: fixture.build_root.clone(),
            changed_files,
        })
        .await
        .expect("second build should succeed")
}

fn assert_unchanged_tail_trailing_toplevel_multi_input_leading_reuse(
    fixture: &UnchangedTailTrailingToplevelMultiInputLeadingFixture,
    second: &CompileOutcome,
    changed_files: Vec<Utf8PathBuf>,
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
    assert_eq!(build_meta.dirty_files, changed_files);
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
