enum BibliographyCheckpointPreferenceToplevelCase {
    Baseline,
    Reversed,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

struct BibliographyCheckpointPreferenceToplevelFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    second_main: String,
    toplevel_checkpoint_id: String,
    toplevel_start_page_index: usize,
    preamble_checkpoint_id: String,
}

async fn prepare_bibliography_checkpoint_preference_toplevel_fixture()
-> BibliographyCheckpointPreferenceToplevelFixture {
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
    let original_prefix = (0..1152)
        .map(|index| format!("p{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    let original_suffix = (1152..1536)
        .map(|index| format!("s{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    let original_main = format!(
        "\\documentclass{{article}}\\begin{{document}}{} \\cite{{alpha}} {}\\bibliography{{refs}}\\end{{document}}",
        original_prefix, original_suffix
    );
    let edited_suffix = format!(
        "{} {}",
        (1152..1344)
            .map(|index| format!("s{index:04}"))
            .collect::<Vec<_>>()
            .join(" "),
        (1344..1536)
            .map(|index| format!("t{index:04}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let second_main = format!(
        "\\documentclass{{article}}\\begin{{document}}{} \\cite{{alpha}} {}\\bibliography{{refs}}\\end{{document}}",
        original_prefix, edited_suffix
    );
    fs::write(root.join("main.tex"), &original_main).expect("write main");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem{alpha} Alpha entry.\n\\end{thebibliography}\n",
    )
    .expect("write bbl");

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
            changed_files: vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")],
        })
        .await
        .expect("first semantic aux build should succeed");
    assert!(
        first.page_metadata.len() >= 2,
        "fixture should place the bibliography on a later page"
    );
    let first_checkpoints =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let bibliography_checkpoint_id = first_checkpoints
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("refs.bbl"))
        })
        .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
        .expect("refs.bbl input boundary");
    let diff_offset = original_main
        .bytes()
        .zip(second_main.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    let toplevel_start_page_index = first_checkpoints
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
            Some(offset_checkpoint.meta.page_index_after.min(span_start_page))
        })
        .expect("expected main checkpoint page");
    let toplevel_checkpoint_id = first_checkpoints
        .checkpoints
        .iter()
        .find(|checkpoint| checkpoint.meta.page_index_after == toplevel_start_page_index)
        .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
        .expect("expected main checkpoint");
    assert_ne!(
        toplevel_checkpoint_id, bibliography_checkpoint_id,
        "the main.tex replay checkpoint should be earlier than the bibliography input checkpoint"
    );

    BibliographyCheckpointPreferenceToplevelFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        second_main,
        toplevel_checkpoint_id,
        toplevel_start_page_index,
        preamble_checkpoint_id: first_checkpoints.checkpoints[0].meta.checkpoint_id.clone(),
    }
}

fn rewrite_bibliography_checkpoint_preference_toplevel(
    fixture: &BibliographyCheckpointPreferenceToplevelFixture,
) {
    fs::write(fixture.root.join("main.tex"), &fixture.second_main).expect("rewrite main");
    fs::write(
        fixture.root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem{alpha} Alpha  entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite bbl");
}

async fn compile_bibliography_checkpoint_preference_toplevel_second_pass(
    fixture: &BibliographyCheckpointPreferenceToplevelFixture,
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

async fn run_bibliography_checkpoint_preference_toplevel_replay(
    case: BibliographyCheckpointPreferenceToplevelCase,
) {
    let fixture = prepare_bibliography_checkpoint_preference_toplevel_fixture().await;
    rewrite_bibliography_checkpoint_preference_toplevel(&fixture);
    match case {
        BibliographyCheckpointPreferenceToplevelCase::UntrackedFollows
        | BibliographyCheckpointPreferenceToplevelCase::UntrackedPrecedes => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        BibliographyCheckpointPreferenceToplevelCase::UnreadableFollows
        | BibliographyCheckpointPreferenceToplevelCase::UnreadablePrecedes => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
        BibliographyCheckpointPreferenceToplevelCase::Baseline
        | BibliographyCheckpointPreferenceToplevelCase::Reversed => {}
    }
    let dirty_files = match case {
        BibliographyCheckpointPreferenceToplevelCase::Baseline => {
            vec![Utf8PathBuf::from("main.tex"), Utf8PathBuf::from("refs.bbl")]
        }
        BibliographyCheckpointPreferenceToplevelCase::Reversed => {
            vec![Utf8PathBuf::from("refs.bbl"), Utf8PathBuf::from("main.tex")]
        }
        BibliographyCheckpointPreferenceToplevelCase::UntrackedFollows
        | BibliographyCheckpointPreferenceToplevelCase::UnreadableFollows => vec![
            Utf8PathBuf::from("main.tex"),
            Utf8PathBuf::from("refs.bbl"),
            Utf8PathBuf::from("notes.txt"),
        ],
        BibliographyCheckpointPreferenceToplevelCase::UntrackedPrecedes
        | BibliographyCheckpointPreferenceToplevelCase::UnreadablePrecedes => vec![
            Utf8PathBuf::from("notes.txt"),
            Utf8PathBuf::from("main.tex"),
            Utf8PathBuf::from("refs.bbl"),
        ],
    };
    let second =
        compile_bibliography_checkpoint_preference_toplevel_second_pass(&fixture, &dirty_files)
            .await;
    let (expected_checkpoint_id, expected_start_page_index) = match case {
        BibliographyCheckpointPreferenceToplevelCase::Baseline
        | BibliographyCheckpointPreferenceToplevelCase::Reversed
        | BibliographyCheckpointPreferenceToplevelCase::UntrackedPrecedes => (
            fixture.toplevel_checkpoint_id.as_str(),
            fixture.toplevel_start_page_index,
        ),
        BibliographyCheckpointPreferenceToplevelCase::UntrackedFollows
        | BibliographyCheckpointPreferenceToplevelCase::UnreadableFollows
        | BibliographyCheckpointPreferenceToplevelCase::UnreadablePrecedes => {
            (fixture.preamble_checkpoint_id.as_str(), 0)
        }
    };
    assert_bibliography_checkpoint_preference_toplevel_replay(
        &fixture.build_root,
        &second,
        &dirty_files,
        expected_checkpoint_id,
        expected_start_page_index,
    );
}

type BibCpTopCase = BibliographyCheckpointPreferenceToplevelCase;

async fn run_bib_cp_top_case(case: BibCpTopCase) {
    run_bibliography_checkpoint_preference_toplevel_replay(case).await;
}

fn assert_bibliography_checkpoint_preference_toplevel_replay(
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
    expected_checkpoint_id: &str,
    expected_start_page_index: usize,
) {
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
    assert!(build_meta.semantic_fixpoint_reached);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(
        build_meta.rebuilt_page_count + build_meta.reused_page_count,
        build_meta.page_count
    );
    assert_eq!(build_meta.semantic_pass_count, 1);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_aux_backdated);
}
