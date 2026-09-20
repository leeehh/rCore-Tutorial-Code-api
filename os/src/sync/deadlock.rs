//! Resource accounting shared by mutex and semaphore deadlock detection.

use super::UPSafeCell;
use crate::task::{current_task, TaskControlBlock};
use alloc::{sync::Arc, vec, vec::Vec};

/// Resource allocation and pending requests.
#[derive(Default)]
pub struct DeadlockDetector {
    /// Whether to reject unsafe requests.
    pub enabled: bool,
    available: Vec<usize>,
    allocation: Vec<Vec<usize>>,
    need: Vec<Option<usize>>,
}

impl DeadlockDetector {
    /// Check whether all threads can finish.
    pub fn is_safe(&self) -> bool {
        let mut work = self.available.clone();
        let mut finish = vec![false; self.need.len()];
        loop {
            let mut progress = false;
            for (tid, done) in finish.iter_mut().enumerate() {
                if !*done && self.need[tid].map_or(true, |id| work[id] > 0) {
                    for (free, held) in work.iter_mut().zip(&self.allocation[tid]) {
                        *free += held;
                    }
                    *done = true;
                    progress = true;
                }
            }
            if !progress {
                return finish.iter().all(|done| *done);
            }
        }
    }

    fn ensure_thread(&mut self, tid: usize) {
        self.need.resize(self.need.len().max(tid + 1), None);
        self.allocation
            .resize_with(self.need.len(), || vec![0; self.available.len()]);
    }

    /// Forget a thread without releasing its resources.
    pub fn remove_thread(&mut self, tid: usize) {
        if tid < self.need.len() {
            self.need[tid] = None;
            self.allocation[tid].fill(0);
        }
    }
}

/// A resource tracked by a detector.
pub struct Resource {
    id: usize,
    detector: Arc<UPSafeCell<DeadlockDetector>>,
}

impl Resource {
    /// Register a resource with its initial count.
    pub fn new(detector: Arc<UPSafeCell<DeadlockDetector>>, count: usize) -> Self {
        let mut state = detector.exclusive_access();
        let id = state.available.len();
        state.available.push(count);
        for row in &mut state.allocation {
            row.push(0);
        }
        drop(state);
        Self { id, detector }
    }

    /// Record a pending request and return its thread ID.
    pub fn wait(&self) -> usize {
        let tid = current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        let mut state = self.detector.exclusive_access();
        state.ensure_thread(tid);
        state.need[tid] = Some(self.id);
        tid
    }

    /// Record a request, returning false if rejected.
    pub fn request(&self) -> bool {
        let tid = self.wait();
        let mut state = self.detector.exclusive_access();
        if state.enabled && !state.is_safe() {
            state.need[tid] = None;
            return false;
        }
        true
    }

    /// Assign one unit to a thread.
    pub fn acquire(&self, task: &TaskControlBlock) {
        let tid = task.inner_exclusive_access().res.as_ref().unwrap().tid;
        let mut state = self.detector.exclusive_access();
        state.ensure_thread(tid);
        state.available[self.id] -= 1;
        state.allocation[tid][self.id] += 1;
        state.need[tid] = None;
    }

    /// Return one unit from the current thread.
    pub fn release(&self) {
        let tid = current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        let mut state = self.detector.exclusive_access();
        state.ensure_thread(tid);
        state.allocation[tid][self.id] = state.allocation[tid][self.id].saturating_sub(1);
        state.available[self.id] += 1;
    }
}
