//! Task management for chapter 3, with syscall accounting provided.
//!
//! This kernel runs on a single core. No borrow of the task manager's internal
//! state may remain active across a context switch.

// Allow unused items, imports, and parameters in the exercise skeleton.
#![allow(dead_code, unused_imports, unused_variables)]

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::config::{MAX_APP_NUM, MAX_SYSCALL_NUM};
use crate::loader::{get_num_app, init_app_cx};
use crate::sync::UPSafeCell;
use lazy_static::lazy_static;
use switch::__switch;

pub use context::TaskContext;
pub use task::{TaskControlBlock, TaskStatus};

/// Manages the statically loaded applications.
pub struct TaskManager {
    /// Number of loaded applications, in `1..=MAX_APP_NUM`.
    num_app: usize,
    /// Mutable task management state with runtime borrow checking.
    inner: UPSafeCell<TaskManagerInner>,
}

/// Mutable state of the task manager.
pub struct TaskManagerInner {
    /// Task control blocks indexed by task ID; `0..num_app` covers loaded applications.
    tasks: [TaskControlBlock; MAX_APP_NUM],
    /// ID of the current task; initialized to 0 before any task starts.
    current_task: usize,
}

lazy_static! {
    /// Global task manager.
    pub static ref TASK_MANAGER: TaskManager = TaskManager::new();
}

impl TaskManager {
    /// Todo: Create a task manager for the loaded applications.
    ///
    /// Inputs: No arguments. The loader provides the application count and
    /// initial application contexts.
    /// Output: A task manager whose loaded tasks are `Ready`, each with a context
    /// for its first run, and whose `current_task` is 0.
    /// Constraints: The application count is in `1..=MAX_APP_NUM`, with one task
    /// per loaded application. Unused slots are `UnInit`, and all syscall counters
    /// start at zero.
    fn new() -> Self {
        todo!("task::TaskManager::new")
    }

    /// Todo: Start the first task.
    ///
    /// Inputs: `self` is initialized and has not started any task.
    /// Output: Application 0 begins execution. This function does not return.
    /// Constraints: Task 0 is `Running` and is identified by `current_task`.
    /// No borrow of `inner` may remain active across the context switch.
    fn run_first_task(&self) -> ! {
        todo!("task::TaskManager::run_first_task")
    }

    /// Todo: Mark the current task as ready to run again.
    ///
    /// Inputs: `current_task` identifies a `Running` task in `self`.
    /// Output: Returns `()` with the current task in the `Ready` state.
    /// Constraints: The task retains the context needed to resume execution.
    fn mark_current_suspended(&self) {
        todo!("task::TaskManager::mark_current_suspended")
    }

    /// Todo: Mark the current task as exited.
    ///
    /// Inputs: `current_task` identifies a `Running` task in `self`.
    /// Output: Returns `()` with the current task in the `Exited` state.
    /// Constraints: Exited tasks must never be scheduled again.
    fn mark_current_exited(&self) {
        todo!("task::TaskManager::mark_current_exited")
    }

    /// Todo: Select the next ready task.
    ///
    /// Inputs: The number of loaded tasks, the current task ID, and task states in `self`.
    /// Output: `Some(id)` for the selected ready task, or `None` if no task is ready.
    /// Constraints: The selected ID is in `0..num_app` and identifies a `Ready` task.
    /// Selection follows round-robin order by task ID, starting after `current_task`.
    fn find_next_task(&self) -> Option<usize> {
        todo!("task::TaskManager::find_next_task")
    }

    /// Todo: Switch to the next ready task.
    ///
    /// Inputs: `self` has already started scheduling tasks. The current task is
    /// `Ready` or `Exited`.
    /// Output: The selected task takes over execution. This call returns `()`
    /// only when the original task resumes.
    /// Constraints: `current_task` identifies the task now running, whose state
    /// is `Running`. Context pointers must remain valid, and no borrow of `inner`
    /// may remain active across the context switch.
    fn run_next_task(&self) {
        todo!("task::TaskManager::run_next_task")
    }
}

/// Record one system call made by the current task.
pub fn record_current_syscall(syscall_id: usize) {
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    if let Some(count) = inner.tasks[current].syscall_counts.get_mut(syscall_id) {
        *count += 1;
    }
}

/// Return the current task's call count for the given syscall ID.
pub fn current_syscall_count(syscall_id: usize) -> usize {
    let inner = TASK_MANAGER.inner.exclusive_access();
    inner.tasks[inner.current_task]
        .syscall_counts
        .get(syscall_id)
        .copied()
        .unwrap_or(0)
}

/// Todo: Start the first task during kernel startup.
///
/// Inputs: No arguments. Uses `TASK_MANAGER` after the applications have been loaded.
/// Output: The first application begins execution; control does not return to the
/// startup code.
/// Constraints: This entry point is only for initial task startup and must retain
/// the signature expected by the kernel's startup code.
pub fn run_first_task() {
    todo!("task::run_first_task")
}

/// Todo: Suspend the current task and yield the CPU.
///
/// Inputs: No arguments. The current task in `TASK_MANAGER` is `Running`.
/// Output: Returns `()` when the current task resumes execution.
/// Constraints: The suspended task remains eligible to run. This entry point
/// must support both voluntary yielding and timer preemption.
pub fn suspend_current_and_run_next() {
    todo!("task::suspend_current_and_run_next")
}

/// Todo: Exit the current task and relinquish the CPU.
///
/// Inputs: No arguments. The current task in `TASK_MANAGER` is `Running`.
/// Output: The current task terminates and does not resume at the call site.
/// Constraints: The task is `Exited` and must never be scheduled again.
pub fn exit_current_and_run_next() {
    todo!("task::exit_current_and_run_next")
}
