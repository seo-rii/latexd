struct SamePageToplevelCheckpointPreferenceFixture {
    _tempdir: tempfile::TempDir,
    root: Utf8PathBuf,
    build_root: Utf8PathBuf,
    world: ProjectWorld,
    driver: CompilerDriver,
    original_main: String,
    preamble_checkpoint_id: String,
}

async fn prepare_same_page_toplevel_checkpoint_preference_fixture()
-> SamePageToplevelCheckpointPreferenceFixture {
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
    let filler = (0..180)
        .map(|index| format!("body{index:04}"))
        .collect::<Vec<_>>()
        .join(" ");
    let original_main = format!(
        "\\documentclass{{article}}\\begin{{document}} {filler} \\cite{{alpha}} and \\cite{{beta}}.\\bibliography{{refsa,refsb}}\\end{{document}}"
    );
    fs::write(root.join("main.tex"), &original_main).expect("write main");
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
            ],
        })
        .await
        .expect("first semantic aux build should succeed");
    assert_eq!(
        first.page_metadata.len(),
        1,
        "fixture should keep both bibliography files on the same page"
    );
    let preamble_checkpoint_id = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load bundle")
        .checkpoints[0]
        .meta
        .checkpoint_id
        .clone();

    SamePageToplevelCheckpointPreferenceFixture {
        _tempdir: tempdir,
        root,
        build_root,
        world,
        driver,
        original_main,
        preamble_checkpoint_id,
    }
}

fn rewrite_same_page_toplevel_checkpoint_preference(
    root: &Utf8Path,
    build_root: &Utf8Path,
    original_main: &str,
) -> String {
    let edited_filler = format!(
        "{} {}",
        (0..90)
            .map(|index| format!("body{index:04}"))
            .collect::<Vec<_>>()
            .join(" "),
        (90..180)
            .map(|index| format!("edit{index:04}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let second_main = format!(
        "\\documentclass{{article}}\\begin{{document}} {edited_filler} \\cite{{alpha}} and \\cite{{beta}}.\\bibliography{{refsa,refsb}}\\end{{document}}"
    );
    fs::write(root.join("main.tex"), &second_main).expect("rewrite main");
    fs::write(
        root.join("refsa.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[A 2024]{alpha}  Alpha entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite first bibliography");
    fs::write(
        root.join("refsb.bbl"),
        "\\begin{thebibliography}{1}\n\\bibitem[B 2025]{beta}  Beta entry.\n\\end{thebibliography}\n",
    )
    .expect("rewrite second bibliography");

    let diff_offset = original_main
        .bytes()
        .zip(second_main.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load bundle")
        .checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.meta.source_offset_utf8 <= diff_offset as u32)
        .last()
        .map(|checkpoint| checkpoint.meta.checkpoint_id.clone())
        .expect("expected main checkpoint")
}

fn assert_same_page_toplevel_checkpoint_preference_replay(
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: Vec<Utf8PathBuf>,
    expected_checkpoint_id: String,
) {
    assert_eq!(
        second.reused_checkpoint_id,
        Some(expected_checkpoint_id.clone())
    );
    let build_meta = serde_json::from_slice::<BuildMeta>(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read build meta"),
    )
    .expect("parse build meta");
    assert!(build_meta.aux_sensitive);
    assert_eq!(build_meta.dirty_files, dirty_files);
    assert_eq!(build_meta.start_checkpoint_id, Some(expected_checkpoint_id));
    assert_eq!(build_meta.start_page_index, 0);
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

#[derive(Clone, Copy)]
enum SamePageToplevelCheckpointPreferenceExtraDirtyKind {
    NoExtraDirty,
    Untracked,
    Unreadable,
}

#[derive(Clone, Copy)]
enum SamePageToplevelCheckpointPreferenceExpectedReplay {
    EarlierToplevel,
    Preamble,
}

enum SamePageToplevelCheckpointPreferenceCase {
    Baseline,
    Reversed,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
}

async fn run_same_page_toplevel_checkpoint_preference_case(
    extra_dirty: SamePageToplevelCheckpointPreferenceExtraDirtyKind,
    dirty_files: &[&str],
    expected_replay: SamePageToplevelCheckpointPreferenceExpectedReplay,
) {
    let fixture = prepare_same_page_toplevel_checkpoint_preference_fixture().await;
    let expected_toplevel_checkpoint_id = rewrite_same_page_toplevel_checkpoint_preference(
        fixture.root.as_path(),
        fixture.build_root.as_path(),
        &fixture.original_main,
    );
    match extra_dirty {
        SamePageToplevelCheckpointPreferenceExtraDirtyKind::NoExtraDirty => {}
        SamePageToplevelCheckpointPreferenceExtraDirtyKind::Untracked => {
            fs::write(fixture.root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        SamePageToplevelCheckpointPreferenceExtraDirtyKind::Unreadable => {
            fs::create_dir_all(fixture.root.join("notes.txt"))
                .expect("create unreadable dirty dir");
        }
    }

    let dirty_files = dirty_files
        .iter()
        .map(|dirty_file| Utf8PathBuf::from(*dirty_file))
        .collect::<Vec<_>>();
    let second = fixture
        .driver
        .compile(CompileRequest {
            root: fixture.root.clone(),
            manifest: fixture.world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: fixture.build_root.clone(),
            changed_files: dirty_files.clone(),
        })
        .await
        .expect("second semantic aux build should succeed");
    let expected_checkpoint_id = match expected_replay {
        SamePageToplevelCheckpointPreferenceExpectedReplay::EarlierToplevel => {
            expected_toplevel_checkpoint_id
        }
        SamePageToplevelCheckpointPreferenceExpectedReplay::Preamble => {
            fixture.preamble_checkpoint_id.clone()
        }
    };

    assert_same_page_toplevel_checkpoint_preference_replay(
        fixture.build_root.as_path(),
        &second,
        dirty_files,
        expected_checkpoint_id,
    );
}

async fn run_same_page_toplevel_checkpoint_preference_compact_case(
    case: SamePageToplevelCheckpointPreferenceCase,
) {
    let (extra_dirty, dirty_files, expected_replay) = match case {
        SamePageToplevelCheckpointPreferenceCase::Baseline => (
            SamePageToplevelCheckpointPreferenceExtraDirtyKind::NoExtraDirty,
            &["main.tex", "refsb.bbl", "refsa.bbl"][..],
            SamePageToplevelCheckpointPreferenceExpectedReplay::EarlierToplevel,
        ),
        SamePageToplevelCheckpointPreferenceCase::Reversed => (
            SamePageToplevelCheckpointPreferenceExtraDirtyKind::NoExtraDirty,
            &["refsb.bbl", "refsa.bbl", "main.tex"][..],
            SamePageToplevelCheckpointPreferenceExpectedReplay::EarlierToplevel,
        ),
        SamePageToplevelCheckpointPreferenceCase::UntrackedFollows => (
            SamePageToplevelCheckpointPreferenceExtraDirtyKind::Untracked,
            &["main.tex", "refsb.bbl", "refsa.bbl", "notes.txt"][..],
            SamePageToplevelCheckpointPreferenceExpectedReplay::Preamble,
        ),
        SamePageToplevelCheckpointPreferenceCase::UntrackedPrecedes => (
            SamePageToplevelCheckpointPreferenceExtraDirtyKind::Untracked,
            &["notes.txt", "main.tex", "refsb.bbl", "refsa.bbl"][..],
            SamePageToplevelCheckpointPreferenceExpectedReplay::EarlierToplevel,
        ),
        SamePageToplevelCheckpointPreferenceCase::UnreadableFollows => (
            SamePageToplevelCheckpointPreferenceExtraDirtyKind::Unreadable,
            &["main.tex", "refsb.bbl", "refsa.bbl", "notes.txt"][..],
            SamePageToplevelCheckpointPreferenceExpectedReplay::Preamble,
        ),
        SamePageToplevelCheckpointPreferenceCase::UnreadablePrecedes => (
            SamePageToplevelCheckpointPreferenceExtraDirtyKind::Unreadable,
            &["notes.txt", "main.tex", "refsb.bbl", "refsa.bbl"][..],
            SamePageToplevelCheckpointPreferenceExpectedReplay::Preamble,
        ),
    };
    run_same_page_toplevel_checkpoint_preference_case(extra_dirty, dirty_files, expected_replay)
        .await;
}

type SamePageTopCpCase = SamePageToplevelCheckpointPreferenceCase;

async fn run_same_page_top_cp_case(case: SamePageTopCpCase) {
    run_same_page_toplevel_checkpoint_preference_compact_case(case).await;
}
