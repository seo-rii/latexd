#[test]
fn copy_test_fixture_tree_skips_dev_server_state_and_vcs_metadata() {
    let tempdir = tempdir().expect("tempdir");
    let fixture_root =
        Utf8PathBuf::from_path_buf(tempdir.path().join("fixture")).expect("utf8 fixture root");
    fs::create_dir_all(fixture_root.join("figures")).expect("create figures");
    fs::create_dir_all(fixture_root.join(".latexd/build/rev-1")).expect("create dev state");
    fs::create_dir_all(fixture_root.join(".git")).expect("create vcs metadata");
    fs::write(fixture_root.join("main.tex"), "\\documentclass{article}").expect("write main");
    fs::write(fixture_root.join("figures/plot.pdf"), "%PDF").expect("write figure");
    fs::write(fixture_root.join(".latexd/build/rev-1/output.txt"), "stale").expect("write state");
    fs::write(fixture_root.join(".git/HEAD"), "ref: refs/heads/main").expect("write head");
    let root =
        Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 project root");

    copy_test_fixture_tree(&fixture_root, &root);

    assert!(root.join("main.tex").is_file());
    assert!(root.join("figures/plot.pdf").is_file());
    assert!(!root.join(".latexd").exists());
    assert!(!root.join(".git").exists());
}
