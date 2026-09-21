# rCore ch7：源代码分析与 GDB 动态跟踪

阅读 `ch7` 的参考实现，分析管道字节传输、描述符共享、等待与 EOF 的形成。

## 1. 使用说明

在 `ch7` 分支的 `os` 目录构建调试内核：

```bash
make build MODE=debug BASE=2
```

内核使用 `MODE=debug`，用户程序与文件系统镜像保持 release 构建。debug 内核使用较大的栈，容纳未优化的进程创建调用；日常运行和实验验收继续使用原有命令。

先按下表阅读源码：

| 阅读位置 | 关注内容 |
| --- | --- |
| [fs/pipe.rs](os/src/fs/pipe.rs) | 32 字节环形缓冲区、端点权限、读写等待与写端弱引用。 |
| [syscall/fs.rs](os/src/syscall/fs.rs)、[mm/page_table.rs](os/src/mm/page_table.rs) | 管道创建、描述符复制、用户地址转换和分片缓冲区。 |
| [task/task.rs](os/src/task/task.rs)、[task/mod.rs](os/src/task/mod.rs) | `fork` 共享端点、退出释放引用和让出 CPU。 |

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
set logging file ../reports/ch7-gdb.log
set logging enabled on
target remote localhost:1234
```

### 观察管道创建

```gdb
tbreak os::syscall::fs::sys_pipe
continue
bt 6
info args
p pipe
```

运行到 QEMU shell 后输入已有 `ch7b_pipetest`。观察 `make_pipe` 的两个端点及 `read_fd`、`write_fd` 的分配：第一次取得的空位先占用，第二次才能得到另一个描述符。`pipe` 为用户地址，其结果须经已有地址转换写回。

### 观察缓冲区写入

```gdb
tbreak 'os::fs::pipe::PipeRingBuffer::write_byte'
continue
info args
p byte
p *self
```

在同一测例中继续，观察 `head`、`tail`、`status`，结合 `next` 查看一次写入后的变化。临时断点记录一次写入；再结合 `ch7b_pipe_large_test` 分析超过容量时的分批传输与读写等待。

对下表各路径结合源码说明，并使用对应程序观察。描述符结果从 syscall 的内核变量取得，再对照已有用户地址转换过程。

## 3. 需要追踪的调用链

| 调用链或事件关系 | 触发程序或 shell 命令 | 需要解释的状态变化 |
| --- | --- | --- |
| `sys_pipe` → `make_pipe` → `PipeRingBuffer::new/set_write_end` | `ch7b_pipetest` | 两端共享缓冲区、写端弱引用、两个描述符。 |
| `sys_write` → `Pipe::write` → `available_write/write_byte` | `ch7b_pipetest`；`ch7b_pipe_large_test` | FIFO 写入，满时释放借用并让出 CPU。 |
| `sys_read` → `Pipe::read` → `available_read/read_byte` | `ch7b_pipetest`；`ch7b_pipe_large_test` | 消费数据、暂时无数据时等待、写端关闭后的短读。 |
| `fork` 克隆文件描述符中的管道端点 `Arc` | `ch7b_pipetest` 创建管道后 fork | 父子进程共享端点和缓冲区。 |
| `sys_dup` 克隆已有打开对象 `Arc` | `ch2b_hello_world > audit7`；随后 `ch7b_cat audit7` | shell 关闭描述符 1 后复制输出文件描述符，得到 1 并共享同一打开对象。 |
| `sys_close` 释放写端引用；后续 `Pipe::read` → `all_write_ends_closed` | `ch7b_pipetest` | 父子都关闭写端，读完残留数据后以 13 字节短读返回。 |
| 退出路径清空文件描述符表 | 上述程序退出 | 释放进程持有的文件对象引用。 |

关闭端点和查询 EOF 是先后发生的事件：`sys_close` 改变引用计数，之后由读取路径检查写端是否全部关闭，不是 `sys_close` 直接调用关闭查询。

解释 `head == tail` 为什么需结合状态区分空满，以及 `dup`、父子进程和 syscall 临时引用如何延长写端寿命。参考 `ch7` 的零长度读写可能先进入空满等待分支，API 契约要求立即返回 `0`；应明确此差异，不能直接沿用参考实现的边界行为。

## 4. 报告要求

报告保存到 `reports/lab7.md`，保留对应 GDB 日志。结合环形状态、读写等待和引用生命周期解释上述功能，可附必要代码片段或源码链接。

若选择本章独立实现，比较自己的管道实现与参考实现，说明缓冲区操作、借用释放、分批传输和零长度处理的差异，遵守 API 指定的 EOF 与读写契约。

总结主要问题、排查依据和解决思路，区分实际观察与源码推导。GDB 跟踪用于分析，功能验收继续使用 API 文档原有命令。
