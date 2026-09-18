//! Process management syscalls

use crate::timer::get_time_us;
use crate::{
    config::PAGE_SIZE,
    fs::{open_file, OpenFlags},
    mm::{
        translated_byte_buffer, translated_ref, translated_refmut, translated_str, MapPermission,
        PTEFlags, PageTable, VirtAddr,
    },
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next, pid2task,
        suspend_current_and_run_next, SignalAction, SignalFlags, MAX_SIG,
    },
};
use alloc::{string::String, sync::Arc, vec::Vec};
use core::mem::size_of;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
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

pub fn sys_exec(path: *const u8, mut args: *const usize) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    let mut args_vec: Vec<String> = Vec::new();
    loop {
        let arg_str_ptr = *translated_ref(token, args);
        if arg_str_ptr == 0 {
            break;
        }
        args_vec.push(translated_str(token, arg_str_ptr as *const u8));
        unsafe {
            args = args.add(1);
        }
    }
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        let argc = args_vec.len();
        task.exec(all_data.as_slice(), args_vec);
        // return argc because cx.x[10] will be covered with it later
        argc as isize
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
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

pub fn sys_kill(pid: usize, signum: i32) -> isize {
    trace!("kernel:pid[{}] sys_kill", current_task().unwrap().pid.0);
    if let Some(task) = pid2task(pid) {
        if let Some(flag) = SignalFlags::from_bits(1 << signum) {
            // insert the signal if legal
            let mut task_ref = task.inner_exclusive_access();
            if task_ref.signals.contains(flag) {
                return -1;
            }
            task_ref.signals.insert(flag);
            0
        } else {
            -1
        }
    } else {
        -1
    }
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
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let data = app_inode.read_all();
        let parent = current_task().unwrap();
        let child = parent.spawn(data.as_slice());
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

pub fn sys_sigprocmask(mask: u32) -> isize {
    trace!(
        "kernel:pid[{}] sys_sigprocmask",
        current_task().unwrap().pid.0
    );
    if let Some(task) = current_task() {
        let mut inner = task.inner_exclusive_access();
        let old_mask = inner.signal_mask;
        if let Some(flag) = SignalFlags::from_bits(mask) {
            inner.signal_mask = flag;
            old_mask.bits() as isize
        } else {
            -1
        }
    } else {
        -1
    }
}

pub fn sys_sigreturn() -> isize {
    trace!(
        "kernel:pid[{}] sys_sigreturn",
        current_task().unwrap().pid.0
    );
    if let Some(task) = current_task() {
        let mut inner = task.inner_exclusive_access();
        inner.handling_sig = -1;
        // restore the trap context
        let trap_ctx = inner.get_trap_cx();
        *trap_ctx = inner.trap_ctx_backup.unwrap();
        // Here we return the value of a0 in the trap_ctx,
        // otherwise it will be overwritten after we trap
        // back to the original execution of the application.
        trap_ctx.x[10] as isize
    } else {
        -1
    }
}

fn check_sigaction_error(signal: SignalFlags, action: usize, old_action: usize) -> bool {
    if action == 0
        || old_action == 0
        || signal == SignalFlags::SIGKILL
        || signal == SignalFlags::SIGSTOP
    {
        true
    } else {
        false
    }
}

pub fn sys_sigaction(
    signum: i32,
    action: *const SignalAction,
    old_action: *mut SignalAction,
) -> isize {
    trace!(
        "kernel:pid[{}] sys_sigaction",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if signum as usize > MAX_SIG {
        return -1;
    }
    if let Some(flag) = SignalFlags::from_bits(1 << signum) {
        if check_sigaction_error(flag, action as usize, old_action as usize) {
            return -1;
        }
        let prev_action = inner.signal_actions.table[signum as usize];
        *translated_refmut(token, old_action) = prev_action;
        inner.signal_actions.table[signum as usize] = *translated_ref(token, action);
        0
    } else {
        -1
    }
}
