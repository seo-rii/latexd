import json
import io
import unittest
from contextlib import redirect_stdout
from pathlib import Path

from scripts.check_tfm_validation_ledger import (
    main,
    validate_extensible_source_contract,
    validate_kern_source_contract,
    validate_parameter_source_contract,
    validate_rule_contract,
    validate_rule_ledger,
    validate_rule_transition,
    validate_rule_transition_v3,
    validate_rule_transition_v4,
    validate_transition_chain,
)


ROOT = Path(__file__).parents[2]
LEDGER = ROOT / "docs/tex82-read-font-info-validation-rules.md"
CORPUS_MANIFEST = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validity-oracle-v2/manifest.json"
)
RULE_CONTRACT = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validation-rules-v1.json"
)
RULE_TRANSITION = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validation-rule-transition-v2.json"
)
RULE_TRANSITION_V3 = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validation-rule-transition-v3.json"
)
RULE_TRANSITION_V4 = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validation-rule-transition-v4.json"
)
KERN_SOURCE_CONTRACT = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-kern-source-contract-v1.json"
)
EXTENSIBLE_SOURCE_CONTRACT = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-extensible-source-contract-v1.json"
)
PARAMETER_SOURCE_CONTRACT = (
    ROOT
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-parameter-source-contract-v1.json"
)
LIG_KERN_SOURCE_CONTRACT = ROOT / "docs/tex82-read-font-info-lig-kern.md"
KERN_TDD_RED_EVIDENCE = ROOT / "docs/evidence/tex-tfm-kern-tdd-red-v1.md"
EXTENSIBLE_TDD_RED_EVIDENCE = (
    ROOT / "docs/evidence/tex-tfm-extensible-tdd-red-v1.md"
)
PARAMETER_SOURCE_DOCUMENT = ROOT / "docs/tex82-read-font-info-parameters.md"
PARAMETER_TDD_RED_EVIDENCE = ROOT / "docs/evidence/tex-tfm-parameter-tdd-red-v1.md"
PARAMETER_PRO_CLOSURE_EVIDENCE = (
    ROOT / "docs/evidence/tex-tfm-parameter-pro-closure-v1.md"
)


def ledger_text() -> str:
    return LEDGER.read_text(encoding="utf-8")


def fixture_case_ids() -> set[str]:
    manifest = json.loads(CORPUS_MANIFEST.read_text(encoding="utf-8"))
    return {case["id"] for case in manifest["cases"]}


def rule_contract() -> dict[str, object]:
    return json.loads(RULE_CONTRACT.read_text(encoding="utf-8"))


def rule_transition() -> dict[str, object]:
    return json.loads(RULE_TRANSITION.read_text(encoding="utf-8"))


def rule_transition_v3() -> dict[str, object]:
    return json.loads(RULE_TRANSITION_V3.read_text(encoding="utf-8"))


def rule_transition_v4() -> dict[str, object]:
    return json.loads(RULE_TRANSITION_V4.read_text(encoding="utf-8"))


def kern_source_contract() -> dict[str, object]:
    return json.loads(KERN_SOURCE_CONTRACT.read_text(encoding="utf-8"))


def extensible_source_contract() -> dict[str, object]:
    return json.loads(EXTENSIBLE_SOURCE_CONTRACT.read_text(encoding="utf-8"))


def parameter_source_contract() -> dict[str, object]:
    return json.loads(PARAMETER_SOURCE_CONTRACT.read_text(encoding="utf-8"))


class TfmValidationLedgerTests(unittest.TestCase):
    def test_parameter_pro_closure_is_content_addressed_and_exact(self) -> None:
        evidence = PARAMETER_PRO_CLOSURE_EVIDENCE.read_text(encoding="utf-8")
        for required in (
            "PROCEED_PRIVATE_TFM_COMPLETION",
            "6a93e9d1-5100-83ee-85a3-cb84f168bbf9",
            "review-20260830-172811-d3b881d5",
            "60ab499e6e920dc635d7e5bb11e9a2f236118e3d6c049306d7880f57adaff9ec",
            "e22ab8f1572275349b92ac7ff54555fbd4b29d1cce93a24f0b20793aa162ee8b",
            "0dcb7124f1b65764235883cc12f8f1c6c6382139be45667ee276b95cc8416a35",
            "confidence 0.88",
            "`6f8bdea`",
            "`98250d6`",
            "out-of-line child module",
            "zero-caller",
            "whole-oracle/no-panic/completion-hardening",
        ):
            self.assertIn(required, evidence)

    def test_parameter_closure_authorizes_only_private_hardening(self) -> None:
        for path in (
            ROOT / "PLAN.md",
            ROOT / "docs/m13-3-dp1-scan-context.md",
            ROOT / "docs/tex82-read-font-info-validation-rules.md",
            ROOT / "docs/tex82-read-font-info-extensibles.md",
            PARAMETER_SOURCE_DOCUMENT,
        ):
            document = path.read_text(encoding="utf-8")
            for required in (
                "PROCEED_PRIVATE_TFM_COMPLETION",
                "6a93e9d1-5100-83ee-85a3-cb84f168bbf9",
                "docs/evidence/tex-tfm-parameter-pro-closure-v1.md",
                "whole-oracle/no-panic/completion-hardening",
                "out-of-line child module",
                "zero caller",
            ):
                self.assertIn(required, document, path)

    def test_parameter_red_evidence_is_content_addressed_and_exact(self) -> None:
        evidence = PARAMETER_TDD_RED_EVIDENCE.read_text(encoding="utf-8")
        for required in (
            "72deabc1701a0c156a105637272de48d7a0ec35aa87fbafd61ebc17cc3f2af45",
            "603949a341a50e9b81b41a29074197c9b5bd33e29337d6146663edd866f80768",
            "c10c863fb9d6baa0ab3264ec1bda7559d99831b75b53472dfd39652700516183",
            "3bcaf9adb2949a6615f3543f907f794a7a841869f866b94155ddfd9d8676621e",
            "no `ParameterCheckedTfm` in `tfm_validation`",
            "no `ParameterValidationRule` in `tfm_validation`",
            "no `SignedSlant` in `tfm_validation`",
            "no `check_parameters` in `tfm_validation`",
            'left: ["check_preamble_header", "check_characters", "check_boxes", "check_lig_kern", "check_kerns", "check_extensibles"]',
            "No non-building RED commit was created",
        ):
            self.assertIn(required, evidence)

    def test_private_parameter_implementation_is_documented(self) -> None:
        for path in (
            ROOT / "PLAN.md",
            ROOT / "docs/m13-3-dp1-scan-context.md",
            ROOT / "docs/tex82-read-font-info-validation-rules.md",
            ROOT / "docs/tex82-read-font-info-extensibles.md",
            PARAMETER_SOURCE_DOCUMENT,
        ):
            document = path.read_text(encoding="utf-8")
            for required in (
                "private `ParameterCheckedTfm` implementation",
                "`SignedSlant`",
                "254 forbidden",
                "`np>7`",
                "`np=32755`",
                "8/8 parameter witnesses",
                "same raw allocation",
                "zero caller",
                "docs/evidence/tex-tfm-parameter-tdd-red-v1.md",
                "dedicated parameter closure review",
            ):
                self.assertIn(required, document, path)

    def test_private_phase_sequence_is_current_through_parameters(self) -> None:
        document = (ROOT / "docs/tex82-read-font-info-validation-rules.md").read_text(
            encoding="utf-8"
        )
        for required in (
            "5. **Implemented privately:** exact kern scaling",
            "6. **Implemented privately:** extensible recipes",
            "7. **Implemented privately:** every supplied parameter",
            "8. after a dedicated parameter closure review",
        ):
            self.assertIn(required, document)

    def test_parameter_source_contract_is_documented_at_every_phase_boundary(
        self,
    ) -> None:
        for path in (
            ROOT / "PLAN.md",
            ROOT / "docs/m13-3-dp1-scan-context.md",
            ROOT / "docs/tex82-read-font-info-validation-rules.md",
            ROOT / "docs/tex82-read-font-info-extensibles.md",
            PARAMETER_SOURCE_DOCUMENT,
        ):
            document = path.read_text(encoding="utf-8")
            for required in (
                "tfm-parameter-source-contract-v1.json",
                "223aad57857393d02096adbdaa9cc587be13c515e9e7e86e1b19454f0c8164dd",
                "90983c5403e96dacbf16767a5cb343ca91c7913d7e60925a497b38870ab36265",
                "lines 11188..11199",
                "`np=32755`",
                "EOF",
                "prospective RED",
            ):
                self.assertIn(required, document, path)

    def test_parameter_source_contract_pins_exact_successor_boundary(self) -> None:
        contract = parameter_source_contract()
        self.assertEqual(
            validate_parameter_source_contract(
                contract,
                rule_transition_v4(),
                extensible_source_contract(),
            ),
            [],
        )
        self.assertEqual(type(contract["schema_version"]), int)
        self.assertEqual(contract["schema_version"], 1)
        self.assertEqual(
            contract["focused_source"],
            {
                "compatibility_source_sha256": (
                    "c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324"
                ),
                "fix_word_scaling_section": {
                    "lines": "11108..11130",
                    "sha256": (
                        "306907b8734bfa4dc990546e1fb84d0158c2b9af2338faed18808a06c4bfa58e"
                    ),
                },
                "complete_parameter_section": {
                    "lines": "11188..11199",
                    "sha256": (
                        "3ab5b795c1f4f0f3f28883d345d4264a3a6d8c5ed391bb41ac23456df5027c07"
                    ),
                },
                "slant_branch_section": {
                    "lines": "11189..11195",
                    "sha256": (
                        "150a57332ca1d79eac34af2a76283536424a51cfc7b4fdcd264ec210de01903f"
                    ),
                },
                "non_slant_branch_section": {
                    "lines": "11196..11196",
                    "sha256": (
                        "b281fc02beafc4e18958430f3525d61205b5006dd5bf6b712e46d3bd9520f134"
                    ),
                },
                "eof_check_section": {
                    "lines": "11197..11197",
                    "sha256": (
                        "a33f2363a60c6b862002eb890c3d95e46f25f1e0672f01c4d226b48ef72c1da0"
                    ),
                },
                "zero_fill_section": {
                    "lines": "11198..11198",
                    "sha256": (
                        "9d3fe901814da1c8bf0d8776a1891d590e83186b90b5978df9a0819edaf9f2bd"
                    ),
                },
            },
        )
        boundary = contract["proof_boundary"]
        self.assertEqual(boundary["input"], "ExtensibleCheckedTfm")
        self.assertEqual(boundary["output"], "ParameterCheckedTfm")
        self.assertEqual(
            boundary["owned_rule_ids"],
            ["TFM-PARAM-001", "TFM-PARAM-002", "TFM-PARAM-003"],
        )
        self.assertEqual(boundary["loop_cardinality"], "np")
        self.assertEqual(boundary["absolute_valid_parameter_count"], 32755)
        self.assertEqual(boundary["standard_parameter_count"], 7)
        self.assertEqual(
            boundary["source_order"],
            ["declared_parameter_loop", "eof_check", "standard_zero_fill"],
        )
        self.assertEqual(
            boundary["excluded_reads"],
            ["eof_state", "raw_suffix", "final_adjustments"],
        )

    def test_parameter_source_contract_scalar_types_and_shapes_are_exact(
        self,
    ) -> None:
        for field_path, replacement, diagnostic in (
            (("schema_version",), 1.0, "schema scalar type"),
            (("schema_version",), True, "schema scalar type"),
            (
                ("proof_boundary", "absolute_valid_parameter_count"),
                32755.0,
                "proof boundary scalar types",
            ),
            (
                ("proof_boundary", "standard_parameter_count"),
                True,
                "proof boundary scalar types",
            ),
            (
                ("proof_boundary", "slant", "signed"),
                1,
                "proof boundary scalar types",
            ),
            (
                ("proof_boundary", "slant", "scale_by_effective_size"),
                0,
                "proof boundary scalar types",
            ),
            (
                (
                    "proof_boundary",
                    "zero_fill",
                    "retains_declared_above_standard_count",
                ),
                1,
                "proof boundary scalar types",
            ),
        ):
            changed = json.loads(json.dumps(parameter_source_contract()))
            target = changed
            for component in field_path[:-1]:
                target = target[component]
            target[field_path[-1]] = replacement
            with self.subTest(field_path=field_path):
                self.assertTrue(
                    any(
                        diagnostic in error
                        for error in validate_parameter_source_contract(
                            changed,
                            rule_transition_v4(),
                            extensible_source_contract(),
                        )
                    )
                )

        changed = json.loads(json.dumps(parameter_source_contract()))
        changed["proof_boundary"] = []
        self.assertIn(
            "parameter source contract proof boundary is invalid",
            validate_parameter_source_contract(
                changed,
                rule_transition_v4(),
                extensible_source_contract(),
            ),
        )

        changed = json.loads(json.dumps(parameter_source_contract()))
        changed["focused_source"] = []
        self.assertIn(
            "parameter source contract focused source is invalid",
            validate_parameter_source_contract(
                changed,
                rule_transition_v4(),
                extensible_source_contract(),
            ),
        )

        changed = json.loads(json.dumps(parameter_source_contract()))
        changed["proof_boundary"]["slant"] = None
        self.assertIn(
            "parameter source contract proof boundary shapes are invalid",
            validate_parameter_source_contract(
                changed,
                rule_transition_v4(),
                extensible_source_contract(),
            ),
        )

    def test_extensible_red_evidence_is_content_addressed_and_exact(self) -> None:
        evidence = EXTENSIBLE_TDD_RED_EVIDENCE.read_text(encoding="utf-8")
        for required in (
            "3883c90f95865262df95f4073f705189f52316e354a1b5fcde51f39948076a60",
            "325902c3d9e130ccf277ef252ac64275c6a871969c808d384dd72029b2d146f2",
            "2bb6c60960cc660e0b73f7a604461569c467a16817c4c15c97b11abd43d32b2e",
            "603949a341a50e9b81b41a29074197c9b5bd33e29337d6146663edd866f80768",
            "no `CheckedExtensibleRecipe` in `tfm_validation`",
            "no `ExtensibleCheckedTfm` in `tfm_validation`",
            "no `ExtensiblePart` in `tfm_validation`",
            "no `ExtensibleValidationRule` in `tfm_validation`",
            "no `check_extensibles` in `tfm_validation`",
            'left: ["check_preamble_header", "check_characters", "check_boxes", "check_lig_kern", "check_kerns"]',
            "No non-building RED commit was created",
        ):
            self.assertIn(required, evidence)

    def test_private_extensible_implementation_evidence_is_documented(self) -> None:
        for relative_path in (
            "PLAN.md",
            "docs/m13-3-dp1-scan-context.md",
            "docs/tex82-read-font-info-validation-rules.md",
            "docs/tex82-read-font-info-extensibles.md",
        ):
            document = (ROOT / relative_path).read_text(encoding="utf-8")
            for required in (
                "`ne=32753`",
                "`ne=32755`",
                "docs/evidence/tex-tfm-extensible-tdd-red-v1.md",
                "parameter",
            ):
                self.assertIn(required, document, relative_path)

    def test_extensible_source_contract_pins_exact_successor_boundary(self) -> None:
        contract = extensible_source_contract()
        self.assertEqual(
            validate_extensible_source_contract(
                contract,
                rule_transition_v3(),
                kern_source_contract(),
            ),
            [],
        )
        self.assertEqual(contract["schema_version"], 1)
        self.assertEqual(
            contract["predecessors"],
            {
                "ownership_transition": {
                    "path": "tfm-validation-rule-transition-v3.json",
                    "schema_version": 3,
                    "raw_sha256": "5929817fa92f3f8ead2a05ba33476281bb16ab5661eef5926730fe6fa27ce09d",
                    "canonical_sha256": "3206379d5f6f6748c2d532da83df565a187aee2077e936a67672336d10569ccf",
                },
                "input_source_contract": {
                    "path": "tfm-kern-source-contract-v1.json",
                    "schema_version": 1,
                    "raw_sha256": "19d08087ce4b96bc4e3e9059e161adfd4705157e5a7e768190695155b7c9b2a1",
                    "canonical_sha256": "754519a85d9479c616fc2a246d6c584f839b617f43c68c4b3fa55c486e3a0b74",
                },
            },
        )
        self.assertEqual(
            contract["focused_source"],
            {
                "compatibility_source_sha256": "c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324",
                "check_existence_section": {
                    "lines": "11150..11154",
                    "sha256": "50b7893997fe98c90314983b83456c0fa15f577d02e91e2a03cf2a8034765c63",
                },
                "extensible_recipe_section": {
                    "lines": "11176..11183",
                    "sha256": "c155058da84f06e687bd1cf226e3fc9900280abb1e4e60783360cb31f8f0c7cc",
                },
            },
        )
        self.assertEqual(
            contract["proof_boundary"],
            {
                "input": "KernCheckedTfm",
                "output": "ExtensibleCheckedTfm",
                "owned_rule_ids": ["TFM-EXT-001", "TFM-EXT-002"],
                "loop_cardinality": "ne",
                "absolute_valid_recipe_count": 32753,
                "field_order": ["top", "middle", "bottom", "repeat"],
                "recipe_fields": [
                    {
                        "source_field": "a",
                        "semantic": "top",
                        "zero_semantics": "absent_optional",
                    },
                    {
                        "source_field": "b",
                        "semantic": "middle",
                        "zero_semantics": "absent_optional",
                    },
                    {
                        "source_field": "c",
                        "semantic": "bottom",
                        "zero_semantics": "absent_optional",
                    },
                    {
                        "source_field": "d",
                        "semantic": "repeat",
                        "zero_semantics": "mandatory_character_code",
                    },
                ],
                "reads": ["character_existence", "extensibles"],
                "excluded_reads": ["parameters", "raw_suffix"],
                "scaling": False,
                "referenced_only": False,
            },
        )

    def test_extensible_source_contract_rejects_predecessor_source_and_scope_drift(
        self,
    ) -> None:
        for field, replacement in (
            ("predecessors", {}),
            ("focused_source", {}),
            ("proof_boundary", {}),
        ):
            changed = extensible_source_contract()
            changed[field] = replacement
            self.assertTrue(
                validate_extensible_source_contract(
                    changed,
                    rule_transition_v3(),
                    kern_source_contract(),
                ),
                field,
            )

    def test_kern_source_contract_pins_exact_successor_boundary(self) -> None:
        contract = kern_source_contract()
        self.assertEqual(
            validate_kern_source_contract(contract, rule_transition()), []
        )
        self.assertEqual(contract["schema_version"], 1)
        self.assertEqual(
            contract["predecessor"],
            {
                "path": "tfm-validation-rule-transition-v2.json",
                "schema_version": 2,
                "raw_sha256": "4a0bb1453055d12037fbbab0c77999feaf9b24f2d71b7e8afeb38453d2788316",
                "canonical_sha256": "773983c0d5a99c21067f79edd887db792092c4d59efb8d9c0af2c478fb5c00fc",
            },
        )
        self.assertEqual(
            contract["focused_source"],
            {
                "compatibility_source_sha256": "c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324",
                "fix_word_scaling_section": {
                    "lines": "11108..11130",
                    "sha256": "306907b8734bfa4dc990546e1fb84d0158c2b9af2338faed18808a06c4bfa58e",
                },
                "scale_normalization_section": {
                    "lines": "11142..11148",
                    "sha256": "e4db0f873ddda4dc750831a8ddcb436bb44dae7cb41044314837a1895a9c1906",
                },
                "kern_loop_section": {
                    "lines": "11173..11174",
                    "sha256": "d1b13b62579f82c3fec9ea7fbf275c751ea1e7eb31a02c2d703233c7c84760f1",
                },
            },
        )
        self.assertEqual(
            contract["proof_boundary"],
            {
                "input": "LigKernCheckedTfm",
                "output": "KernCheckedTfm",
                "owned_rule_ids": ["TFM-KERN-001"],
                "loop_cardinality": "nk",
                "reads": ["effective_size", "kerns"],
                "excluded_reads": ["extensibles", "parameters", "raw_suffix"],
                "entry_zero_check": False,
            },
        )

    def test_kern_source_contract_rejects_predecessor_source_and_scope_drift(
        self,
    ) -> None:
        for field, replacement in (
            ("predecessor", {}),
            ("focused_source", {}),
            ("proof_boundary", {}),
        ):
            changed = kern_source_contract()
            changed[field] = replacement
            self.assertTrue(
                validate_kern_source_contract(changed, rule_transition()),
                field,
            )

    def test_ligkern_replacement_review_and_kern_contract_are_documented(
        self,
    ) -> None:
        for relative_path in (
            "PLAN.md",
            "docs/m13-3-dp1-scan-context.md",
            "docs/tex82-read-font-info-validation-rules.md",
            "docs/tex82-read-font-info-lig-kern.md",
        ):
            document = (ROOT / relative_path).read_text(encoding="utf-8")
            for required in (
                "6a93bc49-6f74-83ee-b517-7f02fcebb9f9",
                "PROCEED_PRIVATE_TFM_KERN",
                "confidence 0.93",
                "tfm-kern-source-contract-v1.json",
                "lines 11108..11130",
                "306907b8734bfa4dc990546e1fb84d0158c2b9af2338faed18808a06c4bfa58e",
                "lines 11142..11148",
                "e4db0f873ddda4dc750831a8ddcb436bb44dae7cb41044314837a1895a9c1906",
                "lines 11173..11174",
                "d1b13b62579f82c3fec9ea7fbf275c751ea1e7eb31a02c2d703233c7c84760f1",
                "whole `nk` table",
                "no entry-zero check",
            ):
                self.assertIn(required, document, relative_path)

    def test_kern_red_evidence_is_content_addressed_and_exact(self) -> None:
        evidence = KERN_TDD_RED_EVIDENCE.read_text(encoding="utf-8")
        for required in (
            "fa3cbfd93cd19b47182be11b1bfa382b8fe4da29f373c55461c3a25d348b5074",
            "b894741a032c1438cc18462d9e9b38e9a3739aa01649d85c05e193f2e252e947",
            "cannot find type `KernCheckedTfm` in this scope",
            "cannot find type `KernValidationRule` in this scope",
            "cannot find value `check_kerns` in this scope",
            'left: ["check_preamble_header", "check_characters", "check_boxes", "check_lig_kern"]',
            "missed non-private syntax in #[forge] struct KernCheckedTfm;",
            "No non-building RED commit was created",
        ):
            self.assertIn(required, evidence)

    def test_private_kern_implementation_evidence_is_documented(self) -> None:
        for relative_path in (
            "PLAN.md",
            "docs/m13-3-dp1-scan-context.md",
            "docs/tex82-read-font-info-validation-rules.md",
            "docs/tex82-read-font-info-lig-kern.md",
        ):
            document = (ROOT / relative_path).read_text(encoding="utf-8")
            for required in (
                "private `KernCheckedTfm` implementation",
                "254 forbidden signs",
                "21 effective sizes × 10 fix words",
                "32,755-word absolute kern maximum",
                "all `TailCheckedTfm` witnesses",
                "same raw allocation",
                "no entry-zero check",
                "production `include!`",
                "unapproved proof-state attributes",
                "docs/evidence/tex-tfm-kern-tdd-red-v1.md",
                "fa3cbfd93cd19b47182be11b1bfa382b8fe4da29f373c55461c3a25d348b5074",
                "b894741a032c1438cc18462d9e9b38e9a3739aa01649d85c05e193f2e252e947",
                "dedicated kern closure review",
            ):
                self.assertIn(required, document, relative_path)

    def test_standalone_gate_reports_transitioned_proof_ownership(self) -> None:
        output = io.StringIO()
        with redirect_stdout(output):
            self.assertEqual(main(), 0)
        self.assertIn("'LigKernCheckedTfm': 8", output.getvalue())
        self.assertIn("'KernCheckedTfm': 1", output.getvalue())
        self.assertIn("'ExtensibleCheckedTfm': 2", output.getvalue())
        self.assertIn("'ParameterCheckedTfm': 3", output.getvalue())
        self.assertIn("'TailCheckedTfm': 0", output.getvalue())
        self.assertIn("transition_chain=v2->v3->v4", output.getvalue())
        self.assertIn("extensible_source_contract=v1", output.getvalue())
        self.assertIn("parameter_source_contract=v1", output.getvalue())

    def test_v3_transition_moves_only_extensible_rules_from_effective_owner(
        self,
    ) -> None:
        transition = rule_transition_v3()
        self.assertEqual(
            validate_rule_transition_v3(transition, rule_transition()), []
        )
        self.assertEqual(transition["schema_version"], 3)
        self.assertEqual(transition["proof_states_added"], ["ExtensibleCheckedTfm"])
        self.assertEqual(
            transition["ownership_changes"],
            [
                {
                    "rule_id": "TFM-EXT-001",
                    "from": "TailCheckedTfm",
                    "to": "ExtensibleCheckedTfm",
                },
                {
                    "rule_id": "TFM-EXT-002",
                    "from": "TailCheckedTfm",
                    "to": "ExtensibleCheckedTfm",
                },
            ],
        )
        self.assertEqual(
            transition["source_predicate_projections"],
            [
                {
                    "source_predicate": "optional_part_character_existence",
                    "runtime_projection": "OptionalPartMissing",
                    "rule_ids": ["TFM-EXT-001"],
                },
                {
                    "source_predicate": "repeat_character_existence",
                    "runtime_projection": "RepeatMissing",
                    "rule_ids": ["TFM-EXT-002"],
                },
            ],
        )

    def test_v4_transition_moves_only_parameter_rules_from_effective_owner(
        self,
    ) -> None:
        transition = rule_transition_v4()
        self.assertEqual(
            validate_rule_transition_v4(transition, rule_transition_v3()), []
        )
        self.assertEqual(type(transition["schema_version"]), int)
        self.assertEqual(transition["schema_version"], 4)
        self.assertEqual(transition["proof_states_added"], ["ParameterCheckedTfm"])
        self.assertEqual(
            transition["ownership_changes"],
            [
                {
                    "rule_id": rule_id,
                    "from": "TailCheckedTfm",
                    "to": "ParameterCheckedTfm",
                }
                for rule_id in (
                    "TFM-PARAM-001",
                    "TFM-PARAM-002",
                    "TFM-PARAM-003",
                )
            ],
        )
        self.assertEqual(
            transition["source_predicate_projections"],
            [
                {
                    "source_predicate": "slant_signed_pure_number",
                    "runtime_projection": "SignedSlant",
                    "rule_ids": ["TFM-PARAM-001"],
                },
                {
                    "source_predicate": "non_slant_store_scaled",
                    "runtime_projection": "ScaledParameter",
                    "rule_ids": ["TFM-PARAM-002"],
                },
                {
                    "source_predicate": (
                        "whole_np_iteration_then_standard_zero_fill"
                    ),
                    "runtime_projection": "CompleteParameterTable",
                    "rule_ids": ["TFM-PARAM-003"],
                },
            ],
        )

    def test_v4_transition_scalar_types_and_shape_diagnostics_are_exact(self) -> None:
        for field_path in (
            ("schema_version",),
            ("predecessor", "schema_version"),
        ):
            for replacement in (4.0, True):
                changed = json.loads(json.dumps(rule_transition_v4()))
                target = changed
                for component in field_path[:-1]:
                    target = target[component]
                target[field_path[-1]] = replacement
                with self.subTest(field_path=field_path, replacement=replacement):
                    self.assertIn(
                        "v4 transition schema scalar type is invalid",
                        validate_rule_transition_v4(changed, rule_transition_v3()),
                    )

        changed = json.loads(json.dumps(rule_transition_v4()))
        changed["source_predicate_projections"][0]["rule_ids"] = {}
        self.assertIn(
            "v4 transition projected rule ids are invalid",
            validate_rule_transition_v4(changed, rule_transition_v3()),
        )

    def test_transition_chain_rejects_omission_reordering_and_invalid_moves(
        self,
    ) -> None:
        v2 = rule_transition()
        v3 = rule_transition_v3()
        v4 = rule_transition_v4()
        self.assertEqual(validate_transition_chain([v2, v3, v4], rule_contract()), [])

        for changed_chain in ([v3, v4], [v3, v2, v4], [v2, v4, v3]):
            self.assertTrue(validate_transition_chain(changed_chain, rule_contract()))

        wrong_owner = json.loads(json.dumps(v3))
        wrong_owner["ownership_changes"][0]["from"] = "LigKernCheckedTfm"
        errors = validate_transition_chain([v2, wrong_owner, v4], rule_contract())
        self.assertTrue(any("current effective owner" in error for error in errors))

        duplicate_in_v3 = json.loads(json.dumps(v3))
        duplicate_in_v3["ownership_changes"].append(
            json.loads(json.dumps(duplicate_in_v3["ownership_changes"][0]))
        )
        errors = validate_transition_chain([v2, duplicate_in_v3, v4], rule_contract())
        self.assertTrue(any("duplicate ownership move" in error for error in errors))

        moved_twice = json.loads(json.dumps(v3))
        moved_twice["ownership_changes"].append(
            {
                "rule_id": "TFM-KERN-001",
                "from": "KernCheckedTfm",
                "to": "ExtensibleCheckedTfm",
            }
        )
        errors = validate_transition_chain([v2, moved_twice, v4], rule_contract())
        self.assertTrue(any("already moved by an earlier transition" in error for error in errors))

        predecessor_drift = json.loads(json.dumps(v3))
        predecessor_drift["predecessor"] = {}
        self.assertTrue(
            validate_transition_chain([v2, predecessor_drift, v4], rule_contract())
        )

    def test_transition_projection_shapes_return_controlled_errors(self) -> None:
        validators = (
            ("v2", rule_transition, rule_contract, validate_rule_transition),
            (
                "v3",
                rule_transition_v3,
                rule_transition,
                validate_rule_transition_v3,
            ),
            (
                "v4",
                rule_transition_v4,
                rule_transition_v3,
                validate_rule_transition_v4,
            ),
        )
        for version, transition_factory, predecessor_factory, validator in validators:
            for malformed in (None, 1, []):
                with self.subTest(version=version, top_level=malformed):
                    self.assertTrue(validator(malformed, predecessor_factory()))
                with self.subTest(version=version, predecessor=malformed):
                    self.assertTrue(validator(transition_factory(), malformed))

            for rule_ids in (None, 1, {}, [{}]):
                transition = json.loads(json.dumps(transition_factory()))
                transition["source_predicate_projections"][0]["rule_ids"] = rule_ids
                with self.subTest(version=version, rule_ids=rule_ids):
                    self.assertTrue(validator(transition, predecessor_factory()))

            transition = json.loads(json.dumps(transition_factory()))
            transition["source_predicate_projections"][0] = []
            with self.subTest(version=version, projection="non-object"):
                self.assertTrue(validator(transition, predecessor_factory()))

    def test_transition_chain_shapes_return_controlled_errors(self) -> None:
        for malformed in (None, 1, []):
            with self.subTest(top_level=malformed):
                self.assertTrue(validate_transition_chain(malformed, rule_contract()))

        for malformed in (None, 1, []):
            with self.subTest(transition_entry=malformed):
                self.assertTrue(
                    validate_transition_chain(
                        [rule_transition(), malformed], rule_contract()
                    )
                )

        for field in ("rule_id", "from", "to"):
            malformed_move = json.loads(json.dumps(rule_transition_v3()))
            malformed_move["ownership_changes"][0][field] = {}
            with self.subTest(ownership_field=field):
                self.assertTrue(
                    validate_transition_chain(
                        [rule_transition(), malformed_move], rule_contract()
                    )
                )

        malformed_move = json.loads(json.dumps(rule_transition_v3()))
        malformed_move["ownership_changes"][0] = []
        self.assertTrue(
            validate_transition_chain(
                [rule_transition(), malformed_move], rule_contract()
            )
        )

    def test_rule_contract_shapes_return_controlled_errors(self) -> None:
        for malformed in (None, 1, []):
            with self.subTest(top_level=malformed):
                self.assertTrue(validate_rule_contract(malformed, fixture_case_ids()))

        mutations = (
            ("rule id object", ("rules", 0, "id"), {}),
            ("rule id array", ("rules", 0, "id"), []),
            ("invariant object", ("invariants", 0), {}),
            ("proof state object", ("proof_states", 0), {}),
            ("dependency object", ("rules", 0, "dependency_ids", 0), {}),
            ("witness object", ("rules", 0, "witnesses", 0), {}),
            ("owner object", ("rules", 0, "proof_state"), {}),
        )
        for name, path, replacement in mutations:
            changed = json.loads(json.dumps(rule_contract()))
            target = changed
            for component in path[:-1]:
                target = target[component]
            target[path[-1]] = replacement
            with self.subTest(name=name):
                self.assertTrue(validate_rule_contract(changed, fixture_case_ids()))

    def test_contract_consumers_return_controlled_errors_for_non_objects(self) -> None:
        for malformed in (None, 1, []):
            with self.subTest(validator="kern source", value=malformed):
                self.assertTrue(
                    validate_kern_source_contract(malformed, rule_transition())
                )
            with self.subTest(validator="extensible source", value=malformed):
                self.assertTrue(
                    validate_extensible_source_contract(
                        malformed,
                        rule_transition_v3(),
                        kern_source_contract(),
                    )
                )
            with self.subTest(validator="parameter source", value=malformed):
                self.assertTrue(
                    validate_parameter_source_contract(
                        malformed,
                        rule_transition_v4(),
                        extensible_source_contract(),
                    )
                )
                self.assertTrue(
                    validate_parameter_source_contract(
                        parameter_source_contract(),
                        malformed,
                        extensible_source_contract(),
                    )
                )
                self.assertTrue(
                    validate_parameter_source_contract(
                        parameter_source_contract(),
                        rule_transition_v4(),
                        malformed,
                    )
                )
            if malformed is not None:
                with self.subTest(validator="ledger", value=malformed):
                    self.assertTrue(
                        validate_rule_ledger(
                            ledger_text(), fixture_case_ids(), malformed
                        )
                    )

    def test_v2_transition_pins_ligkern_source_and_splits_only_kern_owner(
        self,
    ) -> None:
        transition = rule_transition()
        self.assertEqual(validate_rule_transition(transition, rule_contract()), [])
        self.assertEqual(transition["schema_version"], 2)
        self.assertEqual(
            transition["focused_source"]["check_existence_section"],
            {
                "lines": "11150..11154",
                "sha256": "50b7893997fe98c90314983b83456c0fa15f577d02e91e2a03cf2a8034765c63",
            },
        )
        self.assertEqual(
            transition["focused_source"]["lig_kern_instruction_section"],
            {
                "lines": "11156..11172",
                "sha256": "a105c3b6349d6ad4c15e37f3cc0d8b64670c14ffc3f79cdd827da05043d28c5d",
            },
        )
        self.assertEqual(transition["proof_states_added"], ["KernCheckedTfm"])
        self.assertEqual(
            transition["ownership_changes"],
            [
                {
                    "rule_id": "TFM-KERN-001",
                    "from": "LigKernCheckedTfm",
                    "to": "KernCheckedTfm",
                }
            ],
        )
        self.assertEqual(
            transition["source_predicate_projections"],
            [
                {
                    "source_predicate": "empty_instruction_table",
                    "runtime_projection": "AcceptedEmptyInstructionTable",
                    "rule_ids": ["TFM-LIGKERN-001"],
                },
                {
                    "source_predicate": "restart_target_range_check",
                    "runtime_projection": "RestartTargetOutOfRange",
                    "rule_ids": ["TFM-LIGKERN-002", "TFM-LIGKERN-008"],
                },
                {
                    "source_predicate": "first_boundary_character_marker",
                    "runtime_projection": "AcceptedBoundaryCharacter",
                    "rule_ids": ["TFM-LIGKERN-003"],
                },
                {
                    "source_predicate": "ordinary_next_character_existence",
                    "runtime_projection": "NextCharacterMissing",
                    "rule_ids": ["TFM-LIGKERN-004"],
                },
                {
                    "source_predicate": "ligature_replacement_existence",
                    "runtime_projection": "LigatureTargetMissing",
                    "rule_ids": ["TFM-LIGKERN-005"],
                },
                {
                    "source_predicate": "kern_index_range_check",
                    "runtime_projection": "KernIndexOutOfRange",
                    "rule_ids": ["TFM-LIGKERN-006"],
                },
                {
                    "source_predicate": "forward_skip_range_check",
                    "runtime_projection": "ForwardSkipOutOfRange",
                    "rule_ids": ["TFM-LIGKERN-007"],
                },
            ],
        )
        document = LIG_KERN_SOURCE_CONTRACT.read_text(encoding="utf-8")
        for required in (
            "lines 11150..11154",
            "50b7893997fe98c90314983b83456c0fa15f577d02e91e2a03cf2a8034765c63",
            "lines 11156..11172",
            "a105c3b6349d6ad4c15e37f3cc0d8b64670c14ffc3f79cdd827da05043d28c5d",
            "`LigKernCheckedTfm`",
            "`KernCheckedTfm`",
            "must not scale kern fix words",
        ):
            self.assertIn(required, document)
        for relative_path in (
            "PLAN.md",
            "docs/m13-3-dp1-scan-context.md",
            "docs/tex82-read-font-info-validation-rules.md",
        ):
            summary = (ROOT / relative_path).read_text(encoding="utf-8")
            self.assertIn("tfm-validation-rule-transition-v2.json", summary)
            self.assertIn(
                "a105c3b6349d6ad4c15e37f3cc0d8b64670c14ffc3f79cdd827da05043d28c5d",
                summary,
            )
            self.assertIn("docs/tex82-read-font-info-lig-kern.md", summary)

    def test_v2_transition_rejects_source_or_ownership_drift(self) -> None:
        for field, replacement in (
            ("focused_source", {}),
            ("proof_states_added", []),
            ("ownership_changes", []),
            ("source_predicate_projections", []),
        ):
            changed = rule_transition()
            changed[field] = replacement
            self.assertTrue(
                validate_rule_transition(changed, rule_contract()),
                field,
            )

    def test_private_lig_kern_implementation_evidence_is_documented(self) -> None:
        for relative_path in (
            "PLAN.md",
            "docs/m13-3-dp1-scan-context.md",
            "docs/tex82-read-font-info-validation-rules.md",
            "docs/tex82-read-font-info-lig-kern.md",
        ):
            document = (ROOT / relative_path).read_text(encoding="utf-8")
            for required in (
                "private `LigKernCheckedTfm` implementation",
                "83/83 persisted corpus phase outcomes",
                "8/8 exact lig/kern-owned rejections",
                "4,096 generated programs",
                "32,755-instruction absolute maximum",
                "kern words remain unread and unscaled",
                "exactly one production construction",
                "6a93b53b-e6b0-83ee-92f5-686badb00774",
                "REVISE_PRIVATE_TFM_LIGKERN",
                "unsafe `ptr::read`",
                "source_predicate_projections",
                "count-1/count",
            ):
                self.assertIn(required, document, relative_path)

    def test_header_closure_review_authorizes_only_private_character_phase(
        self,
    ) -> None:
        for path in (
            ROOT / "PLAN.md",
            ROOT / "docs/tex82-read-font-info-validation-rules.md",
        ):
            document = path.read_text(encoding="utf-8")
            self.assertIn("6a8e45fc-4bd0-83ee-b4f8-e2c948311ae1", document)
            self.assertIn("PROCEED_PRIVATE_TFM_CHARACTER", document)

    def test_private_character_implementation_evidence_is_documented(self) -> None:
        for path in (
            ROOT / "PLAN.md",
            ROOT / "docs/m13-3-dp1-scan-context.md",
            ROOT / "docs/tex82-read-font-info-validation-rules.md",
        ):
            document = path.read_text(encoding="utf-8")
            self.assertIn("private `CharacterCheckedTfm` implementation", document)
            self.assertIn("10/10 exact character-owned rejections", document)
            self.assertIn("domains `1..=5`", document)
            self.assertIn("lig/kern remains blocked", document)

    def test_character_closure_review_authorizes_only_private_box_phase(
        self,
    ) -> None:
        for path in (
            ROOT / "PLAN.md",
            ROOT / "docs/m13-3-dp1-scan-context.md",
            ROOT / "docs/tex82-read-font-info-validation-rules.md",
        ):
            document = path.read_text(encoding="utf-8")
            self.assertIn("6a939670-0fc8-83e8-923f-ebaed26b4c72", document)
            self.assertIn("PROCEED_PRIVATE_TFM_BOX", document)
            self.assertIn("private `BoxCheckedTfm` implementation", document)
            self.assertIn("lig/kern remains blocked", document)

    def test_character_closure_hardening_evidence_is_documented(self) -> None:
        for path in (
            ROOT / "PLAN.md",
            ROOT / "docs/m13-3-dp1-scan-context.md",
            ROOT / "docs/tex82-read-font-info-validation-rules.md",
        ):
            document = path.read_text(encoding="utf-8")
            self.assertIn("four adjacent metric precedence pairs", document)
            self.assertIn("`CharListTraversalLimit` remains unreachable", document)
            self.assertIn("AST negative mutants", document)
            self.assertIn(
                "db680c23a099b5b39c484d34c357116fc8d6967a9151db4108af0ddf4cfbb0be",
                document,
            )
            self.assertIn(
                "9df44bf4b157acfb65fa0d5cc7de4d42ba7f869bae460e07daf984e1fbca19b4",
                document,
            )
            self.assertIn("one required CI job", document)

    def test_contract_records_machine_ledger_gate(self) -> None:
        contract = (ROOT / "docs/m13-3-dp1-scan-context.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("check_tfm_validation_ledger.py", contract)
        self.assertIn("tfm-validation-rules-v1.json", contract)
        self.assertIn("33/33 semantic rule cells", contract)
        self.assertIn("83/83", contract)
        self.assertIn("source ordinal", contract)
        self.assertIn("proof ownership", contract)
        self.assertIn("6a8e358c-697c-83e8-a6ba-881a469553d7", contract)
        self.assertIn("REVISE_PRIVATE_TFM_HEADER", contract)

    def test_ci_runs_machine_ledger_policy(self) -> None:
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertIn("scripts.tests.test_check_tfm_validation_ledger", workflow)
        ledger_gate = "python3 scripts/check_tfm_validation_ledger.py"
        native_oracle = "python3 scripts/check_tfm_validity_oracle.py"
        rust_suite = "run: cargo test -q"
        self.assertIn(ledger_gate, workflow)
        self.assertLess(workflow.index(ledger_gate), workflow.index(native_oracle))
        self.assertLess(workflow.index(native_oracle), workflow.index(rust_suite))

    def test_repository_ledger_has_exact_order_unique_rules_and_complete_witnesses(
        self,
    ) -> None:
        self.assertEqual(validate_rule_ledger(ledger_text(), fixture_case_ids()), [])

    def test_duplicate_rule_id_is_rejected(self) -> None:
        lines = ledger_text().splitlines()
        row_index = next(
            index for index, line in enumerate(lines) if line.startswith("| `TFM-SIZE-001`")
        )
        lines.insert(row_index + 1, lines[row_index])

        errors = validate_rule_ledger("\n".join(lines), fixture_case_ids())
        self.assertTrue(any("duplicate rule ids" in error for error in errors))

    def test_source_order_swaps_are_rejected(self) -> None:
        lines = ledger_text().splitlines()
        count_index = next(
            index for index, line in enumerate(lines) if line.startswith("| `TFM-COUNT-001`")
        )
        header_index = next(
            index for index, line in enumerate(lines) if line.startswith("| `TFM-HEADER-001`")
        )
        lines[count_index], lines[header_index] = lines[header_index], lines[count_index]

        errors = validate_rule_ledger("\n".join(lines), fixture_case_ids())
        self.assertTrue(any("source rule order" in error for error in errors))

    def test_semantic_rule_cells_cannot_be_reassigned_between_ids(self) -> None:
        lines = ledger_text().splitlines()
        header_index = next(
            index for index, line in enumerate(lines) if line.startswith("| `TFM-HEADER-001`")
        )
        charlist_index = next(
            index
            for index, line in enumerate(lines)
            if line.startswith("| `TFM-CHARLIST-001`")
        )
        header_cells = [
            cell.strip() for cell in lines[header_index].strip().strip("|").split("|")
        ]
        charlist_cells = [
            cell.strip()
            for cell in lines[charlist_index].strip().strip("|").split("|")
        ]
        for cell_index in (1, 2, 3):
            header_cells[cell_index], charlist_cells[cell_index] = (
                charlist_cells[cell_index],
                header_cells[cell_index],
            )
        lines[header_index] = "| " + " | ".join(header_cells) + " |"
        lines[charlist_index] = "| " + " | ".join(charlist_cells) + " |"

        errors = validate_rule_ledger("\n".join(lines), fixture_case_ids())
        self.assertTrue(any("semantic contract" in error for error in errors))

    def test_nonempty_dependency_cells_cannot_be_swapped(self) -> None:
        lines = ledger_text().splitlines()
        first_index = next(
            index for index, line in enumerate(lines) if line.startswith("| `TFM-HEADER-001`")
        )
        second_index = next(
            index
            for index, line in enumerate(lines)
            if line.startswith("| `TFM-CHARLIST-001`")
        )
        first = [cell.strip() for cell in lines[first_index].strip().strip("|").split("|")]
        second = [cell.strip() for cell in lines[second_index].strip().strip("|").split("|")]
        first[2], second[2] = second[2], first[2]
        lines[first_index] = "| " + " | ".join(first) + " |"
        lines[second_index] = "| " + " | ".join(second) + " |"

        errors = validate_rule_ledger("\n".join(lines), fixture_case_ids())
        self.assertTrue(any("semantic contract" in error for error in errors))

    def test_complete_witness_cells_cannot_be_swapped(self) -> None:
        lines = ledger_text().splitlines()
        first_index = next(
            index for index, line in enumerate(lines) if line.startswith("| `TFM-HEADER-001`")
        )
        second_index = next(
            index
            for index, line in enumerate(lines)
            if line.startswith("| `TFM-CHARLIST-001`")
        )
        first = [cell.strip() for cell in lines[first_index].strip().strip("|").split("|")]
        second = [cell.strip() for cell in lines[second_index].strip().strip("|").split("|")]
        first[3], second[3] = second[3], first[3]
        lines[first_index] = "| " + " | ".join(first) + " |"
        lines[second_index] = "| " + " | ".join(second) + " |"

        errors = validate_rule_ledger("\n".join(lines), fixture_case_ids())
        self.assertTrue(any("semantic contract" in error for error in errors))

    def test_contract_pins_source_hashes_and_all_rule_semantics(self) -> None:
        contract = rule_contract()
        self.assertEqual(contract["schema_version"], 1)
        self.assertEqual(
            contract["compatibility_source"]["sha256"],
            "c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324",
        )
        self.assertEqual(
            contract["compatibility_source"]["loader_section_sha256"],
            "57f665ae4cc87c721d444fdde0a1817f194f44bab18388c42a1d26d830c6ddc8",
        )
        self.assertEqual(len(contract["rules"]), 33)
        for rule in contract["rules"]:
            self.assertIn("predicate_sha256", rule)
            self.assertIn("dependency_ids", rule)
            self.assertIn("witnesses", rule)
            self.assertIn("proof_state", rule)
        rust_bridge = (
            ROOT / "crates/tex-tfm-metrics/src/tfm_validation.rs"
        ).read_text(encoding="utf-8")
        self.assertIn(
            "cebc062f771f27c5c46e0e83a74ab7c7c9f6e3a172b2cf1fe01bce0a7f6f6c21",
            rust_bridge,
        )
        self.assertIn("canonical v1 rule contract", rust_bridge)

    def test_eof_rules_are_source_late_but_header_proven(self) -> None:
        rules = {rule["id"]: rule for rule in rule_contract()["rules"]}
        self.assertGreater(
            rules["TFM-EOF-001"]["source_ordinal"],
            rules["TFM-PARAM-003"]["source_ordinal"],
        )
        self.assertEqual(rules["TFM-EOF-001"]["proof_state"], "HeaderCheckedTfm")
        self.assertEqual(rules["TFM-EOF-002"]["proof_state"], "HeaderCheckedTfm")

    def test_header_proof_ownership_cannot_be_moved_to_a_late_phase(self) -> None:
        changed = json.loads(json.dumps(rule_contract()))
        rules = {rule["id"]: rule for rule in changed["rules"]}
        rules["TFM-EOF-001"]["proof_state"] = "TailCheckedTfm"
        rules["TFM-PARAM-003"]["proof_state"] = "HeaderCheckedTfm"

        errors = validate_rule_contract(changed, fixture_case_ids())
        self.assertTrue(any("proof ownership" in error for error in errors))

    def test_character_proof_ownership_cannot_be_moved_to_a_late_phase(self) -> None:
        changed = json.loads(json.dumps(rule_contract()))
        rules = {rule["id"]: rule for rule in changed["rules"]}
        rules["TFM-CHAR-001"]["proof_state"] = "TailCheckedTfm"

        errors = validate_rule_contract(changed, fixture_case_ids())
        self.assertTrue(any("CharacterCheckedTfm proof ownership" in error for error in errors))

    def test_box_proof_ownership_cannot_be_moved_to_a_late_phase(self) -> None:
        changed = json.loads(json.dumps(rule_contract()))
        rules = {rule["id"]: rule for rule in changed["rules"]}
        rules["TFM-BOX-001"]["proof_state"] = "TailCheckedTfm"

        errors = validate_rule_contract(changed, fixture_case_ids())
        self.assertTrue(any("BoxCheckedTfm proof ownership" in error for error in errors))

    def test_reviewed_v1_contract_semantics_require_a_version_transition(self) -> None:
        changed = json.loads(json.dumps(rule_contract()))
        rules = {rule["id"]: rule for rule in changed["rules"]}
        rules["TFM-PARAM-001"]["predicate_sha256"] = "0" * 64

        errors = validate_rule_contract(changed, fixture_case_ids())
        self.assertTrue(any("reviewed v1 contract digest" in error for error in errors))

    def test_unknown_native_witness_is_rejected(self) -> None:
        changed = ledger_text().replace(
            "`valid_cmr10`", "`unknown_native_case`", 1
        )

        errors = validate_rule_ledger(changed, fixture_case_ids())
        self.assertTrue(any("unknown native witnesses" in error for error in errors))

    def test_missing_dependency_is_rejected(self) -> None:
        changed = ledger_text().replace(
            "| Effective size; upstream scanner state. |",
            "| |",
            1,
        )

        errors = validate_rule_ledger(changed, fixture_case_ids())
        self.assertTrue(any("missing dependencies" in error for error in errors))

    def test_unmapped_fixture_case_is_rejected(self) -> None:
        changed = ledger_text().replace("`valid_cmr10`", "valid control", 1)

        errors = validate_rule_ledger(changed, fixture_case_ids())
        self.assertTrue(any("unmapped fixture cases" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
