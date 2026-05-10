struct ReplaySelectionLateToplevelFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    late_edit_source: String,
    preamble_checkpoint_id: String,
    expected_shipout_checkpoint_id: String,
    expected_shipout_start_page_index: usize,
}

enum ReplaySelectionLateToplevelNoiseCase {
    NoNoiseShipout,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

type LateTopNoiseCase = ReplaySelectionLateToplevelNoiseCase;

async fn run_late_top_noise_case(case: LateTopNoiseCase) {
    run_replay_selection_late_toplevel_noise(case).await;
}

async fn prepare_replay_selection_late_toplevel_fixture() -> ReplaySelectionLateToplevelFixture {
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
    let original_body = (0..1536)
        .map(|index| format!("w{index:07}"))
        .collect::<Vec<_>>()
        .join(" ");
    let original_source = format!(
        "\\documentclass{{article}}\\begin{{document}}\n{}\n\\end{{document}}",
        original_body
    );
    fs::write(root.join("main.tex"), &original_source).expect("write main tex");

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
            changed_files: vec![Utf8PathBuf::from("main.tex")],
        })
        .await
        .expect("first build should succeed");
    assert_eq!(first.page_metadata.len(), 4);
    let first_checkpoints = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load rev1 checkpoints");
    assert!(
        first_checkpoints.checkpoints[1..]
            .iter()
            .all(|checkpoint| checkpoint.snapshot.is_some())
    );
    assert!(
        first_checkpoints.checkpoints[1..]
            .windows(2)
            .all(|window| window[0].meta.source_offset_utf8 <= window[1].meta.source_offset_utf8)
    );

    let late_edit_body = format!(
        "{} {}",
        (0..1152)
            .map(|index| format!("w{index:07}"))
            .collect::<Vec<_>>()
            .join(" "),
        (1152..1536)
            .map(|index| format!("z{index:07}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let late_edit_source = format!(
        "\\documentclass{{article}}\\begin{{document}}\n{}\n\\end{{document}}",
        late_edit_body
    );
    let diff_offset = original_source
        .bytes()
        .zip(late_edit_source.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    let expected_shipout = first_checkpoints
        .checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.meta.page_index_after > 0)
        .take_while(|checkpoint| checkpoint.meta.source_offset_utf8 <= diff_offset as u32)
        .last()
        .and_then(|offset_checkpoint| {
            let span_start_page = first.page_metadata.iter().find_map(|page| {
                page.source_spans
                    .iter()
                    .find(|span| span.file == Utf8PathBuf::from("main.tex"))
                    .and_then(|span| {
                        if (diff_offset as u32) < span.end_utf8 {
                            Some(page.index)
                        } else {
                            None
                        }
                    })
            })?;
            let expected_page_index_after =
                offset_checkpoint.meta.page_index_after.min(span_start_page);
            first_checkpoints
                .checkpoints
                .iter()
                .find(|checkpoint| checkpoint.meta.page_index_after == expected_page_index_after)
                .map(|checkpoint| {
                    (
                        checkpoint.meta.checkpoint_id.clone(),
                        checkpoint.meta.page_index_after,
                    )
                })
        })
        .expect("expected shipout checkpoint");

    ReplaySelectionLateToplevelFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        late_edit_source,
        preamble_checkpoint_id: first_checkpoints.checkpoints[0].meta.checkpoint_id.clone(),
        expected_shipout_checkpoint_id: expected_shipout.0,
        expected_shipout_start_page_index: expected_shipout.1,
    }
}

fn rewrite_replay_selection_late_toplevel(fixture: &ReplaySelectionLateToplevelFixture) {
    fs::write(fixture.root.join("main.tex"), &fixture.late_edit_source).expect("rewrite main tex");
}

async fn compile_replay_selection_late_toplevel_second_pass(
    fixture: &ReplaySelectionLateToplevelFixture,
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

fn assert_replay_selection_late_toplevel(
    fixture: &ReplaySelectionLateToplevelFixture,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
    expected_checkpoint_id: &str,
    expected_start_page_index: usize,
    require_mixed_reuse: bool,
) {
    assert_eq!(
        second.reused_checkpoint_id.as_deref(),
        Some(expected_checkpoint_id)
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(fixture.build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(!build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files.to_vec());
    assert_eq!(
        build_meta.start_checkpoint_id.as_deref(),
        Some(expected_checkpoint_id)
    );
    assert_eq!(build_meta.start_page_index, expected_start_page_index);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    if require_mixed_reuse {
        assert!(build_meta.reused_page_count >= 1);
        assert!(build_meta.rebuilt_page_count >= 1);
    } else {
        assert_eq!(
            build_meta.rebuilt_page_count + build_meta.reused_page_count,
            build_meta.page_count
        );
    }
    assert_eq!(build_meta.semantic_pass_count, 0);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(!build_meta.semantic_aux_backdated);
}

async fn run_replay_selection_late_toplevel_noise(case: ReplaySelectionLateToplevelNoiseCase) {
    let fixture = prepare_replay_selection_late_toplevel_fixture().await;
    rewrite_replay_selection_late_toplevel(&fixture);
    match case {
        ReplaySelectionLateToplevelNoiseCase::NoNoiseShipout => {}
        ReplaySelectionLateToplevelNoiseCase::UntrackedFollows
        | ReplaySelectionLateToplevelNoiseCase::UntrackedPrecedes => {
            fs::write(fixture.root.join("notes.txt"), "fresh scratch notes").expect("write notes");
        }
        ReplaySelectionLateToplevelNoiseCase::UnreadableFollows
        | ReplaySelectionLateToplevelNoiseCase::UnreadablePrecedes => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }
    let dirty_files = match case {
        ReplaySelectionLateToplevelNoiseCase::NoNoiseShipout => {
            vec![Utf8PathBuf::from("main.tex")]
        }
        ReplaySelectionLateToplevelNoiseCase::UntrackedFollows
        | ReplaySelectionLateToplevelNoiseCase::UnreadableFollows => {
            vec![
                Utf8PathBuf::from("main.tex"),
                Utf8PathBuf::from("notes.txt"),
            ]
        }
        ReplaySelectionLateToplevelNoiseCase::UntrackedPrecedes
        | ReplaySelectionLateToplevelNoiseCase::UnreadablePrecedes => {
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("main.tex"),
            ]
        }
    };
    let second = compile_replay_selection_late_toplevel_second_pass(&fixture, &dirty_files).await;
    let (expected_checkpoint_id, expected_start_page_index, require_mixed_reuse) = match case {
        ReplaySelectionLateToplevelNoiseCase::NoNoiseShipout => (
            fixture.expected_shipout_checkpoint_id.as_str(),
            fixture.expected_shipout_start_page_index,
            true,
        ),
        ReplaySelectionLateToplevelNoiseCase::UntrackedPrecedes => (
            fixture.expected_shipout_checkpoint_id.as_str(),
            fixture.expected_shipout_start_page_index,
            true,
        ),
        ReplaySelectionLateToplevelNoiseCase::UntrackedFollows
        | ReplaySelectionLateToplevelNoiseCase::UnreadableFollows
        | ReplaySelectionLateToplevelNoiseCase::UnreadablePrecedes => {
            (fixture.preamble_checkpoint_id.as_str(), 0, false)
        }
    };
    assert_replay_selection_late_toplevel(
        &fixture,
        &second,
        &dirty_files,
        expected_checkpoint_id,
        expected_start_page_index,
        require_mixed_reuse,
    );
    if matches!(case, ReplaySelectionLateToplevelNoiseCase::NoNoiseShipout) {
        assert!(
            second.page_artifacts[0]
                .pdf_url
                .starts_with("/artifacts/rev/1/pages/")
        );
        assert!(
            second
                .page_artifacts
                .last()
                .expect("last page artifact")
                .pdf_url
                .starts_with("/artifacts/rev/2/pages/")
        );
        assert!(second.page_patches.iter().any(|patch| matches!(
            patch,
            PagePatchOp::ReplacePage { index, .. } if *index >= 2
        )));
        assert!(fixture.build_root.join("rev-2/output.txt").exists());
        assert!(fixture.build_root.join("rev-2/sources.json").exists());
    }
}
