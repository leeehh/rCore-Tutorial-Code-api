# rCore ch3：源代码分析与 GDB 动态跟踪

阅读 `ch3` 的参考实现，分析任务状态、轮转调度与上下文切换如何配合，使多个应用轮流运行。

## 1. 使用说明

在 `ch3` 分支的 `os` 目录构建调试内核：

```bash
make build MODE=debug BASE=2
```

本节使用 `MODE=debug`；日常运行和实验验收继续使用原有命令。

先按下表阅读源码：

| 阅读位置 | 关注内容 |
| --- | --- |
| [task/mod.rs](os/src/task/mod.rs)、[task/task.rs](os/src/task/task.rs) | `TASK_MANAGER` 初始化、任务状态、当前任务编号和就绪任务选择。 |
| [loader.rs](os/src/loader.rs)、[task/context.rs](os/src/task/context.rs) | 应用栈、初始 Trap 上下文和首次恢复所需的任务上下文。 |
| [task/switch.S](os/src/task/switch.S) | `ra`、`sp` 和 `s0`–`s11` 的保存与恢复。 |
| [trap/mod.rs](os/src/trap/mod.rs)、[syscall/process.rs](os/src/syscall/process.rs)、[timer.rs](os/src/timer.rs) | 主动让出、时钟抢占及任务退出的调度入口。 |

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
set logging file ../reports/ch3-gdb.log
set logging enabled on
target remote localhost:1234
```

### 观察首个任务启动

```gdb
tbreak os::task::run_first_task
continue
bt 6
tbreak *__switch
continue
info registers a0 a1 ra sp
x/14gx $a1
```

`__switch` 入口的 `a0` 指向待保存上下文，`a1` 指向待恢复上下文。结合 `TaskContext` 的布局，解释读取的 14 个机器字如何对应 `ra`、`sp` 和 `s0`–`s11`。首次启动时下一任务的 `ra` 指向恢复应用的入口。继续在 `__restore` 停下时，`sp` 指向 Trap 上下文，可用 `x/34gx $sp` 观察，再从其中的 `sepc` 设置用户入口断点。

### 观察任务暂停

```gdb
tbreak os::syscall::process::sys_yield
continue
tbreak os::task::suspend_current_and_run_next
continue
bt 6
tbreak *__switch
continue
info registers a0 a1 sp
x/14gx $a1
```

这里先选中 `sys_yield` 主动让出路径，记录调用栈，并对照源码说明当前任务怎样从 `Running` 变成 `Ready`、后继任务如何选中。`__switch` 入口的 `a0` 是待保存位置，`a1` 是待恢复位置。

记录这次 `__switch` 入口的 `ra` 和 `sp`，在保存的 `ra` 地址设置断点，并以 `sp` 等于保存值作为条件；命中后检查原任务的调用栈与上下文，观察其恢复执行。结合 `disassemble __switch` 阅读保存和恢复寄存器的过程。

## 3. 需要追踪的调用链

这些箭头包含普通调用和上下文转移。用各阶段的断点记录说明执行过程，不要求它们同时出现在一个调用栈中。

| 调用链 | 触发与观察任务 |
| --- | --- |
| `rust_main` → `run_first_task` → `TASK_MANAGER` 初始化与访问 → `__switch` → `__restore` | 启动时记录应用数量、首个任务上下文及进入用户态的过程；结合初始化循环说明任务槽位状态。 |
| `sys_yield` → `suspend_current_and_run_next` → 选择就绪任务 → `__switch`，随后恢复原任务 | `ch3b_yield0/1/2` 主动让出，按样例记录当前任务、后继任务、上下文及原任务返回点。 |
| 时钟中断 → `set_next_trigger` → `suspend_current_and_run_next` → `__switch` | 在 `trap_handler` 设置条件 `$scause == 0x8000000000000005` 的断点，记录时钟抢占的调用栈和后续切换。 |
| `sys_exit` 或应用异常 → `exit_current_and_run_next` → 后继任务 | hello/power/yield 正常结束或 bad 指令应用异常时，记录退出入口与后继任务；通过源码阅读说明 `Exited` 的排除规则和没有就绪任务时的结束分支。 |
| `syscall` → `record_current_syscall`；`sys_trace` → `current_syscall_count` | 在计数记录处比较两个不同任务的编号与计数单元；阅读 `sys_trace` 的查询路径，说明每任务独立计数和先计数后分发的顺序。 |

阅读切换前的 `drop(inner)`，结合后继任务再次进入任务管理接口的记录，说明借用释放与上下文指针有效性的关系。

## 4. 报告要求

报告保存到 `reports/lab3.md`，保留对应 GDB 日志。结合任务状态变化、调用栈和上下文内容解释调度过程，可附必要的代码片段或源码链接。

若选择本章独立实现，比较自己的任务管理实现与参考实现，说明功能相同但内部组织不同的部分。特别核对有效应用数量与数组容量：参考初始化循环遍历整个任务数组，而 API 要求未使用槽位保持 `UnInit`，实现时应遵守接口约定。

总结主要问题、排查依据和解决思路，区分实际观察与源码推导。GDB 跟踪用于分析，功能验收继续使用 API 文档原有命令。
