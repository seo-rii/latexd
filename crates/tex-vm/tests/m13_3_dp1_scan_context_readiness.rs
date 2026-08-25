use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const EXPECTED_SCAN_CONTEXT_ORACLE: &str =
    include_str!("fixtures/dimension-scan-context-oracle-v1.json");

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_repository_file(relative_path: &str) -> String {
    std::fs::read_to_string(repository().join(relative_path))
        .unwrap_or_else(|error| panic!("failed to read {relative_path}: {error}"))
}

#[test]
fn dimension_scan_context_fixture_is_versioned_json() {
    let fixture: Value = serde_json::from_str(EXPECTED_SCAN_CONTEXT_ORACLE)
        .expect("dimension scan-context fixture must be valid JSON");

    assert_eq!(fixture["schema_version"], 1);
}

#[test]
fn dimension_scan_context_fixture_covers_font_and_magnification_prerequisites() {
    let fixture: Value = serde_json::from_str(EXPECTED_SCAN_CONTEXT_ORACLE)
        .expect("dimension scan-context fixture must be valid JSON");
    let actual = fixture["case_results"]
        .as_object()
        .expect("case_results must be an object")
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let required = [
        "fresh_current_font",
        "cmr10_metrics",
        "second_font_metrics",
        "grouped_font_switch",
        "repeated_font_selection",
        "scaled_font_selection",
        "font_alias_dynamic_lookup",
        "missing_metric_file",
        "invalid_font_definition",
        "font_magnification_interaction",
        "magnification_fresh_query",
        "magnification_direct_assignments",
        "magnification_optional_equals_signs",
        "magnification_scope_globaldefs",
        "magnification_alias_dynamic_lookup",
        "magnification_afterassignment_success",
        "magnification_afterassignment_error",
        "magnification_true_units",
        "magnification_ordinary_vs_true_units",
        "magnification_reassignment_after_use",
        "magnification_missing_number",
        "magnification_range_error",
        "magnification_maximum_legal_first_preparation",
        "magnification_first_illegal_above_maximum",
        "magnification_incompatible_second_preparation",
        "magnification_preparation_group_globaldefs",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();

    assert_eq!(actual, required);
}

#[test]
fn dimension_scan_context_fixture_freezes_case_shape_and_source_identity() {
    let fixture: Value = serde_json::from_str(EXPECTED_SCAN_CONTEXT_ORACLE)
        .expect("dimension scan-context fixture must be valid JSON");
    let cases = fixture["case_results"]
        .as_object()
        .expect("case_results must be an object");

    assert_eq!(fixture["format"], "latexd.dimension-scan-context-oracle");
    assert_eq!(fixture["compatibility_target"], "TeX82 via pdfTeX INITEX");
    assert_eq!(
        fixture["required_metric_files"],
        serde_json::json!(["cmr10.tfm", "cmr7.tfm"])
    );
    assert_eq!(fixture["expected_processes"], cases.len());
    assert_eq!(fixture["expected_processes"], 26);
    for (case_id, result) in cases {
        assert!(result["diagnostics"].is_array(), "{case_id}");
        assert!(result["exit_status"].is_i64(), "{case_id}");
        assert!(result["observations"].is_object(), "{case_id}");
        assert_eq!(
            result["source_sha256"]
                .as_str()
                .unwrap_or_else(|| panic!("{case_id} source_sha256 must be a string"))
                .len(),
            64,
            "{case_id}"
        );
    }
}

#[test]
fn w2_5_scan_context_gate_has_no_production_activation() {
    let command = read_repository_file("crates/tex-vm/src/command.rs");
    let manifest = read_repository_file("crates/tex-vm/Cargo.toml");
    let vm = read_repository_file("crates/tex-vm/src/lib.rs");
    let snapshot = read_repository_file("crates/tex-vm/src/snapshot.rs");
    let checkpoint = read_repository_file("crates/tex-checkpoint/src/lib.rs");

    for forbidden in [
        "Primitive::HangIndent",
        "Primitive::Mag",
        "Primitive::Font",
        "Primitive::FontDef",
        "Primitive::SetFont",
    ] {
        assert!(
            !command.contains(forbidden),
            "command.rs contains {forbidden}"
        );
        assert!(!vm.contains(forbidden), "lib.rs contains {forbidden}");
    }
    assert!(!manifest.contains("tex-fonts"));
    assert!(!vm.contains("\n        \"hangindent\" =>"));
    assert!(!vm.contains("\n        \"mag\" =>"));
    assert!(!vm.contains("\n        \"font\" =>"));
    let supported = snapshot
        .split_once("pub const VM_SNAPSHOT_DOCUMENT_SUPPORTED_CAPABILITIES")
        .expect("supported capability registry must exist")
        .1
        .split_once("];")
        .expect("supported capability registry must terminate")
        .0;
    assert!(supported.contains("VM_SNAPSHOT_DIMENSION_PARAMETER_STATE_V1_CAPABILITY"));
    assert!(!supported.contains("VM_SNAPSHOT_DIMENSION_PARAMETER_COMMAND_V1_CAPABILITY"));
    assert!(checkpoint.contains("pub const CHECKPOINT_VM_SEMANTIC_EPOCH: u32 = 5;"));
    assert!(checkpoint.contains(
        "pub const SNAPSHOT_WRITE_POLICY: SnapshotWritePolicy = SnapshotWritePolicy::LegacyOnly;"
    ));
}

#[test]
fn scan_context_contract_and_ci_expose_characterization_without_activation() {
    let contract = read_repository_file("docs/m13-3-dp1-scan-context.md");
    let hangindent = read_repository_file("docs/m13-3-dp1-hangindent.md");
    let workflow = read_repository_file(".github/workflows/ci.yml");

    for required in [
        "DimensionScanContext",
        "read_magnification",
        "read_current_font_id",
        "quad_sp",
        "x_height_sp",
        "no fallback",
        "epoch 5",
        "W3 remains blocked",
    ] {
        assert!(
            contract.contains(required),
            "missing contract text: {required}"
        );
    }
    assert!(hangindent.contains("m13-3-dp1-scan-context.md"));
    assert!(workflow.contains("python3 scripts/check_dimension_scan_context_oracle.py"));
    assert!(workflow.contains("dimension-scan-context-oracle"));
}
