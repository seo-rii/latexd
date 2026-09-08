//! Synchronous bootstrap output policy and measurements. The daemon owns a
//! production display-list PDF, so its bootstrap passes need only legacy layout
//! metadata. Standalone bootstrap callers continue to receive a legacy PDF.
use std::{cell::RefCell, rc::Rc, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyPdfPolicy {
    Eager,
    MetadataOnly,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BootstrapBuildWork {
    pub legacy_layout_runs: u64,
    pub legacy_layout_source_bytes: u64,
    pub legacy_pdf_exports: u64,
    pub legacy_pdf_bytes: u64,
    pub legacy_layout_us: u64,
    pub legacy_pdf_us: u64,
}

struct ScopeState {
    policy: LegacyPdfPolicy,
    work: RefCell<BootstrapBuildWork>,
}

thread_local! {
    static ACTIVE: RefCell<Vec<Rc<ScopeState>>> = const { RefCell::new(Vec::new()) };
}

/// Applies only to this synchronous thread. This type is deliberately !Send and
/// !Sync: do not keep it across an async yield. Nested scopes choose the output
/// policy independently; all work also contributes to enclosing scope meters.
pub struct BootstrapBuildScope {
    state: Rc<ScopeState>,
}

impl BootstrapBuildScope {
    pub fn start(policy: LegacyPdfPolicy) -> Self {
        let state = Rc::new(ScopeState {
            policy,
            work: RefCell::new(BootstrapBuildWork::default()),
        });
        ACTIVE.with(|active| active.borrow_mut().push(state.clone()));
        Self { state }
    }

    pub fn snapshot(&self) -> BootstrapBuildWork {
        *self.state.work.borrow()
    }
}

impl Drop for BootstrapBuildScope {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            active
                .borrow_mut()
                .retain(|scope| !Rc::ptr_eq(scope, &self.state));
        });
    }
}

pub(crate) fn export_legacy_pdf() -> bool {
    ACTIVE.with(|active| {
        active
            .borrow()
            .last()
            .is_none_or(|scope| scope.policy == LegacyPdfPolicy::Eager)
    })
}

pub(crate) fn record_layout(source_bytes: usize, elapsed: Duration) {
    ACTIVE.with(|active| {
        for scope in active.borrow().iter() {
            let mut work = scope.work.borrow_mut();
            work.legacy_layout_runs += 1;
            work.legacy_layout_source_bytes += source_bytes as u64;
            work.legacy_layout_us += elapsed.as_micros().min(u64::MAX as u128) as u64;
        }
    });
}

pub(crate) fn record_pdf(bytes: usize, elapsed: Duration) {
    ACTIVE.with(|active| {
        for scope in active.borrow().iter() {
            let mut work = scope.work.borrow_mut();
            work.legacy_pdf_exports += 1;
            work.legacy_pdf_bytes += bytes as u64;
            work.legacy_pdf_us += elapsed.as_micros().min(u64::MAX as u128) as u64;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        build_project_pdf, build_project_pdf_from_checkpoint, compile_mini_kernel_snapshot,
        run_project_pdf_from_base_snapshot,
    };
    use camino::Utf8PathBuf;
    use tex_world::ProjectWorld;

    #[test]
    fn metadata_only_preserves_run_layout_and_replay_without_exporting_unused_pdf() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        std::fs::write(
            root.join("main.tex"),
            r"\documentclass{article}\begin{document}Alpha $x^2$\newpage Bravo\end{document}",
        )
        .unwrap();
        let world = ProjectWorld::load(root).unwrap();
        let eager = build_project_pdf(&world).unwrap();
        assert!(eager.pdf_bytes.starts_with(b"%PDF-"));
        let scope = BootstrapBuildScope::start(LegacyPdfPolicy::MetadataOnly);
        let (mut metadata, checkpoint) =
            run_project_pdf_from_base_snapshot(&world, &compile_mini_kernel_snapshot()).unwrap();
        assert!(metadata.pdf_bytes.is_empty());
        let replay = build_project_pdf_from_checkpoint(
            &world,
            &checkpoint,
            &metadata.run.output[..checkpoint.output_offset_utf8 as usize],
        )
        .unwrap();
        assert!(replay.pdf_bytes.is_empty());
        assert_eq!(replay.run.output, metadata.run.output);
        assert_eq!(replay.run.render_events, metadata.run.render_events);
        assert_eq!(replay.layout, metadata.layout);
        assert_eq!(replay.page_metadata, metadata.page_metadata);
        let work = scope.snapshot();
        assert_eq!(work.legacy_layout_runs, 2);
        assert_eq!(
            work.legacy_layout_source_bytes,
            (metadata.run.output.len() * 2) as u64
        );
        assert_eq!(work.legacy_pdf_exports, 0);
        assert_eq!(work.legacy_pdf_bytes, 0);
        assert_eq!(work.legacy_pdf_us, 0);
        metadata.pdf_bytes = eager.pdf_bytes.clone();
        assert_eq!(metadata, eager);
        drop(scope);
        assert!(
            build_project_pdf(&world)
                .unwrap()
                .pdf_bytes
                .starts_with(b"%PDF-")
        );
    }

    #[test]
    fn nested_scopes_meter_real_work_and_restore_policy_on_unwind() {
        let scope = BootstrapBuildScope::start(LegacyPdfPolicy::MetadataOnly);
        assert!(!export_legacy_pdf());
        let result = std::panic::catch_unwind(|| {
            let nested = BootstrapBuildScope::start(LegacyPdfPolicy::Eager);
            assert!(export_legacy_pdf());
            record_layout(12, Duration::from_micros(3));
            record_pdf(40, Duration::from_micros(5));
            assert_eq!(nested.snapshot(), scope_work());
            panic!("scope teardown witness");
        });
        assert!(result.is_err());
        assert!(!export_legacy_pdf());
        assert_eq!(scope.snapshot(), scope_work());
        std::thread::spawn(|| {
            assert!(export_legacy_pdf());
            record_layout(99, Duration::ZERO);
        })
        .join()
        .unwrap();
        assert_eq!(scope.snapshot(), scope_work());
        drop(scope);
        assert!(export_legacy_pdf());
    }

    fn scope_work() -> BootstrapBuildWork {
        BootstrapBuildWork {
            legacy_layout_runs: 1,
            legacy_layout_source_bytes: 12,
            legacy_pdf_exports: 1,
            legacy_pdf_bytes: 40,
            legacy_layout_us: 3,
            legacy_pdf_us: 5,
        }
    }
}
