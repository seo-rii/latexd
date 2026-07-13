use std::fs;

use camino::Utf8PathBuf;
use latexd::compiler::{CompileRequest, CompilerDriver};
use tempfile::tempdir;
use tex_world::ProjectWorld;

fn assert_json_expectations(
    fixture: &Utf8PathBuf,
    rev: u64,
    scope: &str,
    value: &serde_json::Value,
    expectations: &str,
) {
    for line in expectations
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let (path, expected_raw) = line.split_once('=').unwrap_or_else(|| {
            panic!(
                "fixture {} rev {} malformed JSON expectation {:?} for {}",
                fixture, rev, line, scope
            )
        });
        let mut current = value;
        for segment in path.trim().split('.') {
            if let Ok(index) = segment.parse::<usize>() {
                current = current.get(index).unwrap_or_else(|| {
                    panic!(
                        "fixture {} rev {} missing JSON index {} in {} for path {}",
                        fixture, rev, index, scope, path
                    )
                });
            } else {
                current = current.get(segment).unwrap_or_else(|| {
                    panic!(
                        "fixture {} rev {} missing JSON key {:?} in {} for path {}",
                        fixture, rev, segment, scope, path
                    )
                });
            }
        }
        let expected = serde_json::from_str::<serde_json::Value>(expected_raw.trim())
            .unwrap_or_else(|_| serde_json::Value::String(expected_raw.trim().to_string()));
        assert_eq!(
            current, &expected,
            "fixture {} rev {} JSON expectation mismatch at {} in {}",
            fixture, rev, path, scope
        );
    }
}

fn failure_snapshot(failure: &latexd::compiler::CompileFailure) -> serde_json::Value {
    let display = failure.to_string();
    let diagnostics = failure
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.clone())
        .collect::<Vec<_>>();
    let mut haystacks = Vec::with_capacity(diagnostics.len() + 1);
    haystacks.push(display.to_lowercase());
    haystacks.extend(diagnostics.iter().map(|message| message.to_lowercase()));
    let contains = |needle: &str| haystacks.iter().any(|haystack| haystack.contains(needle));
    let stage = if contains("failed to scan semantic aux inputs") {
        "semantic-aux-scan"
    } else {
        "compile"
    };
    let subject_kind = if contains("renewcommand") || contains("newcommand") {
        "command"
    } else if contains("package ") {
        "package"
    } else if contains("class ") {
        "class"
    } else if contains(".tex") || contains("sections/") || contains("preamble/") {
        "input"
    } else {
        "other"
    };
    let surface_kind = if contains("errmessage:") {
        "errmessage"
    } else if contains("latex:") {
        "latex-error"
    } else if contains("generic ") {
        "generic-error"
    } else if contains("internal compiler reported diagnostics") {
        "internal-diagnostic"
    } else {
        "direct"
    };
    serde_json::json!({
        "stage": stage,
        "subject_kind": subject_kind,
        "surface_kind": surface_kind,
        "diagnostic_count": diagnostics.len(),
        "message": display,
        "diagnostics": diagnostics,
    })
}

#[tokio::test]
#[ignore = "large bundled corpus; run explicitly in manual or nightly verification"]
async fn arxiv_smoke_fixture_corpus_builds_with_expected_output() {
    let fixture_root =
        Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/arxiv-smoke");
    let mut fixtures = fs::read_dir(fixture_root.as_std_path())
        .expect("read fixture root")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_dir()))
        .map(|entry| Utf8PathBuf::from_path_buf(entry.path()).expect("fixture path should be utf8"))
        .collect::<Vec<_>>();
    fixtures.sort();
    let driver = CompilerDriver::new(Some("internal".to_string()), Vec::new());

    for fixture in fixtures {
        let tempdir = tempdir().expect("tempdir");
        let project_root =
            Utf8PathBuf::from_path_buf(tempdir.path().join("project")).expect("utf8 project root");
        fs::create_dir_all(project_root.as_std_path()).expect("create project root");
        let mut copy_dirs = vec![(fixture.clone(), project_root.clone())];
        while let Some((source_dir, target_dir)) = copy_dirs.pop() {
            fs::create_dir_all(target_dir.as_std_path()).expect("create target dir");
            for entry in fs::read_dir(source_dir.as_std_path())
                .expect("read source dir")
                .filter_map(|entry| entry.ok())
            {
                let source_path =
                    Utf8PathBuf::from_path_buf(entry.path()).expect("fixture path should be utf8");
                let target_path = target_dir.join(entry.file_name().to_string_lossy().as_ref());
                if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                    copy_dirs.push((source_path, target_path));
                } else {
                    fs::copy(source_path.as_std_path(), target_path.as_std_path())
                        .expect("copy fixture file");
                }
            }
        }
        let build_root = project_root.join(".latexd/build");
        let mut revisions = vec![1u64];
        revisions.extend(
            fs::read_dir(fixture.as_std_path())
                .expect("read fixture revisions")
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    let file_type = entry.file_type().ok()?;
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if file_type.is_dir() {
                        return name.strip_prefix("rev")?.parse::<u64>().ok();
                    }
                    if file_type.is_file() {
                        return name
                            .strip_prefix("REV")
                            .and_then(|rest| rest.split_once('-'))
                            .and_then(|(rev, _)| rev.parse::<u64>().ok());
                    }
                    None
                }),
        );
        revisions.sort_unstable();
        revisions.dedup();

        for rev in revisions {
            let prefix = if rev == 1 {
                String::new()
            } else {
                format!("REV{rev}-")
            };
            let mut changed_files = Vec::new();
            if rev > 1 {
                let overlay_root = fixture.join(format!("rev{rev}"));
                if overlay_root.exists() {
                    let mut overlay_dirs = vec![overlay_root.clone()];
                    while let Some(source_dir) = overlay_dirs.pop() {
                        for entry in fs::read_dir(source_dir.as_std_path())
                            .expect("read overlay dir")
                            .filter_map(|entry| entry.ok())
                        {
                            let source_path = Utf8PathBuf::from_path_buf(entry.path())
                                .expect("overlay path should be utf8");
                            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                                overlay_dirs.push(source_path);
                                continue;
                            }
                            let relative_path = source_path
                                .strip_prefix(&overlay_root)
                                .expect("overlay path should be relative to overlay root");
                            let target_path = project_root.join(relative_path);
                            if let Some(parent) = target_path.parent() {
                                fs::create_dir_all(parent.as_std_path())
                                    .expect("create overlay parent");
                            }
                            fs::copy(source_path.as_std_path(), target_path.as_std_path())
                                .expect("copy overlay file");
                            changed_files.push(relative_path.to_owned());
                        }
                    }
                }
                let delete_path = fixture.join(format!("{prefix}DELETE.txt"));
                if delete_path.exists() {
                    let deletes = fs::read_to_string(delete_path.as_std_path())
                        .expect("read fixture delete list");
                    for relative_path in deletes
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                    {
                        let relative_path = Utf8PathBuf::from(relative_path);
                        let target_path = project_root.join(&relative_path);
                        if target_path.exists() {
                            if target_path.as_std_path().is_dir() {
                                fs::remove_dir_all(target_path.as_std_path())
                                    .expect("remove overlay directory");
                            } else {
                                fs::remove_file(target_path.as_std_path())
                                    .expect("remove overlay file");
                            }
                        }
                        changed_files.push(relative_path);
                    }
                }
                changed_files.sort();
            }
            let world = ProjectWorld::load(project_root.clone()).expect("load fixture world");
            let toplevel = world
                .manifest
                .toplevels
                .first()
                .cloned()
                .expect("fixture must declare one toplevel");
            if rev == 1 {
                changed_files.push(toplevel.clone());
            }
            let failure_path = fixture.join(format!("{prefix}FAIL.txt"));
            let compile_result = driver
                .compile(CompileRequest {
                    root: project_root.clone(),
                    manifest: world.manifest.clone(),
                    toplevel: toplevel.clone(),
                    rev,
                    build_root: build_root.clone(),
                    changed_files,
                })
                .await;
            if failure_path.exists() {
                let failure = match compile_result {
                    Ok(_) => panic!("fixture {} rev {} unexpectedly succeeded", fixture, rev),
                    Err(error) => error,
                };
                let fail_expect = fs::read_to_string(failure_path.as_std_path())
                    .expect("read fixture failure list");
                let diagnostics = failure
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.message.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                let debug = format!("{failure:?}");
                let display = failure.to_string();
                for needle in fail_expect
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                {
                    assert!(
                        display.contains(needle)
                            || debug.contains(needle)
                            || diagnostics.contains(needle),
                        "fixture {} rev {} missing failure expectation {:?} in display={:?} debug={:?} diagnostics={:?}",
                        fixture,
                        rev,
                        needle,
                        display,
                        debug,
                        diagnostics
                    );
                }
                let failure_json_expect_path =
                    fixture.join(format!("{prefix}FAIL-JSON-EXPECT.txt"));
                if failure_json_expect_path.exists() {
                    let failure_json_expect =
                        fs::read_to_string(failure_json_expect_path.as_std_path())
                            .expect("read fixture failure json expectation");
                    let failure_snapshot = failure_snapshot(&failure);
                    assert_json_expectations(
                        &fixture,
                        rev,
                        "failure",
                        &failure_snapshot,
                        &failure_json_expect,
                    );
                }
                continue;
            }
            compile_result.unwrap_or_else(|error| {
                panic!("fixture {} rev {} failed: {error:?}", fixture, rev)
            });

            let rev_dir = build_root.join(format!("rev-{rev}"));
            let output =
                fs::read_to_string(rev_dir.join("output.txt")).expect("read fixture output");
            let expected_path = fixture.join(format!("{prefix}EXPECT.txt"));
            if expected_path.exists() {
                let expected = fs::read_to_string(expected_path.as_std_path())
                    .expect("read fixture expectation");
                for needle in expected
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                {
                    assert!(
                        output.contains(needle),
                        "fixture {} rev {} missing expected output {:?} in {:?}",
                        fixture,
                        rev,
                        needle,
                        output
                    );
                }
            }

            let absent_path = fixture.join(format!("{prefix}ABSENT.txt"));
            if absent_path.exists() {
                let absent = fs::read_to_string(absent_path.as_std_path())
                    .expect("read fixture absent list");
                let stored_sources = serde_json::from_slice::<serde_json::Value>(
                    &fs::read(rev_dir.join("sources.json").as_std_path())
                        .expect("read fixture sources"),
                )
                .expect("parse fixture sources");
                for needle in absent
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                {
                    assert!(
                        !output.contains(needle),
                        "fixture {} rev {} unexpectedly contained {:?} in output {:?}",
                        fixture,
                        rev,
                        needle,
                        output
                    );
                    if let Some(executed_files) = stored_sources
                        .get("executed_files")
                        .and_then(serde_json::Value::as_object)
                    {
                        for (path, source) in executed_files {
                            let source =
                                source.as_str().expect("executed source should be a string");
                            assert!(
                                !source.contains(needle),
                                "fixture {} rev {} unexpectedly contained {:?} in executed file {}: {:?}",
                                fixture,
                                rev,
                                needle,
                                path,
                                source
                            );
                        }
                    }
                }
            }

            let executed_expect_path = fixture.join(format!("{prefix}EXECUTED-EXPECT.txt"));
            if executed_expect_path.exists() {
                let executed_expect = fs::read_to_string(executed_expect_path.as_std_path())
                    .expect("read fixture executed expectation");
                let stored_sources = serde_json::from_slice::<serde_json::Value>(
                    &fs::read(rev_dir.join("sources.json").as_std_path())
                        .expect("read fixture sources"),
                )
                .expect("parse fixture sources");
                let executed_files = stored_sources
                    .get("executed_files")
                    .and_then(serde_json::Value::as_object)
                    .expect("executed_files should be an object");
                for line in executed_expect
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                {
                    if let Some((path, needle)) = line.split_once("::") {
                        let path = path.trim();
                        let needle = needle.trim();
                        let source = executed_files
                            .get(path)
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_else(|| {
                                panic!(
                                    "fixture {} rev {} missing executed file {}",
                                    fixture, rev, path
                                )
                            });
                        assert!(
                            source.contains(needle),
                            "fixture {} rev {} missing executed expectation {:?} in {}: {:?}",
                            fixture,
                            rev,
                            needle,
                            path,
                            source
                        );
                    } else {
                        assert!(
                            executed_files.values().any(|source| {
                                source
                                    .as_str()
                                    .expect("executed source should be a string")
                                    .contains(line)
                            }),
                            "fixture {} rev {} missing executed expectation {:?} in any executed file",
                            fixture,
                            rev,
                            line
                        );
                    }
                }
            }

            let artifact_expect_path = fixture.join(format!("{prefix}ARTIFACT-EXPECT.txt"));
            if artifact_expect_path.exists() {
                let artifact_expect = fs::read_to_string(artifact_expect_path.as_std_path())
                    .expect("read fixture artifact expectation");
                for line in artifact_expect
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                {
                    let (artifact_path, needle) = line
                        .split_once("::")
                        .map_or((line, None), |parts| (parts.0.trim(), Some(parts.1.trim())));
                    let artifact_path = rev_dir.join(artifact_path);
                    let artifact =
                        fs::read_to_string(artifact_path.as_std_path()).unwrap_or_else(|error| {
                            panic!(
                                "fixture {} rev {} failed to read artifact {}: {error}",
                                fixture, rev, artifact_path
                            )
                        });
                    if let Some(needle) = needle {
                        assert!(
                            artifact.contains(needle),
                            "fixture {} rev {} missing artifact expectation {:?} in {}: {:?}",
                            fixture,
                            rev,
                            needle,
                            artifact_path,
                            artifact
                        );
                    }
                }
            }

            let json_expect_path = fixture.join(format!("{prefix}JSON-EXPECT.txt"));
            if json_expect_path.exists() {
                let json_expect = fs::read_to_string(json_expect_path.as_std_path())
                    .expect("read fixture json expectation");
                for line in json_expect
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                {
                    let (artifact_path, expectation) = line.split_once("::").unwrap_or_else(|| {
                        panic!(
                            "fixture {} rev {} malformed JSON expectation {:?}",
                            fixture, rev, line
                        )
                    });
                    let (path, expected_raw) = expectation.split_once('=').unwrap_or_else(|| {
                        panic!(
                            "fixture {} rev {} malformed JSON expectation {:?}",
                            fixture, rev, line
                        )
                    });
                    let artifact_path = rev_dir.join(artifact_path.trim());
                    let artifact = serde_json::from_slice::<serde_json::Value>(
                        &fs::read(artifact_path.as_std_path()).unwrap_or_else(|error| {
                            panic!(
                                "fixture {} rev {} failed to read JSON artifact {}: {error}",
                                fixture, rev, artifact_path
                            )
                        }),
                    )
                    .unwrap_or_else(|error| {
                        panic!(
                            "fixture {} rev {} failed to parse JSON artifact {}: {error}",
                            fixture, rev, artifact_path
                        )
                    });
                    assert_json_expectations(
                        &fixture,
                        rev,
                        artifact_path.as_str(),
                        &artifact,
                        &format!("{path}={expected_raw}"),
                    );
                }
            }
        }
    }
}
