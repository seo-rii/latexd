mod bundled;

use std::borrow::Cow;
use std::collections::BTreeMap;
#[cfg(not(target_family = "wasm"))]
use std::fs;
#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;
#[cfg(not(target_family = "wasm"))]
use std::process::Command;
use std::sync::OnceLock;

use hayro_font::{Matrix, OutlineBuilder};
use tex_render_model::{
    BrowserFontAsset, DrawOp, FontFaceId, FontFamilyRequest, FontRequest, FontSeries, FontShape,
    GlyphIdKind, GlyphOutline, GlyphOutlineCommand, PageDisplayList, Point, PositionedGlyph,
    ResolvedFontRef, TextCluster,
};

pub use bundled::BundledFontResolver;

#[cfg(not(target_family = "wasm"))]
const MAX_FONT_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_BROWSER_OUTLINE_COMMANDS_PER_GLYPH: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TexFontFace {
    Roman10,
    Roman7,
    Roman5,
    MathItalic10,
    MathItalic7,
    MathItalic5,
    MathSymbol10,
    MathSymbol7,
    MathSymbol5,
    MathExtension10,
    TimesRoman,
    TimesBold,
    TimesItalic,
    TimesBoldItalic,
}

impl TexFontFace {
    pub fn stem(self) -> &'static str {
        match self {
            Self::Roman10 => "cmr10",
            Self::Roman7 => "cmr7",
            Self::Roman5 => "cmr5",
            Self::MathItalic10 => "cmmi10",
            Self::MathItalic7 => "cmmi7",
            Self::MathItalic5 => "cmmi5",
            Self::MathSymbol10 => "cmsy10",
            Self::MathSymbol7 => "cmsy7",
            Self::MathSymbol5 => "cmsy5",
            Self::MathExtension10 => "cmex10",
            Self::TimesRoman => "ptmr8r",
            Self::TimesBold => "ptmb8r",
            Self::TimesItalic => "ptmri8r",
            Self::TimesBoldItalic => "ptmbi8r",
        }
    }

    fn type1_stem(self) -> &'static str {
        match self {
            Self::Roman10
            | Self::Roman7
            | Self::Roman5
            | Self::MathItalic10
            | Self::MathItalic7
            | Self::MathItalic5
            | Self::MathSymbol10
            | Self::MathSymbol7
            | Self::MathSymbol5
            | Self::MathExtension10 => self.stem(),
            Self::TimesRoman => "utmr8a",
            Self::TimesBold => "utmb8a",
            Self::TimesItalic => "utmri8a",
            Self::TimesBoldItalic => "utmbi8a",
        }
    }

    pub fn postscript_name(self) -> &'static str {
        match self {
            Self::Roman10 => "CMR10",
            Self::Roman7 => "CMR7",
            Self::Roman5 => "CMR5",
            Self::MathItalic10 => "CMMI10",
            Self::MathItalic7 => "CMMI7",
            Self::MathItalic5 => "CMMI5",
            Self::MathSymbol10 => "CMSY10",
            Self::MathSymbol7 => "CMSY7",
            Self::MathSymbol5 => "CMSY5",
            Self::MathExtension10 => "CMEX10",
            Self::TimesRoman => "NimbusRomNo9L-Regu",
            Self::TimesBold => "NimbusRomNo9L-Medi",
            Self::TimesItalic => "NimbusRomNo9L-ReguItal",
            Self::TimesBoldItalic => "NimbusRomNo9L-MediItal",
        }
    }

    pub fn face_id(self) -> FontFaceId {
        FontFaceId::new(self.stem())
    }

    fn has_bundled_outline(self) -> bool {
        BUNDLED_TEX_FONT_FACES.contains(&self)
    }
}

pub const BUNDLED_TEX_FONT_FACES: [TexFontFace; 10] = [
    TexFontFace::Roman10,
    TexFontFace::Roman7,
    TexFontFace::Roman5,
    TexFontFace::MathItalic10,
    TexFontFace::MathItalic7,
    TexFontFace::MathItalic5,
    TexFontFace::MathSymbol10,
    TexFontFace::MathSymbol7,
    TexFontFace::MathSymbol5,
    TexFontFace::MathExtension10,
];

#[derive(Debug, Clone, PartialEq)]
pub struct ShapedText {
    pub resolved_font: ResolvedFontRef,
    pub glyphs: Vec<PositionedGlyph>,
    pub clusters: Vec<TextCluster>,
    pub advance_pt: f32,
}

#[derive(Debug)]
pub struct ResolvedTexFont {
    pub face: TexFontFace,
    pub content_hash: String,
    pub metrics: TfmMetrics,
    pub type1: Type1Program,
}

#[derive(Debug, Clone)]
pub struct FontData {
    bytes: Cow<'static, [u8]>,
    content_hash: String,
}

impl FontData {
    pub fn borrowed(bytes: &'static [u8]) -> Self {
        Self::new(Cow::Borrowed(bytes))
    }

    pub fn owned(bytes: Vec<u8>) -> Self {
        Self::new(Cow::Owned(bytes))
    }

    fn new(bytes: Cow<'static, [u8]>) -> Self {
        let content_hash = format!("blake3:{}", blake3::hash(&bytes).to_hex());
        Self {
            bytes,
            content_hash,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
}

pub trait FontResolver {
    fn resolve_tfm(&self, stem: &str) -> Option<FontData>;
    fn resolve_type1(&self, stem: &str) -> Option<FontData>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct KpathseaFontResolver;

impl FontResolver for KpathseaFontResolver {
    #[cfg(not(target_family = "wasm"))]
    fn resolve_tfm(&self, stem: &str) -> Option<FontData> {
        read_kpse_file(stem, "tfm").map(FontData::owned)
    }

    #[cfg(target_family = "wasm")]
    fn resolve_tfm(&self, _stem: &str) -> Option<FontData> {
        None
    }

    #[cfg(not(target_family = "wasm"))]
    fn resolve_type1(&self, stem: &str) -> Option<FontData> {
        read_kpse_file(stem, "pfb").map(FontData::owned)
    }

    #[cfg(target_family = "wasm")]
    fn resolve_type1(&self, _stem: &str) -> Option<FontData> {
        None
    }
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

pub fn shape_text(face: TexFontFace, text: &str, size_pt: f32) -> Option<ShapedText> {
    if !size_pt.is_finite() || size_pt <= 0.0 {
        return None;
    }
    let font = resolve_font(face)?;
    let encoded = encode_text(face, text)?;
    if encoded.len() != text.chars().count() {
        return None;
    }

    let mut glyphs = Vec::with_capacity(encoded.len());
    let mut clusters = Vec::with_capacity(encoded.len());
    let mut pen_x = 0.0;
    for (glyph_index, ((text_start, character), code)) in
        text.char_indices().zip(encoded.iter().copied()).enumerate()
    {
        let width_em = if code == b' ' {
            font.metrics.space_em
        } else {
            font.metrics.width_em(code)?
        };
        let kern_em = encoded
            .get(glyph_index + 1)
            .and_then(|next| font.metrics.kern_em(code, *next))
            .unwrap_or(0.0);
        let advance_pt = (width_em + kern_em) * size_pt;
        glyphs.push(PositionedGlyph {
            glyph_id: u32::from(code),
            advance_pt,
            offset: Point { x: pen_x, y: 0.0 },
        });
        clusters.push(TextCluster {
            text_start_utf8: text_start as u32,
            text_end_utf8: (text_start + character.len_utf8()) as u32,
            glyph_start: glyph_index as u32,
            glyph_end: glyph_index as u32 + 1,
        });
        pen_x += advance_pt;
    }

    Some(ShapedText {
        resolved_font: ResolvedFontRef {
            face_id: face.face_id(),
            postscript_name: face.postscript_name().to_string(),
            glyph_id_kind: GlyphIdKind::Type1CharCode,
            content_hash: font.content_hash.clone(),
        },
        glyphs,
        clusters,
        advance_pt: pen_x,
    })
}

pub fn face_for_request(request: &FontRequest, size_pt: f32) -> Option<TexFontFace> {
    match (&request.family, request.series, request.shape) {
        (FontFamilyRequest::Named(name), FontSeries::Regular, FontShape::Upright)
            if name.eq_ignore_ascii_case("times") =>
        {
            Some(TexFontFace::TimesRoman)
        }
        (FontFamilyRequest::Named(name), FontSeries::Bold, FontShape::Upright)
            if name.eq_ignore_ascii_case("times") =>
        {
            Some(TexFontFace::TimesBold)
        }
        (FontFamilyRequest::Named(name), FontSeries::Regular, FontShape::Italic)
            if name.eq_ignore_ascii_case("times") =>
        {
            Some(TexFontFace::TimesItalic)
        }
        (FontFamilyRequest::Named(name), FontSeries::Bold, FontShape::Italic)
            if name.eq_ignore_ascii_case("times") =>
        {
            Some(TexFontFace::TimesBoldItalic)
        }
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
        (FontFamilyRequest::MathExtension, _, _) => Some(TexFontFace::MathExtension10),
        (FontFamilyRequest::Symbol, _, _) => None,
        _ => None,
    }
}

pub fn resolve_font(face: TexFontFace) -> Option<&'static ResolvedTexFont> {
    static ROMAN_10: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static ROMAN_7: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static ROMAN_5: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static MATH_ITALIC_10: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static MATH_ITALIC_7: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static MATH_ITALIC_5: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static MATH_SYMBOL_10: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static MATH_SYMBOL_7: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static MATH_SYMBOL_5: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static MATH_EXTENSION_10: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static TIMES_ROMAN: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static TIMES_BOLD: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static TIMES_ITALIC: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    static TIMES_BOLD_ITALIC: OnceLock<Option<ResolvedTexFont>> = OnceLock::new();
    let slot = match face {
        TexFontFace::Roman10 => &ROMAN_10,
        TexFontFace::Roman7 => &ROMAN_7,
        TexFontFace::Roman5 => &ROMAN_5,
        TexFontFace::MathItalic10 => &MATH_ITALIC_10,
        TexFontFace::MathItalic7 => &MATH_ITALIC_7,
        TexFontFace::MathItalic5 => &MATH_ITALIC_5,
        TexFontFace::MathSymbol10 => &MATH_SYMBOL_10,
        TexFontFace::MathSymbol7 => &MATH_SYMBOL_7,
        TexFontFace::MathSymbol5 => &MATH_SYMBOL_5,
        TexFontFace::MathExtension10 => &MATH_EXTENSION_10,
        TexFontFace::TimesRoman => &TIMES_ROMAN,
        TexFontFace::TimesBold => &TIMES_BOLD,
        TexFontFace::TimesItalic => &TIMES_ITALIC,
        TexFontFace::TimesBoldItalic => &TIMES_BOLD_ITALIC,
    };
    slot.get_or_init(|| {
        resolve_font_with(face, &BundledFontResolver)
            .or_else(|| resolve_font_with(face, &KpathseaFontResolver))
    })
    .as_ref()
}

pub fn resolve_font_with(
    face: TexFontFace,
    resolver: &dyn FontResolver,
) -> Option<ResolvedTexFont> {
    let tfm = resolver.resolve_tfm(face.stem())?;
    let pfb = resolver.resolve_type1(face.type1_stem())?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"tex-fonts-resolved-v1\0");
    hasher.update(face.stem().as_bytes());
    hasher.update(b"\0");
    hasher.update(tfm.content_hash().as_bytes());
    hasher.update(b"\0");
    hasher.update(pfb.content_hash().as_bytes());
    Some(ResolvedTexFont {
        face,
        content_hash: format!("blake3:{}", hasher.finalize().to_hex()),
        metrics: parse_tfm(tfm.as_bytes())?,
        type1: parse_pfb(pfb.as_bytes())?,
    })
}

struct NormalizedOutlineBuilder {
    matrix: Matrix,
    commands: Vec<GlyphOutlineCommand>,
    overflowed: bool,
}

impl NormalizedOutlineBuilder {
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.matrix.sx * x + self.matrix.kx * y + self.matrix.tx,
            self.matrix.ky * x + self.matrix.sy * y + self.matrix.ty,
        )
    }

    fn push(&mut self, command: GlyphOutlineCommand) {
        if self.commands.len() == MAX_BROWSER_OUTLINE_COMMANDS_PER_GLYPH {
            self.overflowed = true;
            return;
        }
        self.commands.push(command);
    }
}

impl OutlineBuilder for NormalizedOutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.push(GlyphOutlineCommand::MoveTo { x, y });
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.push(GlyphOutlineCommand::LineTo { x, y });
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x, y) = self.point(x, y);
        self.push(GlyphOutlineCommand::QuadTo { x1, y1, x, y });
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x2, y2) = self.point(x2, y2);
        let (x, y) = self.point(x, y);
        self.push(GlyphOutlineCommand::CurveTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        });
    }

    fn close(&mut self) {
        self.push(GlyphOutlineCommand::Close);
    }
}

pub fn outline_glyph(face: TexFontFace, glyph_id: u32) -> Option<GlyphOutline> {
    let outline_table = bundled_outline_table(face)?;
    let code = u8::try_from(glyph_id).ok()?;
    let glyph_name = outline_table.code_to_string(code)?;
    let mut builder = NormalizedOutlineBuilder {
        matrix: outline_table.matrix(),
        commands: Vec::new(),
        overflowed: false,
    };
    outline_table.outline(glyph_name, &mut builder)?;
    (!builder.overflowed && builder.commands.iter().all(GlyphOutlineCommand::is_finite)).then_some(
        GlyphOutline {
            glyph_id,
            commands: builder.commands,
        },
    )
}

fn bundled_outline_table(face: TexFontFace) -> Option<&'static hayro_font::type1::Table> {
    static ROMAN_10: OnceLock<Option<hayro_font::type1::Table>> = OnceLock::new();
    static ROMAN_7: OnceLock<Option<hayro_font::type1::Table>> = OnceLock::new();
    static ROMAN_5: OnceLock<Option<hayro_font::type1::Table>> = OnceLock::new();
    static MATH_ITALIC_10: OnceLock<Option<hayro_font::type1::Table>> = OnceLock::new();
    static MATH_ITALIC_7: OnceLock<Option<hayro_font::type1::Table>> = OnceLock::new();
    static MATH_ITALIC_5: OnceLock<Option<hayro_font::type1::Table>> = OnceLock::new();
    static MATH_SYMBOL_10: OnceLock<Option<hayro_font::type1::Table>> = OnceLock::new();
    static MATH_SYMBOL_7: OnceLock<Option<hayro_font::type1::Table>> = OnceLock::new();
    static MATH_SYMBOL_5: OnceLock<Option<hayro_font::type1::Table>> = OnceLock::new();
    static MATH_EXTENSION_10: OnceLock<Option<hayro_font::type1::Table>> = OnceLock::new();
    let slot = match face {
        TexFontFace::Roman10 => &ROMAN_10,
        TexFontFace::Roman7 => &ROMAN_7,
        TexFontFace::Roman5 => &ROMAN_5,
        TexFontFace::MathItalic10 => &MATH_ITALIC_10,
        TexFontFace::MathItalic7 => &MATH_ITALIC_7,
        TexFontFace::MathItalic5 => &MATH_ITALIC_5,
        TexFontFace::MathSymbol10 => &MATH_SYMBOL_10,
        TexFontFace::MathSymbol7 => &MATH_SYMBOL_7,
        TexFontFace::MathSymbol5 => &MATH_SYMBOL_5,
        TexFontFace::MathExtension10 => &MATH_EXTENSION_10,
        TexFontFace::TimesRoman
        | TexFontFace::TimesBold
        | TexFontFace::TimesItalic
        | TexFontFace::TimesBoldItalic => return None,
    };
    slot.get_or_init(|| {
        let pfb = BundledFontResolver.resolve_type1(face.type1_stem())?;
        hayro_font::type1::Table::parse(pfb.as_bytes())
    })
    .as_ref()
}

pub fn browser_font_assets_for_pages(pages: &[PageDisplayList]) -> Vec<BrowserFontAsset> {
    let mut used = BTreeMap::<(FontFaceId, String), (ResolvedFontRef, BTreeMap<u32, bool>)>::new();
    for run in pages
        .iter()
        .flat_map(|page| &page.ops)
        .filter_map(|op| match op {
            DrawOp::TextRun(run) => Some(run),
            _ => None,
        })
    {
        let (Some(resolved), Some(glyphs)) = (&run.resolved_font, &run.glyphs) else {
            continue;
        };
        if resolved.glyph_id_kind != GlyphIdKind::Type1CharCode {
            continue;
        }
        let entry = used
            .entry((resolved.face_id.clone(), resolved.content_hash.clone()))
            .or_insert_with(|| (resolved.clone(), BTreeMap::new()));
        for (glyph_index, glyph) in glyphs.iter().enumerate() {
            let known_empty = run.clusters.as_ref().is_some_and(|clusters| {
                clusters.iter().any(|cluster| {
                    usize::try_from(cluster.glyph_start)
                        .ok()
                        .is_some_and(|start| {
                            usize::try_from(cluster.glyph_end).ok().is_some_and(|end| {
                                glyph_index >= start
                                    && glyph_index < end
                                    && usize::try_from(cluster.text_start_utf8).ok().is_some_and(
                                        |text_start| {
                                            usize::try_from(cluster.text_end_utf8).ok().is_some_and(
                                                |text_end| {
                                                    run.text.get(text_start..text_end).is_some_and(
                                                        |text| {
                                                            !text.is_empty()
                                                                && text
                                                                    .chars()
                                                                    .all(char::is_whitespace)
                                                        },
                                                    )
                                                },
                                            )
                                        },
                                    )
                            })
                        })
                })
            });
            entry
                .1
                .entry(glyph.glyph_id)
                .and_modify(|empty| *empty &= known_empty)
                .or_insert(known_empty);
        }
    }

    used.into_values()
        .filter_map(|(resolved, glyph_ids)| {
            let face = ALL_TEX_FONT_FACES
                .iter()
                .copied()
                .find(|face| face.face_id() == resolved.face_id)?;
            if !face.has_bundled_outline() {
                return None;
            }
            let font = resolve_font(face)?;
            if font.content_hash != resolved.content_hash
                || face.postscript_name() != resolved.postscript_name
                || resolved.glyph_id_kind != GlyphIdKind::Type1CharCode
            {
                return None;
            }
            let glyphs = glyph_ids
                .into_iter()
                .map(|(glyph_id, known_empty)| {
                    if known_empty {
                        Some(GlyphOutline {
                            glyph_id,
                            commands: Vec::new(),
                        })
                    } else {
                        outline_glyph(face, glyph_id)
                    }
                })
                .collect::<Option<Vec<_>>>()?;
            Some(BrowserFontAsset {
                face_id: resolved.face_id,
                postscript_name: resolved.postscript_name,
                glyph_id_kind: resolved.glyph_id_kind,
                content_hash: resolved.content_hash,
                glyphs,
            })
        })
        .collect()
}

const ALL_TEX_FONT_FACES: [TexFontFace; 14] = [
    TexFontFace::Roman10,
    TexFontFace::Roman7,
    TexFontFace::Roman5,
    TexFontFace::MathItalic10,
    TexFontFace::MathItalic7,
    TexFontFace::MathItalic5,
    TexFontFace::MathSymbol10,
    TexFontFace::MathSymbol7,
    TexFontFace::MathSymbol5,
    TexFontFace::MathExtension10,
    TexFontFace::TimesRoman,
    TexFontFace::TimesBold,
    TexFontFace::TimesItalic,
    TexFontFace::TimesBoldItalic,
];

#[cfg(not(target_family = "wasm"))]
fn read_kpse_file(stem: &str, extension: &str) -> Option<Vec<u8>> {
    if stem.is_empty()
        || !stem
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return None;
    }
    let name = format!("{stem}.{extension}");
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
    use tex_render_model::{FontFamilyRequest, FontRequest, FontRole, FontSeries, FontShape};

    use super::{TexFontFace, encode_text, face_for_request, resolve_font};

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

    #[test]
    fn named_times_requests_map_every_series_and_shape_to_a_times_face() {
        let face = |series, shape| {
            face_for_request(
                &FontRequest {
                    family: FontFamilyRequest::Named("times".to_string()),
                    series,
                    shape,
                    size_pt: 10.0,
                    role: FontRole::Body,
                },
                10.0,
            )
        };

        assert_eq!(
            face(FontSeries::Regular, FontShape::Upright),
            Some(TexFontFace::TimesRoman)
        );
        assert_eq!(
            face(FontSeries::Bold, FontShape::Upright),
            Some(TexFontFace::TimesBold)
        );
        assert_eq!(
            face(FontSeries::Regular, FontShape::Italic),
            Some(TexFontFace::TimesItalic)
        );
        assert_eq!(
            face(FontSeries::Bold, FontShape::Italic),
            Some(TexFontFace::TimesBoldItalic)
        );

        let serif = FontRequest {
            family: FontFamilyRequest::Serif,
            series: FontSeries::Regular,
            shape: FontShape::Upright,
            size_pt: 10.0,
            role: FontRole::Body,
        };
        assert_eq!(face_for_request(&serif, 10.0), Some(TexFontFace::Roman10));
    }

    #[test]
    fn symbol_requests_stay_renderer_fonts_while_extensions_use_cmex() {
        let request = |family| FontRequest {
            family,
            series: FontSeries::Regular,
            shape: FontShape::Upright,
            size_pt: 10.0,
            role: FontRole::Math,
        };

        assert_eq!(
            face_for_request(&request(FontFamilyRequest::Symbol), 10.0),
            None
        );
        assert_eq!(
            face_for_request(&request(FontFamilyRequest::MathExtension), 10.0),
            Some(TexFontFace::MathExtension10)
        );
    }

    #[test]
    fn times_faces_pair_psnfss_metrics_with_nimbus_type1_programs() {
        let faces = [
            (
                TexFontFace::TimesRoman,
                "ptmr8r",
                "utmr8a",
                "NimbusRomNo9L-Regu",
            ),
            (
                TexFontFace::TimesBold,
                "ptmb8r",
                "utmb8a",
                "NimbusRomNo9L-Medi",
            ),
            (
                TexFontFace::TimesItalic,
                "ptmri8r",
                "utmri8a",
                "NimbusRomNo9L-ReguItal",
            ),
            (
                TexFontFace::TimesBoldItalic,
                "ptmbi8r",
                "utmbi8a",
                "NimbusRomNo9L-MediItal",
            ),
        ];

        for (face, tfm_stem, type1_stem, postscript_name) in faces {
            assert_eq!(face.stem(), tfm_stem);
            assert_eq!(face.type1_stem(), type1_stem);
            assert_eq!(face.postscript_name(), postscript_name);

            let Some(font) = resolve_font(face) else {
                continue;
            };
            assert_eq!(font.face, face);
            assert!(font.metrics.width_em(b'M').is_some_and(|width| width > 0.0));
            assert!(
                font.type1
                    .bytes
                    .windows(postscript_name.len())
                    .any(|window| window == postscript_name.as_bytes())
            );
        }
    }
}
