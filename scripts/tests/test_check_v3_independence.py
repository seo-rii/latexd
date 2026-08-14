import unittest

from scripts.check_v3_independence import check_migration_patch, check_patch


class V3IndependenceGuardTests(unittest.TestCase):
    def test_conditional_guard_skips_unrelated_commits(self) -> None:
        patch = """\
diff --git a/crates/latexd/src/compiler.rs b/crates/latexd/src/compiler.rs
--- a/crates/latexd/src/compiler.rs
+++ b/crates/latexd/src/compiler.rs
@@ -1,0 +2 @@
+use crate::ExecutedSourceSlice;
"""

        self.assertEqual(check_migration_patch(patch), [])

    def test_conditional_guard_checks_the_whole_v3_migration_commit(self) -> None:
        patch = """\
diff --git a/crates/tex-vm/src/eqtb.rs b/crates/tex-vm/src/eqtb.rs
--- a/crates/tex-vm/src/eqtb.rs
+++ b/crates/tex-vm/src/eqtb.rs
@@ -1,0 +2 @@
+enum ControlSequenceOwner {}
diff --git a/crates/tex-vm/src/snapshot.rs b/crates/tex-vm/src/snapshot.rs
--- a/crates/tex-vm/src/snapshot.rs
+++ b/crates/tex-vm/src/snapshot.rs
@@ -1 +1 @@
-pub const VERSION: u32 = 22;
+pub const VERSION: u32 = 23;
"""

        violations = check_migration_patch(patch)

        self.assertEqual(len(violations), 1)
        self.assertIn("path is outside", violations[0])

    def test_conditional_guard_skips_sibling_eqtb_owner_migrations(self) -> None:
        patch = """\
diff --git a/crates/tex-vm/src/eqtb.rs b/crates/tex-vm/src/eqtb.rs
--- a/crates/tex-vm/src/eqtb.rs
+++ b/crates/tex-vm/src/eqtb.rs
@@ -1,0 +2 @@
+enum IntegerParameterOwner {}
diff --git a/crates/tex-vm/src/snapshot.rs b/crates/tex-vm/src/snapshot.rs
--- a/crates/tex-vm/src/snapshot.rs
+++ b/crates/tex-vm/src/snapshot.rs
@@ -1,0 +2 @@
+const INTEGER_PARAMETER_CAPABILITY: &str = "eqtb.integer-parameter-state.v1";
diff --git a/crates/tex-checkpoint/src/lib.rs b/crates/tex-checkpoint/src/lib.rs
--- a/crates/tex-checkpoint/src/lib.rs
+++ b/crates/tex-checkpoint/src/lib.rs
@@ -1,0 +2 @@
+const INTEGER_PARAMETER_HASH_DOMAIN: &[u8] = b"integer-parameter";
"""

        self.assertEqual(check_migration_patch(patch), [])

    def test_accepts_bounded_v3_ownership_and_test_changes(self) -> None:
        patch = """\
diff --git a/crates/tex-vm/src/eqtb.rs b/crates/tex-vm/src/eqtb.rs
--- a/crates/tex-vm/src/eqtb.rs
+++ b/crates/tex-vm/src/eqtb.rs
@@ -1,0 +2,2 @@
+use tex_tokens::ControlSequenceId;
+const OWNER: &str = "control_sequence";
diff --git a/crates/tex-vm/tests/v3_contract.rs b/crates/tex-vm/tests/v3_contract.rs
--- /dev/null
+++ b/crates/tex-vm/tests/v3_contract.rs
@@ -0,0 +1 @@
+fn identity_words_are_allowed_in_tests() {}
diff --git a/docs/vm-semantic-foundation-plan.md b/docs/vm-semantic-foundation-plan.md
--- a/docs/vm-semantic-foundation-plan.md
+++ b/docs/vm-semantic-foundation-plan.md
@@ -1,0 +2 @@
+V3 proof evidence.
"""

        self.assertEqual(check_patch(patch), [])

    def test_rejects_changes_outside_the_bounded_migration_surface(self) -> None:
        patch = """\
diff --git a/crates/tex-vm/src/snapshot.rs b/crates/tex-vm/src/snapshot.rs
--- a/crates/tex-vm/src/snapshot.rs
+++ b/crates/tex-vm/src/snapshot.rs
@@ -1 +1 @@
-pub const VERSION: u32 = 22;
+pub const VERSION: u32 = 23;
diff --git a/crates/tex-render-model/src/events.rs b/crates/tex-render-model/src/events.rs
--- a/crates/tex-render-model/src/events.rs
+++ b/crates/tex-render-model/src/events.rs
@@ -1 +1 @@
-pub const VERSION: u32 = 5;
+pub const VERSION: u32 = 6;
"""

        violations = check_patch(patch)

        self.assertEqual(len(violations), 2)
        self.assertTrue(all("path is outside" in violation for violation in violations))

    def test_rejects_new_identity_provenance_and_persistence_symbols(self) -> None:
        patch = """\
diff --git a/crates/tex-vm/src/eqtb.rs b/crates/tex-vm/src/eqtb.rs
--- a/crates/tex-vm/src/eqtb.rs
+++ b/crates/tex-vm/src/eqtb.rs
@@ -1,0 +2,4 @@
+use crate::ExecutedSourceSlice;
+pub type DurableCommand = ControlSequenceId;
+#[derive(Serialize, Deserialize)]
+struct PersistedOwner;
"""

        violations = check_patch(patch)

        self.assertEqual(len(violations), 3)
        self.assertTrue(any("ExecutedSourceSlice" in item for item in violations))
        self.assertTrue(any("durable/public ControlSequenceId" in item for item in violations))
        self.assertTrue(any("snapshot persistence" in item for item in violations))

    def test_ignores_removed_and_context_lines(self) -> None:
        patch = """\
diff --git a/crates/tex-vm/src/lib.rs b/crates/tex-vm/src/lib.rs
--- a/crates/tex-vm/src/lib.rs
+++ b/crates/tex-vm/src/lib.rs
@@ -1,2 +1,2 @@
 use tex_render_model::SourceProvenance;
-let old: EventSequence = 1;
+let owner = ControlSequenceScopes::new();
"""

        self.assertEqual(check_patch(patch), [])

    def test_ignores_forbidden_words_in_comments_and_string_literals(self) -> None:
        patch = """\
diff --git a/crates/tex-vm/src/eqtb.rs b/crates/tex-vm/src/eqtb.rs
--- a/crates/tex-vm/src/eqtb.rs
+++ b/crates/tex-vm/src/eqtb.rs
@@ -1,0 +2,2 @@
+// ExecutedSourceSlice remains outside V3.
+const EXPLANATION: &str = "SourceProvenance is deferred";
"""

        self.assertEqual(check_patch(patch), [])


if __name__ == "__main__":
    unittest.main()
