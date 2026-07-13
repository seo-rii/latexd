use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedPdfForm {
    pub root_object_id: u32,
    pub natural_width_pt: f32,
    pub natural_height_pt: f32,
    pub objects: BTreeMap<u32, PreparedPdfObject>,
}

impl PreparedPdfForm {
    pub fn is_complete(&self) -> bool {
        if self.root_object_id == 0
            || !self.natural_width_pt.is_finite()
            || self.natural_width_pt <= 0.0
            || !self.natural_height_pt.is_finite()
            || self.natural_height_pt <= 0.0
            || self.objects.keys().any(|object_id| *object_id == 0)
        {
            return false;
        }
        let Some(PreparedPdfObject::Stream { entries, .. }) =
            self.objects.get(&self.root_object_id)
        else {
            return false;
        };
        if !form_root_entries_are_valid(entries) {
            return false;
        }

        let mut reachable = BTreeSet::new();
        let mut pending = VecDeque::from([self.root_object_id]);
        while let Some(object_id) = pending.pop_front() {
            if !reachable.insert(object_id) {
                continue;
            }
            let Some(object) = self.objects.get(&object_id) else {
                return false;
            };
            let mut references = Vec::new();
            if !object.validate(true, 0, &mut references) {
                return false;
            }
            for reference in references {
                if reference == 0 || !self.objects.contains_key(&reference) {
                    return false;
                }
                pending.push_back(reference);
            }
        }
        reachable.len() == self.objects.len()
    }
}

fn form_root_entries_are_valid(entries: &[PreparedPdfDictionaryEntry]) -> bool {
    let value = |key: &[u8]| {
        entries
            .iter()
            .find(|entry| entry.key.as_slice() == key)
            .map(|entry| &entry.value)
    };
    matches!(value(b"Type"), Some(PreparedPdfObject::Name { value }) if value == b"XObject")
        && matches!(value(b"Subtype"), Some(PreparedPdfObject::Name { value }) if value == b"Form")
        && matches!(
            value(b"BBox"),
            Some(PreparedPdfObject::Array { values })
                if is_unit_bbox(values)
        )
        && matches!(
            value(b"Resources"),
            Some(PreparedPdfObject::Dictionary { .. } | PreparedPdfObject::Reference { .. })
        )
}

fn is_unit_bbox(values: &[PreparedPdfObject]) -> bool {
    if values.len() != 4 {
        return false;
    }
    values
        .iter()
        .zip([0.0_f32, 0.0, 1.0, 1.0])
        .all(|(value, expected)| match value {
            PreparedPdfObject::Integer { value } => (*value as f32 - expected).abs() < 0.000_001,
            PreparedPdfObject::Real { value } => {
                value.is_finite() && (*value - expected).abs() < 0.000_001
            }
            _ => false,
        })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PreparedPdfObject {
    Null,
    Boolean {
        value: bool,
    },
    Integer {
        value: i64,
    },
    Real {
        value: f32,
    },
    Name {
        value: Vec<u8>,
    },
    String {
        value: Vec<u8>,
    },
    Array {
        values: Vec<PreparedPdfObject>,
    },
    Dictionary {
        entries: Vec<PreparedPdfDictionaryEntry>,
    },
    Stream {
        entries: Vec<PreparedPdfDictionaryEntry>,
        data: Vec<u8>,
    },
    Reference {
        object_id: u32,
    },
}

impl PreparedPdfObject {
    fn validate(&self, top_level: bool, depth: usize, references: &mut Vec<u32>) -> bool {
        if depth > 64 {
            return false;
        }
        match self {
            Self::Array { values } => {
                for value in values {
                    if !value.validate(false, depth + 1, references) {
                        return false;
                    }
                }
            }
            Self::Dictionary { entries } | Self::Stream { entries, .. } => {
                if matches!(self, Self::Stream { .. }) && !top_level {
                    return false;
                }
                let mut keys = BTreeSet::new();
                for entry in entries {
                    if !keys.insert(entry.key.as_slice())
                        || !entry.value.validate(false, depth + 1, references)
                    {
                        return false;
                    }
                }
            }
            Self::Reference { object_id } => {
                if *object_id == 0 {
                    return false;
                }
                references.push(*object_id);
            }
            Self::Real { value } if !value.is_finite() => return false,
            Self::Null
            | Self::Boolean { .. }
            | Self::Integer { .. }
            | Self::Real { .. }
            | Self::Name { .. }
            | Self::String { .. } => {}
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedPdfDictionaryEntry {
    pub key: Vec<u8>,
    pub value: PreparedPdfObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedRasterFallback {
    pub format: crate::GraphicAssetFormat,
    pub bytes: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{PreparedPdfDictionaryEntry, PreparedPdfForm, PreparedPdfObject};

    fn valid_form() -> PreparedPdfForm {
        PreparedPdfForm {
            root_object_id: 1,
            natural_width_pt: 100.0,
            natural_height_pt: 50.0,
            objects: BTreeMap::from([(
                1,
                PreparedPdfObject::Stream {
                    entries: vec![
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
                            value: PreparedPdfObject::Dictionary {
                                entries: Vec::new(),
                            },
                        },
                    ],
                    data: Vec::new(),
                },
            )]),
        }
    }

    #[test]
    fn accepts_a_complete_form_root() {
        assert!(valid_form().is_complete());
    }

    #[test]
    fn rejects_missing_semantics_nested_streams_and_unreachable_objects() {
        let mut missing_subtype = valid_form();
        let Some(PreparedPdfObject::Stream { entries, .. }) = missing_subtype.objects.get_mut(&1)
        else {
            panic!("form root");
        };
        entries.retain(|entry| entry.key != b"Subtype");
        assert!(!missing_subtype.is_complete());

        let mut nested_stream = valid_form();
        let Some(PreparedPdfObject::Stream { entries, .. }) = nested_stream.objects.get_mut(&1)
        else {
            panic!("form root");
        };
        entries.push(PreparedPdfDictionaryEntry {
            key: b"Bad".to_vec(),
            value: PreparedPdfObject::Stream {
                entries: Vec::new(),
                data: Vec::new(),
            },
        });
        assert!(!nested_stream.is_complete());

        let mut unreachable = valid_form();
        unreachable.objects.insert(2, PreparedPdfObject::Null);
        assert!(!unreachable.is_complete());
    }
}
