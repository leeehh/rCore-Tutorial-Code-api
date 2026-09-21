# rCore ch1：源代码分析与 GDB 动态跟踪

阅读本分支 [os/src/main.rs](os/src/main.rs) 的参考实现，并使用 GDB 动态跟踪 `rust_main()` 的执行过程，分析其初始化、输出和退出等功能。

## 1. 使用说明

在 `ch1` 分支的 `os` 目录下执行构建命令：

```bash
make build MODE=debug LOG=TRACE
```

`MODE=debug` 生成带调试信息、未优化的内核；`LOG=TRACE` 开启全部五种级别的日志。

### 源代码阅读

按下表阅读，先画出启动流程，再定位每一步使用的符号和调用接口：

| 顺序 | 阅读位置 | 关注的问题 |
| --- | --- | --- |
| 1 | [os/Makefile](os/Makefile)、[os/.cargo/config.toml](os/.cargo/config.toml) | 编译目标是什么？链接脚本在哪里指定？QEMU 如何加载固件和内核？ |
| 2 | [os/src/linker.ld](os/src/linker.ld)、[os/src/entry.asm](os/src/entry.asm) | `_start` 如何放到 `0x80200000`？进入 Rust 前如何建立栈？为什么 `.bss.stack` 在 `sbss` 之前？ |
| 3 | [os/src/main.rs](os/src/main.rs) | `no_std`、`no_main`、`global_asm!`、`#[no_mangle]` 各有什么作用？为什么先 `clear_bss()`，再 `logging::init()`？ |
| 4 | [os/src/console.rs](os/src/console.rs)、[os/src/sbi.rs](os/src/sbi.rs) | `println!` 如何经格式化和逐字符输出，最终执行 `ecall`？ |
| 5 | [os/src/lang_items.rs](os/src/lang_items.rs)、[os/src/sbi.rs](os/src/sbi.rs) | 只读对照 panic 与正常退出的区别，不需要人为触发 panic。 |

链接符号在 Rust 中虽然声明为 `extern "C" { fn ...(); }`，这里使用的是符号地址，并不调用这些“函数”。`[sbss, ebss)` 是清零区间，排除了正在使用的启动栈；段边界还可能包含链接脚本引入的对齐空间。

### 工具与构建参数

使用仓库指定的 Rust 工具链和自带的 `bootloader/rustsbi-qemu.bin`，通过 QEMU 运行内核，调试器使用 `riscv64-unknown-elf-gdb`。

按 [实验环境配置中的“GDB 调试支持”](https://learningos.cn/rCore-Tutorial-Guide/0setup-devel-env.html#gdb) 安装适合宿主平台的 RISC-V GNU 工具链，并将其 `bin` 目录加入 `PATH`。其中包含 `riscv64-unknown-elf-gdb`，名称与本仓库 Makefile 中的调试器配置一致。

debug 构建也可能遇到内联函数、不可读取的泛型参数，或 `bt` 在汇编入口停止回溯。可用 `info functions` 查找当前符号，结合 `list`、`disassemble /r '完整函数名'`、`info registers` 与 `si` 查看实际执行。不要为展示完整调用链而修改源码、关闭原有日志过滤或编造 GDB 未显示的变量值。

## 2. 操作样例

### 2.1 构建并连接 GDB

终端一从仓库根目录进入 `os`，构建并启动等待 GDB 的 QEMU：

```bash
cd os
make gdbserver MODE=debug LOG=TRACE
```

`gdbserver` 会先构建，再以 `-s -S` 启动 QEMU：监听 `1234` 端口，并在启动时暂停 CPU。此时没有启动输出是正常的。仅需编译时，可以单独使用 `make build MODE=debug LOG=TRACE`。

本次构建输出为 `Finished dev profile [unoptimized + debuginfo]`，ELF 位于 `os/target/riscv64gc-unknown-none-elf/debug/os`。Cargo 将这套配置称为 `dev`，产物目录名为 `debug`。GDB 应读取这个 ELF；QEMU 加载的是同目录的 `os.bin`，不能把没有调试信息的 `.bin` 交给 GDB，也不能混用其他构建配置的 ELF。

`LOG` 由 `option_env!("LOG")` 在编译时读取，等级名称使用大写 `TRACE`。必须把它传给上述构建命令，单独在已启动的 GDB 中设置环境变量不会改变内核日志。未指定 `LOG` 时，本章原有默认行为是关闭日志，直接调用 `println!` 的启动信息仍会输出。

终端二同样从仓库根目录执行：

```bash
mkdir -p reports
cd os
riscv64-unknown-elf-gdb -nx -q target/riscv64gc-unknown-none-elf/debug/os \
  -ex "set substitute-path /rustc/$(rustc -vV | sed -n 's/^commit-hash: //p') $(rustc --print sysroot)/lib/rustlib/src/rust"
```

`-nx` 避免个人 GDB 初始化文件干扰连接；`set substitute-path` 根据当前 Rust 工具链自动映射本机源码路径，供进入 `core` 时查看源码。随后在 GDB 中执行：

```gdb
set pagination off
set logging file ../reports/ch1-gdb.log
set logging enabled on
set architecture riscv:rv64
set language c
target remote localhost:1234
```

日志默认追加到 `reports/ch1-gdb.log`，多次实验也可使用不同文件名。这里选择 C 表达式语法，是为了统一使用地址强制转换和 `self->addr` 等表达式，不会改变被调试的 Rust 程序。进入 Rust 栈帧时出现语言不匹配提示，不代表连接失败。

连接后本次首先停在 `0x1000`，此时还没有执行到内核。不要使用 `run` 启动宿主进程，也不需要 `load`；内核镜像已由终端一的 QEMU 加载。下面按执行顺序使用断点和 `continue`。

### 2.2 从入口到 BSS 清零

```gdb
tbreak *_start
continue
x/4i $pc
si
si
info registers pc sp
p/x &boot_stack_lower_bound
p/x &boot_stack_top
p/d (char *)&boot_stack_top - (char *)&boot_stack_lower_bound

tbreak rust_main
continue
bt
info registers pc sp
list src/main.rs:54

tbreak os::clear_bss
continue
bt
set $bss_begin = (unsigned long)&sbss
set $bss_end = (unsigned long)&ebss
p/x $bss_begin
p/x $bss_end
p/d $bss_end - $bss_begin
```

`tbreak` 是命中后自动删除的临时断点。入口的 `la sp, boot_stack_top` 是伪指令，本次展开为两条指令，因此在 `x/4i` 确认后执行两次 `si`，停在调用 Rust 之前。若自己的反汇编不同，以实际指令为准。`_start` 和 `stext` 地址相同，GDB 显示 `stext` 也表示到达了这个入口。

记录进入 Rust 前后的 `sp`：设置栈后应等于栈顶，命中 `rust_main` 的源码断点时，函数序言已经分配栈帧，`sp` 会降低，但仍应位于启动栈内。本次测得栈大小为 `65536` 字节。

确认 BSS 区间至少有 16 字节后，再观察其开头；若区间更短，应相应减少本节中所有内存读取的字节数。空区间则跳过下面的内存读取、闭包断点和写指令跟踪，直接在 `os::logging::init` 设置临时断点并继续：

```gdb
x/16bx $bss_begin
info functions os::clear_bss
tbreak 'os::clear_bss::{closure#0}'
continue
p/x a
bt
x/10i $pc
```

这里可以看到 `Range`、`for_each` 与清零闭包的调用关系，闭包参数 `a` 是本次准备写入的地址。本次第一次进入时 `a == sbss`。在反汇编中找到真正写内存的 `sb`，单步观察一次写入，而不是逐个跟完整个区间。例如本次构建中的指令是：

```text
0x802018bc: sb a0,0(a1)
```

以下地址来自本次构建，使用前必须按自己的反汇编替换；文中的指令地址、寄存器分配和 `{impl#编号}`、`{closure#编号}` 都不是接口保证：

```gdb
tbreak *0x802018bc
continue
info registers pc a0 a1
x/i $pc
si
x/8bx $bss_begin

tbreak os::logging::init
continue
bt
x/16bx $bss_begin
```

本次 `sb` 执行前 `a0=0`、`a1=sbss`，说明正在向区间首字节写零。随后命中 `logging::init`，说明清零调用已返回，再开始日志初始化。结合 `sbss..ebss` 的源码，分析为何末地址不参与写入、为何启动栈不会被清除。

本次内存读取在清零前后都显示零，不能仅凭零值证明发生过清零；报告应结合实际命中的写指令、访问地址和调用顺序说明证据。无需修改内存、增加全局变量或扩大清零范围来制造现象。

### 2.3 跟踪字符输出

从 `logging::init` 的断点继续，先捕获 Hello 的直接输出：

```gdb
tbreak os::console::print
continue
bt
tbreak os::sbi::console_putchar
continue
info args
p/x c
bt
disassemble /r 'os::sbi::console_putchar'
```

本次首字符参数 `c=91`，即 `0x5b`，对应 `[`。调用栈可以观察到 `console::print`、格式化输出、`Stdout::write_str` 和 `console_putchar`。`println!` 是宏，不能用 `break println!` 当作普通函数入口；源码断点也可能定位到宏展开处。

`sbi_call` 使用 `#[inline(always)]`，即使本节使用 debug 构建，也应在 `console_putchar` 的反汇编中定位 `ecall`。本次命中该函数断点后，两次 `si` 到达它：

```gdb
si
si
x/i $pc
info registers a0 a1 a2 a6 a7
tbreak *($pc + 4)
continue
x/i $pc
```

执行 `tbreak *($pc + 4)` 前先确认当前指令确实是 `ecall`；该指令占 4 字节。临时断点放在服务返回后的位置，避免逐指令进入没有内核源码符号的固件。本次在 `ecall` 前观察到：

```text
a0 = 0x5b
a1 = 0
a2 = 0
a6 = 0
a7 = 1
```

结合 `src/sbi.rs`，解释字符参数、SBI 调用号和 `ecall` 的关系。

### 2.4 跟踪成功退出

```gdb
info functions os::board
tbreak 'os::board::{impl#1}::exit_success'
continue
bt
step
info args
p/x code
p/x self->addr
disassemble /r 'os::board::{impl#1}::exit'
```

本次进入 `exit` 时，`code=0x5555`、`self->addr=0x100000`。在反汇编中找到实际写入设备的 `sw`，注意区分函数内写栈的其他 `sw`。本次设备写指令位于 `0x802010a8`：

```gdb
tbreak *0x802010a8
continue
info registers pc a0 a1
x/i $pc
si
set logging enabled off
quit
```

本次在 `sw a0,0(a1)` 前确认 `a0=0x5555`、`a1=0x100000`，执行后 GDB 显示 `Remote connection closed`，终端一的 QEMU 正常退出，命令退出码为 `0`。结合此前的启动信息与五条日志，这才构成完整的正常结束记录；单独连接断开不能证明运行成功。

本章正常退出经板级设备 MMIO 写入完成，不经过 `sbi::shutdown()`。后者当前调用的是 `exit_failure()`，用于 panic 路径；应以代码为准，不能仅凭它的函数名或注释把正常结束写成 SBI shutdown。

## 3. 需要追踪的调用链

将阅读结果和上述断点记录对应起来，至少说明下列调用链及其状态变化。箭头表示源码中的调用或展开关系，不保证每一步都有独立的运行时栈帧。

| 调用链 | 需要记录、解释的证据 |
| --- | --- |
| QEMU / RustSBI → `_start` → 设置 `sp` → `rust_main` | 固件向内核交接的地址、入口指令、启动栈上下界，以及 Rust 函数序言对 `sp` 的影响；不要求深入固件内部实现。 |
| `rust_main` → `clear_bss` → `Range::for_each` → 闭包 → `write_volatile` | 清零起止符号、闭包地址参数、写零指令及初始化先后顺序。 |
| `println!` → `console::print` → 格式化输出 → `Stdout::write_str` → `console_putchar` → 内联 `sbi_call` → `ecall` | 第一个字符、输出调用栈和 `ecall` 前的参数寄存器。 |
| `rust_main` → `exit_success` → `exit` → MMIO `sw` → QEMU 退出 | 成功退出码编码、设备地址、最后一条写指令及终端退出结果。 |

## 4. 报告要求

分析报告采用 Markdown（`.md`）格式，统一保存到仓库根目录下的 `reports/lab1.md`，随实验提交，并保留对应 GDB 日志。

报告应结合源代码说明调用顺序及其原因，并记录关键断点、使用的 GDB 命令、观察到的状态和分析结论。引用源代码时，应注明文件路径、函数名称及行号或行号范围，可附必要的代码片段或源码链接，便于核对。参考实现未包含 API 实验要求的局部数组求和与结果输出，应在报告中明确指出这部分新增任务。

将实际观察、根据源码作出的推导以及未完成的跟踪分开说明；对实验中遇到的问题，记录现象、排查依据和处理结果。
