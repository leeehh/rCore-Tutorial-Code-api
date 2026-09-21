# rCore ch8：源代码分析与 GDB 动态跟踪

阅读 `ch8` 的参考实现，分析互斥锁、信号量、条件变量与线程等待、唤醒的关系。

## 1. 使用说明

在 `ch8` 分支的 `os` 目录构建调试内核：

```bash
make build MODE=debug BASE=2
```

内核使用 `MODE=debug`，用户程序与文件系统镜像保持 release 构建。debug 内核使用较大的栈，容纳未优化的进程创建调用；日常运行和实验验收继续使用原有命令。

本节保持死锁检测默认关闭，不调用 `sys_enable_deadlock_detect(1)`，不要求跟踪 `Resource` 或 `DeadlockDetector::is_safe`。

先按下表阅读源码：

| 阅读位置 | 关注内容 |
| --- | --- |
| [sync/mutex.rs](os/src/sync/mutex.rs)、[semaphore.rs](os/src/sync/semaphore.rs)、[condvar.rs](os/src/sync/condvar.rs) | 让出与阻塞、FIFO 交接、条件等待后重新持锁。 |
| [syscall/sync.rs](os/src/syscall/sync.rs) | 同步对象的创建、参数传递和操作入口。 |
| [task/mod.rs](os/src/task/mod.rs)、[manager.rs](os/src/task/manager.rs)、[process.rs](os/src/task/process.rs) | 线程状态、唤醒、就绪队列和同步对象表。 |

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
set logging file ../reports/ch8-gdb.log
set logging enabled on
target remote localhost:1234
```

### 观察互斥请求

```gdb
tbreak os::syscall::sync::sys_mutex_lock
continue
bt 6
info args
p mutex_id
```

运行到 QEMU shell 后输入 `ch8b_test_condvar`，观察线程获取互斥锁的入口。`mutex_id` 是当前进程对象表的索引。对照源码说明如何找到锁对象，以及条件等待为什么需要先释放锁、返回前重新获取锁。

### 观察信号量

```gdb
tbreak 'os::sync::semaphore::Semaphore::down'
continue
bt 6
p self->inner
```

设置断点并继续后，等待前一个程序完成，在 QEMU shell 输入 `ch8b_sync_sem`。观察信号量计数和等待队列，结合 `next` 与源码说明计数递减、线程入队和阻塞的条件，以及 `up` 如何唤醒等待者。队列可能只显示长度和指针，应按实际可见内容记录。

若函数名未解析，用 `info functions Semaphore::down` 查询符号。局部变量须初始化后再观察；遇到配套记账调用可用 `next` 跳过。调试暂停可能改变线程交错顺序，应区分实际观察与源码推导。

## 3. 需要追踪的调用链

| 调用链 | 需要解释的状态变化 |
| --- | --- |
| `sys_mutex_lock` → `Mutex::lock` | 根据对象编号取得锁，获取成功或进入等待。 |
| `MutexSpin::lock` → `suspend_current_and_run_next`；`MutexBlocking::lock` → `block_current_and_run_next` | `Ready` 重试与 FIFO 排队进入 `Blocked` 的区别。 |
| `MutexBlocking::unlock` → `wakeup_task` | 唤醒队首等待者时，锁仍被占用。 |
| `Semaphore::down` / `up` → 计数变化、入队阻塞或唤醒 | 计数与等待线程数量的关系，许可不足时的等待与恢复。 |
| `Condvar::wait` → 解锁、入队、阻塞、重新加锁；`signal` → `wakeup_task` | 通知不积累、不自动交锁，等待返回前重新持锁。 |

重点比较让出 CPU、阻塞和唤醒的区别，说明为什么切换前要释放内部借用，以及条件变量的等待为什么需要与互斥锁配合。上述调用链省略配套记账过程。

## 4. 报告要求

报告保存到 `reports/lab8.md`，保留对应 GDB 日志。结合锁状态、信号量计数、等待队列和线程状态解释上述功能，可附必要代码片段或源码链接。

若选择本章独立实现，比较自己的同步实现与参考实现，说明等待、唤醒、借用释放和条件等待后重新持锁的处理，遵守 API 文档的接口契约。

总结主要问题、排查依据和解决思路，区分实际观察与源码推导。GDB 跟踪用于分析，功能验收继续使用 API 文档原有命令。
