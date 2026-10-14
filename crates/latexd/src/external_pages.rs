use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use hmr_protocol::PagePreviewArtifact;
use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId, dictionary};

use crate::{compiler::PageArtifactMeta, viewer_prefixed_path};

/// Retain native painting and annotation appearance. Same-page destinations
/// remain local; cross-page GoTo links target the immutable original full PDF.
/// Unresolvable local destinations reject extraction for full-PDF passthrough.
pub(crate) fn extract_page_artifacts(
    pdf: &[u8],
    build_root: &Utf8Path,
    rev: u64,
    original_pdf_filename: &str,
) -> Result<(Vec<PageArtifactMeta>, Vec<PagePreviewArtifact>)> {
    if pdf.len() > 256 * 1024 * 1024 {
        bail!("external PDF exceeds the 256 MiB input budget");
    }
    let source = Document::load_mem_with_options(
        pdf,
        LoadOptions::with_max_decompressed_size(64 * 1024 * 1024),
    )?;
    if source.was_encrypted() || source.is_encrypted() {
        bail!("encrypted PDF page previews are unsupported");
    }
    let pages = source.get_pages();
    if pages.is_empty() || pages.len() > 4096 {
        bail!("external PDF must have between 1 and 4096 pages");
    }
    if original_pdf_filename.is_empty()
        || original_pdf_filename.contains(['/', '\\', '\0'])
        || matches!(original_pdf_filename, "." | "..")
    {
        bail!("original PDF artifact must be a filename in the revision directory");
    }
    let original_file = format!("../{original_pdf_filename}");
    let original_pdf_hash = blake3::hash(pdf);
    let page_indices = pages
        .values()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    let destinations = NamedDestinations::read(&source, &page_indices)?;
    let local_destinations = destinations.page_contexts();
    // These document-level dictionaries influence page painting even though
    // they are not reachable from the Page's Resources dictionary.
    let mut paint_context = Dictionary::new();
    for name in [b"OCProperties".as_slice(), b"OutputIntents", b"AcroForm"] {
        if let Ok(value) = source.catalog()?.get(name) {
            paint_context.set(name, value.clone());
        }
    }
    let page_dir = build_root.join(format!("rev-{rev}/pages"));
    fs::create_dir_all(&page_dir)?;
    let mut occurrences = BTreeMap::<String, usize>::new();
    let mut metadata = Vec::new();
    let mut artifacts = Vec::new();
    for (index, &id) in pages.values().enumerate() {
        let mut page = source.get_dictionary(id)?.clone();
        // This page's rewritten annotations, shadowing the source objects.
        let mut rewritten_objects = BTreeMap::new();
        for name in [b"MediaBox".as_slice(), b"CropBox", b"Resources", b"Rotate"] {
            if let Some(value) = inherited(&source, id, name)? {
                page.set(name, value);
            }
        }
        page.remove(b"Parent");
        let remote_links = rewrite_annotations(
            &source,
            &mut rewritten_objects,
            &mut page,
            id,
            &page_indices,
            &destinations,
            &original_file,
        )?;
        let mut page_context = paint_context.clone();
        if let Some(local) = local_destinations.get(&id) {
            for (key, value) in local.iter() {
                page_context.set(key.clone(), value.clone());
            }
        }
        let media = rect(&source, page.get(b"MediaBox")?)?;
        let bounds = if let Ok(crop) = page.get(b"CropBox") {
            let crop = rect(&source, crop)?;
            [
                media[0].max(crop[0]),
                media[1].max(crop[1]),
                media[2].min(crop[2]),
                media[3].min(crop[3]),
            ]
        } else {
            media
        };
        let unit = page
            .get(b"UserUnit")
            .ok()
            .map(|value| number(&source, value))
            .transpose()?
            .unwrap_or(1.0);
        let rotate = page
            .get(b"Rotate")
            .ok()
            .map(|value| number(&source, value))
            .transpose()?
            .unwrap_or(0.0);
        if unit <= 0.0 || rotate % 90.0 != 0.0 {
            bail!("invalid external PDF page units or rotation");
        }
        let (mut width, mut height) = (
            (bounds[2] - bounds[0]) * unit,
            (bounds[3] - bounds[1]) * unit,
        );
        if rotate.rem_euclid(180.0) == 90.0 {
            std::mem::swap(&mut width, &mut height);
        }
        if !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
            || width > 32768.0
            || height > 32768.0
        {
            bail!("external PDF page geometry is outside the preview budget");
        }
        // A standalone page uses the same visible box for PDF and raster views.
        // Contents/resources remain the original engine's, without semantic relayout.
        page.set(
            "MediaBox",
            bounds
                .into_iter()
                .map(|v| Object::Real(v as f32))
                .collect::<Vec<_>>(),
        );
        page.set(
            "CropBox",
            bounds
                .into_iter()
                .map(|v| Object::Real(v as f32))
                .collect::<Vec<_>>(),
        );
        let mut document = Document::with_version(source.version.clone());
        document.binary_mark = source.binary_mark.clone();
        document.reference_table.cross_reference_type = source.reference_table.cross_reference_type;
        document.max_id = source.max_id;
        document.objects =
            standalone_page_objects(&source, rewritten_objects, id, [&page, &page_context]);
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"latexd-external-page-v1\0");
        let mut seen = BTreeMap::from([(id, 0)]);
        hash_object(
            &document,
            &Object::Dictionary(page.clone()),
            &mut hasher,
            &mut seen,
            0,
        )?;
        hash_dictionary(&document, &page_context, &mut hasher, &mut seen, 0)?;
        if remote_links {
            // A cached page's relative GoToR file refers to its artifact owner.
            // Reuse is safe only when that owner's full PDF has identical bytes.
            hasher.update(b"original-full-pdf\0");
            hasher.update(original_pdf_hash.as_bytes());
        }
        let content_hash = hasher.finalize().to_hex().to_string();
        let occurrence = occurrences.entry(content_hash.clone()).or_default();
        let page_id = format!("external-{}-{}", &content_hash[..24], *occurrence);
        *occurrence += 1;

        let pages_id = document.new_object_id();
        page.set("Parent", pages_id);
        document.objects.insert(id, Object::Dictionary(page));
        document.objects.insert(pages_id, Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => vec![Object::Reference(id)], "Count" => 1}));
        let mut catalog = page_context;
        catalog.set("Type", "Catalog");
        catalog.set("Pages", pages_id);
        let catalog = document.add_object(catalog);
        document.trailer = dictionary! {"Root" => catalog};
        document.prune_objects();
        document.renumber_objects();
        let relative = Utf8PathBuf::from(format!("rev-{rev}/pages/{page_id}.pdf"));
        document
            .save(build_root.join(&relative))
            .context("failed to write external page PDF")?;
        artifacts.push(PagePreviewArtifact {
            page_id: page_id.clone(),
            pdf_url: viewer_prefixed_path(&format!("/artifacts/rev/{rev}/pages/{page_id}.pdf")),
            svg_url: None,
        });
        metadata.push(PageArtifactMeta {
            page_id,
            index,
            line_count: 0,
            width_pt: width.round().max(1.0) as u32,
            height_pt: height.round().max(1.0) as u32,
            natural_size_pt: Some([width, height]),
            content_hash,
            text_start_utf8: 0,
            text_end_utf8: 0,
            pdf_artifact_path: relative,
            source_spans: Vec::new(),
        });
    }
    fs::write(
        build_root.join(format!("rev-{rev}/page-metadata.json")),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    Ok((metadata, artifacts))
}

/// The objects a standalone page reaches from its dictionary and paint context,
/// with `rewritten` objects in place of the source's. The page's own id is left
/// out: the caller inserts the standalone page dictionary under it. Copying only
/// these, rather than cloning the whole source for every page, keeps extraction
/// linear in the document size; the result is what `prune_objects` would keep.
fn standalone_page_objects(
    source: &Document,
    mut rewritten: BTreeMap<ObjectId, Object>,
    page_id: ObjectId,
    roots: [&Dictionary; 2],
) -> BTreeMap<ObjectId, Object> {
    let mut objects = BTreeMap::new();
    let mut seen = BTreeSet::from([page_id]);
    let mut references = Vec::new();
    for root in roots {
        for (_, value) in root.iter() {
            push_references(value, &mut references);
        }
    }
    while let Some(id) = references.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(object) = rewritten
            .remove(&id)
            .or_else(|| source.objects.get(&id).cloned())
        else {
            continue;
        };
        push_references(&object, &mut references);
        objects.insert(id, object);
    }
    objects
}

fn push_references(object: &Object, references: &mut Vec<ObjectId>) {
    let mut stack = vec![object];
    while let Some(object) = stack.pop() {
        match object {
            Object::Reference(id) => references.push(*id),
            Object::Array(items) => stack.extend(items),
            Object::Dictionary(dictionary) => {
                stack.extend(dictionary.iter().map(|(_, value)| value))
            }
            Object::Stream(stream) => stack.extend(stream.dict.iter().map(|(_, value)| value)),
            _ => {}
        }
    }
}

struct NamedDestination {
    value: Object,
    page: ObjectId,
}

#[derive(Default)]
struct NamedDestinations {
    names: BTreeMap<Vec<u8>, NamedDestination>,
    legacy: BTreeMap<Vec<u8>, NamedDestination>,
}

impl NamedDestinations {
    fn read(source: &Document, pages: &BTreeMap<ObjectId, usize>) -> Result<Self> {
        let mut result = Self::default();
        let mut values = BTreeMap::new();
        let catalog = source.catalog()?;
        if let Ok(names) = catalog.get(b"Names") {
            let names = source.dereference(names)?.1.as_dict()?;
            if let Ok(tree) = names.get(b"Dests") {
                collect_destination_names(source, tree, &mut values, &mut BTreeSet::new(), 0)?;
            }
        }
        for (name, value) in values {
            let page = destination_page(source, &value, pages)?;
            result.names.insert(name, NamedDestination { value, page });
        }
        if let Ok(legacy) = catalog.get(b"Dests") {
            for (name, value) in source.dereference(legacy)?.1.as_dict()?.iter() {
                if result.names.len() + result.legacy.len() >= 100_000 {
                    bail!("external PDF destination entry budget exceeded");
                }
                let page = destination_page(source, value, pages)?;
                result.legacy.insert(
                    name.clone(),
                    NamedDestination {
                        value: value.clone(),
                        page,
                    },
                );
            }
        }
        Ok(result)
    }

    fn page_contexts(&self) -> BTreeMap<ObjectId, Dictionary> {
        let mut names = BTreeMap::<ObjectId, Vec<Object>>::new();
        let mut legacy = BTreeMap::<ObjectId, Dictionary>::new();
        for (name, destination) in &self.names {
            names.entry(destination.page).or_default().extend([
                Object::string_literal(name.clone()),
                destination.value.clone(),
            ]);
        }
        for (name, destination) in &self.legacy {
            legacy
                .entry(destination.page)
                .or_default()
                .set(name.clone(), destination.value.clone());
        }
        let mut contexts = BTreeMap::<ObjectId, Dictionary>::new();
        for (page, entries) in names {
            contexts.entry(page).or_default().set(
                "Names",
                dictionary! {"Dests" => dictionary! {"Names" => entries}},
            );
        }
        for (page, entries) in legacy {
            contexts.entry(page).or_default().set("Dests", entries);
        }
        contexts
    }
}

fn collect_destination_names(
    source: &Document,
    tree: &Object,
    names: &mut BTreeMap<Vec<u8>, Object>,
    visited: &mut BTreeSet<ObjectId>,
    depth: usize,
) -> Result<()> {
    if depth > 128 || visited.len() > 100_000 {
        bail!("external PDF destination name tree budget exceeded");
    }
    if let Object::Reference(id) = tree {
        if !visited.insert(*id) {
            bail!("cyclic or repeated external PDF destination name tree node");
        }
    }
    let tree = source.dereference(tree)?.1.as_dict()?;
    if let Ok(kids) = tree.get(b"Kids") {
        for child in source.dereference(kids)?.1.as_array()? {
            collect_destination_names(source, child, names, visited, depth + 1)?;
        }
    }
    if let Ok(entries) = tree.get(b"Names") {
        let entries = source.dereference(entries)?.1.as_array()?;
        if entries.len() % 2 != 0 {
            bail!("invalid external PDF destination name pairs");
        }
        for pair in entries.chunks_exact(2) {
            if names.len() >= 100_000 {
                bail!("external PDF destination entry budget exceeded");
            }
            if names
                .insert(pair[0].as_str()?.to_vec(), pair[1].clone())
                .is_some()
            {
                bail!("duplicate external PDF named destination");
            }
        }
    }
    Ok(())
}

fn destination_array<'a>(source: &'a Document, destination: &'a Object) -> Result<&'a Vec<Object>> {
    let mut destination = source.dereference(destination)?.1;
    if let Object::Dictionary(dictionary) = destination {
        destination = source.dereference(dictionary.get(b"D")?)?.1;
    }
    let values = destination.as_array()?;
    if values.len() < 2 {
        bail!("external PDF destination needs a page and view");
    }
    Ok(values)
}

fn destination_page(
    source: &Document,
    destination: &Object,
    pages: &BTreeMap<ObjectId, usize>,
) -> Result<ObjectId> {
    let page = destination_array(source, destination)?[0].as_reference()?;
    if !pages.contains_key(&page) {
        bail!("external PDF destination refers to a page outside its page tree");
    }
    Ok(page)
}

/// Returns a remote destination only when its target is another original page.
fn remote_destination(
    source: &Document,
    value: &Object,
    page: ObjectId,
    pages: &BTreeMap<ObjectId, usize>,
    named: &NamedDestinations,
) -> Result<Option<Object>> {
    let value = source.dereference(value)?.1;
    let named_value = match value {
        Object::Name(name) => Some(named.legacy.get(name)),
        Object::String(name, _) => Some(named.names.get(name)),
        _ => None,
    };
    if let Some(destination) = named_value {
        let destination = destination.context("unresolved external PDF named link")?;
        return Ok((destination.page != page).then(|| value.clone()));
    }
    let target = destination_page(source, value, pages)?;
    if target == page {
        return Ok(None);
    }
    let mut values = destination_array(source, value)?.clone();
    values[0] = Object::Integer(pages[&target] as i64);
    Ok(Some(Object::Array(values)))
}

fn remote_file_spec(path: &str) -> Dictionary {
    let mut unicode = vec![0xfe, 0xff];
    unicode.extend(path.encode_utf16().flat_map(u16::to_be_bytes));
    dictionary! {"Type" => "Filespec", "F" => Object::string_literal(path), "UF" => Object::string_literal(unicode)}
}

fn rewrite_action(
    source: &Document,
    action: &Object,
    page: ObjectId,
    pages: &BTreeMap<ObjectId, usize>,
    named: &NamedDestinations,
    original_file: &str,
    depth: usize,
    actions: &mut usize,
) -> Result<(Object, bool)> {
    *actions += 1;
    if depth > 128 || *actions > 100_000 {
        bail!("external PDF annotation action graph exceeds preview budget");
    }
    let mut action = source.dereference(action)?.1.as_dict()?.clone();
    let mut remote = false;
    if action.get(b"S").and_then(Object::as_name).ok() == Some(b"GoTo") {
        if let Some(destination) =
            remote_destination(source, action.get(b"D")?, page, pages, named)?
        {
            action.set("S", "GoToR");
            action.set("F", remote_file_spec(original_file));
            action.set("D", destination);
            remote = true;
        }
    }
    if let Ok(next) = action.get(b"Next") {
        let next = source.dereference(next)?.1;
        let replacement = if let Object::Array(next) = next {
            let mut rewritten = Vec::with_capacity(next.len());
            for next in next {
                let (next, changed) = rewrite_action(
                    source,
                    next,
                    page,
                    pages,
                    named,
                    original_file,
                    depth + 1,
                    actions,
                )?;
                remote |= changed;
                rewritten.push(next);
            }
            Object::Array(rewritten)
        } else {
            let (next, changed) = rewrite_action(
                source,
                next,
                page,
                pages,
                named,
                original_file,
                depth + 1,
                actions,
            )?;
            remote |= changed;
            next
        };
        action.set("Next", replacement);
    }
    Ok((Object::Dictionary(action), remote))
}

fn rewrite_additional_actions(
    source: &Document,
    target: &mut Dictionary,
    page: ObjectId,
    pages: &BTreeMap<ObjectId, usize>,
    named: &NamedDestinations,
    original_file: &str,
    actions: &mut usize,
) -> Result<bool> {
    let Ok(additional) = target.get(b"AA") else {
        return Ok(false);
    };
    let mut additional = source.dereference(additional)?.1.as_dict()?.clone();
    let keys = additional
        .iter()
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    let mut remote = false;
    for key in keys {
        let (action, changed) = rewrite_action(
            source,
            additional.get(&key)?,
            page,
            pages,
            named,
            original_file,
            0,
            actions,
        )?;
        remote |= changed;
        additional.set(key, action);
    }
    target.set("AA", additional);
    Ok(remote)
}

fn rewrite_annotations(
    source: &Document,
    rewritten_objects: &mut BTreeMap<ObjectId, Object>,
    page: &mut Dictionary,
    page_id: ObjectId,
    pages: &BTreeMap<ObjectId, usize>,
    named: &NamedDestinations,
    original_file: &str,
) -> Result<bool> {
    let mut actions = 0;
    let mut remote = rewrite_additional_actions(
        source,
        page,
        page_id,
        pages,
        named,
        original_file,
        &mut actions,
    )?;
    let Ok(annotations) = page.get(b"Annots") else {
        return Ok(remote);
    };
    let annotations = source.dereference(annotations)?.1.as_array()?.clone();
    if annotations.len() > 100_000 {
        bail!("external PDF annotation count exceeds preview budget");
    }
    let mut rewritten = Vec::with_capacity(annotations.len());
    for original in annotations {
        let (annotation_id, annotation) = source.dereference(&original)?;
        let mut annotation = annotation.as_dict()?.clone();
        if let Ok(destination) = annotation.get(b"Dest") {
            if annotation.get(b"A").is_ok() {
                bail!("external PDF annotation has ambiguous Dest and A entries");
            }
            if let Some(destination) =
                remote_destination(source, destination, page_id, pages, named)?
            {
                annotation.remove(b"Dest");
                annotation.set("A", dictionary! {"S" => "GoToR", "F" => remote_file_spec(original_file), "D" => destination});
                remote = true;
            }
        }
        if let Ok(action) = annotation.get(b"A") {
            let (action, changed) = rewrite_action(
                source,
                action,
                page_id,
                pages,
                named,
                original_file,
                0,
                &mut actions,
            )?;
            remote |= changed;
            annotation.set("A", action);
        }
        remote |= rewrite_additional_actions(
            source,
            &mut annotation,
            page_id,
            pages,
            named,
            original_file,
            &mut actions,
        )?;
        if let Some(id) = annotation_id {
            // AcroForm Fields may reference this same widget. Retain object
            // identity so the field tree and Page Annots observe one action.
            rewritten_objects.insert(id, Object::Dictionary(annotation));
            rewritten.push(original);
        } else {
            rewritten.push(Object::Dictionary(annotation));
        }
    }
    page.set("Annots", rewritten);
    Ok(remote)
}

fn inherited(source: &Document, id: ObjectId, key: &[u8]) -> Result<Option<Object>> {
    let mut current = id;
    let mut seen = BTreeSet::new();
    while seen.insert(current) {
        let dict = source.get_dictionary(current)?;
        if let Ok(value) = dict.get(key) {
            return Ok(Some(value.clone()));
        }
        let Ok(parent) = dict.get(b"Parent") else {
            return Ok(None);
        };
        current = parent.as_reference()?;
    }
    bail!("cyclic PDF page inheritance")
}

fn number(source: &Document, value: &Object) -> Result<f64> {
    let (_, value) = source.dereference(value)?;
    let result = match value {
        Object::Integer(v) => *v as f64,
        Object::Real(v) => f64::from(*v),
        _ => bail!("invalid PDF number"),
    };
    if !result.is_finite() {
        bail!("nonfinite PDF number");
    }
    Ok(result)
}

fn rect(source: &Document, value: &Object) -> Result<[f64; 4]> {
    let (_, value) = source.dereference(value)?;
    let values = value.as_array()?;
    if values.len() != 4 {
        bail!("invalid PDF page box");
    }
    Ok([
        number(source, &values[0])?,
        number(source, &values[1])?,
        number(source, &values[2])?,
        number(source, &values[3])?,
    ])
}

/// Hash reachable paint/resource data by traversal identity, not original PDF
/// object numbering. Extra annotation/metadata changes can cause conservative
/// misses; omitted font/image/graphics state must never create a false hit.
fn hash_object(
    source: &Document,
    object: &Object,
    hash: &mut blake3::Hasher,
    seen: &mut BTreeMap<ObjectId, u64>,
    depth: usize,
) -> Result<()> {
    if depth > 256 || seen.len() > 100_000 {
        bail!("external PDF resource graph exceeds preview budget");
    }
    match object {
        Object::Reference(id) => {
            if let Some(index) = seen.get(id) {
                hash.update(b"ref");
                hash.update(&index.to_le_bytes());
            } else {
                let index = seen.len() as u64;
                seen.insert(*id, index);
                hash.update(b"object");
                hash_object(source, source.get_object(*id)?, hash, seen, depth + 1)?;
            }
        }
        Object::Array(values) => {
            hash.update(b"array");
            hash.update(&(values.len() as u64).to_le_bytes());
            for value in values {
                hash_object(source, value, hash, seen, depth + 1)?;
            }
        }
        Object::Dictionary(dict) => hash_dictionary(source, dict, hash, seen, depth)?,
        Object::Stream(stream) => {
            hash.update(b"stream");
            // Preserve filter/decoding metadata with encoded bytes. Different
            // compression can miss, but identical compressed bytes are unambiguous.
            hash_dictionary(source, &stream.dict, hash, seen, depth)?;
            hash.update(&(stream.content.len() as u64).to_le_bytes());
            hash.update(&stream.content);
        }
        Object::Null => {
            hash.update(b"null");
        }
        Object::Boolean(value) => {
            hash.update(if *value { b"true" } else { b"false" });
        }
        Object::Integer(value) => {
            hash.update(b"integer");
            hash.update(&value.to_le_bytes());
        }
        Object::Real(value) => {
            hash.update(b"real");
            hash.update(&value.to_bits().to_le_bytes());
        }
        Object::Name(value) => {
            hash.update(b"name");
            hash.update(&(value.len() as u64).to_le_bytes());
            hash.update(value);
        }
        Object::String(value, _) => {
            hash.update(b"string");
            hash.update(&(value.len() as u64).to_le_bytes());
            hash.update(value);
        }
    }
    Ok(())
}

fn hash_dictionary(
    source: &Document,
    dict: &Dictionary,
    hash: &mut blake3::Hasher,
    seen: &mut BTreeMap<ObjectId, u64>,
    depth: usize,
) -> Result<()> {
    hash.update(b"dictionary");
    let entries = dict.iter().collect::<BTreeMap<_, _>>();
    hash.update(&(entries.len() as u64).to_le_bytes());
    for (key, value) in entries {
        hash.update(&(key.len() as u64).to_le_bytes());
        hash.update(key);
        hash_object(source, value, hash, seen, depth + 1)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Stream;

    #[test]
    fn external_page_hash_preserves_binary_strings_and_names() {
        let source = Document::new();
        let hash = |object| {
            let mut hash = blake3::Hasher::new();
            hash_object(&source, &object, &mut hash, &mut BTreeMap::new(), 0).unwrap();
            hash.finalize()
        };
        assert_ne!(
            hash(Object::Name(vec![0xfe])),
            hash(Object::Name(vec![0xff]))
        );
        assert_ne!(
            hash(Object::string_literal(vec![0xfe])),
            hash(Object::string_literal(vec![0xff]))
        );
        assert_ne!(
            hash(Object::Name(b"same".to_vec())),
            hash(Object::string_literal("same"))
        );
    }

    #[test]
    fn external_pages_preserve_and_hash_optional_content_visibility() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let mut source = fixture();
        let layer = source
            .add_object(dictionary! {"Type" => "OCG", "Name" => Object::string_literal("layer")});
        let properties = dictionary! {"OCGs" => vec![Object::Reference(layer)],
        "D" => dictionary! {"OFF" => vec![Object::Reference(layer)]}};
        source
            .catalog_mut()
            .unwrap()
            .set("OCProperties", properties);
        let mut bytes = Vec::new();
        source.save_to(&mut bytes).unwrap();
        let (first, _) = extract_page_artifacts(&bytes, root, 1, "main.pdf").unwrap();
        let standalone = Document::load(root.join(&first[0].pdf_artifact_path)).unwrap();
        let props = standalone
            .catalog()
            .unwrap()
            .get(b"OCProperties")
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(
            props
                .get(b"D")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"OFF")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        source
            .catalog_mut()
            .unwrap()
            .get_mut(b"OCProperties")
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("D", dictionary! {"ON" => vec![Object::Reference(layer)]});
        bytes.clear();
        source.save_to(&mut bytes).unwrap();
        let (second, _) = extract_page_artifacts(&bytes, root, 2, "main.pdf").unwrap();
        assert_ne!(first[0].content_hash, second[0].content_hash);
    }

    fn fixture() -> Document {
        let mut doc = Document::with_version("1.5");
        let pages = doc.new_object_id();
        let resources = doc.add_object(dictionary! {"Font" => dictionary! {"F1" => dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"}}});
        let mut kids = Vec::new();
        for text in ["Alpha", "Bravo"] {
            let content = doc.add_object(Stream::new(
                Dictionary::new(),
                format!("BT /F1 12 Tf 10 20 Td ({text}) Tj ET").into_bytes(),
            ));
            kids.push(Object::Reference(doc.add_object(
                dictionary! {"Type" => "Page", "Parent" => pages, "Contents" => content},
            )));
        }
        doc.objects.insert(pages, Object::Dictionary(dictionary! {"Type" => "Pages", "Count" => 2, "Kids" => kids, "MediaBox" => vec![0.into(),0.into(),400.into(),300.into()], "Resources" => resources}));
        let root = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
        doc.trailer.set("Root", root);
        doc
    }

    #[test]
    fn external_pages_preserve_page_local_named_destinations_from_kids_and_legacy_catalogs() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let mut source = fixture();
        let page_ids = source.get_pages().values().copied().collect::<Vec<_>>();
        let mut leaves = Vec::new();
        let mut legacy = Dictionary::new();
        for (index, page_id) in page_ids.iter().enumerate() {
            let name = format!("named-{index}");
            let legacy_name = format!("legacy-{index}");
            let destination = vec![
                Object::Reference(*page_id),
                Object::Name(b"XYZ".to_vec()),
                Object::Integer(10),
                Object::Integer(200),
                Object::Null,
            ];
            let value = source.add_object(dictionary! {"D" => destination.clone()});
            leaves.push(Object::Reference(source.add_object(dictionary! {
                "Names" => vec![Object::string_literal(name.clone()), Object::Reference(value)]
            })));
            legacy.set(legacy_name.as_bytes(), Object::Array(destination));
            source.get_dictionary_mut(*page_id).unwrap().set("Annots", vec![
                Object::Dictionary(dictionary! {"Type" => "Annot", "Subtype" => "Link",
                    "Rect" => vec![0.into(),0.into(),10.into(),10.into()], "Dest" => Object::string_literal(name)}),
                Object::Dictionary(dictionary! {"Type" => "Annot", "Subtype" => "Link",
                    "Rect" => vec![10.into(),0.into(),20.into(),10.into()],
                    "A" => dictionary! {"S" => "GoTo", "D" => Object::Name(legacy_name.into_bytes())}}),
                Object::Dictionary(dictionary! {"Type" => "Annot", "Subtype" => "Link",
                    "Rect" => vec![20.into(),0.into(),30.into(),10.into()],
                    "Dest" => Object::string_literal(format!("named-{}", 1 - index))}),
            ]);
        }
        let subtree = source.add_object(dictionary! {"Kids" => leaves});
        let tree = source.add_object(dictionary! {"Kids" => vec![Object::Reference(subtree)]});
        let names = source.add_object(dictionary! {"Dests" => tree});
        let legacy = source.add_object(legacy);
        source.catalog_mut().unwrap().set("Names", names);
        source.catalog_mut().unwrap().set("Dests", legacy);
        let mut bytes = Vec::new();
        source.save_to(&mut bytes).unwrap();
        let (pages, _) = extract_page_artifacts(&bytes, root, 1, "main.pdf").unwrap();
        for (index, page) in pages.iter().enumerate() {
            let extracted = Document::load(root.join(&page.pdf_artifact_path)).unwrap();
            let extracted_pages = extracted.get_pages();
            assert_eq!(extracted_pages.len(), 1);
            let page_id = extracted_pages[&1];
            let names = extracted
                .catalog()
                .unwrap()
                .get(b"Names")
                .expect("named destinations retained")
                .as_dict()
                .unwrap();
            let tree = names.get(b"Dests").unwrap().as_dict().unwrap();
            let entries = tree.get(b"Names").unwrap().as_array().unwrap();
            assert_eq!(
                entries.len(),
                2,
                "cross-page names are not exported as local targets"
            );
            assert_eq!(
                entries[0].as_str().unwrap(),
                format!("named-{index}").as_bytes()
            );
            let dest = extracted
                .dereference(&entries[1])
                .unwrap()
                .1
                .as_dict()
                .unwrap()
                .get(b"D")
                .unwrap()
                .as_array()
                .unwrap();
            assert_eq!(dest[0].as_reference().unwrap(), page_id);
            assert_eq!(dest[1].as_name().unwrap(), b"XYZ");
            assert_eq!(dest[2].as_i64().unwrap(), 10);
            assert_eq!(dest[3].as_i64().unwrap(), 200);
            let legacy = extracted
                .catalog()
                .unwrap()
                .get(b"Dests")
                .unwrap()
                .as_dict()
                .unwrap();
            assert_eq!(legacy.len(), 1);
            assert_eq!(
                legacy
                    .get(format!("legacy-{index}").as_bytes())
                    .unwrap()
                    .as_array()
                    .unwrap()[0]
                    .as_reference()
                    .unwrap(),
                page_id
            );
            let annotations = extracted
                .get_dictionary(page_id)
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap();
            assert_eq!(
                annotations.len(),
                3,
                "annotation paint and actions remain native"
            );
            assert!(annotations[0].as_dict().unwrap().get(b"Dest").is_ok());
            assert_eq!(
                annotations[1]
                    .as_dict()
                    .unwrap()
                    .get(b"A")
                    .unwrap()
                    .as_dict()
                    .unwrap()
                    .get(b"S")
                    .unwrap()
                    .as_name()
                    .unwrap(),
                b"GoTo"
            );
            let remote = annotations[2]
                .as_dict()
                .unwrap()
                .get(b"A")
                .unwrap()
                .as_dict()
                .unwrap();
            assert_eq!(remote.get(b"S").unwrap().as_name().unwrap(), b"GoToR");
            assert_eq!(
                remote.get(b"D").unwrap().as_str().unwrap(),
                format!("named-{}", 1 - index).as_bytes()
            );
            assert_eq!(
                remote
                    .get(b"F")
                    .unwrap()
                    .as_dict()
                    .unwrap()
                    .get(b"F")
                    .unwrap()
                    .as_str()
                    .unwrap(),
                b"../main.pdf"
            );
        }
    }

    #[test]
    fn external_cross_page_links_target_original_pdf_and_invalidate_with_its_content() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let mut source = fixture();
        let ids = source.get_pages();
        let destination = source.add_object(vec![
            Object::Reference(ids[&2]),
            Object::Name(b"FitH".to_vec()),
            200.into(),
        ]);
        source
            .catalog_mut()
            .unwrap()
            .set("Dests", dictionary! {"target" => destination});
        let annotation = dictionary! {"Type" => "Annot", "Subtype" => "Link",
        "Rect" => vec![1.into(),2.into(),30.into(),40.into()],
        "Border" => vec![0.into(),0.into(),3.into()],
        "Dest" => vec![Object::Reference(ids[&2]), Object::Name(b"FitH".to_vec()), 123.into()]};
        let action = dictionary! {"S" => "GoTo", "D" => Object::Name(b"target".to_vec()),
        "Next" => dictionary! {"S" => "GoTo", "D" => vec![Object::Reference(ids[&1]), Object::Name(b"Fit".to_vec())]}};
        source.get_dictionary_mut(ids[&1]).unwrap().set("Annots", vec![
            Object::Dictionary(annotation.clone()),
            Object::Dictionary(dictionary! {"Subtype" => "Link", "Rect" => vec![0.into(),0.into(),10.into(),10.into()], "A" => action}),
        ]);
        let mut bytes = Vec::new();
        source.save_to(&mut bytes).unwrap();
        let (first, _) = extract_page_artifacts(&bytes, root, 1, "original paper.pdf").unwrap();
        // The original artifact is retained in the same immutable owner revision.
        fs::write(root.join("rev-1/original paper.pdf"), &bytes).unwrap();
        let extracted = Document::load(root.join(&first[0].pdf_artifact_path)).unwrap();
        let page = extracted.get_dictionary(extracted.get_pages()[&1]).unwrap();
        let annotations = page.get(b"Annots").unwrap().as_array().unwrap();
        let first_annotation = annotations[0].as_dict().unwrap();
        assert_eq!(
            first_annotation.get(b"Rect").unwrap(),
            annotation.get(b"Rect").unwrap()
        );
        assert_eq!(
            first_annotation.get(b"Border").unwrap(),
            annotation.get(b"Border").unwrap()
        );
        let remote = first_annotation.get(b"A").unwrap().as_dict().unwrap();
        assert_eq!(remote.get(b"S").unwrap().as_name().unwrap(), b"GoToR");
        let target = remote.get(b"D").unwrap().as_array().unwrap();
        assert_eq!(
            target[0].as_i64().unwrap(),
            1,
            "GoToR page indices are zero-based"
        );
        assert_eq!(target[1].as_name().unwrap(), b"FitH");
        assert_eq!(target[2].as_i64().unwrap(), 123);
        let filename = remote
            .get(b"F")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"F")
            .unwrap()
            .as_str()
            .unwrap();
        let file = root
            .join(&first[0].pdf_artifact_path)
            .parent()
            .unwrap()
            .join(std::str::from_utf8(filename).unwrap());
        assert_eq!(fs::read(file).unwrap(), bytes);
        let second_action = annotations[1]
            .as_dict()
            .unwrap()
            .get(b"A")
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(
            second_action.get(b"S").unwrap().as_name().unwrap(),
            b"GoToR"
        );
        assert_eq!(
            second_action.get(b"D").unwrap().as_name().unwrap(),
            b"target"
        );
        let next = second_action.get(b"Next").unwrap().as_dict().unwrap();
        assert_eq!(next.get(b"S").unwrap().as_name().unwrap(), b"GoTo");
        assert_eq!(
            next.get(b"D").unwrap().as_array().unwrap()[0]
                .as_reference()
                .unwrap(),
            extracted.get_pages()[&1]
        );

        // Even a target PDF metadata change conservatively invalidates remote links.
        // Pages without rewritten links remain independent of that document identity.
        let info = source
            .add_object(dictionary! {"Producer" => Object::string_literal("changed target PDF")});
        source.trailer.set("Info", info);
        bytes.clear();
        source.save_to(&mut bytes).unwrap();
        let (second, _) = extract_page_artifacts(&bytes, root, 2, "original paper.pdf").unwrap();
        assert_ne!(first[0].content_hash, second[0].content_hash);
        assert_eq!(first[1].content_hash, second[1].content_hash);
    }

    #[test]
    fn external_destination_errors_request_full_pdf_passthrough() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let mut source = fixture();
        let tree = source.new_object_id();
        source.objects.insert(
            tree,
            Object::Dictionary(dictionary! {"Kids" => vec![Object::Reference(tree)]}),
        );
        source
            .catalog_mut()
            .unwrap()
            .set("Names", dictionary! {"Dests" => tree});
        let mut bytes = Vec::new();
        source.save_to(&mut bytes).unwrap();
        assert!(
            extract_page_artifacts(&bytes, root, 1, "main.pdf")
                .unwrap_err()
                .to_string()
                .contains("cyclic")
        );
        source.catalog_mut().unwrap().remove(b"Names");
        let page = source.get_pages()[&1];
        source.get_dictionary_mut(page).unwrap().set(
            "Annots",
            vec![Object::Dictionary(
                dictionary! {"Subtype" => "Link", "Dest" => Object::string_literal("missing")},
            )],
        );
        bytes.clear();
        source.save_to(&mut bytes).unwrap();
        assert!(
            extract_page_artifacts(&bytes, root, 2, "main.pdf")
                .unwrap_err()
                .to_string()
                .contains("unresolved")
        );
    }

    #[test]
    fn external_link_rewrite_retains_widget_identity_and_page_additional_actions() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let mut source = fixture();
        let ids = source.get_pages();
        let appearance = source.add_object(Stream::new(
            dictionary! {"Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(),0.into(),20.into(),10.into()]},
            b"0 0 20 10 re S".to_vec(),
        ));
        let widget = source.add_object(dictionary! {"Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn",
            "T" => Object::string_literal("button"), "P" => ids[&1],
            "Rect" => vec![0.into(),0.into(),20.into(),10.into()], "AP" => dictionary! {"N" => appearance},
            "A" => dictionary! {"S" => "GoTo", "D" => vec![Object::Reference(ids[&2]), Object::Name(b"Fit".to_vec())]}});
        source
            .get_dictionary_mut(ids[&1])
            .unwrap()
            .set("Annots", vec![Object::Reference(widget)]);
        source.catalog_mut().unwrap().set(
            "AcroForm",
            dictionary! {"Fields" => vec![Object::Reference(widget)]},
        );
        source.catalog_mut().unwrap().set("OutputIntents", vec![Object::Dictionary(
            dictionary! {"S" => "GTS_PDFA1", "OutputConditionIdentifier" => Object::string_literal("sRGB")}
        )]);
        source.get_dictionary_mut(ids[&2]).unwrap().set("AA", dictionary! {"O" => dictionary! {
            "S" => "GoTo", "D" => vec![Object::Reference(ids[&1]), Object::Name(b"Fit".to_vec())]
        }});
        let mut bytes = Vec::new();
        source.save_to(&mut bytes).unwrap();
        let (pages, _) = extract_page_artifacts(&bytes, root, 1, "main.pdf").unwrap();
        let first = Document::load(root.join(&pages[0].pdf_artifact_path)).unwrap();
        let page = first.get_dictionary(first.get_pages()[&1]).unwrap();
        let annotation = page.get(b"Annots").unwrap().as_array().unwrap()[0]
            .as_reference()
            .unwrap();
        let fields = first
            .catalog()
            .unwrap()
            .get(b"AcroForm")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Fields")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(
            fields[0].as_reference().unwrap(),
            annotation,
            "one widget is shared by page and field tree"
        );
        let widget = first.get_dictionary(annotation).unwrap();
        assert_eq!(
            widget
                .get(b"A")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"S")
                .unwrap()
                .as_name()
                .unwrap(),
            b"GoToR"
        );
        let appearance = widget
            .get(b"AP")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"N")
            .unwrap();
        assert_eq!(
            first
                .dereference(appearance)
                .unwrap()
                .1
                .as_stream()
                .unwrap()
                .content,
            b"0 0 20 10 re S"
        );
        assert_eq!(
            first
                .catalog()
                .unwrap()
                .get(b"OutputIntents")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let second = Document::load(root.join(&pages[1].pdf_artifact_path)).unwrap();
        let page = second.get_dictionary(second.get_pages()[&1]).unwrap();
        let action = page
            .get(b"AA")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"O")
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(action.get(b"S").unwrap().as_name().unwrap(), b"GoToR");
        assert_eq!(
            action.get(b"D").unwrap().as_array().unwrap()[0]
                .as_i64()
                .unwrap(),
            0
        );
    }

    #[test]
    fn external_pages_preserve_native_content_resources_and_stable_hashes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let mut source = fixture();
        let mut bytes = Vec::new();
        source.save_to(&mut bytes).unwrap();
        let (first, artifacts) = extract_page_artifacts(&bytes, root, 1, "main.pdf").unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(artifacts.len(), 2);
        for (index, page) in first.iter().enumerate() {
            assert_eq!((page.width_pt, page.height_pt), (400, 300));
            let extracted = Document::load(root.join(&page.pdf_artifact_path)).unwrap();
            assert_eq!(extracted.get_pages().len(), 1);
            assert_eq!(
                extracted.extract_text(&[1]).unwrap().trim(),
                ["Alpha", "Bravo"][index]
            );
        }
        source.renumber_objects_with(999);
        let info = source.add_object(
            dictionary! {"CreationDate" => Object::string_literal("a different build time")},
        );
        source.trailer.set("Info", info);
        bytes.clear();
        source.save_to(&mut bytes).unwrap();
        let (second, _) = extract_page_artifacts(&bytes, root, 2, "main.pdf").unwrap();
        assert_eq!(
            first
                .iter()
                .map(|p| (&p.content_hash, &p.page_id))
                .collect::<Vec<_>>(),
            second
                .iter()
                .map(|p| (&p.content_hash, &p.page_id))
                .collect::<Vec<_>>()
        );
        // Resource content is part of identity even when drawing operators do not change.
        for object in source.objects.values_mut() {
            if let Ok(dict) = object.as_dict_mut() {
                if let Ok(fonts) = dict.get_mut(b"Font").and_then(Object::as_dict_mut) {
                    fonts
                        .get_mut(b"F1")
                        .unwrap()
                        .as_dict_mut()
                        .unwrap()
                        .set("BaseFont", "Courier");
                }
            }
        }
        bytes.clear();
        source.save_to(&mut bytes).unwrap();
        let (third, _) = extract_page_artifacts(&bytes, root, 3, "main.pdf").unwrap();
        assert_ne!(second[0].content_hash, third[0].content_hash);
    }

    #[test]
    fn external_pages_use_visible_crop_rotation_and_units() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let mut source = fixture();
        let id = source.get_pages()[&1];
        let page = source.get_dictionary_mut(id).unwrap();
        page.set(
            "CropBox",
            vec![10.into(), 20.into(), 110.into(), 220.into()],
        );
        page.set("Rotate", 90);
        page.set("UserUnit", 2);
        let mut bytes = Vec::new();
        source.save_to(&mut bytes).unwrap();
        let (pages, _) = extract_page_artifacts(&bytes, root, 1, "main.pdf").unwrap();
        assert_eq!((pages[0].width_pt, pages[0].height_pt), (400, 200));
        let page = source.get_dictionary_mut(id).unwrap();
        page.set("Rotate", 45);
        bytes.clear();
        source.save_to(&mut bytes).unwrap();
        assert!(extract_page_artifacts(&bytes, root, 2, "main.pdf").is_err());
    }
}
