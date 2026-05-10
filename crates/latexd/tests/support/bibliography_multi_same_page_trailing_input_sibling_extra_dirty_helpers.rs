enum SamePageTrailingInputSiblingExtraDirtyNoiseKind {
    Untracked,
    Unreadable,
}

enum SamePageTrailingInputSiblingExtraDirtyReplayStart {
    Preamble,
    Bibliography,
}

enum SamePageTrailingInputSiblingExtraDirtyCase {
    UntrackedDirtyFollows,
    UntrackedDirtyPrecedes,
    UntrackedInterleavedFollows,
    UntrackedInterleavedPrecedes,
    UntrackedOtherInterleavedFollows,
    UntrackedOtherInterleavedPrecedes,
    UnreadableBaselineFollows,
    UnreadableBaselinePrecedes,
    UnreadableInterleavedFollows,
    UnreadableInterleavedPrecedes,
    UnreadableOtherInterleavedFollows,
    UnreadableOtherInterleavedPrecedes,
}

type SiblingExtraCase = SamePageTrailingInputSiblingExtraDirtyCase;

async fn run_sibling_extra_case(case: SiblingExtraCase) {
    run_same_page_trailing_input_sibling_extra_dirty_case(case).await;
}

struct SamePageTrailingInputSiblingExtraDirtyFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    preamble_checkpoint_id: String,
    preamble_page_index_after: usize,
    bibliography_checkpoint_id: String,
    bibliography_page_index_after: usize,
}

async fn prepare_same_page_trailing_input_sibling_extra_dirty_fixture()
-> SamePageTrailingInputSiblingExtraDirtyFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let filler = (0..160)
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
        format!(
            "\\documentclass{{article}}\\begin{{document}} {filler} \\cite{{alpha}} and \\cite{{beta}}.\\bibliography{{refsa,refsb}}\\input{{sections/tail}}\\end{{document}}"
        ),
    )
    .expect("write main");
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
        "fixture should keep both bibliography files and tail on the same page"
    );
    let first_bundle =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let preamble_checkpoint = first_bundle
        .checkpoints
        .first()
        .expect("preamble checkpoint");
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

    SamePageTrailingInputSiblingExtraDirtyFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        preamble_checkpoint_id: preamble_checkpoint.meta.checkpoint_id.clone(),
        preamble_page_index_after: preamble_checkpoint.meta.page_index_after,
        bibliography_checkpoint_id: bibliography_checkpoint.meta.checkpoint_id.clone(),
        bibliography_page_index_after: bibliography_checkpoint.meta.page_index_after,
    }
}

fn rewrite_same_page_trailing_input_sibling_extra_dirty(
    fixture: &SamePageTrailingInputSiblingExtraDirtyFixture,
    noise_kind: SamePageTrailingInputSiblingExtraDirtyNoiseKind,
) {
    fs::write(fixture.root.join("sections/tail.tex"), "Tail B.").expect("rewrite tail");
    fs::write(
        fixture.root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta}  Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite second bibliography");

    match noise_kind {
        SamePageTrailingInputSiblingExtraDirtyNoiseKind::Untracked => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        SamePageTrailingInputSiblingExtraDirtyNoiseKind::Unreadable => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }
}

async fn compile_same_page_trailing_input_sibling_extra_dirty_second_pass(
    fixture: &SamePageTrailingInputSiblingExtraDirtyFixture,
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

async fn run_same_page_trailing_input_sibling_extra_dirty_replay(
    noise_kind: SamePageTrailingInputSiblingExtraDirtyNoiseKind,
    dirty_files: &[&str],
    replay_start: SamePageTrailingInputSiblingExtraDirtyReplayStart,
) {
    let fixture = prepare_same_page_trailing_input_sibling_extra_dirty_fixture().await;
    rewrite_same_page_trailing_input_sibling_extra_dirty(&fixture, noise_kind);

    let dirty_files = dirty_files
        .iter()
        .map(|dirty_file| Utf8PathBuf::from(*dirty_file))
        .collect::<Vec<_>>();
    let second =
        compile_same_page_trailing_input_sibling_extra_dirty_second_pass(&fixture, &dirty_files)
            .await;

    let (expected_checkpoint_id, expected_page_index) = match replay_start {
        SamePageTrailingInputSiblingExtraDirtyReplayStart::Preamble => (
            fixture.preamble_checkpoint_id.as_str(),
            fixture.preamble_page_index_after,
        ),
        SamePageTrailingInputSiblingExtraDirtyReplayStart::Bibliography => (
            fixture.bibliography_checkpoint_id.as_str(),
            fixture.bibliography_page_index_after,
        ),
    };
    assert_same_page_trailing_input_sibling_extra_dirty_replay(
        &fixture.build_root,
        &second,
        &dirty_files,
        expected_checkpoint_id,
        expected_page_index,
    );
}

async fn run_same_page_trailing_input_sibling_extra_dirty_case(
    case: SamePageTrailingInputSiblingExtraDirtyCase,
) {
    let (noise_kind, dirty_files, replay_start) = match case {
        SamePageTrailingInputSiblingExtraDirtyCase::UntrackedDirtyFollows => (
            SamePageTrailingInputSiblingExtraDirtyNoiseKind::Untracked,
            &["refsb.bbl", "sections/tail.tex", "refsa.bbl", "notes.txt"][..],
            SamePageTrailingInputSiblingExtraDirtyReplayStart::Preamble,
        ),
        SamePageTrailingInputSiblingExtraDirtyCase::UntrackedDirtyPrecedes => (
            SamePageTrailingInputSiblingExtraDirtyNoiseKind::Untracked,
            &["notes.txt", "sections/tail.tex", "refsa.bbl", "refsb.bbl"][..],
            SamePageTrailingInputSiblingExtraDirtyReplayStart::Bibliography,
        ),
        SamePageTrailingInputSiblingExtraDirtyCase::UntrackedInterleavedFollows => (
            SamePageTrailingInputSiblingExtraDirtyNoiseKind::Untracked,
            &["sections/tail.tex", "refsb.bbl", "refsa.bbl", "notes.txt"][..],
            SamePageTrailingInputSiblingExtraDirtyReplayStart::Preamble,
        ),
        SamePageTrailingInputSiblingExtraDirtyCase::UntrackedInterleavedPrecedes => (
            SamePageTrailingInputSiblingExtraDirtyNoiseKind::Untracked,
            &["notes.txt", "sections/tail.tex", "refsa.bbl", "refsb.bbl"][..],
            SamePageTrailingInputSiblingExtraDirtyReplayStart::Bibliography,
        ),
        SamePageTrailingInputSiblingExtraDirtyCase::UntrackedOtherInterleavedFollows => (
            SamePageTrailingInputSiblingExtraDirtyNoiseKind::Untracked,
            &["sections/tail.tex", "refsa.bbl", "refsb.bbl", "notes.txt"][..],
            SamePageTrailingInputSiblingExtraDirtyReplayStart::Preamble,
        ),
        SamePageTrailingInputSiblingExtraDirtyCase::UntrackedOtherInterleavedPrecedes => (
            SamePageTrailingInputSiblingExtraDirtyNoiseKind::Untracked,
            &["notes.txt", "sections/tail.tex", "refsa.bbl", "refsb.bbl"][..],
            SamePageTrailingInputSiblingExtraDirtyReplayStart::Bibliography,
        ),
        SamePageTrailingInputSiblingExtraDirtyCase::UnreadableBaselineFollows => (
            SamePageTrailingInputSiblingExtraDirtyNoiseKind::Unreadable,
            &["refsb.bbl", "sections/tail.tex", "refsa.bbl", "notes.txt"][..],
            SamePageTrailingInputSiblingExtraDirtyReplayStart::Preamble,
        ),
        SamePageTrailingInputSiblingExtraDirtyCase::UnreadableBaselinePrecedes => (
            SamePageTrailingInputSiblingExtraDirtyNoiseKind::Unreadable,
            &["notes.txt", "sections/tail.tex", "refsa.bbl", "refsb.bbl"][..],
            SamePageTrailingInputSiblingExtraDirtyReplayStart::Preamble,
        ),
        SamePageTrailingInputSiblingExtraDirtyCase::UnreadableInterleavedFollows => (
            SamePageTrailingInputSiblingExtraDirtyNoiseKind::Unreadable,
            &["sections/tail.tex", "refsb.bbl", "refsa.bbl", "notes.txt"][..],
            SamePageTrailingInputSiblingExtraDirtyReplayStart::Preamble,
        ),
        SamePageTrailingInputSiblingExtraDirtyCase::UnreadableInterleavedPrecedes => (
            SamePageTrailingInputSiblingExtraDirtyNoiseKind::Unreadable,
            &["notes.txt", "sections/tail.tex", "refsa.bbl", "refsb.bbl"][..],
            SamePageTrailingInputSiblingExtraDirtyReplayStart::Preamble,
        ),
        SamePageTrailingInputSiblingExtraDirtyCase::UnreadableOtherInterleavedFollows => (
            SamePageTrailingInputSiblingExtraDirtyNoiseKind::Unreadable,
            &["sections/tail.tex", "refsa.bbl", "refsb.bbl", "notes.txt"][..],
            SamePageTrailingInputSiblingExtraDirtyReplayStart::Preamble,
        ),
        SamePageTrailingInputSiblingExtraDirtyCase::UnreadableOtherInterleavedPrecedes => (
            SamePageTrailingInputSiblingExtraDirtyNoiseKind::Unreadable,
            &["notes.txt", "sections/tail.tex", "refsa.bbl", "refsb.bbl"][..],
            SamePageTrailingInputSiblingExtraDirtyReplayStart::Preamble,
        ),
    };
    run_same_page_trailing_input_sibling_extra_dirty_replay(noise_kind, dirty_files, replay_start)
        .await;
}

fn assert_same_page_trailing_input_sibling_extra_dirty_replay(
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
