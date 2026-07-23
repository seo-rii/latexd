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
            for (index, child) in children.iter().enumerate() {
                if allow_atom_spacing && index > 0 {
                    width_pt += math_atom_spacing_pt(&children[index - 1], child, size_pt);
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum MathSpacingClass {
    Ordinary,
    LargeOperator,
    Binary,
    Relation,
}

fn math_spacing_class(node: &MathNode) -> MathSpacingClass {
    match node {
        MathNode::Atom {
            atom_kind: MathAtomKind::Relation,
            ..
        } => MathSpacingClass::Relation,
        MathNode::Atom {
            atom_kind: MathAtomKind::Operator,
            ..
        } => MathSpacingClass::Binary,
        MathNode::LargeOperator { .. } => MathSpacingClass::LargeOperator,
        MathNode::Scripts { base, .. } => math_spacing_class(base),
        _ => MathSpacingClass::Ordinary,
    }
}

fn math_atom_spacing_pt(left: &MathNode, right: &MathNode, size_pt: f32) -> f32 {
    let left = math_spacing_class(left);
    let right = math_spacing_class(right);
    let mu_pt = size_pt / 18.0;
    if left == MathSpacingClass::Relation || right == MathSpacingClass::Relation {
        5.0 * mu_pt
    } else if left == MathSpacingClass::Binary || right == MathSpacingClass::Binary {
        4.0 * mu_pt
    } else if left == MathSpacingClass::LargeOperator || right == MathSpacingClass::LargeOperator {
        3.0 * mu_pt
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use tex_render_model::{
        DrawOp, FontFamilyRequest, FontRequest, FontRole, FontSeries, FontShape, MathAtomKind,
        MathNode, SourceProvenance,
    };

    use super::layout_math_node;

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
