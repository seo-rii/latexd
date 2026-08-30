import copy
import hashlib
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from scripts.check_tfm_box_scaling_oracle import (
    BASE_TFM,
    CASE_SIZES_SP,
    COMPATIBILITY_SOURCE,
    EXPECTED_FIXTURE,
    FIX_WORD_CASES,
    REVIEWED_FIXTURE_SHA256,
    build_mutated_tfm,
    build_probe_source,
    main,
    parse_observations,
    run_oracle,
    validate_results,
)


ROOT = Path(__file__).parents[2]


class TfmBoxScalingOracleTests(unittest.TestCase):
    def test_ci_runs_policy_and_native_oracle_after_tex_install(self) -> None:
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")

        policy = "scripts.tests.test_check_tfm_box_scaling_oracle"
        oracle = "python3 scripts/check_tfm_box_scaling_oracle.py"
        artifact = "tfm-box-scaling-oracle"
        tex_install = "Install Computer Modern test fonts"
        rust_suite = "run: cargo test -q"
        self.assertIn(policy, workflow)
        self.assertIn(oracle, workflow)
        self.assertIn(artifact, workflow)
        self.assertLess(workflow.index(tex_install), workflow.index(oracle))
        self.assertLess(workflow.index(oracle), workflow.index(rust_suite))

    def test_matrix_freezes_normalization_boundaries_and_fix_words(self) -> None:
        self.assertEqual(
            CASE_SIZES_SP,
            {
                "size_1": 1,
                "size_2": 2,
                "size_15": 15,
                "size_16": 16,
                "size_17": 17,
                "size_65535": 65_535,
                "size_65536": 65_536,
                "size_65537": 65_537,
                "size_8388607": 8_388_607,
                "size_8388608": 8_388_608,
                "size_8388609": 8_388_609,
                "size_16777215": 16_777_215,
                "size_16777216": 16_777_216,
                "size_16777217": 16_777_217,
                "size_33554431": 33_554_431,
                "size_33554432": 33_554_432,
                "size_33554433": 33_554_433,
                "size_67108863": 67_108_863,
                "size_67108864": 67_108_864,
                "size_67108865": 67_108_865,
                "size_134217727": 134_217_727,
            },
        )
        self.assertEqual(
            FIX_WORD_CASES,
            {
                "zero": bytes.fromhex("00 00 00 00"),
                "least_positive": bytes.fromhex("00 00 00 01"),
                "sub_byte_carry": bytes.fromhex("00 00 00 ff"),
                "byte_carry": bytes.fromhex("00 00 01 00"),
                "below_one": bytes.fromhex("00 0f ff ff"),
                "one": bytes.fromhex("00 10 00 00"),
                "max_positive": bytes.fromhex("00 ff ff ff"),
                "least_negative": bytes.fromhex("ff ff ff ff"),
                "negative_one": bytes.fromhex("ff f0 00 00"),
                "negative_sixteen": bytes.fromhex("ff 00 00 00"),
            },
        )

    def test_mutation_binds_each_probe_character_to_all_four_metric_tables(self) -> None:
        base = BASE_TFM.read_bytes()
        mutated = build_mutated_tfm(base)
        counts = [int.from_bytes(mutated[index : index + 2], "big") for index in range(0, 24, 2)]
        _, lh, bc, ec, nw, nh, nd, ni, nl, _, _, _ = counts
        character_start = 4 * (6 + lh)
        character_count = ec - bc + 1
        width_start = character_start + 4 * character_count
        height_start = width_start + 4 * nw
        depth_start = height_start + 4 * nh
        italic_start = depth_start + 4 * nd

        self.assertEqual((nd, ni, nl), (11, 11, 81))
        self.assertEqual(mutated[width_start : width_start + 4], bytes(4))
        self.assertEqual(mutated[height_start : height_start + 4], bytes(4))
        self.assertEqual(mutated[depth_start : depth_start + 4], bytes(4))
        self.assertEqual(mutated[italic_start : italic_start + 4], bytes(4))
        for character, word in enumerate(FIX_WORD_CASES.values()):
            metric_index = character + 1
            record = character_start + 4 * (character - bc)
            self.assertEqual(
                mutated[record : record + 4],
                bytes([metric_index, metric_index << 4 | metric_index, metric_index << 2, 0]),
            )
            for table_start in (width_start, height_start, depth_start, italic_start):
                offset = table_start + 4 * metric_index
                self.assertEqual(mutated[offset : offset + 4], word)

    def test_probe_observes_four_metrics_for_every_word(self) -> None:
        source = build_probe_source(65_536)
        for case_id in FIX_WORD_CASES:
            for metric in ("width", "height", "depth", "italic"):
                self.assertIn(f"LATEXD-TFMBOX:{case_id}_{metric}=", source)
        self.assertEqual(source.count(r"\font\probe=latexdprobe at 65536sp"), 1)

    def test_parser_rejects_duplicate_observations(self) -> None:
        with self.assertRaisesRegex(ValueError, "duplicate"):
            parse_observations(
                "LATEXD-TFMBOX:one_width=1 LATEXD-TFMBOX:one_width=2"
            )

    def test_fixture_pins_source_matrix_and_native_results(self) -> None:
        self.assertEqual(
            hashlib.sha256(EXPECTED_FIXTURE.read_bytes()).hexdigest(),
            REVIEWED_FIXTURE_SHA256,
        )
        fixture = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(fixture["format"], "latexd.tfm-box-scaling-oracle")
        self.assertEqual(fixture["schema_version"], 1)
        self.assertEqual(fixture["compatibility_target"], "TeX82 via pdfTeX INITEX")
        self.assertEqual(fixture["compatibility_source"], COMPATIBILITY_SOURCE)
        self.assertEqual(fixture["case_sizes_sp"], CASE_SIZES_SP)
        self.assertEqual(fixture["fix_word_cases"], {key: value.hex() for key, value in FIX_WORD_CASES.items()})
        self.assertEqual(
            fixture["native_observation_projection"],
            {
                "width": "exact_scaled_sp",
                "height": "max_zero_exact_scaled_sp",
                "depth": "max_zero_exact_scaled_sp",
                "italic": "exact_scaled_sp",
            },
        )
        self.assertEqual(set(fixture["case_results"]), set(CASE_SIZES_SP))
        self.assertEqual(validate_results(fixture["case_results"], fixture), [])

        one_pt = fixture["case_results"]["size_65536"]["observations"]
        self.assertEqual(one_pt["one_width"], 65_536)
        self.assertEqual(one_pt["one_height"], 65_536)
        self.assertEqual(one_pt["one_depth"], 65_536)
        self.assertEqual(one_pt["one_italic"], 65_536)
        self.assertEqual(one_pt["least_negative_width"], -1)
        self.assertEqual(one_pt["least_negative_height"], 0)
        self.assertEqual(one_pt["least_negative_depth"], 0)
        self.assertEqual(one_pt["least_negative_italic"], -1)

    def test_box_closure_source_and_scope_are_documented(self) -> None:
        source_contract = (
            ROOT / "docs/tex82-read-font-info-box-scaling.md"
        ).read_text(encoding="utf-8")
        for evidence in (
            COMPATIBILITY_SOURCE["sha256"],
            COMPATIBILITY_SOURCE["box_scaling_sha256"],
            "`store_scaled`",
            "width/height/depth/italic",
            "21 effective sizes × 10 fix words",
            "negative height/depth",
            "`BoxCheckedTfm`",
            "lig/kern remains blocked",
        ):
            self.assertIn(evidence, source_contract)

        for relative_path in (
            "PLAN.md",
            "docs/m13-3-dp1-scan-context.md",
            "docs/tex82-read-font-info-validation-rules.md",
        ):
            document = (ROOT / relative_path).read_text(encoding="utf-8")
            self.assertIn("private `BoxCheckedTfm`", document, relative_path)
            self.assertIn("21 effective sizes × 10 fix words", document, relative_path)
            self.assertIn("exact `BoxCheckedTfm` proof ownership", document, relative_path)
            self.assertIn("lig/kern remains blocked", document, relative_path)

    def test_pro_closure_verdict_and_split_successors_are_documented(self) -> None:
        for relative_path in (
            "PLAN.md",
            "docs/m13-3-dp1-scan-context.md",
            "docs/tex82-read-font-info-validation-rules.md",
            "docs/tex82-read-font-info-box-scaling.md",
        ):
            document = (ROOT / relative_path).read_text(encoding="utf-8")
            self.assertIn(
                "6a93a948-81a8-83ee-8173-a0a58dbe1a08", document, relative_path
            )
            self.assertIn("PROCEED_PRIVATE_TFM_LIGKERN", document, relative_path)
            self.assertIn("confidence 0.95", document, relative_path)
            self.assertIn("exactly one production construction", document, relative_path)
            self.assertIn("base TFM SHA-256", document, relative_path)
            self.assertIn("`LigKernCheckedTfm`", document, relative_path)
            self.assertIn("`KernCheckedTfm`", document, relative_path)

    def test_validation_rejects_changed_missing_and_unexpected_results(self) -> None:
        fixture = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))
        changed = copy.deepcopy(fixture["case_results"])
        changed["size_65536"]["observations"]["one_width"] += 1
        self.assertTrue(any("size_65536" in item for item in validate_results(changed, fixture)))

        missing = copy.deepcopy(fixture["case_results"])
        del missing["size_1"]
        self.assertTrue(any("size_1" in item for item in validate_results(missing, fixture)))

        unexpected = copy.deepcopy(fixture["case_results"])
        unexpected["future_size"] = unexpected["size_1"]
        self.assertTrue(any("unexpected cases" in item for item in validate_results(unexpected, fixture)))

        for field, replacement in (
            ("compatibility_source", {}),
            ("case_sizes_sp", {}),
            ("fix_word_cases", {}),
            ("native_observation_projection", {}),
        ):
            changed_contract = copy.deepcopy(fixture)
            changed_contract[field] = replacement
            self.assertTrue(
                any(
                    field in item
                    for item in validate_results(
                        fixture["case_results"], changed_contract
                    )
                ),
                field,
            )

    def test_overwritten_base_font_drift_is_rejected_even_when_probe_bytes_match(
        self,
    ) -> None:
        fixture = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))
        base = BASE_TFM.read_bytes()
        changed = bytearray(base)
        character_start = 4 * (6 + int.from_bytes(base[2:4], "big"))
        changed[character_start] ^= 1
        self.assertEqual(build_mutated_tfm(bytes(changed)), build_mutated_tfm(base))

        errors = validate_results(
            fixture["case_results"], fixture, base_tfm=bytes(changed)
        )
        self.assertTrue(any("base TFM SHA-256" in error for error in errors))

    @unittest.skipUnless(shutil.which("pdftex"), "pdftex is required for the oracle")
    def test_pdftex_initex_matches_frozen_fixture(self) -> None:
        fixture = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(run_oracle("pdftex"), fixture["case_results"])

    @unittest.skipUnless(shutil.which("pdftex"), "pdftex is required for the oracle")
    def test_cli_records_reproducible_engine_and_probe_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            report_path = Path(temp) / "oracle.json"
            self.assertEqual(main(["--engine", "pdftex", "--report", str(report_path)]), 0)
            report = json.loads(report_path.read_text(encoding="utf-8"))

        self.assertEqual(report["format"], "latexd.tfm-box-scaling-oracle")
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual(report["compatibility_source"], COMPATIBILITY_SOURCE)
        self.assertIn("pdfTeX", report["engine"]["version"])
        self.assertTrue(Path(report["engine"]["path"]).is_absolute())
        self.assertEqual(len(report["engine"]["sha256"]), 64)
        self.assertEqual(report["expected_processes"], len(CASE_SIZES_SP))
        self.assertEqual(report["observed_processes"], len(CASE_SIZES_SP))
        self.assertEqual(report["base_tfm"]["sha256"], hashlib.sha256(BASE_TFM.read_bytes()).hexdigest())
        self.assertEqual(report["case_results"], json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))["case_results"])


if __name__ == "__main__":
    unittest.main()
