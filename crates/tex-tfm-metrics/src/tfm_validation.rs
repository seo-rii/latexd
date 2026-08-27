//! Staged, private TeX82 TFM validation phases.

#![allow(
    dead_code,
    reason = "the reviewed validator phases remain private and unreachable until compatibility closure"
)]

use std::{ops::Range, sync::Arc};

use sha2::{Digest, Sha256};

const TFM_PREAMBLE_BYTES: usize = 24;
const MIN_DESIGN_SIZE_FIX_WORD: i32 = 1 << 20;
const MAX_TEX_FONT_SIZE_SP: i32 = 1 << 27;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EffectiveSizeSp(i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawTfmDigest([u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameTfmDigest([u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DeclaredFrameEnd(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawCounts {
    lf: u16,
    lh: u16,
    bc: u16,
    ec: u16,
    nw: u16,
    nh: u16,
    nd: u16,
    ni: u16,
    nl: u16,
    nk: u16,
    ne: u16,
    np: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharacterDomain {
    Empty,
    Inclusive { first: u8, last: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TableLayout {
    header: Range<usize>,
    characters: Range<usize>,
    widths: Range<usize>,
    heights: Range<usize>,
    depths: Range<usize>,
    italics: Range<usize>,
    lig_kern: Range<usize>,
    kerns: Range<usize>,
    extensibles: Range<usize>,
    parameters: Range<usize>,
}

struct HeaderCheckedTfm {
    raw: Arc<[u8]>,
    effective_size: EffectiveSizeSp,
    raw_counts: RawCounts,
    character_domain: CharacterDomain,
    layout: TableLayout,
    declared_frame_end: DeclaredFrameEnd,
    raw_digest: RawTfmDigest,
    frame_digest: FrameTfmDigest,
    design_size_fix_word: i32,
    design_size_sp: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CountField {
    Lf,
    Lh,
    Bc,
    Ec,
    Nw,
    Nh,
    Nd,
    Ni,
    Nl,
    Nk,
    Ne,
    Np,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetricTable {
    Width,
    Height,
    Depth,
    Italic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreambleHeaderFailure {
    InvalidEffectiveSize,
    Malformed(PreambleHeaderRule),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreambleHeaderRule {
    PreambleUnavailable,
    HalfwordHighBit { field: CountField },
    InvalidCharacterRange,
    CharacterCodeAbove255,
    AggregateGeometryMismatch,
    EmptyRequiredMetricTable { table: MetricTable },
    DeclaredFrameUnavailable,
    HeaderTooShort,
    InvalidDesignSize,
    ArithmeticOverflow,
}

fn check_preamble_header(
    raw: Arc<[u8]>,
    effective_size_sp: i32,
) -> Result<HeaderCheckedTfm, PreambleHeaderFailure> {
    if !(1..MAX_TEX_FONT_SIZE_SP).contains(&effective_size_sp) {
        return Err(PreambleHeaderFailure::InvalidEffectiveSize);
    }
    if raw.len() < TFM_PREAMBLE_BYTES {
        return Err(PreambleHeaderFailure::Malformed(
            PreambleHeaderRule::PreambleUnavailable,
        ));
    }

    let fields = [
        CountField::Lf,
        CountField::Lh,
        CountField::Bc,
        CountField::Ec,
        CountField::Nw,
        CountField::Nh,
        CountField::Nd,
        CountField::Ni,
        CountField::Nl,
        CountField::Nk,
        CountField::Ne,
        CountField::Np,
    ];
    let mut decoded = [0u16; 12];
    for (index, field) in fields.into_iter().enumerate() {
        let offset = index * 2;
        if raw[offset] > 127 {
            return Err(PreambleHeaderFailure::Malformed(
                PreambleHeaderRule::HalfwordHighBit { field },
            ));
        }
        decoded[index] = u16::from_be_bytes([raw[offset], raw[offset + 1]]);
    }
    let [lf, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, np] = decoded;
    let raw_counts = RawCounts {
        lf,
        lh,
        bc,
        ec,
        nw,
        nh,
        nd,
        ni,
        nl,
        nk,
        ne,
        np,
    };

    if bc > ec + 1 {
        return Err(PreambleHeaderFailure::Malformed(
            PreambleHeaderRule::InvalidCharacterRange,
        ));
    }
    if ec > 255 {
        return Err(PreambleHeaderFailure::Malformed(
            PreambleHeaderRule::CharacterCodeAbove255,
        ));
    }
    let (character_domain, character_count) = if bc > ec {
        (CharacterDomain::Empty, 0usize)
    } else {
        (
            CharacterDomain::Inclusive {
                first: u8::try_from(bc).map_err(|_| {
                    PreambleHeaderFailure::Malformed(PreambleHeaderRule::CharacterCodeAbove255)
                })?,
                last: u8::try_from(ec).map_err(|_| {
                    PreambleHeaderFailure::Malformed(PreambleHeaderRule::CharacterCodeAbove255)
                })?,
            },
            usize::from(ec - bc + 1),
        )
    };

    let computed_words = [
        6usize,
        usize::from(lh),
        character_count,
        usize::from(nw),
        usize::from(nh),
        usize::from(nd),
        usize::from(ni),
        usize::from(nl),
        usize::from(nk),
        usize::from(ne),
        usize::from(np),
    ]
    .into_iter()
    .try_fold(0usize, |total, words| total.checked_add(words))
    .ok_or(PreambleHeaderFailure::Malformed(
        PreambleHeaderRule::ArithmeticOverflow,
    ))?;
    if computed_words != usize::from(lf) {
        return Err(PreambleHeaderFailure::Malformed(
            PreambleHeaderRule::AggregateGeometryMismatch,
        ));
    }
    for (count, table) in [
        (nw, MetricTable::Width),
        (nh, MetricTable::Height),
        (nd, MetricTable::Depth),
        (ni, MetricTable::Italic),
    ] {
        if count == 0 {
            return Err(PreambleHeaderFailure::Malformed(
                PreambleHeaderRule::EmptyRequiredMetricTable { table },
            ));
        }
    }

    let mut cursor = TFM_PREAMBLE_BYTES;
    let mut take_words = |words: usize| {
        let byte_count = words
            .checked_mul(4)
            .ok_or(PreambleHeaderFailure::Malformed(
                PreambleHeaderRule::ArithmeticOverflow,
            ))?;
        let end = cursor
            .checked_add(byte_count)
            .ok_or(PreambleHeaderFailure::Malformed(
                PreambleHeaderRule::ArithmeticOverflow,
            ))?;
        let range = cursor..end;
        cursor = end;
        Ok::<Range<usize>, PreambleHeaderFailure>(range)
    };
    let layout = TableLayout {
        header: take_words(usize::from(lh))?,
        characters: take_words(character_count)?,
        widths: take_words(usize::from(nw))?,
        heights: take_words(usize::from(nh))?,
        depths: take_words(usize::from(nd))?,
        italics: take_words(usize::from(ni))?,
        lig_kern: take_words(usize::from(nl))?,
        kerns: take_words(usize::from(nk))?,
        extensibles: take_words(usize::from(ne))?,
        parameters: take_words(usize::from(np))?,
    };
    let declared_frame_end =
        usize::from(lf)
            .checked_mul(4)
            .ok_or(PreambleHeaderFailure::Malformed(
                PreambleHeaderRule::ArithmeticOverflow,
            ))?;
    if cursor != declared_frame_end {
        return Err(PreambleHeaderFailure::Malformed(
            PreambleHeaderRule::AggregateGeometryMismatch,
        ));
    }
    if raw.len() < declared_frame_end {
        return Err(PreambleHeaderFailure::Malformed(
            PreambleHeaderRule::DeclaredFrameUnavailable,
        ));
    }
    if lh < 2 {
        return Err(PreambleHeaderFailure::Malformed(
            PreambleHeaderRule::HeaderTooShort,
        ));
    }

    let design_size_bytes = &raw[TFM_PREAMBLE_BYTES + 4..TFM_PREAMBLE_BYTES + 8];
    if design_size_bytes[0] > 127 {
        return Err(PreambleHeaderFailure::Malformed(
            PreambleHeaderRule::InvalidDesignSize,
        ));
    }
    let design_size_fix_word = i32::from_be_bytes([
        design_size_bytes[0],
        design_size_bytes[1],
        design_size_bytes[2],
        design_size_bytes[3],
    ]);
    if design_size_fix_word < MIN_DESIGN_SIZE_FIX_WORD {
        return Err(PreambleHeaderFailure::Malformed(
            PreambleHeaderRule::InvalidDesignSize,
        ));
    }
    let design_size_sp = design_size_fix_word / 16;

    let raw_digest = RawTfmDigest(Sha256::digest(raw.as_ref()).into());
    let frame_digest = FrameTfmDigest(Sha256::digest(&raw[..declared_frame_end]).into());

    Ok(HeaderCheckedTfm {
        raw,
        effective_size: EffectiveSizeSp(effective_size_sp),
        raw_counts,
        character_domain,
        layout,
        declared_frame_end: DeclaredFrameEnd(declared_frame_end),
        raw_digest,
        frame_digest,
        design_size_fix_word,
        design_size_sp,
    })
}

#[cfg(test)]
mod tests {
    use std::{any::TypeId, collections::HashSet, ops::Range, path::Path, sync::Arc};

    use sha2::{Digest, Sha256};

    use super::{
        CharacterDomain, CountField, EffectiveSizeSp, FrameTfmDigest, HeaderCheckedTfm,
        MetricTable, PreambleHeaderFailure, PreambleHeaderRule, RawTfmDigest,
        check_preamble_header,
    };

    const MAX_TEX_FONT_SIZE_SP: i32 = 1 << 27;
    const PREAMBLE_BYTES: usize = 24;
    const SEED_FRAME_BYTES: usize = 48;

    #[test]
    fn content_addressed_native_corpus_matches_header_proof_ownership() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let corpus_root = fixture_root.join("tfm-validity-oracle-v2");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(corpus_root.join("manifest.json")).unwrap())
                .unwrap();
        let rule_contract: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixture_root.join("tfm-validation-rules-v1.json")).unwrap(),
        )
        .unwrap();
        let header_rules = rule_contract["rules"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|rule| rule["proof_state"] == "HeaderCheckedTfm")
            .map(|rule| rule["id"].as_str().unwrap())
            .collect::<HashSet<_>>();

        let cases = manifest["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 83);
        for case in cases {
            let case_id = case["id"].as_str().unwrap();
            let blob_sha256 = case["blob_sha256"].as_str().unwrap();
            let raw = std::fs::read(corpus_root.join("blobs").join(format!("{blob_sha256}.tfm")))
                .unwrap();
            let actual_blob_sha256 = Sha256::digest(&raw)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            assert_eq!(actual_blob_sha256, blob_sha256, "{case_id}");
            let input_size =
                i32::try_from(case["validator_input_size_sp"].as_i64().unwrap()).unwrap();
            let result = check_preamble_header(Arc::from(raw), input_size);
            let classification = case["expected_classification"].as_str().unwrap();
            let first_rule = case["first_rejecting_rule"].as_str();

            match classification {
                "InvalidEffectiveSize" => assert!(
                    matches!(result, Err(PreambleHeaderFailure::InvalidEffectiveSize)),
                    "{case_id}"
                ),
                "MalformedTfm" if first_rule.is_some_and(|rule| header_rules.contains(rule)) => {
                    let expected = match case_id {
                        "size_field_high_bit" => PreambleHeaderRule::HalfwordHighBit {
                            field: CountField::Nw,
                        },
                        "invalid_character_range" => PreambleHeaderRule::InvalidCharacterRange,
                        "character_range_ec256" => PreambleHeaderRule::CharacterCodeAbove255,
                        "aggregate_length_mismatch" => {
                            PreambleHeaderRule::AggregateGeometryMismatch
                        }
                        "zero_width_table_consistent" => {
                            PreambleHeaderRule::EmptyRequiredMetricTable {
                                table: MetricTable::Width,
                            }
                        }
                        "zero_height_table_consistent" => {
                            PreambleHeaderRule::EmptyRequiredMetricTable {
                                table: MetricTable::Height,
                            }
                        }
                        "zero_depth_table_consistent" => {
                            PreambleHeaderRule::EmptyRequiredMetricTable {
                                table: MetricTable::Depth,
                            }
                        }
                        "zero_italic_table_consistent" => {
                            PreambleHeaderRule::EmptyRequiredMetricTable {
                                table: MetricTable::Italic,
                            }
                        }
                        "short_header" => PreambleHeaderRule::HeaderTooShort,
                        "design_size_below_one_pt" => PreambleHeaderRule::InvalidDesignSize,
                        "premature_eof" => PreambleHeaderRule::DeclaredFrameUnavailable,
                        _ => panic!("missing exact header rule for {case_id}"),
                    };
                    assert_eq!(
                        result.err(),
                        Some(PreambleHeaderFailure::Malformed(expected)),
                        "{case_id}"
                    );
                }
                "AcceptedByNativeLoader" | "MalformedTfm" => {
                    assert!(result.is_ok(), "{case_id}");
                }
                other => panic!("unexpected normalized classification {other} for {case_id}"),
            }
        }
    }

    #[test]
    fn maximum_valid_geometry_accepts_exact_frame_and_rejects_one_byte_short() {
        let maximum_words = 0x7fffusize;
        let mut bytes = vec![0; maximum_words * 4];
        for (index, value) in [0x7fff, 2, 1, 0, 1, 1, 1, 1, 0, 0, 0, 0x7ff3]
            .into_iter()
            .enumerate()
        {
            put_count(&mut bytes, index, value);
        }
        bytes[28..32].copy_from_slice(&(1i32 << 20).to_be_bytes());

        let state = check_preamble_header(Arc::from(bytes.clone()), 1).unwrap();
        assert_eq!(state.declared_frame_end.0, maximum_words * 4);
        bytes.pop();
        assert_rule(bytes, PreambleHeaderRule::DeclaredFrameUnavailable);
    }

    #[test]
    fn generated_structurally_consistent_preambles_have_exact_layouts() {
        for seed in 0..128u16 {
            let lh = 2 + seed % 7;
            let (bc, ec, character_count) = if seed % 2 == 0 {
                let code = seed % 256;
                (code, code, 1u16)
            } else {
                let last = seed % 256;
                (last + 1, last, 0u16)
            };
            let nw = 1 + seed.wrapping_mul(3) % 31;
            let nh = 1 + seed.wrapping_mul(5) % 15;
            let nd = 1 + seed.wrapping_mul(7) % 15;
            let ni = 1 + seed.wrapping_mul(11) % 15;
            let nl = seed.wrapping_mul(13) % 29;
            let nk = seed.wrapping_mul(17) % 23;
            let ne = seed.wrapping_mul(19) % 17;
            let np = seed.wrapping_mul(23) % 33;
            let lf = 6 + lh + character_count + nw + nh + nd + ni + nl + nk + ne + np;
            let mut bytes = vec![0; usize::from(lf) * 4];
            for (index, value) in [lf, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, np]
                .into_iter()
                .enumerate()
            {
                put_count(&mut bytes, index, value);
            }
            bytes[28..32].copy_from_slice(&(1i32 << 20).to_be_bytes());

            let state = check_preamble_header(Arc::from(bytes), 1).unwrap();
            assert_eq!(state.declared_frame_end.0, usize::from(lf) * 4);
            assert_eq!(state.layout.parameters.end, usize::from(lf) * 4);
        }
    }

    fn seed_frame() -> Vec<u8> {
        let mut bytes = vec![0; SEED_FRAME_BYTES];
        for (index, value) in [12, 2, 1, 0, 1, 1, 1, 1, 0, 0, 0, 0]
            .into_iter()
            .enumerate()
        {
            put_count(&mut bytes, index, value);
        }
        bytes[28..32].copy_from_slice(&(1i32 << 20).to_be_bytes());
        bytes
    }

    fn put_count(bytes: &mut [u8], index: usize, value: u16) {
        bytes[index * 2..index * 2 + 2].copy_from_slice(&value.to_be_bytes());
    }

    fn assert_rule(bytes: Vec<u8>, expected: PreambleHeaderRule) {
        match check_preamble_header(Arc::from(bytes), 1) {
            Err(PreambleHeaderFailure::Malformed(actual)) => assert_eq!(actual, expected),
            Err(other) => panic!("expected malformed {expected:?}, got {other:?}"),
            Ok(_) => panic!("expected malformed {expected:?}, got success"),
        }
    }

    #[test]
    fn effective_size_is_checked_before_bytes() {
        for size in [i32::MIN, -1, 0, MAX_TEX_FONT_SIZE_SP, i32::MAX] {
            assert!(matches!(
                check_preamble_header(Arc::from([]), size),
                Err(PreambleHeaderFailure::InvalidEffectiveSize)
            ));
        }
    }

    #[test]
    fn effective_size_accepts_closed_valid_interval() {
        for size in [1, MAX_TEX_FONT_SIZE_SP - 1] {
            let state = check_preamble_header(Arc::from(seed_frame()), size)
                .expect("the first-phase seed must accept every legal effective size");
            assert_eq!(state.effective_size, EffectiveSizeSp(size));
        }
    }

    #[test]
    fn effective_size_rejects_outside_interval() {
        for size in [i32::MIN, -7, -1, 0, MAX_TEX_FONT_SIZE_SP, i32::MAX] {
            assert!(matches!(
                check_preamble_header(Arc::from(seed_frame()), size),
                Err(PreambleHeaderFailure::InvalidEffectiveSize)
            ));
        }
    }

    #[test]
    fn preamble_rejects_every_length_below_24() {
        for length in 0..PREAMBLE_BYTES {
            assert_rule(vec![0; length], PreambleHeaderRule::PreambleUnavailable);
        }
    }

    #[test]
    fn each_of_twelve_halfwords_rejects_a_high_first_byte() {
        for (index, field) in [
            CountField::Lf,
            CountField::Lh,
            CountField::Bc,
            CountField::Ec,
            CountField::Nw,
            CountField::Nh,
            CountField::Nd,
            CountField::Ni,
            CountField::Nl,
            CountField::Nk,
            CountField::Ne,
            CountField::Np,
        ]
        .into_iter()
        .enumerate()
        {
            let mut bytes = seed_frame();
            bytes[index * 2] = 128;
            assert_rule(bytes, PreambleHeaderRule::HalfwordHighBit { field });
        }
    }

    #[test]
    fn halfword_max_without_high_bit_is_decoded_safely() {
        let mut bytes = seed_frame();
        put_count(&mut bytes, 11, 0x7fff);
        assert_rule(bytes, PreambleHeaderRule::AggregateGeometryMismatch);
    }

    #[test]
    fn range_rejects_bc_greater_than_ec_plus_one() {
        let mut bytes = seed_frame();
        put_count(&mut bytes, 2, 3);
        put_count(&mut bytes, 3, 1);
        assert_rule(bytes, PreambleHeaderRule::InvalidCharacterRange);
    }

    #[test]
    fn range_rejects_ec_256_independently() {
        let mut bytes = seed_frame();
        put_count(&mut bytes, 2, 257);
        put_count(&mut bytes, 3, 256);
        assert_rule(bytes, PreambleHeaderRule::CharacterCodeAbove255);
    }

    #[test]
    fn all_native_empty_ranges_are_accepted() {
        for last in 0..=255u16 {
            let mut bytes = seed_frame();
            put_count(&mut bytes, 2, last + 1);
            put_count(&mut bytes, 3, last);
            let state = check_preamble_header(Arc::from(bytes), 1)
                .expect("every bc=ec+1 range through 256,255 is empty");
            assert_eq!(state.character_domain, CharacterDomain::Empty);
        }
    }

    #[test]
    fn all_empty_ranges_preserve_raw_counts_and_share_normalized_semantics() {
        let mut canonical = seed_frame();
        put_count(&mut canonical, 2, 1);
        put_count(&mut canonical, 3, 0);
        let canonical = check_preamble_header(Arc::from(canonical), 1).unwrap();

        let mut upper = seed_frame();
        put_count(&mut upper, 2, 256);
        put_count(&mut upper, 3, 255);
        let upper = check_preamble_header(Arc::from(upper), 1).unwrap();

        assert_eq!(canonical.character_domain, upper.character_domain);
        assert_eq!((canonical.raw_counts.bc, canonical.raw_counts.ec), (1, 0));
        assert_eq!((upper.raw_counts.bc, upper.raw_counts.ec), (256, 255));
    }

    #[test]
    fn ordinary_nonempty_range_normalizes_to_u8_domain() {
        let mut bytes = seed_frame();
        bytes.extend_from_slice(&[0; 4]);
        put_count(&mut bytes, 0, 13);
        put_count(&mut bytes, 2, 7);
        put_count(&mut bytes, 3, 7);

        let state = check_preamble_header(Arc::from(bytes), 1).unwrap();
        assert_eq!(
            state.character_domain,
            CharacterDomain::Inclusive { first: 7, last: 7 }
        );
        assert_eq!(state.layout.characters, 32..36);
    }

    #[test]
    fn aggregate_geometry_must_equal_lf() {
        let mut bytes = seed_frame();
        put_count(&mut bytes, 0, 13);
        assert_rule(bytes, PreambleHeaderRule::AggregateGeometryMismatch);
    }

    #[test]
    fn each_required_metric_table_must_be_nonempty() {
        for (index, table) in [
            (4, MetricTable::Width),
            (5, MetricTable::Height),
            (6, MetricTable::Depth),
            (7, MetricTable::Italic),
        ] {
            let mut bytes = seed_frame();
            put_count(&mut bytes, 0, 11);
            put_count(&mut bytes, index, 0);
            assert_rule(
                bytes,
                PreambleHeaderRule::EmptyRequiredMetricTable { table },
            );
        }
    }

    #[test]
    fn layout_ranges_are_contiguous_and_end_at_declared_extent() {
        let state = check_preamble_header(Arc::from(seed_frame()), 1).unwrap();
        let ranges = [
            &state.layout.header,
            &state.layout.characters,
            &state.layout.widths,
            &state.layout.heights,
            &state.layout.depths,
            &state.layout.italics,
            &state.layout.lig_kern,
            &state.layout.kerns,
            &state.layout.extensibles,
            &state.layout.parameters,
        ];

        assert_eq!(ranges[0].start, PREAMBLE_BYTES);
        for pair in ranges.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
        assert_eq!(ranges.last().unwrap().end, state.declared_frame_end.0);
        assert_eq!(state.declared_frame_end.0, SEED_FRAME_BYTES);
    }

    #[test]
    fn layout_uses_normalized_character_count() {
        let mut bytes = seed_frame();
        put_count(&mut bytes, 2, 256);
        put_count(&mut bytes, 3, 255);

        let state = check_preamble_header(Arc::from(bytes), 1).unwrap();
        assert_eq!(state.layout.characters, Range { start: 32, end: 32 });
        assert_eq!(state.layout.widths, 32..36);
    }

    #[test]
    fn large_counts_never_overflow_or_index_before_validation() {
        let mut bytes = seed_frame();
        for index in [0, 1, 4, 5, 6, 7, 8, 9, 10, 11] {
            put_count(&mut bytes, index, 0x7fff);
        }

        let result = std::panic::catch_unwind(|| check_preamble_header(Arc::from(bytes), 1));
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Err(PreambleHeaderFailure::Malformed(
                PreambleHeaderRule::AggregateGeometryMismatch
            ))
        ));
    }

    #[test]
    fn declared_frame_must_be_fully_available_at_every_truncation() {
        let seed = seed_frame();
        for length in PREAMBLE_BYTES..SEED_FRAME_BYTES {
            assert_rule(
                seed[..length].to_vec(),
                PreambleHeaderRule::DeclaredFrameUnavailable,
            );
        }
    }

    #[test]
    fn exact_declared_frame_is_accepted() {
        let state = check_preamble_header(Arc::from(seed_frame()), 1).unwrap();
        assert_eq!(state.raw.len(), SEED_FRAME_BYTES);
        assert_eq!(state.declared_frame_end.0, SEED_FRAME_BYTES);
    }

    #[test]
    fn suffix_lengths_are_semantically_accepted() {
        for length in [1, 2, 3, 4, 8193] {
            let mut bytes = seed_frame();
            bytes.extend(std::iter::repeat_n(0xa5, length));
            let state = check_preamble_header(Arc::from(bytes), 1)
                .expect("declared-frame suffixes are not TFM semantics");
            assert_eq!(state.declared_frame_end.0, SEED_FRAME_BYTES);
            assert_eq!(state.raw.len(), SEED_FRAME_BYTES + length);
        }
    }

    #[test]
    fn generated_suffixes_do_not_change_layout_or_header_state() {
        let control = check_preamble_header(Arc::from(seed_frame()), 1).unwrap();
        for length in 0..=64 {
            let mut bytes = seed_frame();
            bytes.extend((0..length).map(|index| (index as u8).wrapping_mul(37)));
            let state = check_preamble_header(Arc::from(bytes), 1).unwrap();
            assert_eq!(state.effective_size, control.effective_size);
            assert_eq!(state.raw_counts, control.raw_counts);
            assert_eq!(state.character_domain, control.character_domain);
            assert_eq!(state.layout, control.layout);
            assert_eq!(state.declared_frame_end, control.declared_frame_end);
            assert_eq!(state.frame_digest, control.frame_digest);
            assert_eq!(state.design_size_fix_word, control.design_size_fix_word);
            assert_eq!(state.design_size_sp, control.design_size_sp);
        }
    }

    #[test]
    fn suffix_changes_raw_digest_but_not_frame_digest() {
        let control = check_preamble_header(Arc::from(seed_frame()), 1).unwrap();
        let mut bytes = seed_frame();
        bytes.extend_from_slice(&[1, 2, 3]);
        let suffixed = check_preamble_header(Arc::from(bytes), 1).unwrap();

        assert_ne!(suffixed.raw_digest, control.raw_digest);
        assert_eq!(suffixed.frame_digest, control.frame_digest);
    }

    #[test]
    fn equal_raw_and_frame_digest_bytes_still_have_distinct_types() {
        let state = check_preamble_header(Arc::from(seed_frame()), 1).unwrap();
        assert_eq!(state.raw_digest.0, state.frame_digest.0);
        assert_ne!(TypeId::of::<RawTfmDigest>(), TypeId::of::<FrameTfmDigest>());
    }

    #[test]
    fn header_requires_at_least_two_words() {
        let mut bytes = seed_frame();
        put_count(&mut bytes, 0, 11);
        put_count(&mut bytes, 1, 1);
        assert_rule(bytes, PreambleHeaderRule::HeaderTooShort);
    }

    #[test]
    fn two_word_header_is_accepted() {
        let state = check_preamble_header(Arc::from(seed_frame()), 1).unwrap();
        assert_eq!(state.layout.header, 24..32);
    }

    #[test]
    fn additional_header_words_are_ignored_for_validation_but_bound_to_identity() {
        let mut first = seed_frame();
        put_count(&mut first, 0, 13);
        put_count(&mut first, 1, 3);
        first.splice(32..32, [1, 2, 3, 4]);
        let mut second = first.clone();
        second[32..36].copy_from_slice(&[5, 6, 7, 8]);

        let first = check_preamble_header(Arc::from(first), 1).unwrap();
        let second = check_preamble_header(Arc::from(second), 1).unwrap();
        assert_eq!(first.layout, second.layout);
        assert_eq!(first.design_size_sp, second.design_size_sp);
        assert_ne!(first.frame_digest, second.frame_digest);
    }

    #[test]
    fn design_size_below_one_pt_is_rejected() {
        let mut bytes = seed_frame();
        bytes[28..32].copy_from_slice(&((1i32 << 20) - 1).to_be_bytes());
        assert_rule(bytes, PreambleHeaderRule::InvalidDesignSize);
    }

    #[test]
    fn design_size_exactly_one_pt_is_accepted() {
        let state = check_preamble_header(Arc::from(seed_frame()), 1).unwrap();
        assert_eq!(state.design_size_fix_word, 1 << 20);
        assert_eq!(state.design_size_sp, 1 << 16);
    }

    #[test]
    fn negative_or_forbidden_high_design_encoding_is_rejected() {
        let mut bytes = seed_frame();
        bytes[28..32].copy_from_slice(&0x8000_0000u32.to_be_bytes());
        assert_rule(bytes, PreambleHeaderRule::InvalidDesignSize);
    }

    #[test]
    fn largest_supported_positive_design_encoding_does_not_overflow() {
        let mut bytes = seed_frame();
        bytes[28..32].copy_from_slice(&i32::MAX.to_be_bytes());
        let state = check_preamble_header(Arc::from(bytes), 1).unwrap();
        assert_eq!(state.design_size_fix_word, i32::MAX);
        assert_eq!(state.design_size_sp, MAX_TEX_FONT_SIZE_SP - 1);
    }

    #[test]
    fn success_retains_the_same_arc_allocation() {
        let raw: Arc<[u8]> = Arc::from(seed_frame());
        let retained = Arc::clone(&raw);
        let state = check_preamble_header(raw, 1).unwrap();
        assert!(Arc::ptr_eq(&retained, &state.raw));
    }

    #[test]
    fn success_retains_the_exact_bound_size() {
        let state = check_preamble_header(Arc::from(seed_frame()), 12_345).unwrap();
        assert_eq!(state.effective_size, EffectiveSizeSp(12_345));
    }

    #[test]
    fn entrypoint_accepts_no_replacement_bytes_size_extent_or_digests() {
        let _: fn(Arc<[u8]>, i32) -> Result<HeaderCheckedTfm, PreambleHeaderFailure> =
            check_preamble_header;
    }

    #[test]
    fn later_semantic_invalidity_does_not_block_header_state() {
        let mut bytes = seed_frame();
        bytes[32] = 1;
        assert!(check_preamble_header(Arc::from(bytes), 1).is_ok());
    }

    #[test]
    fn bounded_arbitrary_bytes_and_sizes_never_panic() {
        let mut generator = 0x6a8e_2bef_e164_83e8u64;
        let sizes = [i32::MIN, -1, 0, 1, 16, MAX_TEX_FONT_SIZE_SP - 1, i32::MAX];
        for case_index in 0..2048 {
            generator = generator
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let length = (generator as usize) % 129;
            let mut bytes = vec![0; length];
            for byte in &mut bytes {
                generator = generator
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                *byte = (generator >> 32) as u8;
            }
            let size = sizes[case_index % sizes.len()];
            let outcome =
                std::panic::catch_unwind(|| check_preamble_header(Arc::from(bytes), size));
            assert!(outcome.is_ok(), "case {case_index} panicked");
        }
    }
}
