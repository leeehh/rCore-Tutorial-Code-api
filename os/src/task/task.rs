//! 第三章任务控制块及任务状态定义。

use super::TaskContext;

/// 静态加载应用对应的任务控制块。
///
/// 每个有效任务由任务数组中的索引标识，其状态与上下文共同描述执行状态。
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// 任务的当前生命周期状态。
    pub task_status: TaskStatus,
    /// 任务在内核中暂停或恢复执行所需的寄存器上下文。
    pub task_cx: TaskContext,
}

/// 任务的生命周期状态。
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// 尚未初始化的任务槽位。
    UnInit,
    /// 已就绪，具备被调度运行的条件。
    Ready,
    /// 当前正在运行的任务。
    Running,
    /// 已结束执行的任务；该状态为终态。
    Exited,
}
