use hypher::{Lang, MAX_INLINE_SIZE, hyphenate_bounded};
use tex_render_model::FontRequest;

use crate::font_metrics::{fallback_char_advance_pt, text_advance_pt};

const PRETOLERANCE: f64 = 100.0;
const TOLERANCE: f64 = 200.0;
const LINE_PENALTY: f64 = 10.0;
const HYPHEN_PENALTY: f64 = 50.0;
const ADJACENT_FITNESS_DEMERITS: f64 = 10_000.0;
const CONSECUTIVE_HYPHEN_DEMERITS: f64 = 10_000.0;
const MAX_PARAGRAPH_BYTES: usize = 64 * 1024;
const MAX_BREAK_CANDIDATES: usize = 2_048;
const MAX_DP_TRANSITIONS: usize = 2_000_000;
const WIDTH_EPSILON_PT: f32 = 0.01;

/// Inputs for the renderer-neutral paragraph breakpoint selector.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ParagraphBreakRequest<'a> {
    pub text: &'a str,
    pub font: &'a FontRequest,
    pub font_size_pt: f32,
    pub first_line_width_pt: f32,
    pub continuation_width_pt: f32,
    pub settings: ParagraphBreakSettings,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ParagraphBreakSettings {
    pub pretolerance: f64,
    pub tolerance: f64,
    pub emergency_stretch_pt: f32,
}

impl Default for ParagraphBreakSettings {
    fn default() -> Self {
        Self {
            pretolerance: PRETOLERANCE,
            tolerance: TOLERANCE,
            emergency_stretch_pt: 0.0,
        }
    }
}

/// One selected source range and the information needed to set its line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParagraphLine {
    pub start_utf8: usize,
    pub end_utf8: usize,
    pub append_hyphen: bool,
    pub natural_width_pt: f32,
    pub interword_spaces: usize,
    pub final_line: bool,
    pub forced_break: bool,
}

/// Select TeX-like paragraph breakpoints, leaving rendering and justification
/// to the caller. `None` requests the caller's deterministic fallback path.
pub(crate) fn select_paragraph_breaks(
    request: ParagraphBreakRequest<'_>,
) -> Option<Vec<ParagraphLine>> {
    if request.text.len() > MAX_PARAGRAPH_BYTES
        || !request.font_size_pt.is_finite()
        || request.font_size_pt <= 0.0
        || !request.first_line_width_pt.is_finite()
        || request.first_line_width_pt <= 0.0
        || !request.continuation_width_pt.is_finite()
        || request.continuation_width_pt <= 0.0
        || !request.settings.pretolerance.is_finite()
        || request.settings.pretolerance < 0.0
        || !request.settings.tolerance.is_finite()
        || request.settings.tolerance < 0.0
        || !request.settings.emergency_stretch_pt.is_finite()
        || request.settings.emergency_stretch_pt < 0.0
    {
        return None;
    }

    let Some((paragraph_start, _)) = request
        .text
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
    else {
        return Some(Vec::new());
    };
    let paragraph_end = request
        .text
        .char_indices()
        .rfind(|(_, ch)| !ch.is_whitespace())
        .map(|(index, ch)| index + ch.len_utf8())
        .expect("non-whitespace paragraph has an end");
    let metrics = ParagraphMetrics::new(request.text, request.font, request.font_size_pt);

    run_pass(
        request,
        &metrics,
        paragraph_start,
        paragraph_end,
        false,
        request.settings.pretolerance,
        0.0,
    )
    .or_else(|| {
        run_pass(
            request,
            &metrics,
            paragraph_start,
            paragraph_end,
            true,
            request.settings.tolerance,
            0.0,
        )
    })
    .or_else(|| {
        if request.settings.emergency_stretch_pt > 0.0 {
            run_pass(
                request,
                &metrics,
                paragraph_start,
                paragraph_end,
                true,
                request.settings.tolerance,
                request.settings.emergency_stretch_pt,
            )
        } else {
            None
        }
    })
}

struct ParagraphMetrics {
    fallback_width_prefix_pt: Vec<f32>,
    tex_width_prefix_pt: Vec<f32>,
    unsupported_prefix: Vec<u32>,
    leading_kern_pt: Vec<f32>,
    whitespace_run_prefix: Vec<usize>,
    starts_inside_whitespace: Vec<bool>,
}

impl ParagraphMetrics {
    fn new(text: &str, font: &FontRequest, font_size_pt: f32) -> Self {
        let mut metrics = Self {
            fallback_width_prefix_pt: vec![0.0; text.len() + 1],
            tex_width_prefix_pt: vec![0.0; text.len() + 1],
            unsupported_prefix: vec![0; text.len() + 1],
            leading_kern_pt: vec![0.0; text.len() + 1],
            whitespace_run_prefix: vec![0; text.len() + 1],
            starts_inside_whitespace: vec![false; text.len() + 1],
        };
        let face = tex_fonts::face_for_request(font, font_size_pt);
        let mut previous = None::<(char, f32)>;
        let mut previous_was_whitespace = false;

        for (start, ch) in text.char_indices() {
            let end = start + ch.len_utf8();
            metrics.fallback_width_prefix_pt[end] = metrics.fallback_width_prefix_pt[start]
                + fallback_char_advance_pt(ch, font, font_size_pt);

            let exact_width_pt = face.and_then(|face| {
                let mut encoded = [0; 4];
                tex_fonts::text_advance_em(face, ch.encode_utf8(&mut encoded))
                    .map(|advance_em| advance_em * font_size_pt)
            });
            let leading_kern_pt = match (face, previous, exact_width_pt) {
                (Some(face), Some((previous_ch, previous_width_pt)), Some(width_pt)) => {
                    let mut pair = String::with_capacity(previous_ch.len_utf8() + ch.len_utf8());
                    pair.push(previous_ch);
                    pair.push(ch);
                    tex_fonts::text_advance_em(face, &pair)
                        .map(|advance_em| advance_em * font_size_pt - previous_width_pt - width_pt)
                        .unwrap_or(0.0)
                }
                _ => 0.0,
            };
            metrics.leading_kern_pt[start] = leading_kern_pt;
            metrics.tex_width_prefix_pt[end] = metrics.tex_width_prefix_pt[start]
                + leading_kern_pt
                + exact_width_pt.unwrap_or(0.0);
            metrics.unsupported_prefix[end] =
                metrics.unsupported_prefix[start] + u32::from(exact_width_pt.is_none());

            let is_whitespace = ch.is_whitespace();
            metrics.starts_inside_whitespace[start] = is_whitespace && previous_was_whitespace;
            metrics.whitespace_run_prefix[end] = metrics.whitespace_run_prefix[start]
                + usize::from(is_whitespace && !previous_was_whitespace);
            previous = exact_width_pt.map(|width_pt| (ch, width_pt));
            previous_was_whitespace = is_whitespace;
        }
        metrics
    }

    fn width_pt(&self, start_utf8: usize, end_utf8: usize) -> f32 {
        debug_assert!(start_utf8 <= end_utf8);
        if self.unsupported_prefix[end_utf8] == self.unsupported_prefix[start_utf8] {
            self.tex_width_prefix_pt[end_utf8]
                - self.tex_width_prefix_pt[start_utf8]
                - self.leading_kern_pt[start_utf8]
        } else {
            self.fallback_width_prefix_pt[end_utf8] - self.fallback_width_prefix_pt[start_utf8]
        }
    }

    fn interword_spaces(&self, start_utf8: usize, end_utf8: usize) -> usize {
        let runs = self.whitespace_run_prefix[end_utf8] - self.whitespace_run_prefix[start_utf8];
        runs + usize::from(self.starts_inside_whitespace[start_utf8])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BreakKind {
    Start,
    Whitespace { forced: bool },
    Hyphen,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BreakCandidate {
    line_end_utf8: usize,
    next_start_utf8: usize,
    kind: BreakKind,
}

impl BreakCandidate {
    fn append_hyphen(self) -> bool {
        self.kind == BreakKind::Hyphen
    }

    fn final_line(self) -> bool {
        self.kind == BreakKind::End
    }

    fn forced_break(self) -> bool {
        matches!(self.kind, BreakKind::Whitespace { forced: true })
    }

    fn sort_rank(self) -> u8 {
        match self.kind {
            BreakKind::Start => 0,
            BreakKind::Hyphen => 1,
            BreakKind::Whitespace { .. } => 2,
            BreakKind::End => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FitnessClass {
    Tight = 0,
    Decent = 1,
    Loose = 2,
    VeryLoose = 3,
}

impl FitnessClass {
    fn from_ratio(ratio: f64) -> Self {
        if ratio < -0.5 {
            Self::Tight
        } else if ratio <= 0.5 {
            Self::Decent
        } else if ratio <= 1.0 {
            Self::Loose
        } else {
            Self::VeryLoose
        }
    }

    fn distance(self, other: Self) -> u8 {
        (self as i8 - other as i8).unsigned_abs()
    }
}

#[derive(Debug, Clone)]
struct BreakState {
    candidate_index: usize,
    fitness: FitnessClass,
    previous_hyphen: bool,
    demerits: f64,
    line_count: usize,
    previous_state: Option<usize>,
    line: Option<ParagraphLine>,
}

fn run_pass(
    request: ParagraphBreakRequest<'_>,
    metrics: &ParagraphMetrics,
    paragraph_start: usize,
    paragraph_end: usize,
    allow_hyphenation: bool,
    tolerance: f64,
    emergency_stretch_pt: f32,
) -> Option<Vec<ParagraphLine>> {
    let mut candidates = vec![BreakCandidate {
        line_end_utf8: paragraph_start,
        next_start_utf8: paragraph_start,
        kind: BreakKind::Start,
    }];

    let mut cursor = paragraph_start;
    while cursor < paragraph_end {
        let ch = request.text[cursor..paragraph_end]
            .chars()
            .next()
            .expect("cursor is inside paragraph");
        if ch.is_whitespace() {
            let whitespace_start = cursor;
            let mut forced = false;
            while cursor < paragraph_end {
                let whitespace = request.text[cursor..paragraph_end]
                    .chars()
                    .next()
                    .expect("cursor is inside whitespace");
                if !whitespace.is_whitespace() {
                    break;
                }
                forced |= matches!(whitespace, '\n' | '\r');
                cursor += whitespace.len_utf8();
            }
            candidates.push(BreakCandidate {
                line_end_utf8: whitespace_start,
                next_start_utf8: cursor,
                kind: BreakKind::Whitespace { forced },
            });
        } else {
            cursor += ch.len_utf8();
        }
        if candidates.len() > MAX_BREAK_CANDIDATES {
            return None;
        }
    }

    if allow_hyphenation {
        let mut token_start = paragraph_start;
        while token_start < paragraph_end {
            while token_start < paragraph_end {
                let ch = request.text[token_start..paragraph_end]
                    .chars()
                    .next()
                    .expect("token start is inside paragraph");
                if !ch.is_whitespace() {
                    break;
                }
                token_start += ch.len_utf8();
            }
            if token_start >= paragraph_end {
                break;
            }

            let mut token_end = token_start;
            while token_end < paragraph_end {
                let ch = request.text[token_end..paragraph_end]
                    .chars()
                    .next()
                    .expect("token end is inside paragraph");
                if ch.is_whitespace() {
                    break;
                }
                token_end += ch.len_utf8();
            }

            let token = &request.text[token_start..token_end];
            if token.len() <= MAX_INLINE_SIZE && token.chars().all(char::is_alphabetic) {
                let mut split_offset = 0usize;
                for syllable in hyphenate_bounded(token, Lang::English, 2, 3) {
                    split_offset += syllable.len();
                    if split_offset < token.len() {
                        let split_utf8 = token_start + split_offset;
                        debug_assert!(request.text.is_char_boundary(split_utf8));
                        candidates.push(BreakCandidate {
                            line_end_utf8: split_utf8,
                            next_start_utf8: split_utf8,
                            kind: BreakKind::Hyphen,
                        });
                    }
                    if candidates.len() > MAX_BREAK_CANDIDATES {
                        return None;
                    }
                }
            }
            token_start = token_end;
        }
    }

    candidates.push(BreakCandidate {
        line_end_utf8: paragraph_end,
        next_start_utf8: paragraph_end,
        kind: BreakKind::End,
    });
    if candidates.len() > MAX_BREAK_CANDIDATES {
        return None;
    }
    candidates.sort_by_key(|candidate| (candidate.line_end_utf8, candidate.sort_rank()));
    candidates.dedup();

    let end_candidate_index = candidates
        .iter()
        .position(|candidate| candidate.kind == BreakKind::End)
        .expect("paragraph has an end candidate");
    let mut states = vec![BreakState {
        candidate_index: 0,
        fitness: FitnessClass::Decent,
        previous_hyphen: false,
        demerits: 0.0,
        line_count: 0,
        previous_state: None,
        line: None,
    }];
    let mut best_by_candidate = vec![[None; 8]; candidates.len()];
    best_by_candidate[0][FitnessClass::Decent as usize * 2] = Some(0);
    let mut transition_count = 0usize;
    let space_width_pt = text_advance_pt(" ", request.font, request.font_size_pt).max(0.01);
    let hyphen_width_pt = text_advance_pt("-", request.font, request.font_size_pt);

    for candidate_index in 0..candidates.len() {
        let source_states = best_by_candidate[candidate_index];
        let forced_limit = ((candidate_index + 1)..candidates.len())
            .find(|index| candidates[*index].forced_break())
            .unwrap_or(end_candidate_index);

        for state_index in source_states.into_iter().flatten() {
            let state = states[state_index].clone();
            debug_assert_eq!(state.candidate_index, candidate_index);
            for endpoint_index in (candidate_index + 1)..=forced_limit {
                transition_count += 1;
                if transition_count > MAX_DP_TRANSITIONS {
                    return None;
                }

                let endpoint = candidates[endpoint_index];
                let line_start = candidates[candidate_index].next_start_utf8;
                let line_end = endpoint.line_end_utf8;
                if line_end <= line_start {
                    continue;
                }

                let target_width_pt = if state.line_count == 0 {
                    request.first_line_width_pt
                } else {
                    request.continuation_width_pt
                };
                let interword_spaces = metrics.interword_spaces(line_start, line_end);
                let natural_width_pt = metrics.width_pt(line_start, line_end)
                    + if endpoint.append_hyphen() {
                        hyphen_width_pt
                    } else {
                        0.0
                    };
                let ragged_line = endpoint.final_line() || endpoint.forced_break();
                let (badness, fitness) = if ragged_line {
                    if natural_width_pt > target_width_pt + WIDTH_EPSILON_PT {
                        continue;
                    }
                    (0.0, FitnessClass::Decent)
                } else {
                    let difference_pt = target_width_pt - natural_width_pt;
                    if interword_spaces == 0 {
                        if difference_pt < -WIDTH_EPSILON_PT
                            || difference_pt > emergency_stretch_pt + WIDTH_EPSILON_PT
                        {
                            continue;
                        }
                        let ratio = if difference_pt > WIDTH_EPSILON_PT {
                            difference_pt as f64 / emergency_stretch_pt as f64
                        } else {
                            0.0
                        };
                        let badness = (100.0 * ratio.abs().powi(3)).min(10_000.0);
                        if badness > tolerance {
                            continue;
                        }
                        (badness, FitnessClass::from_ratio(ratio))
                    } else {
                        let ratio = if difference_pt >= 0.0 {
                            let stretch_pt = interword_spaces as f32 * space_width_pt * 0.5
                                + emergency_stretch_pt;
                            difference_pt as f64 / stretch_pt as f64
                        } else {
                            let shrink_pt = interword_spaces as f32 * space_width_pt / 3.0;
                            difference_pt as f64 / shrink_pt as f64
                        };
                        let badness = (100.0 * ratio.abs().powi(3)).min(10_000.0);
                        if badness > tolerance {
                            continue;
                        }
                        (badness, FitnessClass::from_ratio(ratio))
                    }
                };

                let mut demerits = state.demerits + (LINE_PENALTY + badness).powi(2);
                if endpoint.append_hyphen() {
                    demerits += HYPHEN_PENALTY.powi(2);
                }
                if state.line_count > 0 && fitness.distance(state.fitness) > 1 {
                    demerits += ADJACENT_FITNESS_DEMERITS;
                }
                if state.previous_hyphen && endpoint.append_hyphen() {
                    demerits += CONSECUTIVE_HYPHEN_DEMERITS;
                }

                let line = ParagraphLine {
                    start_utf8: line_start,
                    end_utf8: line_end,
                    append_hyphen: endpoint.append_hyphen(),
                    natural_width_pt,
                    interword_spaces,
                    final_line: endpoint.final_line(),
                    forced_break: endpoint.forced_break(),
                };
                let slot = fitness as usize * 2 + usize::from(endpoint.append_hyphen());
                let should_replace =
                    best_by_candidate[endpoint_index][slot].is_none_or(|existing_index| {
                        demerits + f64::EPSILON < states[existing_index].demerits
                    });
                if should_replace {
                    let next_state_index = states.len();
                    states.push(BreakState {
                        candidate_index: endpoint_index,
                        fitness,
                        previous_hyphen: endpoint.append_hyphen(),
                        demerits,
                        line_count: state.line_count + 1,
                        previous_state: Some(state_index),
                        line: Some(line),
                    });
                    best_by_candidate[endpoint_index][slot] = Some(next_state_index);
                }
            }
        }
    }

    let final_state_index = best_by_candidate[end_candidate_index]
        .into_iter()
        .flatten()
        .min_by(|left, right| {
            states[*left]
                .demerits
                .total_cmp(&states[*right].demerits)
                .then_with(|| states[*left].line_count.cmp(&states[*right].line_count))
        })?;
    let mut lines = Vec::with_capacity(states[final_state_index].line_count);
    let mut state_index = final_state_index;
    loop {
        let state = &states[state_index];
        if let Some(line) = &state.line {
            lines.push(line.clone());
        }
        let Some(previous_state) = state.previous_state else {
            break;
        };
        state_index = previous_state;
    }
    lines.reverse();
    Some(lines)
}

#[cfg(test)]
mod tests {
    use tex_render_model::{FontFamilyRequest, FontRequest, FontRole, FontSeries, FontShape};

    use super::{
        MAX_PARAGRAPH_BYTES, ParagraphBreakRequest, ParagraphBreakSettings, ParagraphMetrics,
        select_paragraph_breaks,
    };
    use crate::font_metrics::text_advance_pt;

    fn mono_font() -> FontRequest {
        FontRequest {
            family: FontFamilyRequest::Mono,
            series: FontSeries::Regular,
            shape: FontShape::Upright,
            size_pt: 10.0,
            role: FontRole::Body,
        }
    }

    fn serif_font() -> FontRequest {
        FontRequest {
            family: FontFamilyRequest::Serif,
            series: FontSeries::Regular,
            shape: FontShape::Upright,
            size_pt: 10.0,
            role: FontRole::Body,
        }
    }

    fn request<'a>(
        text: &'a str,
        font: &'a FontRequest,
        first_line_width_pt: f32,
        continuation_width_pt: f32,
    ) -> ParagraphBreakRequest<'a> {
        ParagraphBreakRequest {
            text,
            font,
            font_size_pt: 10.0,
            first_line_width_pt,
            continuation_width_pt,
            settings: ParagraphBreakSettings::default(),
        }
    }

    #[test]
    fn cached_metrics_match_direct_widths_and_whitespace_runs() {
        let text = "AV alpha  \u{03b2}eta gamma";
        let alpha_start = text.find("alpha").unwrap();
        let beta_start = text.find("\u{03b2}eta").unwrap();
        let ranges = [
            (0, 2),
            (alpha_start, alpha_start + "alpha".len()),
            (alpha_start, beta_start + "\u{03b2}eta".len()),
            (beta_start, text.len()),
        ];

        for font in [mono_font(), serif_font()] {
            let metrics = ParagraphMetrics::new(text, &font, 10.0);
            for (start, end) in ranges {
                let direct_width = text_advance_pt(&text[start..end], &font, 10.0);
                assert!((metrics.width_pt(start, end) - direct_width).abs() < 0.001);
                let direct_spaces = text[start..end]
                    .chars()
                    .fold((0usize, false), |(count, in_space), ch| {
                        if ch.is_whitespace() {
                            (count + usize::from(!in_space), true)
                        } else {
                            (count, false)
                        }
                    })
                    .0;
                assert_eq!(metrics.interword_spaces(start, end), direct_spaces);
            }
        }
    }

    #[test]
    fn removes_breakpoint_whitespace_from_both_lines() {
        let text = "alpha beta gamma";
        let font = mono_font();
        let width = text_advance_pt("alpha beta", &font, 10.0);
        let lines = select_paragraph_breaks(request(text, &font, width, width)).unwrap();

        assert_eq!(&text[lines[0].start_utf8..lines[0].end_utf8], "alpha beta");
        assert_eq!(&text[lines[1].start_utf8..lines[1].end_utf8], "gamma");
        assert_eq!(lines[0].end_utf8, 10);
        assert_eq!(lines[1].start_utf8, 11);
        assert_eq!(lines[0].interword_spaces, 1);
    }

    #[test]
    fn keeps_the_last_line_natural_and_ragged() {
        let text = "alpha beta gamma";
        let font = mono_font();
        let width = text_advance_pt("alpha beta", &font, 10.0);
        let lines = select_paragraph_breaks(request(text, &font, width, width)).unwrap();
        let last = lines.last().unwrap();

        assert!(last.final_line);
        assert!(!last.forced_break);
        assert_eq!(last.natural_width_pt, text_advance_pt("gamma", &font, 10.0));
        assert!(last.natural_width_pt < width);
    }

    #[test]
    fn global_solution_can_differ_from_greedy() {
        let text = "a a a a a a a a a a a";
        let font = mono_font();
        let width = 60.0;
        let lines = select_paragraph_breaks(request(text, &font, width, width)).unwrap();
        let greedy_end = text
            .split_inclusive(' ')
            .scan(0.0, |used_width, part| {
                let width = text_advance_pt(part, &font, 10.0);
                (*used_width + width <= 60.0).then(|| {
                    *used_width += width;
                    part.len()
                })
            })
            .sum::<usize>()
            .saturating_sub(1);

        assert_eq!(greedy_end, 9);
        assert_eq!(&text[..lines[0].end_utf8], "a a a a a a");
        assert_ne!(lines[0].end_utf8, greedy_end);
    }

    #[test]
    fn adds_an_english_discretionary_hyphen() {
        let text = "extensive";
        let font = mono_font();
        let width = text_advance_pt("exten-", &font, 10.0);
        let lines = select_paragraph_breaks(request(text, &font, width, width)).unwrap();

        assert_eq!(&text[lines[0].start_utf8..lines[0].end_utf8], "exten");
        assert!(lines[0].append_hyphen);
        assert!(text.is_char_boundary(lines[0].end_utf8));
        assert_eq!(&text[lines[1].start_utf8..lines[1].end_utf8], "sive");
    }

    #[test]
    fn applies_the_narrower_first_line_width_once() {
        let text = "aa bb cc dd ee";
        let font = mono_font();
        let first_width = text_advance_pt("aa bb", &font, 10.0);
        let continuation_width = text_advance_pt("cc dd ee", &font, 10.0);
        let lines =
            select_paragraph_breaks(request(text, &font, first_width, continuation_width)).unwrap();

        assert_eq!(&text[lines[0].start_utf8..lines[0].end_utf8], "aa bb");
        assert_eq!(&text[lines[1].start_utf8..lines[1].end_utf8], "cc dd ee");
    }

    #[test]
    fn sloppy_tolerance_accepts_loose_unhyphenated_lines() {
        let text = "aa1 bb2 cc3 dd4";
        let font = mono_font();
        let width = text_advance_pt("aa1 bb2", &font, 10.0) + 6.0;
        assert!(select_paragraph_breaks(request(text, &font, width, width)).is_none());

        let mut sloppy = request(text, &font, width, width);
        sloppy.settings.tolerance = 9_999.0;
        let lines = select_paragraph_breaks(sloppy).unwrap();

        assert_eq!(lines.len(), 2);
        assert!(!lines[0].append_hyphen);
        assert_eq!(&text[lines[0].start_utf8..lines[0].end_utf8], "aa1 bb2");
    }

    #[test]
    fn emergency_stretch_is_only_used_after_normal_passes_fail() {
        let text = "aaaaaaaaa1 bbbbbbbbb2 ccccccccc3 ddddddddd4";
        let font = mono_font();
        let width = text_advance_pt("aaaaaaaaa1 bbbbbbbbb2", &font, 10.0) + 33.0;
        let mut without_emergency = request(text, &font, width, width);
        without_emergency.settings.tolerance = 9_999.0;
        assert!(select_paragraph_breaks(without_emergency).is_none());

        let mut with_emergency = without_emergency;
        with_emergency.settings.emergency_stretch_pt = 40.0;
        let lines = select_paragraph_breaks(with_emergency).unwrap();

        assert_eq!(lines.len(), 2);
        assert_eq!(
            &text[lines[0].start_utf8..lines[0].end_utf8],
            "aaaaaaaaa1 bbbbbbbbb2"
        );
    }

    #[test]
    fn oversized_input_returns_the_same_fallback_result() {
        let text = "a".repeat(MAX_PARAGRAPH_BYTES + 1);
        let font = mono_font();

        let first = select_paragraph_breaks(request(&text, &font, 100.0, 100.0));
        let second = select_paragraph_breaks(request(&text, &font, 100.0, 100.0));

        assert!(first.is_none());
        assert_eq!(first, second);
    }
}
