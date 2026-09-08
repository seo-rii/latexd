//! Revision roots owned by connected viewers and queued/running render work.
//! Collection and new ownership acquisition share a gate, so a root cannot be
//! added between the GC mark and sweep. Dropping an owner needs no async work.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

use camino::{Utf8Path, Utf8PathBuf};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

#[derive(Debug, Default)]
struct LeaseState {
    clients: Vec<Weak<AtomicU64>>,
    jobs: BTreeMap<u64, Weak<()>>,
}

#[derive(Debug, Default)]
struct Store {
    state: Arc<AsyncMutex<LeaseState>>,
}

fn store(build_root: &Utf8Path) -> Arc<Store> {
    static STORES: OnceLock<Mutex<BTreeMap<Utf8PathBuf, Weak<Store>>>> = OnceLock::new();
    let mut stores = STORES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(store) = stores.get(build_root).and_then(Weak::upgrade) {
        return store;
    }
    stores.retain(|_, store| store.strong_count() > 0);
    let store = Arc::new(Store::default());
    stores.insert(build_root.to_owned(), Arc::downgrade(&store));
    store
}

pub(crate) struct ClientLease {
    store: Arc<Store>,
    build_root: Utf8PathBuf,
    revision: Arc<AtomicU64>,
}

impl ClientLease {
    pub(crate) async fn new(build_root: &Utf8Path) -> Self {
        let store = store(build_root);
        let revision = Arc::new(AtomicU64::new(0));
        {
            let mut state = store.state.lock().await;
            state.clients.retain(|client| client.strong_count() > 0);
            state.clients.push(Arc::downgrade(&revision));
        }
        Self {
            store,
            build_root: build_root.to_owned(),
            revision,
        }
    }

    /// Each connection owns at most one preview. Zero explicitly releases it.
    pub(crate) async fn set_revision(&self, rev: u64, published_rev: u64) -> bool {
        let _gate = self.store.state.lock().await;
        if rev > published_rev
            || (rev != 0
                && !self
                    .build_root
                    .join(format!("rev-{rev}/sources.json"))
                    .is_file())
        {
            return false;
        }
        self.revision.store(rev, Ordering::Release);
        true
    }
}

#[derive(Debug)]
pub(crate) struct RevisionPin {
    _store: Arc<Store>,
    _owner: Arc<()>,
}

pub(crate) async fn pin_job(build_root: &Utf8Path, rev: u64) -> RevisionPin {
    let store = store(build_root);
    let owner = {
        let mut state = store.state.lock().await;
        state.jobs.retain(|_, owner| owner.strong_count() > 0);
        if let Some(owner) = state.jobs.get(&rev).and_then(Weak::upgrade) {
            owner
        } else {
            let owner = Arc::new(());
            state.jobs.insert(rev, Arc::downgrade(&owner));
            owner
        }
    };
    RevisionPin {
        _store: store,
        _owner: owner,
    }
}

pub(crate) struct GcRoots {
    _store: Arc<Store>,
    state: OwnedMutexGuard<LeaseState>,
}

impl GcRoots {
    pub(crate) fn revisions(&self) -> BTreeSet<u64> {
        self.state
            .clients
            .iter()
            .filter_map(Weak::upgrade)
            .map(|rev| rev.load(Ordering::Acquire))
            .filter(|rev| *rev != 0)
            .chain(
                self.state
                    .jobs
                    .iter()
                    .filter(|(_, owner)| owner.strong_count() > 0)
                    .map(|(rev, _)| *rev),
            )
            .collect()
    }
}

pub(crate) async fn gc_roots(build_root: &Utf8Path) -> GcRoots {
    let store = store(build_root);
    let mut state = store.state.clone().lock_owned().await;
    state.clients.retain(|client| client.strong_count() > 0);
    state.jobs.retain(|_, owner| owner.strong_count() > 0);
    GcRoots {
        _store: store,
        state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn revision(root: &Utf8Path, rev: u64, owner: u64) {
        fs::create_dir_all(root.join(format!("rev-{owner}/pages"))).unwrap();
        fs::write(
            root.join(format!("rev-{owner}/pages/page.pdf")),
            "immutable artifact",
        )
        .unwrap();
        fs::create_dir_all(root.join(format!("rev-{rev}"))).unwrap();
        fs::write(root.join(format!("rev-{rev}/sources.json")), "{}").unwrap();
        let page = crate::compiler::PageArtifactMeta {
            page_id: "page".into(),
            index: 0,
            line_count: 1,
            width_pt: 128,
            height_pt: 128,
            content_hash: "paint".into(),
            text_start_utf8: 0,
            text_end_utf8: 1,
            pdf_artifact_path: format!("rev-{owner}/pages/page.pdf").into(),
            source_spans: vec![],
        };
        fs::write(
            root.join(format!("rev-{rev}/page-metadata.json")),
            serde_json::to_vec(&vec![page]).unwrap(),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn preview_leases_preserve_client_revision_and_older_artifact_owner_until_release() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        revision(root, 3, 1);
        revision(root, 20, 20);
        let client = ClientLease::new(root).await;
        assert!(client.set_revision(3, 20).await);
        assert!(!client.set_revision(21, 20).await);
        crate::prune_old_build_revisions(root, 20).await;
        assert!(root.join("rev-3/sources.json").exists());
        assert!(root.join("rev-1/pages/page.pdf").exists());
        assert!(client.set_revision(20, 20).await);
        crate::prune_old_build_revisions(root, 20).await;
        assert!(!root.join("rev-3").exists());
        assert!(!root.join("rev-1").exists());
        assert!(
            !client.set_revision(3, 20).await,
            "expired revision needs resync"
        );
        assert_eq!(gc_roots(root).await.revisions(), BTreeSet::from([20]));
        assert!(client.set_revision(0, 20).await);
        assert!(gc_roots(root).await.revisions().is_empty());
    }

    #[tokio::test]
    async fn preview_leases_disconnect_and_cancel_release_only_their_own_roots() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        revision(root, 2, 2);
        revision(root, 3, 3);
        revision(root, 20, 20);
        let first = ClientLease::new(root).await;
        let second = ClientLease::new(root).await;
        assert!(first.set_revision(2, 20).await);
        assert!(second.set_revision(3, 20).await);
        let job = pin_job(root, 2).await;
        drop(first);
        crate::prune_old_build_revisions(root, 20).await;
        assert!(root.join("rev-2").exists(), "job outlives its client");
        assert!(root.join("rev-3").exists(), "other client remains leased");
        drop(job);
        crate::prune_old_build_revisions(root, 20).await;
        assert!(!root.join("rev-2").exists());
        assert!(root.join("rev-3").exists());
        drop(second);
        crate::prune_old_build_revisions(root, 20).await;
        assert!(!root.join("rev-3").exists());
    }

    #[tokio::test]
    async fn preview_leases_gc_gate_prevents_new_roots_between_mark_and_sweep() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap().to_owned();
        let roots = gc_roots(&root).await;
        let pending = tokio::spawn({
            let root = root.clone();
            async move { pin_job(&root, 1).await }
        });
        tokio::task::yield_now().await;
        assert!(!pending.is_finished());
        drop(roots);
        let pin = pending.await.unwrap();
        assert_eq!(gc_roots(&root).await.revisions(), BTreeSet::from([1]));
        drop(pin);
        assert!(gc_roots(&root).await.revisions().is_empty());
    }

    #[tokio::test]
    async fn preview_leases_queued_render_retains_root_when_reply_is_cancelled() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        revision(root, 1, 1);
        revision(root, 20, 20);
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let sender = crate::RenderSessionSender {
            tx,
            build_root: root.to_owned(),
        };
        let (reply, response) = tokio::sync::oneshot::channel();
        sender
            .send(crate::RenderSessionRequest::RenderPage {
                page: tex_render_gs::PageRenderInput {
                    page_id: "page".into(),
                    revision: 1,
                    content_hash: "paint".into(),
                    width_px: 128,
                    height_px: 128,
                    pdf_path: root.join("rev-1/pages/page.pdf").to_string(),
                },
                scale: 1.0,
                reply,
            })
            .await
            .unwrap();
        drop(response);
        crate::prune_old_build_revisions(root, 20).await;
        assert!(
            root.join("rev-1/pages/page.pdf").exists(),
            "queued work owns its input"
        );
        let request = rx.recv().await.unwrap();
        crate::prune_old_build_revisions(root, 20).await;
        assert!(
            root.join("rev-1/pages/page.pdf").exists(),
            "ownership spans execution"
        );
        drop(request);
        crate::prune_old_build_revisions(root, 20).await;
        assert!(
            !root.join("rev-1").exists(),
            "completed work no longer pins artifacts"
        );
    }
}
