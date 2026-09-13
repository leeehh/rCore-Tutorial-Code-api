//! 任务在内核中暂停和恢复执行所需的寄存器上下文。
//!
//! TaskContext 的字段布局与已提供的 switch.S 一致。
//! 首次运行的入口由已提供的 Trap 返回汇编 __restore 提供。

/// 与 switch.S 共享布局的任务上下文。
///
/// 关键约束：保留 repr(C) 及 ra、sp、s 的字段顺序；s 对应 s0..s11。
#[derive(Copy, Clone)]
#[repr(C)]
pub struct TaskContext {
    /// 上下文恢复后的执行入口或返回位置。
    ra: usize,
    /// 该执行流的内核栈指针。
    sp: usize,
    /// 按 RISC-V 调用约定保存的 s0..s11 寄存器。
    s: [usize; 12],
}

extern "C" {
    /// 已提供的 Trap 返回汇编入口，使用 sp 指向的 TrapContext。
    fn __restore();
}

impl TaskContext {
    /// 创建空任务上下文。
    ///
    /// 输入：无。
    /// 输出：ra、sp 以及 s 的所有元素均为 0 的 TaskContext。
    /// 关键约束：保留与 switch.S 约定的字段布局。
    pub fn zero_init() -> Self {
        todo!("task::TaskContext::zero_init")
    }

    /// 创建应用首次运行所需的任务上下文。
    ///
    /// 输入：kstack_ptr 为 loader::init_app_cx 返回的内核栈上 TrapContext 地址。
    /// 输出：可由 __switch 恢复、并通过 __restore 首次进入应用的 TaskContext。
    /// 关键约束：ra 对应 __restore 入口，sp 等于 kstack_ptr，s0..s11 初值为 0；
    /// 输入地址所指的 TrapContext 已正确初始化，且在首次恢复前持续有效。
    pub fn goto_restore(kstack_ptr: usize) -> Self {
        todo!("task::TaskContext::goto_restore")
    }
}
