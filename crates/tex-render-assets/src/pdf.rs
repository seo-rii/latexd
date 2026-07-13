use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId, Stream};
use tex_render_model::{
    GraphicAssetRequest, PreparedPdfDictionaryEntry, PreparedPdfForm, PreparedPdfObject,
};

const MAX_PDF_INPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_PDF_DECOMPRESSED_STREAM_BYTES: usize = 64 * 1024 * 1024;
const MAX_PDF_RESOURCE_STREAM_BYTES: usize = 128 * 1024 * 1024;
const MAX_PDF_RESOURCE_OBJECTS: usize = 20_000;
const MAX_PDF_OBJECT_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparePdfError(String);

impl PreparePdfError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for PreparePdfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PreparePdfError {}

pub fn prepare_pdf_form(
    request: &GraphicAssetRequest,
    bytes: &[u8],
) -> Result<PreparedPdfForm, PreparePdfError> {
    if bytes.len() > MAX_PDF_INPUT_BYTES {
        return Err(PreparePdfError::new(
            "PDF asset exceeds the direct-import size limit",
        ));
    }
    let document = Document::load_mem_with_options(
        bytes,
        LoadOptions::with_max_decompressed_size(MAX_PDF_DECOMPRESSED_STREAM_BYTES),
    )
    .map_err(|error| PreparePdfError::new(format!("failed to parse PDF asset: {error}")))?;
    if !supported_pdf_version(document.version.as_bytes()) {
        return Err(PreparePdfError::new(
            "PDF versions newer than 1.7 are not eligible for direct import",
        ));
    }
    let catalog = document
        .catalog()
        .map_err(|error| PreparePdfError::new(format!("invalid PDF catalog: {error}")))?;
    if let Ok(version) = catalog.get(b"Version") {
        let version = dereference(&document, version)?;
        let version = version
            .as_name()
            .map_err(|_| PreparePdfError::new("invalid PDF catalog Version"))?;
        if !supported_pdf_version(version) {
            return Err(PreparePdfError::new(
                "PDF versions newer than 1.7 are not eligible for direct import",
            ));
        }
    }
    if document.was_encrypted() || document.is_encrypted() {
        return Err(PreparePdfError::new(
            "encrypted PDF assets are not eligible for direct import",
        ));
    }
    if document.objects.len() > 100_000 {
        return Err(PreparePdfError::new(
            "PDF asset exceeds the loaded-object limit",
        ));
    }

    let requested_page = request
        .page_selection
        .as_ref()
        .and_then(|selection| selection.page)
        .unwrap_or(1);
    if requested_page == 0 {
        return Err(PreparePdfError::new("PDF page selection is one-based"));
    }
    let page_id = document
        .get_pages()
        .get(&requested_page)
        .copied()
        .ok_or_else(|| PreparePdfError::new(format!("PDF page {requested_page} does not exist")))?;

    let pagebox = request
        .page_selection
        .as_ref()
        .and_then(|selection| selection.pagebox.as_deref())
        .unwrap_or("mediabox");
    let page_box = selected_page_box(&document, page_id, pagebox)?;
    let rotate = inherited_integer(&document, page_id, b"Rotate")?.unwrap_or(0);
    let rotate = rotate.rem_euclid(360);
    if rotate % 90 != 0 {
        return Err(PreparePdfError::new(format!(
            "unsupported PDF page rotation {rotate}"
        )));
    }
    let user_unit = page_object(&document, page_id, b"UserUnit")?
        .as_ref()
        .map(|value| parse_number(&document, value))
        .transpose()?
        .unwrap_or(1.0);
    if !user_unit.is_finite() || !(0.0..=75_000.0).contains(&user_unit) || user_unit == 0.0 {
        return Err(PreparePdfError::new("invalid PDF UserUnit"));
    }

    let content = page_content(&document, page_id)?;
    let resources = inherited_object(&document, page_id, b"Resources")?
        .unwrap_or_else(|| Object::Dictionary(Dictionary::new()));
    let group = page_object(&document, page_id, b"Group")?;

    let mut collector = PdfObjectCollector::new(&document);
    let root_object_id = collector.reserve_object()?;
    let resources = collector.convert_object(&resources, 0)?;
    let group = group
        .as_ref()
        .map(|group| collector.convert_object(group, 0))
        .transpose()?;

    let width = page_box[2] - page_box[0];
    let height = page_box[3] - page_box[1];
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(PreparePdfError::new("invalid selected PDF page box"));
    }
    let (natural_width_pt, natural_height_pt) = if rotate % 180 == 0 {
        (width * user_unit, height * user_unit)
    } else {
        (height * user_unit, width * user_unit)
    };
    let matrix = normalized_page_matrix(page_box, rotate);
    let mut form_content = format!(
        "{} {} {} {} {} {} cm\n",
        matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], matrix[5]
    )
    .into_bytes();
    form_content.extend_from_slice(&content);

    let mut entries = vec![
        dictionary_entry(
            b"Type",
            PreparedPdfObject::Name {
                value: b"XObject".to_vec(),
            },
        ),
        dictionary_entry(
            b"Subtype",
            PreparedPdfObject::Name {
                value: b"Form".to_vec(),
            },
        ),
        dictionary_entry(b"FormType", PreparedPdfObject::Integer { value: 1 }),
        dictionary_entry(
            b"BBox",
            PreparedPdfObject::Array {
                values: vec![
                    PreparedPdfObject::Integer { value: 0 },
                    PreparedPdfObject::Integer { value: 0 },
                    PreparedPdfObject::Integer { value: 1 },
                    PreparedPdfObject::Integer { value: 1 },
                ],
            },
        ),
        dictionary_entry(b"Resources", resources),
    ];
    if let Some(group) = group {
        entries.push(dictionary_entry(b"Group", group));
    }
    entries.sort_by(|left, right| left.key.cmp(&right.key));
    collector.insert_reserved(
        root_object_id,
        PreparedPdfObject::Stream {
            entries,
            data: form_content,
        },
    )?;

    let form = PreparedPdfForm {
        root_object_id,
        natural_width_pt,
        natural_height_pt,
        objects: collector.finish(),
    };
    if !form.is_complete() {
        return Err(PreparePdfError::new(
            "prepared PDF Form graph is incomplete",
        ));
    }
    Ok(form)
}

fn dictionary_entry(key: &[u8], value: PreparedPdfObject) -> PreparedPdfDictionaryEntry {
    PreparedPdfDictionaryEntry {
        key: key.to_vec(),
        value,
    }
}

fn supported_pdf_version(version: &[u8]) -> bool {
    let Ok(version) = std::str::from_utf8(version) else {
        return false;
    };
    let Some((major, minor)) = version.trim_start_matches('/').split_once('.') else {
        return false;
    };
    let (Ok(major), Ok(minor)) = (major.parse::<u32>(), minor.parse::<u32>()) else {
        return false;
    };
    major == 1 && minor <= 7
}

fn selected_page_box(
    document: &Document,
    page_id: ObjectId,
    pagebox: &str,
) -> Result<[f32; 4], PreparePdfError> {
    let normalized = pagebox.trim().to_ascii_lowercase();
    let page_only_key: Option<&[u8]> = match normalized.as_str() {
        "mediabox" | "cropbox" => None,
        "bleedbox" => Some(b"BleedBox"),
        "trimbox" => Some(b"TrimBox"),
        "artbox" => Some(b"ArtBox"),
        _ => {
            return Err(PreparePdfError::new(format!(
                "unsupported PDF pagebox {pagebox}"
            )));
        }
    };
    if let Some(key) = page_only_key
        && let Some(value) = page_object(document, page_id, key)?
    {
        return parse_page_box(document, &value);
    }
    let inherited_candidates: &[&[u8]] = if normalized == "mediabox" {
        &[b"MediaBox"]
    } else {
        &[b"CropBox", b"MediaBox"]
    };
    for key in inherited_candidates {
        if let Some(value) = inherited_object(document, page_id, key)? {
            return parse_page_box(document, &value);
        }
    }
    Err(PreparePdfError::new("PDF page has no usable page box"))
}

fn inherited_integer(
    document: &Document,
    page_id: ObjectId,
    key: &[u8],
) -> Result<Option<i64>, PreparePdfError> {
    inherited_object(document, page_id, key)?
        .as_ref()
        .map(|value| match dereference(document, value)? {
            Object::Integer(value) => Ok(*value),
            _ => Err(PreparePdfError::new("expected a PDF integer")),
        })
        .transpose()
}

fn page_object(
    document: &Document,
    page_id: ObjectId,
    key: &[u8],
) -> Result<Option<Object>, PreparePdfError> {
    let page = document
        .get_dictionary(page_id)
        .map_err(|error| PreparePdfError::new(format!("invalid PDF page: {error}")))?;
    Ok(page.get(key).ok().cloned())
}

fn inherited_object(
    document: &Document,
    page_id: ObjectId,
    key: &[u8],
) -> Result<Option<Object>, PreparePdfError> {
    let mut current = page_id;
    let mut visited = BTreeSet::new();
    for _ in 0..MAX_PDF_OBJECT_DEPTH {
        if !visited.insert(current) {
            return Err(PreparePdfError::new("cycle in PDF page inheritance"));
        }
        let dictionary = document
            .get_dictionary(current)
            .map_err(|error| PreparePdfError::new(format!("invalid PDF page tree: {error}")))?;
        if let Ok(value) = dictionary.get(key) {
            return Ok(Some(value.clone()));
        }
        let Ok(parent) = dictionary.get(b"Parent").and_then(Object::as_reference) else {
            return Ok(None);
        };
        current = parent;
    }
    Err(PreparePdfError::new(
        "PDF page inheritance exceeds the depth limit",
    ))
}

fn parse_page_box(document: &Document, value: &Object) -> Result<[f32; 4], PreparePdfError> {
    let value = dereference(document, value)?;
    let values = value
        .as_array()
        .map_err(|_| PreparePdfError::new("PDF page box is not an array"))?;
    if values.len() != 4 {
        return Err(PreparePdfError::new(
            "PDF page box must contain four numbers",
        ));
    }
    Ok([
        parse_number(document, &values[0])?,
        parse_number(document, &values[1])?,
        parse_number(document, &values[2])?,
        parse_number(document, &values[3])?,
    ])
}

fn parse_number(document: &Document, value: &Object) -> Result<f32, PreparePdfError> {
    match dereference(document, value)? {
        Object::Integer(value) => Ok(*value as f32),
        Object::Real(value) => Ok(*value),
        _ => Err(PreparePdfError::new("expected a PDF number")),
    }
}

fn parse_positive_integer(document: &Document, value: &Object) -> Result<u64, PreparePdfError> {
    match dereference(document, value)? {
        Object::Integer(value) if *value > 0 => u64::try_from(*value)
            .map_err(|_| PreparePdfError::new("PDF integer exceeds the supported range")),
        _ => Err(PreparePdfError::new("expected a positive PDF integer")),
    }
}

fn dereference<'a>(
    document: &'a Document,
    mut value: &'a Object,
) -> Result<&'a Object, PreparePdfError> {
    let mut visited = BTreeSet::new();
    for _ in 0..MAX_PDF_OBJECT_DEPTH {
        let Object::Reference(object_id) = value else {
            return Ok(value);
        };
        if !visited.insert(*object_id) {
            return Err(PreparePdfError::new("cycle in PDF object references"));
        }
        value = document
            .objects
            .get(object_id)
            .ok_or_else(|| PreparePdfError::new("dangling PDF object reference"))?;
    }
    Err(PreparePdfError::new(
        "PDF object reference depth exceeds the limit",
    ))
}

fn page_content(document: &Document, page_id: ObjectId) -> Result<Vec<u8>, PreparePdfError> {
    let page = document
        .get_dictionary(page_id)
        .map_err(|error| PreparePdfError::new(format!("invalid PDF page: {error}")))?;
    let Ok(contents) = page.get(b"Contents") else {
        return Ok(Vec::new());
    };
    let mut output = Vec::new();
    let mut visited = BTreeSet::new();
    append_page_content(document, contents, &mut visited, &mut output, 0)?;
    Ok(output)
}

fn append_page_content(
    document: &Document,
    value: &Object,
    visited: &mut BTreeSet<ObjectId>,
    output: &mut Vec<u8>,
    depth: usize,
) -> Result<(), PreparePdfError> {
    if depth > MAX_PDF_OBJECT_DEPTH {
        return Err(PreparePdfError::new(
            "PDF page content exceeds the depth limit",
        ));
    }
    match value {
        Object::Reference(object_id) => {
            if !visited.insert(*object_id) {
                return Err(PreparePdfError::new("cycle in PDF page content"));
            }
            let referenced = document
                .objects
                .get(object_id)
                .ok_or_else(|| PreparePdfError::new("dangling PDF content reference"))?;
            append_page_content(document, referenced, visited, output, depth + 1)?;
            visited.remove(object_id);
        }
        Object::Array(values) => {
            for value in values {
                append_page_content(document, value, visited, output, depth + 1)?;
            }
        }
        Object::Stream(stream) => {
            let remaining = MAX_PDF_DECOMPRESSED_STREAM_BYTES.saturating_sub(output.len());
            let content = stream
                .decompressed_content_with_limit(remaining)
                .map_err(|error| {
                    PreparePdfError::new(format!("failed to decode PDF page content: {error}"))
                })?;
            if content.len() > remaining {
                return Err(PreparePdfError::new(
                    "PDF page content exceeds the size limit",
                ));
            }
            output.extend_from_slice(&content);
            output.push(b'\n');
        }
        _ => {
            return Err(PreparePdfError::new(
                "PDF Contents is not a stream or array",
            ));
        }
    }
    Ok(())
}

fn normalized_page_matrix(page_box: [f32; 4], rotate: i64) -> [f32; 6] {
    let [llx, lly, urx, ury] = page_box;
    let width = urx - llx;
    let height = ury - lly;
    match rotate {
        90 => [
            0.0,
            -1.0 / width,
            1.0 / height,
            0.0,
            -lly / height,
            urx / width,
        ],
        180 => [
            -1.0 / width,
            0.0,
            0.0,
            -1.0 / height,
            urx / width,
            ury / height,
        ],
        270 => [
            0.0,
            1.0 / width,
            -1.0 / height,
            0.0,
            ury / height,
            -llx / width,
        ],
        _ => [
            1.0 / width,
            0.0,
            0.0,
            1.0 / height,
            -llx / width,
            -lly / height,
        ],
    }
}

struct PdfObjectCollector<'a> {
    document: &'a Document,
    source_to_local: BTreeMap<ObjectId, u32>,
    objects: BTreeMap<u32, PreparedPdfObject>,
    next_object_id: u32,
    stream_bytes: usize,
}

impl<'a> PdfObjectCollector<'a> {
    fn new(document: &'a Document) -> Self {
        Self {
            document,
            source_to_local: BTreeMap::new(),
            objects: BTreeMap::new(),
            next_object_id: 1,
            stream_bytes: 0,
        }
    }

    fn reserve_object(&mut self) -> Result<u32, PreparePdfError> {
        if self.next_object_id as usize > MAX_PDF_RESOURCE_OBJECTS {
            return Err(PreparePdfError::new(
                "PDF resource graph exceeds the object limit",
            ));
        }
        let object_id = self.next_object_id;
        self.next_object_id += 1;
        Ok(object_id)
    }

    fn insert_reserved(
        &mut self,
        object_id: u32,
        object: PreparedPdfObject,
    ) -> Result<(), PreparePdfError> {
        if self.objects.insert(object_id, object).is_some() {
            return Err(PreparePdfError::new("duplicate prepared PDF object id"));
        }
        Ok(())
    }

    fn convert_object(
        &mut self,
        object: &Object,
        depth: usize,
    ) -> Result<PreparedPdfObject, PreparePdfError> {
        if depth > MAX_PDF_OBJECT_DEPTH {
            return Err(PreparePdfError::new(
                "PDF resource graph exceeds the depth limit",
            ));
        }
        match object {
            Object::Null => Ok(PreparedPdfObject::Null),
            Object::Boolean(value) => Ok(PreparedPdfObject::Boolean { value: *value }),
            Object::Integer(value) => Ok(PreparedPdfObject::Integer { value: *value }),
            Object::Real(value) if value.is_finite() => {
                Ok(PreparedPdfObject::Real { value: *value })
            }
            Object::Real(_) => Err(PreparePdfError::new("non-finite PDF real number")),
            Object::Name(value) => Ok(PreparedPdfObject::Name {
                value: value.clone(),
            }),
            Object::String(value, _) => Ok(PreparedPdfObject::String {
                value: value.clone(),
            }),
            Object::Array(values) => Ok(PreparedPdfObject::Array {
                values: values
                    .iter()
                    .map(|value| self.convert_object(value, depth + 1))
                    .collect::<Result<_, _>>()?,
            }),
            Object::Dictionary(dictionary) => Ok(PreparedPdfObject::Dictionary {
                entries: self.convert_dictionary(dictionary, depth + 1, false)?,
            }),
            Object::Stream(stream) => {
                reject_dangerous_dictionary(&stream.dict)?;
                if stream.dict.has(b"F")
                    || stream.dict.has(b"FFilter")
                    || stream.dict.has(b"FDecodeParms")
                {
                    return Err(PreparePdfError::new(
                        "external PDF streams are not importable",
                    ));
                }
                let decoded_size = self.resource_stream_decoded_size(stream)?;
                self.stream_bytes = self
                    .stream_bytes
                    .checked_add(decoded_size)
                    .ok_or_else(|| PreparePdfError::new("PDF resource stream size overflow"))?;
                if self.stream_bytes > MAX_PDF_RESOURCE_STREAM_BYTES {
                    return Err(PreparePdfError::new(
                        "PDF resource streams exceed the total size limit",
                    ));
                }
                Ok(PreparedPdfObject::Stream {
                    entries: self.convert_dictionary(&stream.dict, depth + 1, true)?,
                    data: stream.content.clone(),
                })
            }
            Object::Reference(source_id) => {
                if let Some(object_id) = self.source_to_local.get(source_id) {
                    return Ok(PreparedPdfObject::Reference {
                        object_id: *object_id,
                    });
                }
                let object_id = self.reserve_object()?;
                self.source_to_local.insert(*source_id, object_id);
                let source = self
                    .document
                    .objects
                    .get(source_id)
                    .cloned()
                    .ok_or_else(|| PreparePdfError::new("dangling PDF resource reference"))?;
                reject_non_resource_object(&source)?;
                let converted = self.convert_object(&source, depth + 1)?;
                self.insert_reserved(object_id, converted)?;
                Ok(PreparedPdfObject::Reference { object_id })
            }
        }
    }

    fn convert_dictionary(
        &mut self,
        dictionary: &Dictionary,
        depth: usize,
        stream_dictionary: bool,
    ) -> Result<Vec<PreparedPdfDictionaryEntry>, PreparePdfError> {
        reject_dangerous_dictionary(dictionary)?;
        let mut entries = dictionary
            .iter()
            .filter(|(key, _)| !(stream_dictionary && key.as_slice() == b"Length"))
            .map(|(key, value)| {
                Ok(PreparedPdfDictionaryEntry {
                    key: key.clone(),
                    value: self.convert_object(value, depth + 1)?,
                })
            })
            .collect::<Result<Vec<_>, PreparePdfError>>()?;
        entries.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(entries)
    }

    fn resource_stream_decoded_size(&self, stream: &Stream) -> Result<usize, PreparePdfError> {
        let filters = match stream.filters() {
            Ok(filters) => filters,
            Err(_) if !stream.dict.has(b"Filter") => Vec::new(),
            Err(error) => {
                return Err(PreparePdfError::new(format!(
                    "invalid PDF resource stream filters: {error}"
                )));
            }
        };
        if filters.is_empty()
            || filters.iter().all(|filter| {
                matches!(
                    *filter,
                    b"FlateDecode" | b"Fl" | b"LZWDecode" | b"LZW" | b"ASCII85Decode" | b"A85"
                )
            })
        {
            return stream
                .decompressed_content_with_limit(MAX_PDF_RESOURCE_STREAM_BYTES)
                .map(|content| content.len())
                .map_err(|error| {
                    PreparePdfError::new(format!(
                        "PDF resource stream exceeds or cannot satisfy the decode limit: {error}"
                    ))
                });
        }

        let subtype = stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|value| value.as_name().ok());
        let image_codec_count = filters
            .iter()
            .filter(|filter| {
                matches!(
                    **filter,
                    b"DCTDecode"
                        | b"DCT"
                        | b"JPXDecode"
                        | b"CCITTFaxDecode"
                        | b"CCF"
                        | b"JBIG2Decode"
                )
            })
            .count();
        let opaque_image_chain = subtype == Some(b"Image")
            && image_codec_count == 1
            && filters.iter().all(|filter| {
                matches!(
                    *filter,
                    b"DCTDecode"
                        | b"DCT"
                        | b"JPXDecode"
                        | b"CCITTFaxDecode"
                        | b"CCF"
                        | b"JBIG2Decode"
                        | b"ASCII85Decode"
                        | b"A85"
                        | b"ASCIIHexDecode"
                        | b"AHx"
                )
            });
        if !opaque_image_chain {
            return Err(PreparePdfError::new(
                "unsupported PDF resource stream filter chain",
            ));
        }
        let width = stream
            .dict
            .get(b"Width")
            .map_err(|_| PreparePdfError::new("PDF image resource has no Width"))?;
        let height = stream
            .dict
            .get(b"Height")
            .map_err(|_| PreparePdfError::new("PDF image resource has no Height"))?;
        let width = parse_positive_integer(self.document, width)?;
        let height = parse_positive_integer(self.document, height)?;
        width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(8))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .filter(|bytes| *bytes <= MAX_PDF_RESOURCE_STREAM_BYTES)
            .ok_or_else(|| {
                PreparePdfError::new("PDF image resource exceeds the decoded-size limit")
            })
    }

    fn finish(self) -> BTreeMap<u32, PreparedPdfObject> {
        self.objects
    }
}

fn reject_non_resource_object(object: &Object) -> Result<(), PreparePdfError> {
    let dictionary = match object {
        Object::Dictionary(dictionary) => Some(dictionary),
        Object::Stream(Stream { dict, .. }) => Some(dict),
        _ => None,
    };
    let Some(dictionary) = dictionary else {
        return Ok(());
    };
    let object_type = dictionary
        .get(b"Type")
        .ok()
        .and_then(|value| value.as_name().ok());
    if matches!(
        object_type,
        Some(b"Catalog" | b"Page" | b"Pages" | b"Outlines")
    ) {
        return Err(PreparePdfError::new(
            "PDF resource graph references document structure",
        ));
    }
    reject_dangerous_dictionary(dictionary)
}

fn reject_dangerous_dictionary(dictionary: &Dictionary) -> Result<(), PreparePdfError> {
    for key in [
        b"AA".as_slice(),
        b"OpenAction".as_slice(),
        b"JS".as_slice(),
        b"JavaScript".as_slice(),
        b"Launch".as_slice(),
        b"EF".as_slice(),
        b"EmbeddedFiles".as_slice(),
        b"RichMediaContent".as_slice(),
        b"RichMediaSettings".as_slice(),
        b"Rendition".as_slice(),
        b"Ref".as_slice(),
    ] {
        if dictionary.has(key) {
            return Err(PreparePdfError::new(
                "active or embedded PDF content is not importable",
            ));
        }
    }
    for (key, rejected_names) in [
        (
            b"Type".as_slice(),
            &[
                b"Action".as_slice(),
                b"Filespec".as_slice(),
                b"EmbeddedFile".as_slice(),
            ][..],
        ),
        (
            b"Subtype".as_slice(),
            &[
                b"RichMedia".as_slice(),
                b"Movie".as_slice(),
                b"Sound".as_slice(),
                b"FileAttachment".as_slice(),
                b"PS".as_slice(),
            ][..],
        ),
        (
            b"S".as_slice(),
            &[
                b"JavaScript".as_slice(),
                b"Launch".as_slice(),
                b"Rendition".as_slice(),
                b"GoToR".as_slice(),
                b"SubmitForm".as_slice(),
                b"ImportData".as_slice(),
            ][..],
        ),
    ] {
        let name = dictionary
            .get(key)
            .ok()
            .and_then(|value| value.as_name().ok());
        if name.is_some_and(|name| rejected_names.contains(&name)) {
            return Err(PreparePdfError::new(
                "active or embedded PDF content is not importable",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use lopdf::{Document, Object, Stream, dictionary};
    use tex_render_model::{
        GraphicAssetFormat, GraphicAssetRequest, GraphicPageSelection, PreparedPdfObject,
    };

    use super::normalized_page_matrix;
    use super::prepare_pdf_form;

    fn two_page_pdf() -> Vec<u8> {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let page_one_id = document.new_object_id();
        let page_two_id = document.new_object_id();
        let content_one =
            document.add_object(Stream::new(dictionary! {}, b"0 0 20 10 re f".to_vec()));
        let content_two =
            document.add_object(Stream::new(dictionary! {}, b"5 6 30 20 re f".to_vec()));
        document.set_object(
            page_one_id,
            dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 100.into()],
                "Contents" => content_one,
            },
        );
        document.set_object(
            page_two_id,
            dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "CropBox" => vec![10.into(), 20.into(), 160.into(), 100.into()],
                "Rotate" => 90,
                "UserUnit" => 2,
                "Contents" => content_two,
            },
        );
        document.set_object(
            pages_id,
            dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_one_id.into(), page_two_id.into()],
                "Count" => 2,
                "MediaBox" => vec![0.into(), 0.into(), 300.into(), 200.into()],
                "Resources" => dictionary! {},
            },
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).expect("serialize fixture PDF");
        bytes
    }

    #[test]
    fn prepares_selected_rotated_page_as_normalized_form() {
        let request = GraphicAssetRequest {
            asset_ref: "figures/two-pages.pdf".to_string(),
            source_format: Some(GraphicAssetFormat::Pdf),
            page_selection: Some(GraphicPageSelection {
                page: Some(2),
                pagebox: Some("cropbox".to_string()),
            }),
            asset_hash: None,
        };

        let form = prepare_pdf_form(&request, &two_page_pdf()).expect("prepare PDF form");

        assert_eq!(form.natural_width_pt, 160.0);
        assert_eq!(form.natural_height_pt, 300.0);
        assert!(form.is_complete());
        let root = form.objects.get(&form.root_object_id).expect("root form");
        let PreparedPdfObject::Stream { data, .. } = root else {
            panic!("root object is not a stream");
        };
        let text = String::from_utf8_lossy(data);
        assert!(text.contains("5 6 30 20"));
        assert!(!text.contains("0 0 20 10"));
    }

    #[test]
    fn rejects_missing_pages_and_unknown_pageboxes() {
        let mut request = GraphicAssetRequest::for_embedded_asset("figures/two-pages.pdf");
        request.source_format = Some(GraphicAssetFormat::Pdf);
        request.page_selection = Some(GraphicPageSelection {
            page: Some(3),
            pagebox: None,
        });
        assert!(prepare_pdf_form(&request, &two_page_pdf()).is_err());

        request.page_selection = Some(GraphicPageSelection {
            page: Some(1),
            pagebox: Some("unknownbox".to_string()),
        });
        assert!(prepare_pdf_form(&request, &two_page_pdf()).is_err());
    }

    #[test]
    fn rejects_pdf_2_and_active_resource_graphs() {
        let mut request = GraphicAssetRequest::for_embedded_asset("figures/page.pdf");
        request.source_format = Some(GraphicAssetFormat::Pdf);

        let mut pdf_2 = two_page_pdf();
        pdf_2[..8].copy_from_slice(b"%PDF-2.0");
        assert!(prepare_pdf_form(&request, &pdf_2).is_err());

        let mut active = Document::load_mem(&two_page_pdf()).expect("load fixture PDF");
        let page_id = *active.get_pages().get(&1).expect("first page");
        let parent_id = active
            .get_dictionary(page_id)
            .expect("page dictionary")
            .get(b"Parent")
            .and_then(Object::as_reference)
            .expect("page parent");
        let action_id = active.add_object(dictionary! {
            "Type" => "Action",
            "S" => "JavaScript",
            "JS" => Object::string_literal("app.alert('no')"),
        });
        active
            .get_dictionary_mut(parent_id)
            .expect("pages dictionary")
            .set(
                "Resources",
                dictionary! {
                    "XObject" => dictionary! { "Bad" => action_id },
                },
            );
        let mut active_bytes = Vec::new();
        active
            .save_to(&mut active_bytes)
            .expect("serialize active fixture");
        assert!(prepare_pdf_form(&request, &active_bytes).is_err());

        let mut indirect_version = Document::load_mem(&two_page_pdf()).expect("load fixture PDF");
        let version_id = indirect_version.add_object(Object::Name(b"2.0".to_vec()));
        indirect_version
            .catalog_mut()
            .expect("catalog")
            .set("Version", version_id);
        let mut indirect_version_bytes = Vec::new();
        indirect_version
            .save_to(&mut indirect_version_bytes)
            .expect("serialize indirect version fixture");
        assert!(prepare_pdf_form(&request, &indirect_version_bytes).is_err());

        let mut postscript = Document::load_mem(&two_page_pdf()).expect("load fixture PDF");
        let page_id = *postscript.get_pages().get(&1).expect("first page");
        let parent_id = postscript
            .get_dictionary(page_id)
            .expect("page dictionary")
            .get(b"Parent")
            .and_then(Object::as_reference)
            .expect("page parent");
        let postscript_id = postscript.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "PS" },
            b"executive".to_vec(),
        ));
        postscript
            .get_dictionary_mut(parent_id)
            .expect("pages dictionary")
            .set(
                "Resources",
                dictionary! { "XObject" => dictionary! { "Bad" => postscript_id } },
            );
        let mut postscript_bytes = Vec::new();
        postscript
            .save_to(&mut postscript_bytes)
            .expect("serialize PostScript fixture");
        assert!(prepare_pdf_form(&request, &postscript_bytes).is_err());

        let mut external_form = Document::load_mem(&two_page_pdf()).expect("load fixture PDF");
        let page_id = *external_form.get_pages().get(&1).expect("first page");
        let parent_id = external_form
            .get_dictionary(page_id)
            .expect("page dictionary")
            .get(b"Parent")
            .and_then(Object::as_reference)
            .expect("page parent");
        let external_form_id = external_form.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 1.into(), 1.into()],
                "Ref" => dictionary! { "F" => Object::string_literal("other.pdf"), "Page" => 0 },
            },
            Vec::new(),
        ));
        external_form
            .get_dictionary_mut(parent_id)
            .expect("pages dictionary")
            .set(
                "Resources",
                dictionary! { "XObject" => dictionary! { "Bad" => external_form_id } },
            );
        let mut external_form_bytes = Vec::new();
        external_form
            .save_to(&mut external_form_bytes)
            .expect("serialize external Form fixture");
        assert!(prepare_pdf_form(&request, &external_form_bytes).is_err());

        let mut oversized_image = Document::load_mem(&two_page_pdf()).expect("load fixture PDF");
        let page_id = *oversized_image.get_pages().get(&1).expect("first page");
        let parent_id = oversized_image
            .get_dictionary(page_id)
            .expect("page dictionary")
            .get(b"Parent")
            .and_then(Object::as_reference)
            .expect("page parent");
        let image_id = oversized_image.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 100_000,
                "Height" => 100_000,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            vec![0xff, 0xd8, 0xff, 0xd9],
        ));
        oversized_image
            .get_dictionary_mut(parent_id)
            .expect("pages dictionary")
            .set(
                "Resources",
                dictionary! { "XObject" => dictionary! { "Huge" => image_id } },
            );
        let mut oversized_image_bytes = Vec::new();
        oversized_image
            .save_to(&mut oversized_image_bytes)
            .expect("serialize oversized image fixture");
        assert!(prepare_pdf_form(&request, &oversized_image_bytes).is_err());
    }

    #[test]
    fn normalizes_every_quarter_turn_into_the_unit_form_box() {
        let page_box = [10.0, 20.0, 110.0, 220.0];
        let corners = [(10.0, 20.0), (110.0, 20.0), (10.0, 220.0), (110.0, 220.0)];
        for rotation in [0, 90, 180, 270] {
            let [a, b, c, d, e, f] = normalized_page_matrix(page_box, rotation);
            let transformed = corners.map(|(x, y)| (a * x + c * y + e, b * x + d * y + f));
            let min_x = transformed
                .iter()
                .map(|point| point.0)
                .fold(f32::INFINITY, f32::min);
            let max_x = transformed
                .iter()
                .map(|point| point.0)
                .fold(f32::NEG_INFINITY, f32::max);
            let min_y = transformed
                .iter()
                .map(|point| point.1)
                .fold(f32::INFINITY, f32::min);
            let max_y = transformed
                .iter()
                .map(|point| point.1)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!((min_x - 0.0).abs() < 0.000_01, "rotation {rotation}");
            assert!((max_x - 1.0).abs() < 0.000_01, "rotation {rotation}");
            assert!((min_y - 0.0).abs() < 0.000_01, "rotation {rotation}");
            assert!((max_y - 1.0).abs() < 0.000_01, "rotation {rotation}");
        }
    }
}
