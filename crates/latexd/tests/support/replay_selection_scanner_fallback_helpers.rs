/// An execution boundary is insufficient when the scanner has already read a
/// changed active parent's suffix. Preserve the old candidate as a witness of
/// that hazard, then require an explicit safe fallback and a clean-build oracle.
async fn assert_scanner_lookahead_fallback(
    root: &Utf8Path,
    build_root: &Utf8Path,
    second: &CompileOutcome,
    dirty_files: &[Utf8PathBuf],
    rejected_checkpoint_id: &str,
    rejected_page_index: usize,
) -> (String, usize) {
    let previous = load_checkpoint_bundle(&build_root.join("rev-1/checkpoints.json"))
        .expect("load candidate checkpoint states");
    let rejected = previous
        .checkpoints
        .iter()
        .find(|checkpoint| checkpoint.meta.checkpoint_id == rejected_checkpoint_id)
        .expect("original input-boundary candidate remains captured");
    assert_eq!(rejected.meta.kind, CheckpointKind::InputBoundary);
    assert_eq!(rejected.meta.page_index_after, rejected_page_index);
    let restore = rejected
        .snapshot_for_restore()
        .expect("candidate has a restorable execution snapshot");
    let state = restore.state();
    let continuation = state.input_continuation.as_ref().expect("active inputs");
    let capture = state.semantic_capture.as_ref().expect("scanner readset");
    let changed_active_sources = continuation
        .source_stack
        .iter()
        .filter(|frame| dirty_files.contains(&frame.path))
        .filter_map(|frame| {
            let captured = capture.source_buffers.get(&frame.path)?;
            let current =
                fs::read_to_string(root.join(&frame.path)).expect("read changed active parent");
            (captured != &current).then_some(frame.path.clone())
        })
        .collect::<Vec<_>>();
    assert!(
        !changed_active_sources.is_empty(),
        "the original candidate must own stale scanner state for a changed active parent"
    );

    let safe = previous.checkpoints.first().expect("preamble checkpoint");
    assert_eq!(safe.meta.kind, CheckpointKind::Preamble);
    assert!(safe.meta.continuation_safety.is_safe());
    assert!(safe.snapshot_for_restore().is_some());
    assert_ne!(safe.meta.checkpoint_id, rejected_checkpoint_id);
    assert!(safe.meta.output_start_utf8 <= rejected.meta.output_start_utf8);
    assert_eq!(
        second.reused_checkpoint_id.as_deref(),
        Some(safe.meta.checkpoint_id.as_str()),
        "changed scanner parents {changed_active_sources:?} must restart before their captured effects"
    );
    let metadata: serde_json::Value = serde_json::from_slice(
        &fs::read(build_root.join("rev-2/build-meta.json")).expect("read fallback telemetry"),
    )
    .expect("parse fallback telemetry");
    assert_eq!(
        metadata["trace"]["checkpoint_fallback"],
        "changed_active_source_scanner_lookahead"
    );
    assert_eq!(metadata["start_checkpoint_id"], safe.meta.checkpoint_id);
    assert_eq!(metadata["start_page_index"], 0);

    assert_eager_rebuild_matches_clean(root, build_root, 2, second).await;
    (safe.meta.checkpoint_id.clone(), 0)
}
