# rCore ch4：源代码分析与 GDB 动态跟踪

阅读 `ch4` 的参考实现，分析 Sv39 页表、逻辑段与地址空间如何支撑内核运行和用户应用访问。

## 1. 使用说明

在 `ch4` 分支的 `os` 目录构建调试内核：

```bash
make build MODE=debug BASE=2
```

本节使用 `MODE=debug`；日常运行和实验验收继续使用原有命令。

先按下表阅读源码：

| 阅读位置 | 关注内容 |
| --- | --- |
| [mm/address.rs](os/src/mm/address.rs)、[mm/page_table.rs](os/src/mm/page_table.rs) | 页号、页内偏移、三级索引、PTE 编码及用户地址转换。 |
| [mm/memory_set.rs](os/src/mm/memory_set.rs) | 内核映射、ELF 装载、用户栈、保护页、Trap 上下文与跳板页。 |
| [mm/frame_allocator.rs](os/src/mm/frame_allocator.rs)、[mm/mod.rs](os/src/mm/mod.rs) | 页帧清零与回收、页表激活和 `satp`。 |
| [task/task.rs](os/src/task/task.rs)、[syscall/process.rs](os/src/syscall/process.rs)、[syscall/fs.rs](os/src/syscall/fs.rs) | 地址空间的调用方，`mmap`、`munmap`、`sbrk` 和用户缓冲区访问。 |

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
set logging file ../reports/ch4-gdb.log
set logging enabled on
target remote localhost:1234
```

### 观察内核页表映射

```gdb
info functions new_kernel
tbreak 'os::mm::memory_set::MemorySet::new_kernel'
continue
bt 6
info functions ::map
tbreak 'os::mm::page_table::PageTable::map'
continue
next
info args
bt 6
```

命中 `map` 后先执行一次 `next`，让参数保存完成后再读取。通过调用栈判断这次映射的用途，再结合函数参数理解虚拟页号 `vpn`、物理页号 `ppn` 和权限 `flags`。结合 `find_pte_create` 阅读三级页表的建立过程，并解释中间页表页与数据页分别由谁持有。首次命中不代表全部内核段已经完成映射。

### 观察用户缓冲区转换

```gdb
tbreak os::mm::page_table::translated_byte_buffer
continue
info args
bt 6
info registers satp
```

记录用户页表的 `token`、缓冲区首地址 `ptr` 和长度 `len`，并观察一次循环中的页号、物理页号和页内偏移。系统调用处理期间运行的是内核页表，函数使用传入的用户 `token` 查询映射。

使用 `info functions` 查找完整函数名称。根据页表转换结果读取物理页内容，并通过源码阅读说明循环如何处理跨页范围、页内偏移与物理不连续的页面。

## 3. 需要追踪的调用链

| 调用链 | 触发与观察任务 |
| --- | --- |
| `mm::init` → `KERNEL_SPACE` 初始化（`new_kernel` → `MapArea::map` → `PageTable::map`），随后调用 `MemorySet::activate` | 启动时记录一次映射及页表层级；在 `activate` 前后记录 `satp`，跟踪 `remap_test` 对内核段权限的检查。 |
| `TaskControlBlock::new` → `MemorySet::from_elf` → ELF 段、栈和 Trap 上下文映射 | 在应用创建时记录用户栈边界，查看保护页、Trap 上下文和跳板页的映射权限；比较两个应用的页表及数据页，说明其独立性。 |
| `sys_write` → `translated_byte_buffer` → `PageTable::from_token` → `translate` | hello/power 输出时动态记录一次缓冲区转换；通过源码阅读解释跨页切片的构造与字节顺序。 |
| `sys_mmap` → 任务的 `mmap` → `MemorySet::insert_framed_area` → `MapArea::map` → `PageTable::map` | `ch4_mmap0` 建立可读写映射，`ch4_mmap1` 写只读页触发异常；先在 `sys_mmap` 停下，再记录对应的映射参数和权限。 |
| `sys_munmap` → 任务的 `munmap` → `MemorySet::remove_framed_area` → `MapArea::unmap` | `ch4_unmap` 解除映射并重新映射，`ch4_unmap2` 检查错误参数；记录页表项变化和 `frame_dealloc` 的回收调用，结合源码说明逻辑段记录与页帧所有权。 |
| `sys_sbrk` → `change_program_brk` → `MemorySet::append_to` / `shrink_to` → `MapArea` | `ch4b_sbrk` 先扩容再缩容；分别记录正负 `size`、堆边界和映射变化，观察最后写入已释放页的异常。 |
| `sys_trace` → `PageTable::translate_user` | `ch4_trace1` 读写已映射字节、访问无效地址、写只读页及访问撤销后的映射；记录地址、所需权限和返回结果。 |

结合源码说明 `translate_user` 的规范地址和 `V/U/R/W` 检查，以及 `translate` 返回末级 PTE 与允许用户访问之间的区别。

## 4. 报告要求

报告保存到 `reports/lab4.md`，保留对应 GDB 日志。结合映射参数、页表权限和地址转换过程解释上述功能，可附必要的代码片段或源码链接。

若选择本章独立实现，比较自己的页表与地址空间实现和参考实现，说明辅助函数组织、边界处理或实现方式不同的原因，并遵守 API 规定的接口契约。

总结主要问题、排查依据和解决思路，区分实际观察与源码推导。GDB 跟踪用于分析，功能验收继续使用 API 文档原有命令。
