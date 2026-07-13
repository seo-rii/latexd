struct UnchangedTailWithToplevelLeadingNoiseFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first: CompileOutcome,
    body_a: String,
    body_b: String,
}

#[derive(Clone, Copy)]
enum UnchangedTailWithToplevelLeadingNoiseKind {
    Untracked,
    Unreadable,
}

#[derive(Clone, Copy)]
enum UnchangedTailWithToplevelLeadingNoiseDirtyOrder {
    Precedes,
    ReversedPrecedes,
    Follows,
    ReversedFollows,
    InterleavedPrecedes,
    InterleavedFollows,
    OtherInterleavedPrecedes,
    OtherInterleavedFollows,
}

enum UnchangedTailWithToplevelLeadingNoiseCase {
    UntrackedPrecedes,
    ReversedUntrackedPrecedes,
    UnreadablePrecedes,
    ReversedUnreadablePrecedes,
    UntrackedFollows,
    ReversedUntrackedFollows,
    UnreadableFollows,
    ReversedUnreadableFollows,
    InterleavedUntrackedPrecedes,
    InterleavedUnreadablePrecedes,
    InterleavedUntrackedFollows,
    InterleavedUnreadableFollows,
    OtherInterleavedUntrackedPrecedes,
    OtherInterleavedUnreadablePrecedes,
    OtherInterleavedUntrackedFollows,
    OtherInterleavedUnreadableFollows,
}

type LeadingNoiseCase = UnchangedTailWithToplevelLeadingNoiseCase;

async fn prepare_unchanged_tail_with_toplevel_leading_noise_fixture()
-> UnchangedTailWithToplevelLeadingNoiseFixture {
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

    UnchangedTailWithToplevelLeadingNoiseFixture {
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

fn rewrite_unchanged_tail_with_toplevel_leading_noise(
    fixture: &UnchangedTailWithToplevelLeadingNoiseFixture,
) {
    fs::write(
        fixture.root.join("main.tex"),
        "% leading comment before documentclass\n\\documentclass{article}\\begin{document}\n\\input{sections/body-a}\n\\input{sections/body-b}\n\\end{document}\n",
    )
    .expect("rewrite main tex");
    fs::write(
        fixture.root.join("sections/body-a.tex"),
        format!("{}\n% body-a trailing comment\n", fixture.body_a),
    )
    .expect("rewrite body a");
    fs::write(
        fixture.root.join("sections/body-b.tex"),
        format!("{}\n% body-b trailing comment\n", fixture.body_b),
    )
    .expect("rewrite body b");
}

fn write_unchanged_tail_with_toplevel_leading_noise_untracked(root: &Utf8Path) {
    fs::write(root.join("notes.txt"), "scratch notes").expect("write notes");
}

fn write_unchanged_tail_with_toplevel_leading_noise_unreadable(root: &Utf8Path) {
    fs::create_dir_all(root.join("notes.txt")).expect("create unreadable dirty dir");
}

async fn compile_unchanged_tail_with_toplevel_leading_noise_second_pass(
    fixture: &UnchangedTailWithToplevelLeadingNoiseFixture,
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

fn assert_unchanged_tail_with_toplevel_leading_noise_reuse(
    fixture: &UnchangedTailWithToplevelLeadingNoiseFixture,
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
    assert_eq!(second.reused_checkpoint_id, None);
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
    assert_eq!(build_meta.start_checkpoint_id, None);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(build_meta.rebuilt_page_count, 0);
    assert_eq!(build_meta.reused_page_count, build_meta.page_count);
    assert_eq!(build_meta.start_page_index, 0);
    assert_eq!(build_meta.semantic_pass_count, 0);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}

async fn run_unchanged_tail_with_toplevel_leading_noise_reuse(
    noise_kind: UnchangedTailWithToplevelLeadingNoiseKind,
    dirty_order: UnchangedTailWithToplevelLeadingNoiseDirtyOrder,
) {
    let fixture = prepare_unchanged_tail_with_toplevel_leading_noise_fixture().await;
    rewrite_unchanged_tail_with_toplevel_leading_noise(&fixture);
    match noise_kind {
        UnchangedTailWithToplevelLeadingNoiseKind::Untracked => {
            write_unchanged_tail_with_toplevel_leading_noise_untracked(&fixture.root);
        }
        UnchangedTailWithToplevelLeadingNoiseKind::Unreadable => {
            write_unchanged_tail_with_toplevel_leading_noise_unreadable(&fixture.root);
        }
    }
    let changed_files = match dirty_order {
        UnchangedTailWithToplevelLeadingNoiseDirtyOrder::Precedes => [
            "notes.txt",
            "sections/body-a.tex",
            "sections/body-b.tex",
            "main.tex",
        ],
        UnchangedTailWithToplevelLeadingNoiseDirtyOrder::ReversedPrecedes => [
            "notes.txt",
            "main.tex",
            "sections/body-b.tex",
            "sections/body-a.tex",
        ],
        UnchangedTailWithToplevelLeadingNoiseDirtyOrder::Follows => [
            "sections/body-a.tex",
            "sections/body-b.tex",
            "main.tex",
            "notes.txt",
        ],
        UnchangedTailWithToplevelLeadingNoiseDirtyOrder::ReversedFollows => [
            "main.tex",
            "sections/body-b.tex",
            "sections/body-a.tex",
            "notes.txt",
        ],
        UnchangedTailWithToplevelLeadingNoiseDirtyOrder::InterleavedPrecedes => [
            "notes.txt",
            "sections/body-a.tex",
            "main.tex",
            "sections/body-b.tex",
        ],
        UnchangedTailWithToplevelLeadingNoiseDirtyOrder::InterleavedFollows => [
            "sections/body-a.tex",
            "main.tex",
            "sections/body-b.tex",
            "notes.txt",
        ],
        UnchangedTailWithToplevelLeadingNoiseDirtyOrder::OtherInterleavedPrecedes => [
            "notes.txt",
            "sections/body-b.tex",
            "main.tex",
            "sections/body-a.tex",
        ],
        UnchangedTailWithToplevelLeadingNoiseDirtyOrder::OtherInterleavedFollows => [
            "sections/body-b.tex",
            "main.tex",
            "sections/body-a.tex",
            "notes.txt",
        ],
    };
    let changed_files = changed_files
        .iter()
        .copied()
        .map(Utf8PathBuf::from)
        .collect::<Vec<_>>();
    let second =
        compile_unchanged_tail_with_toplevel_leading_noise_second_pass(&fixture, &changed_files)
            .await;
    assert_unchanged_tail_with_toplevel_leading_noise_reuse(&fixture, &second, &changed_files);
}

async fn run_unchanged_tail_with_toplevel_leading_noise_case(
    case: UnchangedTailWithToplevelLeadingNoiseCase,
) {
    let (noise_kind, dirty_order) = match case {
        UnchangedTailWithToplevelLeadingNoiseCase::UntrackedPrecedes => (
            UnchangedTailWithToplevelLeadingNoiseKind::Untracked,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::Precedes,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::ReversedUntrackedPrecedes => (
            UnchangedTailWithToplevelLeadingNoiseKind::Untracked,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::ReversedPrecedes,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::UnreadablePrecedes => (
            UnchangedTailWithToplevelLeadingNoiseKind::Unreadable,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::Precedes,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::ReversedUnreadablePrecedes => (
            UnchangedTailWithToplevelLeadingNoiseKind::Unreadable,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::ReversedPrecedes,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::UntrackedFollows => (
            UnchangedTailWithToplevelLeadingNoiseKind::Untracked,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::Follows,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::ReversedUntrackedFollows => (
            UnchangedTailWithToplevelLeadingNoiseKind::Untracked,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::ReversedFollows,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::UnreadableFollows => (
            UnchangedTailWithToplevelLeadingNoiseKind::Unreadable,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::Follows,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::ReversedUnreadableFollows => (
            UnchangedTailWithToplevelLeadingNoiseKind::Unreadable,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::ReversedFollows,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::InterleavedUntrackedPrecedes => (
            UnchangedTailWithToplevelLeadingNoiseKind::Untracked,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::InterleavedPrecedes,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::InterleavedUnreadablePrecedes => (
            UnchangedTailWithToplevelLeadingNoiseKind::Unreadable,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::InterleavedPrecedes,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::InterleavedUntrackedFollows => (
            UnchangedTailWithToplevelLeadingNoiseKind::Untracked,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::InterleavedFollows,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::InterleavedUnreadableFollows => (
            UnchangedTailWithToplevelLeadingNoiseKind::Unreadable,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::InterleavedFollows,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::OtherInterleavedUntrackedPrecedes => (
            UnchangedTailWithToplevelLeadingNoiseKind::Untracked,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::OtherInterleavedPrecedes,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::OtherInterleavedUnreadablePrecedes => (
            UnchangedTailWithToplevelLeadingNoiseKind::Unreadable,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::OtherInterleavedPrecedes,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::OtherInterleavedUntrackedFollows => (
            UnchangedTailWithToplevelLeadingNoiseKind::Untracked,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::OtherInterleavedFollows,
        ),
        UnchangedTailWithToplevelLeadingNoiseCase::OtherInterleavedUnreadableFollows => (
            UnchangedTailWithToplevelLeadingNoiseKind::Unreadable,
            UnchangedTailWithToplevelLeadingNoiseDirtyOrder::OtherInterleavedFollows,
        ),
    };

    run_unchanged_tail_with_toplevel_leading_noise_reuse(noise_kind, dirty_order).await;
}

async fn run_leading_noise_case(case: LeadingNoiseCase) {
    run_unchanged_tail_with_toplevel_leading_noise_case(case).await;
}
