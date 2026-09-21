# rCore ch2：源代码分析与 GDB 动态跟踪

阅读 `ch2` 的参考实现，分析用户程序如何通过系统调用进入内核，以及内核如何恢复应用或转入下一个应用。

## 1. 使用说明

在 `ch2` 分支的 `os` 目录构建调试内核：

```bash
make build MODE=debug BASE=2
```

本节使用 `MODE=debug`；日常运行和实验验收继续使用原有命令。

先按下表阅读源码：

| 阅读位置 | 关注内容 |
| --- | --- |
| [main.rs](os/src/main.rs)、[batch.rs](os/src/batch.rs) | Trap 初始化、应用顺序加载、初始上下文和批次结束。 |
| [trap.S](os/src/trap/trap.S)、[context.rs](os/src/trap/context.rs) | 用户栈与内核栈切换，寄存器保存恢复，`sret`。 |
| [trap/mod.rs](os/src/trap/mod.rs) | 系统调用、应用异常与不支持的 Trap 如何分流。 |
| [syscall/mod.rs](os/src/syscall/mod.rs)、[fs.rs](os/src/syscall/fs.rs)、[process.rs](os/src/syscall/process.rs) | `write`、`exit` 的参数转换及返回方式。 |

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
set logging file ../reports/ch2-gdb.log
set logging enabled on
target remote localhost:1234
```

### 观察应用加载与非法指令异常

```gdb
tbreak os::batch::AppManager::load_app
continue
info args
bt 6
tbreak *__restore
continue
info registers a0
x/34gx $a0
tbreak trap_handler if $scause == 2
continue
bt 6
info registers scause stval
p/x cx->sepc
```

`load_app` 的 `app_id` 是本次加载的应用序号。`__restore` 入口的 `a0` 指向 `TrapContext`，34 个机器字依次保存通用寄存器、`sstatus` 和 `sepc`；其中 `x[2]` 是用户栈指针。随后用异常条件断点选中 `ch2b_bad_instructions` 或 `ch2b_bad_register` 的非法指令，记录 `scause`、`stval` 和保存的 `sepc`。

再次在 `AppManager::load_app` 设置断点，记录后继应用序号。各应用复用同一装入地址，应通过 `app_id` 和终端的加载信息区分应用。

### 观察 write 分发与返回

```gdb
tbreak trap_handler if $scause == 8 && cx->x[17] == 64
continue
set $write_cx = cx
set $write_sepc = cx->sepc
tbreak os::syscall::syscall
continue
info args
bt 6
finish
next
p/x $write_cx->sepc - $write_sepc
p/d $write_cx->x[10]
```

这里选中编号为 `64` 的 `write`，记录 `syscall_id`、`args` 和调用栈。`finish` 返回 `trap_handler` 后，执行 `next` 完成返回值写回，再观察 `sepc` 增量和 `x[10]`。对照缓冲区长度，解释返回值及恢复后越过 `ecall` 的原因。

继续运行，记录后续应用的输出与最后的 `All applications completed!`。使用 `info functions` 查找函数符号，结合汇编中的 `sret` 说明应用恢复过程。

## 3. 需要追踪的调用链

| 调用链 | 触发与观察任务 |
| --- | --- |
| `rust_main` → `batch::run_next_app` → `__restore` → 用户程序 | 启动时记录应用序号、初始 `sepc` 和用户栈。在上下文保存的 `sepc` 地址设置断点，观察应用入口的 `pc`、`sp`；阅读恢复汇编，说明进入 U 模式的过程。 |
| 用户 `ecall` → `__alltraps` → `trap_handler` → `syscall` → `sys_write`，随后经 `__restore` 返回 | 由 `ch2b_hello_world` 或 power 程序输出触发，按样例记录参数、返回值与 `sepc` 的更新。 |
| 应用异常 → `trap_handler` → `run_next_app` → `__restore` | `ch2b_bad_instructions` 和 `ch2b_bad_register` 触发非法指令异常；记录一次异常现场和后继应用序号。 |
| `sys_exit` → `run_next_app` → `__restore` 或批次结束 | 在 `os::syscall::process::sys_exit` 和 `AppManager::load_app` 设置断点，观察 hello/power 程序正常退出后继续加载；最后记录整个批次结束。 |

调用链包含函数调用、汇编跳转及恢复执行，分别通过断点与源码说明。`StoreFault`、`StorePageFault` 分支，以及不支持的系统调用和其他 Trap 分支，只做源码分析。

## 4. 报告要求

报告保存到 `reports/lab2.md`，保留对应 GDB 日志。说明上述调用关系，选取关键断点记录、寄存器或参数值解释系统调用与异常的区别；可附必要的代码片段或源码链接。

若选择本章独立实现，补充自己的 `syscall()`、`trap_handler()` 与参考实现的异同及原因，并按 API 文档原有命令验收。总结主要问题、排查依据和解决思路，区分实际观察与根据源码作出的推导。
