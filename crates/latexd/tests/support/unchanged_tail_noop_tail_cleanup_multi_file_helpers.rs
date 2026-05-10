enum TailCleanupMultiFileNoiseKind {
    None,
    Untracked,
    Unreadable,
}

enum TailCleanupMultiFileCase {
    Baseline,
    UntrackedFollows,
    UnreadableFollows,
    UntrackedPrecedes,
    UnreadablePrecedes,
    InterleavedUntrackedBetween,
    InterleavedUnreadableBetween,
    OtherInterleavedUntrackedBetween,
    OtherInterleavedUnreadableBetween,
}

type TailMultiCase = TailCleanupMultiFileCase;

async fn run_tail_multi_case(case: TailMultiCase) {
    run_unchanged_tail_tail_cleanup_multi_file_case(case).await;
}

struct UnchangedTailTailCleanupMultiFileFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first: CompileOutcome,
    original_body: String,
}

async fn prepare_unchanged_tail_tail_cleanup_multi_file_fixture()
-> UnchangedTailTailCleanupMultiFileFixture {
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

    UnchangedTailTailCleanupMultiFileFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        first,
        original_body,
    }
}

fn rewrite_unchanged_tail_tail_cleanup_multi_file(
    fixture: &UnchangedTailTailCleanupMultiFileFixture,
    noise_kind: TailCleanupMultiFileNoiseKind,
) {
    fs::write(
        fixture.root.join("main.tex"),
        "\\documentclass{article}\\begin{document}\\input{sections/body}\\end{document}\n% main trailing comment\n",
    )
    .expect("rewrite main tex");
    fs::write(
        fixture.root.join("sections/body.tex"),
        format!("{}\n% body trailing comment\n", fixture.original_body),
    )
    .expect("rewrite body tex");
    match noise_kind {
        TailCleanupMultiFileNoiseKind::None => {}
        TailCleanupMultiFileNoiseKind::Untracked => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        TailCleanupMultiFileNoiseKind::Unreadable => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }
}

async fn compile_unchanged_tail_tail_cleanup_multi_file_second_pass(
    fixture: &UnchangedTailTailCleanupMultiFileFixture,
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

async fn run_unchanged_tail_tail_cleanup_multi_file_reuse(
    noise_kind: TailCleanupMultiFileNoiseKind,
    changed_files: &[&str],
) {
    let fixture = prepare_unchanged_tail_tail_cleanup_multi_file_fixture().await;
    rewrite_unchanged_tail_tail_cleanup_multi_file(&fixture, noise_kind);
    let changed_files = changed_files
        .iter()
        .map(|changed_file| Utf8PathBuf::from(*changed_file))
        .collect::<Vec<_>>();
    let second =
        compile_unchanged_tail_tail_cleanup_multi_file_second_pass(&fixture, &changed_files).await;
    assert_unchanged_tail_tail_cleanup_multi_file_reuse(&fixture, &second, &changed_files);
}

async fn run_unchanged_tail_tail_cleanup_multi_file_case(case: TailCleanupMultiFileCase) {
    match case {
        TailCleanupMultiFileCase::Baseline => {
            run_unchanged_tail_tail_cleanup_multi_file_reuse(
                TailCleanupMultiFileNoiseKind::None,
                &["sections/body.tex", "main.tex"],
            )
            .await;
        }
        TailCleanupMultiFileCase::UntrackedFollows => {
            run_unchanged_tail_tail_cleanup_multi_file_reuse(
                TailCleanupMultiFileNoiseKind::Untracked,
                &["sections/body.tex", "main.tex", "notes.txt"],
            )
            .await;
        }
        TailCleanupMultiFileCase::UnreadableFollows => {
            run_unchanged_tail_tail_cleanup_multi_file_reuse(
                TailCleanupMultiFileNoiseKind::Unreadable,
                &["sections/body.tex", "main.tex", "notes.txt"],
            )
            .await;
        }
        TailCleanupMultiFileCase::UntrackedPrecedes => {
            run_unchanged_tail_tail_cleanup_multi_file_reuse(
                TailCleanupMultiFileNoiseKind::Untracked,
                &["notes.txt", "sections/body.tex", "main.tex"],
            )
            .await;
        }
        TailCleanupMultiFileCase::UnreadablePrecedes => {
            run_unchanged_tail_tail_cleanup_multi_file_reuse(
                TailCleanupMultiFileNoiseKind::Unreadable,
                &["notes.txt", "sections/body.tex", "main.tex"],
            )
            .await;
        }
        TailCleanupMultiFileCase::InterleavedUntrackedBetween => {
            run_unchanged_tail_tail_cleanup_multi_file_reuse(
                TailCleanupMultiFileNoiseKind::Untracked,
                &["sections/body.tex", "notes.txt", "main.tex"],
            )
            .await;
        }
        TailCleanupMultiFileCase::InterleavedUnreadableBetween => {
            run_unchanged_tail_tail_cleanup_multi_file_reuse(
                TailCleanupMultiFileNoiseKind::Unreadable,
                &["sections/body.tex", "notes.txt", "main.tex"],
            )
            .await;
        }
        TailCleanupMultiFileCase::OtherInterleavedUntrackedBetween => {
            run_unchanged_tail_tail_cleanup_multi_file_reuse(
                TailCleanupMultiFileNoiseKind::Untracked,
                &["main.tex", "notes.txt", "sections/body.tex"],
            )
            .await;
        }
        TailCleanupMultiFileCase::OtherInterleavedUnreadableBetween => {
            run_unchanged_tail_tail_cleanup_multi_file_reuse(
                TailCleanupMultiFileNoiseKind::Unreadable,
                &["main.tex", "notes.txt", "sections/body.tex"],
            )
            .await;
        }
    }
}

fn assert_unchanged_tail_tail_cleanup_multi_file_reuse(
    fixture: &UnchangedTailTailCleanupMultiFileFixture,
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
