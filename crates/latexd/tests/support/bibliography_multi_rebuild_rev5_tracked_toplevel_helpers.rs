enum Rev5TrackedToplevelNoise {
    Untracked,
    Unreadable,
}

enum Rev5TrackedToplevelCase {
    Baseline,
    Reversed,
    UntrackedFollows,
    UntrackedPrecedes,
    UnreadableFollows,
    UnreadablePrecedes,
    SiblingBaseline,
    SiblingReversed,
    SiblingInterleaved,
    SiblingOtherInterleaved,
    SiblingUntrackedFollows,
    SiblingUntrackedPrecedes,
    SiblingUnreadableFollows,
    SiblingUnreadablePrecedes,
    SiblingInterleavedUntrackedFollows,
    SiblingInterleavedUntrackedPrecedes,
    SiblingInterleavedUnreadableFollows,
    SiblingInterleavedUnreadablePrecedes,
    SiblingOtherInterleavedUntrackedFollows,
    SiblingOtherInterleavedUntrackedPrecedes,
    SiblingOtherInterleavedUnreadableFollows,
    SiblingOtherInterleavedUnreadablePrecedes,
}

async fn run_bibliography_multi_rebuild_rev5_tracked_toplevel_case(case: Rev5TrackedToplevelCase) {
    let (dirty_file_paths, noise) = match case {
        Rev5TrackedToplevelCase::Baseline => (&["refsb.bbl", "main.tex"][..], None),
        Rev5TrackedToplevelCase::Reversed => (&["main.tex", "refsb.bbl"][..], None),
        Rev5TrackedToplevelCase::UntrackedFollows => (
            &["refsb.bbl", "main.tex", "notes.txt"][..],
            Some(Rev5TrackedToplevelNoise::Untracked),
        ),
        Rev5TrackedToplevelCase::UntrackedPrecedes => (
            &["notes.txt", "refsb.bbl", "main.tex"][..],
            Some(Rev5TrackedToplevelNoise::Untracked),
        ),
        Rev5TrackedToplevelCase::UnreadableFollows => (
            &["refsb.bbl", "main.tex", "notes.txt"][..],
            Some(Rev5TrackedToplevelNoise::Unreadable),
        ),
        Rev5TrackedToplevelCase::UnreadablePrecedes => (
            &["notes.txt", "refsb.bbl", "main.tex"][..],
            Some(Rev5TrackedToplevelNoise::Unreadable),
        ),
        Rev5TrackedToplevelCase::SiblingBaseline => {
            (&["refsb.bbl", "main.tex", "refsa.bbl"][..], None)
        }
        Rev5TrackedToplevelCase::SiblingReversed => {
            (&["refsa.bbl", "main.tex", "refsb.bbl"][..], None)
        }
        Rev5TrackedToplevelCase::SiblingInterleaved => {
            (&["main.tex", "refsb.bbl", "refsa.bbl"][..], None)
        }
        Rev5TrackedToplevelCase::SiblingOtherInterleaved => {
            (&["main.tex", "refsa.bbl", "refsb.bbl"][..], None)
        }
        Rev5TrackedToplevelCase::SiblingUntrackedFollows => (
            &["refsb.bbl", "main.tex", "refsa.bbl", "notes.txt"][..],
            Some(Rev5TrackedToplevelNoise::Untracked),
        ),
        Rev5TrackedToplevelCase::SiblingUntrackedPrecedes => (
            &["notes.txt", "refsa.bbl", "main.tex", "refsb.bbl"][..],
            Some(Rev5TrackedToplevelNoise::Untracked),
        ),
        Rev5TrackedToplevelCase::SiblingUnreadableFollows => (
            &["refsb.bbl", "main.tex", "refsa.bbl", "notes.txt"][..],
            Some(Rev5TrackedToplevelNoise::Unreadable),
        ),
        Rev5TrackedToplevelCase::SiblingUnreadablePrecedes => (
            &["notes.txt", "refsa.bbl", "main.tex", "refsb.bbl"][..],
            Some(Rev5TrackedToplevelNoise::Unreadable),
        ),
        Rev5TrackedToplevelCase::SiblingInterleavedUntrackedFollows => (
            &["main.tex", "refsb.bbl", "refsa.bbl", "notes.txt"][..],
            Some(Rev5TrackedToplevelNoise::Untracked),
        ),
        Rev5TrackedToplevelCase::SiblingInterleavedUntrackedPrecedes => (
            &["notes.txt", "main.tex", "refsb.bbl", "refsa.bbl"][..],
            Some(Rev5TrackedToplevelNoise::Untracked),
        ),
        Rev5TrackedToplevelCase::SiblingInterleavedUnreadableFollows => (
            &["main.tex", "refsb.bbl", "refsa.bbl", "notes.txt"][..],
            Some(Rev5TrackedToplevelNoise::Unreadable),
        ),
        Rev5TrackedToplevelCase::SiblingInterleavedUnreadablePrecedes => (
            &["notes.txt", "main.tex", "refsb.bbl", "refsa.bbl"][..],
            Some(Rev5TrackedToplevelNoise::Unreadable),
        ),
        Rev5TrackedToplevelCase::SiblingOtherInterleavedUntrackedFollows => (
            &["main.tex", "refsa.bbl", "refsb.bbl", "notes.txt"][..],
            Some(Rev5TrackedToplevelNoise::Untracked),
        ),
        Rev5TrackedToplevelCase::SiblingOtherInterleavedUntrackedPrecedes => (
            &["notes.txt", "main.tex", "refsa.bbl", "refsb.bbl"][..],
            Some(Rev5TrackedToplevelNoise::Untracked),
        ),
        Rev5TrackedToplevelCase::SiblingOtherInterleavedUnreadableFollows => (
            &["main.tex", "refsa.bbl", "refsb.bbl", "notes.txt"][..],
            Some(Rev5TrackedToplevelNoise::Unreadable),
        ),
        Rev5TrackedToplevelCase::SiblingOtherInterleavedUnreadablePrecedes => (
            &["notes.txt", "main.tex", "refsa.bbl", "refsb.bbl"][..],
            Some(Rev5TrackedToplevelNoise::Unreadable),
        ),
    };

    run_bibliography_multi_rebuild_rev5_tracked_toplevel(dirty_file_paths, noise).await;
}

async fn run_rev5_tracked_toplevel_case(case: Rev5TrackedToplevelCase) {
    run_bibliography_multi_rebuild_rev5_tracked_toplevel_case(case).await;
}

type Rev5TopCase = Rev5TrackedToplevelCase;

async fn run_rev5_top_case(case: Rev5TopCase) {
    run_bibliography_multi_rebuild_rev5_tracked_toplevel_case(case).await;
}

async fn run_bibliography_multi_rebuild_rev5_tracked_toplevel(
    dirty_file_paths: &[&str],
    noise: Option<Rev5TrackedToplevelNoise>,
) {
    let fixture_root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/arxiv-smoke/optioned-bibliography-order-stack");
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 tempdir");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    build_optioned_bibliography_order_stack_to_rev4(&fixture_root, &root, &driver, &build_root)
        .await;

    apply_fixture_overlay(&fixture_root.join("rev5"), &root);
    let main =
        fs::read_to_string(root.join("main.tex")).expect("read tracked toplevel after rev4 build");
    fs::write(
        root.join("main.tex"),
        main.replace("\\end{document}", "Tracked tail revision.\n\\end{document}"),
    )
    .expect("rewrite tracked toplevel");
    match noise {
        Some(Rev5TrackedToplevelNoise::Untracked) => {
            fs::write(root.join("notes.txt"), "scratch notes").expect("write notes");
        }
        Some(Rev5TrackedToplevelNoise::Unreadable) => {
            fs::create_dir_all(root.join("notes.txt")).expect("create unreadable dirty dir");
        }
        None => {}
    }
    let dirty_files = dirty_file_paths
        .iter()
        .map(|path| Utf8PathBuf::from(*path))
        .collect::<Vec<_>>();
    let world = ProjectWorld::load(root.clone()).expect("world");
    let fifth = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest.clone(),
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 5,
            build_root: build_root.clone(),
            changed_files: dirty_files.clone(),
        })
        .await
        .expect("fifth build should succeed");

    assert_rev5_semantic_multi_bibliography_base_rebuild(&build_root, &fifth, dirty_files);
    let fifth_sources = serde_json::from_slice::<StoredSources>(
        &fs::read(build_root.join("rev-5/sources.json")).expect("read fifth sources"),
    )
    .expect("parse fifth sources");
    assert!(
        fifth_sources.executed_files[&Utf8PathBuf::from("main.tex")]
            .contains("Tracked tail revision."),
        "executed main.tex should reflect the later tracked toplevel change"
    );
    let fifth_output =
        fs::read_to_string(build_root.join("rev-5/output.txt")).expect("read fifth output");
    assert!(fifth_output.contains("Tracked tail revision."));
}
