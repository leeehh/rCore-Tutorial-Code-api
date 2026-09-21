# rCore ch5：源代码分析与 GDB 动态跟踪

阅读 `ch5` 的参考实现，分析进程创建、程序替换、退出回收和 stride 调度如何配合。

## 1. 使用说明

在 `ch5` 分支的 `os` 目录构建调试内核：

```bash
make build MODE=debug BASE=2
```

本节使用 `MODE=debug`；日常运行和实验验收继续使用原有命令。

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

### 观察调度选择

```gdb
tbreak 'os::task::manager::TaskManager::fetch'
continue
bt 6
next
next
info locals
```

在启动阶段记录一次调度：继续执行到 `inner` 初始化之后，观察选中的 `index`、进程的 `stride` 和 `prio`，并比较累加前后的 stride。通过源码阅读说明最小 stride 的选择规则，以及优先级怎样决定 `BIG_STRIDE / prio`。

### 观察进程复制

```gdb
tbreak os::syscall::process::sys_fork
continue
bt 6
info registers pc sp
```

初始进程第一次调用 `fork` 时即可命中。结合 `list`、`next` 观察 `current_task`、`new_task`、`new_pid` 的建立，区分地址空间复制、父子关系登记、子进程 `a0 = 0` 和入队由谁完成。用户返回现场位于 TrapContext，不能把内核调用栈当成用户程序调用栈。

动态跟踪限于启动阶段的进程创建、第一次 fork 和调度选择。完成这些观察后结束调试，其余进程生命周期通过源码分析。

## 3. 需要追踪的调用链

| 调用链 | 触发时机 | 需要记录的状态 |
| --- | --- | --- |
| `add_initproc` → `INITPROC` 延迟初始化 → `TaskControlBlock::new`，随后 `add_task` | 内核启动 | 独立 PID、内核栈、地址空间、初始 TrapContext 与 `Ready` 状态。 |
| `sys_fork` → `TaskControlBlock::fork`，随后 syscall 设置子进程返回值并 `add_task` | `ch5b_initproc` 第一次 fork | 用户空间深拷贝、父子关系、子进程内核栈和 `a0=0`。 |
| `run_tasks` → `fetch_task` → `TaskManager::fetch` | 启动阶段的一次调度 | 被选进程的编号、优先级及 stride 累加前后的值。 |

通过源码阅读分析 `exec` 替换地址空间、`spawn` 创建子进程、主动让出与 idle 上下文恢复，以及退出、孤儿移交和 `waitpid` 回收的调用关系，说明各阶段的状态与资源归属。`__switch` 恢复 idle 保存的现场后，`run_tasks` 从原有切换位置继续执行。

参考分支将等待回收和优先级逻辑写在 `sys_waitpid`、`sys_set_priority` 中，API 已拆为 `TaskControlBlock::waitpid`、`set_priority`。参考 `exec` 没有更新堆边界，API 要求重置 `heap_bottom`、`program_brk`，应按契约实现。分析还需说明退出路径为何主动释放局部 `Arc`，以及 `TaskContext` 与 TrapContext 的不同用途。

## 4. 报告要求

报告保存到 `reports/lab5.md`，保留启动阶段的 GDB 日志。结合实际记录解释进程创建、第一次 fork 和一次调度；通过源码分析说明其余生命周期与调度规则，可附必要代码片段或源码链接。

若选择本章独立实现，比较自己的进程生命周期和调度实现与参考实现，说明 TCB 接口拆分、资源归属和实现方式不同的原因，遵守 API 指定范围。

总结主要问题、排查依据和解决思路，区分实际观察与源码推导。GDB 跟踪用于分析，功能验收继续使用 API 文档原有命令。
