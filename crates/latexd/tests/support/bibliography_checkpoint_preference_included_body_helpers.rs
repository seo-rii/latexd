enum BibliographyCheckpointPreferenceIncludedBodyCase {
    Baseline,
    Reversed,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

struct BibliographyCheckpointPreferenceIncludedBodyFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    second_appendix: String,
    second_bbl: String,
    appendix_checkpoint_id: String,
    appendix_start_page_index: usize,
    preamble_checkpoint_id: String,
}

async fn prepare_bibliography_checkpoint_preference_included_body_fixture()
-> BibliographyCheckpointPreferenceIncludedBodyFixture {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let body_filler = "bibliography replay filler text ".repeat(220);
    let appendix_filler = "appendix trailing filler text ".repeat(180);
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
        "\\documentclass{article}\\begin{document}\\input{sections/body}\\input{sections/appendix}\\bibliography{refs}\\end{document}",
    )
    .expect("write main");
    fs::write(
        root.join("sections/body.tex"),
        format!("Cite \\cite{{alpha}}.\\section{{Intro}} {body_filler}"),
    )
    .expect("write body");
    fs::write(
        root.join("sections/appendix.tex"),
        format!("Appendix A. {appendix_filler}"),
    )
    .expect("write appendix");
    fs::write(
        root.join("refs.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem{alpha} Alpha entry.\n\\end{thebibliography}\n",
    )
    .expect("write bbl");

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
                Utf8PathBuf::from("refs.bbl"),
            ],
        })
        .await
        .expect("first semantic aux build should succeed");
    let first_bundle =
        load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json")).expect("load bundle");
    let appendix_checkpoint = first_bundle
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
        .find(|checkpoint| {
            checkpoint.meta.kind == CheckpointKind::InputBoundary
                && checkpoint.meta.module_path.as_ref() == Some(&Utf8PathBuf::from("refs.bbl"))
        })
        .expect("refs.bbl input boundary");
    assert!(
        appendix_checkpoint.meta.page_index_after <= bibliography_checkpoint.meta.page_index_after,
        "appendix input boundary should not be later than refs.bbl replay boundary"
    );

    BibliographyCheckpointPreferenceIncludedBodyFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        second_appendix: format!("Appendix B. {appendix_filler}"),
        second_bbl:
            "\\begin{thebibliography}{1}\n\\bibitem{alpha} Alpha   entry.\n\\end{thebibliography}\n"
                .to_string(),
        appendix_checkpoint_id: appendix_checkpoint.meta.checkpoint_id.clone(),
        appendix_start_page_index: appendix_checkpoint.meta.page_index_after,
        preamble_checkpoint_id: first_bundle.checkpoints[0].meta.checkpoint_id.clone(),
    }
}

fn rewrite_bibliography_checkpoint_preference_included_body(
    fixture: &BibliographyCheckpointPreferenceIncludedBodyFixture,
) {
    fs::write(
        fixture.root.join("sections/appendix.tex"),
        &fixture.second_appendix,
    )
    .expect("rewrite appendix");
    fs::write(fixture.root.join("refs.bbl"), &fixture.second_bbl).expect("rewrite bbl");
}

fn write_bibliography_checkpoint_preference_included_body_untracked_noise(root: &Utf8Path) {
    fs::write(root.join("notes.txt"), "scratch notes").expect("write notes");
}

fn write_bibliography_checkpoint_preference_included_body_unreadable_noise(root: &Utf8Path) {
    fs::create_dir_all(root.join("notes.txt")).expect("create unreadable dirty dir");
}

async fn compile_bibliography_checkpoint_preference_included_body_second_pass(
    fixture: &BibliographyCheckpointPreferenceIncludedBodyFixture,
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

async fn run_bibliography_checkpoint_preference_included_body_replay(
    case: BibliographyCheckpointPreferenceIncludedBodyCase,
) {
    let fixture = prepare_bibliography_checkpoint_preference_included_body_fixture().await;
    rewrite_bibliography_checkpoint_preference_included_body(&fixture);
    match case {
        BibliographyCheckpointPreferenceIncludedBodyCase::UntrackedFollows
        | BibliographyCheckpointPreferenceIncludedBodyCase::UntrackedPrecedes => {
            write_bibliography_checkpoint_preference_included_body_untracked_noise(&fixture.root);
        }
        BibliographyCheckpointPreferenceIncludedBodyCase::UnreadableFollows
        | BibliographyCheckpointPreferenceIncludedBodyCase::UnreadablePrecedes => {
            write_bibliography_checkpoint_preference_included_body_unreadable_noise(&fixture.root);
        }
        BibliographyCheckpointPreferenceIncludedBodyCase::Baseline
        | BibliographyCheckpointPreferenceIncludedBodyCase::Reversed => {}
    }
    let dirty_files = match case {
        BibliographyCheckpointPreferenceIncludedBodyCase::Baseline => vec![
            Utf8PathBuf::from("sections/appendix.tex"),
            Utf8PathBuf::from("refs.bbl"),
        ],
        BibliographyCheckpointPreferenceIncludedBodyCase::Reversed => vec![
            Utf8PathBuf::from("refs.bbl"),
            Utf8PathBuf::from("sections/appendix.tex"),
        ],
        BibliographyCheckpointPreferenceIncludedBodyCase::UntrackedFollows
        | BibliographyCheckpointPreferenceIncludedBodyCase::UnreadableFollows => vec![
            Utf8PathBuf::from("sections/appendix.tex"),
            Utf8PathBuf::from("refs.bbl"),
            Utf8PathBuf::from("notes.txt"),
        ],
        BibliographyCheckpointPreferenceIncludedBodyCase::UntrackedPrecedes
        | BibliographyCheckpointPreferenceIncludedBodyCase::UnreadablePrecedes => vec![
            Utf8PathBuf::from("notes.txt"),
            Utf8PathBuf::from("sections/appendix.tex"),
            Utf8PathBuf::from("refs.bbl"),
        ],
    };
    let second = compile_bibliography_checkpoint_preference_included_body_second_pass(
        &fixture,
        &dirty_files,
    )
    .await;
    let (expected_checkpoint_id, expected_start_page_index) = match case {
        BibliographyCheckpointPreferenceIncludedBodyCase::Baseline
        | BibliographyCheckpointPreferenceIncludedBodyCase::Reversed
        | BibliographyCheckpointPreferenceIncludedBodyCase::UntrackedPrecedes => (
            fixture.appendix_checkpoint_id.as_str(),
            fixture.appendix_start_page_index,
        ),
        BibliographyCheckpointPreferenceIncludedBodyCase::UntrackedFollows
        | BibliographyCheckpointPreferenceIncludedBodyCase::UnreadableFollows
        | BibliographyCheckpointPreferenceIncludedBodyCase::UnreadablePrecedes => {
            (fixture.preamble_checkpoint_id.as_str(), 0)
        }
    };
    assert_bibliography_checkpoint_preference_included_body_replay(
        &fixture.build_root,
        &second,
        &dirty_files,
        expected_checkpoint_id,
        expected_start_page_index,
    );
}

type BibCpBodyCase = BibliographyCheckpointPreferenceIncludedBodyCase;

async fn run_bib_cp_body_case(case: BibCpBodyCase) {
    run_bibliography_checkpoint_preference_included_body_replay(case).await;
}

fn assert_bibliography_checkpoint_preference_included_body_replay(
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
