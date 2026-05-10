struct BibliographyCheckpointPreferenceDirtyOnlyFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    first: CompileOutcome,
    preamble_checkpoint_id: String,
    bibliography_checkpoint_id: String,
    bibliography_start_page_index: usize,
}

enum BibliographyCheckpointPreferenceDirtyOnlyCase {
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

async fn prepare_bibliography_checkpoint_preference_dirty_only_fixture()
-> BibliographyCheckpointPreferenceDirtyOnlyFixture {
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
    let filler = "bibliography replay filler text ".repeat(220);
    fs::write(
        root.join("main.tex"),
        format!(
            "\\documentclass{{article}}\\begin{{document}}Cite \\cite{{alpha}}.\\section{{Intro}} {filler}\\bibliography{{refs}}\\end{{document}}"
        ),
    )
    .expect("write main");
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
    let first_bundle =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let bibliography_checkpoint = first_bundle
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("refs.bbl"))
        })
        .expect("refs.bbl input boundary");

    BibliographyCheckpointPreferenceDirtyOnlyFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        first,
        preamble_checkpoint_id: first_bundle.checkpoints[0].meta.checkpoint_id.clone(),
        bibliography_checkpoint_id: bibliography_checkpoint.meta.checkpoint_id.clone(),
        bibliography_start_page_index: bibliography_checkpoint.meta.page_index_after,
    }
}

fn rewrite_bibliography_checkpoint_preference_dirty_only(
    fixture: &BibliographyCheckpointPreferenceDirtyOnlyFixture,
) {
    fs::write(
        fixture.root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem{alpha} Alpha   entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite bbl");
}

fn write_bibliography_checkpoint_preference_dirty_only_untracked_noise(root: &Utf8Path) {
    fs::write(root.join("notes.txt"), "scratch notes").expect("write notes");
}

fn write_bibliography_checkpoint_preference_dirty_only_unreadable_noise(root: &Utf8Path) {
    fs::create_dir_all(root.join("notes.txt")).expect("create unreadable dirty dir");
}

async fn compile_bibliography_checkpoint_preference_dirty_only_second_pass(
    fixture: &BibliographyCheckpointPreferenceDirtyOnlyFixture,
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

fn assert_bibliography_checkpoint_preference_dirty_only_replay(
    fixture: &BibliographyCheckpointPreferenceDirtyOnlyFixture,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
    expected_checkpoint_id: &str,
    expected_start_page_index: usize,
) {
    assert_eq!(
        second.reused_checkpoint_id.as_deref(),
        Some(expected_checkpoint_id)
    );
    assert!(second.page_patches.is_empty());
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
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files.to_vec());
    assert_eq!(
        build_meta.start_checkpoint_id.as_deref(),
        Some(expected_checkpoint_id)
    );
    assert_eq!(build_meta.start_page_index, expected_start_page_index);
    assert_eq!(build_meta.page_count, second.page_metadata.len());
    assert_eq!(build_meta.rebuilt_page_count, 0);
    assert_eq!(build_meta.reused_page_count, build_meta.page_count);
    assert_eq!(build_meta.semantic_pass_count, 1);
    assert_eq!(build_meta.semantic_rerun_count, 0);
    assert!(build_meta.semantic_fixpoint_reached);
    assert!(build_meta.semantic_aux_backdated);
}

async fn run_bibliography_checkpoint_preference_dirty_only_replay(
    case: BibliographyCheckpointPreferenceDirtyOnlyCase,
) {
    let fixture = prepare_bibliography_checkpoint_preference_dirty_only_fixture().await;
    rewrite_bibliography_checkpoint_preference_dirty_only(&fixture);
    match case {
        BibliographyCheckpointPreferenceDirtyOnlyCase::UntrackedFollows
        | BibliographyCheckpointPreferenceDirtyOnlyCase::UntrackedPrecedes => {
            write_bibliography_checkpoint_preference_dirty_only_untracked_noise(&fixture.root);
        }
        BibliographyCheckpointPreferenceDirtyOnlyCase::UnreadableFollows
        | BibliographyCheckpointPreferenceDirtyOnlyCase::UnreadablePrecedes => {
            write_bibliography_checkpoint_preference_dirty_only_unreadable_noise(&fixture.root);
        }
    }
    let dirty_files = match case {
        BibliographyCheckpointPreferenceDirtyOnlyCase::UntrackedFollows
        | BibliographyCheckpointPreferenceDirtyOnlyCase::UnreadableFollows => {
            vec![
                Utf8PathBuf::from("refs.bbl"),
                Utf8PathBuf::from("notes.txt"),
            ]
        }
        BibliographyCheckpointPreferenceDirtyOnlyCase::UntrackedPrecedes
        | BibliographyCheckpointPreferenceDirtyOnlyCase::UnreadablePrecedes => {
            vec![
                Utf8PathBuf::from("notes.txt"),
                Utf8PathBuf::from("refs.bbl"),
            ]
        }
    };
    let second =
        compile_bibliography_checkpoint_preference_dirty_only_second_pass(&fixture, &dirty_files)
            .await;
    let (expected_checkpoint_id, expected_start_page_index) = match case {
        BibliographyCheckpointPreferenceDirtyOnlyCase::UntrackedPrecedes => (
            fixture.bibliography_checkpoint_id.as_str(),
            fixture.bibliography_start_page_index,
        ),
        BibliographyCheckpointPreferenceDirtyOnlyCase::UntrackedFollows
        | BibliographyCheckpointPreferenceDirtyOnlyCase::UnreadableFollows
        | BibliographyCheckpointPreferenceDirtyOnlyCase::UnreadablePrecedes => {
            (fixture.preamble_checkpoint_id.as_str(), 0)
        }
    };
    assert_bibliography_checkpoint_preference_dirty_only_replay(
        &fixture,
        &second,
        &dirty_files,
        expected_checkpoint_id,
        expected_start_page_index,
    );
}

type BibCpDirtyCase = BibliographyCheckpointPreferenceDirtyOnlyCase;

async fn run_bib_cp_dirty_case(case: BibCpDirtyCase) {
    run_bibliography_checkpoint_preference_dirty_only_replay(case).await;
}
