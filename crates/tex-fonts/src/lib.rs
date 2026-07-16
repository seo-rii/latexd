use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use tex_render_model::{FontFamilyRequest, FontRequest, FontSeries, FontShape};

const MAX_FONT_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TexFontFace {
    Roman10,
    Roman7,
    MathItalic10,
    MathItalic7,
    MathExtension10,
}

impl TexFontFace {
    pub fn stem(self) -> &'static str {
        match self {
            Self::Roman10 => "cmr10",
            Self::Roman7 => "cmr7",
            Self::MathItalic10 => "cmmi10",
            Self::MathItalic7 => "cmmi7",
            Self::MathExtension10 => "cmex10",
        }
    }

    pub fn postscript_name(self) -> &'static str {
        match self {
            Self::Roman10 => "CMR10",
            Self::Roman7 => "CMR7",
            Self::MathItalic10 => "CMMI10",
            Self::MathItalic7 => "CMMI7",
            Self::MathExtension10 => "CMEX10",
        }
    }
}

#[derive(Debug)]
pub struct ResolvedTexFont {
    pub face: TexFontFace,
    pub metrics: TfmMetrics,
    pub type1: Type1Program,
}

#[derive(Debug)]
pub struct Type1Program {
    pub bytes: Vec<u8>,
    pub length1: usize,
    pub length2: usize,
    pub length3: usize,
}

#[derive(Debug)]
pub struct TfmMetrics {
    bc: u8,
    ec: u8,
    widths: Vec<f32>,
    char_width_indices: Vec<u8>,
    char_remainders: Vec<u8>,
    char_tags: Vec<u8>,
    lig_kern: Vec<[u8; 4]>,
    kerns: Vec<f32>,
    space_em: f32,
}

impl TfmMetrics {
    pub fn advance_em(&self, text: &str) -> Option<f32> {
        if !text.is_ascii() {
            return None;
        }
        self.advance_bytes(text.as_bytes())
    }

    pub fn advance_bytes(&self, bytes: &[u8]) -> Option<f32> {
        let mut advance = 0.0;
        for (index, byte) in bytes.iter().copied().enumerate() {
            advance += if byte == b' ' {
                self.space_em
            } else {
                self.width_em(byte)?
            };
            if let Some(next) = bytes.get(index + 1).copied() {
                advance += self.kern_em(byte, next).unwrap_or(0.0);
            }
        }
        Some(advance)
    }

    pub fn width_em(&self, code: u8) -> Option<f32> {
        if code < self.bc || code > self.ec {
            return None;
        }
        let index = self.char_width_indices[(code - self.bc) as usize] as usize;
        self.widths.get(index).copied()
    }

    pub fn kern_em(&self, left: u8, right: u8) -> Option<f32> {
        if left < self.bc || left > self.ec {
            return None;
        }
        let char_index = (left - self.bc) as usize;
        if self.char_tags.get(char_index).copied()? != 1 {
            return None;
        }
        let mut instruction_index = self.char_remainders[char_index] as usize;
        loop {
            let instruction = *self.lig_kern.get(instruction_index)?;
            if instruction[1] == right && instruction[2] >= 128 {
                let kern_index = ((instruction[2] as usize - 128) << 8) | instruction[3] as usize;
                return self.kerns.get(kern_index).copied();
            }
            if instruction[0] >= 128 {
                return None;
            }
            instruction_index += instruction[0] as usize + 1;
        }
    }

    pub fn pdf_widths(&self) -> Vec<f32> {
        (self.bc..=self.ec)
            .map(|code| self.width_em(code).unwrap_or(0.0) * 1000.0)
            .collect()
    }

    pub fn first_char(&self) -> u8 {
        self.bc
    }

    pub fn last_char(&self) -> u8 {
        self.ec
    }
}

pub fn encode_text(face: TexFontFace, text: &str) -> Option<Vec<u8>> {
    text.chars()
        .map(|ch| {
            if ch.is_whitespace() {
                return Some(b' ');
            }
            if face == TexFontFace::MathExtension10 {
                match ch {
                    '∑' => Some(88),
                    '∏' => Some(89),
                    '∫' => Some(90),
                    _ if ch.is_ascii() => Some(ch as u8),
                    _ => None,
                }
            } else {
                ch.is_ascii().then_some(ch as u8)
            }
        })
        .collect()
}

pub fn text_advance_em(face: TexFontFace, text: &str) -> Option<f32> {
    let font = resolve_font(face)?;
    font.metrics.advance_bytes(&encode_text(face, text)?)
}

pub fn face_for_request(request: &FontRequest, size_pt: f32) -> Option<TexFontFace> {
    match (&request.family, request.series, request.shape) {
        (FontFamilyRequest::Serif, FontSeries::Regular, FontShape::Upright) => {
            Some(TexFontFace::Roman10)
        }
        (FontFamilyRequest::Math, FontSeries::Regular, FontShape::Italic) if size_pt < 8.5 => {
            Some(TexFontFace::MathItalic7)
        }
        (FontFamilyRequest::Math, FontSeries::Regular, FontShape::Italic) => {
            Some(TexFontFace::MathItalic10)
        }
        (FontFamilyRequest::Math, FontSeries::Regular, FontShape::Upright) if size_pt < 8.5 => {
            Some(TexFontFace::Roman7)
        }
        (FontFamilyRequest::Math, FontSeries::Regular, FontShape::Upright) => {
            Some(TexFontFace::Roman10)
        }
        (FontFamilyRequest::Symbol, _, _) => Some(TexFontFace::MathExtension10),
        _ => None,
    }
}

pub fn resolve_font(face: TexFontFace) -> Option<&'static ResolvedTexFont> {
    static ROMAN_10: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static ROMAN_7: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static MATH_ITALIC_10: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static MATH_ITALIC_7: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static MATH_EXTENSION_10: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    let slot = match face {
        TexFontFace::Roman10 => &ROMAN_10,
        TexFontFace::Roman7 => &ROMAN_7,
        TexFontFace::MathItalic10 => &MATH_ITALIC_10,
        TexFontFace::MathItalic7 => &MATH_ITALIC_7,
        TexFontFace::MathExtension10 => &MATH_EXTENSION_10,
    };
    slot.get_or_init(|| load_font(face)).as_ref()
}

fn load_font(face: TexFontFace) -> Option<ResolvedTexFont> {
    let tfm = read_kpse_file(&format!("{}.tfm", face.stem()))?;
    let pfb = read_kpse_file(&format!("{}.pfb", face.stem()))?;
    Some(ResolvedTexFont {
        face,
        metrics: parse_tfm(&tfm)?,
        type1: parse_pfb(&pfb)?,
    })
}

fn read_kpse_file(name: &str) -> Option<Vec<u8>> {
    let output = Command::new("kpsewhich").arg(name).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let path = PathBuf::from(path.trim());
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FONT_FILE_BYTES {
        return None;
    }
    fs::read(path).ok()
}

fn parse_tfm(bytes: &[u8]) -> Option<TfmMetrics> {
    if bytes.len() < 24 {
        return None;
    }
    let half = |index: usize| -> Option<usize> {
        let offset = index.checked_mul(2)?;
        Some(u16::from_be_bytes([*bytes.get(offset)?, *bytes.get(offset + 1)?]) as usize)
    };
    let lf = half(0)?;
    let lh = half(1)?;
    let bc = half(2)?;
    let ec = half(3)?;
    let nw = half(4)?;
    let nh = half(5)?;
    let nd = half(6)?;
    let ni = half(7)?;
    let nl = half(8)?;
    let nk = half(9)?;
    let ne = half(10)?;
    let np = half(11)?;
    if lf.checked_mul(4)? != bytes.len() || bc > ec || ec > u8::MAX as usize {
        return None;
    }
    let char_count = ec - bc + 1;
    let char_start = 24usize.checked_add(lh.checked_mul(4)?)?;
    let width_start = char_start.checked_add(char_count.checked_mul(4)?)?;
    let height_start = width_start.checked_add(nw.checked_mul(4)?)?;
    let depth_start = height_start.checked_add(nh.checked_mul(4)?)?;
    let italic_start = depth_start.checked_add(nd.checked_mul(4)?)?;
    let lig_start = italic_start.checked_add(ni.checked_mul(4)?)?;
    let kern_start = lig_start.checked_add(nl.checked_mul(4)?)?;
    let extensible_start = kern_start.checked_add(nk.checked_mul(4)?)?;
    let parameter_start = extensible_start.checked_add(ne.checked_mul(4)?)?;
    let fixed = |offset: usize| -> Option<f32> {
        let value = i32::from_be_bytes([
            *bytes.get(offset)?,
            *bytes.get(offset + 1)?,
            *bytes.get(offset + 2)?,
            *bytes.get(offset + 3)?,
        ]);
        Some(value as f32 / 1_048_576.0)
    };
    let mut char_width_indices = Vec::with_capacity(char_count);
    let mut char_remainders = Vec::with_capacity(char_count);
    let mut char_tags = Vec::with_capacity(char_count);
    for index in 0..char_count {
        let offset = char_start + index * 4;
        char_width_indices.push(*bytes.get(offset)?);
        char_tags.push(*bytes.get(offset + 2)? & 0x03);
        char_remainders.push(*bytes.get(offset + 3)?);
    }
    let widths = (0..nw)
        .map(|index| fixed(width_start + index * 4))
        .collect::<Option<Vec<_>>>()?;
    let lig_kern = (0..nl)
        .map(|index| {
            let offset = lig_start + index * 4;
            Some([
                *bytes.get(offset)?,
                *bytes.get(offset + 1)?,
                *bytes.get(offset + 2)?,
                *bytes.get(offset + 3)?,
            ])
        })
        .collect::<Option<Vec<_>>>()?;
    let kerns = (0..nk)
        .map(|index| fixed(kern_start + index * 4))
        .collect::<Option<Vec<_>>>()?;
    let space_em = if np >= 2 {
        fixed(parameter_start + 4)?
    } else {
        0.0
    };
    Some(TfmMetrics {
        bc: bc as u8,
        ec: ec as u8,
        widths,
        char_width_indices,
        char_remainders,
        char_tags,
        lig_kern,
        kerns,
        space_em,
    })
}

fn parse_pfb(bytes: &[u8]) -> Option<Type1Program> {
    let mut offset = 0usize;
    let mut program = Vec::new();
    let mut lengths = [0usize; 3];
    let mut segment = 0usize;
    while offset < bytes.len() {
        if *bytes.get(offset)? != 0x80 {
            return None;
        }
        let kind = *bytes.get(offset + 1)?;
        offset += 2;
        if kind == 0x03 {
            break;
        }
        if !matches!(kind, 0x01 | 0x02) || segment >= lengths.len() {
            return None;
        }
        let length = u32::from_le_bytes([
            *bytes.get(offset)?,
            *bytes.get(offset + 1)?,
            *bytes.get(offset + 2)?,
            *bytes.get(offset + 3)?,
        ]) as usize;
        offset += 4;
        let end = offset.checked_add(length)?;
        program.extend_from_slice(bytes.get(offset..end)?);
        lengths[segment] = length;
        segment += 1;
        offset = end;
    }
    (segment >= 2).then_some(Type1Program {
        bytes: program,
        length1: lengths[0],
        length2: lengths[1],
        length3: lengths[2],
    })
}

#[cfg(test)]
mod tests {
    use super::{TexFontFace, encode_text, resolve_font};

    #[test]
    fn text_encoding_normalizes_all_whitespace_to_the_tex_space_slot() {
        assert_eq!(
            encode_text(TexFontFace::Roman10, "a\n\tb\u{a0}c"),
            Some(b"a  b c".to_vec())
        );
    }

    #[test]
    fn installed_computer_modern_metrics_include_tex_space_and_kern() {
        let Some(font) = resolve_font(TexFontFace::Roman10) else {
            return;
        };
        assert!((font.metrics.width_em(b'T').unwrap() - 0.722_222).abs() < 0.000_01);
        assert!((font.metrics.kern_em(b'o', b'w').unwrap() + 0.027_779).abs() < 0.000_01);
        assert!(font.metrics.advance_em("The following").unwrap() > 5.0);
        assert!(font.type1.length1 > 0);
        assert!(font.type1.length2 > 0);
    }
}
