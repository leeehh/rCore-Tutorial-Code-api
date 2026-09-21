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

`__switch` 入口的 `a0` 指向待保存上下文，`a1` 指向待恢复上下文。结合 `TaskContext` 的布局，解释读取的 14 个机器字如何对应 `ra`、`sp` 和 `s0`–`s11`。首次启动时下一任务的 `ra` 指向恢复应用的入口，不能把它当成某次普通函数调用的返回地址。

### 观察任务暂停

```gdb
tbreak os::task::suspend_current_and_run_next
continue
bt 6
tbreak *__switch
continue
info registers a0 a1 sp
x/14gx $a1
```

从调用栈判断暂停来自 `sys_yield` 还是时钟中断，并对照源码说明当前任务怎样从 `Running` 变成 `Ready`、后继任务如何选中。断在 `__switch` 入口时保存指令尚未执行，因此不能把此时 `a0` 指向的旧内容当成本次刚保存的现场。

不要用一次普通 `finish` 来推断整个任务切换已结束：切换会改变栈和返回地址，原任务要等再次被调度才会继续。GDB 在汇编边界的回溯也可能不完整，可结合 `disassemble __switch` 与源码判断。

## 3. 需要追踪的调用链

| 调用链 | 需要解释的状态变化 |
| --- | --- |
| `rust_main` → `run_first_task` → `TASK_MANAGER` 初始化与访问 → `__switch` → `__restore` | 应用数量、任务初始状态、首个任务上下文和启动后状态。 |
| `sys_yield` 或时钟中断 → `suspend_current_and_run_next` → 选择就绪任务 → `__switch` | 轮转顺序、当前任务编号、暂停后保存的执行进度，以及恢复后如何返回原调用点。 |
| `sys_exit` 或应用异常 → `exit_current_and_run_next` → 后继任务 | `Exited` 为什么不能再次调度；没有就绪任务时参考实现怎样结束。 |

同时说明为什么必须在上下文切换前释放任务管理状态的借用，以及任务切换怎样保持各任务的系统调用计数独立。

## 4. 报告要求

报告保存到 `reports/lab3.md`，保留对应 GDB 日志。结合任务状态变化、调用栈和上下文内容解释调度过程，可附必要的代码片段或源码链接。

若选择本章独立实现，比较自己的任务管理实现与参考实现，说明功能相同但内部组织不同的部分。特别核对有效应用数量与数组容量：参考初始化循环遍历整个任务数组，而 API 要求未使用槽位保持 `UnInit`，实现时应遵守接口约定。

总结主要问题、排查依据和解决思路，区分实际观察与源码推导。GDB 跟踪用于分析，功能验收继续使用 API 文档原有命令。
