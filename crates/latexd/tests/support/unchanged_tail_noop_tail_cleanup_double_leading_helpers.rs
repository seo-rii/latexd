enum TailCleanupDoubleLeadingNoiseKind {
    None,
    Untracked,
    Unreadable,
}

enum TailCleanupDoubleLeadingCase {
    Baseline,
    UntrackedFollows,
    UnreadableFollows,
    UntrackedPrecedes,
    UnreadablePrecedes,
    InterleavedUntrackedBetween,
    InterleavedUnreadableBetween,
}

type TailDoubleCase = TailCleanupDoubleLeadingCase;

async fn run_tail_double_case(case: TailDoubleCase) {
    run_unchanged_tail_tail_cleanup_double_leading_case(case).await;
}

struct UnchangedTailTailCleanupDoubleLeadingFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first: CompileOutcome,
    original_body: String,
}

async fn prepare_unchanged_tail_tail_cleanup_double_leading_fixture()
-> UnchangedTailTailCleanupDoubleLeadingFixture {
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
    let original_body = (0..1536)
        .map(|index| format!("w{index:07}"))
        .collect::<Vec<_>>()
        .join(" ");
    fs::write(root.join("sections/body.tex"), &original_body).expect("write body tex");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\input{sections/body}\\end{document}\n",
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
                Utf8PathBuf::from("sections/body.tex"),
            ],
        })
        .await
        .expect("first build should succeed");
    assert!(
        !first.page_metadata.is_empty(),
        "fixture should render at least one page"
    );

    UnchangedTailTailCleanupDoubleLeadingFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        first,
        original_body,
    }
}

fn rewrite_unchanged_tail_tail_cleanup_double_leading(
    fixture: &UnchangedTailTailCleanupDoubleLeadingFixture,
    noise_kind: TailCleanupDoubleLeadingNoiseKind,
) {
    fs::write(
        fixture.root.join("main.tex"),
        "% leading comment before documentclass\n\\documentclass{article}\\begin{document}\\input{sections/body}\\end{document}\n",
    )
    .expect("rewrite main tex");
    fs::write(
        fixture.root.join("sections/body.tex"),
        format!(
            "% leading comment before body content\n{}\n",
            fixture.original_body
        ),
    )
    .expect("rewrite body tex");
    match noise_kind {
        TailCleanupDoubleLeadingNoiseKind::None => {}
        TailCleanupDoubleLeadingNoiseKind::Untracked => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        TailCleanupDoubleLeadingNoiseKind::Unreadable => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }
}

async fn compile_unchanged_tail_tail_cleanup_double_leading_second_pass(
    fixture: &UnchangedTailTailCleanupDoubleLeadingFixture,
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
        .expect("second build should succeed")
}

async fn run_unchanged_tail_tail_cleanup_double_leading_reuse(
    noise_kind: TailCleanupDoubleLeadingNoiseKind,
    dirty_files: &[&str],
) {
    let fixture = prepare_unchanged_tail_tail_cleanup_double_leading_fixture().await;
    rewrite_unchanged_tail_tail_cleanup_double_leading(&fixture, noise_kind);
    let dirty_files = dirty_files
        .iter()
        .map(|dirty_file| Utf8PathBuf::from(*dirty_file))
        .collect::<Vec<_>>();
    let second =
        compile_unchanged_tail_tail_cleanup_double_leading_second_pass(&fixture, &dirty_files)
            .await;
    assert_unchanged_tail_tail_cleanup_double_leading_reuse(&fixture, &second, &dirty_files);
}

async fn run_unchanged_tail_tail_cleanup_double_leading_case(case: TailCleanupDoubleLeadingCase) {
    match case {
        TailCleanupDoubleLeadingCase::Baseline => {
            run_unchanged_tail_tail_cleanup_double_leading_reuse(
                TailCleanupDoubleLeadingNoiseKind::None,
                &["sections/body.tex", "main.tex"],
            )
            .await;
        }
        TailCleanupDoubleLeadingCase::UntrackedFollows => {
            run_unchanged_tail_tail_cleanup_double_leading_reuse(
                TailCleanupDoubleLeadingNoiseKind::Untracked,
                &["sections/body.tex", "main.tex", "notes.txt"],
            )
            .await;
        }
        TailCleanupDoubleLeadingCase::UnreadableFollows => {
            run_unchanged_tail_tail_cleanup_double_leading_reuse(
                TailCleanupDoubleLeadingNoiseKind::Unreadable,
                &["sections/body.tex", "main.tex", "notes.txt"],
            )
            .await;
        }
        TailCleanupDoubleLeadingCase::UntrackedPrecedes => {
            run_unchanged_tail_tail_cleanup_double_leading_reuse(
                TailCleanupDoubleLeadingNoiseKind::Untracked,
                &["notes.txt", "sections/body.tex", "main.tex"],
            )
            .await;
        }
        TailCleanupDoubleLeadingCase::UnreadablePrecedes => {
            run_unchanged_tail_tail_cleanup_double_leading_reuse(
                TailCleanupDoubleLeadingNoiseKind::Unreadable,
                &["notes.txt", "sections/body.tex", "main.tex"],
            )
            .await;
        }
        TailCleanupDoubleLeadingCase::InterleavedUntrackedBetween => {
            run_unchanged_tail_tail_cleanup_double_leading_reuse(
                TailCleanupDoubleLeadingNoiseKind::Untracked,
                &["sections/body.tex", "notes.txt", "main.tex"],
            )
            .await;
        }
        TailCleanupDoubleLeadingCase::InterleavedUnreadableBetween => {
            run_unchanged_tail_tail_cleanup_double_leading_reuse(
                TailCleanupDoubleLeadingNoiseKind::Unreadable,
                &["sections/body.tex", "notes.txt", "main.tex"],
            )
            .await;
        }
    }
}

fn assert_unchanged_tail_tail_cleanup_double_leading_reuse(
    fixture: &UnchangedTailTailCleanupDoubleLeadingFixture,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
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
    assert_eq!(build_meta.dirty_files, dirty_files.to_vec());
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
