//! Processor state and idle-to-process context switching for chapter 5.
//!
//! The idle context runs the scheduler; it is not a user process or PID.
//! While a process runs, `current` owns it and its status is `Running`.
//! No dynamic borrow may remain active across a context switch.

#![allow(dead_code)]

use super::__switch;
use super::{fetch_task, TaskStatus};
use super::{TaskContext, TaskControlBlock};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use alloc::sync::Arc;
use lazy_static::*;

/// Processor management structure
pub struct Processor {
    ///The task currently executing on the current processor
    current: Option<Arc<TaskControlBlock>>,

    ///The basic control flow of each core, helping to select and switch process
    idle_task_cx: TaskContext,
}

impl Processor {
    ///Create an empty Processor
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
        }
    }

    ///Get mutable reference to `idle_task_cx`
    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut _
    }

    ///Get current task in moving semanteme
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }

    ///Get current task in cloning semanteme
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(Arc::clone)
    }
}

lazy_static! {
    pub static ref PROCESSOR: UPSafeCell<Processor> = unsafe { UPSafeCell::new(Processor::new()) };
}

/// Run the scheduler's idle control flow.
///
/// Inputs: The global processor and ready queue, with initproc already enqueued.
/// Output: Repeatedly dispatch ready processes; never return to kernel startup.
/// Constraints: Select through `fetch_task`, mark the process `Running`, and
/// install it as `current` before switching from idle to its task context.
/// Resume selection when idle regains control. An empty queue keeps idle alive.
/// Release all dynamic borrows before switching and keep both contexts valid.
pub fn run_tasks() {
    loop {
        if let Some(task) = fetch_task() {
            let mut processor = PROCESSOR.exclusive_access();
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
            let mut inner = task.inner_exclusive_access();
            inner.task_status = TaskStatus::Running;
            let next_task_cx_ptr = &inner.task_cx as *const TaskContext;
            drop(inner);
            // Move ownership into current; leave no extra Arc on the idle stack.
            processor.current = Some(task);
            drop(processor);
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        }
    }
}

/// Get current task through take, leaving a None in its place
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}

/// Get a copy of the current task
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}

/// Get the current user token(addr of page table)
pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    task.get_user_token()
}

///Get the mutable reference to trap context of current task
pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}

/// Save the caller's task context and return control to idle.
///
/// Inputs: `switched_task_cx_ptr` points to writable task-context storage that
/// remains valid during the switch. The caller has removed `current` and
/// finished the process's state and queue updates, releasing all dynamic borrows.
/// Output: Idle resumes scheduling. This call returns only if the saved context
/// is later scheduled again; the exit path's context is never resumed.
/// Constraints: Switch to the processor's idle context without selecting a task
/// or changing its stride here. Hold no processor borrow across the switch.
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let idle_task_cx_ptr = PROCESSOR.exclusive_access().get_idle_task_cx_ptr();
    // The temporary processor borrow has ended before changing control flow.
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}
