//! Process management syscalls
use alloc::sync::Arc;
use core::mem::size_of;

use crate::{
    config::PAGE_SIZE,
    loader::get_app_data_by_name,
    mm::{
        translated_byte_buffer, translated_refmut, translated_str, MapPermission, PTEFlags,
        PageTable, VirtAddr,
    },
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next,
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!(
        "kernel::pid[{}] sys_waitpid [{}]",
        current_task().unwrap().pid.0,
        pid
    );
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// 获取时间，并按当前进程的页表分段写回可能跨页的 TimeVal。
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel:pid[{}] sys_get_time", current_task().unwrap().pid.0);
    let token = current_user_token();
    let page_table = PageTable::from_token(token);
    let len = size_of::<TimeVal>();
    let mut start = ts as usize;
    let end = match start.checked_add(len) {
        Some(end) => end,
        None => return -1,
    };
    // 先检查所有页面，避免后续页不可写时只写入一部分数据。
    while start < end {
        let va = VirtAddr::from(start);
        if usize::from(va) != start {
            return -1;
        }
        match page_table.translate(va.floor()) {
            Some(pte)
                if pte
                    .flags()
                    .contains(PTEFlags::V | PTEFlags::U | PTEFlags::W) => {}
            _ => return -1,
        }
        start += (PAGE_SIZE - va.page_offset()).min(end - start);
    }
    let us = get_time_us();
    let time = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    // 两个 usize 字段连续且无填充；按字节复制不要求用户地址对齐。
    let bytes = unsafe { core::slice::from_raw_parts(&time as *const TimeVal as *const u8, len) };
    let mut offset = 0;
    for buffer in translated_byte_buffer(token, ts as *const u8, len) {
        let next = offset + buffer.len();
        buffer.copy_from_slice(&bytes[offset..next]);
        offset = next;
    }
    0
}

// 检查地址溢出和 Sv39 规范地址，并将结束地址向上按页对齐。
fn user_page_range(start: usize, len: usize) -> Option<(VirtAddr, VirtAddr)> {
    let end = start.checked_add(len)?.checked_add(PAGE_SIZE - 1)? & !(PAGE_SIZE - 1);
    let start_va = VirtAddr::from(start);
    let end_va = VirtAddr::from(end);
    if usize::from(start_va) != start || usize::from(end_va) != end || start_va >= end_va {
        return None;
    }
    Some((start_va, end_va))
}

/// 在当前进程的地址空间中建立匿名映射。
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel:pid[{}] sys_mmap", current_task().unwrap().pid.0);
    if start % PAGE_SIZE != 0 || prot == 0 || prot & !0x7 != 0 {
        return -1;
    }
    if len == 0 {
        return 0;
    }
    let (start_va, end_va) = match user_page_range(start, len) {
        Some(range) => range,
        None => return -1,
    };
    let permission = MapPermission::from_bits((prot as u8) << 1).unwrap() | MapPermission::U;
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    inner
        .memory_set
        .mmap(start_va, end_va, permission)
        .map_or(-1, |_| 0)
}

/// 解除当前进程中起止页面完全匹配的用户逻辑段。
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_munmap", current_task().unwrap().pid.0);
    if start % PAGE_SIZE != 0 {
        return -1;
    }
    if len == 0 {
        return 0;
    }
    let (start_va, end_va) = match user_page_range(start, len) {
        Some(range) => range,
        None => return -1,
    };
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    inner.memory_set.munmap(start_va, end_va).map_or(-1, |_| 0)
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// 从目标 ELF 创建子进程，成功返回 PID，文件名不存在时返回 -1。
pub fn sys_spawn(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_spawn", current_task().unwrap().pid.0);
    let path = translated_str(current_user_token(), path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let parent = current_task().unwrap();
        let child = parent.spawn(data);
        let pid = child.getpid();
        add_task(child);
        pid as isize
    } else {
        -1
    }
}

/// 设置当前进程优先级，合法时返回 prio，否则返回 -1。
pub fn sys_set_priority(prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority",
        current_task().unwrap().pid.0
    );
    if prio < 2 {
        return -1;
    }
    current_task().unwrap().inner_exclusive_access().prio = prio as usize;
    prio
}
