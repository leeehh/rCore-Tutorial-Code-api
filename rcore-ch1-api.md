# rCore ch1 API 实验：裸机 Rust 程序的启动与运行

## 实验内容

本实验基于 `ch1-api` 分支，要求同学借助 AI 完成裸机 Rust 程序的入口 `rust_main()`。程序需要建立必要的运行时初始状态，执行简单计算，输出内存布局，最后正常退出 QEMU。

学生只修改 [os/src/main.rs](os/src/main.rs) 中 `rust_main()` 的函数体，共完成一处 TODO。保留入口属性、函数签名、已有链接符号声明和其他配套实现，不增加公共接口。

| 实现文件 | 待实现接口 | 数量 |
| --- | --- | --- |
| [os/src/main.rs](os/src/main.rs) | `rust_main` | 1 |

本实验的重点是理解没有宿主操作系统和标准库时，一个 Rust 程序如何开始运行、完成计算并结束。入口汇编、链接脚本、控制台、日志、panic handler 和退出设备均已提供，不要求重新实现。

## 已提供的启动环境

### 从 QEMU 到 rust_main

当前启动流程为：

```text
QEMU 将内核二进制加载到 0x80200000
    → RustSBI 初始化并交接到内核入口
    → entry.asm 中的 _start 设置 sp
    → call rust_main
    → 学生实现的初始化、计算、输出与退出
```

[os/Makefile](os/Makefile) 指定内核加载地址为 `0x80200000`，与 [os/src/linker.ld](os/src/linker.ld) 中的 `BASE_ADDRESS` 一致。链接脚本将 `.text.entry` 放在代码段起始位置，并指定 `_start` 为入口。

[os/src/entry.asm](os/src/entry.asm) 为启动栈保留 `4096 * 16` 字节，即 64 KiB，将栈指针 `sp` 设置为 `boot_stack_top`，再调用 `rust_main()`。进入 Rust 代码时栈已经可用，不需要重新设置栈指针。局部变量和函数调用可以使用这个栈，不能在入口中将其清零。

### no_std、no_main 与 core

`#![no_std]` 表示程序不依赖 Rust 标准库 `std`。本章没有提供宿主操作系统的文件、线程或标准输出服务，但仍可使用 `core` 中不依赖这些服务的基础功能，包括整数、数组、循环、迭代器和格式化。

`#![no_main]` 表示不采用通常由 Rust 运行时安排的 `main` 入口。这里由 `_start` 调用 `rust_main()`；`#[no_mangle]` 保留汇编所需的符号名称，必须保留。

本实验没有初始化堆，不使用 `alloc`、`Vec`、`String` 或其他依赖堆分配的容器。计算只需使用固定长度的局部数组。控制台输出使用已有宏，也不需要用户态系统调用。

### 链接符号与地址范围

`rust_main()` 内已声明以下链接符号，学生保留声明，通过符号取地址：

| 输出名称 | 起始符号 | 结束符号 | 含义 |
| --- | --- | --- | --- |
| `.text` | `stext` | `etext` | 程序代码范围 |
| `.rodata` | `srodata` | `erodata` | 只读数据范围 |
| `.data` | `sdata` | `edata` | 已初始化的可写数据范围 |
| `boot_stack` | `boot_stack_lower_bound` | `boot_stack_top` | 启动栈范围 |
| `.bss` | `sbss` | `ebss` | 需要清零的数据范围，不含启动栈 |

这些名称由链接脚本或入口汇编定义。虽然代码使用 `extern "C"` 块中的 `fn` 声明引用它们，它们在这里表示地址边界，不是可以调用的函数。可以使用 `stext as usize` 这样的表达式取得数值地址，不要调用符号或解引用地址来读取内容。

所有范围均为左闭右开的 `[start, end)`。链接脚本对部分边界进行页对齐，范围中可能包含对齐填充；例如 `ebss` 位于 BSS 内容结束后的页对齐位置。空段也合法，不能要求每段的起始地址严格小于结束地址。

`.bss.stack` 在链接时位于 `sbss` 之前，所以 `[sbss, ebss)` 不包含启动栈。输出表中 `.bss` 一项按这对符号表示清零范围，而不是包含启动栈的整个输出节。

各段的具体地址随代码和构建结果变化，应从本次构建的链接符号取得，不能照抄某次运行结果或硬编码地址。

## rust_main

description: 裸机 Rust 程序入口。在启动栈已建立后，完成数据初始化、简单计算、内存布局输出和正常退出。

```rust
#[no_mangle]
pub fn rust_main() -> !;
```

**输入**

无参数。入口汇编已经设置栈指针，程序代码和数据已按链接脚本放入内存，控制台所需的 SBI 环境已提供。清零 BSS 和初始化日志仍由本接口完成。

**输出**

不返回。程序按下列顺序完成运行后，通过提供的 QEMU 退出接口以成功状态结束，QEMU 进程退出码为 `0`。

### 1. 初始化运行时状态

先调用一次 `clear_bss()`，再调用一次 `logging::init()`，之后执行计算和输出。

`clear_bss()` 已完整实现，使用逐字节的 volatile 写将 `[sbss, ebss)` 清零。裸机环境中不能依靠宿主程序的启动逻辑替内核完成这项初始化。不要修改它的范围，也不要从启动栈下界开始清零，否则会破坏正在运行的栈。

日志初始化依赖全局状态，因此必须在 BSS 清零之后进行。初始化后也不能再次清零 BSS，以免覆盖已经建立的状态。

### 2. 输出启动信息

使用已有的 `println!` 输出一行：

```text
[kernel] Hello, world!
```

这里的 `println!` 来自 [os/src/console.rs](os/src/console.rs)，通过 `core::fmt` 格式化，再通过 SBI 逐字符输出，不是 `std::println!`，也不依赖宿主标准输出接口。

### 3. 执行局部数组计算

在函数内定义元素类型为 `usize` 的局部数组 `[1, 2, 3, 4, 5]`，实际遍历元素并求和。可以使用循环或迭代器，不要求固定的求和写法。

将计算结果交给 `println!`，输出：

```text
[kernel] sum = 15
```

不能直接输出包含结果常量的字符串来代替计算，也不需要引入堆分配或新的公共函数。此步骤用于理解：栈和入口准备好之后，普通的 Rust 计算可以在裸机环境中运行。

### 4. 按原有日志方式记录内存布局

保留原有的日志宏、级别、消息格式和调用顺序，不改为 `println!`。各条日志使用对应链接符号的实际地址，整数按 `{:#x}` 格式输出：

| 顺序 | 日志宏 | 消息格式 | 地址参数顺序 |
| --- | --- | --- | --- |
| 1 | `trace!` | `[kernel] .text [{:#x}, {:#x})` | `stext`、`etext` |
| 2 | `debug!` | `[kernel] .rodata [{:#x}, {:#x})` | `srodata`、`erodata` |
| 3 | `info!` | `[kernel] .data [{:#x}, {:#x})` | `sdata`、`edata` |
| 4 | `warn!` | `[kernel] boot_stack top=bottom={:#x}, lower_bound={:#x}` | `boot_stack_top`、`boot_stack_lower_bound` |
| 5 | `error!` | `[kernel] .bss [{:#x}, {:#x})` | `sbss`、`ebss` |

代码段、数据段和 BSS 使用半开区间格式，先输出低地址再输出高地址。启动栈沿用原有字段格式：`top=bottom` 对应 `boot_stack_top`，`lower_bound` 对应 `boot_stack_lower_bound`。这两个边界所定义的有效栈范围仍是 `[boot_stack_lower_bound, boot_stack_top)`，不要因日志中的字段名称而互换地址。

保留 `use log::*;` 和已有日志实现。本章日志模块在缺省 `LOG` 时使用 `Off` 级别，上述日志的可见性取决于构建时的日志配置。不要为了显示所有地址而修改 Makefile、强制日志级别或绕过日志宏。启动信息和计算结果仍使用 `println!`，它们不受日志级别影响。

### 5. 正常结束程序

完成全部计算和输出后，调用已提供的：

```rust
crate::board::QEMU_EXIT_HANDLE.exit_success()
```

这是 `QEMUExit` trait 提供的方法，调用处需要让该 trait 处于作用域内，例如在函数体中使用 `use crate::board::QEMUExit;`。

`exit_success()` 的返回类型是 `!`，表示不会正常返回，满足 `rust_main() -> !` 的签名。这里没有宿主进程替入口接收返回值，也没有要求入口返回到汇编后的其他代码。

不要使用 `panic!`、`todo!` 或无限循环代替成功退出。当前 [os/src/sbi.rs](os/src/sbi.rs) 中的 `shutdown()` 实际调用 `exit_failure()`，用于 panic 路径；它不能用于本实验的正常结束。

**共同约束**

- 保留 `#[no_mangle]`、`pub fn rust_main() -> !` 和已有链接符号声明。
- 实现范围限于 `rust_main()` 的函数体，不修改 `clear_bss()` 或其他模块，不新增公共 API。
- 不使用 `std`、堆分配或用户态系统调用。
- 按上述顺序初始化、计算和输出，最后成功退出。
- 不通过硬编码地址、伪造结果或修改底层退出行为满足输出要求。

## 已提供的配套功能

以下内容完整提供，无需学生补全或改动：

| 代码位置 | 已提供内容 |
| --- | --- |
| [os/src/main.rs](os/src/main.rs) | crate 属性、模块声明、汇编引入、`clear_bss()`、入口签名和链接符号声明 |
| [os/src/entry.asm](os/src/entry.asm) | `_start`、64 KiB 启动栈、栈指针设置和入口调用 |
| [os/src/linker.ld](os/src/linker.ld) | 内核基址、段布局和边界符号 |
| [os/src/console.rs](os/src/console.rs) | `print!`、`println!` 和格式化输出 |
| [os/src/sbi.rs](os/src/sbi.rs) | SBI 字符输出和失败退出包装 |
| [os/src/logging.rs](os/src/logging.rs) | 日志初始化与日志输出 |
| [os/src/lang_items.rs](os/src/lang_items.rs) | panic handler，输出 panic 信息后失败退出 |
| [os/src/boards/qemu.rs](os/src/boards/qemu.rs) | QEMU 退出设备、`QEMUExit` 和 `QEMU_EXIT_HANDLE` |
| [os/Makefile](os/Makefile) | 编译内核、生成二进制和启动 QEMU |

可阅读这些文件理解调用关系，但实验不涉及任务管理、地址空间、文件系统或用户程序加载。

## 运行与验收

在已有运行环境准备好后，从仓库根目录执行：

```bash
cd os
make run BASE=2
```

本章 Makefile 不使用 `BASE` 参数，保留该参数是为了沿用统一的运行命令。命令会编译内核并启动 QEMU；本章没有用户终端、用户应用或 `usertest` 阶段，不需要再输入测例名称。

验收只使用上述命令，不新增测试脚本或额外验证工作。RustSBI 的启动输出之后，应能观察到：

1. `[kernel] Hello, world!`。
2. 由局部数组求和得到的 `[kernel] sum = 15`。
3. 程序完成后 QEMU 正常退出，命令成功结束，没有 panic 或挂起。

内存布局日志遵守现有日志配置；缺省 `LOG` 时不显示这些日志是正常现象。本次运行命令不自动验证段日志内容，也不要求增加其他运行命令。实现仍需满足前文关于日志宏、消息格式、顺序和地址来源的约束；日志可见时应与相应链接符号一致。

验收不固定各段的具体地址，也不要求每段非空。启动栈范围由已提供的汇编保留空间决定，长度为 64 KiB。

未完成 TODO 时，实验骨架可以编译，但运行到 `rust_main()` 后会触发 TODO panic，并通过已提供的 panic handler 失败退出。这是未实现骨架的预期表现，不代表实验已经通过；补全入口后应按上述要求正常结束。
