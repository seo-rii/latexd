const VM_SOURCE: &str = include_str!("../src/lib.rs");

#[test]
fn vm_has_no_parallel_control_sequence_scope_owner() {
    assert!(
        !VM_SOURCE.contains("ControlSequenceScopes"),
        "the VM must route control-sequence state through Eqtb and SaveStack"
    );
    assert!(
        !VM_SOURCE.contains("control_sequences:"),
        "the VM must not retain a parallel control-sequence state field"
    );
}
