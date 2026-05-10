struct SamePageIncludedBodyCheckpointPreferenceFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    expected_checkpoint_id: String,
    expected_start_page_index: usize,
    preamble_checkpoint_id: String,
}

async fn prepare_same_page_included_body_checkpoint_preference_fixture()
-> SamePageIncludedBodyCheckpointPreferenceFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let body_filler = "same page bibliography replay filler ".repeat(24);
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
        "\\documentclass{article}\\begin{document}\\input{sections/body}\\input{sections/appendix}\\bibliography{refsa,refsb}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("sections/body.tex"),
        format!("Cite \\cite{{alpha}} and \\cite{{beta}}. {body_filler}"),
    )
    .expect("write body");
    fs::write(root.join("sections/appendix.tex"), "Appendix A.").expect("write appendix");
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
    let _first = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 1,
            build_root: build_root.clone(),
            changed_files: vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("sections/body.tex"),
                Utf8PathBuf::from("sections/appendix.tex"),
                Utf8PathBuf::from("refsa.bbl"),
                Utf8PathBuf::from("refsb.bbl"),
            ],
        })
        .await
        .expect("first semantic aux build should succeed");
    let first_bundle =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let expected_checkpoint = first_bundle
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.module_path.as_ref()
                    == Some(&Utf8PathBuf::from("sections/appendix.tex"))
        })
        .expect("appendix input boundary");
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
    assert_eq!(
        expected_checkpoint.meta.page_index_after,
        bibliography_checkpoint.meta.page_index_after
    );
    assert!(
        expected_checkpoint.meta.output_start_utf8
            <= bibliography_checkpoint.meta.output_start_utf8
    );

    SamePageIncludedBodyCheckpointPreferenceFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        expected_checkpoint_id: expected_checkpoint.meta.checkpoint_id.clone(),
        expected_start_page_index: expected_checkpoint.meta.page_index_after,
        preamble_checkpoint_id: first_bundle.checkpoints[0].meta.checkpoint_id.clone(),
    }
}

fn rewrite_same_page_included_body_checkpoint_preference(
    fixture: &SamePageIncludedBodyCheckpointPreferenceFixture,
) {
    fs::write(fixture.root.join("sections/appendix.tex"), "Appendix B.").expect("rewrite appendix");
    fs::write(
        fixture.root.join("refsa.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[A 2024]{alpha}  Alpha entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite first bibliography");
    fs::write(
        fixture.root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta}  Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite second bibliography");
}

async fn compile_same_page_included_body_checkpoint_preference_second_pass(
    fixture: &SamePageIncludedBodyCheckpointPreferenceFixture,
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

fn assert_same_page_included_body_checkpoint_preference_replay(
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
    expected_checkpoint_id: &str,
    expected_start_page_index: usize,
) {
    let second_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-2/sources.json")).expect("read second sources"),
    )
    .expect("parse second sources");
    assert!(
        second_sources.executed_files[&Utf8PathBuf::from("sections/appendix.tex")]
            .contains("Appendix B."),
        "executed appendix.tex should reflect the earlier tracked change"
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
    assert_eq!(build_meta.start_page_index, expected_start_page_index);
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

#[derive(Clone, Copy)]
enum SamePageIncludedBodyCheckpointPreferenceExtraDirtyKind {
    NoExtraDirty,
    Untracked,
    Unreadable,
}

#[derive(Clone, Copy)]
enum SamePageIncludedBodyCheckpointPreferenceExpectedReplay {
    EarlierInput,
    Preamble,
}

enum SamePageIncludedBodyCheckpointPreferenceDirtyNoiseCase {
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

enum SamePageIncludedBodyCheckpointPreferenceBaselineCase {
    Baseline,
    Reversed,
}

async fn run_same_page_included_body_checkpoint_preference_case(
    extra_dirty: SamePageIncludedBodyCheckpointPreferenceExtraDirtyKind,
    dirty_files: &[&str],
    expected_replay: SamePageIncludedBodyCheckpointPreferenceExpectedReplay,
) {
    let fixture = prepare_same_page_included_body_checkpoint_preference_fixture().await;
    rewrite_same_page_included_body_checkpoint_preference(&fixture);
    match extra_dirty {
        SamePageIncludedBodyCheckpointPreferenceExtraDirtyKind::NoExtraDirty => {}
        SamePageIncludedBodyCheckpointPreferenceExtraDirtyKind::Untracked => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        SamePageIncludedBodyCheckpointPreferenceExtraDirtyKind::Unreadable => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }

    let dirty_files = dirty_files
        .iter()
        .map(|dirty_file| Utf8PathBuf::from(*dirty_file))
        .collect::<Vec<_>>();
    let second =
        compile_same_page_included_body_checkpoint_preference_second_pass(&fixture, &dirty_files)
            .await;
    let (expected_checkpoint_id, expected_start_page_index) = match expected_replay {
        SamePageIncludedBodyCheckpointPreferenceExpectedReplay::EarlierInput => (
            fixture.expected_checkpoint_id.as_str(),
            fixture.expected_start_page_index,
        ),
        SamePageIncludedBodyCheckpointPreferenceExpectedReplay::Preamble => {
            (fixture.preamble_checkpoint_id.as_str(), 0)
        }
    };

    assert_same_page_included_body_checkpoint_preference_replay(
        &fixture.build_root,
        &second,
        &dirty_files,
        expected_checkpoint_id,
        expected_start_page_index,
    );
}

async fn run_same_page_included_body_checkpoint_preference_dirty_noise_case(
    case: SamePageIncludedBodyCheckpointPreferenceDirtyNoiseCase,
) {
    let (extra_dirty, dirty_files, expected_replay) = match case {
        SamePageIncludedBodyCheckpointPreferenceDirtyNoiseCase::UntrackedFollows => (
            SamePageIncludedBodyCheckpointPreferenceExtraDirtyKind::Untracked,
            vec![
                "sections/appendix.tex",
                "refsb.bbl",
                "refsa.bbl",
                "notes.txt",
            ],
            SamePageIncludedBodyCheckpointPreferenceExpectedReplay::Preamble,
        ),
        SamePageIncludedBodyCheckpointPreferenceDirtyNoiseCase::UntrackedPrecedes => (
            SamePageIncludedBodyCheckpointPreferenceExtraDirtyKind::Untracked,
            vec![
                "notes.txt",
                "sections/appendix.tex",
                "refsb.bbl",
                "refsa.bbl",
            ],
            SamePageIncludedBodyCheckpointPreferenceExpectedReplay::EarlierInput,
        ),
        SamePageIncludedBodyCheckpointPreferenceDirtyNoiseCase::UnreadableFollows => (
            SamePageIncludedBodyCheckpointPreferenceExtraDirtyKind::Unreadable,
            vec![
                "sections/appendix.tex",
                "refsb.bbl",
                "refsa.bbl",
                "notes.txt",
            ],
            SamePageIncludedBodyCheckpointPreferenceExpectedReplay::Preamble,
        ),
        SamePageIncludedBodyCheckpointPreferenceDirtyNoiseCase::UnreadablePrecedes => (
            SamePageIncludedBodyCheckpointPreferenceExtraDirtyKind::Unreadable,
            vec![
                "notes.txt",
                "sections/appendix.tex",
                "refsb.bbl",
                "refsa.bbl",
            ],
            SamePageIncludedBodyCheckpointPreferenceExpectedReplay::Preamble,
        ),
    };
    run_same_page_included_body_checkpoint_preference_case(
        extra_dirty,
        &dirty_files,
        expected_replay,
    )
    .await;
}

async fn run_same_page_included_body_checkpoint_preference_baseline_case(
    case: SamePageIncludedBodyCheckpointPreferenceBaselineCase,
) {
    let dirty_files = match case {
        SamePageIncludedBodyCheckpointPreferenceBaselineCase::Baseline => {
            ["sections/appendix.tex", "refsb.bbl", "refsa.bbl"]
        }
        SamePageIncludedBodyCheckpointPreferenceBaselineCase::Reversed => {
            ["refsb.bbl", "refsa.bbl", "sections/appendix.tex"]
        }
    };
    run_same_page_included_body_checkpoint_preference_case(
        SamePageIncludedBodyCheckpointPreferenceExtraDirtyKind::NoExtraDirty,
        &dirty_files,
        SamePageIncludedBodyCheckpointPreferenceExpectedReplay::EarlierInput,
    )
    .await;
}

type BodyCpBase = SamePageIncludedBodyCheckpointPreferenceBaselineCase;
type BodyCpNoise = SamePageIncludedBodyCheckpointPreferenceDirtyNoiseCase;

async fn run_body_cp_base(case: BodyCpBase) {
    run_same_page_included_body_checkpoint_preference_baseline_case(case).await;
}

async fn run_body_cp_noise(case: BodyCpNoise) {
    run_same_page_included_body_checkpoint_preference_dirty_noise_case(case).await;
}
