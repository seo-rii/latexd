use std::collections::{BTreeMap, BTreeSet};

use tex_render_model::{PreparedPdfDictionaryEntry, PreparedPdfForm, PreparedPdfObject};

const MAX_PREPARED_PDF_DEPTH: usize = 64;

pub(crate) struct PdfFormObjects {
    pub root_object_id: usize,
    pub objects: Vec<Vec<u8>>,
    pub natural_width_pt: f32,
    pub natural_height_pt: f32,
}

pub(crate) fn build_pdf_form_objects(
    first_object_id: usize,
    form: &PreparedPdfForm,
) -> Option<PdfFormObjects> {
    if !form.is_complete() || first_object_id == 0 {
        return None;
    }
    let target_ids = form
        .objects
        .keys()
        .enumerate()
        .map(|(index, local_id)| Some((*local_id, first_object_id.checked_add(index)?)))
        .collect::<Option<BTreeMap<_, _>>>()?;
    let root_object_id = *target_ids.get(&form.root_object_id)?;
    let mut objects = Vec::with_capacity(form.objects.len());
    for (local_id, object) in &form.objects {
        let target_id = *target_ids.get(local_id)?;
        let mut bytes = format!("{target_id} 0 obj ").into_bytes();
        write_object(&mut bytes, object, &target_ids, 0)?;
        bytes.extend_from_slice(b"\nendobj\n");
        objects.push(bytes);
    }
    Some(PdfFormObjects {
        root_object_id,
        objects,
        natural_width_pt: form.natural_width_pt,
        natural_height_pt: form.natural_height_pt,
    })
}

fn write_object(
    output: &mut Vec<u8>,
    object: &PreparedPdfObject,
    target_ids: &BTreeMap<u32, usize>,
    depth: usize,
) -> Option<()> {
    if depth > MAX_PREPARED_PDF_DEPTH {
        return None;
    }
    match object {
        PreparedPdfObject::Null => output.extend_from_slice(b"null"),
        PreparedPdfObject::Boolean { value } => {
            output.extend_from_slice(if *value { b"true" } else { b"false" })
        }
        PreparedPdfObject::Integer { value } => {
            output.extend_from_slice(value.to_string().as_bytes())
        }
        PreparedPdfObject::Real { value } => {
            output.extend_from_slice(format_pdf_real(*value)?.as_bytes())
        }
        PreparedPdfObject::Name { value } => write_name(output, value),
        PreparedPdfObject::String { value } => write_hex_string(output, value),
        PreparedPdfObject::Array { values } => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(b' ');
                }
                write_object(output, value, target_ids, depth + 1)?;
            }
            output.push(b']');
        }
        PreparedPdfObject::Dictionary { entries } => {
            write_dictionary(output, entries, target_ids, depth + 1, None)?;
        }
        PreparedPdfObject::Stream { entries, data } => {
            write_dictionary(output, entries, target_ids, depth + 1, Some(data.len()))?;
            output.extend_from_slice(b" stream\n");
            output.extend_from_slice(data);
            output.extend_from_slice(b"\nendstream");
        }
        PreparedPdfObject::Reference { object_id } => {
            let target_id = target_ids.get(object_id)?;
            output.extend_from_slice(format!("{target_id} 0 R").as_bytes());
        }
    }
    Some(())
}

fn write_dictionary(
    output: &mut Vec<u8>,
    entries: &[PreparedPdfDictionaryEntry],
    target_ids: &BTreeMap<u32, usize>,
    depth: usize,
    stream_length: Option<usize>,
) -> Option<()> {
    let mut seen = BTreeSet::new();
    output.extend_from_slice(b"<<");
    for entry in entries {
        if entry.key == b"Length" && stream_length.is_some() {
            continue;
        }
        if !seen.insert(entry.key.as_slice()) {
            return None;
        }
        output.push(b' ');
        write_name(output, &entry.key);
        output.push(b' ');
        write_object(output, &entry.value, target_ids, depth + 1)?;
    }
    if let Some(length) = stream_length {
        output.extend_from_slice(format!(" /Length {length}").as_bytes());
    }
    output.extend_from_slice(b" >>");
    Some(())
}

fn write_name(output: &mut Vec<u8>, value: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    output.push(b'/');
    for byte in value {
        if matches!(*byte, b'!'..=b'~')
            && !matches!(
                *byte,
                b'#' | b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
            )
        {
            output.push(*byte);
        } else {
            output.push(b'#');
            output.push(HEX[(byte >> 4) as usize]);
            output.push(HEX[(byte & 0x0f) as usize]);
        }
    }
}

fn write_hex_string(output: &mut Vec<u8>, value: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    output.push(b'<');
    for byte in value {
        output.push(HEX[(byte >> 4) as usize]);
        output.push(HEX[(byte & 0x0f) as usize]);
    }
    output.push(b'>');
}

fn format_pdf_real(value: f32) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    let raw = value.to_string();
    let Some(exponent_index) = raw.find(['e', 'E']) else {
        return Some(if raw == "-0" { "0".to_string() } else { raw });
    };
    let exponent = raw[exponent_index + 1..].parse::<i32>().ok()?;
    let mantissa = &raw[..exponent_index];
    let negative = mantissa.starts_with('-');
    let unsigned = mantissa.trim_start_matches('-');
    let decimal_index = unsigned.find('.').unwrap_or(unsigned.len()) as i32;
    let digits = unsigned.replace('.', "");
    let shifted_decimal = decimal_index.checked_add(exponent)?;
    let mut formatted = if shifted_decimal <= 0 {
        format!(
            "0.{}{}",
            "0".repeat(shifted_decimal.unsigned_abs() as usize),
            digits
        )
    } else if shifted_decimal as usize >= digits.len() {
        format!(
            "{}{}",
            digits,
            "0".repeat(shifted_decimal as usize - digits.len())
        )
    } else {
        let split = shifted_decimal as usize;
        format!("{}.{}", &digits[..split], &digits[split..])
    };
    if negative && formatted != "0" {
        formatted.insert(0, '-');
    }
    Some(formatted)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tex_render_model::{PreparedPdfDictionaryEntry, PreparedPdfForm, PreparedPdfObject};

    use super::build_pdf_form_objects;

    fn root_entries(
        resources: PreparedPdfObject,
        mut extra: Vec<PreparedPdfDictionaryEntry>,
    ) -> Vec<PreparedPdfDictionaryEntry> {
        let mut entries = vec![
            PreparedPdfDictionaryEntry {
                key: b"Type".to_vec(),
                value: PreparedPdfObject::Name {
                    value: b"XObject".to_vec(),
                },
            },
            PreparedPdfDictionaryEntry {
                key: b"Subtype".to_vec(),
                value: PreparedPdfObject::Name {
                    value: b"Form".to_vec(),
                },
            },
            PreparedPdfDictionaryEntry {
                key: b"BBox".to_vec(),
                value: PreparedPdfObject::Array {
                    values: vec![
                        PreparedPdfObject::Integer { value: 0 },
                        PreparedPdfObject::Integer { value: 0 },
                        PreparedPdfObject::Integer { value: 1 },
                        PreparedPdfObject::Integer { value: 1 },
                    ],
                },
            },
            PreparedPdfDictionaryEntry {
                key: b"Resources".to_vec(),
                value: resources,
            },
        ];
        entries.append(&mut extra);
        entries
    }

    #[test]
    fn relocates_references_and_serializes_binary_values() {
        let form = PreparedPdfForm {
            root_object_id: 1,
            natural_width_pt: 100.0,
            natural_height_pt: 50.0,
            objects: BTreeMap::from([
                (
                    1,
                    PreparedPdfObject::Stream {
                        entries: root_entries(
                            PreparedPdfObject::Reference { object_id: 2 },
                            Vec::new(),
                        ),
                        data: b"0 0 1 1 re f".to_vec(),
                    },
                ),
                (
                    2,
                    PreparedPdfObject::Dictionary {
                        entries: vec![PreparedPdfDictionaryEntry {
                            key: b"Odd Name".to_vec(),
                            value: PreparedPdfObject::String {
                                value: vec![0, b')', 255],
                            },
                        }],
                    },
                ),
            ]),
        };

        let imported = build_pdf_form_objects(20, &form).expect("serialize form");

        assert_eq!(imported.root_object_id, 20);
        let text = String::from_utf8_lossy(&imported.objects.concat()).into_owned();
        assert!(text.contains("/Resources 21 0 R"));
        assert!(text.contains("/Odd#20Name <0029FF>"));
        assert!(text.contains("/Length 12"));
    }

    #[test]
    fn serializes_small_reals_without_exponent_or_precision_loss() {
        let form = PreparedPdfForm {
            root_object_id: 1,
            natural_width_pt: 1.0,
            natural_height_pt: 1.0,
            objects: BTreeMap::from([(
                1,
                PreparedPdfObject::Stream {
                    entries: root_entries(
                        PreparedPdfObject::Dictionary {
                            entries: Vec::new(),
                        },
                        vec![PreparedPdfDictionaryEntry {
                            key: b"Matrix".to_vec(),
                            value: PreparedPdfObject::Array {
                                values: vec![PreparedPdfObject::Real { value: 4.0e-9 }],
                            },
                        }],
                    ),
                    data: Vec::new(),
                },
            )]),
        };

        let imported = build_pdf_form_objects(5, &form).expect("serialize small real");
        let text = String::from_utf8_lossy(&imported.objects.concat()).into_owned();
        assert!(text.contains("/Matrix [0.000000004]"), "{text}");
        assert!(!text.contains("4e-9"), "{text}");
    }
}
