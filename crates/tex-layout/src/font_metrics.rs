use tex_render_model::{FontFamilyRequest, FontRequest, FontSeries, TextCluster};

pub(crate) fn text_advance_pt(text: &str, font: &FontRequest, size_pt: f32) -> f32 {
    if let Some(face) = tex_fonts::face_for_request(font, size_pt)
        && let Some(advance_em) = tex_fonts::text_advance_em(face, text)
    {
        return advance_em * size_pt;
    }
    text.chars()
        .map(|ch| {
            let em_width = match font.family {
                FontFamilyRequest::Mono => 0.6,
                FontFamilyRequest::Math => {
                    if ch.is_whitespace() {
                        0.25
                    } else if ch.is_ascii_digit() {
                        0.5
                    } else {
                        0.62
                    }
                }
                FontFamilyRequest::Symbol => 0.75,
                FontFamilyRequest::Serif
                | FontFamilyRequest::Sans
                | FontFamilyRequest::Named(_) => {
                    if ch.is_whitespace() {
                        0.25
                    } else if matches!(ch, 'i' | 'j' | 'l' | 'I' | '!' | '|' | '\'' | '`') {
                        0.28
                    } else if matches!(ch, '.' | ',' | ';' | ':' | '-' | '/' | '\\') {
                        0.33
                    } else if matches!(ch, '(' | ')' | '[' | ']' | '{' | '}') {
                        0.38
                    } else if matches!(ch, 'm' | 'w' | 'M' | 'W') {
                        0.82
                    } else if ch.is_ascii_uppercase() {
                        0.68
                    } else if ch.is_ascii_digit() {
                        0.5
                    } else if ch.is_ascii() {
                        0.5
                    } else {
                        0.8
                    }
                }
            };
            let series_adjust = if font.series == FontSeries::Bold && !ch.is_whitespace() {
                1.04
            } else {
                1.0
            };
            em_width * series_adjust * size_pt
        })
        .sum()
}

pub(crate) fn approximate_text_clusters(text: &str) -> Option<Vec<TextCluster>> {
    if text.is_empty() {
        return None;
    }
    let glyph_count = text.chars().count() as u32;
    if text.len() == glyph_count as usize {
        return Some(vec![TextCluster {
            text_start_utf8: 0,
            text_end_utf8: text.len() as u32,
            glyph_start: 0,
            glyph_end: glyph_count,
        }]);
    }
    let mut clusters = Vec::new();
    for (glyph_index, (start, ch)) in text.char_indices().enumerate() {
        clusters.push(TextCluster {
            text_start_utf8: start as u32,
            text_end_utf8: (start + ch.len_utf8()) as u32,
            glyph_start: glyph_index as u32,
            glyph_end: glyph_index as u32 + 1,
        });
    }
    Some(clusters)
}
