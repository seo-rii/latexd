/// Compare a resumed result with a fresh build of the same frozen fixture.
/// Only the build directory differs: no checkpoint, source cache, or previous
/// output manifest is available to the clean driver.
async fn assert_eager_rebuild_matches_clean(
    root: &Utf8Path,
    build_root: &Utf8Path,
    rev: u64,
    outcome: &CompileOutcome,
) -> tex_checkpoint::CheckpointBundle {
    let clean_directory = tempfile::Builder::new()
        .prefix("replay-clean-")
        .tempdir_in(root.join(".latexd"))
        .expect("independent clean build directory");
    let clean_root = Utf8Path::from_path(clean_directory.path()).expect("UTF-8 clean build root");
    let clean = CompilerDriver::new(Some("internal".into()), Vec::new())
        .compile(CompileRequest {
            root: root.to_owned(),
            manifest: ProjectWorld::load(root.to_owned())
                .expect("fixture world")
                .manifest,
            toplevel: "main.tex".into(),
            rev: 1,
            build_root: clean_root.to_owned(),
            changed_files: outcome.dep_trace.inputs.clone(),
        })
        .await
        .expect("independent clean compilation");
    assert_eq!(outcome.diagnostics, clean.diagnostics, "clean diagnostics");
    assert_eq!(outcome.dep_trace, clean.dep_trace, "clean input readset");
    let current_dir = build_root.join(format!("rev-{rev}"));
    let clean_dir = clean_root.join("rev-1");
    for name in [
        "render-ir/page-display-list.json",
        "render-ir/events.json",
        "render-ir/document-ir.json",
        "page-syncmap.json",
        "aux.json",
        "semantic-index.json",
    ] {
        let read = |directory: &Utf8Path| {
            let path = directory.join(name);
            path.exists().then(|| {
                serde_json::from_slice::<serde_json::Value>(
                    &fs::read(path).expect("read comparison artifact"),
                )
                .expect("parse comparison artifact")
            })
        };
        assert!(
            read(&current_dir) == read(&clean_dir),
            "clean mismatch: {name}"
        );
    }
    let read_sources = |directory: &Utf8Path| {
        serde_json::from_slice::<StoredSources>(
            &fs::read(directory.join("sources.json")).expect("read source snapshot"),
        )
        .expect("parse source snapshot")
    };
    let current_sources = read_sources(&current_dir);
    let clean_sources = read_sources(&clean_dir);
    assert_eq!(current_sources.files, clean_sources.files, "source bytes");
    assert_eq!(
        current_sources.executed_files, clean_sources.executed_files,
        "executed source bytes"
    );
    assert_eq!(
        current_sources.rewrite_spans, clean_sources.rewrite_spans,
        "source rewrite provenance"
    );
    for name in ["output.txt", "semantic.aux"] {
        let read = |directory: &Utf8Path| {
            let path = directory.join(name);
            path.exists()
                .then(|| fs::read(path).expect("read text artifact"))
        };
        assert!(
            read(&current_dir) == read(&clean_dir),
            "clean mismatch: {name}"
        );
    }
    load_checkpoint_bundle(&clean_dir.join("checkpoints.json")).expect("clean checkpoints")
}
