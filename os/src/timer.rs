//! 时间查询函数已提供，set_next_trigger 为待实现的时钟中断接口。

// 允许骨架中尚未使用的常量和导入。
#![allow(dead_code, unused_imports)]

use crate::config::CLOCK_FREQ;
use crate::sbi::set_timer;
use riscv::register::time;

/// 目标时钟中断频率，单位为次/秒。
const TICKS_PER_SEC: usize = 100;
/// 一秒包含的毫秒数。
const MSEC_PER_SEC: usize = 1000;
/// 一秒包含的微秒数。
const MICRO_PER_SEC: usize = 1_000_000;

/// 获取当前硬件时间计数（tick）。
pub fn get_time() -> usize {
    time::read()
}

/// 获取当前时间（毫秒）。
pub fn get_time_ms() -> usize {
    time::read() * MSEC_PER_SEC / CLOCK_FREQ
}

/// 获取当前时间（微秒）。
pub fn get_time_us() -> usize {
    time::read() * MICRO_PER_SEC / CLOCK_FREQ
}

/// Todo: 安排下一次时钟中断。
///
/// 输入：无显式参数；当前硬件时间计数与 TICKS_PER_SEC 确定触发时刻。
/// 输出：返回 ()，下一次时钟中断的触发时刻已设定。
/// 关键约束：触发时刻相对当前时间间隔为 1 / TICKS_PER_SEC 秒；
/// 提交给 SBI 的时间使用绝对硬件计数，单位为 tick。
pub fn set_next_trigger() {
    todo!("timer::set_next_trigger")
}
