//! Task control blocks and task states for chapter 3.

use super::TaskContext;
use crate::config::MAX_SYSCALL_NUM;

/// Task control block for a statically loaded application.
///
/// Each loaded task is identified by its index in the task array. Its status and
/// context describe its execution state.
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// Current state in the task's lifecycle.
    pub task_status: TaskStatus,
    /// Kernel register context used when suspending or resuming the task.
    pub task_cx: TaskContext,
    /// Syscall counters for the provided `sys_trace`, initialized to zero for each task.
    pub syscall_counts: [usize; MAX_SYSCALL_NUM],
}

/// A task's lifecycle state.
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// Uninitialized task slot.
    UnInit,
    /// Ready to be scheduled.
    Ready,
    /// Currently executing.
    Running,
    /// Finished execution; a terminal state.
    Exited,
}
