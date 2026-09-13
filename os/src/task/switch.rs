//! 已提供的任务切换汇编的 Rust 接口。
//!
//! switch.S 实现任务上下文切换；本实验通过此声明调用该实现。

use super::TaskContext;
use core::arch::global_asm;

global_asm!(include_str!("switch.S"));

extern "C" {
    /// 切换内核执行流的任务上下文。
    ///
    /// 输入：current_task_cx_ptr 指向本次执行上下文的可写存储，
    /// next_task_cx_ptr 指向待恢复的有效任务上下文。
    /// 输出：本次执行上下文保存在 current_task_cx_ptr 指向的位置，
    /// 执行流进入 next_task_cx_ptr 描述的上下文；原上下文恢复时本调用返回 ()。
    /// 关键约束：指针满足 TaskContext 的对齐与布局要求，其存储在切换和恢复期间
    /// 持续有效；切换时不得持有任务管理器内部数据的动态借用。
    pub fn __switch(current_task_cx_ptr: *mut TaskContext, next_task_cx_ptr: *const TaskContext);
}
