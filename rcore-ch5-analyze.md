# rCore ch5：源代码分析与 GDB 动态跟踪

阅读 `ch5` 的参考实现，分析进程创建、程序替换、退出回收和 stride 调度如何配合。

## 1. 使用说明

在 `ch5` 分支的 `os` 目录构建调试内核：

```bash
make build MODE=debug BASE=2
```

本节使用 `MODE=debug`，调试内核的进程内核栈为 64 KiB；日常运行和实验验收继续使用原有命令，release 内核栈仍为 8 KiB。

先按下表阅读源码：

| 阅读位置 | 关注内容 |
| --- | --- |
| [task/task.rs](os/src/task/task.rs)、[syscall/process.rs](os/src/syscall/process.rs) | 进程创建、复制、替换、父子关系、等待回收与优先级。 |
| [task/manager.rs](os/src/task/manager.rs)、[task/processor.rs](os/src/task/processor.rs) | 就绪队列、stride 选择、当前进程与 idle 上下文。 |
| [task/mod.rs](os/src/task/mod.rs)、[task/switch.S](os/src/task/switch.S) | 暂停、退出、孤儿移交、动态借用释放与上下文切换。 |

## 2. 操作样例

终端一在仓库根目录执行：

```bash
cd os
make gdbserver MODE=debug BASE=2
```

终端二在仓库根目录执行：

```bash
mkdir -p reports
cd os
riscv64-unknown-elf-gdb -nx -q target/riscv64gc-unknown-none-elf/debug/os
```

在 GDB 中连接并记录操作：

```gdb
set pagination off
set architecture riscv:rv64
set language c
set logging file ../reports/ch5-gdb.log
set logging enabled on
target remote localhost:1234
```

### 观察进程复制

```gdb
tbreak os::syscall::process::sys_fork
continue
bt 6
info registers pc sp
```

启动用户 shell 时即可命中。结合 `list`、`next` 观察 `current_task`、`new_task`、`new_pid` 的建立，区分地址空间复制、父子关系登记、子进程 `a0 = 0` 和入队由谁完成。用户返回现场位于 TrapContext，不能把内核调用栈当成用户程序调用栈。

### 观察调度选择

```gdb
tbreak 'os::task::manager::TaskManager::fetch'
continue
bt 6
next
next
info locals
```

对照源码追踪就绪队列与选中的 `index`；继续执行到 `inner` 初始化之后，再观察被选进程的 `stride`、`prio`。选择依据是累加前的 stride，选中后增加 `BIG_STRIDE / prio`。在局部变量初始化后记录 `index`，并比较一次调度前后的 stride。

对各项功能结合下表梳理源码调用关系；两个操作样例记录进程复制和调度的关键现场，表中程序可用于观察相应路径。

## 3. 需要追踪的调用链

| 调用链或控制流程 | 触发程序 | 需要解释的状态变化 |
| --- | --- | --- |
| `add_initproc` → `INITPROC` 延迟初始化 → `TaskControlBlock::new`，随后 `add_task` | 内核启动 | 独立 PID、内核栈、地址空间、初始 TrapContext 与 `Ready` 状态。 |
| `sys_fork` → `TaskControlBlock::fork`，随后 syscall 设置子进程返回值并 `add_task` | 启动的 `ch5b_initproc`；`ch5b_forktest_simple` | 用户空间深拷贝、父子关系、子进程内核栈和 `a0=0`。 |
| `sys_exec` → `TaskControlBlock::exec` | initproc 启动 shell；shell 执行任一程序 | PID 保持，用户程序、页表和入口替换。 |
| `sys_spawn` → `TaskControlBlock::spawn` → `new`，随后 `add_task` | `ch5_spawn1` | 直接从 ELF 创建子进程并登记父子关系。 |
| `run_tasks` → `fetch_task` → `TaskManager::fetch` | 启动；`ch5_stride` | 选择最小 stride，按优先级累加整数步长。 |
| `sys_yield` → `suspend_current_and_run_next` → `schedule` → `__switch` | `ch5b_exit` 中的显式 yield | 进程设为 `Ready` 并入队，保存当前现场、恢复 idle 上下文。 |
| `exit_current_and_run_next`；父进程随后执行 `sys_waitpid` | `ch5_spawn1`；`ch5b_exit` | `Zombie`、退出码、用户数据页释放和成功等待后的回收。 |
| 退出进程的 children 移交 `INITPROC` | `ch5b_forktree` | 子进程 parent 更新、INITPROC 接管和后续等待。 |

`__switch` 恢复 idle 保存的现场，`run_tasks` 从原有切换位置继续执行；这一步是上下文恢复，不是 `__switch` 对 `run_tasks` 的普通函数调用。分别在切换两侧记录调用栈与栈指针。

参考分支将等待回收和优先级逻辑写在 `sys_waitpid`、`sys_set_priority` 中，API 已拆为 `TaskControlBlock::waitpid`、`set_priority`。参考 `exec` 没有更新堆边界，API 要求重置 `heap_bottom`、`program_brk`，应按契约实现。分析还需说明退出路径为何主动释放局部 `Arc`，以及 `TaskContext` 与 TrapContext 的不同用途。

## 4. 报告要求

报告保存到 `reports/lab5.md`，保留对应 GDB 日志。结合进程身份、父子关系、上下文和调度字段解释上述功能，可附必要代码片段或源码链接。

若选择本章独立实现，比较自己的进程生命周期和调度实现与参考实现，说明 TCB 接口拆分、资源归属和实现方式不同的原因，遵守 API 指定范围。

总结主要问题、排查依据和解决思路，区分实际观察与源码推导。GDB 跟踪用于分析，功能验收继续使用 API 文档原有命令。
