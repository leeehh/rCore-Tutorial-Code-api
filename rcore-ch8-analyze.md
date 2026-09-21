# rCore ch8：源代码分析与 GDB 动态跟踪

阅读 `ch8` 的参考实现，分析线程同步、资源交接及当前请求的安全性判断。

## 1. 使用说明

在 `ch8` 分支的 `os` 目录构建调试内核：

```bash
make build MODE=debug BASE=2
```

内核使用 `MODE=debug`，用户程序与文件系统镜像保持 release 构建。debug 内核使用较大的栈，容纳未优化的进程创建调用；日常运行和实验验收继续使用原有命令。

先按下表阅读源码：

| 阅读位置 | 关注内容 |
| --- | --- |
| [sync/mutex.rs](os/src/sync/mutex.rs)、[semaphore.rs](os/src/sync/semaphore.rs)、[condvar.rs](os/src/sync/condvar.rs) | 让出与阻塞、FIFO 交接、条件等待后重新持锁。 |
| [sync/deadlock.rs](os/src/sync/deadlock.rs)、[syscall/sync.rs](os/src/syscall/sync.rs) | 请求登记、安全性模拟、拒绝与返回值转换。 |
| [task/mod.rs](os/src/task/mod.rs)、[manager.rs](os/src/task/manager.rs)、[process.rs](os/src/task/process.rs) | 线程状态、唤醒、同步对象表和两个独立检测器。 |

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

运行到 QEMU shell 后输入 `ch8_deadlock_mutex1`，观察同一线程两次请求同一把阻塞锁。`mutex_id` 是当前进程对象表的索引。对照源码区分 `Resource::request` 和 `mutex.lock`；拒绝后返回 `-0xDEAD`，不能继续阻塞或获取资源。

### 观察安全性判断

```gdb
tbreak 'os::sync::deadlock::DeadlockDetector::is_safe'
continue
bt 6
p *self
```

继续同一测例，观察 `enabled`、`available`、`allocation`、`need`，再结合源码解释临时 `work` 与 `finish`。向量可能只显示长度和指针，应按实际可见内容记录。要比较重复加锁，可再次设置同名临时断点后继续；注意区分启用检测时的空状态检查和请求检查。

若函数名未解析，用 `info functions is_safe` 查询符号。局部变量须初始化后再观察；`is_safe` 不修改真实分配，撤销拒绝请求由 `Resource::request` 完成。单个互斥测例不代表动态覆盖了所有同步原语。

## 3. 需要追踪的调用链

| 调用链 | 需要解释的状态变化 |
| --- | --- |
| `sys_mutex_lock` → `Resource::request` → `is_safe` → `Mutex::lock` | 请求登记、检查、获取或等待。 |
| `MutexSpin::lock` → `suspend_current_and_run_next`；`MutexBlocking::lock` → `block_current_and_run_next` | `Ready` 重试与 FIFO 排队进入 `Blocked` 的区别。 |
| `MutexBlocking::unlock` → `Resource::release/acquire` → `wakeup_task` | 先把资源交给队首再唤醒，锁仍被占用。 |
| `Semaphore::down` / `up` → 请求、计数、入队或资源交付 | 负计数表示等待者，可用资源非负，恢复后不重复记账。 |
| `Condvar::wait` → 解锁、入队、阻塞、重新加锁；`signal` → `wakeup_task` | 通知不积累、不自动交锁，等待返回前重新持锁。 |

分析关闭检测仍需记账、每线程只有一项单位请求、两类资源分别检测的边界。条件变量不入资源图，线程清理记录不自动归还资源；重获锁不增加第二次拒绝检查。参考实现与 API 核心语义一致，不改变锁接口或扩展检测范围。

## 4. 报告要求

报告保存到 `reports/lab8.md`，保留对应 GDB 日志。结合线程状态、资源交接和安全性模拟解释上述功能，可附必要代码片段或源码链接。

若选择本章独立实现，比较自己的同步实现与参考实现，说明等待、唤醒、借用释放和请求拒绝处理的差异，保持 API 的资源模型及单核假设。

总结主要问题、排查依据和解决思路，区分实际观察与源码推导。GDB 跟踪用于分析，功能验收继续使用 API 文档原有命令。
