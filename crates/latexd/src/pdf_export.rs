//! Lazy whole-document PDF export from immutable, self-contained page PDFs.
use std::{
    collections::BTreeMap,
    fs,
    sync::{Arc, Mutex, OnceLock, Weak},
};

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use lopdf::{Document, LoadOptions, Object, dictionary};
use serde::{Deserialize, Serialize};

use crate::compiler::PageArtifactMeta;

const MAX_INPUT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_OUTPUT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_OBJECTS: usize = 200_000;

struct BoundedWriter<W> {
    inner: W,
    remaining: u64,
}

impl<W: std::io::Write> std::io::Write for BoundedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(std::io::Error::other(
                "PDF export exceeds output byte budget",
            ));
        }
        let written = self.inner.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn object_payload_bytes(object: &Object, depth: usize) -> Result<u64> {
    if depth > 128 {
        bail!("PDF export object nesting limit exceeded");
    }
    let size = match object {
        Object::Array(items) => items
            .iter()
            .map(|item| object_payload_bytes(item, depth + 1))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .sum(),
        Object::Dictionary(dict) => dict
            .iter()
            .map(|(key, value)| Ok(key.len() as u64 + object_payload_bytes(value, depth + 1)?))
            .collect::<Result<Vec<u64>>>()?
            .into_iter()
            .sum(),
        Object::Stream(stream) => {
            stream.content.len() as u64
                + object_payload_bytes(&Object::Dictionary(stream.dict.clone()), depth + 1)?
        }
        Object::String(bytes, _) | Object::Name(bytes) => bytes.len() as u64,
        _ => 16,
    };
    if size > MAX_OUTPUT_BYTES {
        bail!("PDF export decoded resource budget exceeded");
    }
    Ok(size)
}

fn collect_names(
    document: &Document,
    tree: &Object,
    names: &mut Vec<(Vec<u8>, Object)>,
    depth: usize,
) -> Result<()> {
    if depth > 128 {
        bail!("PDF destination name tree depth exceeded");
    }
    let tree = document.dereference(tree)?.1.as_dict()?;
    if let Ok(kids) = tree.get(b"Kids") {
        for kid in document.dereference(kids)?.1.as_array()? {
            collect_names(document, kid, names, depth + 1)?;
        }
    }
    if let Ok(entries) = tree.get(b"Names") {
        let entries = document.dereference(entries)?.1.as_array()?;
        if entries.len() % 2 != 0 {
            bail!("invalid PDF destination name tree");
        }
        for pair in entries.chunks_exact(2) {
            names.push((pair[0].as_str()?.to_vec(), pair[1].clone()));
            if names.len() > MAX_OBJECTS {
                bail!("PDF destination entry limit exceeded");
            }
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Recipe {
    version: u32,
    rev: u64,
    target: Utf8PathBuf,
    pages: Vec<Utf8PathBuf>,
}

fn relative(build_root: &Utf8Path, path: &Utf8Path) -> Result<Utf8PathBuf> {
    let relative = path.strip_prefix(build_root)?;
    if relative.as_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, camino::Utf8Component::Normal(_)))
    {
        bail!("invalid PDF export artifact path");
    }
    Ok(relative.to_owned())
}

fn recipe_path(target: &Utf8Path) -> Result<Utf8PathBuf> {
    Ok(target
        .parent()
        .context("PDF target has no revision directory")?
        .join("pdf-export.json"))
}

fn read_recipe(build_root: &Utf8Path, target: &Utf8Path) -> Result<Recipe> {
    let recipe: Recipe = serde_json::from_slice(&fs::read(recipe_path(target)?)?)?;
    if recipe.version != 1
        || recipe.target != relative(build_root, target)?
        || recipe.target.parent() != Some(Utf8Path::new(&format!("rev-{}", recipe.rev)))
        || recipe.pages.len() > 4096
    {
        bail!("invalid PDF export recipe");
    }
    let canonical_root = fs::canonicalize(build_root)?;
    if !fs::canonicalize(target.parent().context("missing export directory")?)?
        .starts_with(&canonical_root)
    {
        bail!("PDF export target is outside the artifact store");
    }
    let mut total_input_bytes = 0u64;
    for page in &recipe.pages {
        let path = build_root.join(page);
        if relative(build_root, &path)? != *page
            || !path.is_file()
            || !fs::canonicalize(&path)?.starts_with(&canonical_root)
        {
            bail!("PDF export page is missing or outside the artifact store: {page}");
        }
        total_input_bytes = total_input_bytes.saturating_add(fs::metadata(&path)?.len());
        if total_input_bytes > MAX_INPUT_BYTES {
            bail!("PDF export exceeds total input byte budget");
        }
    }
    Ok(recipe)
}

pub(crate) fn write_recipe(
    build_root: &Utf8Path,
    rev: u64,
    pdf_path: &Utf8Path,
    pages: &[PageArtifactMeta],
) -> Result<()> {
    let recipe = Recipe {
        version: 1,
        rev,
        target: relative(build_root, pdf_path)?,
        pages: pages
            .iter()
            .map(|page| page.pdf_artifact_path.clone())
            .collect(),
    };
    let path = recipe_path(pdf_path)?;
    fs::write(path, serde_json::to_vec(&recipe)?)?;
    read_recipe(build_root, pdf_path)?;
    Ok(())
}

pub(crate) fn validated_recipe(build_root: &Utf8Path, target: &Utf8Path) -> Result<bool> {
    if !recipe_path(target)?.is_file() {
        return Ok(false);
    }
    read_recipe(build_root, target)?;
    Ok(true)
}

fn export(build_root: &Utf8Path, target: &Utf8Path) -> Result<()> {
    let recipe = read_recipe(build_root, target)?;
    let mut output = Document::with_version("1.7");
    let root_pages = output.new_object_id();
    let mut kids = Vec::new();
    let mut named_destinations = Vec::new();
    let mut legacy_destinations = BTreeMap::new();
    let mut decoded_bytes = 0u64;
    for page_path in &recipe.pages {
        let path = build_root.join(page_path);
        let mut page = Document::load_mem_with_options(
            &fs::read(&path)?,
            LoadOptions::with_max_decompressed_size(64 * 1024 * 1024),
        )?;
        if page.is_encrypted() || page.was_encrypted() || page.get_pages().len() != 1 {
            bail!("export requires a self-contained, unencrypted single-page artifact");
        }
        if output
            .objects
            .len()
            .saturating_add(page.objects.len())
            .saturating_add(2)
            > MAX_OBJECTS
        {
            bail!("PDF export total object limit exceeded");
        }
        for object in page.objects.values() {
            decoded_bytes = decoded_bytes.saturating_add(object_payload_bytes(object, 0)?);
            if decoded_bytes > MAX_OUTPUT_BYTES {
                bail!("PDF export decoded resource budget exceeded");
            }
        }
        page.renumber_objects_with(
            output
                .max_id
                .checked_add(1)
                .context("PDF object limit exceeded")?,
        );
        let page_root = page.catalog()?.get(b"Pages")?.as_reference()?;
        if let Ok(names) = page.catalog()?.get(b"Names") {
            let names = page.dereference(names)?.1.as_dict()?;
            if let Ok(destinations) = names.get(b"Dests") {
                collect_names(&page, destinations, &mut named_destinations, 0)?;
            }
        }
        if let Ok(destinations) = page.catalog()?.get(b"Dests") {
            for (name, destination) in page.dereference(destinations)?.1.as_dict()?.iter() {
                legacy_destinations
                    .entry(name.clone())
                    .or_insert_with(|| destination.clone());
            }
        }
        // Graft the entire page-tree subtree. Keeping each original parent chain
        // preserves inherited MediaBox, crop, rotation, and resource dictionaries.
        page.get_dictionary_mut(page_root)?
            .set("Parent", root_pages);
        kids.push(Object::Reference(page_root));
        output.max_id = page.max_id;
        output.objects.extend(page.objects);
    }
    output.objects.insert(
        root_pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => recipe.pages.len() as i64
        }),
    );
    let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => root_pages };
    if !named_destinations.is_empty() {
        // Name tree keys must be unique: keep the first page's entry for a
        // repeated name, as the legacy `/Dests` merge does (the sort is stable).
        named_destinations.sort_by(|left, right| left.0.cmp(&right.0));
        named_destinations.dedup_by(|later, earlier| later.0 == earlier.0);
        let entries = named_destinations
            .into_iter()
            .flat_map(|(name, destination)| [Object::string_literal(name), destination])
            .collect::<Vec<_>>();
        catalog.set(
            "Names",
            dictionary! { "Dests" => dictionary! { "Names" => entries } },
        );
    }
    if !legacy_destinations.is_empty() {
        let mut destinations = lopdf::Dictionary::new();
        for (name, destination) in legacy_destinations {
            destinations.set(name, destination);
        }
        catalog.set("Dests", destinations);
    }
    let catalog = output.add_object(catalog);
    output.trailer.set("Root", catalog);
    output.prune_objects();
    output.renumber_objects();
    let pending = target.with_extension("pdf.exporting");
    let result = (|| {
        let mut file = fs::File::create(&pending)?;
        output.save_to(&mut BoundedWriter {
            inner: &mut file,
            remaining: MAX_OUTPUT_BYTES,
        })?;
        file.sync_all()?;
        fs::rename(&pending, target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(pending);
    }
    result
}

pub(crate) async fn ensure_pdf(build_root: &Utf8Path, target: &Utf8Path) -> Result<()> {
    let target_relative = relative(build_root, target)?;
    let rev = target_relative
        .parent()
        .and_then(Utf8Path::file_name)
        .and_then(|name| name.strip_prefix("rev-"))
        .context("invalid PDF export revision")?
        .parse::<u64>()?;
    let pin = crate::preview_leases::pin_job(build_root, rev).await;
    if target.is_file() {
        return Ok(());
    }
    read_recipe(build_root, target)?;
    static EXPORTS: OnceLock<Mutex<BTreeMap<Utf8PathBuf, Weak<tokio::sync::Mutex<()>>>>> =
        OnceLock::new();
    let lock = {
        let mut exports = EXPORTS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        exports.retain(|_, lock| lock.strong_count() > 0);
        match exports.get(target).and_then(Weak::upgrade) {
            Some(lock) => lock,
            None => {
                let lock = Arc::new(tokio::sync::Mutex::new(()));
                exports.insert(target.to_owned(), Arc::downgrade(&lock));
                lock
            }
        }
    };
    let guard = lock.lock_owned().await;
    if target.is_file() {
        return Ok(());
    }
    let build_root = build_root.to_owned();
    let target = target.to_owned();
    tokio::task::spawn_blocking(move || {
        // Export continues safely even if a download request is cancelled.
        let (_pin, _guard) = (pin, guard);
        export(&build_root, &target)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{CompileRequest, CompilerDriver};

    fn page(root: &Utf8Path, rev: u64, text: &str, font: &str) -> PageArtifactMeta {
        let mut document = Document::with_version("1.7");
        let parent = document.new_object_id();
        let resources = document.add_object(dictionary! {
            "Font" => dictionary! { "F1" => dictionary! {
                "Type" => "Font", "Subtype" => "Type1", "BaseFont" => font,
            }}
        });
        let contents = document.add_object(lopdf::Stream::new(
            dictionary! {},
            format!("BT /F1 12 Tf 10 20 Td ({text}) Tj ET").into_bytes(),
        ));
        let child = document.add_object(
            dictionary! { "Type" => "Page", "Parent" => parent, "Contents" => contents },
        );
        document.objects.insert(
            parent,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Count" => 1, "Kids" => vec![Object::Reference(child)],
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 300.into()],
                "Rotate" => 90, "Resources" => resources,
            }),
        );
        let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => parent });
        document.trailer.set("Root", catalog);
        let relative = Utf8PathBuf::from(format!("rev-{rev}/pages/{text}.pdf"));
        fs::create_dir_all(root.join(relative.parent().unwrap())).unwrap();
        document.save(root.join(&relative)).unwrap();
        PageArtifactMeta {
            page_id: text.into(),
            index: 0,
            line_count: 1,
            width_pt: 300,
            height_pt: 200,
            natural_size_pt: None,
            content_hash: text.into(),
            text_start_utf8: 0,
            text_end_utf8: 1,
            pdf_artifact_path: relative,
            source_spans: vec![],
        }
    }

    #[tokio::test]
    async fn pdf_export_preserves_page_order_inherited_resources_and_reused_owners() {
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();
        let first = page(root, 1, "First", "Helvetica");
        let second = page(root, 3, "Second", "Courier");
        let target = root.join("rev-3/main.pdf");
        write_recipe(root, 3, &target, &[second, first]).unwrap();
        assert!(!target.exists(), "recipe creation must not export");
        assert!(validated_recipe(root, &target).unwrap());
        let mut requests = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let root = root.to_owned();
            let target = target.clone();
            requests.spawn(async move { ensure_pdf(&root, &target).await });
        }
        while let Some(result) = requests.join_next().await {
            result.unwrap().unwrap();
        }
        let result = Document::load(&target).unwrap();
        assert_eq!(result.get_pages().len(), 2);
        assert_eq!(result.extract_text(&[1]).unwrap().trim(), "Second");
        assert_eq!(result.extract_text(&[2]).unwrap().trim(), "First");
        for ((_, page), font) in result
            .get_pages()
            .iter()
            .zip([b"Courier".as_slice(), b"Helvetica"])
        {
            assert_eq!(
                result.get_page_fonts(*page).unwrap()[b"F1".as_slice()]
                    .get(b"BaseFont")
                    .unwrap()
                    .as_name()
                    .unwrap(),
                font
            );
            let parent = result
                .get_dictionary(*page)
                .unwrap()
                .get(b"Parent")
                .unwrap()
                .as_reference()
                .unwrap();
            let inherited = result.get_dictionary(parent).unwrap();
            assert_eq!(inherited.get(b"Rotate").unwrap().as_i64().unwrap(), 90);
            assert_eq!(
                inherited.get(b"MediaBox").unwrap().as_array().unwrap()[2]
                    .as_i64()
                    .unwrap(),
                200
            );
        }
        let bytes = fs::read(&target).unwrap();
        ensure_pdf(root, &target).await.unwrap();
        assert_eq!(fs::read(&target).unwrap(), bytes);
        assert!(!target.with_extension("pdf.exporting").exists());
    }

    #[tokio::test]
    async fn pdf_export_failed_merge_does_not_publish_partial_pdf() {
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();
        let artifact = page(root, 1, "First", "Helvetica");
        let target = root.join("rev-1/main.pdf");
        write_recipe(root, 1, &target, std::slice::from_ref(&artifact)).unwrap();
        fs::write(root.join(artifact.pdf_artifact_path), "corrupt PDF").unwrap();
        assert!(ensure_pdf(root, &target).await.is_err());
        assert!(!target.exists());
        assert!(!target.with_extension("pdf.exporting").exists());
    }

    #[tokio::test]
    async fn pdf_export_preserves_forward_and_back_named_link_destinations() {
        use tex_render_model::{
            Destination, DrawOp, LinkAnnotation, PageDisplayList, Point, Rect, SourceProvenance,
        };
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();
        let pages = [("first", "second"), ("second", "first")]
            .into_iter()
            .map(|(name, target)| PageDisplayList {
                page_id: name.into(),
                width_pt: 612.0,
                height_pt: 792.0,
                content_hash: name.into(),
                source_spans: vec![],
                ops: vec![
                    DrawOp::NamedDestination(Destination {
                        name: name.into(),
                        point: Point { x: 72.0, y: 90.0 },
                        source: SourceProvenance::file("main.tex", 0, 1),
                    }),
                    DrawOp::LinkAnnotation(LinkAnnotation {
                        target: format!("#{target}"),
                        rect: Rect {
                            x: 72.0,
                            y: 96.0,
                            width: 80.0,
                            height: 12.0,
                        },
                        source: SourceProvenance::file("main.tex", 1, 2),
                    }),
                ],
            })
            .collect::<Vec<_>>();
        let eager = Document::load_mem(&tex_pdf::render_display_list_pdf(&pages)).unwrap();
        let metadata = pages
            .iter()
            .enumerate()
            .map(|(index, display)| {
                let mut artifact = page(root, 1, &display.page_id, "Helvetica");
                artifact.index = index;
                artifact.width_pt = display.width_pt as u32;
                artifact.height_pt = display.height_pt as u32;
                fs::write(
                    root.join(&artifact.pdf_artifact_path),
                    tex_pdf::render_display_list_pdf(std::slice::from_ref(display)),
                )
                .unwrap();
                artifact
            })
            .collect::<Vec<_>>();
        let target = root.join("rev-1/main.pdf");
        write_recipe(root, 1, &target, &metadata).unwrap();
        ensure_pdf(root, &target).await.unwrap();
        let exported = Document::load(&target).unwrap();
        let resolved_links = |document: &Document| {
            let names = document.catalog().unwrap().get(b"Names").unwrap();
            let tree = document
                .dereference(names)
                .unwrap()
                .1
                .as_dict()
                .unwrap()
                .get(b"Dests")
                .unwrap();
            let mut destinations = vec![];
            collect_names(document, tree, &mut destinations, 0).unwrap();
            let page_numbers = document
                .get_pages()
                .into_iter()
                .map(|(index, id)| (id, index))
                .collect::<BTreeMap<_, _>>();
            let targets = destinations
                .into_iter()
                .map(|(name, value)| {
                    let value = document.dereference(&value).unwrap().1.as_array().unwrap();
                    (name, page_numbers[&value[0].as_reference().unwrap()])
                })
                .collect::<BTreeMap<_, _>>();
            document
                .get_pages()
                .into_iter()
                .map(|(index, id)| {
                    let annotations = document
                        .get_dictionary(id)
                        .unwrap()
                        .get(b"Annots")
                        .unwrap()
                        .as_array()
                        .unwrap();
                    let annotation = document
                        .dereference(&annotations[0])
                        .unwrap()
                        .1
                        .as_dict()
                        .unwrap();
                    let action = document
                        .dereference(annotation.get(b"A").unwrap())
                        .unwrap()
                        .1
                        .as_dict()
                        .unwrap();
                    let name = action
                        .get(b"URI")
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .strip_prefix(b"#")
                        .unwrap();
                    (index, targets[name])
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(resolved_links(&eager), vec![(1, 2), (2, 1)]);
        assert_eq!(resolved_links(&exported), resolved_links(&eager));
    }

    #[tokio::test]
    async fn pdf_export_keeps_first_of_duplicate_destination_names() {
        use tex_render_model::{Destination, DrawOp, PageDisplayList, Point, SourceProvenance};
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();
        let metadata = ["first", "second"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let display = PageDisplayList {
                    page_id: name.into(),
                    width_pt: 612.0,
                    height_pt: 792.0,
                    content_hash: name.into(),
                    source_spans: vec![],
                    ops: vec![DrawOp::NamedDestination(Destination {
                        name: "fig:dup".into(),
                        point: Point { x: 72.0, y: 90.0 },
                        source: SourceProvenance::file("main.tex", 0, 1),
                    })],
                };
                let mut artifact = page(root, 1, name, "Helvetica");
                artifact.index = index;
                artifact.width_pt = display.width_pt as u32;
                artifact.height_pt = display.height_pt as u32;
                fs::write(
                    root.join(&artifact.pdf_artifact_path),
                    tex_pdf::render_display_list_pdf(std::slice::from_ref(&display)),
                )
                .unwrap();
                artifact
            })
            .collect::<Vec<_>>();
        let target = root.join("rev-1/main.pdf");
        write_recipe(root, 1, &target, &metadata).unwrap();
        ensure_pdf(root, &target).await.unwrap();

        let exported = Document::load(&target).unwrap();
        let names = exported.catalog().unwrap().get(b"Names").unwrap();
        let tree = exported
            .dereference(names)
            .unwrap()
            .1
            .as_dict()
            .unwrap()
            .get(b"Dests")
            .unwrap();
        let mut destinations = vec![];
        collect_names(&exported, tree, &mut destinations, 0).unwrap();
        assert_eq!(destinations.len(), 1);
        assert_eq!(destinations[0].0, b"fig:dup");
        let destination = exported
            .dereference(&destinations[0].1)
            .unwrap()
            .1
            .as_array()
            .unwrap();
        assert_eq!(
            destination[0].as_reference().unwrap(),
            exported.get_pages()[&1]
        );
    }

    #[tokio::test]
    async fn pdf_export_rejects_total_input_budget_before_loading_page_pdfs() {
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();
        let pages = [
            page(root, 1, "First", "Helvetica"),
            page(root, 1, "Second", "Helvetica"),
        ];
        let target = root.join("rev-1/main.pdf");
        write_recipe(root, 1, &target, &pages).unwrap();
        for page in pages {
            fs::OpenOptions::new()
                .write(true)
                .open(root.join(page.pdf_artifact_path))
                .unwrap()
                .set_len(MAX_INPUT_BYTES / 2 + 1)
                .unwrap();
        }
        let error = ensure_pdf(root, &target).await.unwrap_err();
        assert!(error.to_string().contains("total input byte budget"));
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn pdf_export_preview_matches_eager_semantics_paint_and_interaction() {
        let directory = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(directory.path()).unwrap();
        fs::write(root.join("main.tex"), r"\documentclass{article}\begin{document}First $x^2$ page.\newpage Second \textbf{bold} page.\end{document}").unwrap();
        let preview_root = root.join(".latexd/preview");
        let eager_root = root.join(".latexd/eager");
        let request = |build_root: &Utf8Path| CompileRequest {
            root: root.to_owned(),
            manifest: tex_world::ProjectManifest::discover(root).unwrap(),
            toplevel: "main.tex".into(),
            rev: 1,
            build_root: build_root.to_owned(),
            changed_files: vec!["main.tex".into()],
        };
        let preview = CompilerDriver::new(Some("internal".into()), vec![])
            .with_preview_artifacts()
            .compile(request(&preview_root))
            .await
            .unwrap();
        let eager = CompilerDriver::new(Some("internal".into()), vec![])
            .compile(request(&eager_root))
            .await
            .unwrap();
        assert!(!preview.pdf_path.exists());
        assert!(eager.pdf_path.exists());
        assert_eq!(preview.renderer_page_metadata, eager.renderer_page_metadata);
        for name in ["render-ir/page-display-list.json", "page-syncmap.json"] {
            let preview_json: serde_json::Value = serde_json::from_slice(
                &fs::read(preview_root.join(format!("rev-1/{name}"))).unwrap(),
            )
            .unwrap();
            let eager_json: serde_json::Value = serde_json::from_slice(
                &fs::read(eager_root.join(format!("rev-1/{name}"))).unwrap(),
            )
            .unwrap();
            assert_eq!(preview_json, eager_json, "{name}");
        }
        for name in [
            "events.json",
            "document-ir.json",
            "display-list.pdf",
            "legacy-output.txt",
        ] {
            assert!(
                !preview_root
                    .join(format!("rev-1/render-ir/{name}"))
                    .exists(),
                "{name} stays outside live preview"
            );
        }
        ensure_pdf(&preview_root, &preview.pdf_path).await.unwrap();
        let exported = Document::load(&preview.pdf_path).unwrap();
        let eager_pdf = Document::load(&eager.pdf_path).unwrap();
        assert_eq!(exported.get_pages().len(), eager_pdf.get_pages().len());
        for (index, page) in exported.get_pages() {
            let artifact = &preview.renderer_page_metadata[index as usize - 1];
            let standalone =
                Document::load(preview_root.join(&artifact.pdf_artifact_path)).unwrap();
            assert_eq!(
                exported.get_page_content(page),
                standalone.get_page_content(standalone.get_pages()[&1])
            );
            assert_eq!(
                exported.extract_text(&[index]).unwrap(),
                eager_pdf.extract_text(&[index]).unwrap()
            );
        }
    }
}
