//! Exact, owner-neutral TFM dimension metrics.

/// Exact dimension fields extracted from one exact declared TFM frame.
///
/// Success does not imply that TeX82 or pdfTeX would load the font. This module
/// checks only the aggregate layout and fields required for exact design-size,
/// x-height, and quad projection. It deliberately does not validate character
/// info, metric tables unrelated to those fields, lig/kern programs, character
/// lists, or extensible recipes.
///
/// Broad parsing aliases deliberately do not exist:
///
/// ```compile_fail
/// use tex_tfm_metrics::parse_tfm;
/// ```
///
/// ```compile_fail
/// use tex_tfm_metrics::TfmParseError;
/// ```
pub mod dimension_subset {

    use std::{
        error::Error,
        fmt::{self, Write as _},
    };

    use sha2::{Digest, Sha256};

    const TFM_PREAMBLE_BYTES: usize = 24;
    const MIN_DESIGN_SIZE_FIX_WORD: i32 = 1 << 20;
    const MAX_METRIC_FIX_WORD: i32 = 16 << 20;
    const MAX_TEX_FONT_SIZE_SP: i32 = 1 << 27;

    /// Exact semantic dimensions and identity extracted from one exact TFM frame.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ExactTfmDimensionMetrics {
        design_size_sp: i32,
        x_height_fix_word: i32,
        quad_fix_word: i32,
        exact_frame_content_hash: String,
    }

    /// Font dimensions scaled to a selected TeX font size.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ExactTfmDimensions {
        /// `fontdimen6`, in scaled points.
        pub quad_sp: i32,
        /// `fontdimen5`, in scaled points.
        pub x_height_sp: i32,
    }

    /// A selected exact dimension field whose fix word is invalid.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SelectedDimension {
        XHeight,
        Quad,
    }

    /// A failure to extract the dimension subset from an exact TFM frame.
    ///
    /// These errors describe this extractor's bounded contract. They are not
    /// complete TeX font-load validity results.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ExtractError {
        TooShort,
        ExactFrameLengthMismatch {
            declared_bytes: usize,
            actual_bytes: usize,
        },
        InvalidCharacterRange {
            first: usize,
            last: usize,
        },
        InconsistentAggregateTableLength {
            declared_words: usize,
            computed_words: usize,
        },
        MissingDesignSize,
        InvalidDesignSize,
        InvalidSelectedDimension {
            dimension: SelectedDimension,
        },
        ArithmeticOverflow,
    }

    impl fmt::Display for ExtractError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::TooShort => formatter.write_str("TFM data is shorter than its preamble"),
                Self::ExactFrameLengthMismatch {
                    declared_bytes,
                    actual_bytes,
                } => write!(
                    formatter,
                    "TFM declares {declared_bytes} bytes but contains {actual_bytes}"
                ),
                Self::InvalidCharacterRange { first, last } => {
                    write!(formatter, "invalid TFM character range {first}..={last}")
                }
                Self::InconsistentAggregateTableLength {
                    declared_words,
                    computed_words,
                } => write!(
                    formatter,
                    "TFM declares {declared_words} words but its tables require {computed_words}"
                ),
                Self::MissingDesignSize => formatter.write_str("TFM header omits the design size"),
                Self::InvalidDesignSize => formatter.write_str("TFM design size is below 1pt"),
                Self::InvalidSelectedDimension { dimension } => {
                    write!(
                        formatter,
                        "selected TFM dimension {dimension:?} is outside fix-word range"
                    )
                }
                Self::ArithmeticOverflow => {
                    formatter.write_str("TFM table arithmetic overflowed the host size")
                }
            }
        }
    }

    impl Error for ExtractError {}

    /// An invalid effective size or unrepresentable scaled dimension.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ScaleError {
        NonPositiveSize,
        Overflow,
    }

    impl fmt::Display for ScaleError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::NonPositiveSize => {
                    formatter.write_str("effective font size must be positive")
                }
                Self::Overflow => formatter.write_str("scaled TFM dimension does not fit in i32"),
            }
        }
    }

    impl Error for ScaleError {}

    impl ExactTfmDimensionMetrics {
        /// Returns the TFM design size in scaled points.
        pub const fn design_size_sp(&self) -> i32 {
            self.design_size_sp
        }

        /// Returns the lowercase SHA-256 identity of every byte in the exact frame.
        pub fn exact_frame_content_hash(&self) -> &str {
            &self.exact_frame_content_hash
        }

        /// Scales the semantic dimensions with TeX82's exact `store_scaled` arithmetic.
        pub fn at_size_sp(&self, effective_size_sp: i32) -> Result<ExactTfmDimensions, ScaleError> {
            if effective_size_sp <= 0 {
                return Err(ScaleError::NonPositiveSize);
            }
            if effective_size_sp >= MAX_TEX_FONT_SIZE_SP {
                return Err(ScaleError::Overflow);
            }
            Ok(ExactTfmDimensions {
                quad_sp: scale_fix_word(self.quad_fix_word, effective_size_sp)?,
                x_height_sp: scale_fix_word(self.x_height_fix_word, effective_size_sp)?,
            })
        }
    }

    /// Extracts exact design-size, x-height, quad, and identity from one exact frame.
    ///
    /// The supplied slice must contain exactly the bytes declared by the TFM `lf`
    /// field. Success guarantees only this module's documented dimension subset;
    /// it does not establish native font-load validity.
    pub fn extract_exact_frame(bytes: &[u8]) -> Result<ExactTfmDimensionMetrics, ExtractError> {
        if bytes.len() < TFM_PREAMBLE_BYTES {
            return Err(ExtractError::TooShort);
        }

        let counts = (0..12)
            .map(|index| read_u16(bytes, index * 2).map(usize::from))
            .collect::<Result<Vec<_>, _>>()?;
        let [lf, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, np] = counts.as_slice() else {
            unreachable!("the fixed TFM preamble always contains twelve halfwords")
        };
        let declared_bytes = lf.checked_mul(4).ok_or(ExtractError::ArithmeticOverflow)?;
        if declared_bytes != bytes.len() {
            return Err(ExtractError::ExactFrameLengthMismatch {
                declared_bytes,
                actual_bytes: bytes.len(),
            });
        }

        let character_count = if bc <= ec {
            ec.checked_sub(*bc)
                .and_then(|difference| difference.checked_add(1))
                .ok_or(ExtractError::ArithmeticOverflow)?
        } else if *bc == 1 && *ec == 0 {
            0
        } else {
            return Err(ExtractError::InvalidCharacterRange {
                first: *bc,
                last: *ec,
            });
        };
        let computed_words = [
            6,
            *lh,
            character_count,
            *nw,
            *nh,
            *nd,
            *ni,
            *nl,
            *nk,
            *ne,
            *np,
        ]
        .into_iter()
        .try_fold(0usize, |total, value| total.checked_add(value))
        .ok_or(ExtractError::ArithmeticOverflow)?;
        if computed_words != *lf {
            return Err(ExtractError::InconsistentAggregateTableLength {
                declared_words: *lf,
                computed_words,
            });
        }
        if *lh < 2 {
            return Err(ExtractError::MissingDesignSize);
        }
        let design_size_fix_word = read_i32(bytes, TFM_PREAMBLE_BYTES + 4)?;
        if design_size_fix_word < MIN_DESIGN_SIZE_FIX_WORD {
            return Err(ExtractError::InvalidDesignSize);
        }
        let design_size_sp = design_size_fix_word / 16;

        let parameter_start_words = [6, *lh, character_count, *nw, *nh, *nd, *ni, *nl, *nk, *ne]
            .into_iter()
            .try_fold(0usize, |total, value| total.checked_add(value))
            .ok_or(ExtractError::ArithmeticOverflow)?;
        let parameter_start = parameter_start_words
            .checked_mul(4)
            .ok_or(ExtractError::ArithmeticOverflow)?;
        let x_height_fix_word = if *np >= 5 {
            read_i32(bytes, parameter_start + 4 * 4)?
        } else {
            0
        };
        let quad_fix_word = if *np >= 6 {
            read_i32(bytes, parameter_start + 5 * 4)?
        } else {
            0
        };
        for (dimension, value) in [
            (SelectedDimension::XHeight, x_height_fix_word),
            (SelectedDimension::Quad, quad_fix_word),
        ] {
            if !(-MAX_METRIC_FIX_WORD..MAX_METRIC_FIX_WORD).contains(&value) {
                return Err(ExtractError::InvalidSelectedDimension { dimension });
            }
        }

        let mut exact_frame_content_hash = String::with_capacity("sha256:".len() + 64);
        exact_frame_content_hash.push_str("sha256:");
        for byte in Sha256::digest(bytes) {
            write!(exact_frame_content_hash, "{byte:02x}")
                .expect("writing to a String cannot fail");
        }

        Ok(ExactTfmDimensionMetrics {
            design_size_sp,
            x_height_fix_word,
            quad_fix_word,
            exact_frame_content_hash,
        })
    }

    fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ExtractError> {
        let end = offset
            .checked_add(2)
            .ok_or(ExtractError::ArithmeticOverflow)?;
        let word = bytes.get(offset..end).ok_or(ExtractError::TooShort)?;
        Ok(u16::from_be_bytes([word[0], word[1]]))
    }

    fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, ExtractError> {
        let end = offset
            .checked_add(4)
            .ok_or(ExtractError::ArithmeticOverflow)?;
        let word = bytes.get(offset..end).ok_or(ExtractError::TooShort)?;
        Ok(i32::from_be_bytes([word[0], word[1], word[2], word[3]]))
    }

    fn scale_fix_word(fix_word: i32, effective_size_sp: i32) -> Result<i32, ScaleError> {
        let mut reduced_size = i64::from(effective_size_sp);
        let mut alpha = 16i64;
        while reduced_size >= 1 << 23 {
            reduced_size /= 2;
            alpha *= 2;
        }
        let beta = 256 / alpha;

        let bytes = fix_word.to_be_bytes();
        let b = i64::from(bytes[1]);
        let c = i64::from(bytes[2]);
        let d = i64::from(bytes[3]);
        let positive_fraction =
            (((d * reduced_size / 256) + c * reduced_size) / 256 + b * reduced_size) / beta;
        let scaled = if bytes[0] == 0 {
            positive_fraction
        } else {
            debug_assert_eq!(bytes[0], u8::MAX);
            positive_fraction - alpha * reduced_size
        };
        i32::try_from(scaled).map_err(|_| ScaleError::Overflow)
    }
}
