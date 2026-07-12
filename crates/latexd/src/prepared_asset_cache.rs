use std::{
    collections::BTreeMap,
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, bail};
use camino::{Utf8Path, Utf8PathBuf};
use tex_render_model::{
    GraphicAssetRequest, MATERIALIZED_GRAPHIC_ASSET_HASH_VERSION, MaterializedGraphicAsset,
};

pub(crate) const PREPARED_ASSET_CACHE_SCHEMA_VERSION: u32 = 1;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PersistedLookup {
    schema_version: u32,
    lookup_hash: String,
    content_hash: String,
    binding_hash: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PersistedObject {
    schema_version: u32,
    content_hash: String,
    request: GraphicAssetRequest,
    materialized: MaterializedGraphicAsset,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedAssetCache {
    root: Utf8PathBuf,
}

impl PreparedAssetCache {
    pub(crate) fn new(root: impl Into<Utf8PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub(crate) fn lookup_hash(
        request: &GraphicAssetRequest,
        source_bytes: &[u8],
        embedded_assets: &BTreeMap<String, Option<Vec<u8>>>,
        materializer_identity: &str,
    ) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"prepared-asset-cache-lookup");
        hasher.update(&PREPARED_ASSET_CACHE_SCHEMA_VERSION.to_le_bytes());
        hasher.update(&MATERIALIZED_GRAPHIC_ASSET_HASH_VERSION.to_le_bytes());
        let mut update_field = |value: &[u8]| {
            hasher.update(&(value.len() as u64).to_le_bytes());
            hasher.update(value);
        };
        let request_json =
            serde_json::to_vec(request).expect("graphic asset request must serialize");
        update_field(&request_json);
        update_field(materializer_identity.as_bytes());
        update_field(source_bytes);
        for (asset_ref, bytes) in embedded_assets {
            update_field(asset_ref.as_bytes());
            update_field(&[u8::from(bytes.is_some())]);
            if let Some(bytes) = bytes {
                update_field(bytes);
            }
        }
        hasher.finalize().to_hex().to_string()
    }

    pub(crate) fn load(
        &self,
        lookup_hash: &str,
        request: &GraphicAssetRequest,
    ) -> anyhow::Result<Option<MaterializedGraphicAsset>> {
        let lookup_path = self.lookup_path(lookup_hash);
        let lookup_payload = match fs::read(lookup_path.as_std_path()) {
            Ok(payload) => payload,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let lookup = serde_json::from_slice::<PersistedLookup>(&lookup_payload)
            .context("failed to deserialize prepared asset cache lookup")?;
        if lookup.schema_version != PREPARED_ASSET_CACHE_SCHEMA_VERSION {
            return Ok(None);
        }
        if lookup.lookup_hash != lookup_hash
            || lookup.binding_hash
                != lookup_binding_hash(lookup_hash, &lookup.content_hash, request)
        {
            bail!("invalid prepared asset cache lookup binding");
        }

        let object_path = self.object_path(&lookup.content_hash)?;
        let object_payload = fs::read(object_path.as_std_path())
            .context("failed to read prepared asset cache object")?;
        let object = serde_json::from_slice::<PersistedObject>(&object_payload)
            .context("failed to deserialize prepared asset cache object")?;
        if object.schema_version != PREPARED_ASSET_CACHE_SCHEMA_VERSION {
            return Ok(None);
        }
        if object.content_hash != lookup.content_hash
            || object.request != *request
            || object.materialized.content_hash != lookup.content_hash
            || !object.materialized.has_valid_content_hash(request)
        {
            bail!("invalid prepared asset cache object binding");
        }
        Ok(Some(object.materialized))
    }

    pub(crate) fn store(
        &self,
        lookup_hash: &str,
        request: &GraphicAssetRequest,
        materialized: &MaterializedGraphicAsset,
    ) -> anyhow::Result<()> {
        if !materialized.has_valid_content_hash(request) {
            bail!("refusing to cache prepared asset with invalid content hash");
        }
        let object_path = self.object_path(&materialized.content_hash)?;
        let object_payload = serde_json::to_vec(&PersistedObject {
            schema_version: PREPARED_ASSET_CACHE_SCHEMA_VERSION,
            content_hash: materialized.content_hash.clone(),
            request: request.clone(),
            materialized: materialized.clone(),
        })
        .context("failed to serialize prepared asset cache object")?;
        atomic_write(&object_path, &object_payload)?;

        let lookup_path = self.lookup_path(lookup_hash);
        let lookup_payload = serde_json::to_vec(&PersistedLookup {
            schema_version: PREPARED_ASSET_CACHE_SCHEMA_VERSION,
            lookup_hash: lookup_hash.to_string(),
            content_hash: materialized.content_hash.clone(),
            binding_hash: lookup_binding_hash(lookup_hash, &materialized.content_hash, request),
        })
        .context("failed to serialize prepared asset cache lookup")?;
        atomic_write(&lookup_path, &lookup_payload)
    }

    fn lookup_path(&self, lookup_hash: &str) -> Utf8PathBuf {
        self.root
            .join(format!("v{PREPARED_ASSET_CACHE_SCHEMA_VERSION}/lookups"))
            .join(&lookup_hash[..2])
            .join(format!("{lookup_hash}.json"))
    }

    fn object_path(&self, content_hash: &str) -> anyhow::Result<Utf8PathBuf> {
        let Some(hash) = content_hash.strip_prefix("blake3:") else {
            bail!("unsupported prepared asset content hash {content_hash}");
        };
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("invalid prepared asset content hash {content_hash}");
        }
        Ok(self
            .root
            .join(format!(
                "v{PREPARED_ASSET_CACHE_SCHEMA_VERSION}/objects/blake3"
            ))
            .join(&hash[..2])
            .join(format!("{hash}.json")))
    }
}

fn lookup_binding_hash(
    lookup_hash: &str,
    content_hash: &str,
    request: &GraphicAssetRequest,
) -> String {
    let request_json = serde_json::to_vec(request).expect("graphic asset request must serialize");
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"prepared-asset-cache-binding");
    hasher.update(&PREPARED_ASSET_CACHE_SCHEMA_VERSION.to_le_bytes());
    for value in [
        lookup_hash.as_bytes(),
        content_hash.as_bytes(),
        &request_json,
    ] {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value);
    }
    hasher.finalize().to_hex().to_string()
}

fn atomic_write(path: &Utf8Path, payload: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .context("prepared asset cache path has no parent")?;
    fs::create_dir_all(parent.as_std_path())?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary_path = parent.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name().unwrap_or("entry"),
        std::process::id(),
        sequence
    ));
    fs::write(temporary_path.as_std_path(), payload)?;
    if let Err(error) = fs::rename(temporary_path.as_std_path(), path.as_std_path()) {
        if fs::read(path.as_std_path()).is_ok_and(|existing| existing == payload) {
            let _ = fs::remove_file(temporary_path.as_std_path());
            return Ok(());
        }
        #[cfg(windows)]
        {
            if fs::remove_file(path.as_std_path()).is_ok()
                && fs::rename(temporary_path.as_std_path(), path.as_std_path()).is_ok()
            {
                return Ok(());
            }
        }
        let _ = fs::remove_file(temporary_path.as_std_path());
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, process::Command, sync::Arc, time::Duration};

    use camino::Utf8PathBuf;
    use tex_render_model::{GraphicAssetFormat, GraphicAssetRequest, MaterializedGraphicAsset};

    use super::{
        PREPARED_ASSET_CACHE_SCHEMA_VERSION, PersistedLookup, PreparedAssetCache,
        lookup_binding_hash,
    };

    fn fixture() -> (GraphicAssetRequest, MaterializedGraphicAsset) {
        let request = GraphicAssetRequest {
            asset_ref: "figures/image.png".to_string(),
            source_format: Some(GraphicAssetFormat::Png),
            page_selection: None,
            asset_hash: Some("blake3:source".to_string()),
        };
        let materialized = MaterializedGraphicAsset::from_source(&request, b"png-bytes".to_vec())
            .expect("materialized fixture");
        (request, materialized)
    }

    #[test]
    fn cache_roundtrips_through_lookup_and_content_addressed_object() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 path");
        let (request, materialized) = fixture();
        let lookup_hash =
            PreparedAssetCache::lookup_hash(&request, b"png-bytes", &BTreeMap::new(), "source");

        let first = PreparedAssetCache::new(root.clone());
        assert!(
            first
                .load(&lookup_hash, &request)
                .expect("initial load")
                .is_none()
        );
        first
            .store(&lookup_hash, &request, &materialized)
            .expect("store cache entry");

        let restarted = PreparedAssetCache::new(root);
        assert_eq!(
            restarted.load(&lookup_hash, &request).expect("reload"),
            Some(materialized.clone())
        );
        assert!(restarted.lookup_path(&lookup_hash).exists());
        assert!(
            restarted
                .object_path(&materialized.content_hash)
                .expect("object path")
                .exists()
        );
        fs::remove_file(restarted.lookup_path(&lookup_hash)).expect("remove lookup pointer");
        assert!(
            restarted
                .load(&lookup_hash, &request)
                .expect("load object without lookup")
                .is_none()
        );
        restarted
            .store(&lookup_hash, &request, &materialized)
            .expect("restore lookup pointer");
    }

    #[test]
    fn cache_lookup_changes_with_embedded_asset_presence_and_bytes() {
        let (request, _) = fixture();
        let missing = BTreeMap::from([("figures/pixel.png".to_string(), None)]);
        let present = BTreeMap::from([("figures/pixel.png".to_string(), Some(vec![1, 2, 3]))]);
        let changed = BTreeMap::from([("figures/pixel.png".to_string(), Some(vec![1, 2, 4]))]);

        let missing_hash = PreparedAssetCache::lookup_hash(&request, b"svg", &missing, "svg");
        let present_hash = PreparedAssetCache::lookup_hash(&request, b"svg", &present, "svg");
        let changed_hash = PreparedAssetCache::lookup_hash(&request, b"svg", &changed, "svg");

        assert_ne!(missing_hash, present_hash);
        assert_ne!(present_hash, changed_hash);
    }

    #[test]
    fn corrupt_object_is_an_error_and_can_be_replaced() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 path");
        let (request, materialized) = fixture();
        let lookup_hash =
            PreparedAssetCache::lookup_hash(&request, b"png-bytes", &BTreeMap::new(), "source");
        let cache = PreparedAssetCache::new(root);
        cache
            .store(&lookup_hash, &request, &materialized)
            .expect("store cache entry");
        fs::write(
            cache
                .object_path(&materialized.content_hash)
                .expect("object path"),
            b"not json",
        )
        .expect("corrupt object");

        assert!(cache.load(&lookup_hash, &request).is_err());
        cache
            .store(&lookup_hash, &request, &materialized)
            .expect("replace corrupt object");
        assert_eq!(
            cache.load(&lookup_hash, &request).expect("reloaded object"),
            Some(materialized.clone())
        );
        fs::remove_file(
            cache
                .object_path(&materialized.content_hash)
                .expect("object path"),
        )
        .expect("remove cached object");
        assert!(cache.load(&lookup_hash, &request).is_err());
        cache
            .store(&lookup_hash, &request, &materialized)
            .expect("recover dangling lookup");
    }

    #[test]
    fn schema_mismatch_is_a_cache_miss() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 path");
        let (request, materialized) = fixture();
        let lookup_hash =
            PreparedAssetCache::lookup_hash(&request, b"png-bytes", &BTreeMap::new(), "source");
        let cache = PreparedAssetCache::new(root);
        cache
            .store(&lookup_hash, &request, &materialized)
            .expect("store cache entry");
        fs::write(
            cache.lookup_path(&lookup_hash),
            serde_json::to_vec(&PersistedLookup {
                schema_version: PREPARED_ASSET_CACHE_SCHEMA_VERSION + 1,
                lookup_hash: lookup_hash.clone(),
                content_hash: materialized.content_hash.clone(),
                binding_hash: lookup_binding_hash(
                    &lookup_hash,
                    &materialized.content_hash,
                    &request,
                ),
            })
            .expect("serialize mismatched lookup"),
        )
        .expect("write mismatched lookup");

        assert!(
            cache
                .load(&lookup_hash, &request)
                .expect("load mismatch")
                .is_none()
        );
    }

    #[test]
    fn stale_lookup_pointer_is_rejected_and_recoverable() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 path");
        let (request, first) = fixture();
        let second = MaterializedGraphicAsset::from_source(&request, b"changed-png".to_vec())
            .expect("changed materialized fixture");
        let first_lookup =
            PreparedAssetCache::lookup_hash(&request, b"png-bytes", &BTreeMap::new(), "source");
        let second_lookup =
            PreparedAssetCache::lookup_hash(&request, b"changed-png", &BTreeMap::new(), "source");
        let cache = PreparedAssetCache::new(root);
        cache
            .store(&first_lookup, &request, &first)
            .expect("store first entry");
        cache
            .store(&second_lookup, &request, &second)
            .expect("store second entry");

        let first_lookup_path = cache.lookup_path(&first_lookup);
        let mut stale = serde_json::from_slice::<PersistedLookup>(
            &fs::read(&first_lookup_path).expect("read first lookup"),
        )
        .expect("parse first lookup");
        stale.content_hash = second.content_hash;
        fs::write(
            &first_lookup_path,
            serde_json::to_vec(&stale).expect("serialize stale lookup"),
        )
        .expect("write stale lookup");

        assert!(cache.load(&first_lookup, &request).is_err());
        cache
            .store(&first_lookup, &request, &first)
            .expect("recover stale lookup");
        assert_eq!(
            cache.load(&first_lookup, &request).expect("load recovery"),
            Some(first.clone())
        );
        fs::write(cache.lookup_path(&first_lookup), b"not json").expect("corrupt lookup");
        assert!(cache.load(&first_lookup, &request).is_err());
        cache
            .store(&first_lookup, &request, &first)
            .expect("recover corrupt lookup");
    }

    #[test]
    fn concurrent_identical_stores_leave_complete_entries_without_temp_files() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 path");
        let (request, materialized) = fixture();
        let lookup_hash =
            PreparedAssetCache::lookup_hash(&request, b"png-bytes", &BTreeMap::new(), "source");
        let cache = Arc::new(PreparedAssetCache::new(root));
        let writers = (0..8)
            .map(|_| {
                let cache = Arc::clone(&cache);
                let lookup_hash = lookup_hash.clone();
                let request = request.clone();
                let materialized = materialized.clone();
                std::thread::spawn(move || cache.store(&lookup_hash, &request, &materialized))
            })
            .collect::<Vec<_>>();
        for writer in writers {
            writer
                .join()
                .expect("writer thread")
                .expect("concurrent store");
        }

        assert_eq!(
            cache.load(&lookup_hash, &request).expect("load entry"),
            Some(materialized.clone())
        );
        for directory in [
            cache
                .lookup_path(&lookup_hash)
                .parent()
                .expect("lookup parent")
                .to_path_buf(),
            cache
                .object_path(&materialized.content_hash)
                .expect("object path")
                .parent()
                .expect("object parent")
                .to_path_buf(),
        ] {
            assert!(
                fs::read_dir(directory)
                    .expect("cache directory")
                    .all(|entry| !entry
                        .expect("cache entry")
                        .file_name()
                        .to_string_lossy()
                        .ends_with(".tmp"))
            );
        }
    }

    #[test]
    fn multiprocess_writer_helper() {
        let Ok(root) = std::env::var("LATEXD_PREPARED_ASSET_CACHE_TEST_CHILD") else {
            return;
        };
        let (request, materialized) = fixture();
        let lookup_hash =
            PreparedAssetCache::lookup_hash(&request, b"png-bytes", &BTreeMap::new(), "source");
        PreparedAssetCache::new(Utf8PathBuf::from(root))
            .store(&lookup_hash, &request, &materialized)
            .expect("child process store");
    }

    #[test]
    fn concurrent_process_stores_are_atomic_for_overlapping_readers() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).expect("utf8 path");
        let (request, materialized) = fixture();
        let lookup_hash =
            PreparedAssetCache::lookup_hash(&request, b"png-bytes", &BTreeMap::new(), "source");
        let cache = PreparedAssetCache::new(root.clone());
        let current_exe = std::env::current_exe().expect("current test executable");
        let mut children = (0..4)
            .map(|_| {
                Command::new(&current_exe)
                    .args([
                        "--exact",
                        "prepared_asset_cache::tests::multiprocess_writer_helper",
                        "--nocapture",
                    ])
                    .env("LATEXD_PREPARED_ASSET_CACHE_TEST_CHILD", root.as_str())
                    .spawn()
                    .expect("spawn cache writer")
            })
            .collect::<Vec<_>>();

        loop {
            let mut running = false;
            for child in &mut children {
                match child.try_wait().expect("poll cache writer") {
                    Some(status) => assert!(status.success(), "cache writer failed: {status}"),
                    None => running = true,
                }
            }
            assert!(cache.load(&lookup_hash, &request).is_ok());
            if !running {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        assert_eq!(
            cache
                .load(&lookup_hash, &request)
                .expect("load process entry"),
            Some(materialized)
        );
    }
}
