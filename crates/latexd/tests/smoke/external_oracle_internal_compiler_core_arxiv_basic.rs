#[tokio::test]
async fn bundled_arxiv_basic_fixture_builds_with_internal_compiler() {
    let fixture_root =
        Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/arxiv-basic");
    let tempdir = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 tempdir");
    copy_test_fixture_tree(&fixture_root, &root);

    let outcome = compile_internal_compiler_main(
        &root,
        1,
        root.join(".latexd/build"),
        vec![Utf8PathBuf::from("main.tex")],
    )
    .await;

    assert!(outcome.pdf_path.exists());
    assert!(outcome.diagnostics.is_empty());
    assert_eq!(outcome.page_metadata.len(), 3);
    assert!(
        outcome
            .dep_trace
            .inputs
            .contains(&Utf8PathBuf::from("main.tex"))
    );
    assert!(
        outcome
            .dep_trace
            .inputs
            .contains(&Utf8PathBuf::from("article.cls"))
    );
    assert_first_page_artifact_urls(&root, &outcome);
}
