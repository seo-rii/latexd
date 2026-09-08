//! Request-scoped instrumentation for synchronous VM execution. Measurements
//! are deliberately outside semantic snapshots and outcome equality.
use std::{cell::RefCell, marker::PhantomData, rc::Rc};

thread_local! { static ACTIVE: RefCell<Vec<Rc<RefCell<VmWork>>>> = const { RefCell::new(Vec::new()) }; }

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VmWork {
    pub executions: u64,
    /// Tokens dispatched by the execution queue (including macro expansion).
    /// Argument tokens consumed inside a primitive are not dispatched separately.
    pub dispatched_tokens: u64,
}

/// A meter must surround a synchronous block and cannot cross executor threads.
/// Nested meters contribute to their enclosing request, including bootstrap runs.
pub struct VmWorkMeter {
    work: Rc<RefCell<VmWork>>,
    _same_thread: PhantomData<Rc<()>>,
}

impl VmWorkMeter {
    pub fn start() -> Self {
        let work = Rc::new(RefCell::new(VmWork::default()));
        ACTIVE.with(|active| active.borrow_mut().push(work.clone()));
        Self {
            work,
            _same_thread: PhantomData,
        }
    }
    pub fn snapshot(&self) -> VmWork {
        *self.work.borrow()
    }
}

impl Drop for VmWorkMeter {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            active
                .borrow_mut()
                .retain(|entry| !Rc::ptr_eq(entry, &self.work))
        });
    }
}

pub(crate) fn record_execution(tokens: usize) {
    ACTIVE.with(|active| {
        for meter in active.borrow().iter() {
            let mut work = meter.borrow_mut();
            work.executions += 1;
            work.dispatched_tokens += tokens as u64;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meters_count_real_dispatch_and_keep_thread_requests_isolated() {
        let meter = VmWorkMeter::start();
        let mut interner = tex_tokens::ControlSequenceInterner::new();
        let mut vm = crate::Vm::new(&mut interner);
        let initial = vm.run_plain("abc");
        assert_eq!(initial.output.trim(), "abc");
        assert_eq!(meter.snapshot().executions, 1);
        assert!(meter.snapshot().dispatched_tokens >= 3);
        let before = meter.snapshot();
        std::thread::spawn(|| {
            let meter = VmWorkMeter::start();
            let mut interner = tex_tokens::ControlSequenceInterner::new();
            let mut vm = crate::Vm::new(&mut interner);
            vm.run_plain("another request");
            assert_eq!(meter.snapshot().executions, 1);
        })
        .join()
        .unwrap();
        assert_eq!(meter.snapshot(), before);
        let nested = VmWorkMeter::start();
        vm.run_plain("more");
        assert_eq!(nested.snapshot().executions, 1);
        assert_eq!(meter.snapshot().executions, 2);
        drop(nested);
        vm.run_plain("last");
        assert_eq!(meter.snapshot().executions, 3);
    }
}
