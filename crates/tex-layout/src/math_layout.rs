use tex_render_model::{
    DrawOp, FontFamilyRequest, FontRequest, FontRole, FontSeries, FontShape, MathAtomKind,
    MathLargeOperator, MathNode, MathScriptPlacement, Point, PositionedTextRun, Rect,
    SourceProvenance,
};

use crate::font_metrics::{approximate_text_clusters, text_advance_pt};

pub(crate) struct MathLayoutBox {
    pub(crate) width_pt: f32,
    pub(crate) ascent_pt: f32,
    pub(crate) descent_pt: f32,
    pub(crate) ops: Vec<DrawOp>,
}

pub(crate) fn layout_math_node(
    node: &MathNode,
    size_pt: f32,
    source: &SourceProvenance,
    math_font: &FontRequest,
) -> MathLayoutBox {
    layout_math_node_with_spacing(node, size_pt, source, math_font, true)
}

fn layout_math_node_with_spacing(
    node: &MathNode,
    size_pt: f32,
    source: &SourceProvenance,
    math_font: &FontRequest,
    allow_atom_spacing: bool,
) -> MathLayoutBox {
    match node {
        MathNode::Atom { text, atom_kind } => {
            let mut font = math_font.clone();
            font.size_pt = size_pt;
            if *atom_kind == MathAtomKind::Relation {
                font.family = FontFamilyRequest::Symbol;
            }
            font.shape = if *atom_kind == MathAtomKind::Identifier {
                FontShape::Italic
            } else {
                FontShape::Upright
            };
            let width_pt = text_advance_pt(text, &font, size_pt);
            MathLayoutBox {
                width_pt,
                ascent_pt: size_pt * 0.75,
                descent_pt: size_pt * 0.2,
                ops: vec![DrawOp::TextRun(PositionedTextRun {
                    origin: Point { x: 0.0, y: 0.0 },
                    text: text.clone(),
                    font,
                    size_pt,
                    approximate_advance_pt: width_pt,
                    resolved_font: None,
                    glyphs: None,
                    clusters: approximate_text_clusters(text),
                    source: source.clone(),
                })],
            }
        }
        MathNode::LargeOperator { operator } => {
            let text = match operator {
                MathLargeOperator::Sum => "∑",
                MathLargeOperator::Product => "∏",
                MathLargeOperator::Integral => "∫",
            };
            let operator_size_pt = size_pt;
            let font = FontRequest {
                family: FontFamilyRequest::MathExtension,
                series: FontSeries::Regular,
                shape: FontShape::Upright,
                size_pt: operator_size_pt,
                role: FontRole::Math,
            };
            let width_pt = text_advance_pt(text, &font, operator_size_pt);
            MathLayoutBox {
                width_pt,
                ascent_pt: size_pt * 1.05,
                descent_pt: size_pt * 0.55,
                ops: vec![DrawOp::TextRun(PositionedTextRun {
                    origin: Point {
                        x: 0.0,
                        y: -size_pt * 0.95,
                    },
                    text: text.to_string(),
                    font,
                    size_pt: operator_size_pt,
                    approximate_advance_pt: width_pt,
                    resolved_font: None,
                    glyphs: None,
                    clusters: approximate_text_clusters(text),
                    source: source.clone(),
                })],
            }
        }
        MathNode::Row { children } => {
            let mut width_pt = 0.0_f32;
            let mut ascent_pt = 0.0_f32;
            let mut descent_pt = 0.0_f32;
            let mut ops = Vec::new();
            let classes = row_spacing_classes(children);
            for (index, child) in children.iter().enumerate() {
                if index > 0 {
                    width_pt += math_atom_spacing_pt(
                        classes[index - 1],
                        classes[index],
                        size_pt,
                        allow_atom_spacing,
                    );
                }
                let mut child_box = layout_math_node_with_spacing(
                    child,
                    size_pt,
                    source,
                    math_font,
                    allow_atom_spacing,
                );
                for op in &mut child_box.ops {
                    op.translate(width_pt, 0.0);
                }
                width_pt += child_box.width_pt;
                ascent_pt = ascent_pt.max(child_box.ascent_pt);
                descent_pt = descent_pt.max(child_box.descent_pt);
                ops.extend(child_box.ops);
            }
            MathLayoutBox {
                width_pt,
                ascent_pt,
                descent_pt,
                ops,
            }
        }
        MathNode::Fraction {
            numerator,
            denominator,
        } => {
            let mut numerator =
                layout_math_node_with_spacing(numerator, size_pt, source, math_font, true);
            let mut denominator =
                layout_math_node_with_spacing(denominator, size_pt, source, math_font, true);
            let rule_width_pt = numerator.width_pt.max(denominator.width_pt);
            let null_delimiter_space_pt = size_pt * 0.12;
            let width_pt = rule_width_pt + null_delimiter_space_pt * 2.0;
            let numerator_x = null_delimiter_space_pt + (rule_width_pt - numerator.width_pt) / 2.0;
            let denominator_x =
                null_delimiter_space_pt + (rule_width_pt - denominator.width_pt) / 2.0;
            let rule_axis_y = -size_pt * 0.23;
            let numerator_y = -size_pt * 0.6765;
            let denominator_y = size_pt * 0.686;
            for op in &mut numerator.ops {
                op.translate(numerator_x, numerator_y);
            }
            for op in &mut denominator.ops {
                op.translate(denominator_x, denominator_y);
            }
            let rule_height_pt = size_pt * 0.04;
            let mut ops = numerator.ops;
            ops.push(DrawOp::Rule(Rect {
                x: null_delimiter_space_pt,
                y: rule_axis_y - rule_height_pt / 2.0,
                width: rule_width_pt,
                height: rule_height_pt,
            }));
            ops.extend(denominator.ops);
            MathLayoutBox {
                width_pt,
                ascent_pt: size_pt * 1.426508,
                descent_pt: size_pt * 0.685951,
                ops,
            }
        }
        MathNode::Scripts {
            base,
            subscript,
            superscript,
            placement,
        } => {
            let base_is_large_operator = matches!(base.as_ref(), MathNode::LargeOperator { .. });
            let mut base =
                layout_math_node_with_spacing(base, size_pt, source, math_font, allow_atom_spacing);
            let script_size_pt = size_pt * 0.7;
            let mut subscript = subscript.as_deref().map(|node| {
                layout_math_node_with_spacing(node, script_size_pt, source, math_font, false)
            });
            let mut superscript = superscript.as_deref().map(|node| {
                layout_math_node_with_spacing(node, script_size_pt, source, math_font, false)
            });
            if *placement == MathScriptPlacement::Limits {
                let width_pt = subscript
                    .as_ref()
                    .map(|layout| layout.width_pt)
                    .into_iter()
                    .chain(superscript.as_ref().map(|layout| layout.width_pt))
                    .fold(base.width_pt, f32::max);
                let base_x = (width_pt - base.width_pt) / 2.0;
                for op in &mut base.ops {
                    op.translate(base_x, 0.0);
                }
                let mut ascent_pt = base.ascent_pt;
                let mut descent_pt = base.descent_pt;
                if let Some(layout) = &mut superscript {
                    let baseline_y = -size_pt * 1.25;
                    let x = (width_pt - layout.width_pt) / 2.0;
                    for op in &mut layout.ops {
                        op.translate(x, baseline_y);
                    }
                    ascent_pt = ascent_pt.max(-baseline_y + layout.ascent_pt);
                }
                if let Some(layout) = &mut subscript {
                    let baseline_y = size_pt * 1.18;
                    let x = (width_pt - layout.width_pt) / 2.0;
                    for op in &mut layout.ops {
                        op.translate(x, baseline_y);
                    }
                    descent_pt = descent_pt.max(baseline_y + layout.descent_pt);
                }
                if base_is_large_operator {
                    ascent_pt = size_pt * 1.651393;
                    descent_pt = size_pt * 1.279865;
                }
                let mut ops = base.ops;
                if let Some(layout) = superscript {
                    ops.extend(layout.ops);
                }
                if let Some(layout) = subscript {
                    ops.extend(layout.ops);
                }
                MathLayoutBox {
                    width_pt,
                    ascent_pt,
                    descent_pt,
                    ops,
                }
            } else {
                let script_width_pt = subscript
                    .as_ref()
                    .map(|layout| layout.width_pt)
                    .into_iter()
                    .chain(superscript.as_ref().map(|layout| layout.width_pt))
                    .fold(0.0_f32, f32::max);
                let script_x = base.width_pt + size_pt * 0.05;
                let mut ascent_pt = base.ascent_pt;
                let mut descent_pt = base.descent_pt;
                if let Some(layout) = &mut superscript {
                    let baseline_y = -size_pt * 0.55;
                    for op in &mut layout.ops {
                        op.translate(script_x, baseline_y);
                    }
                    ascent_pt = ascent_pt.max(-baseline_y + layout.ascent_pt);
                }
                if let Some(layout) = &mut subscript {
                    let baseline_y = size_pt * 0.35;
                    for op in &mut layout.ops {
                        op.translate(script_x, baseline_y);
                    }
                    descent_pt = descent_pt.max(baseline_y + layout.descent_pt);
                }
                let mut ops = base.ops;
                if let Some(layout) = superscript {
                    ops.extend(layout.ops);
                }
                if let Some(layout) = subscript {
                    ops.extend(layout.ops);
                }
                MathLayoutBox {
                    width_pt: base.width_pt + size_pt * 0.05 + script_width_pt,
                    ascent_pt,
                    descent_pt,
                    ops,
                }
            }
        }
    }
}

/// TeX's atom types, in the row and column order of its spacing table.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MathSpacingClass {
    Ordinary,
    LargeOperator,
    Binary,
    Relation,
    Open,
    Close,
    Punctuation,
    Inner,
}

fn math_spacing_class(node: &MathNode) -> MathSpacingClass {
    match node {
        // Classes follow plain TeX's \mathcode: `/` and `.` are ordinary, `:` is
        // a relation, and `,` `;` are punctuation.
        MathNode::Atom { text, atom_kind } => match (atom_kind, text.as_str()) {
            (MathAtomKind::Operator, "/") => MathSpacingClass::Ordinary,
            (MathAtomKind::Operator, _) => MathSpacingClass::Binary,
            (MathAtomKind::Relation, _) | (MathAtomKind::Punctuation, ":") => {
                MathSpacingClass::Relation
            }
            (MathAtomKind::Punctuation, "," | ";") => MathSpacingClass::Punctuation,
            (MathAtomKind::Delimiter, "(" | "[") => MathSpacingClass::Open,
            (MathAtomKind::Delimiter, ")" | "]") => MathSpacingClass::Close,
            _ => MathSpacingClass::Ordinary,
        },
        MathNode::LargeOperator { .. } => MathSpacingClass::LargeOperator,
        MathNode::Fraction { .. } => MathSpacingClass::Inner,
        MathNode::Scripts { base, .. } => math_spacing_class(base),
        MathNode::Row { .. } => MathSpacingClass::Ordinary,
    }
}

/// Spacing classes of a row after TeX's Bin-to-Ord rule (tex.web §728-§729): a
/// binary operator with nothing to combine on either side acts as ordinary.
fn row_spacing_classes(children: &[MathNode]) -> Vec<MathSpacingClass> {
    use MathSpacingClass::*;

    let mut classes = children.iter().map(math_spacing_class).collect::<Vec<_>>();
    for index in 0..classes.len() {
        let previous = index.checked_sub(1).map(|previous| classes[previous]);
        match classes[index] {
            Binary
                if matches!(
                    previous,
                    None | Some(Binary | LargeOperator | Relation | Open | Punctuation)
                ) =>
            {
                classes[index] = Ordinary;
            }
            Relation | Close | Punctuation if previous == Some(Binary) => {
                classes[index - 1] = Ordinary;
            }
            _ => {}
        }
    }
    if let Some(last @ Binary) = classes.last_mut() {
        *last = Ordinary;
    }
    classes
}

/// Inter-atom space from TeX's table (tex.web §764): 1 is a thin space only
/// outside scripts, 2 a thin space, 3 a medium and 4 a thick space only
/// outside scripts. Pairs TeX rules out after the Bin-to-Ord rule are 0.
fn math_atom_spacing_pt(
    left: MathSpacingClass,
    right: MathSpacingClass,
    size_pt: f32,
    non_script: bool,
) -> f32 {
    const SPACING: [[u8; 8]; 8] = [
        [0, 2, 3, 4, 0, 0, 0, 1],
        [2, 2, 0, 4, 0, 0, 0, 1],
        [3, 3, 0, 0, 3, 0, 0, 3],
        [4, 4, 0, 0, 4, 0, 0, 4],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 2, 3, 4, 0, 0, 0, 1],
        [1, 1, 0, 1, 1, 1, 1, 1],
        [1, 2, 3, 4, 1, 0, 1, 1],
    ];
    let mu_pt = size_pt / 18.0;
    match SPACING[left as usize][right as usize] {
        1 if non_script => 3.0 * mu_pt,
        2 => 3.0 * mu_pt,
        3 if non_script => 4.0 * mu_pt,
        4 if non_script => 5.0 * mu_pt,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use tex_render_model::{
        DrawOp, FontFamilyRequest, FontRequest, FontRole, FontSeries, FontShape, MathAtomKind,
        MathNode, SourceProvenance,
    };

    use super::layout_math_node;
    use crate::math_ir::parse_display_math_structure;

    /// The gaps, in mu, between consecutive top-level glyphs of `source`.
    fn gaps_mu(source: &str) -> Vec<(String, f32)> {
        let node = parse_display_math_structure(source).expect("parsable math");
        let math_font = FontRequest {
            family: FontFamilyRequest::Math,
            series: FontSeries::Regular,
            shape: FontShape::Upright,
            size_pt: 18.0,
            role: FontRole::Math,
        };
        let layout = layout_math_node(
            &node,
            18.0,
            &SourceProvenance::generated("test", "spacing"),
            &math_font,
        );
        let runs = layout
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::TextRun(run) => Some(run),
                _ => None,
            })
            .collect::<Vec<_>>();
        runs.windows(2)
            .map(|pair| {
                let gap = pair[1].origin.x - (pair[0].origin.x + pair[0].approximate_advance_pt);
                // At 18pt one mu is exactly 1pt.
                (
                    format!("{}{}", pair[0].text, pair[1].text),
                    (gap * 100.0).round() / 100.0,
                )
            })
            .collect()
    }

    fn gap(source: &str, pair: &str) -> f32 {
        gaps_mu(source)
            .into_iter()
            .find_map(|(glyphs, gap)| (glyphs == pair).then_some(gap))
            .unwrap_or_else(|| panic!("no {pair:?} in {:?}", gaps_mu(source)))
    }

    #[test]
    fn binary_operators_after_relations_and_openings_act_as_ordinary() {
        // TeX turns a Bin atom into Ord after Rel/Open/Bin/Op/Punct or at the
        // start of a list (tex.web §728), so unary minus is not spaced.
        assert_eq!(gap("a=-b", "a="), 5.0);
        assert_eq!(gap("a=-b", "=-"), 5.0);
        assert_eq!(gap("a=-b", "-b"), 0.0);
        assert_eq!(gap("(-b)", "(-"), 0.0);
        assert_eq!(gap("(-b)", "-b"), 0.0);
        assert_eq!(gap("a+b", "a+"), 4.0);
        assert_eq!(gap("a+b", "+b"), 4.0);
    }

    #[test]
    fn math_atoms_use_plain_tex_classes_and_spacing() {
        // `,` is punctuation (thin space after), `/` and `.` are ordinary, and
        // `:` is a relation, so `:=` is two adjacent relations.
        assert_eq!(gap("f(x,y)", ",y"), 3.0);
        assert_eq!(gap("f(x,y)", "x,"), 0.0);
        assert_eq!(gap("a/b", "a/"), 0.0);
        assert_eq!(gap("a/b", "/b"), 0.0);
        assert_eq!(gap("a:=b", ":="), 0.0);
        assert_eq!(gap("a:=b", "a:"), 5.0);
        assert_eq!(gap("3.14", "3."), 0.0);
    }

    #[test]
    fn relation_atoms_use_symbol_font_and_relation_spacing() {
        let node = MathNode::Row {
            children: vec![
                MathNode::Atom {
                    text: "a".to_string(),
                    atom_kind: MathAtomKind::Identifier,
                },
                MathNode::Atom {
                    text: "≤".to_string(),
                    atom_kind: MathAtomKind::Relation,
                },
                MathNode::Atom {
                    text: "b".to_string(),
                    atom_kind: MathAtomKind::Identifier,
                },
            ],
        };
        let math_font = FontRequest {
            family: FontFamilyRequest::Math,
            series: FontSeries::Regular,
            shape: FontShape::Upright,
            size_pt: 10.0,
            role: FontRole::Math,
        };

        let layout = layout_math_node(
            &node,
            10.0,
            &SourceProvenance::generated("test", "relation layout"),
            &math_font,
        );
        let runs = layout
            .ops
            .iter()
            .map(|op| match op {
                DrawOp::TextRun(run) => run,
                other => panic!("unexpected relation draw op: {other:?}"),
            })
            .collect::<Vec<_>>();

        assert_eq!(runs[1].font.family, FontFamilyRequest::Symbol);
        assert!(
            runs[1].origin.x > runs[0].origin.x + runs[0].approximate_advance_pt,
            "relation must have thick math spacing before it"
        );
        assert!(
            runs[2].origin.x > runs[1].origin.x + runs[1].approximate_advance_pt,
            "relation must have thick math spacing after it"
        );
    }
}
