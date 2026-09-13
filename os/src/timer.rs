//! 第三章时间读取与时钟中断设置接口。
//!
//! 时间查询函数直接提供，待实现接口为 set_next_trigger。
//!
//! 输入：RISC-V 硬件时间计数、平台时钟频率及目标时钟中断频率。
//! 输出：硬件计数或换算后的时间，以及下一次时钟中断的触发时刻。
//! 关键约束：所有读数采用同一硬件时间基准，CLOCK_FREQ 的单位为 tick/秒。
//!
//! 已提供的依赖接口：
//! - riscv::register::time::read 返回当前硬件时间计数。
//! - sbi::set_timer 接收下一次时钟中断的绝对硬件时间计数。

// TODO 骨架保留了供实现使用的常量和导入，允许它们暂时未被使用。
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

/// 获取当前硬件时间计数。
///
/// 输入：无显式参数；时间来源为硬件时间计数器。
/// 输出：usize 类型的当前计数，单位为 tick。
/// 关键约束：每秒对应 CLOCK_FREQ 个 tick，与其他时间读取接口采用相同基准。
pub fn get_time() -> usize {
    time::read()
}

/// 获取以毫秒表示的当前时间。
///
/// 输入：无显式参数；硬件时间计数与 CLOCK_FREQ 提供时间基准。
/// 输出：usize 类型的毫秒数，取换算结果的整数部分。
/// 关键约束：时间基准与 get_time 一致，一秒对应 MSEC_PER_SEC 毫秒。
pub fn get_time_ms() -> usize {
    time::read() * MSEC_PER_SEC / CLOCK_FREQ
}

/// 获取以微秒表示的当前时间。
///
/// 输入：无显式参数；硬件时间计数与 CLOCK_FREQ 提供时间基准。
/// 输出：usize 类型的微秒数，取换算结果的整数部分。
/// 关键约束：时间基准与 get_time 一致，一秒对应 MICRO_PER_SEC 微秒。
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
