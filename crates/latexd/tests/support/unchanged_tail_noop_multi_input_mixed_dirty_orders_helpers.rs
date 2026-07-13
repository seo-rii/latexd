enum UnchangedTailNoopMixedDirtyOrdersEditMode {
    TrailingComments,
    LeadingAndTrailingComments,
}

enum UnchangedTailNoopMixedDirtyOrdersNoiseMode {
    Untracked,
    Unreadable,
}

enum UnchangedTailNoopMixedDirtyOrdersBaselineFollowsCase {
    Baseline,
    ReversedDirtyOrder,
    UntrackedFollows,
    UnreadableFollows,
    ReversedUntrackedFollows,
    ReversedUnreadableFollows,
}

enum UnchangedTailNoopMixedDirtyOrdersPrecedesCase {
    UntrackedPlain,
    UnreadablePlain,
    UntrackedReversed,
    UnreadableReversed,
}

type MixBaseFollowCase = UnchangedTailNoopMixedDirtyOrdersBaselineFollowsCase;
type MixPreCase = UnchangedTailNoopMixedDirtyOrdersPrecedesCase;

async fn run_mix_base_follow_case(case: MixBaseFollowCase) {
    run_unchanged_tail_noop_multi_input_mixed_dirty_orders_baseline_follows(case).await;
}

async fn run_mix_pre_case(case: MixPreCase) {
    run_unchanged_tail_noop_multi_input_mixed_dirty_orders_precedes(case).await;
}

async fn run_unchanged_tail_noop_multi_input_mixed_dirty_orders_case(
    edit_mode: UnchangedTailNoopMixedDirtyOrdersEditMode,
    noise_mode: Option<UnchangedTailNoopMixedDirtyOrdersNoiseMode>,
    changed_files: Vec<Utf8PathBuf>,
) {
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

    match edit_mode {
        UnchangedTailNoopMixedDirtyOrdersEditMode::TrailingComments => {
            fs::write(
                root.join("sections/body-a.tex"),
                format!("{}\n% body-a trailing comment\n", body_a),
            )
            .expect("rewrite body a");
        }
        UnchangedTailNoopMixedDirtyOrdersEditMode::LeadingAndTrailingComments => {
            fs::write(
                root.join("sections/body-a.tex"),
                format!("% leading comment before body-a content\n{}\n", body_a),
            )
            .expect("rewrite body a");
        }
    }
    fs::write(
        root.join("sections/body-b.tex"),
        format!("{}\n% body-b trailing comment\n", body_b),
    )
    .expect("rewrite body b");
    match noise_mode {
        Some(UnchangedTailNoopMixedDirtyOrdersNoiseMode::Untracked) => {
            fs::write(root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        Some(UnchangedTailNoopMixedDirtyOrdersNoiseMode::Unreadable) => {
            fs::create_dir_all(root.join("notes.txt")).expect("create unreadable dirty dir");
        }
        None => {}
    }

    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: changed_files.clone(),
        })
        .await
        .expect("second build should succeed");

    let tail = second.unchanged_tail.as_ref().expect("unchanged tail");
    assert_eq!(tail.previous_rev, 1);
    assert_eq!(tail.previous_page_start, 0);
    assert_eq!(tail.current_page_start, 0);
    assert_eq!(tail.page_count, first.page_metadata.len());
    assert_eq!(tail.page_count, second.page_metadata.len());
    assert_eq!(
        second
            .page_metadata
            .iter()
            .map(|page| page.page_id.as_str())
            .collect::<Vec<_>>(),
        first
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
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
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

async fn run_unchanged_tail_noop_multi_input_mixed_dirty_orders_baseline_follows(
    case: UnchangedTailNoopMixedDirtyOrdersBaselineFollowsCase,
) {
    let (edit_mode, noise_mode, changed_files) = match case {
        UnchangedTailNoopMixedDirtyOrdersBaselineFollowsCase::Baseline => (
            UnchangedTailNoopMixedDirtyOrdersEditMode::TrailingComments,
            None,
            vec![
                Utf8PathBuf::from("sections/body-a.tex"),
                Utf8PathBuf::from("sections/body-b.tex"),
            ],
        ),
        UnchangedTailNoopMixedDirtyOrdersBaselineFollowsCase::ReversedDirtyOrder => (
            UnchangedTailNoopMixedDirtyOrdersEditMode::TrailingComments,
            None,
            vec![
                Utf8PathBuf::from("sections/body-b.tex"),
                Utf8PathBuf::from("sections/body-a.tex"),
            ],
        ),
        UnchangedTailNoopMixedDirtyOrdersBaselineFollowsCase::UntrackedFollows => (
            UnchangedTailNoopMixedDirtyOrdersEditMode::LeadingAndTrailingComments,
            Some(UnchangedTailNoopMixedDirtyOrdersNoiseMode::Untracked),
            vec![
                Utf8PathBuf::from("sections/body-a.tex"),
                Utf8PathBuf::from("sections/body-b.tex"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        UnchangedTailNoopMixedDirtyOrdersBaselineFollowsCase::UnreadableFollows => (
            UnchangedTailNoopMixedDirtyOrdersEditMode::LeadingAndTrailingComments,
            Some(UnchangedTailNoopMixedDirtyOrdersNoiseMode::Unreadable),
            vec![
                Utf8PathBuf::from("sections/body-a.tex"),
                Utf8PathBuf::from("sections/body-b.tex"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        UnchangedTailNoopMixedDirtyOrdersBaselineFollowsCase::ReversedUntrackedFollows => (
            UnchangedTailNoopMixedDirtyOrdersEditMode::LeadingAndTrailingComments,
            Some(UnchangedTailNoopMixedDirtyOrdersNoiseMode::Untracked),
            vec![
                Utf8PathBuf::from("sections/body-b.tex"),
                Utf8PathBuf::from("sections/body-a.tex"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
        UnchangedTailNoopMixedDirtyOrdersBaselineFollowsCase::ReversedUnreadableFollows => (
            UnchangedTailNoopMixedDirtyOrdersEditMode::LeadingAndTrailingComments,
            Some(UnchangedTailNoopMixedDirtyOrdersNoiseMode::Unreadable),
            vec![
                Utf8PathBuf::from("sections/body-b.tex"),
                Utf8PathBuf::from("sections/body-a.tex"),
                Utf8PathBuf::from("notes.txt"),
            ],
        ),
    };

    run_unchanged_tail_noop_multi_input_mixed_dirty_orders_case(
        edit_mode,
        noise_mode,
        changed_files,
    )
    .await;
}

async fn run_unchanged_tail_noop_multi_input_mixed_dirty_orders_precedes(
    case: UnchangedTailNoopMixedDirtyOrdersPrecedesCase,
) {
    let (noise_mode, changed_files) = match case {
        UnchangedTailNoopMixedDirtyOrdersPrecedesCase::UntrackedPlain => (
            UnchangedTailNoopMixedDirtyOrdersNoiseMode::Untracked,
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("sections/body-a.tex"),
                Utf8PathBuf::from("sections/body-b.tex"),
            ],
        ),
        UnchangedTailNoopMixedDirtyOrdersPrecedesCase::UnreadablePlain => (
            UnchangedTailNoopMixedDirtyOrdersNoiseMode::Unreadable,
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("sections/body-a.tex"),
                Utf8PathBuf::from("sections/body-b.tex"),
            ],
        ),
        UnchangedTailNoopMixedDirtyOrdersPrecedesCase::UntrackedReversed => (
            UnchangedTailNoopMixedDirtyOrdersNoiseMode::Untracked,
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("sections/body-b.tex"),
                Utf8PathBuf::from("sections/body-a.tex"),
            ],
        ),
        UnchangedTailNoopMixedDirtyOrdersPrecedesCase::UnreadableReversed => (
            UnchangedTailNoopMixedDirtyOrdersNoiseMode::Unreadable,
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("sections/body-b.tex"),
                Utf8PathBuf::from("sections/body-a.tex"),
            ],
        ),
    };

    run_unchanged_tail_noop_multi_input_mixed_dirty_orders_case(
        UnchangedTailNoopMixedDirtyOrdersEditMode::LeadingAndTrailingComments,
        Some(noise_mode),
        changed_files,
    )
    .await;
}
