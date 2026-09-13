//! 第三章任务管理，系统调用统计接口已提供。
//!
//! 运行环境为单核；上下文切换时不得持有任务管理器内部数据的动态借用。

// 允许骨架中尚未使用的定义、导入和参数。
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

/// 静态应用的任务管理器。
pub struct TaskManager {
    /// 实际加载的应用数量，满足 1 <= num_app <= MAX_APP_NUM。
    num_app: usize,
    /// 单核环境下可动态借用的任务管理状态。
    inner: UPSafeCell<TaskManagerInner>,
}

/// 任务管理器的内部可变状态。
pub struct TaskManagerInner {
    /// 按任务编号索引的控制块数组，0..num_app 对应有效应用。
    tasks: [TaskControlBlock; MAX_APP_NUM],
    /// 当前任务编号；首次启动前为 0，运行期间指向当前任务。
    current_task: usize,
}

lazy_static! {
    /// 全局任务管理器。
    pub static ref TASK_MANAGER: TaskManager = TaskManager::new();
}

impl TaskManager {
    /// Todo: 创建已加载应用对应的任务管理器。
    ///
    /// 输入：无显式参数；应用已加载，数量与初始上下文由 loader 提供。
    /// 输出：有效任务均为 Ready 并具有首次运行上下文的管理器，current_task 为 0。
    /// 关键约束：应用数量处于 1..=MAX_APP_NUM；有效任务与加载的应用一一对应，
    /// 其余任务槽位为 UnInit；每个任务的 syscall_counts 初值均为 0。
    fn new() -> Self {
        todo!("task::TaskManager::new")
    }

    /// Todo: 首次启动任务执行。
    ///
    /// 输入：self 为已初始化且尚未启动任务的管理器。
    /// 输出：执行权交给编号为 0 的应用；正常执行不返回。
    /// 关键约束：首个任务的状态为 Running，current_task 与之对应；
    /// 切换时不得持有 inner 的动态借用。
    fn run_first_task(&self) -> ! {
        todo!("task::TaskManager::run_first_task")
    }

    /// Todo: 将当前任务标记为可再次运行。
    ///
    /// 输入：self 的 current_task 指向 Running 任务。
    /// 输出：返回 ()，当前任务的状态为 Ready。
    /// 关键约束：该任务仍保有恢复执行所需的上下文。
    fn mark_current_suspended(&self) {
        todo!("task::TaskManager::mark_current_suspended")
    }

    /// Todo: 将当前任务标记为已退出。
    ///
    /// 输入：self 的 current_task 指向 Running 任务。
    /// 输出：返回 ()，当前任务的状态为 Exited。
    /// 关键约束：Exited 任务不再参与后续调度。
    fn mark_current_exited(&self) {
        todo!("task::TaskManager::mark_current_exited")
    }

    /// Todo: 选择下一次运行的任务。
    ///
    /// 输入：self 中的有效任务数量、当前任务编号及各任务状态。
    /// 输出：Some(id) 表示选中的就绪任务编号；None 表示没有就绪任务。
    /// 关键约束：返回编号位于 0..num_app，且对应任务为 Ready；
    /// 就绪任务的调度优先级按当前编号之后的循环编号顺序排列。
    fn find_next_task(&self) -> Option<usize> {
        todo!("task::TaskManager::find_next_task")
    }

    /// Todo: 将执行权交给下一次运行的任务。
    ///
    /// 输入：self 中的任务调度已经启动，当前任务已标记为 Ready 或 Exited。
    /// 输出：执行权交给选中的就绪任务；原任务恢复执行时，本调用返回 ()。
    /// 关键约束：current_task 与实际运行任务一致，其状态为 Running；
    /// 切换时不得持有 inner 的动态借用，上下文指针所指存储必须持续有效。
    fn run_next_task(&self) {
        todo!("task::TaskManager::run_next_task")
    }
}

/// 记录当前任务的一次系统调用。
pub fn record_current_syscall(syscall_id: usize) {
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    if let Some(count) = inner.tasks[current].syscall_counts.get_mut(syscall_id) {
        *count += 1;
    }
}

/// 查询当前任务的系统调用次数。
pub fn current_syscall_count(syscall_id: usize) -> usize {
    let inner = TASK_MANAGER.inner.exclusive_access();
    inner.tasks[inner.current_task]
        .syscall_counts
        .get(syscall_id)
        .copied()
        .unwrap_or(0)
}

/// Todo: 内核启动代码使用的首个任务入口。
///
/// 输入：无显式参数；应用已加载，依赖全局 TASK_MANAGER。
/// 输出：首个应用开始执行；正常执行不返回启动代码。
/// 关键约束：仅用于首次启动任务，保留与启动框架约定的函数签名。
pub fn run_first_task() {
    todo!("task::run_first_task")
}

/// Todo: 暂停当前任务并让出执行权。
///
/// 输入：无显式参数；TASK_MANAGER 的当前任务为 Running。
/// 输出：当前任务重新获得执行权时返回 ()，继续原来的执行流。
/// 关键约束：暂停的任务保持可再次调度；适用于主动让出 CPU 和时钟抢占。
pub fn suspend_current_and_run_next() {
    todo!("task::suspend_current_and_run_next")
}

/// Todo: 结束当前任务并交出执行权。
///
/// 输入：无显式参数；TASK_MANAGER 的当前任务为 Running。
/// 输出：当前任务结束执行；正常执行不返回该任务的调用点。
/// 关键约束：退出的任务处于 Exited 状态，后续调度不会再次运行它。
pub fn exit_current_and_run_next() {
    todo!("task::exit_current_and_run_next")
}
