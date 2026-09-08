use std::fs;

use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use tempfile::{TempDir, tempdir};
use tower::util::ServiceExt;

use super::*;

fn raster_test_state() -> (TempDir, Arc<AppState>) {
    let temp = tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).expect("utf8 path");
    fs::write(
        root.join("00README.yaml"),
        "compiler: pdf_latex\ntoplevel:\n  - main.tex\n",
    )
    .expect("manifest");
    fs::write(root.join("main.tex"), "hello").expect("source");
    let build_root = root.join(".latexd/build");
    fs::create_dir_all(build_root.join("rev-1")).expect("build directory");
    fs::write(
        build_root.join("rev-1/page-metadata.json"),
        serde_json::to_vec(&vec![PageArtifactMeta {
            page_id: "page-a".into(),
            index: 0,
            line_count: 1,
            width_pt: 416,
            height_pt: 640,
            natural_size_pt: None,
            content_hash: "hash-a".into(),
            text_start_utf8: 0,
            text_end_utf8: 5,
            pdf_artifact_path: Utf8PathBuf::from("rev-1/pages/page-a.pdf"),
            source_spans: vec![],
        }])
        .expect("metadata"),
    )
    .expect("write metadata");
    let state = Arc::new(AppState {
        root: root.clone(),
        build_root,
        artifacts_root: root.join(".latexd/artifacts"),
        world: tex_world::ProjectWorld::load(root).expect("world"),
        compiler: CompilerDriver::new(None, vec![]),
        tile_renderer: TileRendererConfig::Mock,
        editor_bridge: None,
        raster_cache: RwLock::new(BTreeMap::new()),
        inflight_rasters: RwLock::new(BTreeMap::new()),
        build_cache: RwLock::new(BuildCache::default()),
        live: RwLock::new(LivePreviewState::default()),
        events: broadcast::channel(4).0,
    });
    (temp, state)
}

async fn request(router: &Router, uri: &str) -> Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response")
}

#[tokio::test]
async fn manifest_urls_render_the_declared_tile_size_and_canonical_page_bounds() {
    let (_temp, state) = raster_test_state();
    let router = build_router(state);
    for (scale, zoom_bucket, width, height) in [
        (1.234, 123, 512, 787),
        (1.236, 124, 516, 794),
        (0.104, 10, 42, 64),
    ] {
        for tile_size in [128, 256, 512] {
            let response = request(&router, &format!(
                "/api/tiles/1/page-a?scale={scale}&tile_size={tile_size}&left=0&top=0&width=10000&height=10000"
            )).await;
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), 1024 * 1024)
                .await
                .expect("manifest");
            let manifest: TileManifestResponse = serde_json::from_slice(&body).expect("decode");
            assert_eq!(
                manifest.raster_spec,
                RasterSpec {
                    zoom_bucket,
                    tile_size,
                    width_px: width,
                    height_px: height,
                }
            );
            assert_eq!(manifest.tile_size, tile_size);
            assert_eq!(
                manifest.items.len() as u32,
                width.div_ceil(tile_size) * height.div_ceil(tile_size)
            );
            let mut pixels = 0;
            for tile in manifest.items {
                assert_eq!(tile.zoom_bucket, zoom_bucket);
                assert!(tile.png_url.ends_with(&format!("?tile_size={tile_size}")));
                let response = request(&router, &tile.png_url).await;
                assert_eq!(response.status(), StatusCode::OK, "{}", tile.png_url);
                let body = to_bytes(response.into_body(), 4 * 1024 * 1024)
                    .await
                    .expect("PNG");
                let image = image::load_from_memory(&body).expect("decode PNG");
                let expected_width = (width - tile.tile_x * tile_size).min(tile_size);
                let expected_height = (height - tile.tile_y * tile_size).min(tile_size);
                assert_eq!(image.width(), expected_width, "{}", tile.png_url);
                assert_eq!(image.height(), expected_height, "{}", tile.png_url);
                pixels += image.width() * image.height();
            }
            assert_eq!(pixels, width * height);
        }
    }
}

#[tokio::test]
async fn raster_endpoints_reject_invalid_specs_before_rendering() {
    let (_temp, state) = raster_test_state();
    let router = build_router(state.clone());
    for query in [
        "scale=NaN",
        "scale=inf",
        "scale=-1",
        "scale=0",
        "scale=655.36",
        "scale=20",
        "tile_size=0",
        "tile_size=4294967295",
        "tile_size=1",
    ] {
        let response = request(
            &router,
            &format!("/api/tiles/1/page-a?{query}&left=0&top=0&width=10000&height=10000"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
    }
    for path in [
        "/artifacts/rev/1/tiles/page-a/0/0/0.png",
        "/artifacts/rev/1/tiles/page-a/9/0/0.png",
        "/artifacts/rev/1/tiles/page-a/2000/0/0.png",
        "/artifacts/rev/1/tiles/page-a/100/0/0.png?tile_size=0",
        "/artifacts/rev/1/tiles/page-a/100/0/0.png?scale=1.23",
        "/artifacts/rev/1/tiles/page-a/100/0/0.png?scale=NaN",
        "/artifacts/rev/1/page-raster/page-a.png?scale=inf",
        "/artifacts/rev/1/page-raster/page-a.png?scale=20",
    ] {
        assert_eq!(
            request(&router, path).await.status(),
            StatusCode::BAD_REQUEST,
            "{path}"
        );
    }
    let outside = request(
        &router,
        "/artifacts/rev/1/tiles/page-a/100/4294967295/0.png",
    )
    .await;
    assert_eq!(outside.status(), StatusCode::NOT_FOUND);
    let metrics = render_session_metrics_snapshot(&state).await;
    assert_eq!(metrics.render_request_count, 0);
    assert_eq!(metrics.tile_render_request_count, 0);
}

#[tokio::test]
async fn full_page_raster_cache_uses_the_same_canonical_scale_as_tiles() {
    let (_temp, state) = raster_test_state();
    let page = load_revision_page_input(&state, 1, "page-a")
        .await
        .expect("page");
    let first = cache_raster_image(&state, &page, 1.234)
        .await
        .expect("raster");
    let canonical = cache_raster_image(&state, &page, 1.23)
        .await
        .expect("raster");
    assert_eq!((first.width, first.height), (512, 787));
    assert_eq!(first.rgba, canonical.rgba);
    assert_eq!(state.raster_cache.read().await.len(), 1);
    let metrics = render_session_metrics_snapshot(&state).await;
    assert_eq!(metrics.render_request_count, 1);
}

#[test]
fn raster_spec_clips_viewports_and_bounds_manifest_allocation() {
    let spec = RasterSpec::from_scale(416, 640, 1.234, Some(1)).expect("spec");
    assert!(
        spec.required_tiles(&Viewport {
            left: 0,
            top: 0,
            width: 512,
            height: 787
        })
        .is_err()
    );
    assert_eq!(
        spec.required_tiles(&Viewport {
            left: 510,
            top: 786,
            width: u32::MAX,
            height: u32::MAX
        })
        .expect("clipped"),
        vec![(510, 786), (511, 786)]
    );
    assert!(
        spec.required_tiles(&Viewport {
            left: u32::MAX,
            top: 0,
            width: 1,
            height: 1
        })
        .expect("outside")
        .is_empty()
    );
    assert!(spec.tile_rect(u32::MAX, u32::MAX).is_none());
    for scale in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -1.0,
        0.0,
        0.09,
        655.36,
    ] {
        assert!(RasterSpec::from_scale(416, 640, scale, None).is_err());
    }
    assert!(RasterSpec::from_scale(u32::MAX, 1, 1.0, None).is_err());
    assert!(RasterSpec::from_scale(8193, 8192, 1.0, None).is_err());
    assert!(RasterSpec::from_scale(0, 640, 1.0, None).is_err());
    for dimension in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
        assert!(RasterSpec::from_size_scale([dimension, 640.0], 1.0, None).is_err());
    }
}
