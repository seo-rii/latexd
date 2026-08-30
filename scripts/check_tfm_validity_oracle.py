#!/usr/bin/env python3
"""Characterize TeX82 TFM validity boundaries needed before DP1 font loading."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[1]
EXPECTED_FIXTURE = (
    REPOSITORY
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validity-oracle-v1.json"
)
RULE_CONTRACT = (
    REPOSITORY
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validation-rules-v1.json"
)
CORPUS_ROOT = (
    REPOSITORY
    / "crates/tex-tfm-metrics/tests/fixtures/tfm-validity-oracle-v2"
)
CORPUS_MANIFEST = CORPUS_ROOT / "manifest.json"
REVIEWED_V2_CORPUS_MANIFEST_CANONICAL_SHA256 = (
    "658a807fbc5f3a07e0bdf766590e39eb339db7fe29d0302276c80b60456b8a70"
)

TEX82_READ_FONT_INFO_SOURCE = {
    "url": "https://tug.ctan.org/systems/knuth/dist/tex/tex.web",
    "sha256": "c62ab513ef167e93f71a23bd34f311e243210afd7c7a0f9b779614b71e398324",
    "loader_section_lines": [10870, 11210],
    "loader_section_sha256": "57f665ae4cc87c721d444fdde0a1817f194f44bab18388c42a1d26d830c6ddc8",
}

CMR10_TFM = "crates/tex-fonts/assets/classic/tfm/cmr10.tfm"
CMEX10_TFM = "crates/tex-fonts/assets/classic/tfm/cmex10.tfm"

CASE_SPECS = {
    "valid_cmr10": {
        "base_tfm": CMR10_TFM,
        "description": "unmodified cmr10 control",
    },
    "valid_cmr10_at_1sp": {
        "base_tfm": CMR10_TFM,
        "description": "unmodified cmr10 control at 1sp",
    },
    "valid_cmr10_at_16sp": {
        "base_tfm": CMR10_TFM,
        "description": "unmodified cmr10 control at 16sp",
    },
    "valid_cmr10_at_max_sp": {
        "base_tfm": CMR10_TFM,
        "description": "unmodified cmr10 at the largest valid explicit at-size",
    },
    "invalid_at_size_zero": {
        "base_tfm": CMR10_TFM,
        "description": "zero at-size is corrected before TFM validation",
    },
    "invalid_at_size_limit": {
        "base_tfm": CMR10_TFM,
        "description": "2^27sp at-size is corrected before TFM validation",
    },
    "size_field_high_bit": {
        "base_tfm": CMR10_TFM,
        "description": "a TFM size halfword uses the forbidden high bit",
    },
    "invalid_character_range": {
        "base_tfm": CMR10_TFM,
        "description": "bc is greater than ec plus one",
    },
    "character_range_ec256": {
        "base_tfm": CMR10_TFM,
        "description": "ec exceeds the TeX82 character-code range",
    },
    "empty_range_2_1": {
        "base_tfm": CMR10_TFM,
        "description": "ordinary native-valid empty character range",
    },
    "empty_range_256_255": {
        "base_tfm": CMR10_TFM,
        "description": "native-valid empty range at the normalization boundary",
    },
    "aggregate_length_mismatch": {
        "base_tfm": CMR10_TFM,
        "description": "lf disagrees with the aggregate table counts",
    },
    "short_np5": {
        "base_tfm": CMR10_TFM,
        "description": "fontdimen6 absent from an otherwise valid parameter table",
    },
    "short_np4": {
        "base_tfm": CMR10_TFM,
        "description": "fontdimen5 and fontdimen6 both absent",
    },
    "short_np0": {
        "base_tfm": CMR10_TFM,
        "description": "empty parameter table receives TeX82 zero defaults",
    },
    "trailing_word": {
        "base_tfm": CMR10_TFM,
        "description": "one complete word follows the declared TFM length",
    },
    "trailing_1_byte_nonzero": {
        "base_tfm": CMR10_TFM,
        "description": "one nonzero byte follows the declared TFM length",
    },
    "trailing_2_bytes_nonzero": {
        "base_tfm": CMR10_TFM,
        "description": "two nonzero bytes follow the declared TFM length",
    },
    "trailing_3_bytes_nonzero": {
        "base_tfm": CMR10_TFM,
        "description": "three nonzero bytes follow the declared TFM length",
    },
    "trailing_long_nonzero": {
        "base_tfm": CMR10_TFM,
        "description": "8193 nonzero bytes follow the declared TFM length",
    },
    "zero_width_table_consistent": {
        "base_tfm": CMR10_TFM,
        "description": "zero width-table count with a compensating table count",
    },
    "zero_height_table_consistent": {
        "base_tfm": CMR10_TFM,
        "description": "zero height-table count with a compensating table count",
    },
    "zero_depth_table_consistent": {
        "base_tfm": CMR10_TFM,
        "description": "zero depth-table count with a compensating table count",
    },
    "zero_italic_table_consistent": {
        "base_tfm": CMR10_TFM,
        "description": "zero italic-table count with a compensating table count",
    },
    "short_header": {
        "base_tfm": CMR10_TFM,
        "description": "the header contains fewer than checksum and design-size words",
    },
    "minimal_header_lh2": {
        "base_tfm": CMR10_TFM,
        "description": "the header has exactly the two words TeX82 reads",
    },
    "design_size_below_one_pt": {
        "base_tfm": CMR10_TFM,
        "description": "the design size is one fix-word unit below 1pt",
    },
    "design_size_exactly_one_pt": {
        "base_tfm": CMR10_TFM,
        "description": "the design size is exactly the 1pt acceptance boundary",
    },
    "invalid_character_width_index": {
        "base_tfm": CMR10_TFM,
        "description": "character info addresses width index nw",
    },
    "invalid_character_height_index": {
        "base_tfm": CMR10_TFM,
        "description": "character info addresses height index nh",
    },
    "invalid_character_depth_index": {
        "base_tfm": CMR10_TFM,
        "description": "character info addresses depth index nd",
    },
    "invalid_character_italic_index": {
        "base_tfm": CMR10_TFM,
        "description": "character info addresses italic index ni",
    },
    "invalid_character_ligature_index": {
        "base_tfm": CMR10_TFM,
        "description": "ligature-tagged character info addresses instruction nl",
    },
    "invalid_character_extensible_index": {
        "base_tfm": CMR10_TFM,
        "description": "extensible-tagged character info addresses recipe ne",
    },
    "charlist_out_of_range": {
        "base_tfm": CMR10_TFM,
        "description": "character list points outside bc through ec",
    },
    "charlist_self_cycle": {
        "base_tfm": CMR10_TFM,
        "description": "character list points back to the same character",
    },
    "valid_charlist_acyclic_chain": {
        "base_tfm": CMR10_TFM,
        "description": "three character records form an acyclic list",
    },
    "charlist_two_node_cycle": {
        "base_tfm": CMR10_TFM,
        "description": "two character records form a list cycle",
    },
    "charlist_three_node_cycle": {
        "base_tfm": CMR10_TFM,
        "description": "three character records form a list cycle",
    },
    "charlist_target_in_range_absent": {
        "base_tfm": CMR10_TFM,
        "description": "charlist range accepts a target whose width index is zero",
    },
    "invalid_width_fix_word_sign": {
        "base_tfm": CMR10_TFM,
        "description": "unselected width fix word has a forbidden sign byte",
    },
    "invalid_height_fix_word_sign": {
        "base_tfm": CMR10_TFM,
        "description": "height fix word has a forbidden sign byte",
    },
    "invalid_depth_fix_word_sign": {
        "base_tfm": CMR10_TFM,
        "description": "depth fix word has a forbidden sign byte",
    },
    "invalid_italic_fix_word_sign": {
        "base_tfm": CMR10_TFM,
        "description": "italic fix word has a forbidden sign byte",
    },
    "nonzero_width_zero": {
        "base_tfm": CMR10_TFM,
        "description": "width table entry zero scales to a nonzero dimension",
    },
    "nonzero_width_zero_at_1sp": {
        "base_tfm": CMR10_TFM,
        "description": "nonzero raw width[0] scales to zero at 1sp",
    },
    "nonzero_width_zero_at_16sp": {
        "base_tfm": CMR10_TFM,
        "description": "nonzero raw width[0] remains nonzero at 16sp",
    },
    "nonzero_height_zero": {
        "base_tfm": CMR10_TFM,
        "description": "height table entry zero scales to a nonzero dimension",
    },
    "nonzero_height_zero_at_1sp": {
        "base_tfm": CMR10_TFM,
        "description": "nonzero raw height[0] scales to zero at 1sp",
    },
    "nonzero_height_zero_at_16sp": {
        "base_tfm": CMR10_TFM,
        "description": "nonzero raw height[0] remains nonzero at 16sp",
    },
    "nonzero_depth_zero": {
        "base_tfm": CMR10_TFM,
        "description": "depth table entry zero scales to a nonzero dimension",
    },
    "nonzero_depth_zero_at_1sp": {
        "base_tfm": CMR10_TFM,
        "description": "nonzero raw depth[0] scales to zero at 1sp",
    },
    "nonzero_depth_zero_at_16sp": {
        "base_tfm": CMR10_TFM,
        "description": "nonzero raw depth[0] remains nonzero at 16sp",
    },
    "nonzero_italic_zero": {
        "base_tfm": CMR10_TFM,
        "description": "italic table entry zero scales to a nonzero dimension",
    },
    "nonzero_italic_zero_at_1sp": {
        "base_tfm": CMR10_TFM,
        "description": "nonzero raw italic[0] scales to zero at 1sp",
    },
    "nonzero_italic_zero_at_16sp": {
        "base_tfm": CMR10_TFM,
        "description": "nonzero raw italic[0] remains nonzero at 16sp",
    },
    "invalid_fontdimen2": {
        "base_tfm": CMR10_TFM,
        "description": "unselected fontdimen2 has a forbidden fix-word sign byte",
    },
    "invalid_fontdimen5": {
        "base_tfm": CMR10_TFM,
        "description": "selected fontdimen5 has a forbidden fix-word sign byte",
    },
    "invalid_ligkern": {
        "base_tfm": CMR10_TFM,
        "description": "ligature/kern instruction jumps outside its table",
    },
    "invalid_ligkern_next_character": {
        "base_tfm": CMR10_TFM,
        "description": "ligature/kern instruction names an absent next character",
    },
    "invalid_ligature_target": {
        "base_tfm": CMR10_TFM,
        "description": "ligature instruction names an absent replacement character",
    },
    "invalid_ligkern_kern_index": {
        "base_tfm": CMR10_TFM,
        "description": "kern instruction addresses kern index nk",
    },
    "invalid_ligkern_skip": {
        "base_tfm": CMR10_TFM,
        "description": "ligature/kern skip advances past the instruction table",
    },
    "valid_ligkern_restart": {
        "base_tfm": CMR10_TFM,
        "description": "ligature/kern restart points inside its table",
    },
    "valid_boundary_character_absent_next_bypass": {
        "base_tfm": CMR10_TFM,
        "description": "the declared boundary character bypasses next-character existence",
    },
    "ligkern_next_in_range_absent": {
        "base_tfm": CMR10_TFM,
        "description": "ordinary next character has an in-range zero width index",
    },
    "ligature_target_in_range_absent": {
        "base_tfm": CMR10_TFM,
        "description": "ligature target has an in-range zero width index",
    },
    "valid_boundary_label": {
        "base_tfm": CMR10_TFM,
        "description": "terminal boundary label points inside the instruction table",
    },
    "invalid_boundary_label": {
        "base_tfm": CMR10_TFM,
        "description": "terminal boundary label points outside the instruction table",
    },
    "invalid_kern_fix_word": {
        "base_tfm": CMR10_TFM,
        "description": "kern fix word has a forbidden sign byte",
    },
    "invalid_extensible": {
        "base_tfm": CMEX10_TFM,
        "description": "extensible recipe references a character outside the range",
    },
    "invalid_extensible_top": {
        "base_tfm": CMEX10_TFM,
        "description": "extensible recipe top references a character outside the range",
    },
    "invalid_extensible_middle": {
        "base_tfm": CMEX10_TFM,
        "description": "extensible recipe middle references a character outside the range",
    },
    "invalid_extensible_bottom": {
        "base_tfm": CMEX10_TFM,
        "description": "extensible recipe bottom references a character outside the range",
    },
    "extensible_top_in_range_absent": {
        "base_tfm": CMEX10_TFM,
        "description": "extensible top has an in-range zero width index",
    },
    "extensible_middle_in_range_absent": {
        "base_tfm": CMEX10_TFM,
        "description": "extensible middle has an in-range zero width index",
    },
    "extensible_bottom_in_range_absent": {
        "base_tfm": CMEX10_TFM,
        "description": "extensible bottom has an in-range zero width index",
    },
    "extensible_repeat_in_range_absent": {
        "base_tfm": CMEX10_TFM,
        "description": "extensible repeat has an in-range zero width index",
    },
    "signed_slant_parameter": {
        "base_tfm": CMR10_TFM,
        "description": "fontdimen1 uses a sign byte that is valid for the pure-number slant",
    },
    "parameter_count_8_valid": {
        "base_tfm": CMR10_TFM,
        "description": "an eighth zero parameter is validated and accepted",
    },
    "parameter_8_invalid_fix_word": {
        "base_tfm": CMR10_TFM,
        "description": "the eighth parameter has a forbidden fix-word sign byte",
    },
    "premature_eof": {
        "base_tfm": CMR10_TFM,
        "description": "the final byte declared by lf is missing",
    },
}

SUPPLEMENTAL_CORPUS_SPECS = {
    "design_size_largest_positive": {
        "base_tfm": CMR10_TFM,
        "description": "largest positive TeX82 design-size encoding",
        "expected_classification": "AcceptedByNativeLoader",
    }
}
CORPUS_CASE_SPECS = CASE_SPECS | SUPPLEMENTAL_CORPUS_SPECS

CASE_SIZES = {case_id: {"mode": "natural"} for case_id in CASE_SPECS}
CASE_SIZES.update(
    {
        "valid_cmr10_at_1sp": {"mode": "at_sp", "value": 1},
        "valid_cmr10_at_16sp": {"mode": "at_sp", "value": 16},
        "valid_cmr10_at_max_sp": {"mode": "at_sp", "value": (1 << 27) - 1},
        "invalid_at_size_zero": {"mode": "at_sp", "value": 0},
        "invalid_at_size_limit": {"mode": "at_sp", "value": 1 << 27},
        "nonzero_width_zero_at_1sp": {"mode": "at_sp", "value": 1},
        "nonzero_width_zero_at_16sp": {"mode": "at_sp", "value": 16},
        "nonzero_height_zero_at_1sp": {"mode": "at_sp", "value": 1},
        "nonzero_height_zero_at_16sp": {"mode": "at_sp", "value": 16},
        "nonzero_depth_zero_at_1sp": {"mode": "at_sp", "value": 1},
        "nonzero_depth_zero_at_16sp": {"mode": "at_sp", "value": 16},
        "nonzero_italic_zero_at_1sp": {"mode": "at_sp", "value": 1},
        "nonzero_italic_zero_at_16sp": {"mode": "at_sp", "value": 16},
    }
)
CORPUS_CASE_SIZES = CASE_SIZES | {
    case_id: {"mode": "natural"} for case_id in SUPPLEMENTAL_CORPUS_SPECS
}

OBSERVATION_PATTERN = re.compile(
    r"LATEXD-TFMV:([A-Za-z0-9_]+)=([A-Za-z0-9_.:/+-]+)"
)

PROBE_SOURCE = r"""
\catcode123=1
\catcode125=2
\font\baseline=cmr10
\let\probe=\baseline
\font\probe=latexdprobe
\probe
\dimen0=\fontdimen6\font
\dimen2=\fontdimen5\font
\message{^^JLATEXD-TFMV:font=\fontname\font}
\message{^^JLATEXD-TFMV:quad=\number\dimen0}
\message{^^JLATEXD-TFMV:xheight=\number\dimen2}
\message{^^JLATEXD-TFMV:sentinel=1}
\end
"""


def _counts(tfm: bytes) -> list[int]:
    if len(tfm) < 24:
        raise ValueError("base TFM is shorter than its size fields")
    return [
        int.from_bytes(tfm[offset : offset + 2], "big")
        for offset in range(0, 24, 2)
    ]


def _table_offsets(tfm: bytes) -> dict[str, int]:
    _, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, _ = _counts(tfm)
    character_count = ec - bc + 1 if bc <= ec else 0
    return {
        "character": 4 * (6 + lh),
        "width": 4 * (6 + lh + character_count),
        "height": 4 * (6 + lh + character_count + nw),
        "depth": 4 * (6 + lh + character_count + nw + nh),
        "italic": 4 * (6 + lh + character_count + nw + nh + nd),
        "ligkern": 4 * (6 + lh + character_count + nw + nh + nd + ni),
        "kern": 4 * (6 + lh + character_count + nw + nh + nd + ni + nl),
        "extensible": 4
        * (6 + lh + character_count + nw + nh + nd + ni + nl + nk),
        "parameter": 4
        * (6 + lh + character_count + nw + nh + nd + ni + nl + nk + ne),
    }


def _short_parameter_table(tfm: bytes, parameter_count: int) -> bytes:
    counts = _counts(tfm)
    original_count = counts[11]
    if original_count != 7 or not 0 <= parameter_count < original_count:
        raise ValueError("short-parameter mutations require a seven-parameter base")
    removed_words = original_count - parameter_count
    mutated = bytearray(tfm[: len(tfm) - removed_words * 4])
    mutated[0:2] = (counts[0] - removed_words).to_bytes(2, "big")
    mutated[22:24] = parameter_count.to_bytes(2, "big")
    return bytes(mutated)


def mutate_tfm(case_id: str, base_tfm: bytes) -> bytes:
    if case_id in {
        "valid_cmr10",
        "valid_cmr10_at_1sp",
        "valid_cmr10_at_16sp",
        "valid_cmr10_at_max_sp",
        "invalid_at_size_zero",
        "invalid_at_size_limit",
    }:
        return bytes(base_tfm)
    if case_id == "short_np5":
        return _short_parameter_table(base_tfm, 5)
    if case_id == "short_np4":
        return _short_parameter_table(base_tfm, 4)
    if case_id == "short_np0":
        return _short_parameter_table(base_tfm, 0)
    if case_id == "trailing_word":
        return bytes(base_tfm) + bytes(4)
    if case_id == "trailing_1_byte_nonzero":
        return bytes(base_tfm) + bytes([165])
    if case_id == "trailing_2_bytes_nonzero":
        return bytes(base_tfm) + bytes([165]) * 2
    if case_id == "trailing_3_bytes_nonzero":
        return bytes(base_tfm) + bytes([165]) * 3
    if case_id == "trailing_long_nonzero":
        return bytes(base_tfm) + bytes([165]) * 8193
    if case_id == "premature_eof":
        return bytes(base_tfm[:-1])
    if case_id in {"parameter_count_8_valid", "parameter_8_invalid_fix_word"}:
        counts = _counts(base_tfm)
        mutated = bytearray(base_tfm)
        mutated.extend(
            bytes(4)
            if case_id == "parameter_count_8_valid"
            else bytes([1, 0, 0, 0])
        )
        mutated[0:2] = (counts[0] + 1).to_bytes(2, "big")
        mutated[22:24] = (counts[11] + 1).to_bytes(2, "big")
        return bytes(mutated)
    if case_id in {"empty_range_2_1", "empty_range_256_255"}:
        offsets = _table_offsets(base_tfm)
        character_start = offsets["character"]
        width_start = offsets["width"]
        ligkern_start = offsets["ligkern"]
        parameter_start = offsets["parameter"]
        mutated = bytearray(
            base_tfm[:character_start]
            + base_tfm[width_start:ligkern_start]
            + base_tfm[parameter_start:]
        )
        first, last = (
            (2, 1) if case_id == "empty_range_2_1" else (256, 255)
        )
        mutated[0:2] = (len(mutated) // 4).to_bytes(2, "big")
        mutated[4:6] = first.to_bytes(2, "big")
        mutated[6:8] = last.to_bytes(2, "big")
        mutated[16:22] = bytes(6)
        return bytes(mutated)
    if case_id == "minimal_header_lh2":
        counts = _counts(base_tfm)
        header_end = 4 * (6 + counts[1])
        mutated = bytearray(base_tfm[:32] + base_tfm[header_end:])
        mutated[0:2] = (len(mutated) // 4).to_bytes(2, "big")
        mutated[2:4] = (2).to_bytes(2, "big")
        return bytes(mutated)

    mutated = bytearray(base_tfm)
    offsets = _table_offsets(base_tfm)
    if case_id == "size_field_high_bit":
        mutated[8] = 128
    elif case_id == "invalid_character_range":
        mutated[4:6] = (2).to_bytes(2, "big")
        mutated[6:8] = (0).to_bytes(2, "big")
    elif case_id == "character_range_ec256":
        mutated[6:8] = (256).to_bytes(2, "big")
    elif case_id == "aggregate_length_mismatch":
        mutated[0:2] = (_counts(base_tfm)[0] + 1).to_bytes(2, "big")
    elif case_id == "zero_width_table_consistent":
        counts = _counts(base_tfm)
        mutated[8:10] = (0).to_bytes(2, "big")
        mutated[16:18] = (counts[8] + counts[4]).to_bytes(2, "big")
    elif case_id == "zero_height_table_consistent":
        counts = _counts(base_tfm)
        mutated[10:12] = (0).to_bytes(2, "big")
        mutated[18:20] = (counts[9] + counts[5]).to_bytes(2, "big")
    elif case_id == "zero_depth_table_consistent":
        counts = _counts(base_tfm)
        mutated[12:14] = (0).to_bytes(2, "big")
        mutated[18:20] = (counts[9] + counts[6]).to_bytes(2, "big")
    elif case_id == "zero_italic_table_consistent":
        counts = _counts(base_tfm)
        mutated[14:16] = (0).to_bytes(2, "big")
        mutated[18:20] = (counts[9] + counts[7]).to_bytes(2, "big")
    elif case_id == "short_header":
        counts = _counts(base_tfm)
        mutated[2:4] = (1).to_bytes(2, "big")
        mutated[18:20] = (counts[9] + counts[1] - 1).to_bytes(2, "big")
    elif case_id == "design_size_below_one_pt":
        mutated[28:32] = ((1 << 20) - 1).to_bytes(4, "big")
    elif case_id == "design_size_exactly_one_pt":
        mutated[28:32] = (1 << 20).to_bytes(4, "big")
    elif case_id == "design_size_largest_positive":
        mutated[28:32] = ((1 << 31) - 1).to_bytes(4, "big")
    elif case_id == "invalid_character_width_index":
        mutated[offsets["character"]] = _counts(base_tfm)[4]
    elif case_id == "invalid_character_height_index":
        counts = _counts(base_tfm)
        mutated[10:12] = (counts[5] - 1).to_bytes(2, "big")
        mutated[18:20] = (counts[9] + 1).to_bytes(2, "big")
        mutated[offsets["character"] + 1] = ((counts[5] - 1) << 4) | (
            mutated[offsets["character"] + 1] & 15
        )
    elif case_id == "invalid_character_depth_index":
        character_byte = mutated[offsets["character"] + 1]
        mutated[offsets["character"] + 1] = (character_byte & 240) | _counts(
            base_tfm
        )[6]
    elif case_id == "invalid_character_italic_index":
        character_byte = mutated[offsets["character"] + 2]
        mutated[offsets["character"] + 2] = (_counts(base_tfm)[7] << 2) | (
            character_byte & 3
        )
    elif case_id == "invalid_character_ligature_index":
        mutated[offsets["character"] + 2] = 1
        mutated[offsets["character"] + 3] = _counts(base_tfm)[8]
    elif case_id == "invalid_character_extensible_index":
        mutated[offsets["character"] + 2] = 3
        mutated[offsets["character"] + 3] = _counts(base_tfm)[10]
    elif case_id == "charlist_out_of_range":
        mutated[offsets["character"] + 2] = 2
        mutated[offsets["character"] + 3] = 255
    elif case_id == "charlist_self_cycle":
        mutated[offsets["character"] + 2] = 2
        mutated[offsets["character"] + 3] = _counts(base_tfm)[2]
    elif case_id in {
        "valid_charlist_acyclic_chain",
        "charlist_two_node_cycle",
        "charlist_three_node_cycle",
    }:
        links = {
            "valid_charlist_acyclic_chain": ((125, 126), (126, 127)),
            "charlist_two_node_cycle": ((126, 127), (127, 126)),
            "charlist_three_node_cycle": ((125, 126), (126, 127), (127, 125)),
        }[case_id]
        for character, target in links:
            record = offsets["character"] + 4 * (character - _counts(base_tfm)[2])
            mutated[record + 2] = (mutated[record + 2] & 252) | 2
            mutated[record + 3] = target
    elif case_id == "charlist_target_in_range_absent":
        absent_record = offsets["character"] + 4 * (127 - _counts(base_tfm)[2])
        mutated[absent_record] = 0
        mutated[offsets["character"] + 2] = (
            mutated[offsets["character"] + 2] & 252
        ) | 2
        mutated[offsets["character"] + 3] = 127
    elif case_id == "invalid_width_fix_word_sign":
        mutated[offsets["width"] + 4] = 1
    elif case_id == "invalid_height_fix_word_sign":
        mutated[offsets["height"] + 4] = 1
    elif case_id == "invalid_depth_fix_word_sign":
        mutated[offsets["depth"] + 4] = 1
    elif case_id == "invalid_italic_fix_word_sign":
        mutated[offsets["italic"] + 4] = 1
    elif case_id in {
        "nonzero_width_zero",
        "nonzero_width_zero_at_1sp",
        "nonzero_width_zero_at_16sp",
    }:
        mutated[offsets["width"] : offsets["width"] + 4] = (1 << 16).to_bytes(4, "big")
    elif case_id in {
        "nonzero_height_zero",
        "nonzero_height_zero_at_1sp",
        "nonzero_height_zero_at_16sp",
    }:
        mutated[offsets["height"] : offsets["height"] + 4] = (1 << 16).to_bytes(4, "big")
    elif case_id in {
        "nonzero_depth_zero",
        "nonzero_depth_zero_at_1sp",
        "nonzero_depth_zero_at_16sp",
    }:
        mutated[offsets["depth"] : offsets["depth"] + 4] = (1 << 16).to_bytes(4, "big")
    elif case_id in {
        "nonzero_italic_zero",
        "nonzero_italic_zero_at_1sp",
        "nonzero_italic_zero_at_16sp",
    }:
        mutated[offsets["italic"] : offsets["italic"] + 4] = (1 << 16).to_bytes(4, "big")
    elif case_id == "invalid_fontdimen2":
        mutated[offsets["parameter"] + 4] = 1
    elif case_id == "invalid_fontdimen5":
        mutated[offsets["parameter"] + 16] = 1
    elif case_id == "invalid_ligkern":
        mutated[offsets["ligkern"] : offsets["ligkern"] + 4] = bytes([129, 0, 1, 0])
    elif case_id == "invalid_ligkern_next_character":
        mutated[offsets["ligkern"] : offsets["ligkern"] + 4] = bytes([128, 255, 128, 0])
    elif case_id == "invalid_ligature_target":
        mutated[offsets["ligkern"] : offsets["ligkern"] + 4] = bytes([128, 0, 0, 255])
    elif case_id == "invalid_ligkern_kern_index":
        mutated[offsets["ligkern"] : offsets["ligkern"] + 4] = bytes(
            [128, 0, 128, _counts(base_tfm)[9]]
        )
    elif case_id == "invalid_ligkern_skip":
        mutated[offsets["ligkern"] : offsets["ligkern"] + 4] = bytes([127, 0, 128, 0])
    elif case_id == "valid_ligkern_restart":
        mutated[offsets["ligkern"] : offsets["ligkern"] + 4] = bytes([129, 0, 0, 0])
    elif case_id == "valid_boundary_character_absent_next_bypass":
        absent_record = offsets["character"] + 4 * (127 - _counts(base_tfm)[2])
        mutated[absent_record] = 0
        mutated[offsets["ligkern"] : offsets["ligkern"] + 8] = bytes(
            [255, 127, 0, 1, 128, 127, 128, 0]
        )
    elif case_id == "ligkern_next_in_range_absent":
        absent_record = offsets["character"] + 4 * (127 - _counts(base_tfm)[2])
        mutated[absent_record] = 0
        mutated[offsets["ligkern"] : offsets["ligkern"] + 4] = bytes(
            [128, 127, 128, 0]
        )
    elif case_id == "ligature_target_in_range_absent":
        absent_record = offsets["character"] + 4 * (127 - _counts(base_tfm)[2])
        mutated[absent_record] = 0
        mutated[offsets["ligkern"] : offsets["ligkern"] + 4] = bytes(
            [128, 0, 0, 127]
        )
    elif case_id in {"valid_boundary_label", "invalid_boundary_label"}:
        last_instruction = offsets["ligkern"] + 4 * (_counts(base_tfm)[8] - 1)
        target = 0 if case_id == "valid_boundary_label" else _counts(base_tfm)[8]
        mutated[last_instruction : last_instruction + 4] = bytes([255, 0, 0, target])
    elif case_id == "invalid_kern_fix_word":
        mutated[offsets["kern"]] = 1
    elif case_id == "invalid_extensible":
        if _counts(base_tfm)[10] == 0:
            raise ValueError("extensible mutation requires a nonempty recipe table")
        mutated[offsets["extensible"] + 3] = 255
    elif case_id == "invalid_extensible_top":
        if _counts(base_tfm)[10] == 0:
            raise ValueError("extensible mutation requires a nonempty recipe table")
        mutated[offsets["extensible"]] = 255
    elif case_id == "invalid_extensible_middle":
        if _counts(base_tfm)[10] == 0:
            raise ValueError("extensible mutation requires a nonempty recipe table")
        mutated[offsets["extensible"] + 1] = 255
    elif case_id == "invalid_extensible_bottom":
        if _counts(base_tfm)[10] == 0:
            raise ValueError("extensible mutation requires a nonempty recipe table")
        mutated[offsets["extensible"] + 2] = 255
    elif case_id in {
        "extensible_top_in_range_absent",
        "extensible_middle_in_range_absent",
        "extensible_bottom_in_range_absent",
        "extensible_repeat_in_range_absent",
    }:
        absent_record = offsets["character"] + 4 * (1 - _counts(base_tfm)[2])
        mutated[absent_record] = 0
        recipe_byte = {
            "extensible_top_in_range_absent": 0,
            "extensible_middle_in_range_absent": 1,
            "extensible_bottom_in_range_absent": 2,
            "extensible_repeat_in_range_absent": 3,
        }[case_id]
        mutated[offsets["extensible"] + recipe_byte] = 1
    elif case_id == "signed_slant_parameter":
        mutated[offsets["parameter"]] = 1
    else:
        raise ValueError(f"unknown TFM validity case: {case_id}")
    return bytes(mutated)


def build_case_inputs() -> dict[str, bytes]:
    return {
        case_id: mutate_tfm(
            case_id,
            (REPOSITORY / spec["base_tfm"]).read_bytes(),
        )
        for case_id, spec in CORPUS_CASE_SPECS.items()
    }


def build_corpus_manifest(case_inputs: dict[str, bytes]) -> dict[str, object]:
    fixture = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))
    rule_contract = json.loads(RULE_CONTRACT.read_text(encoding="utf-8"))
    rules = rule_contract["rules"]
    rule_ordinals = {rule["id"]: rule["source_ordinal"] for rule in rules}
    supports_by_case = {case_id: [] for case_id in CORPUS_CASE_SPECS}
    for rule in rules:
        for case_id in rule["witnesses"]:
            supports_by_case[case_id].append(rule["id"])

    cases = []
    for case_id in sorted(CORPUS_CASE_SPECS):
        oracle_result = fixture["case_results"].get(case_id)
        if case_id in {"invalid_at_size_zero", "invalid_at_size_limit"}:
            classification = "InvalidEffectiveSize"
        elif case_id in SUPPLEMENTAL_CORPUS_SPECS:
            classification = SUPPLEMENTAL_CORPUS_SPECS[case_id][
                "expected_classification"
            ]
        elif oracle_result["observations"]["font"] == "nullfont":
            classification = "MalformedTfm"
        else:
            classification = "AcceptedByNativeLoader"

        supports_rules = sorted(
            supports_by_case[case_id], key=rule_ordinals.__getitem__
        )
        first_rejecting_rule = None
        if classification == "InvalidEffectiveSize":
            first_rejecting_rule = "TFM-SIZE-001"
        elif classification == "MalformedTfm":
            first_rejecting_rule = supports_rules[0]

        requested_size = CORPUS_CASE_SIZES[case_id]
        if requested_size["mode"] == "at_sp":
            requested_value = requested_size["value"]
            validator_input_size_sp = requested_value
            resolved_effective_size_sp = (
                655_360
                if classification == "InvalidEffectiveSize"
                else requested_value
            )
        else:
            validator_input_size_sp = 655_360
            resolved_effective_size_sp = None
            source_rejects_before_size = (
                first_rejecting_rule is not None
                and rule_ordinals[first_rejecting_rule]
                <= rule_ordinals["TFM-HEADER-002"]
            )
            if not source_rejects_before_size:
                raw = case_inputs[case_id]
                design_size_fix_word = int.from_bytes(raw[28:32], "big", signed=True)
                resolved_effective_size_sp = design_size_fix_word // 16
                validator_input_size_sp = resolved_effective_size_sp

        cases.append(
            {
                "blob_sha256": hashlib.sha256(case_inputs[case_id]).hexdigest(),
                "expected_classification": classification,
                "first_rejecting_rule": first_rejecting_rule,
                "id": case_id,
                "requested_size": dict(requested_size),
                "resolved_effective_size_sp": resolved_effective_size_sp,
                "supports_rules": supports_rules,
                "validator_input_size_sp": validator_input_size_sp,
            }
        )

    return {
        "compatibility_source": TEX82_READ_FONT_INFO_SOURCE,
        "compatibility_target": fixture["compatibility_target"],
        "format": "latexd.tfm-validity-corpus",
        "normalization": {
            "AcceptedByNativeLoader": "native loader selected latexdprobe",
            "InvalidEffectiveSize": (
                "project precondition rejected the requested size before byte validation; "
                "native TeX recovered to 10pt"
            ),
            "MalformedTfm": "native loader selected nullfont after bad-TFM recovery",
            "unresolved_size": (
                "null when natural-size native loading rejected before a valid design "
                "size became effective; validator_input_size_sp uses the 10pt harness size"
            ),
        },
        "rule_contract": {
            "format": rule_contract["format"],
            "repository_path": str(RULE_CONTRACT.relative_to(REPOSITORY)),
            "sha256": hashlib.sha256(RULE_CONTRACT.read_bytes()).hexdigest(),
        },
        "schema_version": 2,
        "source_oracle": {
            "format": fixture["format"],
            "repository_path": str(EXPECTED_FIXTURE.relative_to(REPOSITORY)),
            "sha256": hashlib.sha256(EXPECTED_FIXTURE.read_bytes()).hexdigest(),
        },
        "cases": cases,
    }


def validate_corpus_manifest(
    manifest: dict[str, object], corpus_root: Path = CORPUS_ROOT
) -> list[str]:
    errors = []
    canonical_manifest = json.dumps(
        manifest,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode()
    if hashlib.sha256(canonical_manifest).hexdigest() != (
        REVIEWED_V2_CORPUS_MANIFEST_CANONICAL_SHA256
    ):
        errors.append(
            "reviewed v2 corpus manifest digest differs; create a version transition"
        )
    case_inputs = build_case_inputs()
    expected = build_corpus_manifest(case_inputs)
    if manifest != expected:
        errors.append("TFM validity corpus manifest differs from generated semantics")

    cases = manifest.get("cases")
    if not isinstance(cases, list):
        return errors + ["TFM validity corpus cases must be an array"]
    referenced_hashes = {
        case.get("blob_sha256")
        for case in cases
        if isinstance(case, dict) and isinstance(case.get("blob_sha256"), str)
    }
    blob_root = corpus_root / "blobs"
    actual_files = set(blob_root.glob("*.tfm")) if blob_root.is_dir() else set()
    actual_hashes = {path.stem for path in actual_files}
    if actual_hashes != referenced_hashes:
        errors.append("TFM validity corpus has missing or orphan blob files")
    for path in actual_files:
        if hashlib.sha256(path.read_bytes()).hexdigest() != path.stem:
            errors.append(f"TFM validity corpus blob hash mismatch: {path.name}")
    return errors


def load_corpus_case_inputs(
    corpus_root: Path = CORPUS_ROOT,
) -> dict[str, bytes]:
    manifest_path = corpus_root / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    errors = validate_corpus_manifest(manifest, corpus_root)
    if errors:
        raise ValueError("; ".join(errors))
    return {
        case["id"]: (corpus_root / "blobs" / f"{case['blob_sha256']}.tfm").read_bytes()
        for case in manifest["cases"]
    }


def parse_observations(output: str) -> dict[str, int | str]:
    observations: dict[str, int | str] = {}
    for name, raw_value in OBSERVATION_PATTERN.findall(output):
        if name in observations:
            raise ValueError(f"duplicate TFM validity observation: {name}")
        observations[name] = (
            int(raw_value) if re.fullmatch(r"[+-]?[0-9]+", raw_value) else raw_value
        )
    return observations


def parse_diagnostics(output: str) -> list[str]:
    lines = output.splitlines()
    diagnostics = []
    for index, line in enumerate(lines):
        if not line.startswith("! "):
            continue
        diagnostic = line[2:]
        if diagnostic.endswith(";") and index + 1 < len(lines):
            continuation = lines[index + 1]
            if continuation.startswith(" "):
                diagnostic = f"{diagnostic} {continuation.strip()}"
        diagnostics.append(diagnostic.removesuffix("."))
    return diagnostics


def _fixture_result(result: dict[str, object]) -> dict[str, object]:
    return {
        "diagnostics": result["diagnostics"],
        "exit_status": result["exit_status"],
        "mutated_tfm_sha256": result["mutated_tfm_sha256"],
        "observations": result["observations"],
        "source_sha256": result["source_sha256"],
    }


def _run_native_case(
    engine: str,
    case_id: str,
    mutated: bytes,
    size: dict[str, object],
    *,
    include_raw_output: bool,
) -> dict[str, object]:
    if size == {"mode": "natural"}:
        font_definition = r"\font\probe=latexdprobe"
    elif size.get("mode") == "at_sp" and isinstance(size.get("value"), int):
        font_definition = rf"\font\probe=latexdprobe at {size['value']}sp"
    else:
        raise ValueError(f"invalid TFM oracle size for {case_id}: {size!r}")
    source = PROBE_SOURCE.replace(r"\font\probe=latexdprobe", font_definition, 1)
    source_sha256 = hashlib.sha256(source.encode()).hexdigest()
    process_environment = os.environ.copy()
    process_environment.update({"LC_ALL": "C.UTF-8", "TZ": "UTC"})
    with tempfile.TemporaryDirectory(prefix="latexd-tfm-validity-oracle-") as temp:
        root = Path(temp)
        (root / "latexdprobe.tfm").write_bytes(mutated)
        (root / "probe.tex").write_text(source, encoding="utf-8")
        process_environment["TEXFONTS"] = f"{root}{os.pathsep}"
        completed = subprocess.run(
            [engine, "-ini", "-interaction=nonstopmode", "probe.tex"],
            cwd=root,
            env=process_environment,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
    result: dict[str, object] = {
        "diagnostics": parse_diagnostics(completed.stdout),
        "exit_status": completed.returncode,
        "mutated_tfm_sha256": hashlib.sha256(mutated).hexdigest(),
        "observations": parse_observations(completed.stdout),
        "size": dict(size),
        "source_sha256": source_sha256,
    }
    if include_raw_output:
        result["raw_output"] = completed.stdout
        result["source"] = source
    return result


def run_corpus_case(engine: str, case_id: str) -> dict[str, object]:
    if case_id not in CORPUS_CASE_SPECS:
        raise ValueError(f"unknown TFM validity corpus case: {case_id}")
    return _run_native_case(
        engine,
        case_id,
        load_corpus_case_inputs()[case_id],
        CORPUS_CASE_SIZES[case_id],
        include_raw_output=False,
    )


def _collect_case_results(
    engine: str, *, include_raw_output: bool
) -> dict[str, dict[str, object]]:
    results = {}
    case_inputs = load_corpus_case_inputs()
    for case_id in CASE_SPECS:
        results[case_id] = _run_native_case(
            engine,
            case_id,
            case_inputs[case_id],
            CASE_SIZES[case_id],
            include_raw_output=include_raw_output,
        )
    return results


def run_oracle(engine: str) -> dict[str, dict[str, object]]:
    return {
        case_id: _fixture_result(result)
        for case_id, result in _collect_case_results(
            engine, include_raw_output=False
        ).items()
    }


def validate_case_results(
    results: dict[str, dict[str, object]], fixture: dict[str, object]
) -> list[str]:
    expected = fixture["case_results"]
    if not isinstance(expected, dict):
        raise ValueError("TFM validity fixture case_results must be an object")
    expected_sizes = fixture.get("case_sizes")
    if not isinstance(expected_sizes, dict):
        raise ValueError("TFM validity fixture case_sizes must be an object")
    violations = []
    for case_id, expected_result in expected.items():
        actual = results.get(case_id)
        semantic_actual = None if actual is None else _fixture_result(actual)
        if semantic_actual != expected_result:
            violations.append(
                f"{case_id} mismatch: expected {expected_result!r}, "
                f"observed {semantic_actual!r}"
            )
        if actual is not None and "size" in actual:
            actual_size = actual["size"]
            if actual_size != expected_sizes.get(case_id):
                violations.append(
                    f"{case_id} size mismatch: expected {expected_sizes.get(case_id)!r}, "
                    f"observed {actual_size!r}"
                )
    unexpected = set(results).difference(expected)
    if unexpected:
        violations.append(f"unexpected cases: {sorted(unexpected)!r}")
    return violations


def _base_tfm_evidence() -> dict[str, dict[str, object]]:
    evidence = {}
    for relative_path in sorted({spec["base_tfm"] for spec in CASE_SPECS.values()}):
        path = (REPOSITORY / relative_path).resolve(strict=True)
        evidence[path.name] = {
            "repository_path": relative_path,
            "resolved_path": str(path),
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        }
    return evidence


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", default="pdftex")
    parser.add_argument(
        "--report",
        type=Path,
        default=Path("target/tfm-validity-oracle.json"),
    )
    args = parser.parse_args(argv)

    fixture = json.loads(EXPECTED_FIXTURE.read_text(encoding="utf-8"))
    case_results = _collect_case_results(args.engine, include_raw_output=True)
    violations = validate_case_results(case_results, fixture)
    if violations:
        print("TeX82 TFM validity oracle failed:")
        for violation in violations:
            print(f"- {violation}")
        return 1

    engine_path = shutil.which(args.engine)
    if engine_path is None:
        raise RuntimeError(f"TeX oracle engine not found: {args.engine}")
    engine_path = str(Path(engine_path).resolve())
    engine_version = subprocess.run(
        [engine_path, "--version"],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout.strip()
    report = {
        "format": fixture["format"],
        "schema_version": fixture["schema_version"],
        "compatibility_target": fixture["compatibility_target"],
        "compatibility_source": TEX82_READ_FONT_INFO_SOURCE,
        "engine": {
            "path": engine_path,
            "sha256": hashlib.sha256(Path(engine_path).read_bytes()).hexdigest(),
            "version": engine_version,
        },
        "invocation": [args.engine, "-ini", "-interaction=nonstopmode"],
        "environment": {"locale": "C.UTF-8", "timezone": "UTC"},
        "base_tfm_files": _base_tfm_evidence(),
        "expected_processes": len(CASE_SPECS),
        "observed_processes": len(case_results),
        "case_descriptions": {
            case_id: spec["description"] for case_id, spec in CASE_SPECS.items()
        },
        "case_sizes": CASE_SIZES,
        "normalization": {
            "diagnostics": (
                "lines beginning !; semicolon continuation joined; "
                "one trailing period removed"
            ),
            "observations": "LATEXD-TFMV:<name>=<integer-or-font-name>",
        },
        "case_results": case_results,
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"TeX82 TFM validity oracle passed ({args.engine} -ini); "
        f"report: {args.report}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
