#[tokio::test]
async fn internal_compiler_prefers_conservative_rebuild_over_skip_for_late_multi_bibliography_semantic_change()
 {
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 tempdir");
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());
    let build_root = root.join(".latexd/build");
    let world =
        prepare_late_multi_bibliography_semantic_change_workspace(&root, &driver, &build_root)
            .await;

    let second = driver
        .compile(CompileRequest {
            root: root.clone(),
            manifest: world.manifest,
            toplevel: Utf8PathBuf::from("main.tex"),
            rev: 2,
            build_root: build_root.clone(),
            changed_files: vec![Utf8PathBuf::from("refsa.bbl")],
        })
        .await
        .expect("second semantic aux build should succeed");
    assert_late_multi_bibliography_semantic_change_rebuild(
        &build_root,
        &second,
        vec![Utf8PathBuf::from("refsa.bbl")],
    );
}
