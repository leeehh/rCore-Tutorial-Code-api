# rCore ch5 API 实验：进程生命周期与 stride 调度

## 实验内容

本实验基于 `ch5-api` 分支，要求同学借助 AI 完成第五章 `task` 模块中的进程管理与调度。完成后的模块需要支持进程创建、复制、执行程序替换、退出和回收，维护父子关系，并通过 stride 算法为就绪进程分配 CPU 时间。

第三章的任务来自静态加载的应用集合。本章的进程可以在运行期间创建，每个进程拥有独立的地址空间、PID 和内核栈。调度通过 `Processor` 保存的 idle 上下文完成：运行中的进程暂停或退出后，返回调度循环，再由调度循环选择下一个进程。

实验提供必要的数据结构、固定接口签名及配套实现。学生只修改下表中的四个文件，共完成十一处 TODO；各接口的名称、可见性、参数和返回类型保持原样。内部辅助函数可以在这四个文件中自行设计。

| 实现文件 | 待实现接口 | 数量 |
| --- | --- | --- |
| [os/src/task/task.rs](os/src/task/task.rs) | `TaskControlBlock::new`、`fork`、`exec`、`spawn`、`waitpid`、`set_priority` | 6 |
| [os/src/task/manager.rs](os/src/task/manager.rs) | `TaskManager::fetch` | 1 |
| [os/src/task/processor.rs](os/src/task/processor.rs) | `run_tasks`、`schedule` | 2 |
| [os/src/task/mod.rs](os/src/task/mod.rs) | `suspend_current_and_run_next`、`exit_current_and_run_next` | 2 |

本文沿用前两章 API 实验按模块和接口组织 `description`、签名、输入、输出与关键约束的形式。以下契约以本分支代码为准。页表、地址空间和系统调用适配已提供，本实验不要求维护 `sys_trace`。

## 实验要求

1. **代码修改范围**：仅允许修改 `ch5-api` 分支中 [os/src/task/task.rs](os/src/task/task.rs) 的 `TaskControlBlock::new/fork/exec/spawn/waitpid/set_priority`、[os/src/task/manager.rs](os/src/task/manager.rs) 的 `TaskManager::fetch`、[os/src/task/processor.rs](os/src/task/processor.rs) 的 `run_tasks/schedule`，以及 [os/src/task/mod.rs](os/src/task/mod.rs) 的 `suspend_current_and_run_next/exit_current_and_run_next`，共十一处 TODO；内部辅助函数可在这四个文件中设计。保留已有类型、接口和配套实现，不修改其他代码文件、构建配置或测试文件；可以新增实验报告等说明文档。

2. **静态分析与动态跟踪**：阅读 `ch5` 分支中进程管理与调度的参考实现，使用 GDB 跟踪进程复制及调度等关键路径，说明地址空间、父子关系、进程状态、上下文和 stride 如何变化。操作参考 [ch5 源代码分析与动态跟踪文档](../../blob/ch5/rcore-ch5-analyze.md)。报告应记录实际断点、GDB 命令、观察结果和分析结论，可附必要代码片段或源码链接。参考分支的等待回收和优先级逻辑位于 syscall 中，API 已拆到 TCB 接口；参考 `exec` 未重置堆边界，API 明确要求重置，也应分析这一区别。

3. **独立实现与对比**：依据本文契约独立完成十一处接口，并按“运行与验收”小节执行验收。在报告中对比自己的实现与 `ch5` 参考实现的功能、状态转换、资源生命周期和调度方法，说明实现选择及原因；不能只罗列代码文本差异。

4. **主要问题与解决思路**：总结主要问题的现象、原因、排查过程、解决思路及处理结果，结合代码位置、GDB 记录或运行输出说明依据。特别关注动态借用、上下文指针生命周期及退出与回收的分工；未解决的问题如实记录。

实验报告采用 Markdown 格式，保存到仓库根目录的 `reports/lab5.md`，至少包含静态分析与动态跟踪、独立实现对比、主要问题与解决思路三部分，并随实验代码提交。参考实现上的 GDB 跟踪用于分析，完成后的功能验收仍使用本文规定的命令和既有测例。

## 提供的数据结构

### TaskStatus

description: `TaskStatus` 定义进程状态，位于 [task/task.rs](os/src/task/task.rs)。状态表示进程是否具备调度资格，与地址空间或 CPU 上下文本身分开维护。

```rust
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    UnInit,
    Ready,
    Running,
    Zombie,
}
```

| 状态 | 含义 |
| --- | --- |
| `UnInit` | 尚未完成初始化；本实验的进程创建接口直接产生 `Ready` 进程 |
| `Ready` | 可以执行，等待调度 |
| `Running` | 当前正在执行，包括该进程的内核处理过程 |
| `Zombie` | 已退出，保存 PID 和退出码等待父进程回收，不得再次调度 |

### TaskControlBlock 与 TaskControlBlockInner

description: `TaskControlBlock` 保存进程身份、内核栈和可变运行状态。进程通过 `Arc<TaskControlBlock>` 在处理器、就绪队列和父进程之间共享；子进程通过 `Weak` 指向父进程，避免父子引用形成强引用环。

```rust
pub struct TaskControlBlock {
    pub pid: PidHandle,
    pub kernel_stack: KernelStack,
    inner: UPSafeCell<TaskControlBlockInner>,
}

pub struct TaskControlBlockInner {
    pub trap_cx_ppn: PhysPageNum,
    pub base_size: usize,
    pub task_cx: TaskContext,
    pub task_status: TaskStatus,
    pub stride: usize,
    pub prio: usize,
    pub memory_set: MemorySet,
    pub parent: Option<Weak<TaskControlBlock>>,
    pub children: Vec<Arc<TaskControlBlock>>,
    pub exit_code: i32,
    pub heap_bottom: usize,
    pub program_brk: usize,
}
```

`memory_set` 拥有用户地址空间，`trap_cx_ppn` 指向该地址空间内保存陷阱上下文的物理页。`base_size` 保存程序初始用户栈顶对应的边界；`heap_bottom` 和 `program_brk` 分别记录堆起点和当前堆末尾，供已有 `sbrk` 实现使用。

`task_cx` 保存进程在内核中的切换现场；陷阱上下文保存用户寄存器状态及陷阱处理所需的内核信息。两者共同支持进程首次进入用户态，以及暂停后继续原有执行。

`children` 包含尚未被回收的子进程，其中既可能有正在运行的进程，也可能有 `Zombie`。`exit_code` 在退出时设置。进程最终释放时，`PidHandle` 与 `KernelStack` 的已有析构实现归还 PID、内核栈映射和栈编号。

### TaskManager 与 Processor

description: [manager.rs](os/src/task/manager.rs) 的 `TaskManager` 保存就绪队列；[processor.rs](os/src/task/processor.rs) 的 `Processor` 保存当前运行进程和调度循环的 idle 上下文。

```rust
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

pub struct Processor {
    current: Option<Arc<TaskControlBlock>>,
    idle_task_cx: TaskContext,
}
```

`TASK_MANAGER`、`PROCESSOR` 及其初始化已提供。`add_task` 将进程加入队尾，`fetch_task` 调用待实现的 `TaskManager::fetch`。`current_task` 克隆当前进程的 `Arc`，`take_current_task` 移出该 `Arc` 并将 `Processor.current` 留为 `None`。

idle 上下文表示内核调度循环，不是一个用户进程，也没有自己的 PID。代码中的 `IDLE_PID = 0` 是首个用户进程的 PID，用于保留现有退出策略，不能把它与 `Processor.idle_task_cx` 混为一谈。

## os::task::task

description: 本子模块负责创建和修改进程控制块，维护地址空间、父子关系、退出后的回收和调度属性。以下六个方法是固定接口；进程创建方法负责准备执行环境，就绪队列由已有调用方管理。

### TaskControlBlock::new

description: 根据 ELF 程序创建一个独立进程，使其具备首次进入用户态执行的条件。初始进程和 `spawn` 都依赖此接口。

```rust
pub fn new(elf_data: &[u8]) -> Self;
```

**输入**

`elf_data` 是加载器提供的有效 ELF 程序字节。已有 `MemorySet::from_elf` 提供程序地址空间、初始用户栈指针和入口地址，PID 与内核栈分配接口均已提供。

**输出**

返回完整的 `TaskControlBlock`。进程处于 `Ready`，拥有独立的 PID、内核栈和用户地址空间；尚未进入就绪队列，也尚未执行。

**关键约束**

- `trap_cx_ppn` 对应新地址空间中 `TRAP_CONTEXT_BASE` 所在的物理页。
- `task_cx` 支持通过本进程内核栈进入已有的 `trap_return`。
- 陷阱上下文完整初始化，包括用户态权限、程序入口、用户栈指针、内核页表 token、内核栈顶和 `trap_handler` 地址。不能仅设置入口和用户栈。
- `base_size`、`heap_bottom`、`program_brk` 均初始化为 `from_elf` 返回的用户栈指针。
- `parent` 为 `None`，`children` 为空，`exit_code = 0`，`stride = 0`，`prio = 16`。
- 不在本接口中入队；无需为有效 ELF 输入之外的情况新增错误返回接口。

### TaskControlBlock::fork

description: 以当前进程为父进程，复制其用户执行环境并建立一个子进程。父子进程的用户数据相互独立，子进程从复制的用户现场继续执行。

```rust
pub fn fork(self: &Arc<Self>) -> Arc<Self>;
```

**输入**

`self` 是父进程的共享引用。父进程提供待复制的地址空间、陷阱上下文和栈、堆元数据。

**输出**

返回新子进程的 `Arc`，并将同一子进程登记到父进程的 `children`。子进程为 `Ready`，其 `parent` 弱引用指向父进程，尚未加入就绪队列。

**关键约束**

- 使用已有地址空间复制能力深拷贝用户数据和陷阱上下文；父子进程不能共享可写用户数据页。
- 子进程分配新的 PID 和内核栈，其 `task_cx` 支持从自己的内核栈进入 `trap_return`。
- `trap_cx_ppn` 指向子进程的陷阱上下文；其中的 `kernel_sp` 必须对应子进程内核栈。
- 继承父进程的 `base_size`、`heap_bottom` 和 `program_brk`；子进程的 `children` 为空、`exit_code = 0`。
- 子进程重新初始化 `stride = 0`、`prio = 16`，不继承父进程的调度属性。
- 已有 `sys_fork` 负责设置子进程陷阱上下文中的返回值 `a0 = 0`，将子进程入队，并向父进程返回子进程 PID。本接口保留这一职责划分。

### TaskControlBlock::exec

description: 将当前进程的用户程序替换为指定 ELF 程序。进程身份和关系保持有效，系统调用处理完成后从新程序入口返回用户态。

```rust
pub fn exec(&self, elf_data: &[u8]);
```

**输入**

`self` 是待替换程序的进程，`elf_data` 是加载器提供的有效目标 ELF。文件名查询及查询失败时的返回值由已有 `sys_exec` 处理。

**输出**

返回 `()`。进程拥有新程序的地址空间和完整的初始陷阱上下文，后续陷阱返回使用新入口与新用户栈。

**关键约束**

- 替换 `memory_set` 并更新 `trap_cx_ppn`，后续访问不能继续使用旧地址空间中的陷阱上下文。
- `base_size`、`heap_bottom`、`program_brk` 均重置为新程序的初始用户栈指针。
- 完整更新陷阱上下文，包含新程序入口、用户栈、用户态权限、内核页表 token、已有内核栈顶和陷阱处理入口。
- 保留 PID、内核栈、父子关系、退出码、任务状态及 `stride`、`prio`；不创建新进程或新增队列项。
- 保留当前内核调用流使用的 `task_cx`，让现有系统调用和陷阱返回路径完成程序替换。
- 替换后释放旧用户地址空间及其拥有的资源，不能遗留对旧陷阱上下文的访问。

### TaskControlBlock::spawn

description: 直接从目标 ELF 创建子进程。该接口复用 `new` 准备新的程序执行环境，并补充父子关系。

```rust
pub fn spawn(self: &Arc<Self>, elf_data: &[u8]) -> Arc<Self>;
```

**输入**

`self` 是父进程，`elf_data` 是加载器找到的目标 ELF。用户传入的文件名转换和无效文件名处理由已有 `sys_spawn` 完成。

**输出**

返回新的子进程 `Arc`。子进程位于父进程的 `children` 中，其 `parent` 弱引用指向父进程。

**关键约束**

- 必须复用 `TaskControlBlock::new` 创建程序环境，满足其完整初始化契约。
- 子进程从目标程序入口开始执行，初始 `stride = 0`、`prio = 16`。
- 父子关系登记一次，父进程持有的子进程与返回值指向同一个 TCB。
- 不在本接口中入队。已有 `sys_spawn` 负责入队并返回子进程 PID；目标文件不存在时返回 `-1`，不调用本接口。

### TaskControlBlock::waitpid

description: 查找指定子进程，并在其已退出时完成回收。该接口立即返回查询结果；用户库已有的等待逻辑负责在子进程尚未退出时重试。

```rust
pub fn waitpid(&self, pid: isize) -> Result<(usize, i32), isize>;
```

**输入**

`self` 是父进程。`pid == -1` 表示任意子进程；其他值匹配相应 PID。查找范围仅为本进程的 `children`，不搜索其他进程的子进程。

**输出**

| 返回值 | 含义 |
| --- | --- |
| `Ok((child_pid, exit_code))` | 找到并回收一个匹配的 `Zombie` 子进程，返回其 PID 和退出码 |
| `Err(-1)` | 不存在匹配的子进程 |
| `Err(-2)` | 存在匹配子进程，但没有匹配的 `Zombie` 子进程可回收 |

**关键约束**

- `pid == -1` 时按 `children` 的顺序选择第一个已退出的子进程；不能因前面的子进程仍在运行而忽略后面的 `Zombie`。
- 成功时从 `children` 移除该子进程，取得返回信息后释放持有的引用，使 PID、内核栈和剩余地址空间资源能够最终回收。
- 每个子进程只能成功回收一次；失败时不移除或修改任何子进程。
- 本接口不接受用户指针、不转换用户地址，也不写用户内存。已有 `sys_waitpid` 仅在成功时写回退出码，将本接口的结果转换为系统调用返回值。
- 返回后不遗留对子进程的强引用或动态借用；整个退出与回收流程应保证 `Zombie` 不被处理器或就绪队列继续持有。

### TaskControlBlock::set_priority

description: 设置本进程的 stride 调度优先级，供 `sys_set_priority` 使用。

```rust
pub fn set_priority(&self, prio: isize) -> isize;
```

**输入**

`prio` 是请求的优先级，合法范围为 `prio >= 2`。不能因优先级大于 `BIG_STRIDE` 而拒绝输入。

**输出**

合法时将进程的 `prio` 更新为对应的 `usize` 值并返回传入值；非法时返回 `-1`，保持原优先级。

**关键约束**

- 不修改已经累计的 `stride`，不改变进程状态，也不触发立即切换。
- 后续调度按新优先级计算步长；结构体无需额外保存 `pass` 字段。

## os::task::manager

description: 本子模块维护就绪队列。队列初始化、队尾插入和全局包装函数已经提供，学生实现按照 stride 选择进程的 `fetch`。

### TaskManager::fetch

description: 从就绪队列中选出当前 stride 最小的进程，移出队列，并为本次调度累加步长。

```rust
pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>>;
```

**输入**

`self.ready_queue` 保存 `Ready` 进程，各进程的优先级满足 `prio >= 2`。[config.rs](os/src/config.rs) 已定义 `pub const BIG_STRIDE: usize = 1 << 16;`。

**输出**

空队列返回 `None`；非空队列返回被选进程的 `Some(Arc<...>)`，该进程已经从队列移除，且其 `stride` 增加本次步长。

**关键约束**

- 根据累加前的 `stride` 选择最小值；多个进程相同时选择队列中位置最靠前者。
- 步长严格使用 `BIG_STRIDE / prio` 的 `usize` 整数除法，每次选中只累加一次。
- 不给除法结果增加下限，不为大优先级另设上限；合法优先级大于 `BIG_STRIDE` 时步长可以为零。
- 保持未选中进程的相对队列顺序和调度属性。
- 返回的进程仍为 `Ready`，由 `run_tasks` 在准备实际运行时设为 `Running`。

## os::task::processor

description: 本子模块管理 CPU 的当前进程，并通过 idle 上下文连接不同进程的内核控制流。`Processor` 的构造、访问器及全局初始化均已提供。

### run_tasks

description: 内核启动后进入的调度循环，反复获取就绪进程，使其开始或恢复执行。进程暂停或退出后，控制流回到此循环继续调度。

```rust
pub fn run_tasks();
```

**输入**

没有显式参数。使用 `PROCESSOR` 和 `fetch_task()`；已有启动代码负责将初始进程加入队列。

**输出**

正常情况下持续运行，不返回内核启动调用点。每次选中进程后，其状态为 `Running`，`Processor.current` 持有该进程，CPU 从对应 `task_cx` 开始或继续执行。

**关键约束**

- 使用 `fetch_task` 统一选择进程并累计 stride，不在本接口重复计费。
- 从 idle 上下文切换到选中进程的任务上下文，进程返回 idle 后继续下一轮调度。
- 切换前设置当前进程与 `Running` 状态，确保二者与实际运行的进程一致。
- 切换时释放 `Processor` 和 TCB 的动态借用；上下文指针的所有者必须保持存活。
- 队列为空时继续等待后续调度机会，不对 `None` 解包，也不把空队列解释为初始进程退出。

### schedule

description: 将当前内核控制流切换回 idle 调度循环。调用方决定进程是暂停还是退出，本接口负责保存给定现场并恢复 idle 上下文。

```rust
pub fn schedule(switched_task_cx_ptr: *mut TaskContext);
```

**输入**

`switched_task_cx_ptr` 指向可写的任务上下文保存位置。暂停路径使用当前进程的 `task_cx`；退出路径可以提供无需再次恢复的临时上下文。调用方已处理任务状态、队列及 `Processor.current`。

**输出**

执行权返回 `run_tasks` 的 idle 控制流。若保存的进程以后被重新选中，本次调用才在该进程中返回，延续原内核执行流。

**关键约束**

- 使用已有 `__switch` 完成切换，保存位置在切换期间有效且可写。
- idle 上下文来自 `PROCESSOR`，切换时不持有其动态借用。
- 本接口不选择后继进程，不修改 stride，也不负责入队或退出资源清理。

## os::task

description: 本模块向系统调用和陷阱处理提供暂停、退出入口。两条路径都通过 `schedule` 回到 idle，再由 `run_tasks` 选择后继进程。

### suspend_current_and_run_next

description: 暂停当前进程，保留其执行进度和再次调度的资格。应用主动 `yield` 和时钟抢占共用此入口。

```rust
pub fn suspend_current_and_run_next();
```

**输入**

没有显式参数，`Processor.current` 持有正在运行的进程，其状态为 `Running`。

**输出**

当前进程变为 `Ready` 并加入就绪队列，处理器的当前进程被移出，执行权返回 idle。该进程再次被调度后，本次调用返回 `()`，原系统调用或陷阱处理继续执行。

**关键约束**

- 保留当前进程的任务上下文、用户地址空间、父子关系及调度属性；队列中只加入一次。
- 通过 `schedule` 保存现场，不直接选择或切换到另一个用户进程。
- 上下文指针在切换和后续恢复期间保持有效，切换时不持有 TCB 或处理器的动态借用。
- 即使只有一个可运行进程，也要允许该进程完成暂停和再次恢复。

### exit_current_and_run_next

description: 终止当前进程，记录退出结果、移交子进程并释放用户数据页，然后返回调度循环。正常退出和异常终止共用此入口。

```rust
pub fn exit_current_and_run_next(exit_code: i32);
```

**输入**

`exit_code` 为正常退出或异常处理提供的退出码；`Processor.current` 持有待退出的 `Running` 进程。

**输出**

普通进程进入 `Zombie`，保留供父进程等待的身份与退出码，并让其他进程继续调度。本接口签名返回 `()`，但正常退出路径不会返回原调用点。

**关键约束**

- 从处理器移出当前进程，记录 `exit_code` 并设为 `Zombie`，不重新入队，也不恢复其用户或内核执行现场。
- 将其所有子进程交给已有 `INITPROC`：更新每个子进程的 `parent` 弱引用，加入 `INITPROC.children`，并清空退出进程原来的 `children`。
- 使用已有 `memory_set.recycle_data_pages()` 释放用户数据页。TCB、PID、内核栈及剩余页表资源留待父进程成功 `waitpid` 后最终释放。
- 返回 idle 前释放动态借用与本路径持有的局部 `Arc`。退出栈不会再次正常返回，不能依靠函数返回来释放这些引用。
- PID 为 `IDLE_PID`（即 `0`）时，保留当前仓库的日志和 `panic!("All applications completed!")` 终止策略，不进入普通进程的孤儿移交流程。

## 已提供的配套功能与共同约束

以下代码完整提供，学生保持现有数据结构、实现和调用关系。实现范围仍限于开头列出的四个 `task` 文件。

| 代码位置 | 已提供内容 |
| --- | --- |
| [task/context.rs](os/src/task/context.rs) | `TaskContext`、`zero_init()`、`goto_trap_return()` |
| [task/switch.rs](os/src/task/switch.rs)、[task/switch.S](os/src/task/switch.S) | `__switch` 声明与寄存器切换汇编 |
| [task/id.rs](os/src/task/id.rs) | PID、内核栈分配及其析构回收 |
| [task/task.rs](os/src/task/task.rs) | TCB 数据结构、内部访问器、状态查询、`getpid`、页表 token 查询和 `change_program_brk` |
| [task/manager.rs](os/src/task/manager.rs)、[task/processor.rs](os/src/task/processor.rs) | 管理器与处理器的初始化、入队、全局访问包装函数和当前进程查询 |
| [task/mod.rs](os/src/task/mod.rs) | `INITPROC` 的创建表达式、`add_initproc` 和模块导出 |
| [mm](os/src/mm) | ELF 地址空间构造、地址空间深拷贝、用户地址转换、映射和资源回收 |
| [syscall/process.rs](os/src/syscall/process.rs) | 完整系统调用适配，包括已迁移的 `sys_get_time`、`sys_mmap`、`sys_munmap` |
| [trap](os/src/trap)、[timer.rs](os/src/timer.rs) | 陷阱上下文初始化、陷阱返回、异常和定时器处理 |
| [sync/up.rs](os/src/sync/up.rs)、[config.rs](os/src/config.rs) | 单核动态借用支持、调度常量及其他配置 |

系统调用分工保持明确：`sys_fork` 设置子进程返回值并入队；`sys_exec` 和 `sys_spawn` 查询 ELF；`sys_spawn` 将新子进程入队；`sys_waitpid` 负责向用户地址写退出码；`sys_set_priority` 将参数交给 TCB 接口。系统调用编号保持原样，`spawn` 为 `400`，`set_priority` 为 `140`。

`UPSafeCell::exclusive_access()` 返回的 `RefMut` 遵守动态借用约束。同一对象不能出现重叠的独占借用，所有上下文切换前都必须结束相关借用。将引用转成裸指针并不延长所指对象的生命周期，进程必须由适当的 `Arc` 所有者保证存活。

进程退出与回收是两个阶段：退出使进程失去执行资格并释放用户数据页，回收移除父进程持有的 TCB，最终释放剩余资源。就绪队列、处理器和父子关系必须共同维持这一约定，不能遗留阻止回收的强引用。

## 运行与验收

在已有运行环境和 `user` 测试目录准备好后，从仓库根目录执行：

```bash
cd os
make run BASE=2
```

本分支的 Makefile 已支持从 `ch5-api` 分支名提取章节号。`BASE=2` 加载已有第二至第五章的基础与编程测试应用。进入用户终端后输入：

```text
ch5_usertest
```

功能验收只使用这一已有测试入口，不新增测试。实现完成后，内核应启动到用户终端，并完成 `ch5_usertest` 中的应用启动、等待回收、优先级和 stride 检查。

结果以各子测例的实际输出、原有断言和退出码为准：`ch4_mmap1`、`ch4_mmap2` 因预期访问异常退出，退出码应为 `-2`；其余子测例应成功退出，退出码为 `0`。进程创建应返回有效 PID，等待返回的 PID 应与目标子进程一致。现有总测例的最终 `ch5 Usertests passed!` 标语不能替代这些子测例结果。

未完成的骨架会在创建初始进程时触发 `TaskControlBlock::new` 的 TODO。能够编译并运行到该位置仅说明骨架可以构建；完成十一处接口后，才具备进入用户终端并运行现有测例的条件。
