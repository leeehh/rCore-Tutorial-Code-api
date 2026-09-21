# rCore ch7 API 实验：管道通信与文件描述符共享

## 实验内容

本实验基于 `ch7-api` 分支，要求同学借助 AI 完成第七章的管道通信模块。完成后的模块需要通过环形缓冲区在进程之间传递字节，支持管道读写时的等待与 EOF 判断，并将管道端点接入进程的文件描述符表。

实验分为三层：`PipeRingBuffer` 管理 FIFO 字节队列；`Pipe` 实现文件读写接口并在条件不满足时让出 CPU；`sys_pipe` 和 `sys_dup` 管理进程可以使用的文件描述符。进程创建、地址转换、普通文件、标准输入输出和信号处理等配套代码已经提供。

学生只修改下表中的两个文件，共完成九处 TODO。各接口的名称、可见性、参数和返回类型保持原样；保留已有数据结构、常量和配套函数的语义，可以在这两个文件中自行设计内部辅助函数。

| 实现文件 | 待实现接口 | 数量 |
| --- | --- | --- |
| [os/src/fs/pipe.rs](os/src/fs/pipe.rs) | `PipeRingBuffer::write_byte`、`read_byte`、`available_read`、`available_write` | 4 |
| 同上 | `make_pipe`，以及 `File for Pipe` 中的 `read`、`write` | 3 |
| [os/src/syscall/fs.rs](os/src/syscall/fs.rs) | `sys_pipe`、`sys_dup` | 2 |

本文沿用前几章 API 实验按模块和接口组织 `description`、签名、输入、输出与关键约束的形式。以下契约以本分支代码为准，不要求增加公共接口或实现完整的 POSIX 管道语义。

## 实验要求

1. **代码修改范围**：仅允许修改 `ch7-api` 分支中 [os/src/fs/pipe.rs](os/src/fs/pipe.rs) 的 `PipeRingBuffer::write_byte/read_byte/available_read/available_write`、`make_pipe`、`File for Pipe` 中的 `read/write`，以及 [os/src/syscall/fs.rs](os/src/syscall/fs.rs) 的 `sys_pipe/sys_dup`，共九处 TODO；内部辅助函数可在这两个文件中设计。保留已有数据结构、常量、接口和配套函数语义，不修改其他代码文件、构建配置或测试文件；可以新增实验报告等说明文档。

2. **静态分析与动态跟踪**：阅读 `ch7` 分支的管道和描述符参考实现，使用 GDB 观察管道创建与环形缓冲区操作，分析等待、共享和 EOF 的调用关系。操作参考 [ch7 源代码分析与动态跟踪文档](../../blob/ch7/rcore-ch7-analyze.md)。报告应记录实际断点、命令、关键状态与结论，可附必要源码片段或链接。参考实现的零长度读写可能先进入空满等待分支，本文要求立即返回 `0`，应明确说明这一差异。

3. **独立实现与对比**：依据本文契约独立完成九处接口，并按“运行与验收”小节执行验收。在报告中对比自己的实现与 `ch7` 参考实现的环形状态、分批传输、借用释放和描述符共享，解释写端生命周期如何决定 EOF；按本文零长度契约实现，不把参考实现的边界行为直接视为要求。

4. **主要问题与解决思路**：总结主要问题的现象、原因、排查过程、解决思路及处理结果，结合代码位置、GDB 记录或运行输出说明依据。关注缓冲区回绕、空满区分、调度前释放借用、强弱引用和描述符分配；未解决的问题如实记录。

实验报告采用 Markdown 格式，保存到仓库根目录的 `reports/lab7.md`，至少包含静态分析与动态跟踪、独立实现对比、主要问题与解决思路三部分，并随实验代码提交。参考实现上的 GDB 跟踪用于分析，完成后的功能验收仍使用本文规定的命令和既有测例。

## 提供的数据结构

### Pipe 与管道端点

description: [pipe.rs](os/src/fs/pipe.rs) 中的 `Pipe` 表示一个管道端点，保存访问权限，并通过 `Arc` 共享管道缓冲区。

```rust
pub struct Pipe {
    readable: bool,
    writable: bool,
    buffer: Arc<UPSafeCell<PipeRingBuffer>>,
}
```

`Pipe::read_end_with_buffer` 和 `Pipe::write_end_with_buffer` 已提供。前者创建只读端点，后者创建只写端点；二者仅包装传入的缓冲区，不负责创建另一端，也不负责注册写端的弱引用。

同一个管道的两个端点必须共享同一份 `PipeRingBuffer`。管道不保存磁盘文件偏移，读取会消费缓冲区中的数据，写入会追加数据。不同次 `make_pipe` 创建的管道互不共享内容。

### PipeRingBuffer 与 RingBufferStatus

description: 管道使用容量固定为 32 字节的环形缓冲区，保存读位置、写位置、空满状态以及写端弱引用。

```rust
const RING_BUFFER_SIZE: usize = 32;

#[derive(Copy, Clone, PartialEq)]
enum RingBufferStatus {
    Full,
    Empty,
    Normal,
}

pub struct PipeRingBuffer {
    arr: [u8; RING_BUFFER_SIZE],
    head: usize,
    tail: usize,
    status: RingBufferStatus,
    write_end: Option<Weak<Pipe>>,
}
```

字段和状态的含义固定如下：

| 字段或状态 | 含义 |
| --- | --- |
| `head` | 下一次读取的数组位置 |
| `tail` | 下一次写入的数组位置 |
| `Empty` | 没有可读数据，可写容量为 32 |
| `Full` | 已有 32 字节可读数据，没有可写空间 |
| `Normal` | 可读数据量在 1 至 31 字节之间 |
| `write_end` | 指向该管道写端对象的弱引用，用于判断全部写端引用是否已释放 |

`head` 和 `tail` 始终位于数组范围内，推进到数组末尾后回到起始位置。空缓冲区和满缓冲区都满足 `head == tail`，必须结合 `status` 区分，不能为了避免相等而浪费一个字节的容量。

`PipeRingBuffer::new` 已提供，创建空缓冲区，两个位置均为 `0`，`write_end` 尚未设置。`set_write_end` 使用传入写端的 `Arc` 设置弱引用；`all_write_ends_closed` 检查该弱引用能否升级。调用关闭状态查询之前必须已经完成写端注册。

缓冲区由 `UPSafeCell` 管理，`exclusive_access()` 返回动态独占借用。同一缓冲区不能同时存在重叠的独占借用，尤其不能在持有借用时切换到另一个可能访问管道的进程。

### File 与 UserBuffer

description: [os/src/fs/mod.rs](os/src/fs/mod.rs) 定义统一的 `File` trait，管道与普通文件、标准输入输出通过同一组接口接入系统调用。

```rust
pub trait File: Send + Sync {
    fn readable(&self) -> bool;
    fn writable(&self) -> bool;
    fn read(&self, buf: UserBuffer) -> usize;
    fn write(&self, buf: UserBuffer) -> usize;
    fn stat(&self) -> Option<Stat>;
}
```

上面的签名用于说明接口。`File::stat` 已有返回 `None` 的默认实现，管道沿用该实现，不需要生成磁盘 inode 元数据。`Pipe` 的权限查询方法也已提供，只需补全 `read` 和 `write`。

`UserBuffer` 由 [os/src/mm/page_table.rs](os/src/mm/page_table.rs) 中的地址转换代码生成，保存按用户虚拟地址顺序排列的多个字节切片：

```rust
pub struct UserBuffer {
    pub buffers: Vec<&'static mut [u8]>,
}
```

`len()` 返回切片的总长度，`into_iter()` 逐字节返回可访问的 `*mut u8`。跨页的用户缓冲区可能对应不连续的物理内存，必须按切片或迭代器顺序访问，不能将第一个切片当成全部连续内存。

本实验使用已有转换函数产生的 `UserBuffer`：总长度为零时没有切片，非零请求中的各切片都非空。不要求扩展既有迭代器以处理人为构造的空切片。管道层收到的地址已经转换完成，无需再次访问用户页表。

### 文件描述符表

description: [os/src/task/task.rs](os/src/task/task.rs) 中的 `TaskControlBlockInner` 保存当前进程的文件描述符表，元素类型为 `Option<Arc<dyn File + Send + Sync>>`。索引就是文件描述符，`None` 表示空闲位置。

已提供的 `alloc_fd()` 返回当前最小的空闲索引；没有空槽时，在表尾追加一个 `None` 并返回其索引。这个接口只找到可用位置，调用者必须随后写入文件对象。如果连续调用两次而没有占用第一次返回的位置，会再次取得同一个描述符。

一个文件对象可以同时被多个描述符持有。`dup` 和 `fork` 都通过克隆 `Arc` 共享对象；`close` 只移除相应表项。共享普通文件时使用同一份打开偏移，共享管道时使用同一个端点及其缓冲区。

## os::fs::pipe

description: 本模块实现环形缓冲区、管道创建及端点读写。缓冲区操作不负责调度；端点读写根据空间、数据及写端生命周期决定继续传输、让出 CPU 或返回。

### PipeRingBuffer::write_byte

description: 在环形缓冲区尾部写入一个字节。

```rust
pub fn write_byte(&mut self, byte: u8);
```

**输入**

`byte` 是待写入字节。调用方保证缓冲区尚未满，即 `available_write() > 0`。

**输出**

返回 `()`，可读字节数增加一，可写字节数减少一。

**关键约束**

- 在当前写位置保存字节，推进写位置并正确处理数组末尾回绕。
- 保持已有未读数据及其 FIFO 顺序，不移动读位置。
- 写入后根据实际数据量维护 `Full` 或 `Normal` 状态。
- 不等待、不调度，也不修改写端弱引用。

### PipeRingBuffer::read_byte

description: 读取并消费环形缓冲区中最早写入的一个未读字节。

```rust
pub fn read_byte(&mut self) -> u8;
```

**输入**

调用方保证缓冲区非空，即 `available_read() > 0`。

**输出**

返回队首字节，可读字节数减少一，可写字节数增加一。

**关键约束**

- 从当前读位置取得字节，推进读位置并正确处理数组末尾回绕。
- 保持剩余数据的 FIFO 顺序，不移动写位置。
- 读取后根据实际数据量维护 `Empty` 或 `Normal` 状态。
- 不等待、不调度，也不修改写端弱引用。

### PipeRingBuffer::available_read

description: 查询当前可立即读取的字节数。

```rust
pub fn available_read(&self) -> usize;
```

**输入**

`self` 是满足位置和状态约束的环形缓冲区。

**输出**

返回 `0..=RING_BUFFER_SIZE` 范围内的可读字节数：空时为 `0`，满时为 `32`，其他情况为环形队列中的实际数据量。

**关键约束**

- 正确处理未回绕、已回绕以及 `head == tail` 的情况。
- 查询不消费数据，不改变位置或状态。
- 写端是否关闭不改变当前可读数据量。

### PipeRingBuffer::available_write

description: 查询当前可立即写入的字节数。

```rust
pub fn available_write(&self) -> usize;
```

**输入**

`self` 是满足位置和状态约束的环形缓冲区。

**输出**

返回 `0..=RING_BUFFER_SIZE` 范围内的剩余容量：满时为 `0`，空时为 `32`。

**关键约束**

- 任意有效状态下，`available_read() + available_write() == RING_BUFFER_SIZE`。
- 查询不写入数据，不改变位置或状态。
- 不能通过预留一个永远不可使用的字节来区分空与满。

### make_pipe

description: 创建一对共享空缓冲区的管道端点，并建立关闭状态查询需要的弱引用。

```rust
pub fn make_pipe() -> (Arc<Pipe>, Arc<Pipe>);
```

**输入**

无参数，调用时内存资源足够。

**输出**

返回 `(read_end, write_end)`，第一个端点只读，第二个端点只写。

**关键约束**

- 两个端点共享同一个新建的空缓冲区，不能分别创建互不相通的缓冲区。
- 使用已提供的端点构造器，保持其权限设置。
- 返回前通过 `set_write_end` 注册所返回写端的弱引用，使 `all_write_ends_closed` 可以安全调用。
- 缓冲区只保存写端的 `Weak`，不能保存额外强引用形成引用环或阻止 EOF。
- 此接口不分配文件描述符，不访问用户地址，也不创建进程。

### File for Pipe::read

description: 从管道读端消费字节并写入用户缓冲区，在暂时无数据时让出 CPU。

```rust
fn read(&self, buf: UserBuffer) -> usize;
```

**输入**

`self` 是读端；`buf` 是已转换的有效用户缓冲区。已有 `sys_read` 完成描述符和读取权限检查，并在调用前释放当前进程内部状态的借用。

**输出**

持续读取，直到填满请求或到达 EOF。填满时返回 `buf.len()`；全部写端引用已释放且缓冲区耗尽时，返回本次实际读取量，可以小于请求长度或为 `0`。

**关键约束**

- 请求总长度为零时立即返回 `0`，不能因为管道暂时为空而等待。
- 按 FIFO 顺序向用户缓冲区写入数据，每个字节只消费一次，保持跨切片的连续顺序。
- 当前数据不足以填满请求且写端仍存在时，继续等待后续数据，不能仅因暂时读空就返回短读。已有大管道测例要求一次读取完整的 3000 字节。
- 全部写端关闭后，先读完缓冲区中的残留数据；只有缓冲区也为空时才到达 EOF。
- 等待时先释放缓冲区的独占借用，再调用 `suspend_current_and_run_next()`；恢复后重新取得借用并检查当前状态。
- 返回长度之外的用户缓冲区内容保持不变。不修改管道端点权限或写端弱引用。

### File for Pipe::write

description: 按用户缓冲区的字节顺序向管道写端写入数据，在缓冲区满时让出 CPU。

```rust
fn write(&self, buf: UserBuffer) -> usize;
```

**输入**

`self` 是写端；`buf` 是已转换的有效用户缓冲区。已有 `sys_write` 完成描述符和写入权限检查，并在调用前释放当前进程内部状态的借用。需要等待时，读端会继续消费数据，使写入能够推进。

**输出**

完整写入本次请求，返回 `buf.len()`。

**关键约束**

- 请求总长度为零时立即返回 `0`，不能因为管道已满而等待。
- 按切片或迭代器顺序读取用户数据，并按相同顺序追加到缓冲区，不覆盖未读字节。
- 请求长度可以超过 32 字节，需要分批写入，不能把缓冲区容量当作单次调用的返回长度上限。
- 缓冲区满时，先释放独占借用，再调用 `suspend_current_and_run_next()`；恢复后重新检查剩余空间。
- 不修改用户缓冲区内容，不因为暂时无空间而返回短写。
- 本实验不增加读端关闭检测、`EPIPE` 或 `SIGPIPE` 行为，也不要求多写者之间整次写调用的原子性。

## os::syscall::fs

description: 本模块通过进程文件描述符表向用户态提供管道创建和描述符复制。已有文件系统系统调用保持完整，仅下列两个接口为 TODO。

### sys_pipe

description: 为当前进程创建一对管道端点，将读端和写端的文件描述符写回用户空间。

```rust
pub fn sys_pipe(pipe: *mut usize) -> isize;
```

系统调用编号为 `59`，用户态包装和内核分发已经提供。

**输入**

`pipe` 是当前进程用户地址空间中的有效指针，指向两个连续、正确对齐、可写的 `usize`。调用时内存与文件描述符表资源足够。

**输出**

成功返回 `0`，并将读端描述符写入 `pipe[0]`、写端描述符写入 `pipe[1]`。

**关键约束**

- 调用 `make_pipe` 创建端点，把两个对象存入当前进程的文件描述符表。
- 每次使用 `alloc_fd()` 获取最小可用描述符；先存入读端，再分配并存入写端，保证两个描述符不同。
- 已被占用的表项保持不变。标准输入输出均存在且没有其他打开文件时，新管道通常取得描述符 `3` 和 `4`；不能硬编码这些值。
- 使用 `current_user_token()` 和已有 `translated_refmut` 分别转换两个结果位置，不能直接解引用用户虚拟地址，也不能假设两个值对应的物理地址连续。
- 当前进程内部状态同样使用独占借用，不能在持有该借用时再次通过其他接口借用同一内部状态。
- 本实验以合法结果缓冲区和资源足够为前提，不新增无效指针检查、部分分配回滚或文件描述符数量限制。

### sys_dup

description: 复制当前进程的一个有效文件描述符，使新描述符与原描述符共享同一个文件对象。

```rust
pub fn sys_dup(fd: usize) -> isize;
```

系统调用编号为 `24`，用户态包装和内核分发已经提供。

**输入**

`fd` 是要复制的文件描述符。合法调用有足够的文件描述符表和内存资源。

**输出**

成功返回最小可用的新描述符。`fd` 超出表长或对应表项为 `None` 时返回 `-1`。

**关键约束**

- 无效输入不改变文件描述符表，不创建文件对象。
- 成功时克隆原表项中的 `Arc`，把引用存入新表项；原表项及其权限保持不变。
- 不重新打开普通文件，不重置文件偏移，不复制管道缓冲区，也不创建新的管道端点对象。
- 关闭其中一个描述符不影响其他描述符的有效性。管道写端的全部共享引用释放后，读端才能观察到写端全部关闭。
- 文件描述符是当前进程内的索引，不能用文件对象地址或 `Arc` 引用数量作为返回值。

## 已提供的配套功能与共同约束

以下实现完整提供，学生保持其数据结构、语义和调用关系，实现范围仍限于开头列出的两个文件。

| 代码位置 | 已提供内容 |
| --- | --- |
| [os/src/fs/pipe.rs](os/src/fs/pipe.rs) | 端点和缓冲区结构、状态枚举、容量常量、两个端点构造器、`PipeRingBuffer::new`、`set_write_end`、`all_write_ends_closed`、端点权限查询 |
| [os/src/fs/mod.rs](os/src/fs/mod.rs) | `File` trait、`stat` 的默认实现、模块导出 |
| [os/src/fs/inode.rs](os/src/fs/inode.rs)、[stdio.rs](os/src/fs/stdio.rs) | 普通文件和标准输入输出 |
| [os/src/syscall/fs.rs](os/src/syscall/fs.rs) | `sys_read`、`sys_write`、`sys_open`、`sys_close` 以及往章文件系统调用 |
| [os/src/syscall/mod.rs](os/src/syscall/mod.rs) | 系统调用编号和分发 |
| [os/src/mm/page_table.rs](os/src/mm/page_table.rs) | 用户地址转换、`UserBuffer` 和字节迭代器 |
| [os/src/task/task.rs](os/src/task/task.rs) | 文件描述符表、`alloc_fd`、`fork` 和 `exec` |
| [os/src/task/mod.rs](os/src/task/mod.rs) | 让出 CPU、进程退出与文件描述符引用释放 |
| [os/src/sync](os/src/sync) | `UPSafeCell` 与独占借用 |
| [easy-fs](easy-fs) | 上一章已完成的磁盘文件系统 |

`fork` 克隆文件描述符表中的 `Arc`，父子进程共享已有文件对象。`exec` 保留文件描述符表，因此可将管道端点传递给执行的新程序。`exit` 清空文件描述符表并释放引用，`close` 释放单个表项的引用。学生无需重新实现这些生命周期处理。

EOF 取决于所有写端强引用是否已经释放，而非某一个进程是否调用过 `close`。父进程、子进程和 `dup` 产生的描述符均可能继续持有同一个写端。已有读写 syscall 也会临时克隆文件引用，并在调用完成时释放；管道内部不能额外保存使写端始终存活的强引用。

本实验通过已有调度接口反复让出 CPU 并重新检查条件，不要求新增等待队列、唤醒机制或信号中断管道等待的返回语义。已有信号处理、进程调度、地址空间及往章 syscall 都作为配套功能保留，不在本次重构范围内，也不要求维护 `sys_trace`。

## 运行与验收

在已有运行环境和 `user` 测试目录准备好后，从仓库根目录执行：

```bash
cd os
make run BASE=2
```

本分支的 Makefile 支持从 `ch7-api` 分支名提取章节号。`BASE=2` 准备已有第二至第七章的基础与编程测试应用，随后生成文件系统镜像并启动内核。进入用户终端后输入：

```text
ch7b_usertest
```

功能验收只使用这一已有测试入口，不新增测例或额外验收命令。当前 `ch7_usertest` 的测试列表为空，不能用它的通过提示作为验收依据。

`ch7b_usertest` 包含往章基础测例、信号测例，以及 `ch7b_pipetest` 和 `ch7b_pipe_large_test`。前者检查管道端点描述符、父子进程传输和关闭写端后的短读，后者检查超过缓冲区容量的 3000 字节传输和双向管道结果。两个管道测例均应成功完成并以 `0` 退出。

结果以各子测例的实际输出、原有断言和退出码为准。`ch4b_sbrk` 会主动访问已回收页面，在本章通过 `SIGSEGV` 处理，预期退出码为 `-11`；不能沿用前章的 `-2`。其余顶层子测例应以 `0` 退出。进程创建必须返回有效 PID，等待返回的 PID 应与目标进程一致。

现有总测例只断言等待返回值与创建返回值相等，并打印退出码，没有逐项断言预期退出码。因此，最后的 `Basic usertests passed!` 提示不能替代对子测例实际执行结果的判断。这个集合也没有覆盖 shell 重定向中的 `sys_dup` 和零长度读写等全部边界，接口实现仍需满足本文契约。

未完成 TODO 时，实验骨架可以启动到普通用户终端。运行上述总测例后，首次创建管道会触发 `sys_pipe` 的 TODO；补全它后，管道路径还会依赖其余对应接口。启动成功或往章测例完成，不代表本章接口已经实现。
