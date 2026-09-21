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

### 观察 Trap 现场

```gdb
tbreak trap_handler
continue
bt 6
info registers scause stval
p/x cx->sepc
p/x cx->x[17]
p/x cx->x[10]
```

结合 `scause` 判断这次进入内核的原因。只有用户系统调用路径才把保存的 `a7` 解释为调用号；异常应用的寄存器值不能直接当作有效的系统调用参数。`cx->sepc` 保存的是应用现场，而当前 `$pc` 位于内核。

### 观察系统调用分发

```gdb
tbreak os::syscall::syscall
continue
info args
bt 6
```

记录 `syscall_id` 和 `args`，对照分发到的 `sys_write` 或 `sys_exit`。阅读 `trap_handler` 中 `sepc += 4` 和返回值写入 `x[10]` 的代码，解释为什么恢复后不会重复执行同一条 `ecall`。`sys_exit` 不返回当前应用，不能用 `finish` 等待它正常返回。

`continue` 可继续运行已有应用。观察异常应用之后是否还会加载后续应用，以及最后是否出现 `All applications completed!`。GDB 连接断开应结合 QEMU 终端输出判断。调试中若断点名称无法解析，可先用 `info functions` 查找符号；跨汇编入口时，`bt` 不一定能显示完整调用过程。

## 3. 需要追踪的调用链

| 调用链 | 需要解释的状态变化 |
| --- | --- |
| `rust_main` → `batch::run_next_app` → `__restore` → 用户程序 | 应用装入位置、初始 `sepc` 和用户栈，以及进入 U 模式的过程。 |
| 用户 `ecall` → `__alltraps` → `trap_handler` → `syscall` → `sys_write` → `__restore` | 保存的参数、系统调用分发、返回值与 `sepc` 的更新。 |
| `sys_exit` 或应用异常 → `run_next_app` → `__restore` | 旧应用不再返回，新应用使用新上下文；全部应用结束后退出。 |

不支持的系统调用和其他 Trap 分支通过源码分析说明即可，不新增程序来人为触发。

## 4. 报告要求

报告保存到 `reports/lab2.md`，保留对应 GDB 日志。说明上述调用关系，选取关键断点记录、寄存器或参数值解释系统调用与异常的区别；可附必要的代码片段或源码链接。

若选择本章独立实现，补充自己的 `syscall()`、`trap_handler()` 与参考实现的异同及原因，并按 API 文档原有命令验收。总结主要问题、排查依据和解决思路，区分实际观察与根据源码作出的推导。
