//! Staged, private TeX82 TFM validation phases.

#![allow(
    dead_code,
    reason = "the reviewed validator phases remain private and unreachable until compatibility closure"
)]
#![forbid(unsafe_code)]

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
enum CharacterMetric {
    Width,
    Height,
    Depth,
    Italic,
}

const CHARACTER_METRIC_SOURCE_ORDER: [CharacterMetric; 4] = [
    CharacterMetric::Width,
    CharacterMetric::Height,
    CharacterMetric::Depth,
    CharacterMetric::Italic,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharacterTag {
    None,
    Ligature { start: u8 },
    List { target: u8 },
    Extensible { recipe: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CheckedCharacterRecord {
    character: u8,
    width_index: u8,
    height_index: u8,
    depth_index: u8,
    italic_index: u8,
    tag: CharacterTag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExistingCharacters([u64; 4]);

struct CharacterCheckedTfm {
    predecessor: HeaderCheckedTfm,
    records: Box<[CheckedCharacterRecord]>,
    existing_characters: ExistingCharacters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScaledSp(i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoxMetric {
    Width,
    Height,
    Depth,
    Italic,
}

struct BoxCheckedTfm {
    predecessor: CharacterCheckedTfm,
    widths: Box<[ScaledSp]>,
    heights: Box<[ScaledSp]>,
    depths: Box<[ScaledSp]>,
    italics: Box<[ScaledSp]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CheckedLigKernInstruction {
    skip_byte: u8,
    next_character: u8,
    operation_byte: u8,
    remainder: u8,
}

struct LigKernCheckedTfm {
    predecessor: BoxCheckedTfm,
    instructions: Box<[CheckedLigKernInstruction]>,
    boundary_character: Option<u8>,
    boundary_program_start: Option<u16>,
}

struct KernCheckedTfm {
    predecessor: LigKernCheckedTfm,
    kerns: Box<[ScaledSp]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CheckedExtensibleRecipe {
    top: Option<u8>,
    middle: Option<u8>,
    bottom: Option<u8>,
    repeat: u8,
}

struct ExtensibleCheckedTfm {
    predecessor: KernCheckedTfm,
    extensibles: Box<[CheckedExtensibleRecipe]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SignedSlant(i32);

struct ParameterCheckedTfm {
    predecessor: ExtensibleCheckedTfm,
    slant: SignedSlant,
    dimensions: Box<[ScaledSp]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParameterValidationRule {
    InvalidFixWordSign { parameter: u16, sign: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtensiblePart {
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtensibleValidationRule {
    OptionalPartMissing {
        recipe: u16,
        part: ExtensiblePart,
        character: u8,
    },
    RepeatMissing {
        recipe: u16,
        character: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KernValidationRule {
    InvalidFixWordSign { index: u16, sign: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LigKernValidationRule {
    RestartTargetOutOfRange {
        instruction: u16,
        target: u16,
        count: u16,
    },
    NextCharacterMissing {
        instruction: u16,
        character: u8,
    },
    LigatureTargetMissing {
        instruction: u16,
        character: u8,
    },
    KernIndexOutOfRange {
        instruction: u16,
        index: u16,
        count: u16,
    },
    ForwardSkipOutOfRange {
        instruction: u16,
        skip: u8,
        target: u32,
        count: u16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoxValidationRule {
    InvalidFixWordSign {
        table: BoxMetric,
        index: u16,
        sign: u8,
    },
    NonzeroScaledEntryZero {
        table: BoxMetric,
        scaled_sp: i32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharacterValidationRule {
    MetricIndexOutOfRange {
        character: u8,
        metric: CharacterMetric,
        index: u8,
        count: u16,
    },
    LigatureIndexOutOfRange {
        character: u8,
        index: u8,
        count: u16,
    },
    ExtensibleIndexOutOfRange {
        character: u8,
        index: u8,
        count: u16,
    },
    CharListTargetOutOfRange {
        character: u8,
        target: u8,
        first: u8,
        last: u8,
    },
    CharListCycle {
        character: u8,
    },
    CharListTraversalLimit {
        character: u8,
    },
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

fn check_characters(
    predecessor: HeaderCheckedTfm,
) -> Result<CharacterCheckedTfm, CharacterValidationRule> {
    let mut records: Vec<CheckedCharacterRecord> = Vec::new();
    let mut existing_characters = ExistingCharacters([0; 4]);
    if let CharacterDomain::Inclusive { first, last } = predecessor.character_domain {
        for (character, raw_record) in (first..=last)
            .zip(predecessor.raw[predecessor.layout.characters.clone()].chunks_exact(4))
        {
            let width_index = raw_record[0];
            let height_index = raw_record[1] >> 4;
            let depth_index = raw_record[1] & 0x0f;
            let italic_index = raw_record[2] >> 2;
            let tag_code = raw_record[2] & 0x03;
            let remainder = raw_record[3];

            for ((index, count), metric) in [
                (width_index, predecessor.raw_counts.nw),
                (height_index, predecessor.raw_counts.nh),
                (depth_index, predecessor.raw_counts.nd),
                (italic_index, predecessor.raw_counts.ni),
            ]
            .into_iter()
            .zip(CHARACTER_METRIC_SOURCE_ORDER)
            {
                if u16::from(index) >= count {
                    return Err(CharacterValidationRule::MetricIndexOutOfRange {
                        character,
                        metric,
                        index,
                        count,
                    });
                }
            }

            let tag = match tag_code {
                0 => CharacterTag::None,
                1 => {
                    if u16::from(remainder) >= predecessor.raw_counts.nl {
                        return Err(CharacterValidationRule::LigatureIndexOutOfRange {
                            character,
                            index: remainder,
                            count: predecessor.raw_counts.nl,
                        });
                    }
                    CharacterTag::Ligature { start: remainder }
                }
                2 => {
                    if remainder < first || remainder > last {
                        return Err(CharacterValidationRule::CharListTargetOutOfRange {
                            character,
                            target: remainder,
                            first,
                            last,
                        });
                    }

                    let mut target = remainder;
                    let mut steps = 0usize;
                    let domain_size = usize::from(last - first) + 1;
                    while target < character {
                        let target_record = records[usize::from(target - first)];
                        let CharacterTag::List {
                            target: next_target,
                        } = target_record.tag
                        else {
                            break;
                        };
                        target = next_target;
                        steps += 1;
                        if steps > domain_size {
                            return Err(CharacterValidationRule::CharListTraversalLimit {
                                character,
                            });
                        }
                    }
                    if target == character {
                        return Err(CharacterValidationRule::CharListCycle { character });
                    }
                    CharacterTag::List { target: remainder }
                }
                3 => {
                    if u16::from(remainder) >= predecessor.raw_counts.ne {
                        return Err(CharacterValidationRule::ExtensibleIndexOutOfRange {
                            character,
                            index: remainder,
                            count: predecessor.raw_counts.ne,
                        });
                    }
                    CharacterTag::Extensible { recipe: remainder }
                }
                _ => unreachable!("the tag code is masked to two bits"),
            };

            records.push(CheckedCharacterRecord {
                character,
                width_index,
                height_index,
                depth_index,
                italic_index,
                tag,
            });
            if width_index != 0 {
                let word = usize::from(character) / 64;
                let bit = u32::from(character % 64);
                existing_characters.0[word] |= 1u64 << bit;
            }
        }
    }

    Ok(CharacterCheckedTfm {
        predecessor,
        records: records.into_boxed_slice(),
        existing_characters,
    })
}

fn check_boxes(predecessor: CharacterCheckedTfm) -> Result<BoxCheckedTfm, BoxValidationRule> {
    let mut reduced_size = i64::from(predecessor.predecessor.effective_size.0);
    let mut alpha = 16i64;
    while reduced_size >= 1 << 23 {
        reduced_size /= 2;
        alpha += alpha;
    }
    let beta = 256 / alpha;
    alpha *= reduced_size;

    let mut widths = Vec::with_capacity(usize::from(predecessor.predecessor.raw_counts.nw));
    let mut heights = Vec::with_capacity(usize::from(predecessor.predecessor.raw_counts.nh));
    let mut depths = Vec::with_capacity(usize::from(predecessor.predecessor.raw_counts.nd));
    let mut italics = Vec::with_capacity(usize::from(predecessor.predecessor.raw_counts.ni));
    for (table, range) in [
        (
            BoxMetric::Width,
            predecessor.predecessor.layout.widths.clone(),
        ),
        (
            BoxMetric::Height,
            predecessor.predecessor.layout.heights.clone(),
        ),
        (
            BoxMetric::Depth,
            predecessor.predecessor.layout.depths.clone(),
        ),
        (
            BoxMetric::Italic,
            predecessor.predecessor.layout.italics.clone(),
        ),
    ] {
        for (index, raw_word) in predecessor.predecessor.raw[range]
            .chunks_exact(4)
            .enumerate()
        {
            let sign = raw_word[0];
            let b = i64::from(raw_word[1]);
            let c = i64::from(raw_word[2]);
            let d = i64::from(raw_word[3]);
            let positive_fraction =
                ((d * reduced_size / 256 + c * reduced_size) / 256 + b * reduced_size) / beta;
            let scaled = match sign {
                0 => positive_fraction,
                255 => positive_fraction - alpha,
                _ => {
                    let index = match u16::try_from(index) {
                        Ok(index) => index,
                        Err(_) => unreachable!("header-checked TFM table indices fit u16"),
                    };
                    return Err(BoxValidationRule::InvalidFixWordSign { table, index, sign });
                }
            };
            let scaled_sp = match i32::try_from(scaled) {
                Ok(scaled_sp) => ScaledSp(scaled_sp),
                Err(_) => unreachable!("TeX82 fix-word and effective-size bounds fit scaled"),
            };
            match table {
                BoxMetric::Width => widths.push(scaled_sp),
                BoxMetric::Height => heights.push(scaled_sp),
                BoxMetric::Depth => depths.push(scaled_sp),
                BoxMetric::Italic => italics.push(scaled_sp),
            }
        }
    }
    for (table, values) in [
        (BoxMetric::Width, widths.as_slice()),
        (BoxMetric::Height, heights.as_slice()),
        (BoxMetric::Depth, depths.as_slice()),
        (BoxMetric::Italic, italics.as_slice()),
    ] {
        let ScaledSp(scaled_sp) = values[0];
        if scaled_sp != 0 {
            return Err(BoxValidationRule::NonzeroScaledEntryZero { table, scaled_sp });
        }
    }

    Ok(BoxCheckedTfm {
        predecessor,
        widths: widths.into_boxed_slice(),
        heights: heights.into_boxed_slice(),
        depths: depths.into_boxed_slice(),
        italics: italics.into_boxed_slice(),
    })
}

fn check_lig_kern(predecessor: BoxCheckedTfm) -> Result<LigKernCheckedTfm, LigKernValidationRule> {
    let character_state = &predecessor.predecessor;
    let header_state = &character_state.predecessor;
    let instruction_count = header_state.raw_counts.nl;
    let kern_count = header_state.raw_counts.nk;
    let mut instructions = Vec::with_capacity(usize::from(instruction_count));
    let mut boundary_character = None;

    for (instruction_index, raw_instruction) in header_state.raw
        [header_state.layout.lig_kern.clone()]
    .chunks_exact(4)
    .enumerate()
    {
        let instruction = match u16::try_from(instruction_index) {
            Ok(instruction) => instruction,
            Err(_) => unreachable!("header-checked TFM instruction indices fit u16"),
        };
        let skip_byte = raw_instruction[0];
        let next_character = raw_instruction[1];
        let operation_byte = raw_instruction[2];
        let remainder = raw_instruction[3];
        if skip_byte > 128 {
            let target = u16::from(operation_byte) * 256 + u16::from(remainder);
            if target >= instruction_count {
                return Err(LigKernValidationRule::RestartTargetOutOfRange {
                    instruction,
                    target,
                    count: instruction_count,
                });
            }
            if skip_byte == 255 && instruction == 0 {
                boundary_character = Some(next_character);
            }
        } else {
            if boundary_character != Some(next_character) {
                let word = usize::from(next_character) / 64;
                let bit = u32::from(next_character % 64);
                if character_state.existing_characters.0[word] & (1u64 << bit) == 0 {
                    return Err(LigKernValidationRule::NextCharacterMissing {
                        instruction,
                        character: next_character,
                    });
                }
            }
            if operation_byte < 128 {
                let word = usize::from(remainder) / 64;
                let bit = u32::from(remainder % 64);
                if character_state.existing_characters.0[word] & (1u64 << bit) == 0 {
                    return Err(LigKernValidationRule::LigatureTargetMissing {
                        instruction,
                        character: remainder,
                    });
                }
            } else {
                let index = u16::from(operation_byte - 128) * 256 + u16::from(remainder);
                if index >= kern_count {
                    return Err(LigKernValidationRule::KernIndexOutOfRange {
                        instruction,
                        index,
                        count: kern_count,
                    });
                }
            }
            if skip_byte < 128 {
                let target = u32::from(instruction) + u32::from(skip_byte) + 1;
                if target >= u32::from(instruction_count) {
                    return Err(LigKernValidationRule::ForwardSkipOutOfRange {
                        instruction,
                        skip: skip_byte,
                        target,
                        count: instruction_count,
                    });
                }
            }
        }
        instructions.push(CheckedLigKernInstruction {
            skip_byte,
            next_character,
            operation_byte,
            remainder,
        });
    }

    let boundary_program_start = instructions.last().and_then(|instruction| {
        (instruction.skip_byte == 255)
            .then(|| u16::from(instruction.operation_byte) * 256 + u16::from(instruction.remainder))
    });
    Ok(LigKernCheckedTfm {
        predecessor,
        instructions: instructions.into_boxed_slice(),
        boundary_character,
        boundary_program_start,
    })
}

fn check_kerns(predecessor: LigKernCheckedTfm) -> Result<KernCheckedTfm, KernValidationRule> {
    let box_state = &predecessor.predecessor;
    let character_state = &box_state.predecessor;
    let header_state = &character_state.predecessor;
    let mut reduced_size = i64::from(header_state.effective_size.0);
    let mut alpha = 16i64;
    while reduced_size >= 1 << 23 {
        reduced_size /= 2;
        alpha += alpha;
    }
    let beta = 256 / alpha;
    alpha *= reduced_size;

    let mut kerns = Vec::with_capacity(usize::from(header_state.raw_counts.nk));
    for (index, raw_word) in header_state.raw[header_state.layout.kerns.clone()]
        .chunks_exact(4)
        .enumerate()
    {
        let sign = raw_word[0];
        let b = i64::from(raw_word[1]);
        let c = i64::from(raw_word[2]);
        let d = i64::from(raw_word[3]);
        let positive_fraction =
            ((d * reduced_size / 256 + c * reduced_size) / 256 + b * reduced_size) / beta;
        let scaled = match sign {
            0 => positive_fraction,
            255 => positive_fraction - alpha,
            _ => {
                let index = match u16::try_from(index) {
                    Ok(index) => index,
                    Err(_) => unreachable!("header-checked TFM kern indices fit u16"),
                };
                return Err(KernValidationRule::InvalidFixWordSign { index, sign });
            }
        };
        let scaled_sp = match i32::try_from(scaled) {
            Ok(scaled_sp) => ScaledSp(scaled_sp),
            Err(_) => unreachable!("TeX82 fix-word and effective-size bounds fit scaled kerns"),
        };
        kerns.push(scaled_sp);
    }

    Ok(KernCheckedTfm {
        predecessor,
        kerns: kerns.into_boxed_slice(),
    })
}

fn check_extensibles(
    predecessor: KernCheckedTfm,
) -> Result<ExtensibleCheckedTfm, ExtensibleValidationRule> {
    let character_state = &predecessor.predecessor.predecessor.predecessor;
    let header_state = &character_state.predecessor;
    let mut extensibles = Vec::with_capacity(usize::from(header_state.raw_counts.ne));
    for (recipe_index, raw_recipe) in header_state.raw[header_state.layout.extensibles.clone()]
        .chunks_exact(4)
        .enumerate()
    {
        let recipe = match u16::try_from(recipe_index) {
            Ok(recipe) => recipe,
            Err(_) => unreachable!("header-checked TFM extensible indices fit u16"),
        };
        let mut optional_parts = [None; 3];
        for (slot, (part, character)) in optional_parts.iter_mut().zip([
            (ExtensiblePart::Top, raw_recipe[0]),
            (ExtensiblePart::Middle, raw_recipe[1]),
            (ExtensiblePart::Bottom, raw_recipe[2]),
        ]) {
            if character == 0 {
                continue;
            }
            let word = usize::from(character) / 64;
            let bit = u32::from(character % 64);
            if character_state.existing_characters.0[word] & (1u64 << bit) == 0 {
                return Err(ExtensibleValidationRule::OptionalPartMissing {
                    recipe,
                    part,
                    character,
                });
            }
            *slot = Some(character);
        }

        let repeat = raw_recipe[3];
        let repeat_word = usize::from(repeat) / 64;
        let repeat_bit = u32::from(repeat % 64);
        if character_state.existing_characters.0[repeat_word] & (1u64 << repeat_bit) == 0 {
            return Err(ExtensibleValidationRule::RepeatMissing {
                recipe,
                character: repeat,
            });
        }
        extensibles.push(CheckedExtensibleRecipe {
            top: optional_parts[0],
            middle: optional_parts[1],
            bottom: optional_parts[2],
            repeat,
        });
    }

    Ok(ExtensibleCheckedTfm {
        predecessor,
        extensibles: extensibles.into_boxed_slice(),
    })
}

fn check_parameters(
    predecessor: ExtensibleCheckedTfm,
) -> Result<ParameterCheckedTfm, ParameterValidationRule> {
    let header_state = &predecessor
        .predecessor
        .predecessor
        .predecessor
        .predecessor
        .predecessor;
    let mut raw_parameters = header_state.raw[header_state.layout.parameters.clone()]
        .chunks_exact(4)
        .enumerate();
    let slant = raw_parameters.next().map_or(SignedSlant(0), |(_, word)| {
        SignedSlant(i32::from_be_bytes([word[0], word[1], word[2], word[3]]) >> 4)
    });

    let mut reduced_size = i64::from(header_state.effective_size.0);
    let mut alpha = 16i64;
    while reduced_size >= 1 << 23 {
        reduced_size /= 2;
        alpha += alpha;
    }
    let beta = 256 / alpha;
    alpha *= reduced_size;

    let stored_parameter_count = header_state.raw_counts.np.max(7);
    let mut dimensions = Vec::with_capacity(usize::from(stored_parameter_count.saturating_sub(1)));
    for (zero_based_index, raw_word) in raw_parameters {
        let parameter = match u16::try_from(zero_based_index + 1) {
            Ok(parameter) => parameter,
            Err(_) => unreachable!("header-checked TFM parameter indices fit u16"),
        };
        let sign = raw_word[0];
        let b = i64::from(raw_word[1]);
        let c = i64::from(raw_word[2]);
        let d = i64::from(raw_word[3]);
        let positive_fraction =
            ((d * reduced_size / 256 + c * reduced_size) / 256 + b * reduced_size) / beta;
        let scaled = match sign {
            0 => positive_fraction,
            255 => positive_fraction - alpha,
            _ => {
                return Err(ParameterValidationRule::InvalidFixWordSign { parameter, sign });
            }
        };
        let scaled_sp = match i32::try_from(scaled) {
            Ok(scaled_sp) => ScaledSp(scaled_sp),
            Err(_) => unreachable!("TeX82 fix-word and effective-size bounds fit parameters"),
        };
        dimensions.push(scaled_sp);
    }
    if dimensions.len() < 6 {
        dimensions.resize(6, ScaledSp(0));
    }

    Ok(ParameterCheckedTfm {
        predecessor,
        slant,
        dimensions: dimensions.into_boxed_slice(),
    })
}

#[cfg(test)]
mod tests {
    use std::{
        any::TypeId,
        collections::{HashMap, HashSet},
        ops::Range,
        path::Path,
        sync::Arc,
    };

    use sha2::{Digest, Sha256};

    use super::{
        BoxCheckedTfm, BoxMetric, BoxValidationRule, CHARACTER_METRIC_SOURCE_ORDER,
        CharacterCheckedTfm, CharacterDomain, CharacterMetric, CharacterTag,
        CharacterValidationRule, CheckedExtensibleRecipe, CountField, EffectiveSizeSp,
        ExtensibleCheckedTfm, ExtensiblePart, ExtensibleValidationRule, FrameTfmDigest,
        HeaderCheckedTfm, KernCheckedTfm, KernValidationRule, LigKernCheckedTfm,
        LigKernValidationRule, MetricTable, ParameterCheckedTfm, ParameterValidationRule,
        PreambleHeaderFailure, PreambleHeaderRule, RawTfmDigest, ScaledSp, SignedSlant,
        check_boxes, check_characters, check_extensibles, check_kerns, check_lig_kern,
        check_parameters, check_preamble_header,
    };

    const MAX_TEX_FONT_SIZE_SP: i32 = 1 << 27;
    const PREAMBLE_BYTES: usize = 24;
    const SEED_FRAME_BYTES: usize = 48;

    fn reviewed_corpus_manifest(fixture_root: &Path) -> serde_json::Value {
        let manifest_bytes =
            std::fs::read(fixture_root.join("tfm-validity-oracle-v2/manifest.json")).unwrap();
        let rule_contract_bytes =
            std::fs::read(fixture_root.join("tfm-validation-rules-v1.json")).unwrap();
        let rule_transition_bytes =
            std::fs::read(fixture_root.join("tfm-validation-rule-transition-v2.json")).unwrap();
        let native_fixture_bytes =
            std::fs::read(fixture_root.join("tfm-validity-oracle-v1.json")).unwrap();
        for (label, bytes, expected_sha256) in [
            (
                "v2 corpus manifest",
                manifest_bytes.as_slice(),
                "db680c23a099b5b39c484d34c357116fc8d6967a9151db4108af0ddf4cfbb0be",
            ),
            (
                "v1 rule contract",
                rule_contract_bytes.as_slice(),
                "260bff0aa11e2b839985875c6df1e5075b37fc110ab0c3723a2837db70d20353",
            ),
            (
                "v1 native fixture",
                native_fixture_bytes.as_slice(),
                "9df44bf4b157acfb65fa0d5cc7de4d42ba7f869bae460e07daf984e1fbca19b4",
            ),
            (
                "v2 rule transition",
                rule_transition_bytes.as_slice(),
                "4a0bb1453055d12037fbbab0c77999feaf9b24f2d71b7e8afeb38453d2788316",
            ),
        ] {
            let actual_sha256 = Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            assert_eq!(actual_sha256, expected_sha256, "{label}");
        }

        let canonical_rule_contract = serde_json::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&rule_contract_bytes).unwrap(),
        )
        .unwrap();
        assert_eq!(
            Sha256::digest(&canonical_rule_contract)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "cebc062f771f27c5c46e0e83a74ab7c7c9f6e3a172b2cf1fe01bce0a7f6f6c21",
            "canonical v1 rule contract"
        );

        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
        assert_eq!(
            manifest["rule_contract"]["repository_path"],
            "crates/tex-tfm-metrics/tests/fixtures/tfm-validation-rules-v1.json"
        );
        assert_eq!(
            manifest["rule_contract"]["sha256"],
            "260bff0aa11e2b839985875c6df1e5075b37fc110ab0c3723a2837db70d20353"
        );
        assert_eq!(
            manifest["source_oracle"]["repository_path"],
            "crates/tex-tfm-metrics/tests/fixtures/tfm-validity-oracle-v1.json"
        );
        assert_eq!(
            manifest["source_oracle"]["sha256"],
            "9df44bf4b157acfb65fa0d5cc7de4d42ba7f869bae460e07daf984e1fbca19b4"
        );
        let transition: serde_json::Value = serde_json::from_slice(&rule_transition_bytes).unwrap();
        assert_eq!(transition["schema_version"], 2);
        assert_eq!(
            transition["ownership_changes"],
            serde_json::json!([{
                "rule_id": "TFM-KERN-001",
                "from": "LigKernCheckedTfm",
                "to": "KernCheckedTfm",
            }])
        );
        manifest
    }

    #[test]
    fn content_addressed_native_corpus_matches_header_proof_ownership() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let corpus_root = fixture_root.join("tfm-validity-oracle-v2");
        let manifest = reviewed_corpus_manifest(&fixture_root);
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
    fn content_addressed_native_corpus_matches_character_proof_ownership() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let corpus_root = fixture_root.join("tfm-validity-oracle-v2");
        let manifest = reviewed_corpus_manifest(&fixture_root);
        let rule_contract: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixture_root.join("tfm-validation-rules-v1.json")).unwrap(),
        )
        .unwrap();
        let proof_rules = |proof_state| {
            rule_contract["rules"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|rule| rule["proof_state"] == proof_state)
                .map(|rule| rule["id"].as_str().unwrap())
                .collect::<HashSet<_>>()
        };
        let header_rules = proof_rules("HeaderCheckedTfm");
        let character_rules = proof_rules("CharacterCheckedTfm");
        assert_eq!(header_rules.len(), 10);
        assert_eq!(character_rules.len(), 4);

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
            let classification = case["expected_classification"].as_str().unwrap();
            let first_rule = case["first_rejecting_rule"].as_str();
            let header = check_preamble_header(Arc::from(raw), input_size);

            if classification == "InvalidEffectiveSize" {
                assert!(
                    matches!(header, Err(PreambleHeaderFailure::InvalidEffectiveSize)),
                    "{case_id} {blob_sha256}"
                );
                continue;
            }
            if first_rule.is_some_and(|rule| header_rules.contains(rule)) {
                assert!(
                    matches!(header, Err(PreambleHeaderFailure::Malformed(_))),
                    "{case_id} {blob_sha256}"
                );
                continue;
            }

            let result = check_characters(header.unwrap_or_else(|failure| {
                panic!("{case_id} {blob_sha256} failed in header phase: {failure:?}")
            }));
            if first_rule.is_some_and(|rule| character_rules.contains(rule)) {
                let expected = match case_id {
                    "invalid_character_width_index" => {
                        CharacterValidationRule::MetricIndexOutOfRange {
                            character: 0,
                            metric: CharacterMetric::Width,
                            index: 36,
                            count: 36,
                        }
                    }
                    "invalid_character_height_index" => {
                        CharacterValidationRule::MetricIndexOutOfRange {
                            character: 0,
                            metric: CharacterMetric::Height,
                            index: 15,
                            count: 15,
                        }
                    }
                    "invalid_character_depth_index" => {
                        CharacterValidationRule::MetricIndexOutOfRange {
                            character: 0,
                            metric: CharacterMetric::Depth,
                            index: 10,
                            count: 10,
                        }
                    }
                    "invalid_character_italic_index" => {
                        CharacterValidationRule::MetricIndexOutOfRange {
                            character: 0,
                            metric: CharacterMetric::Italic,
                            index: 5,
                            count: 5,
                        }
                    }
                    "invalid_character_ligature_index" => {
                        CharacterValidationRule::LigatureIndexOutOfRange {
                            character: 0,
                            index: 88,
                            count: 88,
                        }
                    }
                    "invalid_character_extensible_index" => {
                        CharacterValidationRule::ExtensibleIndexOutOfRange {
                            character: 0,
                            index: 0,
                            count: 0,
                        }
                    }
                    "charlist_out_of_range" => CharacterValidationRule::CharListTargetOutOfRange {
                        character: 0,
                        target: 255,
                        first: 0,
                        last: 127,
                    },
                    "charlist_self_cycle" => {
                        CharacterValidationRule::CharListCycle { character: 0 }
                    }
                    "charlist_two_node_cycle" | "charlist_three_node_cycle" => {
                        CharacterValidationRule::CharListCycle { character: 127 }
                    }
                    _ => panic!("missing exact character rule for {case_id}"),
                };
                assert_eq!(result.err(), Some(expected), "{case_id} {blob_sha256}");
            } else {
                assert!(result.is_ok(), "{case_id} {blob_sha256}");
            }
        }
    }

    #[test]
    fn content_addressed_native_corpus_matches_box_proof_ownership() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let corpus_root = fixture_root.join("tfm-validity-oracle-v2");
        let manifest = reviewed_corpus_manifest(&fixture_root);
        let rule_contract: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixture_root.join("tfm-validation-rules-v1.json")).unwrap(),
        )
        .unwrap();
        let proof_rules = |proof_state| {
            rule_contract["rules"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|rule| rule["proof_state"] == proof_state)
                .map(|rule| rule["id"].as_str().unwrap())
                .collect::<HashSet<_>>()
        };
        let header_rules = proof_rules("HeaderCheckedTfm");
        let character_rules = proof_rules("CharacterCheckedTfm");
        let box_rules = proof_rules("BoxCheckedTfm");
        assert_eq!(header_rules.len(), 10);
        assert_eq!(character_rules.len(), 4);
        assert_eq!(box_rules.len(), 3);

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
            let classification = case["expected_classification"].as_str().unwrap();
            let first_rule = case["first_rejecting_rule"].as_str();
            let header = check_preamble_header(Arc::from(raw), input_size);

            if classification == "InvalidEffectiveSize" {
                assert!(
                    matches!(header, Err(PreambleHeaderFailure::InvalidEffectiveSize)),
                    "{case_id} {blob_sha256}"
                );
                continue;
            }
            if first_rule.is_some_and(|rule| header_rules.contains(rule)) {
                assert!(
                    matches!(header, Err(PreambleHeaderFailure::Malformed(_))),
                    "{case_id} {blob_sha256}"
                );
                continue;
            }

            let character = check_characters(header.unwrap_or_else(|failure| {
                panic!("{case_id} {blob_sha256} failed in header phase: {failure:?}")
            }));
            if first_rule.is_some_and(|rule| character_rules.contains(rule)) {
                assert!(character.is_err(), "{case_id} {blob_sha256}");
                continue;
            }

            let result = check_boxes(character.unwrap_or_else(|failure| {
                panic!("{case_id} {blob_sha256} failed in character phase: {failure:?}")
            }));
            if first_rule.is_some_and(|rule| box_rules.contains(rule)) {
                let expected = match case_id {
                    "invalid_width_fix_word_sign" => BoxValidationRule::InvalidFixWordSign {
                        table: BoxMetric::Width,
                        index: 1,
                        sign: 1,
                    },
                    "invalid_height_fix_word_sign" => BoxValidationRule::InvalidFixWordSign {
                        table: BoxMetric::Height,
                        index: 1,
                        sign: 1,
                    },
                    "invalid_depth_fix_word_sign" => BoxValidationRule::InvalidFixWordSign {
                        table: BoxMetric::Depth,
                        index: 1,
                        sign: 1,
                    },
                    "invalid_italic_fix_word_sign" => BoxValidationRule::InvalidFixWordSign {
                        table: BoxMetric::Italic,
                        index: 1,
                        sign: 1,
                    },
                    "nonzero_width_zero" | "nonzero_width_zero_at_16sp" => {
                        BoxValidationRule::NonzeroScaledEntryZero {
                            table: BoxMetric::Width,
                            scaled_sp: if input_size == 16 { 1 } else { 40_960 },
                        }
                    }
                    "nonzero_height_zero" | "nonzero_height_zero_at_16sp" => {
                        BoxValidationRule::NonzeroScaledEntryZero {
                            table: BoxMetric::Height,
                            scaled_sp: if input_size == 16 { 1 } else { 40_960 },
                        }
                    }
                    "nonzero_depth_zero" | "nonzero_depth_zero_at_16sp" => {
                        BoxValidationRule::NonzeroScaledEntryZero {
                            table: BoxMetric::Depth,
                            scaled_sp: if input_size == 16 { 1 } else { 40_960 },
                        }
                    }
                    "nonzero_italic_zero" | "nonzero_italic_zero_at_16sp" => {
                        BoxValidationRule::NonzeroScaledEntryZero {
                            table: BoxMetric::Italic,
                            scaled_sp: if input_size == 16 { 1 } else { 40_960 },
                        }
                    }
                    _ => panic!("missing exact box rule for {case_id}"),
                };
                assert_eq!(result.err(), Some(expected), "{case_id} {blob_sha256}");
            } else {
                assert!(result.is_ok(), "{case_id} {blob_sha256}");
            }
        }
    }

    #[test]
    fn content_addressed_native_corpus_matches_lig_kern_proof_ownership() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let corpus_root = fixture_root.join("tfm-validity-oracle-v2");
        let manifest = reviewed_corpus_manifest(&fixture_root);
        let rule_contract: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixture_root.join("tfm-validation-rules-v1.json")).unwrap(),
        )
        .unwrap();
        let proof_rules = |proof_state| {
            rule_contract["rules"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|rule| rule["proof_state"] == proof_state)
                .map(|rule| rule["id"].as_str().unwrap())
                .collect::<HashSet<_>>()
        };
        let header_rules = proof_rules("HeaderCheckedTfm");
        let character_rules = proof_rules("CharacterCheckedTfm");
        let box_rules = proof_rules("BoxCheckedTfm");
        let mut lig_kern_rules = proof_rules("LigKernCheckedTfm");
        assert!(lig_kern_rules.remove("TFM-KERN-001"));
        assert_eq!(lig_kern_rules.len(), 8);
        let transition: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixture_root.join("tfm-validation-rule-transition-v2.json")).unwrap(),
        )
        .unwrap();
        let mut runtime_projection_by_rule = HashMap::new();
        for projection in transition["source_predicate_projections"]
            .as_array()
            .unwrap()
        {
            let runtime_projection = projection["runtime_projection"].as_str().unwrap();
            for rule_id in projection["rule_ids"].as_array().unwrap() {
                assert!(
                    runtime_projection_by_rule
                        .insert(rule_id.as_str().unwrap(), runtime_projection)
                        .is_none()
                );
            }
        }
        assert_eq!(runtime_projection_by_rule.len(), 8);
        assert_eq!(
            runtime_projection_by_rule.get("TFM-LIGKERN-002"),
            Some(&"RestartTargetOutOfRange")
        );
        assert_eq!(
            runtime_projection_by_rule.get("TFM-LIGKERN-008"),
            Some(&"RestartTargetOutOfRange")
        );

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
            let first_rule = case["first_rejecting_rule"].as_str();
            let header = check_preamble_header(Arc::from(raw), input_size);

            if case["expected_classification"] == "InvalidEffectiveSize" {
                assert!(matches!(
                    header,
                    Err(PreambleHeaderFailure::InvalidEffectiveSize)
                ));
                continue;
            }
            if first_rule.is_some_and(|rule| header_rules.contains(rule)) {
                assert!(matches!(header, Err(PreambleHeaderFailure::Malformed(_))));
                continue;
            }
            let character = check_characters(header.unwrap_or_else(|failure| {
                panic!("{case_id} {blob_sha256} failed in header phase: {failure:?}")
            }));
            if first_rule.is_some_and(|rule| character_rules.contains(rule)) {
                assert!(character.is_err(), "{case_id} {blob_sha256}");
                continue;
            }
            let box_state = check_boxes(character.unwrap_or_else(|failure| {
                panic!("{case_id} {blob_sha256} failed in character phase: {failure:?}")
            }));
            if first_rule.is_some_and(|rule| box_rules.contains(rule)) {
                assert!(box_state.is_err(), "{case_id} {blob_sha256}");
                continue;
            }

            let result = check_lig_kern(box_state.unwrap_or_else(|failure| {
                panic!("{case_id} {blob_sha256} failed in box phase: {failure:?}")
            }));
            if first_rule.is_some_and(|rule| lig_kern_rules.contains(rule)) {
                let first_rule = first_rule.unwrap();
                let actual_projection = match result.as_ref().err().unwrap() {
                    LigKernValidationRule::RestartTargetOutOfRange { .. } => {
                        "RestartTargetOutOfRange"
                    }
                    LigKernValidationRule::NextCharacterMissing { .. } => "NextCharacterMissing",
                    LigKernValidationRule::LigatureTargetMissing { .. } => "LigatureTargetMissing",
                    LigKernValidationRule::KernIndexOutOfRange { .. } => "KernIndexOutOfRange",
                    LigKernValidationRule::ForwardSkipOutOfRange { .. } => "ForwardSkipOutOfRange",
                };
                assert_eq!(
                    runtime_projection_by_rule.get(first_rule),
                    Some(&actual_projection),
                    "{case_id} {first_rule}"
                );
                let expected = match case_id {
                    "invalid_boundary_label" => LigKernValidationRule::RestartTargetOutOfRange {
                        instruction: 87,
                        target: 88,
                        count: 88,
                    },
                    "invalid_ligkern" => LigKernValidationRule::RestartTargetOutOfRange {
                        instruction: 0,
                        target: 256,
                        count: 88,
                    },
                    "invalid_ligkern_next_character" => {
                        LigKernValidationRule::NextCharacterMissing {
                            instruction: 0,
                            character: 255,
                        }
                    }
                    "ligkern_next_in_range_absent" => LigKernValidationRule::NextCharacterMissing {
                        instruction: 0,
                        character: 127,
                    },
                    "invalid_ligature_target" => LigKernValidationRule::LigatureTargetMissing {
                        instruction: 0,
                        character: 255,
                    },
                    "ligature_target_in_range_absent" => {
                        LigKernValidationRule::LigatureTargetMissing {
                            instruction: 0,
                            character: 127,
                        }
                    }
                    "invalid_ligkern_kern_index" => LigKernValidationRule::KernIndexOutOfRange {
                        instruction: 0,
                        index: 10,
                        count: 10,
                    },
                    "invalid_ligkern_skip" => LigKernValidationRule::ForwardSkipOutOfRange {
                        instruction: 0,
                        skip: 127,
                        target: 128,
                        count: 88,
                    },
                    _ => panic!("missing exact lig/kern rule for {case_id}"),
                };
                assert_eq!(result.err(), Some(expected), "{case_id} {blob_sha256}");
            } else {
                assert!(result.is_ok(), "{case_id} {blob_sha256}");
            }
        }
    }

    #[test]
    fn character_entrypoint_consumes_only_the_header_state() {
        let _: fn(HeaderCheckedTfm) -> Result<CharacterCheckedTfm, CharacterValidationRule> =
            check_characters;
    }

    #[test]
    fn box_entrypoint_consumes_only_the_character_state() {
        let _: fn(CharacterCheckedTfm) -> Result<BoxCheckedTfm, BoxValidationRule> = check_boxes;
    }

    #[test]
    fn lig_kern_entrypoint_consumes_only_the_box_state() {
        let _: fn(BoxCheckedTfm) -> Result<LigKernCheckedTfm, LigKernValidationRule> =
            check_lig_kern;
    }

    #[test]
    fn kern_entrypoint_consumes_only_the_lig_kern_state() {
        let _: fn(LigKernCheckedTfm) -> Result<KernCheckedTfm, KernValidationRule> = check_kerns;
    }

    #[test]
    fn extensible_entrypoint_consumes_only_the_kern_state() {
        let _: fn(KernCheckedTfm) -> Result<ExtensibleCheckedTfm, ExtensibleValidationRule> =
            check_extensibles;
    }

    #[test]
    fn parameter_entrypoint_consumes_only_the_extensible_state() {
        let _: fn(ExtensibleCheckedTfm) -> Result<ParameterCheckedTfm, ParameterValidationRule> =
            check_parameters;
    }

    #[test]
    fn parameter_phase_source_contract_is_content_addressed() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let bytes =
            std::fs::read(fixture_root.join("tfm-parameter-source-contract-v1.json")).unwrap();
        assert_eq!(
            Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "223aad57857393d02096adbdaa9cc587be13c515e9e7e86e1b19454f0c8164dd"
        );
        let contract: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(contract["schema_version"], 1);
        assert_eq!(contract["proof_boundary"]["input"], "ExtensibleCheckedTfm");
        assert_eq!(contract["proof_boundary"]["output"], "ParameterCheckedTfm");
        assert_eq!(
            contract["proof_boundary"]["owned_rule_ids"],
            serde_json::json!(["TFM-PARAM-001", "TFM-PARAM-002", "TFM-PARAM-003"])
        );
        assert_eq!(contract["proof_boundary"]["loop_cardinality"], "np");
        assert_eq!(
            contract["proof_boundary"]["absolute_valid_parameter_count"],
            32_755
        );
        assert_eq!(contract["proof_boundary"]["standard_parameter_count"], 7);
        assert_eq!(
            contract["proof_boundary"]["excluded_reads"],
            serde_json::json!(["eof_state", "raw_suffix", "final_adjustments"])
        );
    }

    #[test]
    fn extensible_phase_source_contract_is_content_addressed() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let bytes =
            std::fs::read(fixture_root.join("tfm-extensible-source-contract-v1.json")).unwrap();
        assert_eq!(
            Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "5ce088a9e04d5de598fbabd4d59347f0e7c089f7cb491ebffe83314d3fc9ebdd"
        );
        let contract: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(contract["schema_version"], 1);
        assert_eq!(contract["proof_boundary"]["input"], "KernCheckedTfm");
        assert_eq!(contract["proof_boundary"]["output"], "ExtensibleCheckedTfm");
        assert_eq!(
            contract["proof_boundary"]["owned_rule_ids"],
            serde_json::json!(["TFM-EXT-001", "TFM-EXT-002"])
        );
        assert_eq!(contract["proof_boundary"]["loop_cardinality"], "ne");
        assert_eq!(
            contract["proof_boundary"]["absolute_valid_recipe_count"],
            32_753
        );
        assert_eq!(
            contract["proof_boundary"]["field_order"],
            serde_json::json!(["top", "middle", "bottom", "repeat"])
        );
        assert_eq!(
            contract["proof_boundary"]["recipe_fields"][0]["zero_semantics"],
            "absent_optional"
        );
        assert_eq!(
            contract["proof_boundary"]["recipe_fields"][3]["zero_semantics"],
            "mandatory_character_code"
        );
    }

    #[test]
    fn kern_phase_source_contract_is_content_addressed() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let bytes = std::fs::read(fixture_root.join("tfm-kern-source-contract-v1.json")).unwrap();
        assert_eq!(
            Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "19d08087ce4b96bc4e3e9059e161adfd4705157e5a7e768190695155b7c9b2a1"
        );
        let contract: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(contract["schema_version"], 1);
        assert_eq!(contract["proof_boundary"]["input"], "LigKernCheckedTfm");
        assert_eq!(contract["proof_boundary"]["output"], "KernCheckedTfm");
        assert_eq!(
            contract["proof_boundary"]["owned_rule_ids"],
            serde_json::json!(["TFM-KERN-001"])
        );
        assert_eq!(contract["proof_boundary"]["loop_cardinality"], "nk");
        assert_eq!(contract["proof_boundary"]["entry_zero_check"], false);
    }

    #[test]
    fn empty_lig_kern_table_retains_the_exact_box_predecessor_and_empty_state() {
        let state = check_lig_kern(
            check_boxes(
                check_characters(check_preamble_header(Arc::from(seed_frame()), 1).unwrap())
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

        assert!(state.instructions.is_empty());
        assert_eq!(state.boundary_character, None);
        assert_eq!(state.boundary_program_start, None);
        assert_eq!(state.predecessor.predecessor.records.len(), 0);
        assert_eq!(state.predecessor.predecessor.predecessor.raw_counts.nl, 0);
    }

    #[test]
    fn lig_kern_decodes_instructions_and_retains_both_boundary_states() {
        let bytes = lig_kern_frame(
            &[[1, 0, 0, 0]],
            &[
                [255, 42, 0, 2],
                [128, 42, 0, 7],
                [128, 7, 128, 0],
                [255, 0, 0, 1],
            ],
            1,
        );
        let state = check_lig_kern(
            check_boxes(
                check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(state.instructions.len(), 4);
        assert_eq!(state.instructions[0].skip_byte, 255);
        assert_eq!(state.instructions[1].next_character, 42);
        assert_eq!(state.instructions[2].operation_byte, 128);
        assert_eq!(state.instructions[3].remainder, 1);
        assert_eq!(state.boundary_character, Some(42));
        assert_eq!(state.boundary_program_start, Some(1));
    }

    #[test]
    fn lig_kern_failures_preserve_instruction_source_order() {
        let cases: Vec<(Vec<[u8; 4]>, u16, LigKernValidationRule)> = vec![
            (
                vec![[129, 0, 0, 1]],
                0,
                LigKernValidationRule::RestartTargetOutOfRange {
                    instruction: 0,
                    target: 1,
                    count: 1,
                },
            ),
            (
                vec![[255, 8, 0, 1]],
                0,
                LigKernValidationRule::RestartTargetOutOfRange {
                    instruction: 0,
                    target: 1,
                    count: 1,
                },
            ),
            (
                vec![[128, 8, 0, 8]],
                0,
                LigKernValidationRule::NextCharacterMissing {
                    instruction: 0,
                    character: 8,
                },
            ),
            (
                vec![[128, 7, 0, 8]],
                0,
                LigKernValidationRule::LigatureTargetMissing {
                    instruction: 0,
                    character: 8,
                },
            ),
            (
                vec![[0, 7, 0, 8]],
                0,
                LigKernValidationRule::LigatureTargetMissing {
                    instruction: 0,
                    character: 8,
                },
            ),
            (
                vec![[128, 7, 128, 1]],
                1,
                LigKernValidationRule::KernIndexOutOfRange {
                    instruction: 0,
                    index: 1,
                    count: 1,
                },
            ),
            (
                vec![[0, 7, 128, 1]],
                1,
                LigKernValidationRule::KernIndexOutOfRange {
                    instruction: 0,
                    index: 1,
                    count: 1,
                },
            ),
            (
                vec![[0, 7, 0, 7]],
                0,
                LigKernValidationRule::ForwardSkipOutOfRange {
                    instruction: 0,
                    skip: 0,
                    target: 1,
                    count: 1,
                },
            ),
        ];
        for (instructions, nk, expected) in cases {
            let bytes = lig_kern_frame(&[[1, 0, 0, 0]], &instructions, nk);
            let result = check_lig_kern(
                check_boxes(
                    check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap(),
                )
                .unwrap(),
            );
            assert_eq!(result.err(), Some(expected));
        }

        let compound = lig_kern_frame(&[[1, 0, 0, 0]], &[[0, 8, 128, 1]], 1);
        let result = check_lig_kern(
            check_boxes(
                check_characters(check_preamble_header(Arc::from(compound), 1).unwrap()).unwrap(),
            )
            .unwrap(),
        );
        assert_eq!(
            result.err(),
            Some(LigKernValidationRule::NextCharacterMissing {
                instruction: 0,
                character: 8,
            })
        );
    }

    #[test]
    fn only_the_first_marker_installs_boundary_character_and_only_final_marker_labels() {
        let bytes = lig_kern_frame(
            &[[1, 0, 0, 0]],
            &[[129, 0, 0, 0], [255, 8, 0, 0], [128, 8, 0, 7]],
            0,
        );
        let result = check_lig_kern(
            check_boxes(
                check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap(),
            )
            .unwrap(),
        );
        assert_eq!(
            result.err(),
            Some(LigKernValidationRule::NextCharacterMissing {
                instruction: 2,
                character: 8,
            })
        );

        let bytes = lig_kern_frame(&[[1, 0, 0, 0]], &[[255, 8, 0, 1], [128, 8, 0, 7]], 0);
        let state = check_lig_kern(
            check_boxes(
                check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(state.boundary_character, Some(8));
        assert_eq!(state.boundary_program_start, None);

        let bytes = lig_kern_frame(&[[1, 0, 0, 0]], &[[255, 8, 0, 0]], 0);
        let state = check_lig_kern(
            check_boxes(
                check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(state.boundary_character, Some(8));
        assert_eq!(state.boundary_program_start, Some(0));
    }

    #[test]
    fn generated_lig_kern_programs_match_an_independent_source_order_oracle() {
        let reference = |instructions: &[[u8; 4]], kern_count: u16| {
            let count = u16::try_from(instructions.len()).unwrap();
            let mut boundary_character = None;
            for (instruction_index, [skip, next, operation, remainder]) in
                instructions.iter().copied().enumerate()
            {
                let instruction = u16::try_from(instruction_index).unwrap();
                if skip > 128 {
                    let target = u16::from(operation) * 256 + u16::from(remainder);
                    if target >= count {
                        return Err(LigKernValidationRule::RestartTargetOutOfRange {
                            instruction,
                            target,
                            count,
                        });
                    }
                    if skip == 255 && instruction == 0 {
                        boundary_character = Some(next);
                    }
                    continue;
                }
                if boundary_character != Some(next) && next != 7 && next != 9 {
                    return Err(LigKernValidationRule::NextCharacterMissing {
                        instruction,
                        character: next,
                    });
                }
                if operation < 128 {
                    if remainder != 7 && remainder != 9 {
                        return Err(LigKernValidationRule::LigatureTargetMissing {
                            instruction,
                            character: remainder,
                        });
                    }
                } else {
                    let index = u16::from(operation - 128) * 256 + u16::from(remainder);
                    if index >= kern_count {
                        return Err(LigKernValidationRule::KernIndexOutOfRange {
                            instruction,
                            index,
                            count: kern_count,
                        });
                    }
                }
                if skip < 128 {
                    let target = u32::from(instruction) + u32::from(skip) + 1;
                    if target >= u32::from(count) {
                        return Err(LigKernValidationRule::ForwardSkipOutOfRange {
                            instruction,
                            skip,
                            target,
                            count,
                        });
                    }
                }
            }
            let boundary_program_start =
                instructions
                    .last()
                    .and_then(|[skip, _, operation, remainder]| {
                        (*skip == 255).then(|| u16::from(*operation) * 256 + u16::from(*remainder))
                    });
            Ok((boundary_character, boundary_program_start))
        };

        let mut programs = Vec::new();
        for skip in [0, 127, 128, 129, 255] {
            for next in [7, 8, 9] {
                for operation in [0, 127, 128, 255] {
                    for remainder in [7, 8, 9, 255] {
                        programs.push(vec![[skip, next, operation, remainder]]);
                    }
                }
            }
        }
        let mut generator = 0x6a93_a948_81a8_83eeu64;
        for _ in 0..4096 {
            let length = 1 + (generator as usize % 8);
            let mut program = Vec::with_capacity(length);
            for _ in 0..length {
                let mut instruction = [0; 4];
                for byte in &mut instruction {
                    generator = generator
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1);
                    *byte = (generator >> 32) as u8;
                }
                program.push(instruction);
            }
            programs.push(program);
        }

        for program in programs {
            let kern_count = 3;
            let expected = reference(&program, kern_count);
            let bytes = lig_kern_frame(
                &[[1, 0, 0, 0], [0, 0, 0, 0], [1, 0, 0, 0]],
                &program,
                kern_count,
            );
            let actual = check_lig_kern(
                check_boxes(
                    check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap(),
                )
                .unwrap(),
            );
            match (actual, expected) {
                (Ok(actual), Ok((boundary_character, boundary_program_start))) => {
                    assert_eq!(actual.boundary_character, boundary_character, "{program:?}");
                    assert_eq!(
                        actual.boundary_program_start, boundary_program_start,
                        "{program:?}"
                    );
                    assert_eq!(actual.instructions.len(), program.len(), "{program:?}");
                }
                (Err(actual), Err(expected)) => assert_eq!(actual, expected, "{program:?}"),
                _ => panic!("program {program:?}: success/error classification differs"),
            }
        }
    }

    #[test]
    fn absolute_maximum_lig_kern_table_is_bounded_and_decoded_completely() {
        let instructions = vec![[129, 0, 0, 0]; 32_755];
        let bytes = maximum_lig_kern_frame(&instructions);
        let state = check_lig_kern(
            check_boxes(
                check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            state.predecessor.predecessor.predecessor.raw_counts.lf,
            32_767
        );
        assert_eq!(
            state.predecessor.predecessor.predecessor.raw_counts.nl,
            32_755
        );
        assert_eq!(state.instructions.len(), 32_755);
        assert_eq!(state.boundary_character, None);
        assert_eq!(state.boundary_program_start, None);
    }

    #[test]
    fn high_restart_forward_and_kern_boundaries_accept_count_minus_one_only() {
        let mut restart_instructions = vec![[129, 0, 0, 0]; 32_755];
        restart_instructions[32_754] = [129, 0, 127, 242];
        let accepted = maximum_lig_kern_frame(&restart_instructions);
        assert!(
            check_lig_kern(
                check_boxes(
                    check_characters(check_preamble_header(Arc::from(accepted), 1).unwrap())
                        .unwrap(),
                )
                .unwrap(),
            )
            .is_ok()
        );

        restart_instructions[32_754] = [129, 0, 127, 243];
        let rejected = maximum_lig_kern_frame(&restart_instructions);
        let result = check_lig_kern(
            check_boxes(
                check_characters(check_preamble_header(Arc::from(rejected), 1).unwrap()).unwrap(),
            )
            .unwrap(),
        );
        assert_eq!(
            result.err(),
            Some(LigKernValidationRule::RestartTargetOutOfRange {
                instruction: 32_754,
                target: 32_755,
                count: 32_755,
            })
        );

        let mut forward_instructions = vec![[128, 7, 0, 7]; 32_753];
        forward_instructions[32_624] = [127, 7, 0, 7];
        let accepted = lig_kern_frame(&[[1, 0, 0, 0]], &forward_instructions, 0);
        assert!(
            check_lig_kern(
                check_boxes(
                    check_characters(check_preamble_header(Arc::from(accepted), 1).unwrap())
                        .unwrap(),
                )
                .unwrap(),
            )
            .is_ok()
        );

        forward_instructions[32_625] = [127, 7, 0, 7];
        let rejected = lig_kern_frame(&[[1, 0, 0, 0]], &forward_instructions, 0);
        let result = check_lig_kern(
            check_boxes(
                check_characters(check_preamble_header(Arc::from(rejected), 1).unwrap()).unwrap(),
            )
            .unwrap(),
        );
        assert_eq!(
            result.err(),
            Some(LigKernValidationRule::ForwardSkipOutOfRange {
                instruction: 32_625,
                skip: 127,
                target: 32_753,
                count: 32_753,
            })
        );

        for (remainder, expected) in [(239, None), (240, Some(32_752))] {
            let bytes = lig_kern_frame(&[[1, 0, 0, 0]], &[[128, 7, 255, remainder]], 32_752);
            let result = check_lig_kern(
                check_boxes(
                    check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap(),
                )
                .unwrap(),
            );
            match expected {
                None => assert!(result.is_ok()),
                Some(index) => assert_eq!(
                    result.err(),
                    Some(LigKernValidationRule::KernIndexOutOfRange {
                        instruction: 0,
                        index,
                        count: 32_752,
                    })
                ),
            }
        }
    }

    #[test]
    fn kern_words_and_raw_suffixes_do_not_change_lig_kern_semantics() {
        let bytes = lig_kern_frame(&[[1, 0, 0, 0]], &[[128, 7, 128, 1]], 2);
        let control = check_lig_kern(
            check_boxes(
                check_characters(check_preamble_header(Arc::from(bytes.clone()), 65_536).unwrap())
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let layout = control.predecessor.predecessor.predecessor.layout.clone();

        let mut invalid_kern_words = bytes.clone();
        invalid_kern_words[layout.kerns.clone()].fill(0x7f);
        let kern_mutant = check_lig_kern(
            check_boxes(
                check_characters(
                    check_preamble_header(Arc::from(invalid_kern_words), 65_536).unwrap(),
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(kern_mutant.instructions, control.instructions);
        assert_eq!(kern_mutant.boundary_character, control.boundary_character);
        assert_eq!(
            kern_mutant.boundary_program_start,
            control.boundary_program_start
        );

        let mut suffixed = bytes;
        suffixed.extend((0..8193).map(|index| (index as u8).wrapping_mul(29)));
        let suffix_state = check_lig_kern(
            check_boxes(
                check_characters(check_preamble_header(Arc::from(suffixed), 65_536).unwrap())
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(suffix_state.instructions, control.instructions);
        assert_eq!(
            suffix_state
                .predecessor
                .predecessor
                .predecessor
                .frame_digest,
            control.predecessor.predecessor.predecessor.frame_digest
        );
        assert_ne!(
            suffix_state.predecessor.predecessor.predecessor.raw_digest,
            control.predecessor.predecessor.predecessor.raw_digest
        );
    }

    #[test]
    fn empty_kern_table_retains_the_exact_lig_kern_predecessor() {
        let state = check_kern_frame(kern_frame(&[], &[], &[]), 1).unwrap();

        assert!(state.kerns.is_empty());
        assert!(state.predecessor.instructions.is_empty());
        assert_eq!(
            state
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .raw_counts
                .nk,
            0
        );
    }

    #[test]
    fn kern_scaling_matches_literal_source_for_valid_signs_and_boundaries() {
        let words = [
            [0, 0, 0, 0],
            [0, 0, 0, 1],
            [0, 0, 0, 255],
            [0, 0, 1, 0],
            [0, 15, 255, 255],
            [0, 16, 0, 0],
            [0, 255, 255, 255],
            [255, 255, 255, 255],
            [255, 240, 0, 0],
            [255, 0, 0, 0],
        ];
        let sizes = [
            1,
            2,
            15,
            16,
            17,
            65_535,
            65_536,
            65_537,
            8_388_607,
            8_388_608,
            8_388_609,
            16_777_215,
            16_777_216,
            16_777_217,
            33_554_431,
            33_554_432,
            33_554_433,
            67_108_863,
            67_108_864,
            67_108_865,
            MAX_TEX_FONT_SIZE_SP - 1,
        ];

        for size in sizes {
            let state = check_kern_frame(kern_frame(&words, &[], &[]), size).unwrap();
            let expected = words.map(|[sign, b, c, d]| {
                let mut reduced_size = i64::from(size);
                let mut alpha = 16i64;
                while reduced_size >= 1 << 23 {
                    reduced_size /= 2;
                    alpha += alpha;
                }
                let beta = 256 / alpha;
                alpha *= reduced_size;
                let positive_fraction =
                    ((i64::from(d) * reduced_size / 256 + i64::from(c) * reduced_size) / 256
                        + i64::from(b) * reduced_size)
                        / beta;
                let scaled = match sign {
                    0 => positive_fraction,
                    255 => positive_fraction - alpha,
                    _ => unreachable!("the test matrix contains only valid signs"),
                };
                ScaledSp(i32::try_from(scaled).unwrap())
            });
            assert_eq!(state.kerns.as_ref(), expected, "effective size {size}");
        }
    }

    #[test]
    fn every_forbidden_kern_sign_rejects_with_exact_index_and_sign() {
        for sign in 1..=254 {
            let result = check_kern_frame(kern_frame(&[[sign, 0, 0, 0]], &[], &[]), 1);
            assert_eq!(
                result.err(),
                Some(KernValidationRule::InvalidFixWordSign { index: 0, sign })
            );
        }
    }

    #[test]
    fn kern_failure_reports_the_first_invalid_word_in_source_order() {
        let result = check_kern_frame(
            kern_frame(&[[0, 16, 0, 0], [1, 0, 0, 0], [2, 0, 0, 0]], &[], &[]),
            65_536,
        );
        assert_eq!(
            result.err(),
            Some(KernValidationRule::InvalidFixWordSign { index: 1, sign: 1 })
        );
    }

    #[test]
    fn absolute_maximum_kern_table_is_scaled_completely() {
        let mut bytes = vec![0; 32_767 * 4];
        for (index, value) in [32_767, 2, 1, 0, 1, 1, 1, 1, 0, 32_755, 0, 0]
            .into_iter()
            .enumerate()
        {
            put_count(&mut bytes, index, value);
        }
        bytes[28..32].copy_from_slice(&(1i32 << 20).to_be_bytes());

        let state = check_kern_frame(bytes, MAX_TEX_FONT_SIZE_SP - 1).unwrap();
        assert_eq!(state.kerns.len(), 32_755);
        assert!(state.kerns.iter().all(|scaled| *scaled == ScaledSp(0)));
        assert_eq!(
            state
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .raw_counts
                .lf,
            32_767
        );
        assert_eq!(
            state
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .raw_counts
                .nk,
            32_755
        );
    }

    #[test]
    fn extensibles_parameters_and_suffix_do_not_change_kern_semantics() {
        let kerns = [[0, 16, 0, 0], [255, 240, 0, 0]];
        let control =
            check_kern_frame(kern_frame(&kerns, &[[0, 0, 0, 0]], &[[0, 0, 0, 0]]), 65_536).unwrap();
        let later_table_mutant =
            check_kern_frame(kern_frame(&kerns, &[[1, 2, 3, 4]], &[[1, 2, 3, 4]]), 65_536).unwrap();
        assert_eq!(later_table_mutant.kerns, control.kerns);

        let mut suffixed = kern_frame(&kerns, &[[0, 0, 0, 0]], &[[0, 0, 0, 0]]);
        suffixed.extend((0..8193).map(|index| (index as u8).wrapping_mul(31)));
        let suffix_state = check_kern_frame(suffixed, 65_536).unwrap();
        assert_eq!(suffix_state.kerns, control.kerns);
        assert_eq!(
            suffix_state
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .frame_digest,
            control
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .frame_digest
        );
        assert_ne!(
            suffix_state
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .raw_digest,
            control
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .raw_digest
        );
    }

    #[test]
    fn kern_success_retains_all_lig_kern_state_and_the_same_raw_allocation() {
        let mut bytes = lig_kern_frame(&[[1, 0, 0, 0]], &[[255, 7, 0, 0]], 1);
        let layout = check_preamble_header(Arc::from(bytes.clone()), 65_536)
            .unwrap()
            .layout;
        bytes[layout.kerns].copy_from_slice(&[0, 16, 0, 0]);
        let raw: Arc<[u8]> = Arc::from(bytes);
        let retained = Arc::clone(&raw);
        let state = check_kerns(
            check_lig_kern(
                check_boxes(check_characters(check_preamble_header(raw, 65_536).unwrap()).unwrap())
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(state.kerns.as_ref(), [ScaledSp(65_536)]);
        assert_eq!(state.predecessor.instructions.len(), 1);
        assert_eq!(state.predecessor.boundary_character, Some(7));
        assert_eq!(state.predecessor.boundary_program_start, Some(0));
        assert!(Arc::ptr_eq(
            &retained,
            &state.predecessor.predecessor.predecessor.predecessor.raw
        ));
    }

    #[test]
    fn persisted_corpus_moves_only_kern_rule_to_the_kern_phase() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let corpus_root = fixture_root.join("tfm-validity-oracle-v2");
        let manifest = reviewed_corpus_manifest(&fixture_root);
        let rule_contract: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixture_root.join("tfm-validation-rules-v1.json")).unwrap(),
        )
        .unwrap();
        let tail_rules = rule_contract["rules"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|rule| rule["proof_state"] == "TailCheckedTfm")
            .map(|rule| rule["id"].as_str().unwrap())
            .collect::<HashSet<_>>();
        let mut kern_cases = 0;
        let mut later_cases = 0;

        for case in manifest["cases"].as_array().unwrap() {
            let first_rule = case["first_rejecting_rule"].as_str();
            if first_rule != Some("TFM-KERN-001")
                && !first_rule.is_some_and(|rule| tail_rules.contains(rule))
            {
                continue;
            }
            let case_id = case["id"].as_str().unwrap();
            let blob_sha256 = case["blob_sha256"].as_str().unwrap();
            let raw = std::fs::read(corpus_root.join("blobs").join(format!("{blob_sha256}.tfm")))
                .unwrap();
            let input_size =
                i32::try_from(case["validator_input_size_sp"].as_i64().unwrap()).unwrap();
            let lig_kern = check_lig_kern(
                check_boxes(
                    check_characters(check_preamble_header(Arc::from(raw), input_size).unwrap())
                        .unwrap(),
                )
                .unwrap(),
            )
            .unwrap_or_else(|failure| {
                panic!("{case_id} {blob_sha256} failed before kern phase: {failure:?}")
            });
            let result = check_kerns(lig_kern);
            if first_rule == Some("TFM-KERN-001") {
                kern_cases += 1;
                assert_eq!(
                    result.err(),
                    Some(KernValidationRule::InvalidFixWordSign { index: 0, sign: 1 }),
                    "{case_id} {blob_sha256}"
                );
            } else {
                later_cases += 1;
                assert!(result.is_ok(), "{case_id} {blob_sha256}");
            }
        }
        assert_eq!(kern_cases, 1);
        assert!(later_cases > 0);
    }

    #[test]
    fn empty_extensible_table_retains_the_exact_kern_predecessor() {
        let state = check_extensible_frame(kern_frame(&[], &[], &[]), 1).unwrap();

        assert!(state.extensibles.is_empty());
        assert!(state.predecessor.kerns.is_empty());
        assert_eq!(
            state
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .raw_counts
                .ne,
            0
        );
    }

    #[test]
    fn extensible_recipes_decode_optional_zero_and_existing_parts_in_source_order() {
        let bytes = extensible_frame_at(
            7,
            &[[1, 0, 0, 0], [0, 0, 0, 0], [1, 0, 0, 0]],
            &[[0, 7, 0, 9], [7, 9, 7, 9]],
            &[],
        );
        let state = check_extensible_frame(bytes, 1).unwrap();

        assert_eq!(
            state.extensibles.as_ref(),
            [
                CheckedExtensibleRecipe {
                    top: None,
                    middle: Some(7),
                    bottom: None,
                    repeat: 9,
                },
                CheckedExtensibleRecipe {
                    top: Some(7),
                    middle: Some(9),
                    bottom: Some(7),
                    repeat: 9,
                },
            ]
        );
    }

    #[test]
    fn extensible_optional_and_repeat_failures_have_exact_payloads() {
        let records = [[1, 0, 0, 0], [0, 0, 0, 0], [1, 0, 0, 0]];
        for (recipe, expected) in [
            (
                [8, 0, 0, 7],
                ExtensibleValidationRule::OptionalPartMissing {
                    recipe: 0,
                    part: ExtensiblePart::Top,
                    character: 8,
                },
            ),
            (
                [7, 8, 0, 7],
                ExtensibleValidationRule::OptionalPartMissing {
                    recipe: 0,
                    part: ExtensiblePart::Middle,
                    character: 8,
                },
            ),
            (
                [7, 9, 8, 7],
                ExtensibleValidationRule::OptionalPartMissing {
                    recipe: 0,
                    part: ExtensiblePart::Bottom,
                    character: 8,
                },
            ),
            (
                [7, 9, 7, 8],
                ExtensibleValidationRule::RepeatMissing {
                    recipe: 0,
                    character: 8,
                },
            ),
        ] {
            let result =
                check_extensible_frame(extensible_frame_at(7, &records, &[recipe], &[]), 1);
            assert_eq!(result.err(), Some(expected), "recipe {recipe:?}");
        }
    }

    #[test]
    fn optional_zero_bypasses_existence_but_repeat_zero_is_mandatory() {
        let optional_zero = check_extensible_frame(
            extensible_frame_at(7, &[[1, 0, 0, 0]], &[[0, 0, 0, 7]], &[]),
            1,
        )
        .unwrap();
        assert_eq!(
            optional_zero.extensibles.as_ref(),
            [CheckedExtensibleRecipe {
                top: None,
                middle: None,
                bottom: None,
                repeat: 7,
            }]
        );

        let missing_zero = check_extensible_frame(
            extensible_frame_at(7, &[[1, 0, 0, 0]], &[[0, 0, 0, 0]], &[]),
            1,
        );
        assert_eq!(
            missing_zero.err(),
            Some(ExtensibleValidationRule::RepeatMissing {
                recipe: 0,
                character: 0,
            })
        );

        let existing_zero = check_extensible_frame(
            extensible_frame_at(0, &[[1, 0, 0, 0]], &[[0, 0, 0, 0]], &[]),
            1,
        )
        .unwrap();
        assert_eq!(existing_zero.extensibles[0].repeat, 0);
    }

    #[test]
    fn extensible_failure_reports_first_recipe_and_field_in_source_order() {
        let records = [[1, 0, 0, 0], [0, 0, 0, 0], [1, 0, 0, 0]];
        let first_recipe = check_extensible_frame(
            extensible_frame_at(7, &records, &[[0, 8, 0, 7], [8, 0, 0, 7]], &[]),
            1,
        );
        assert_eq!(
            first_recipe.err(),
            Some(ExtensibleValidationRule::OptionalPartMissing {
                recipe: 0,
                part: ExtensiblePart::Middle,
                character: 8,
            })
        );

        let first_field =
            check_extensible_frame(extensible_frame_at(7, &records, &[[8, 8, 8, 8]], &[]), 1);
        assert_eq!(
            first_field.err(),
            Some(ExtensibleValidationRule::OptionalPartMissing {
                recipe: 0,
                part: ExtensiblePart::Top,
                character: 8,
            })
        );
    }

    #[test]
    fn unreferenced_invalid_extensible_recipe_is_rejected() {
        let result = check_extensible_frame(
            extensible_frame_at(7, &[[1, 0, 0, 0]], &[[8, 0, 0, 7]], &[]),
            1,
        );
        assert_eq!(
            result.err(),
            Some(ExtensibleValidationRule::OptionalPartMissing {
                recipe: 0,
                part: ExtensiblePart::Top,
                character: 8,
            })
        );
    }

    #[test]
    fn absolute_maximum_extensible_table_is_checked_completely() {
        let mut bytes = vec![0; 32_767 * 4];
        for (index, value) in [32_767, 2, 0, 0, 2, 1, 1, 1, 0, 0, 32_753, 0]
            .into_iter()
            .enumerate()
        {
            put_count(&mut bytes, index, value);
        }
        bytes[28..32].copy_from_slice(&(1i32 << 20).to_be_bytes());
        bytes[32..36].copy_from_slice(&[1, 0, 0, 0]);

        let state = check_extensible_frame(bytes, 1).unwrap();
        assert_eq!(state.extensibles.len(), 32_753);
        assert!(state.extensibles.iter().all(|recipe| *recipe
            == CheckedExtensibleRecipe {
                top: None,
                middle: None,
                bottom: None,
                repeat: 0,
            }));
        assert_eq!(
            state
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .raw_counts
                .lf,
            32_767
        );
        assert_eq!(
            state
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .raw_counts
                .ne,
            32_753
        );
    }

    #[test]
    fn absolute_declared_extensible_geometry_rejects_first_mandatory_repeat() {
        let mut bytes = vec![0; 32_767 * 4];
        for (index, value) in [32_767, 2, 1, 0, 1, 1, 1, 1, 0, 0, 32_755, 0]
            .into_iter()
            .enumerate()
        {
            put_count(&mut bytes, index, value);
        }
        bytes[28..32].copy_from_slice(&(1i32 << 20).to_be_bytes());

        let result = check_extensible_frame(bytes, 1);
        assert_eq!(
            result.err(),
            Some(ExtensibleValidationRule::RepeatMissing {
                recipe: 0,
                character: 0,
            })
        );
    }

    #[test]
    fn parameters_and_suffix_do_not_change_extensible_semantics() {
        let records = [[1, 0, 0, 0]];
        let recipes = [[0, 0, 0, 7]];
        let control = check_extensible_frame(
            extensible_frame_at(7, &records, &recipes, &[[0, 0, 0, 0]]),
            1,
        )
        .unwrap();
        let parameter_mutant = check_extensible_frame(
            extensible_frame_at(7, &records, &recipes, &[[127, 1, 2, 3]]),
            1,
        )
        .unwrap();
        assert_eq!(parameter_mutant.extensibles, control.extensibles);

        let mut suffixed = extensible_frame_at(7, &records, &recipes, &[[0, 0, 0, 0]]);
        suffixed.extend((0..8193).map(|index| (index as u8).wrapping_mul(37)));
        let suffix_state = check_extensible_frame(suffixed, 1).unwrap();
        assert_eq!(suffix_state.extensibles, control.extensibles);
        assert_eq!(
            suffix_state
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .frame_digest,
            control
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .frame_digest
        );
        assert_ne!(
            suffix_state
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .raw_digest,
            control
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .raw_digest
        );
    }

    #[test]
    fn extensible_success_retains_the_same_raw_allocation_and_kern_state() {
        let bytes = extensible_frame_at(7, &[[1, 0, 0, 0]], &[[0, 0, 0, 7]], &[]);
        let raw: Arc<[u8]> = Arc::from(bytes);
        let retained = Arc::clone(&raw);
        let state = check_extensibles(
            check_kerns(
                check_lig_kern(
                    check_boxes(check_characters(check_preamble_header(raw, 1).unwrap()).unwrap())
                        .unwrap(),
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(state.extensibles[0].repeat, 7);
        assert!(state.predecessor.kerns.is_empty());
        assert!(Arc::ptr_eq(
            &retained,
            &state
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .raw
        ));
    }

    #[test]
    fn persisted_corpus_moves_only_extensible_rules_to_the_extensible_phase() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let corpus_root = fixture_root.join("tfm-validity-oracle-v2");
        let manifest = reviewed_corpus_manifest(&fixture_root);
        let mut extensible_cases = 0;
        let mut parameter_cases = 0;

        for case in manifest["cases"].as_array().unwrap() {
            let first_rule = case["first_rejecting_rule"].as_str();
            if !first_rule
                .is_some_and(|rule| rule.starts_with("TFM-EXT-") || rule.starts_with("TFM-PARAM-"))
            {
                continue;
            }
            let case_id = case["id"].as_str().unwrap();
            let blob_sha256 = case["blob_sha256"].as_str().unwrap();
            let raw = std::fs::read(corpus_root.join("blobs").join(format!("{blob_sha256}.tfm")))
                .unwrap();
            let input_size =
                i32::try_from(case["validator_input_size_sp"].as_i64().unwrap()).unwrap();
            let kern = check_kerns(
                check_lig_kern(
                    check_boxes(
                        check_characters(
                            check_preamble_header(Arc::from(raw), input_size).unwrap(),
                        )
                        .unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
            .unwrap_or_else(|failure| {
                panic!("{case_id} {blob_sha256} failed before extensible phase: {failure:?}")
            });
            let result = check_extensibles(kern);
            if first_rule.is_some_and(|rule| rule.starts_with("TFM-EXT-")) {
                extensible_cases += 1;
                let expected = match case_id {
                    "invalid_extensible_top" => ExtensibleValidationRule::OptionalPartMissing {
                        recipe: 0,
                        part: ExtensiblePart::Top,
                        character: 255,
                    },
                    "extensible_top_in_range_absent" => {
                        ExtensibleValidationRule::OptionalPartMissing {
                            recipe: 0,
                            part: ExtensiblePart::Top,
                            character: 1,
                        }
                    }
                    "invalid_extensible_middle" => ExtensibleValidationRule::OptionalPartMissing {
                        recipe: 0,
                        part: ExtensiblePart::Middle,
                        character: 255,
                    },
                    "extensible_middle_in_range_absent" => {
                        ExtensibleValidationRule::OptionalPartMissing {
                            recipe: 0,
                            part: ExtensiblePart::Middle,
                            character: 1,
                        }
                    }
                    "invalid_extensible_bottom" => ExtensibleValidationRule::OptionalPartMissing {
                        recipe: 0,
                        part: ExtensiblePart::Bottom,
                        character: 255,
                    },
                    "extensible_bottom_in_range_absent" => {
                        ExtensibleValidationRule::OptionalPartMissing {
                            recipe: 0,
                            part: ExtensiblePart::Bottom,
                            character: 1,
                        }
                    }
                    "invalid_extensible" => ExtensibleValidationRule::RepeatMissing {
                        recipe: 0,
                        character: 255,
                    },
                    "extensible_repeat_in_range_absent" => {
                        ExtensibleValidationRule::RepeatMissing {
                            recipe: 0,
                            character: 1,
                        }
                    }
                    _ => panic!("missing exact extensible rule for {case_id}"),
                };
                assert_eq!(result.err(), Some(expected), "{case_id} {blob_sha256}");
            } else {
                parameter_cases += 1;
                assert!(result.is_ok(), "{case_id} {blob_sha256}");
            }
        }
        assert_eq!(extensible_cases, 8);
        assert!(parameter_cases > 0);
    }

    #[test]
    fn empty_parameter_table_zero_fills_the_standard_typed_state() {
        let state = check_parameter_frame(kern_frame(&[], &[], &[]), 1).unwrap();

        assert_eq!(state.slant, SignedSlant(0));
        assert_eq!(state.dimensions.as_ref(), [ScaledSp(0); 6]);
        assert!(state.predecessor.extensibles.is_empty());
        assert_eq!(
            state
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .raw_counts
                .np,
            0
        );
    }

    #[test]
    fn parameter_shape_matrix_names_every_declared_and_filled_slot() {
        for np in 0..=8u16 {
            let parameters = (0..np)
                .map(|zero_based_index| {
                    if zero_based_index == 0 {
                        [0x80 | u8::try_from(np).unwrap(), 0x35, 0xa6, 0xf7]
                    } else {
                        let index = u8::try_from(zero_based_index).unwrap();
                        [0, index, index.wrapping_mul(17), 255 - index]
                    }
                })
                .collect::<Vec<_>>();
            let state = check_parameter_frame(kern_frame(&[], &[], &parameters), 65_537).unwrap();

            let expected_slant = parameters
                .first()
                .copied()
                .map_or(SignedSlant(0), literal_signed_slant);
            assert_eq!(state.slant, expected_slant, "np={np}");
            assert_eq!(
                state.dimensions.len(),
                usize::from(np.max(7) - 1),
                "np={np}"
            );
            for (ordinary_index, word) in parameters.iter().copied().skip(1).enumerate() {
                assert_eq!(
                    state.dimensions[ordinary_index],
                    literal_scaled_parameter(word, 65_537).unwrap(),
                    "np={np} fontdimen={}",
                    ordinary_index + 2
                );
            }
            for (ordinary_index, value) in state
                .dimensions
                .iter()
                .enumerate()
                .skip(parameters.len().saturating_sub(1))
            {
                assert_eq!(
                    *value,
                    ScaledSp(0),
                    "np={np} zero-filled fontdimen={}",
                    ordinary_index + 2
                );
            }
        }
    }

    #[test]
    fn slant_is_a_signed_pure_number_independent_of_effective_size() {
        for (word, expected) in [
            ([1, 35, 69, 111], 1_193_046),
            ([254, 220, 186, 159], -1_193_047),
            ([255, 255, 255, 255], -1),
            ([128, 0, 0, 0], -134_217_728),
            ([127, 255, 255, 255], 134_217_727),
        ] {
            for effective_size_sp in [1, 655_360, MAX_TEX_FONT_SIZE_SP - 1] {
                let state = check_parameter_frame(kern_frame(&[], &[], &[word]), effective_size_sp)
                    .unwrap();
                assert_eq!(state.slant, SignedSlant(expected), "{word:?}");
                assert_eq!(state.dimensions.as_ref(), [ScaledSp(0); 6]);
            }
        }
    }

    #[test]
    fn slant_low_nibbles_match_literal_signed_byte_decomposition() {
        for high_byte in 0..=u8::MAX {
            for second_byte in [0, u8::MAX] {
                for third_byte in [0x55, 0xaa] {
                    for fourth_high_nibble in [0, 0xf0] {
                        let mut baseline = None;
                        for low_nibble in 0..16 {
                            let word = [
                                high_byte,
                                second_byte,
                                third_byte,
                                fourth_high_nibble | low_nibble,
                            ];
                            let state = check_parameter_frame(
                                kern_frame(&[], &[], &[word]),
                                if high_byte & 1 == 0 {
                                    1
                                } else {
                                    MAX_TEX_FONT_SIZE_SP - 1
                                },
                            )
                            .unwrap();
                            let expected = literal_signed_slant(word);
                            assert_eq!(state.slant, expected, "word={word:02x?}");
                            assert_eq!(
                                *baseline.get_or_insert(state.slant),
                                state.slant,
                                "discarded low nibble changed slant for word={word:02x?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn non_slant_parameters_use_exact_scaling_then_zero_fill() {
        let state = check_parameter_frame(
            kern_frame(
                &[],
                &[],
                &[
                    [0, 0, 0, 0],
                    [0, 0x10, 0, 0],
                    [255, 0xf0, 0, 0],
                    [0, 0, 1, 0],
                    [0, 0, 0, 1],
                ],
            ),
            655_360,
        )
        .unwrap();

        assert_eq!(state.slant, SignedSlant(0));
        assert_eq!(
            state.dimensions.as_ref(),
            [
                ScaledSp(655_360),
                ScaledSp(-655_360),
                ScaledSp(160),
                ScaledSp(0),
                ScaledSp(0),
                ScaledSp(0),
            ]
        );
    }

    #[test]
    fn frozen_native_box_matrix_matches_non_slant_parameter_values() {
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/tfm-box-scaling-oracle-v1.json");
        let fixture_bytes = std::fs::read(fixture_path).unwrap();
        assert_eq!(
            Sha256::digest(&fixture_bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "287f3c33038b05279239f0836af5e03a306f4589d41127eb3aec2af88f051eb4"
        );
        let fixture: serde_json::Value = serde_json::from_slice(&fixture_bytes).unwrap();

        for (size_id, size) in fixture["case_sizes_sp"].as_object().unwrap() {
            let size = i32::try_from(size.as_i64().unwrap()).unwrap();
            let observations = fixture["case_results"][size_id]["observations"]
                .as_object()
                .unwrap();
            for (word_id, raw_word) in fixture["fix_word_cases"].as_object().unwrap() {
                let raw_word = raw_word.as_str().unwrap();
                let raw_word: [u8; 4] = std::array::from_fn(|index| {
                    u8::from_str_radix(&raw_word[index * 2..index * 2 + 2], 16).unwrap()
                });
                let state =
                    check_parameter_frame(kern_frame(&[], &[], &[[0, 0, 0, 0], raw_word]), size)
                        .unwrap();
                let expected =
                    i32::try_from(observations[&format!("{word_id}_width")].as_i64().unwrap())
                        .unwrap();
                assert_eq!(
                    state.dimensions[0],
                    ScaledSp(expected),
                    "{size_id} {word_id}"
                );
            }
        }
    }

    #[test]
    fn each_forbidden_non_slant_sign_has_exact_parameter_identity() {
        for sign in 1..=254u8 {
            let result =
                check_parameter_frame(kern_frame(&[], &[], &[[sign, 2, 3, 4], [sign, 0, 0, 0]]), 1);
            assert_eq!(
                result.err(),
                Some(ParameterValidationRule::InvalidFixWordSign { parameter: 2, sign })
            );
        }
    }

    #[test]
    fn parameter_failures_follow_whole_declared_source_order() {
        let result = check_parameter_frame(
            kern_frame(
                &[],
                &[],
                &[
                    [0, 0, 0, 0],
                    [3, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [4, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [5, 0, 0, 0],
                ],
            ),
            1,
        );
        assert_eq!(
            result.err(),
            Some(ParameterValidationRule::InvalidFixWordSign {
                parameter: 2,
                sign: 3,
            })
        );

        let eighth = check_parameter_frame(
            kern_frame(
                &[],
                &[],
                &[
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [5, 0, 0, 0],
                ],
            ),
            1,
        );
        assert_eq!(
            eighth.err(),
            Some(ParameterValidationRule::InvalidFixWordSign {
                parameter: 8,
                sign: 5,
            })
        );
    }

    #[test]
    fn parameters_above_seven_are_scaled_and_retained() {
        let state = check_parameter_frame(
            kern_frame(
                &[],
                &[],
                &[
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0, 0, 0],
                    [0, 0x10, 0, 0],
                ],
            ),
            65_536,
        )
        .unwrap();

        assert_eq!(state.dimensions.len(), 7);
        assert_eq!(state.dimensions[6], ScaledSp(65_536));
    }

    #[test]
    fn absolute_maximum_parameter_table_is_checked_and_retained() {
        let parameters = vec![[0, 0, 0, 0]; 32_755];
        let state = check_parameter_frame(kern_frame(&[], &[], &parameters), 1).unwrap();

        assert_eq!(state.slant, SignedSlant(0));
        assert_eq!(state.dimensions.len(), 32_754);
        assert!(state.dimensions.iter().all(|value| *value == ScaledSp(0)));
    }

    #[test]
    fn final_parameter_at_absolute_maximum_is_still_validated() {
        let mut parameters = vec![[0, 0, 0, 0]; 32_755];
        parameters[32_754] = [254, 0, 0, 0];
        let result = check_parameter_frame(kern_frame(&[], &[], &parameters), 1);

        assert_eq!(
            result.err(),
            Some(ParameterValidationRule::InvalidFixWordSign {
                parameter: 32_755,
                sign: 254,
            })
        );
    }

    #[test]
    fn generated_parameters_match_independent_reference_and_never_panic() {
        let sizes = [
            1,
            15,
            16,
            17,
            65_535,
            65_536,
            65_537,
            (1 << 23) - 1,
            1 << 23,
            (1 << 23) + 1,
            (1 << 24) - 1,
            1 << 24,
            (1 << 24) + 1,
            (1 << 25) - 1,
            1 << 25,
            (1 << 25) + 1,
            (1 << 26) - 1,
            1 << 26,
            (1 << 26) + 1,
            MAX_TEX_FONT_SIZE_SP - 1,
        ];
        let mut generator = 0x6a93_e9d1_5100_83eeu64;

        for case_index in 0..512usize {
            let np = case_index % 33;
            let mut parameters = Vec::with_capacity(np);
            for parameter_index in 0..np {
                generator = generator
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let mut word = (generator as u32).to_be_bytes();
                if parameter_index > 0 {
                    word[0] = if generator & (1 << 63) == 0 { 0 } else { 255 };
                }
                parameters.push(word);
            }
            let size = sizes[case_index % sizes.len()];
            let expected_slant = parameters
                .first()
                .copied()
                .map_or(SignedSlant(0), literal_signed_slant);
            let mut expected_dimensions = parameters
                .iter()
                .copied()
                .skip(1)
                .map(|word| literal_scaled_parameter(word, size).unwrap())
                .collect::<Vec<_>>();
            if expected_dimensions.len() < 6 {
                expected_dimensions.resize(6, ScaledSp(0));
            }

            let outcome = std::panic::catch_unwind(|| {
                check_parameter_frame(kern_frame(&[], &[], &parameters), size)
            });
            assert!(outcome.is_ok(), "case {case_index} panicked");
            let state = outcome.unwrap().unwrap();
            assert_eq!(state.slant, expected_slant, "case {case_index}");
            assert_eq!(
                state.dimensions.as_ref(),
                expected_dimensions,
                "case {case_index}"
            );
        }
    }

    #[test]
    fn generated_invalid_parameters_return_first_sign_without_panicking() {
        let mut generator = 0x0dcb_7124_f1b6_5764u64;

        for case_index in 0..256usize {
            let np = 2 + case_index % 63;
            let mut parameters = vec![[0, 0, 0, 0]; np];
            for (parameter_index, word) in parameters.iter_mut().enumerate() {
                generator = generator
                    .wrapping_mul(2_862_933_555_777_941_757)
                    .wrapping_add(3_037_000_493);
                *word = (generator as u32).to_be_bytes();
                if parameter_index > 0 {
                    word[0] = if generator & 1 == 0 { 0 } else { 255 };
                }
            }
            let first_invalid_index = 1 + usize::try_from(generator % (np - 1) as u64).unwrap();
            let first_invalid_sign = 1 + u8::try_from(generator % 254).unwrap();
            parameters[first_invalid_index][0] = first_invalid_sign;
            if first_invalid_index + 1 < np {
                parameters[np - 1][0] = if first_invalid_sign == 254 {
                    1
                } else {
                    first_invalid_sign + 1
                };
            }
            let size = 1 + i32::try_from(generator % ((1u64 << 27) - 1)).unwrap();

            let outcome = std::panic::catch_unwind(|| {
                check_parameter_frame(kern_frame(&[], &[], &parameters), size)
            });
            assert!(outcome.is_ok(), "case {case_index} panicked");
            assert_eq!(
                outcome.unwrap().err(),
                Some(ParameterValidationRule::InvalidFixWordSign {
                    parameter: u16::try_from(first_invalid_index + 1).unwrap(),
                    sign: first_invalid_sign,
                }),
                "case {case_index}"
            );
        }
    }

    #[test]
    fn suffix_does_not_change_parameter_semantics() {
        let bytes = kern_frame(&[], &[], &[[255, 255, 255, 255], [0, 0x10, 0, 0]]);
        let control = check_parameter_frame(bytes.clone(), 65_536).unwrap();
        let mut suffixed = bytes;
        suffixed.extend((0..8193).map(|index| (index as u8).wrapping_mul(37)));
        let state = check_parameter_frame(suffixed, 65_536).unwrap();

        assert_eq!(state.slant, control.slant);
        assert_eq!(state.dimensions, control.dimensions);
        let state_header = &state
            .predecessor
            .predecessor
            .predecessor
            .predecessor
            .predecessor
            .predecessor;
        let control_header = &control
            .predecessor
            .predecessor
            .predecessor
            .predecessor
            .predecessor
            .predecessor;
        assert_eq!(state_header.frame_digest, control_header.frame_digest);
        assert_ne!(state_header.raw_digest, control_header.raw_digest);
    }

    #[test]
    fn parameter_success_retains_the_same_raw_allocation_and_extensible_state() {
        let bytes = extensible_frame_at(
            7,
            &[[1, 0, 0, 0]],
            &[[0, 0, 0, 7]],
            &[[0, 0, 0, 0], [0, 0x10, 0, 0]],
        );
        let raw: Arc<[u8]> = Arc::from(bytes);
        let retained = Arc::clone(&raw);
        let state = check_parameters(
            check_extensibles(
                check_kerns(
                    check_lig_kern(
                        check_boxes(
                            check_characters(check_preamble_header(raw, 65_536).unwrap()).unwrap(),
                        )
                        .unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(state.slant, SignedSlant(0));
        assert_eq!(state.dimensions[0], ScaledSp(65_536));
        assert_eq!(state.predecessor.extensibles[0].repeat, 7);
        assert!(Arc::ptr_eq(
            &retained,
            &state
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .predecessor
                .raw
        ));
    }

    #[test]
    fn persisted_parameter_witnesses_have_exact_private_results() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let corpus_root = fixture_root.join("tfm-validity-oracle-v2");
        let manifest = reviewed_corpus_manifest(&fixture_root);
        let parameter_witnesses = [
            "signed_slant_parameter",
            "invalid_fontdimen2",
            "invalid_fontdimen5",
            "short_np0",
            "short_np4",
            "short_np5",
            "parameter_count_8_valid",
            "parameter_8_invalid_fix_word",
        ];
        let mut accepted = 0;
        let mut rejected = 0;

        for case in manifest["cases"].as_array().unwrap() {
            let case_id = case["id"].as_str().unwrap();
            if !parameter_witnesses.contains(&case_id) {
                continue;
            }
            let blob_sha256 = case["blob_sha256"].as_str().unwrap();
            let raw = std::fs::read(corpus_root.join("blobs").join(format!("{blob_sha256}.tfm")))
                .unwrap();
            let input_size =
                i32::try_from(case["validator_input_size_sp"].as_i64().unwrap()).unwrap();
            let extensible = check_extensibles(
                check_kerns(
                    check_lig_kern(
                        check_boxes(
                            check_characters(
                                check_preamble_header(Arc::from(raw), input_size).unwrap(),
                            )
                            .unwrap(),
                        )
                        .unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
            .unwrap_or_else(|failure| {
                panic!("{case_id} {blob_sha256} failed before parameter phase: {failure:?}")
            });
            let result = check_parameters(extensible);
            let expected = match case_id {
                "invalid_fontdimen2" => Some(ParameterValidationRule::InvalidFixWordSign {
                    parameter: 2,
                    sign: 1,
                }),
                "invalid_fontdimen5" => Some(ParameterValidationRule::InvalidFixWordSign {
                    parameter: 5,
                    sign: 1,
                }),
                "parameter_8_invalid_fix_word" => {
                    Some(ParameterValidationRule::InvalidFixWordSign {
                        parameter: 8,
                        sign: 1,
                    })
                }
                _ => None,
            };
            if let Some(expected) = expected {
                rejected += 1;
                assert_eq!(result.err(), Some(expected), "{case_id} {blob_sha256}");
            } else {
                accepted += 1;
                assert!(result.is_ok(), "{case_id} {blob_sha256}");
            }
        }
        assert_eq!(accepted, 5);
        assert_eq!(rejected, 3);
    }

    #[test]
    fn box_tables_scale_exact_positive_negative_fractional_and_carry_words() {
        let bytes = box_frame_with_words(
            [5, 3, 3, 3],
            &[
                (BoxMetric::Width, 1, [0, 0x10, 0, 0]),
                (BoxMetric::Width, 2, [255, 0xf0, 0, 0]),
                (BoxMetric::Width, 3, [0, 0, 1, 0]),
                (BoxMetric::Width, 4, [0, 0, 0, 1]),
                (BoxMetric::Height, 1, [0, 1, 0, 0]),
                (BoxMetric::Height, 2, [255, 255, 255, 255]),
                (BoxMetric::Depth, 1, [0, 0, 255, 255]),
                (BoxMetric::Depth, 2, [255, 255, 0, 0]),
                (BoxMetric::Italic, 1, [0, 15, 255, 255]),
                (BoxMetric::Italic, 2, [0, 0, 0, 1]),
            ],
        );
        let state = check_boxes(
            check_characters(check_preamble_header(Arc::from(bytes), 655_360).unwrap()).unwrap(),
        )
        .unwrap();

        assert_eq!(
            state.widths.as_ref(),
            [
                ScaledSp(0),
                ScaledSp(655_360),
                ScaledSp(-655_360),
                ScaledSp(160),
                ScaledSp(0),
            ]
        );
        assert_eq!(
            state.heights.as_ref(),
            [ScaledSp(0), ScaledSp(40_960), ScaledSp(-1)]
        );
        assert_eq!(
            state.depths.as_ref(),
            [ScaledSp(0), ScaledSp(40_959), ScaledSp(-40_960)]
        );
        assert_eq!(
            state.italics.as_ref(),
            [ScaledSp(0), ScaledSp(655_359), ScaledSp(0)]
        );
    }

    #[test]
    fn each_box_table_rejects_a_forbidden_sign_with_exact_identity() {
        for table in [
            BoxMetric::Width,
            BoxMetric::Height,
            BoxMetric::Depth,
            BoxMetric::Italic,
        ] {
            let bytes = box_frame_with_words([2, 2, 2, 2], &[(table, 1, [1, 2, 3, 4])]);
            let result = check_boxes(
                check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap(),
            );
            assert_eq!(
                result.err(),
                Some(BoxValidationRule::InvalidFixWordSign {
                    table,
                    index: 1,
                    sign: 1,
                })
            );
        }
    }

    #[test]
    fn every_forbidden_sign_byte_rejects_in_every_box_table() {
        for table in [
            BoxMetric::Width,
            BoxMetric::Height,
            BoxMetric::Depth,
            BoxMetric::Italic,
        ] {
            for sign in 1..=254u8 {
                let bytes = box_frame_with_words([2, 2, 2, 2], &[(table, 1, [sign, 0, 0, 0])]);
                let result = check_boxes(
                    check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap(),
                );
                assert_eq!(
                    result.err(),
                    Some(BoxValidationRule::InvalidFixWordSign {
                        table,
                        index: 1,
                        sign,
                    })
                );
            }
        }
    }

    #[test]
    fn box_sign_failures_follow_table_and_entry_source_order() {
        for (words, expected) in [
            (
                vec![
                    (BoxMetric::Width, 2, [1, 0, 0, 0]),
                    (BoxMetric::Height, 0, [2, 0, 0, 0]),
                ],
                BoxValidationRule::InvalidFixWordSign {
                    table: BoxMetric::Width,
                    index: 2,
                    sign: 1,
                },
            ),
            (
                vec![
                    (BoxMetric::Depth, 2, [4, 0, 0, 0]),
                    (BoxMetric::Depth, 1, [3, 0, 0, 0]),
                ],
                BoxValidationRule::InvalidFixWordSign {
                    table: BoxMetric::Depth,
                    index: 1,
                    sign: 3,
                },
            ),
        ] {
            let bytes = box_frame_with_words([3, 3, 3, 3], &words);
            let result = check_boxes(
                check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap(),
            );
            assert_eq!(result.err(), Some(expected));
        }
    }

    #[test]
    fn box_scaling_matches_literal_effective_size_normalization_boundaries() {
        let bytes = box_frame_with_words(
            [6, 1, 1, 1],
            &[
                (BoxMetric::Width, 1, [0, 16, 0, 0]),
                (BoxMetric::Width, 2, [255, 240, 0, 0]),
                (BoxMetric::Width, 3, [0, 0, 0, 1]),
                (BoxMetric::Width, 4, [255, 255, 255, 255]),
                (BoxMetric::Width, 5, [0, 255, 255, 255]),
            ],
        );
        for (size, expected) in [
            (1, [1, -1, 0, -1, 15]),
            (2, [2, -2, 0, -1, 31]),
            (15, [15, -15, 0, -1, 239]),
            (16, [16, -16, 0, -1, 255]),
            (17, [17, -17, 0, -1, 271]),
            ((1 << 16) - 1, [65_535, -65_535, 0, -1, 1_048_559]),
            (1 << 16, [65_536, -65_536, 0, -1, 1_048_575]),
            ((1 << 16) + 1, [65_537, -65_537, 0, -1, 1_048_591]),
            ((1 << 23) - 1, [8_388_607, -8_388_607, 7, -8, 134_217_704]),
            (1 << 23, [8_388_608, -8_388_608, 8, -8, 134_217_720]),
            ((1 << 23) + 1, [8_388_608, -8_388_608, 8, -8, 134_217_720]),
            (
                (1 << 24) - 1,
                [16_777_214, -16_777_214, 15, -16, 268_435_408],
            ),
            (1 << 24, [16_777_216, -16_777_216, 16, -16, 268_435_440]),
            (
                (1 << 24) + 1,
                [16_777_216, -16_777_216, 16, -16, 268_435_440],
            ),
            (
                (1 << 25) - 1,
                [33_554_428, -33_554_428, 31, -32, 536_870_816],
            ),
            (1 << 25, [33_554_432, -33_554_432, 32, -32, 536_870_880]),
            (
                (1 << 25) + 1,
                [33_554_432, -33_554_432, 32, -32, 536_870_880],
            ),
            (
                (1 << 26) - 1,
                [67_108_856, -67_108_856, 63, -64, 1_073_741_632],
            ),
            (1 << 26, [67_108_864, -67_108_864, 64, -64, 1_073_741_760]),
            (
                (1 << 26) + 1,
                [67_108_864, -67_108_864, 64, -64, 1_073_741_760],
            ),
            (
                (1 << 27) - 1,
                [134_217_712, -134_217_712, 127, -128, 2_147_483_264],
            ),
        ] {
            let state = check_boxes(
                check_characters(check_preamble_header(Arc::from(bytes.clone()), size).unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                state.widths[1..],
                expected.map(ScaledSp),
                "effective size {size}"
            );
        }
    }

    #[test]
    fn frozen_native_box_matrix_matches_private_scaled_values() {
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/tfm-box-scaling-oracle-v1.json");
        let fixture_bytes = std::fs::read(fixture_path).unwrap();
        assert_eq!(
            Sha256::digest(&fixture_bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "287f3c33038b05279239f0836af5e03a306f4589d41127eb3aec2af88f051eb4"
        );
        let fixture: serde_json::Value = serde_json::from_slice(&fixture_bytes).unwrap();
        assert_eq!(
            fixture["native_observation_projection"],
            serde_json::json!({
                "width": "exact_scaled_sp",
                "height": "max_zero_exact_scaled_sp",
                "depth": "max_zero_exact_scaled_sp",
                "italic": "exact_scaled_sp",
            })
        );

        for (size_id, size) in fixture["case_sizes_sp"].as_object().unwrap() {
            let size = i32::try_from(size.as_i64().unwrap()).unwrap();
            let observations = fixture["case_results"][size_id]["observations"]
                .as_object()
                .unwrap();
            for (word_id, raw_word) in fixture["fix_word_cases"].as_object().unwrap() {
                let raw_word = raw_word.as_str().unwrap();
                let raw_word: [u8; 4] = std::array::from_fn(|index| {
                    u8::from_str_radix(&raw_word[index * 2..index * 2 + 2], 16).unwrap()
                });
                let bytes = box_frame_with_words(
                    [2, 2, 2, 2],
                    &[
                        (BoxMetric::Width, 1, raw_word),
                        (BoxMetric::Height, 1, raw_word),
                        (BoxMetric::Depth, 1, raw_word),
                        (BoxMetric::Italic, 1, raw_word),
                    ],
                );
                let state = check_boxes(
                    check_characters(check_preamble_header(Arc::from(bytes), size).unwrap())
                        .unwrap(),
                )
                .unwrap();
                let ScaledSp(scaled_sp) = state.widths[1];
                assert_eq!(state.heights[1], ScaledSp(scaled_sp));
                assert_eq!(state.depths[1], ScaledSp(scaled_sp));
                assert_eq!(state.italics[1], ScaledSp(scaled_sp));
                for (metric, expected) in [
                    ("width", scaled_sp),
                    ("height", scaled_sp.max(0)),
                    ("depth", scaled_sp.max(0)),
                    ("italic", scaled_sp),
                ] {
                    assert_eq!(
                        observations[&format!("{word_id}_{metric}")],
                        expected,
                        "{size_id} {word_id} {metric}"
                    );
                }
            }
        }
    }

    #[test]
    fn scaled_entry_zero_checks_use_the_bound_size_for_each_box_table() {
        for table in [
            BoxMetric::Width,
            BoxMetric::Height,
            BoxMetric::Depth,
            BoxMetric::Italic,
        ] {
            let bytes = box_frame_with_words([1, 1, 1, 1], &[(table, 0, [0, 1, 0, 0])]);
            assert!(
                check_boxes(
                    check_characters(check_preamble_header(Arc::from(bytes.clone()), 1).unwrap())
                        .unwrap()
                )
                .is_ok(),
                "{table:?} raw entry zero should round to zero at 1sp"
            );
            let result = check_boxes(
                check_characters(check_preamble_header(Arc::from(bytes), 16).unwrap()).unwrap(),
            );
            assert_eq!(
                result.err(),
                Some(BoxValidationRule::NonzeroScaledEntryZero {
                    table,
                    scaled_sp: 1,
                })
            );
        }
    }

    #[test]
    fn scaled_entry_zero_failures_follow_box_table_source_order() {
        let tables = [
            BoxMetric::Width,
            BoxMetric::Height,
            BoxMetric::Depth,
            BoxMetric::Italic,
        ];
        for first_failure in 0..tables.len() {
            let words = tables[first_failure..]
                .iter()
                .map(|&table| (table, 0, [0, 1, 0, 0]))
                .collect::<Vec<_>>();
            let bytes = box_frame_with_words([1, 1, 1, 1], &words);
            let result = check_boxes(
                check_characters(check_preamble_header(Arc::from(bytes), 16).unwrap()).unwrap(),
            );
            assert_eq!(
                result.err(),
                Some(BoxValidationRule::NonzeroScaledEntryZero {
                    table: tables[first_failure],
                    scaled_sp: 1,
                })
            );
        }
    }

    #[test]
    fn every_box_word_is_scaled_before_entry_zero_checks() {
        let bytes = box_frame_with_words(
            [1, 1, 1, 2],
            &[
                (BoxMetric::Width, 0, [0, 1, 0, 0]),
                (BoxMetric::Italic, 1, [7, 0, 0, 0]),
            ],
        );
        let result = check_boxes(
            check_characters(check_preamble_header(Arc::from(bytes), 16).unwrap()).unwrap(),
        );
        assert_eq!(
            result.err(),
            Some(BoxValidationRule::InvalidFixWordSign {
                table: BoxMetric::Italic,
                index: 1,
                sign: 7,
            })
        );
    }

    #[test]
    fn box_success_retains_the_exact_character_predecessor_and_table_lengths() {
        let bytes = box_frame_with_words(
            [2, 3, 4, 5],
            &[
                (BoxMetric::Width, 1, [0, 0, 1, 0]),
                (BoxMetric::Height, 2, [0, 1, 0, 0]),
                (BoxMetric::Depth, 3, [255, 255, 255, 255]),
                (BoxMetric::Italic, 4, [0, 0, 0, 1]),
            ],
        );
        let raw: Arc<[u8]> = Arc::from(bytes);
        let retained = Arc::clone(&raw);
        let character = check_characters(check_preamble_header(raw, 12_345).unwrap()).unwrap();
        let expected_records = character.records.to_vec();
        let expected_existing = character.existing_characters;
        let expected_counts = character.predecessor.raw_counts;
        let expected_domain = character.predecessor.character_domain;
        let expected_layout = character.predecessor.layout.clone();
        let expected_endpoint = character.predecessor.declared_frame_end;
        let expected_raw_digest = character.predecessor.raw_digest;
        let expected_frame_digest = character.predecessor.frame_digest;
        let expected_design_fix = character.predecessor.design_size_fix_word;
        let expected_design_sp = character.predecessor.design_size_sp;

        let state = check_boxes(character).unwrap();

        assert!(Arc::ptr_eq(&retained, &state.predecessor.predecessor.raw));
        assert_eq!(
            state.predecessor.predecessor.effective_size,
            EffectiveSizeSp(12_345)
        );
        assert_eq!(state.predecessor.predecessor.raw_counts, expected_counts);
        assert_eq!(
            state.predecessor.predecessor.character_domain,
            expected_domain
        );
        assert_eq!(state.predecessor.predecessor.layout, expected_layout);
        assert_eq!(
            state.predecessor.predecessor.declared_frame_end,
            expected_endpoint
        );
        assert_eq!(
            state.predecessor.predecessor.raw_digest,
            expected_raw_digest
        );
        assert_eq!(
            state.predecessor.predecessor.frame_digest,
            expected_frame_digest
        );
        assert_eq!(
            state.predecessor.predecessor.design_size_fix_word,
            expected_design_fix
        );
        assert_eq!(
            state.predecessor.predecessor.design_size_sp,
            expected_design_sp
        );
        assert_eq!(state.predecessor.records.as_ref(), expected_records);
        assert_eq!(state.predecessor.existing_characters, expected_existing);
        assert_eq!(state.widths.len(), 2);
        assert_eq!(state.heights.len(), 3);
        assert_eq!(state.depths.len(), 4);
        assert_eq!(state.italics.len(), 5);
    }

    #[test]
    fn suffixes_and_post_box_tables_do_not_change_scaled_box_semantics() {
        let mut base = box_frame_with_words(
            [2, 2, 2, 2],
            &[
                (BoxMetric::Width, 1, [0, 0, 1, 0]),
                (BoxMetric::Height, 1, [0, 0, 2, 0]),
                (BoxMetric::Depth, 1, [0, 0, 3, 0]),
                (BoxMetric::Italic, 1, [0, 0, 4, 0]),
            ],
        );
        let original_lf = u16::from_be_bytes([base[0], base[1]]);
        put_count(&mut base, 0, original_lf + 4);
        for count_index in 8..=11 {
            put_count(&mut base, count_index, 1);
        }
        base.extend_from_slice(&[0; 16]);
        let control = check_boxes(
            check_characters(check_preamble_header(Arc::from(base.clone()), 65_536).unwrap())
                .unwrap(),
        )
        .unwrap();
        let layout = control.predecessor.predecessor.layout.clone();

        for range in [
            layout.lig_kern,
            layout.kerns,
            layout.extensibles,
            layout.parameters,
        ] {
            let mut bytes = base.clone();
            bytes[range.clone()].fill(0xff);
            let state = check_boxes(
                check_characters(check_preamble_header(Arc::from(bytes), 65_536).unwrap()).unwrap(),
            )
            .unwrap();
            assert_eq!(state.widths, control.widths, "later range {range:?}");
            assert_eq!(state.heights, control.heights, "later range {range:?}");
            assert_eq!(state.depths, control.depths, "later range {range:?}");
            assert_eq!(state.italics, control.italics, "later range {range:?}");
        }

        for suffix_length in [1, 2, 3, 4, 65, 8193] {
            let mut bytes = base.clone();
            bytes.extend((0..suffix_length).map(|index| (index as u8).wrapping_mul(41)));
            let state = check_boxes(
                check_characters(check_preamble_header(Arc::from(bytes), 65_536).unwrap()).unwrap(),
            )
            .unwrap();
            assert_eq!(state.widths, control.widths);
            assert_eq!(state.heights, control.heights);
            assert_eq!(state.depths, control.depths);
            assert_eq!(state.italics, control.italics);
            assert_eq!(
                state.predecessor.predecessor.frame_digest,
                control.predecessor.predecessor.frame_digest
            );
            assert_ne!(
                state.predecessor.predecessor.raw_digest,
                control.predecessor.predecessor.raw_digest
            );
        }
    }

    #[test]
    fn maximum_box_geometry_scales_without_overflow() {
        let count = 8189;
        let bytes = box_frame_with_words([count, count, count, count], &[]);
        let state = check_boxes(
            check_characters(
                check_preamble_header(Arc::from(bytes), MAX_TEX_FONT_SIZE_SP - 1).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(state.widths.len(), usize::from(count));
        assert_eq!(state.heights.len(), usize::from(count));
        assert_eq!(state.depths.len(), usize::from(count));
        assert_eq!(state.italics.len(), usize::from(count));
    }

    #[test]
    fn bounded_generated_sign_valid_box_words_never_panic_or_fail() {
        let mut generator = 0x6a93_9670_0fc8_83e8u64;
        for case_index in 0..256 {
            generator = generator
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let count = 2 + u16::try_from(generator % 63).unwrap();
            let mut words = Vec::new();
            for table in [
                BoxMetric::Width,
                BoxMetric::Height,
                BoxMetric::Depth,
                BoxMetric::Italic,
            ] {
                for index in 1..count {
                    let mut word = [0; 4];
                    for byte in &mut word {
                        generator = generator
                            .wrapping_mul(6_364_136_223_846_793_005)
                            .wrapping_add(1);
                        *byte = (generator >> 32) as u8;
                    }
                    word[0] = if word[0] & 1 == 0 { 0 } else { 255 };
                    words.push((table, index, word));
                }
            }
            generator = generator
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let size = 1 + i32::try_from(generator % u64::from((1u32 << 27) - 1)).unwrap();
            let bytes = box_frame_with_words([count, count, count, count], &words);
            let header = check_preamble_header(Arc::from(bytes), size).unwrap();
            let character = check_characters(header).unwrap();
            let result = std::panic::catch_unwind(|| check_boxes(character));
            assert!(result.is_ok(), "case {case_index} panicked");
            assert!(result.unwrap().is_ok(), "case {case_index} failed");
        }
    }

    #[test]
    fn empty_character_domain_preserves_the_predecessor_without_records() {
        let raw: Arc<[u8]> = Arc::from(seed_frame());
        let retained = Arc::clone(&raw);
        let header = check_preamble_header(raw, 12_345).unwrap();
        let expected_counts = header.raw_counts;
        let expected_domain = header.character_domain;
        let expected_layout = header.layout.clone();
        let expected_endpoint = header.declared_frame_end;
        let expected_raw_digest = header.raw_digest;
        let expected_frame_digest = header.frame_digest;
        let expected_design_fix = header.design_size_fix_word;
        let expected_design_sp = header.design_size_sp;

        let state = check_characters(header).unwrap();

        assert!(Arc::ptr_eq(&retained, &state.predecessor.raw));
        assert_eq!(state.predecessor.effective_size, EffectiveSizeSp(12_345));
        assert_eq!(state.predecessor.raw_counts, expected_counts);
        assert_eq!(state.predecessor.character_domain, expected_domain);
        assert_eq!(state.predecessor.layout, expected_layout);
        assert_eq!(state.predecessor.declared_frame_end, expected_endpoint);
        assert_eq!(state.predecessor.raw_digest, expected_raw_digest);
        assert_eq!(state.predecessor.frame_digest, expected_frame_digest);
        assert_eq!(state.predecessor.design_size_fix_word, expected_design_fix);
        assert_eq!(state.predecessor.design_size_sp, expected_design_sp);
        assert!(state.records.is_empty());
        assert_eq!(state.existing_characters.0, [0; 4]);
    }

    #[test]
    fn packed_character_indices_decode_at_their_exact_valid_maxima() {
        let bytes = character_frame(&[[1, 0xab, 0x95, 0x5a]], [2, 12, 12, 38, 91, 91]);
        let raw: Arc<[u8]> = Arc::from(bytes);
        let retained = Arc::clone(&raw);
        let state = check_characters(check_preamble_header(raw, 1).unwrap()).unwrap();

        assert!(Arc::ptr_eq(&retained, &state.predecessor.raw));
        assert_eq!(state.records.len(), 1);
        assert_eq!(state.records[0].character, 7);
        assert_eq!(state.records[0].width_index, 1);
        assert_eq!(state.records[0].height_index, 10);
        assert_eq!(state.records[0].depth_index, 11);
        assert_eq!(state.records[0].italic_index, 37);
        assert_eq!(state.records[0].tag, CharacterTag::Ligature { start: 0x5a });
        assert_eq!(state.existing_characters.0, [1 << 7, 0, 0, 0]);
    }

    #[test]
    fn metric_indices_accept_count_minus_one_and_reject_count() {
        let accepted = character_frame(&[[1, 0x11, 0x04, 0]], [2, 2, 2, 2, 0, 0]);
        assert!(check_characters(check_preamble_header(Arc::from(accepted), 1).unwrap()).is_ok());

        for (record, metric) in [
            ([2, 0x00, 0x00, 0], CharacterMetric::Width),
            ([0, 0x20, 0x00, 0], CharacterMetric::Height),
            ([0, 0x02, 0x00, 0], CharacterMetric::Depth),
            ([0, 0x00, 0x08, 0], CharacterMetric::Italic),
        ] {
            assert_character_rule(
                character_frame(&[record], [2, 2, 2, 2, 0, 0]),
                CharacterValidationRule::MetricIndexOutOfRange {
                    character: 7,
                    metric,
                    index: 2,
                    count: 2,
                },
            );
        }
    }

    #[test]
    fn width_zero_does_not_skip_other_metric_checks() {
        for (record, metric) in [
            ([0, 0x10, 0x00, 0], CharacterMetric::Height),
            ([0, 0x01, 0x00, 0], CharacterMetric::Depth),
            ([0, 0x00, 0x04, 0], CharacterMetric::Italic),
        ] {
            assert_character_rule(
                character_frame(&[record], [1, 1, 1, 1, 0, 0]),
                CharacterValidationRule::MetricIndexOutOfRange {
                    character: 7,
                    metric,
                    index: 1,
                    count: 1,
                },
            );
        }
    }

    #[test]
    fn counts_above_packed_index_width_accept_every_encodable_index() {
        let bytes = character_frame(&[[255, 0xff, 0xfc, 255]], [256, 16, 16, 64, 0, 0]);
        let state = check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap();
        assert_eq!(state.records[0].width_index, 255);
        assert_eq!(state.records[0].height_index, 15);
        assert_eq!(state.records[0].depth_index, 15);
        assert_eq!(state.records[0].italic_index, 63);
    }

    #[test]
    fn untagged_record_ignores_the_remainder() {
        let bytes = character_frame(&[[0, 0, 0, 255]], [1, 1, 1, 1, 0, 0]);
        let state = check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap();
        assert_eq!(state.records[0].tag, CharacterTag::None);
    }

    #[test]
    fn ligature_tag_checks_the_exact_table_boundary_even_when_width_is_zero() {
        let accepted = character_frame(&[[0, 0, 1, 1]], [1, 1, 1, 1, 2, 0]);
        assert!(check_characters(check_preamble_header(Arc::from(accepted), 1).unwrap()).is_ok());

        for (count, index) in [(0, 0), (2, 2)] {
            assert_character_rule(
                character_frame(&[[0, 0, 1, index as u8]], [1, 1, 1, 1, count, 0]),
                CharacterValidationRule::LigatureIndexOutOfRange {
                    character: 7,
                    index: index as u8,
                    count,
                },
            );
        }
    }

    #[test]
    fn ligature_count_above_byte_range_accepts_every_encodable_index() {
        let bytes = character_frame(&[[0, 0, 1, 255]], [1, 1, 1, 1, 256, 0]);
        assert!(check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).is_ok());
    }

    #[test]
    fn extensible_tag_checks_the_exact_table_boundary_even_when_width_is_zero() {
        let accepted = character_frame(&[[0, 0, 3, 1]], [1, 1, 1, 1, 0, 2]);
        assert!(check_characters(check_preamble_header(Arc::from(accepted), 1).unwrap()).is_ok());

        for (count, index) in [(0, 0), (2, 2)] {
            assert_character_rule(
                character_frame(&[[0, 0, 3, index as u8]], [1, 1, 1, 1, 0, count]),
                CharacterValidationRule::ExtensibleIndexOutOfRange {
                    character: 7,
                    index: index as u8,
                    count,
                },
            );
        }
    }

    #[test]
    fn extensible_count_above_byte_range_accepts_every_encodable_index() {
        let bytes = character_frame(&[[0, 0, 3, 255]], [1, 1, 1, 1, 0, 256]);
        assert!(check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).is_ok());
    }

    #[test]
    fn charlist_targets_accept_both_domain_endpoints() {
        for records in [[[1, 0, 2, 8], [1, 0, 0, 0]], [[1, 0, 0, 0], [1, 0, 2, 7]]] {
            let bytes = character_frame(&records, [2, 1, 1, 1, 0, 0]);
            assert!(check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).is_ok());
        }
    }

    #[test]
    fn charlist_target_must_remain_inside_the_normalized_domain() {
        for target in [6, 8] {
            assert_character_rule(
                character_frame(&[[0, 0, 2, target]], [1, 1, 1, 1, 0, 0]),
                CharacterValidationRule::CharListTargetOutOfRange {
                    character: 7,
                    target,
                    first: 7,
                    last: 7,
                },
            );
        }
    }

    #[test]
    fn charlist_target_need_not_denote_an_existing_character() {
        let bytes = character_frame(&[[1, 0, 2, 8], [0, 0, 0, 0]], [2, 1, 1, 1, 0, 0]);
        let state = check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap();
        assert_eq!(state.records[0].tag, CharacterTag::List { target: 8 });
        assert_eq!(state.existing_characters.0, [1 << 7, 0, 0, 0]);
    }

    #[test]
    fn width_zero_source_does_not_skip_charlist_range_validation() {
        assert_character_rule(
            character_frame(&[[0, 0, 2, 6]], [1, 1, 1, 1, 0, 0]),
            CharacterValidationRule::CharListTargetOutOfRange {
                character: 7,
                target: 6,
                first: 7,
                last: 7,
            },
        );
    }

    #[test]
    fn charlist_rejects_self_two_three_and_longer_cycles() {
        for (records, expected_character) in [
            (vec![[1, 0, 2, 7]], 7),
            (vec![[1, 0, 2, 8], [1, 0, 2, 7]], 8),
            (vec![[1, 0, 2, 8], [1, 0, 2, 9], [1, 0, 2, 7]], 9),
            (
                vec![[1, 0, 2, 8], [1, 0, 2, 9], [1, 0, 2, 10], [1, 0, 2, 7]],
                10,
            ),
        ] {
            assert_character_rule(
                character_frame(&records, [2, 1, 1, 1, 0, 0]),
                CharacterValidationRule::CharListCycle {
                    character: expected_character,
                },
            );
        }
    }

    #[test]
    fn charlist_accepts_increasing_decreasing_and_mixed_acyclic_chains() {
        for records in [
            vec![[1, 0, 2, 8], [1, 0, 2, 9], [1, 0, 0, 0]],
            vec![[1, 0, 0, 0], [1, 0, 2, 7], [1, 0, 2, 8]],
            vec![[1, 0, 2, 8], [1, 0, 0, 0], [1, 0, 2, 7]],
        ] {
            let bytes = character_frame(&records, [2, 1, 1, 1, 0, 0]);
            assert!(check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).is_ok());
        }
    }

    #[test]
    fn charlist_cycle_detection_does_not_depend_on_character_existence() {
        assert_character_rule(
            character_frame(&[[0, 0, 2, 8], [0, 0, 2, 7]], [1, 1, 1, 1, 0, 0]),
            CharacterValidationRule::CharListCycle { character: 8 },
        );
    }

    #[test]
    fn full_domain_charlist_chain_and_cycle_are_bounded() {
        let mut acyclic = vec![[1, 0, 0, 0]; 256];
        for (character, record) in acyclic.iter_mut().enumerate().skip(1) {
            *record = [1, 0, 2, (character - 1) as u8];
        }
        let bytes = character_frame_at(&acyclic, 0, [2, 1, 1, 1, 0, 0]);
        assert!(check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).is_ok());

        let mut cyclic = vec![[1, 0, 2, 0]; 256];
        for (character, record) in cyclic.iter_mut().enumerate().take(255) {
            record[3] = (character + 1) as u8;
        }
        assert_character_rule(
            character_frame_at(&cyclic, 0, [2, 1, 1, 1, 0, 0]),
            CharacterValidationRule::CharListCycle { character: 255 },
        );
    }

    #[test]
    fn exhaustive_small_charlist_graphs_match_an_independent_cycle_oracle() {
        for domain_size in 1..=5usize {
            let variants = domain_size + 1;
            for mut encoded_graph in 0..variants.pow(domain_size as u32) {
                let mut choices = vec![0usize; domain_size];
                let mut records = vec![[1, 0, 0, 0]; domain_size];
                for (choice, record) in choices.iter_mut().zip(&mut records) {
                    *choice = encoded_graph % variants;
                    encoded_graph /= variants;
                    if *choice != 0 {
                        record[2] = 2;
                        record[3] = (*choice - 1) as u8;
                    }
                }

                let mut reference_acyclic = true;
                for start in 0..domain_size {
                    let mut seen = [false; 5];
                    let mut current = start;
                    loop {
                        if seen[current] {
                            reference_acyclic = false;
                            break;
                        }
                        seen[current] = true;
                        if choices[current] == 0 {
                            break;
                        }
                        current = choices[current] - 1;
                    }
                    if !reference_acyclic {
                        break;
                    }
                }

                let bytes = character_frame_at(&records, 0, [2, 1, 1, 1, 0, 0]);
                let result = check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap());
                assert!(
                    !matches!(
                        result,
                        Err(CharacterValidationRule::CharListTraversalLimit { .. })
                    ),
                    "reachable traversal limit: domain={domain_size} choices={choices:?}"
                );
                let actual = result.is_ok();
                assert_eq!(
                    actual, reference_acyclic,
                    "domain={domain_size} choices={choices:?}"
                );
            }
        }
    }

    #[test]
    fn character_failures_follow_record_and_field_source_order() {
        assert_character_rule(
            character_frame(&[[1, 0, 2, 6]], [1, 1, 1, 1, 0, 0]),
            CharacterValidationRule::MetricIndexOutOfRange {
                character: 7,
                metric: CharacterMetric::Width,
                index: 1,
                count: 1,
            },
        );
        assert_character_rule(
            character_frame(&[[0, 0, 1, 0], [0, 0, 2, 8]], [1, 1, 1, 1, 0, 0]),
            CharacterValidationRule::LigatureIndexOutOfRange {
                character: 7,
                index: 0,
                count: 0,
            },
        );
        assert_character_rule(
            character_frame(&[[0, 0x10, 0, 0], [0, 0, 2, 8]], [1, 1, 1, 1, 0, 0]),
            CharacterValidationRule::MetricIndexOutOfRange {
                character: 7,
                metric: CharacterMetric::Height,
                index: 1,
                count: 1,
            },
        );
    }

    #[test]
    fn adjacent_character_metric_failures_follow_source_order() {
        assert_eq!(
            CHARACTER_METRIC_SOURCE_ORDER,
            [
                CharacterMetric::Width,
                CharacterMetric::Height,
                CharacterMetric::Depth,
                CharacterMetric::Italic,
            ]
        );
        for (record, metric) in [
            ([1, 0x10, 0x00, 0], CharacterMetric::Width),
            ([0, 0x11, 0x00, 0], CharacterMetric::Height),
            ([0, 0x01, 0x04, 0], CharacterMetric::Depth),
            ([0, 0x00, 0x05, 0], CharacterMetric::Italic),
        ] {
            assert_character_rule(
                character_frame(&[record], [1, 1, 1, 1, 0, 0]),
                CharacterValidationRule::MetricIndexOutOfRange {
                    character: 7,
                    metric,
                    index: 1,
                    count: 1,
                },
            );
        }
    }

    #[test]
    fn header_invalidity_prevents_character_construction() {
        let mut bytes = character_frame(&[[255; 4]], [256, 16, 16, 64, 256, 256]);
        bytes.pop();
        assert!(matches!(
            check_preamble_header(Arc::from(bytes), 1),
            Err(PreambleHeaderFailure::Malformed(
                PreambleHeaderRule::DeclaredFrameUnavailable
            ))
        ));
    }

    #[test]
    fn later_table_contents_do_not_affect_character_validation() {
        let mut base = character_frame(&[[1, 0, 0, 0]], [2, 1, 1, 1, 1, 1]);
        put_count(&mut base, 0, 18);
        put_count(&mut base, 9, 1);
        put_count(&mut base, 11, 1);
        base.extend_from_slice(&[0; 8]);
        let layout = check_preamble_header(Arc::from(base.clone()), 1)
            .unwrap()
            .layout;

        for range in [
            layout.widths,
            layout.heights,
            layout.depths,
            layout.italics,
            layout.lig_kern,
            layout.kerns,
            layout.extensibles,
            layout.parameters,
        ] {
            let mut mutated = base.clone();
            mutated[range.start..range.end].fill(0xff);
            let header = check_preamble_header(Arc::from(mutated), 1).unwrap();
            assert!(check_characters(header).is_ok(), "later range {range:?}");
        }
    }

    #[test]
    fn suffixes_preserve_character_semantics_and_frame_identity() {
        let base = character_frame(&[[1, 0, 2, 8], [0, 0, 0, 0]], [2, 1, 1, 1, 0, 0]);
        let control =
            check_characters(check_preamble_header(Arc::from(base.clone()), 12_345).unwrap())
                .unwrap();
        for suffix_length in [1, 2, 3, 4, 65, 8193] {
            let mut bytes = base.clone();
            bytes.extend((0..suffix_length).map(|index| (index as u8).wrapping_mul(37)));
            let state =
                check_characters(check_preamble_header(Arc::from(bytes), 12_345).unwrap()).unwrap();
            assert_eq!(state.records, control.records);
            assert_eq!(state.existing_characters, control.existing_characters);
            assert_eq!(
                state.predecessor.frame_digest,
                control.predecessor.frame_digest
            );
            assert_ne!(state.predecessor.raw_digest, control.predecessor.raw_digest);
            assert_eq!(
                state.predecessor.effective_size,
                control.predecessor.effective_size
            );
        }
    }

    #[test]
    fn generated_full_domain_records_preserve_all_checked_records() {
        let mut records = vec![[0; 4]; 256];
        for (character, record) in records.iter_mut().enumerate() {
            record[0] = character as u8;
            record[1] = (((character % 16) << 4) | ((255 - character) % 16)) as u8;
            let italic = (character % 64) as u8;
            match character % 4 {
                0 => *record = [record[0], record[1], italic << 2, 255],
                1 => *record = [record[0], record[1], (italic << 2) | 1, 255],
                2 => *record = [record[0], record[1], (italic << 2) | 2, 0],
                3 => *record = [record[0], record[1], (italic << 2) | 3, 255],
                _ => unreachable!(),
            }
        }
        let bytes = character_frame_at(&records, 0, [256, 16, 16, 64, 256, 256]);
        let state = check_characters(check_preamble_header(Arc::from(bytes), 1).unwrap()).unwrap();
        assert_eq!(state.records.len(), 256);
        assert_eq!(state.records[0].character, 0);
        assert_eq!(state.records[255].character, 255);
        assert_eq!(state.existing_characters.0[0] & 1, 0);
        assert_ne!(state.existing_characters.0[3] & (1 << 63), 0);
    }

    #[test]
    fn bounded_generated_character_bytes_never_panic() {
        let mut generator = 0x6a8e_45fc_4bd0_83eeu64;
        for case_index in 0..512 {
            generator = generator
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let record_count = 1 + (generator as usize) % 256;
            let mut records = vec![[0; 4]; record_count];
            for record in &mut records {
                for byte in record {
                    generator = generator
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1);
                    *byte = (generator >> 32) as u8;
                }
            }
            let bytes = character_frame_at(&records, 0, [256, 16, 16, 64, 256, 256]);
            let header = check_preamble_header(Arc::from(bytes), 1).unwrap();
            let result = std::panic::catch_unwind(|| check_characters(header));
            assert!(result.is_ok(), "case {case_index} panicked");
            assert!(
                !matches!(
                    result.unwrap(),
                    Err(CharacterValidationRule::CharListTraversalLimit { .. })
                ),
                "case {case_index} reached the defensive traversal limit"
            );
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

    fn character_frame(records: &[[u8; 4]], counts: [u16; 6]) -> Vec<u8> {
        character_frame_at(records, 7, counts)
    }

    fn character_frame_at(records: &[[u8; 4]], first: u8, counts: [u16; 6]) -> Vec<u8> {
        let [nw, nh, nd, ni, nl, ne] = counts;
        let character_count = u16::try_from(records.len()).unwrap();
        assert!(character_count > 0);
        let last = u16::from(first) + character_count - 1;
        assert!(last <= 255);
        let lf = 6 + 2 + character_count + nw + nh + nd + ni + nl + ne;
        let mut bytes = vec![0; usize::from(lf) * 4];
        for (index, value) in [lf, 2, u16::from(first), last, nw, nh, nd, ni, nl, 0, ne, 0]
            .into_iter()
            .enumerate()
        {
            put_count(&mut bytes, index, value);
        }
        bytes[28..32].copy_from_slice(&(1i32 << 20).to_be_bytes());
        for (index, record) in records.iter().enumerate() {
            let offset = 32 + index * 4;
            bytes[offset..offset + 4].copy_from_slice(record);
        }
        bytes
    }

    fn box_frame_with_words(counts: [u16; 4], words: &[(BoxMetric, u16, [u8; 4])]) -> Vec<u8> {
        let [nw, nh, nd, ni] = counts;
        let mut bytes = character_frame(&[[0; 4]], [nw, nh, nd, ni, 0, 0]);
        let layout = check_preamble_header(Arc::from(bytes.clone()), 1)
            .unwrap()
            .layout;
        for &(table, index, word) in words {
            let range = match table {
                BoxMetric::Width => &layout.widths,
                BoxMetric::Height => &layout.heights,
                BoxMetric::Depth => &layout.depths,
                BoxMetric::Italic => &layout.italics,
            };
            let start = range.start + usize::from(index) * 4;
            bytes[start..start + 4].copy_from_slice(&word);
        }
        bytes
    }

    fn lig_kern_frame(records: &[[u8; 4]], instructions: &[[u8; 4]], kern_count: u16) -> Vec<u8> {
        let mut bytes = character_frame(
            records,
            [2, 1, 1, 1, u16::try_from(instructions.len()).unwrap(), 0],
        );
        let extra_bytes = usize::from(kern_count) * 4;
        bytes.resize(bytes.len() + extra_bytes, 0);
        let word_count = u16::try_from(bytes.len() / 4).unwrap();
        put_count(&mut bytes, 0, word_count);
        put_count(&mut bytes, 9, kern_count);
        let layout = check_preamble_header(Arc::from(bytes.clone()), 1)
            .unwrap()
            .layout;
        for (slot, instruction) in bytes[layout.lig_kern].chunks_exact_mut(4).zip(instructions) {
            slot.copy_from_slice(instruction);
        }
        bytes
    }

    fn kern_frame(kerns: &[[u8; 4]], extensibles: &[[u8; 4]], parameters: &[[u8; 4]]) -> Vec<u8> {
        let nk = u16::try_from(kerns.len()).unwrap();
        let ne = u16::try_from(extensibles.len()).unwrap();
        let np = u16::try_from(parameters.len()).unwrap();
        let lf = 12 + nk + ne + np;
        let mut bytes = vec![0; usize::from(lf) * 4];
        for (index, value) in [lf, 2, 1, 0, 1, 1, 1, 1, 0, nk, ne, np]
            .into_iter()
            .enumerate()
        {
            put_count(&mut bytes, index, value);
        }
        bytes[28..32].copy_from_slice(&(1i32 << 20).to_be_bytes());
        let layout = check_preamble_header(Arc::from(bytes.clone()), 1)
            .unwrap()
            .layout;
        for (slot, word) in bytes[layout.kerns].chunks_exact_mut(4).zip(kerns) {
            slot.copy_from_slice(word);
        }
        for (slot, word) in bytes[layout.extensibles]
            .chunks_exact_mut(4)
            .zip(extensibles)
        {
            slot.copy_from_slice(word);
        }
        for (slot, word) in bytes[layout.parameters].chunks_exact_mut(4).zip(parameters) {
            slot.copy_from_slice(word);
        }
        bytes
    }

    fn extensible_frame_at(
        first: u8,
        records: &[[u8; 4]],
        extensibles: &[[u8; 4]],
        parameters: &[[u8; 4]],
    ) -> Vec<u8> {
        let ne = u16::try_from(extensibles.len()).unwrap();
        let np = u16::try_from(parameters.len()).unwrap();
        let mut bytes = character_frame_at(records, first, [2, 1, 1, 1, 0, ne]);
        bytes.resize(bytes.len() + usize::from(np) * 4, 0);
        let word_count = u16::try_from(bytes.len() / 4).unwrap();
        put_count(&mut bytes, 0, word_count);
        put_count(&mut bytes, 11, np);
        let layout = check_preamble_header(Arc::from(bytes.clone()), 1)
            .unwrap()
            .layout;
        for (slot, recipe) in bytes[layout.extensibles]
            .chunks_exact_mut(4)
            .zip(extensibles)
        {
            slot.copy_from_slice(recipe);
        }
        for (slot, parameter) in bytes[layout.parameters].chunks_exact_mut(4).zip(parameters) {
            slot.copy_from_slice(parameter);
        }
        bytes
    }

    fn check_kern_frame(
        bytes: Vec<u8>,
        effective_size_sp: i32,
    ) -> Result<KernCheckedTfm, KernValidationRule> {
        check_kerns(
            check_lig_kern(
                check_boxes(
                    check_characters(
                        check_preamble_header(Arc::from(bytes), effective_size_sp).unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
            .unwrap(),
        )
    }

    fn check_extensible_frame(
        bytes: Vec<u8>,
        effective_size_sp: i32,
    ) -> Result<ExtensibleCheckedTfm, ExtensibleValidationRule> {
        check_extensibles(check_kern_frame(bytes, effective_size_sp).unwrap())
    }

    fn check_parameter_frame(
        bytes: Vec<u8>,
        effective_size_sp: i32,
    ) -> Result<ParameterCheckedTfm, ParameterValidationRule> {
        check_parameters(check_extensible_frame(bytes, effective_size_sp).unwrap())
    }

    fn literal_signed_slant(word: [u8; 4]) -> SignedSlant {
        let signed_high_byte = if word[0] < 128 {
            i32::from(word[0])
        } else {
            i32::from(word[0]) - 256
        };
        SignedSlant(
            signed_high_byte * (1 << 20)
                + i32::from(word[1]) * (1 << 12)
                + i32::from(word[2]) * (1 << 4)
                + i32::from(word[3] / 16),
        )
    }

    fn literal_scaled_parameter(word: [u8; 4], effective_size_sp: i32) -> Result<ScaledSp, u8> {
        if !matches!(word[0], 0 | 255) {
            return Err(word[0]);
        }
        assert!((1..MAX_TEX_FONT_SIZE_SP).contains(&effective_size_sp));

        let size_shift = if effective_size_sp < 1 << 23 {
            0
        } else if effective_size_sp < 1 << 24 {
            1
        } else if effective_size_sp < 1 << 25 {
            2
        } else if effective_size_sp < 1 << 26 {
            3
        } else {
            4
        };
        let reduced_size = i128::from(effective_size_sp >> size_shift);
        let alpha_factor = 16i128 << size_shift;
        let beta = 256 / alpha_factor;
        let magnitude =
            i128::from(word[1]) * 65_536 + i128::from(word[2]) * 256 + i128::from(word[3]);
        let positive_fraction = magnitude * reduced_size / (65_536 * beta);
        let scaled = if word[0] == 0 {
            positive_fraction
        } else {
            positive_fraction - alpha_factor * reduced_size
        };
        Ok(ScaledSp(i32::try_from(scaled).unwrap()))
    }

    fn maximum_lig_kern_frame(instructions: &[[u8; 4]]) -> Vec<u8> {
        assert_eq!(instructions.len(), 32_755);
        let mut bytes = vec![0; 32_767 * 4];
        for (index, value) in [32_767, 2, 1, 0, 1, 1, 1, 1, 32_755, 0, 0, 0]
            .into_iter()
            .enumerate()
        {
            put_count(&mut bytes, index, value);
        }
        bytes[28..32].copy_from_slice(&(1i32 << 20).to_be_bytes());
        let layout = check_preamble_header(Arc::from(bytes.clone()), 1)
            .unwrap()
            .layout;
        for (slot, instruction) in bytes[layout.lig_kern].chunks_exact_mut(4).zip(instructions) {
            slot.copy_from_slice(instruction);
        }
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

    fn assert_character_rule(bytes: Vec<u8>, expected: CharacterValidationRule) {
        let header = check_preamble_header(Arc::from(bytes), 1).unwrap();
        assert_eq!(check_characters(header).err(), Some(expected));
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
