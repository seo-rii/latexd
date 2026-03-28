use std::{
    ffi::{CString, c_char, c_int, c_void},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result, anyhow};
use libloading::Library;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRenderInput {
    pub page_id: String,
    pub revision: u64,
    pub content_hash: String,
    pub width_px: u32,
    pub height_px: u32,
    pub pdf_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewport {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileKey {
    pub page_id: String,
    pub zoom_bucket: u16,
    pub tile_x: u32,
    pub tile_y: u32,
    pub content_hash: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileRequest {
    pub key: TileKey,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileImage {
    pub key: TileKey,
    pub rect: Rect,
    pub image: RasterImage,
}

pub trait Renderer {
    fn render_full_page(&mut self, page: &PageRenderInput, scale: f32) -> Result<RasterImage>;
    fn render_tiles(
        &mut self,
        page: &PageRenderInput,
        scale: f32,
        rects: &[Rect],
    ) -> Result<Vec<TileImage>>;
}

#[derive(Debug, Default, Clone)]
pub struct MockRenderer;

#[derive(Debug, Clone)]
pub struct CliRenderer {
    pub program: String,
}

#[derive(Default, Clone)]
pub struct GsApiRenderer {
    pub library_path: Option<String>,
    pub runtime: Option<Arc<GsApiRuntime>>,
    pub runtime_pool: Option<Arc<GsApiRuntimePool>>,
}

pub struct GsApiRuntime {
    library_path: PathBuf,
    _library: Library,
    new_instance: unsafe extern "C" fn(*mut *mut c_void, *mut c_void) -> c_int,
    delete_instance: unsafe extern "C" fn(*mut c_void),
    init_with_args: unsafe extern "C" fn(*mut c_void, c_int, *mut *mut c_char) -> c_int,
    exit: unsafe extern "C" fn(*mut c_void) -> c_int,
    set_arg_encoding: Option<unsafe extern "C" fn(*mut c_void, c_int) -> c_int>,
    instance: Mutex<usize>,
}

pub struct GsApiRuntimePool {
    library_path: PathBuf,
    runtimes: Vec<Arc<GsApiRuntime>>,
    next_index: Mutex<usize>,
}

impl std::fmt::Debug for GsApiRenderer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GsApiRenderer")
            .field("library_path", &self.library_path)
            .field("has_runtime", &self.runtime.is_some())
            .field("has_runtime_pool", &self.runtime_pool.is_some())
            .finish()
    }
}

impl std::fmt::Debug for GsApiRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GsApiRuntime")
            .field("library_path", &self.library_path)
            .finish()
    }
}

impl std::fmt::Debug for GsApiRuntimePool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GsApiRuntimePool")
            .field("library_path", &self.library_path)
            .field("size", &self.runtimes.len())
            .finish()
    }
}

impl GsApiRuntime {
    pub fn new(explicit: Option<&str>) -> Result<Self> {
        let library_path = probe_gsapi_library(explicit)
            .ok_or_else(|| anyhow!("failed to locate a loadable libgs shared library"))?;
        // SAFETY: The library path was successfully probed above. Symbol types below match the
        // public Ghostscript C API.
        unsafe {
            let library = Library::new(&library_path)
                .with_context(|| format!("failed to load libgs {}", library_path.display()))?;
            let new_instance = *library
                .get::<unsafe extern "C" fn(*mut *mut c_void, *mut c_void) -> c_int>(
                    b"gsapi_new_instance\0",
                )?;
            let delete_instance =
                *library.get::<unsafe extern "C" fn(*mut c_void)>(b"gsapi_delete_instance\0")?;
            let init_with_args =
                *library
                    .get::<unsafe extern "C" fn(*mut c_void, c_int, *mut *mut c_char) -> c_int>(
                        b"gsapi_init_with_args\0",
                    )?;
            let exit =
                *library.get::<unsafe extern "C" fn(*mut c_void) -> c_int>(b"gsapi_exit\0")?;
            let set_arg_encoding = library
                .get::<unsafe extern "C" fn(*mut c_void, c_int) -> c_int>(
                    b"gsapi_set_arg_encoding\0",
                )
                .ok()
                .map(|symbol| *symbol);

            let mut instance = std::ptr::null_mut();
            let status = new_instance(&mut instance, std::ptr::null_mut());
            if status != 0 {
                return Err(anyhow!("gsapi_new_instance failed with status {status}"));
            }
            if let Some(set_arg_encoding) = set_arg_encoding {
                let _ = set_arg_encoding(instance, 1);
            }
            Ok(Self {
                library_path,
                _library: library,
                new_instance,
                delete_instance,
                init_with_args,
                exit,
                set_arg_encoding,
                instance: Mutex::new(instance as usize),
            })
        }
    }

    pub fn library_path(&self) -> &Path {
        &self.library_path
    }

    pub fn render_full_page(&self, page: &PageRenderInput, scale: f32) -> Result<RasterImage> {
        ensure_pdf_path(page)?;
        let output =
            tempfile::NamedTempFile::new().context("failed to create temporary output png")?;
        let output_path = output.path().with_extension("png");
        let output_path_text = output_path
            .to_str()
            .ok_or_else(|| anyhow!("temporary output path is not valid UTF-8"))?;
        let density = format!("-r{}", 72.0 * scale.max(0.1));
        let output_arg = format!("-sOutputFile={output_path_text}");
        let args = [
            "gsapi",
            "-q",
            "-dSAFER",
            "-dBATCH",
            "-dNOPAUSE",
            "-sDEVICE=pngalpha",
            "-dTextAlphaBits=4",
            "-dGraphicsAlphaBits=4",
            "-dFirstPage=1",
            "-dLastPage=1",
            &density,
            &output_arg,
            &page.pdf_path,
        ];
        let cstrings = args
            .iter()
            .map(|arg| {
                CString::new(*arg).map_err(|error| anyhow!("invalid ghostscript arg: {error}"))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut argv = cstrings
            .iter()
            .map(|arg| arg.as_ptr() as *mut c_char)
            .collect::<Vec<_>>();

        let mut instance = self
            .instance
            .lock()
            .map_err(|_| anyhow!("ghostscript runtime mutex is poisoned"))?;
        let init_status = unsafe {
            (self.init_with_args)(
                *instance as *mut c_void,
                argv.len() as c_int,
                argv.as_mut_ptr(),
            )
        };
        let exit_status = unsafe { (self.exit)(*instance as *mut c_void) };
        unsafe {
            (self.delete_instance)(*instance as *mut c_void);
        }
        let mut replacement = std::ptr::null_mut();
        let replacement_status =
            unsafe { (self.new_instance)(&mut replacement, std::ptr::null_mut()) };
        if replacement_status == 0 {
            if let Some(set_arg_encoding) = self.set_arg_encoding {
                let _ = unsafe { set_arg_encoding(replacement, 1) };
            }
            *instance = replacement as usize;
        } else {
            *instance = 0;
        }
        if init_status != 0 && init_status != -101 {
            return Err(anyhow!(
                "gsapi_init_with_args failed with status {init_status} (exit {exit_status}, replacement {replacement_status})"
            ));
        }
        if replacement_status != 0 {
            return Err(anyhow!(
                "ghostscript completed render but failed to prepare replacement instance: {replacement_status}"
            ));
        }
        drop(instance);

        let bytes = std::fs::read(&output_path).with_context(|| {
            format!("failed to read gsapi output png {}", output_path.display())
        })?;
        decode_png_raster(&bytes)
    }
}

impl GsApiRuntimePool {
    pub fn new(explicit: Option<&str>, size: usize) -> Result<Self> {
        let first = Arc::new(GsApiRuntime::new(explicit)?);
        let library_path = first.library_path().to_path_buf();
        let pool_size = size.max(1);
        let mut runtimes = Vec::with_capacity(pool_size);
        runtimes.push(first);
        for _ in 1..pool_size {
            runtimes.push(Arc::new(GsApiRuntime::new(Some(
                library_path.to_string_lossy().as_ref(),
            ))?));
        }
        Ok(Self {
            library_path,
            runtimes,
            next_index: Mutex::new(0),
        })
    }

    pub fn library_path(&self) -> &Path {
        &self.library_path
    }

    pub fn render_full_page(&self, page: &PageRenderInput, scale: f32) -> Result<RasterImage> {
        let mut index = self
            .next_index
            .lock()
            .map_err(|_| anyhow!("ghostscript runtime pool mutex is poisoned"))?;
        let runtime = self.runtimes[*index % self.runtimes.len()].clone();
        *index = (*index + 1) % self.runtimes.len();
        drop(index);
        runtime.render_full_page(page, scale)
    }
}

impl Drop for GsApiRuntime {
    fn drop(&mut self) {
        if let Ok(instance) = self.instance.lock() {
            if *instance != 0 {
                unsafe {
                    (self.delete_instance)(*instance as *mut c_void);
                }
            }
        }
    }
}

impl Renderer for MockRenderer {
    fn render_full_page(&mut self, page: &PageRenderInput, scale: f32) -> Result<RasterImage> {
        let width = scaled_dimension(page.width_px, scale);
        let height = scaled_dimension(page.height_px, scale);
        Ok(mock_image(
            width,
            height,
            &format!("{}:{scale}", page.page_id),
        ))
    }

    fn render_tiles(
        &mut self,
        page: &PageRenderInput,
        scale: f32,
        rects: &[Rect],
    ) -> Result<Vec<TileImage>> {
        let zoom_bucket = bucket_for_scale(scale);
        Ok(rects
            .iter()
            .enumerate()
            .map(|(index, rect)| TileImage {
                key: tile_key(
                    &page.page_id,
                    &page.content_hash,
                    zoom_bucket,
                    rect.x / rect.width.max(1),
                    rect.y / rect.height.max(1),
                ),
                rect: rect.clone(),
                image: mock_image(
                    rect.width,
                    rect.height,
                    &format!("{}:{scale}:{index}", page.page_id),
                ),
            })
            .collect())
    }
}

impl Renderer for CliRenderer {
    fn render_full_page(&mut self, page: &PageRenderInput, scale: f32) -> Result<RasterImage> {
        render_full_page_with_cli(&self.program, page, scale)
    }

    fn render_tiles(
        &mut self,
        page: &PageRenderInput,
        scale: f32,
        rects: &[Rect],
    ) -> Result<Vec<TileImage>> {
        crop_tiles_from_full(page, scale, rects, self.render_full_page(page, scale)?)
    }
}

impl Renderer for GsApiRenderer {
    fn render_full_page(&mut self, page: &PageRenderInput, scale: f32) -> Result<RasterImage> {
        if let Some(runtime_pool) = &self.runtime_pool {
            return runtime_pool.render_full_page(page, scale);
        }
        if let Some(runtime) = &self.runtime {
            return runtime.render_full_page(page, scale);
        }
        render_full_page_with_gsapi(self.library_path.as_deref(), page, scale)
    }

    fn render_tiles(
        &mut self,
        page: &PageRenderInput,
        scale: f32,
        rects: &[Rect],
    ) -> Result<Vec<TileImage>> {
        crop_tiles_from_full(page, scale, rects, self.render_full_page(page, scale)?)
    }
}

pub fn probe_gsapi_library(explicit: Option<&str>) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = explicit {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(path) = std::env::var("LATEXD_LIBGS") {
        candidates.push(PathBuf::from(path));
    }
    candidates.extend(
        [
            "/usr/lib/x86_64-linux-gnu/libgs.so.10",
            "/usr/lib/x86_64-linux-gnu/libgs.so.9",
            "/usr/lib/libgs.so",
            "/usr/local/lib/libgs.so",
            "libgs.so.10",
            "libgs.so.9",
            "libgs.so",
        ]
        .into_iter()
        .map(PathBuf::from),
    );

    candidates.into_iter().find(|candidate| {
        // SAFETY: Loading the library only probes runtime availability. The handle is dropped
        // immediately and no symbols are invoked here.
        unsafe { Library::new(candidate).is_ok() }
    })
}

pub fn tile_key(
    page_id: &str,
    content_hash: &str,
    zoom_bucket: u16,
    tile_x: u32,
    tile_y: u32,
) -> TileKey {
    let digest = blake3::hash(
        format!("{page_id}:{content_hash}:{zoom_bucket}:{tile_x}:{tile_y}").as_bytes(),
    )
    .to_hex()
    .to_string();

    TileKey {
        page_id: page_id.to_string(),
        zoom_bucket,
        tile_x,
        tile_y,
        content_hash: content_hash.to_string(),
        digest,
    }
}

pub fn required_tiles_for_viewport(
    page: &PageRenderInput,
    scale: f32,
    viewport: &Viewport,
    tile_size: u32,
) -> Vec<TileRequest> {
    assert!(tile_size > 0, "tile size must be positive");

    let width = scaled_dimension(page.width_px, scale);
    let height = scaled_dimension(page.height_px, scale);
    let left = viewport.left.min(width);
    let top = viewport.top.min(height);
    let right = viewport.left.saturating_add(viewport.width).min(width);
    let bottom = viewport.top.saturating_add(viewport.height).min(height);
    if right <= left || bottom <= top {
        return Vec::new();
    }

    let zoom_bucket = bucket_for_scale(scale);
    let start_tile_x = left / tile_size;
    let end_tile_x = (right - 1) / tile_size;
    let start_tile_y = top / tile_size;
    let end_tile_y = (bottom - 1) / tile_size;
    let mut requests = Vec::new();

    for tile_y in start_tile_y..=end_tile_y {
        for tile_x in start_tile_x..=end_tile_x {
            let x = tile_x * tile_size;
            let y = tile_y * tile_size;
            let rect = Rect {
                x,
                y,
                width: (width - x).min(tile_size),
                height: (height - y).min(tile_size),
            };
            requests.push(TileRequest {
                key: tile_key(
                    &page.page_id,
                    &page.content_hash,
                    zoom_bucket,
                    tile_x,
                    tile_y,
                ),
                rect,
            });
        }
    }

    requests
}

pub fn stale_tiles(cached: &[TileKey], required: &[TileRequest]) -> Vec<TileKey> {
    let required_digests = required
        .iter()
        .map(|request| request.key.digest.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    cached
        .iter()
        .filter(|key| !required_digests.contains(key.digest.as_str()))
        .cloned()
        .collect()
}

fn scaled_dimension(dimension: u32, scale: f32) -> u32 {
    ((dimension as f32 * scale).round() as u32).max(1)
}

fn bucket_for_scale(scale: f32) -> u16 {
    (scale * 100.0).round().clamp(1.0, u16::MAX as f32) as u16
}

fn mock_image(width: u32, height: u32, seed: &str) -> RasterImage {
    let color = blake3::hash(seed.as_bytes()).as_bytes()[0];
    RasterImage {
        width,
        height,
        rgba: vec![color; width as usize * height as usize * 4],
    }
}

fn render_full_page_with_cli(
    program: &str,
    page: &PageRenderInput,
    scale: f32,
) -> Result<RasterImage> {
    ensure_pdf_path(page)?;
    let density = format!("-r{}", 72.0 * scale.max(0.1));
    let output = std::process::Command::new(program)
        .args([
            "-q",
            "-dSAFER",
            "-dBATCH",
            "-dNOPAUSE",
            "-sDEVICE=pngalpha",
            "-dTextAlphaBits=4",
            "-dGraphicsAlphaBits=4",
            "-dFirstPage=1",
            "-dLastPage=1",
            &density,
            "-sOutputFile=%stdout",
            &page.pdf_path,
        ])
        .output()
        .map_err(|error| anyhow!("failed to run ghostscript {}: {error}", program))?;
    if !output.status.success() {
        return Err(anyhow!(
            "ghostscript {} failed: {}",
            program,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    decode_png_raster(&output.stdout)
}

fn render_full_page_with_gsapi(
    library_path: Option<&str>,
    page: &PageRenderInput,
    scale: f32,
) -> Result<RasterImage> {
    GsApiRuntime::new(library_path)?.render_full_page(page, scale)
}

fn ensure_pdf_path(page: &PageRenderInput) -> Result<()> {
    if page.pdf_path.is_empty() {
        return Err(anyhow!(
            "page {} does not declare a PDF source",
            page.page_id
        ));
    }
    if !Path::new(&page.pdf_path).exists() {
        return Err(anyhow!("page PDF source {} does not exist", page.pdf_path));
    }
    Ok(())
}

fn decode_png_raster(bytes: &[u8]) -> Result<RasterImage> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map_err(|error| anyhow!("failed to decode ghostscript PNG output: {error}"))?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Ok(RasterImage {
        width,
        height,
        rgba: image.into_raw(),
    })
}

fn crop_tiles_from_full(
    page: &PageRenderInput,
    scale: f32,
    rects: &[Rect],
    full: RasterImage,
) -> Result<Vec<TileImage>> {
    let zoom_bucket = bucket_for_scale(scale);
    let stride = full.width as usize * 4;
    let mut tiles = Vec::with_capacity(rects.len());
    for rect in rects {
        if rect.x >= full.width || rect.y >= full.height {
            return Err(anyhow!(
                "tile rect {},{} is outside rendered page {}x{}",
                rect.x,
                rect.y,
                full.width,
                full.height
            ));
        }
        let clipped_width = rect.width.min(full.width - rect.x);
        let clipped_height = rect.height.min(full.height - rect.y);
        let mut rgba = Vec::with_capacity(clipped_width as usize * clipped_height as usize * 4);
        for row in rect.y..rect.y + clipped_height {
            let start = row as usize * stride + rect.x as usize * 4;
            let end = start + clipped_width as usize * 4;
            rgba.extend_from_slice(&full.rgba[start..end]);
        }
        tiles.push(TileImage {
            key: tile_key(
                &page.page_id,
                &page.content_hash,
                zoom_bucket,
                rect.x / rect.width.max(1),
                rect.y / rect.height.max(1),
            ),
            rect: Rect {
                x: rect.x,
                y: rect.y,
                width: clipped_width,
                height: clipped_height,
            },
            image: RasterImage {
                width: clipped_width,
                height: clipped_height,
                rgba,
            },
        });
    }
    Ok(tiles)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use tempfile::NamedTempFile;

    use super::{
        CliRenderer, GsApiRenderer, GsApiRuntime, GsApiRuntimePool, MockRenderer, PageRenderInput,
        Rect, Renderer, Viewport, probe_gsapi_library, required_tiles_for_viewport, stale_tiles,
        tile_key,
    };

    fn sample_page() -> PageRenderInput {
        PageRenderInput {
            page_id: "page-a".to_string(),
            revision: 3,
            content_hash: "hash-a".to_string(),
            width_px: 512,
            height_px: 640,
            pdf_path: String::new(),
        }
    }

    fn write_sample_pdf(path: &std::path::Path, width: u32, height: u32) {
        let stream = "BT /F1 12 Tf 18 36 Td (latexd gs smoke) Tj ET";
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.4\n");
        let objects = [
            "1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".to_string(),
            "2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n".to_string(),
            format!(
                "3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >> endobj\n"
            ),
            "4 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n".to_string(),
            format!(
                "5 0 obj << /Length {} >> stream\n{}\nendstream\nendobj\n",
                stream.len(),
                stream
            ),
        ];
        let mut offsets = vec![0usize];
        for object in &objects {
            offsets.push(pdf.len());
            pdf.extend_from_slice(object.as_bytes());
        }
        let xref_offset = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                objects.len() + 1,
                xref_offset
            )
            .as_bytes(),
        );
        fs::write(path, pdf).expect("write sample pdf");
    }

    fn gs_page(program: &str) -> (NamedTempFile, PageRenderInput) {
        let pdf_file = NamedTempFile::new().expect("named temp file");
        write_sample_pdf(pdf_file.path(), 144, 72);
        let page = PageRenderInput {
            page_id: "page-a".to_string(),
            revision: 9,
            content_hash: "hash-a".to_string(),
            width_px: 144,
            height_px: 72,
            pdf_path: pdf_file.path().display().to_string(),
        };
        assert!(
            std::path::Path::new(&page.pdf_path).exists(),
            "sample pdf must exist for {program}"
        );
        (pdf_file, page)
    }

    #[test]
    fn mock_renderer_matches_trait_contract() {
        let mut renderer = MockRenderer;
        let page = sample_page();
        let full = renderer.render_full_page(&page, 1.5).expect("full page");
        let tiles = renderer
            .render_tiles(
                &page,
                1.0,
                &[
                    Rect {
                        x: 0,
                        y: 0,
                        width: 128,
                        height: 64,
                    },
                    Rect {
                        x: 128,
                        y: 64,
                        width: 128,
                        height: 64,
                    },
                ],
            )
            .expect("tiles");

        assert_eq!(full.width, 768);
        assert_eq!(full.height, 960);
        assert_eq!(tiles.len(), 2);
        assert_eq!(tiles[0].image.width, 128);
        assert_eq!(tiles[1].image.height, 64);
        assert_ne!(tiles[0].key.digest, tiles[1].key.digest);
    }

    #[test]
    fn tile_key_hash_changes_with_zoom_or_content() {
        let base = tile_key("page-a", "hash-a", 100, 0, 0);
        let zoom = tile_key("page-a", "hash-a", 150, 0, 0);
        let content = tile_key("page-a", "hash-b", 100, 0, 0);

        assert_ne!(base.digest, zoom.digest);
        assert_ne!(base.digest, content.digest);
    }

    #[test]
    fn required_tiles_cover_visible_viewport() {
        let page = sample_page();
        let tiles = required_tiles_for_viewport(
            &page,
            1.0,
            &Viewport {
                left: 128,
                top: 128,
                width: 300,
                height: 300,
            },
            256,
        );

        assert_eq!(tiles.len(), 4);
        assert_eq!(tiles[0].key.tile_x, 0);
        assert_eq!(tiles[0].key.tile_y, 0);
        assert_eq!(tiles[3].key.tile_x, 1);
        assert_eq!(tiles[3].key.tile_y, 1);
        assert_eq!(tiles[3].rect.width, 256);
    }

    #[test]
    fn required_tiles_clip_to_page_bounds() {
        let page = sample_page();
        let tiles = required_tiles_for_viewport(
            &page,
            1.0,
            &Viewport {
                left: 400,
                top: 512,
                width: 200,
                height: 200,
            },
            256,
        );

        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].rect.width, 256);
        assert_eq!(tiles[0].rect.height, 128);
    }

    #[test]
    fn stale_tile_invalidation_drops_non_required_tiles() {
        let page = sample_page();
        let required = required_tiles_for_viewport(
            &page,
            1.0,
            &Viewport {
                left: 0,
                top: 0,
                width: 256,
                height: 256,
            },
            256,
        );
        let cached = vec![
            required[0].key.clone(),
            tile_key("page-a", "hash-a", 100, 1, 1),
            tile_key("page-a", "old-hash", 100, 0, 0),
        ];

        let stale = stale_tiles(&cached, &required);

        assert_eq!(stale.len(), 2);
        assert!(stale.iter().any(|key| key.tile_x == 1 && key.tile_y == 1));
        assert!(stale.iter().any(|key| key.content_hash == "old-hash"));
    }

    #[test]
    fn cli_renderer_renders_page_with_ghostscript_when_available() {
        let Ok(program) = which::which("gs") else {
            return;
        };
        let (_pdf_file, page) = gs_page(program.to_string_lossy().as_ref());
        let mut renderer = CliRenderer {
            program: program.to_string_lossy().to_string(),
        };

        let image = renderer.render_full_page(&page, 1.0).expect("full page");

        assert_eq!(image.width, 144);
        assert_eq!(image.height, 72);
        assert_eq!(image.rgba.len(), 144 * 72 * 4);
    }

    #[test]
    fn cli_renderer_crops_tiles_from_rendered_page_when_available() {
        let Ok(program) = which::which("gs") else {
            return;
        };
        let (_pdf_file, page) = gs_page(program.to_string_lossy().as_ref());
        let mut renderer = CliRenderer {
            program: program.to_string_lossy().to_string(),
        };

        let tiles = renderer
            .render_tiles(
                &page,
                1.0,
                &[
                    Rect {
                        x: 0,
                        y: 0,
                        width: 32,
                        height: 16,
                    },
                    Rect {
                        x: 80,
                        y: 24,
                        width: 32,
                        height: 16,
                    },
                ],
            )
            .expect("tiles");

        assert_eq!(tiles.len(), 2);
        assert_eq!(tiles[0].image.width, 32);
        assert_eq!(tiles[0].image.height, 16);
        assert_eq!(tiles[1].rect.x, 80);
        assert_eq!(tiles[1].rect.y, 24);
    }

    #[test]
    fn probe_gsapi_library_finds_system_library_when_available() {
        let Some(path) = probe_gsapi_library(None) else {
            return;
        };

        assert!(!path.as_os_str().is_empty());
    }

    #[test]
    fn gs_api_renderer_renders_page_when_library_available() {
        let Some(library_path) = probe_gsapi_library(None) else {
            return;
        };
        let (_pdf_file, page) = gs_page(library_path.to_string_lossy().as_ref());
        let runtime = Arc::new(
            GsApiRuntime::new(Some(library_path.to_string_lossy().as_ref())).expect("runtime"),
        );
        let mut renderer = GsApiRenderer {
            library_path: Some(library_path.to_string_lossy().to_string()),
            runtime: Some(runtime),
            runtime_pool: None,
        };

        let image = renderer.render_full_page(&page, 1.0).expect("full page");

        assert_eq!(image.width, 144);
        assert_eq!(image.height, 72);
        assert_eq!(image.rgba.len(), 144 * 72 * 4);
    }

    #[test]
    fn gs_api_renderer_crops_tiles_when_library_available() {
        let Some(library_path) = probe_gsapi_library(None) else {
            return;
        };
        let (_pdf_file, page) = gs_page(library_path.to_string_lossy().as_ref());
        let runtime = Arc::new(
            GsApiRuntime::new(Some(library_path.to_string_lossy().as_ref())).expect("runtime"),
        );
        let mut renderer = GsApiRenderer {
            library_path: Some(library_path.to_string_lossy().to_string()),
            runtime: Some(runtime),
            runtime_pool: None,
        };

        let tiles = renderer
            .render_tiles(
                &page,
                1.0,
                &[
                    Rect {
                        x: 0,
                        y: 0,
                        width: 32,
                        height: 16,
                    },
                    Rect {
                        x: 80,
                        y: 24,
                        width: 32,
                        height: 16,
                    },
                ],
            )
            .expect("tiles");

        assert_eq!(tiles.len(), 2);
        assert_eq!(tiles[0].image.width, 32);
        assert_eq!(tiles[0].image.height, 16);
    }

    #[test]
    fn gs_api_runtime_can_be_reused_across_multiple_renders() {
        let Some(library_path) = probe_gsapi_library(None) else {
            return;
        };
        let (_pdf_file, page) = gs_page(library_path.to_string_lossy().as_ref());
        let runtime =
            GsApiRuntime::new(Some(library_path.to_string_lossy().as_ref())).expect("runtime");

        let first = runtime.render_full_page(&page, 1.0).expect("first render");
        let second = runtime.render_full_page(&page, 1.0).expect("second render");

        assert_eq!(first.width, second.width);
        assert_eq!(first.height, second.height);
        assert_eq!(first.rgba, second.rgba);
    }

    #[test]
    fn gs_api_runtime_pool_can_render_multiple_times() {
        let Some(library_path) = probe_gsapi_library(None) else {
            return;
        };
        let (_pdf_file, page) = gs_page(library_path.to_string_lossy().as_ref());
        let pool = Arc::new(
            GsApiRuntimePool::new(Some(library_path.to_string_lossy().as_ref()), 2).expect("pool"),
        );
        let mut renderer = GsApiRenderer {
            library_path: Some(library_path.to_string_lossy().to_string()),
            runtime: None,
            runtime_pool: Some(pool),
        };

        let first = renderer.render_full_page(&page, 1.0).expect("first render");
        let second = renderer
            .render_full_page(&page, 1.0)
            .expect("second render");

        assert_eq!(first.width, second.width);
        assert_eq!(first.height, second.height);
        assert_eq!(first.rgba, second.rgba);
    }
}
