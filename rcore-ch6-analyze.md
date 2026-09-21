# rCore ch6：源代码分析与 GDB 动态跟踪

阅读 `ch6` 的参考实现，分析磁盘 inode、打开文件对象与文件描述符如何共同完成文件操作。

## 1. 使用说明

在 `ch6` 分支的 `os` 目录构建调试内核：

```bash
make build MODE=debug BASE=2
```

内核使用 `MODE=debug`，进程内核栈为 64 KiB；用户程序与文件系统镜像保持 release 构建。日常运行和实验验收继续使用原有命令，release 内核栈仍为 8 KiB。

先按下表阅读源码：

| 阅读位置 | 关注内容 |
| --- | --- |
| [easy-fs/src/vfs.rs](easy-fs/src/vfs.rs)、[layout.rs](easy-fs/src/layout.rs) | 目录项、磁盘 inode、块索引、硬链接与资源回收。 |
| [fs/inode.rs](os/src/fs/inode.rs)、[fs/mod.rs](os/src/fs/mod.rs) | 打开标志、访问权限、打开偏移和 `Stat`。 |
| [syscall/fs.rs](os/src/syscall/fs.rs)、[task/task.rs](os/src/task/task.rs) | 描述符分配、地址转换、关闭和 `fork` 共享打开对象。 |

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
set logging file ../reports/ch6-gdb.log
set logging enabled on
target remote localhost:1234
```

### 观察打开文件

```gdb
tbreak os::fs::inode::open_file
continue
info args
p name
p flags
```

启动时加载初始应用即可命中。`name` 是带长度的 Rust 字符串，按 `length` 判断内容，不把指针后面的相邻字节算入文件名。结合 `list`、`next` 确认当前打开标志及执行分支；创建和清空路径使用下表中的既有程序观察。每次打开创建新的 `OSInode`，克隆文件描述符中的 `Arc` 则共享已有打开偏移。

### 观察加载文件内容

```gdb
tbreak 'os::fs::inode::OSInode::read_all'
continue
bt 6
info args
p *self
```

观察 `inner.offset`、每次读取的 `len` 和返回向量 `v` 如何累积。局部值须执行到初始化后再用 `info locals` 检查。将本次 ELF 加载与下表中的用户文件读写调用方对应起来。

结合下表梳理各项文件操作。读取 syscall 文件名时观察已有地址转换后的内核字符串；读取 `&str` 时按其长度取内容。

## 3. 需要追踪的调用链

| 调用链 | 触发程序或操作 | 需要解释的状态变化 |
| --- | --- | --- |
| `open_file` → `Inode::find`，随后 `OSInode::read_all` → `Inode::read_at` | 内核启动加载初始程序 | 打开对象、当前偏移和 ELF 内容读取。 |
| `sys_open` → `open_file` → `Inode::find/create` | 新镜像中运行 `ch6_file0` | 名称定位、新 inode、目录项和描述符分配。 |
| `open_file` → `Inode::clear` → `clear_inode_data` | 同一 QEMU 中再次运行 `ch6_file0` | CREATE 打开已有文件会清空内容，保留 inode 身份和链接数。 |
| `sys_read` → `OSInode::read` → `Inode::read_at` → `DiskInode::read_at` | `ch6_file0` | 分片缓冲区、实际读取长度和打开偏移。 |
| `sys_write` → `OSInode::write` → `Inode::write_at` → `increase_size` | `ch6_file0`；`ch6_file3` | 文件扩容、直接及间接索引块、缓存同步。 |
| `sys_linkat/sys_unlinkat` → `link_file/unlink_file` → `Inode::link/unlink` | `ch6_file2` | 多个名称共享 inode、链接数增减、空目录项和最后链接回收。 |
| `sys_fstat` → `OSInode::stat` → `Inode::stat` | `ch6_file1` 或 `ch6_file2` | inode 编号、最新链接数与文件类型。 |

两次 `ch6_file0` 在同一次 QEMU 运行中完成，保留第一次创建的 `fname`。`ch6_file2` 正常结束会删除其全部链接；`ch6_file3` 的大文件写入用于观察间接块分配及回收。

解释磁盘 inode 与打开偏移的分工、`nlink` 与 `Arc` 数量的区别、`clear` 和最后一次 `unlink` 的资源归属，以及文件系统锁与块缓存锁为何不能重入。API 配套 `exec` 已补充重置栈和堆边界，与参考 `ch6` 此处不同；该提供实现不属于文件实验的 TODO。

## 4. 报告要求

报告保存到 `reports/lab6.md`，保留对应 GDB 日志。结合调用关系解释文件身份、偏移、目录项和回收策略，可附必要代码片段或源码链接。

若选择本章独立实现，比较自己的文件操作实现与参考实现，说明偏移推进、缓存同步、空槽复用或边界处理不同的原因，并保持 API 的立即回收约定。

总结主要问题、排查依据和解决思路，区分实际观察与源码推导。GDB 跟踪用于分析，功能验收继续使用 API 文档原有命令。
