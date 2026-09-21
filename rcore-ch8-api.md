# rCore ch8 API 实验：线程同步与死锁检测

## 实验内容

本实验基于 `ch8-api` 分支，要求同学借助 AI 完成第八章的线程同步模块。完成后的模块需要支持互斥锁、信号量和条件变量，在资源不可用时正确等待，并根据当前资源分配和请求状态判断是否存在安全的完成顺序。

线程创建与回收、进程管理、FIFO 调度、定时器和系统调用已经提供。学生只修改下表中的四个文件，共完成九处 TODO。保持已有类型、字段、接口名称、可见性、参数和返回类型，保留构造器、资源访问接口、资源记账方法及已有日志方式；可以在这四个文件内自行设计私有辅助函数，不新增公共 API。

| 实现文件 | 待实现接口 | 数量 |
| --- | --- | --- |
| [os/src/sync/mutex.rs](os/src/sync/mutex.rs) | `MutexSpin::lock`、`unlock`，`MutexBlocking::lock`、`unlock` | 4 |
| [os/src/sync/semaphore.rs](os/src/sync/semaphore.rs) | `Semaphore::down`、`up` | 2 |
| [os/src/sync/condvar.rs](os/src/sync/condvar.rs) | `Condvar::wait`、`signal` | 2 |
| [os/src/sync/deadlock.rs](os/src/sync/deadlock.rs) | `DeadlockDetector::is_safe` | 1 |

本文沿用前几章 API 实验按模块和接口组织 `description`、签名、输入、输出与关键约束的形式。以下契约以本分支代码为准，线程与同步对象的配套接入不属于待实现内容。

## 实验要求

1. **代码修改范围**：仅允许修改 `ch8-api` 分支中 [os/src/sync/mutex.rs](os/src/sync/mutex.rs) 的 `MutexSpin::lock/unlock`、`MutexBlocking::lock/unlock`，[os/src/sync/semaphore.rs](os/src/sync/semaphore.rs) 的 `Semaphore::down/up`，[os/src/sync/condvar.rs](os/src/sync/condvar.rs) 的 `Condvar::wait/signal`，以及 [os/src/sync/deadlock.rs](os/src/sync/deadlock.rs) 的 `DeadlockDetector::is_safe`，共九处 TODO；私有辅助函数可在这四个文件中设计。保留类型、签名、构造器、资源访问器、全部 `Resource` 方法及线程清理等配套实现，不新增公共 API，不修改其他代码文件、构建配置或测试文件；可以新增实验报告等说明文档。

2. **静态分析与动态跟踪**：阅读 `ch8` 分支的线程同步参考实现，使用 GDB 观察互斥锁和信号量操作，并静态分析阻塞、唤醒与条件变量的调用关系。跟踪时保持死锁检测默认关闭，不调用 `sys_enable_deadlock_detect(1)`，不要求跟踪 `Resource` 或 `DeadlockDetector::is_safe`。操作参考 [ch8 源代码分析与动态跟踪文档](../../blob/ch8/rcore-ch8-analyze.md)。报告应记录实际断点、命令、锁状态、信号量计数、等待队列和线程状态及结论，可附必要源码片段或链接。

3. **独立实现与对比**：依据本文契约独立完成九处接口，并按“运行与验收”小节执行验收。在报告中对比自己的实现与 `ch8` 参考实现的等待方式、资源交接、请求拒绝和安全性模拟，说明采用相关方式的原因；保持单核假设、两类资源分别检测等范围，不新增跨类型死锁或条件变量入图机制。

4. **主要问题与解决思路**：总结主要问题的现象、原因、排查过程、解决思路及处理结果，结合代码位置、GDB 记录或运行输出说明依据。关注调度前释放借用、FIFO 唤醒、锁或许可交接，以及条件等待后重新持锁等问题；未解决的问题如实记录。

实验报告采用 Markdown 格式，保存到仓库根目录的 `reports/lab8.md`，至少包含静态分析与动态跟踪、独立实现对比、主要问题与解决思路三部分，并随实验代码提交。参考实现上的 GDB 跟踪用于分析，完成后的功能验收仍使用本文规定的命令和既有测例。

## 提供的数据结构与调度接口

### Mutex、MutexSpin 与 MutexBlocking

description: [mutex.rs](os/src/sync/mutex.rs) 定义统一的互斥锁接口。两种实现都通过 `Resource` 记录锁资源，区别在于获取失败后的等待方式。

```rust
pub trait Mutex: Sync + Send {
    fn resource(&self) -> &Resource;
    fn lock(&self);
    fn unlock(&self);
}

pub struct MutexSpin {
    locked: UPSafeCell<bool>,
    resource: Resource,
}

pub struct MutexBlocking {
    inner: UPSafeCell<MutexBlockingInner>,
    resource: Resource,
}

pub struct MutexBlockingInner {
    locked: bool,
    wait_queue: VecDeque<Arc<TaskControlBlock>>,
}
```

`locked` 表示锁当前是否已被占用，阻塞锁的 `wait_queue` 按 FIFO 顺序保存等待线程。锁资源的初始数量为 `1`，由创建系统调用注册。每个锁对象有自己的状态和资源，不共享不同锁的占用标志或等待队列。

`MutexSpin::new(resource: Resource) -> Self`、`MutexBlocking::new(resource: Resource) -> Self` 及两种实现的 `resource()` 已提供。构造器保存资源并初始化未占用的锁，阻塞锁的等待队列初始为空；`resource()` 返回对应资源的引用。

### Semaphore 与 SemaphoreInner

description: [semaphore.rs](os/src/sync/semaphore.rs) 使用有符号计数表示剩余许可和等待情况，并通过 FIFO 队列保存阻塞线程。

```rust
pub struct Semaphore {
    pub inner: UPSafeCell<SemaphoreInner>,
    resource: Resource,
}

pub struct SemaphoreInner {
    pub count: isize,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}
```

`Semaphore::new(res_count: usize, resource: Resource) -> Self` 已提供，初始 `count` 为 `res_count as isize`，等待队列为空。创建系统调用以相同的初始数量注册资源；本实验以资源数量和后续计数可由现有类型表示为前提。

`count > 0` 表示有可立即获取的许可；`count == 0` 表示没有剩余许可且没有排队请求；`count < 0` 时，`-count` 表示等待队列中的线程数量。一次成功登记的 `down` 将计数减一，一次 `up` 将计数加一。这里的“成功”包括接受请求后等待，死锁检测拒绝的请求不进入计数或队列。

### Condvar 与 CondvarInner

description: [condvar.rs](os/src/sync/condvar.rs) 保存等待条件通知的线程。条件变量本身不保存用户条件，也没有可积累的许可。

```rust
pub struct Condvar {
    pub inner: UPSafeCell<CondvarInner>,
}

pub struct CondvarInner {
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}
```

`Condvar::new() -> Self` 已提供，创建空等待队列。调用者通过互斥锁保护共享条件，并在持有该锁时循环检查条件、按需调用 `wait`；通知只表示线程可以继续检查条件，不保证用户条件一直成立。

### 线程调度

本实验依赖单核、内核态不发生任务抢占的既有假设。同步操作在显式调度之前可以连续完成状态更新；不能把本章实现直接当作多核或可抢占内核中的同步方案。

以下调度接口已经提供：

| 接口 | 行为 |
| --- | --- |
| `current_task() -> Option<Arc<TaskControlBlock>>` | 取得当前运行线程的共享引用；本章调用路径中存在当前线程 |
| `suspend_current_and_run_next()` | 将当前线程设为 `Ready`，放回就绪队列，再切换出去 |
| `block_current_and_run_next()` | 将当前线程设为 `Blocked` 并切换出去，不放回就绪队列 |
| `wakeup_task(task: Arc<TaskControlBlock>)` | 将目标线程设为 `Ready` 并加入就绪队列，不立即切换到该线程 |

等待队列保存 `Arc<TaskControlBlock>`，入队使用队尾、唤醒取出队首。被阻塞的线程只有经已有唤醒路径才重新进入就绪队列，不能使用反复让出 CPU 代替阻塞队列。

调用 `suspend_current_and_run_next()` 或 `block_current_and_run_next()` 前，必须释放同步对象、任务、进程及检测器等内部状态的 `RefMut`。恢复后需要检查状态的路径应重新取得借用。释放的是内部状态借用，等待期间仍可持有同步对象的 `Arc`。

## 资源记账与检测模型

### DeadlockDetector 与 Resource

description: [deadlock.rs](os/src/sync/deadlock.rs) 保存资源可用量、线程已持有数量，以及每个线程当前的一项待满足请求。

```rust
#[derive(Default)]
pub struct DeadlockDetector {
    pub enabled: bool,
    available: Vec<usize>,
    allocation: Vec<Vec<usize>>,
    need: Vec<Option<usize>>,
}

pub struct Resource {
    id: usize,
    detector: Arc<UPSafeCell<DeadlockDetector>>,
}
```

| 字段 | 类型与含义 |
| --- | --- |
| `enabled` | `bool`，控制请求方是否拒绝不安全请求，默认 `false` |
| `available[r]` | `usize`，资源 `r` 当前未分配的单位数，始终非负 |
| `allocation[t][r]` | `usize`，线程 `t` 已持有的资源 `r` 的单位数 |
| `need[t]` | `None` 表示没有待满足请求，`Some(r)` 表示正在请求资源 `r` 的一个单位 |
| `Resource::id` | 该资源在所属检测器中的列索引 |
| `Resource::detector` | 共享的检测器状态，同一检测器管理多个同类资源 |

矩阵行索引是进程内的线程 ID，列索引是检测器内的资源 ID。`allocation.len() == need.len()`，每行长度与 `available.len()` 相等，已记录的请求指向有效资源列。线程行按需建立，不要求每一行都对应当前活跃线程。

这不是预先声明最大资源需求的模型：`need` 只保存当前一项单位请求，不保存线程未来会申请什么资源，也没有 `Max` 矩阵。安全性判断只使用当前记录。

信号量的 `count` 与检测器的 `available` 含义不同。例如初始许可为零，一个线程接受 `down` 并阻塞后，`count` 为 `-1`、队列有一个线程，而 `available` 仍为 `0`，请求记录在 `need` 中。不能把负计数写入 `available`，也不能在排队时提前扣减检测器的可用量。

### 已提供的资源方法

description: `Resource` 的全部方法已经实现，负责维护检测器状态。学生保留这些方法的实现，通过同步原语中的正确调用连接实际资源状态与记账状态。

| 签名 | 已提供的行为 |
| --- | --- |
| `pub fn new(detector: Arc<UPSafeCell<DeadlockDetector>>, count: usize) -> Self` | 新增资源列，将可用量初始化为 `count`，给已有分配行补零 |
| `pub fn wait(&self) -> usize` | 取得当前线程 ID，确保对应行存在，将 `need[tid]` 设为当前资源 ID，返回线程 ID |
| `pub fn request(&self) -> bool` | 先调用 `wait`；启用检测且 `is_safe()` 为假时，清除此线程的请求并返回 `false`，否则返回 `true` |
| `pub fn acquire(&self, task: &TaskControlBlock)` | 将一个单位分配给指定线程：可用量减一，线程持有量加一，清除该线程的请求 |
| `pub fn release(&self)` | 当前线程持有量饱和减一，资源可用量加一 |

`request` 接受请求只代表允许继续获取或等待，不代表已经取得资源；拒绝时只撤销本次 `need`，不修改实际分配，也不要求调用者归还尚未获得的资源。`acquire` 必须在资源确实可用时执行一次，指定的线程可以是当前线程，也可以是即将被唤醒的等待者。

`release` 保留既有的饱和减法语义：当前线程没有持有记录时，可用量仍然增加。这使信号量支持另一个线程用 `up` 发送通知，不能新增“只有持有者才能 up”的检查。互斥锁的正常使用则要求当前线程持锁后再解锁。

关闭死锁检测只关闭拒绝请求的判断，不关闭记账。`wait`、`request`、`acquire`、`release` 都要按各接口契约调用，保证之后启用检测时仍能观察已发生的分配。

`DeadlockDetector::ensure_thread(&mut self, tid: usize)` 为私有配套方法，扩展请求和分配行；`pub fn remove_thread(&mut self, tid: usize)` 清除对应请求与分配行。二者已经提供。`remove_thread` 不增加 `available`，线程退出不自动归还同步资源，不能把清理记录改成隐式 `unlock` 或 `up`。

### 系统调用接入

description: [syscall/sync.rs](os/src/syscall/sync.rs) 已经完成对象创建、索引查找、检测开关和返回值转换。系统调用在进入可能阻塞的同步方法前释放进程内部状态的借用。

`sys_mutex_lock` 先调用 `mutex.resource().request()`；拒绝时直接返回 `-0xDEAD`，接受后才调用 `mutex.lock()` 并返回 `0`。`Mutex::lock` 自身仍须调用 `wait` 登记请求，因为条件变量恢复后也会直接调用它重新获取锁。此重获路径没有第二次 `request` 检查，不能为返回 `()` 的 `lock` 新增拒绝返回值。

`sys_semaphore_down` 根据 `Semaphore::down` 的布尔结果返回 `0` 或 `-0xDEAD`。互斥锁解锁、信号量 `up`、条件变量通知及成功返回的等待系统调用均返回 `0`。本实验使用有效对象 ID，不扩展这些系统调用已有的参数校验行为。

`sys_enable_deadlock_detect(enabled: usize)` 只接受 `0`、`1`。参数无效，或启用时任一检测器当前不安全，返回 `-1`；成功返回 `0` 并设置两个检测器的开关。检测开关由调用者处理，`is_safe` 本身始终计算状态是否安全。

## os::sync::mutex

description: 本模块实现两种锁的获取与释放。两种锁都只有一个资源单位，均不是递归锁；实际占用状态、等待状态和资源记账必须一致。

### MutexSpin::lock

description: 尝试获取互斥锁，失败时让出 CPU，恢复后重试。

```rust
fn lock(&self);
```

**输入**

`self` 是有效的自旋式互斥锁。普通加锁系统调用已完成请求检查；条件变量重获锁直接进入本接口。

**输出**

返回 `()`；返回时当前线程已经获得锁及对应的一个资源单位。

**关键约束**

- 调用 `resource.wait()` 登记当前请求。
- 锁空闲时设为已占用，并对当前线程调用 `resource.acquire`，随后返回。
- 锁已占用时先释放 `locked` 的独占借用，再调用 `suspend_current_and_run_next()`；当前线程保持可再次调度的 `Ready` 状态。
- 恢复后重新取得借用并检查锁，不假设一次让出后就一定空闲。获取失败时不分配资源、不解除其他线程持有的锁。
- 不增加阻塞等待队列，不在持有独占借用时反复忙等。

### MutexSpin::unlock

description: 释放当前线程持有的自旋式互斥锁。

```rust
fn unlock(&self);
```

**输入**

调用前当前线程持有该锁。

**输出**

返回 `()`；锁变为空闲，对应的一个资源单位被归还。

**关键约束**

- 通过 `resource.release()` 归还资源，将占用标志置为 `false`。
- 此锁没有阻塞队列，不调用队列唤醒，也不主动切换线程。
- 不清除其他线程的请求，不替等待线程获取锁；它们之后重试时自行获取。

### MutexBlocking::lock

description: 获取互斥锁，锁已被占用时进入 FIFO 等待队列并阻塞。

```rust
fn lock(&self);
```

**输入**

`self` 是有效的阻塞互斥锁，请求检查与条件变量重获路径的约定同 `MutexSpin::lock`。

**输出**

返回 `()`；返回时当前线程已持有锁，可能在等待其他线程解锁后才返回。

**关键约束**

- 先调用 `resource.wait()` 登记请求，再检查锁状态。
- 锁空闲时设置占用标志，并对当前线程调用 `resource.acquire`。
- 锁已占用时将当前线程放入等待队尾，释放内部状态借用，再调用 `block_current_and_run_next()`。同一次等待只入队一次。
- 被唤醒的线程已经由解锁方取得资源，恢复后直接完成本次获取；不能再次 `acquire`，也不能把仍为 `true` 的占用标志当成需要重新排队的理由。

### MutexBlocking::unlock

description: 释放当前占用；有等待者时将锁直接交给队首线程。

```rust
fn unlock(&self);
```

**输入**

调用前当前线程持有该锁，应断言锁处于已占用状态。

**输出**

返回 `()`；没有等待者时锁空闲，有等待者时锁已交给一个等待线程。

**关键约束**

- 调用 `resource.release()` 归还旧持有者的一个资源单位。
- 队列非空时取出队首，对该等待线程调用 `resource.acquire(&waking_task)`，然后 `wakeup_task(waking_task)`。
- 交接时 `locked` 保持 `true`。资源先分配再唤醒，其他线程不能趁交接取得同一把锁。
- 队列为空时才将 `locked` 置为 `false`。一次解锁最多唤醒一个等待线程。

## os::sync::semaphore

description: 本模块实现计数信号量。请求检查、计数修改、等待队列和资源交付分别发生在约定位置，避免拒绝请求影响实际同步状态。

### Semaphore::down

description: 申请一个许可；接受但暂时没有许可时阻塞，检测拒绝时立即返回。

```rust
pub fn down(&self) -> bool;
```

**输入**

`self` 是已创建的信号量，当前线程每次调用申请一个许可。

**输出**

取得一个许可后返回 `true`；死锁检测拒绝本次请求时返回 `false`。

**关键约束**

- 首先调用 `resource.request()`。返回 `false` 时立即返回，不改变 `count`、等待队列或实际分配，不阻塞当前线程。
- 接受后将 `count` 减一；减一后非负，说明可以立即取得许可，对当前线程调用 `resource.acquire`。
- 减一后为负，将当前线程放入等待队尾，释放内部状态借用，再调用 `block_current_and_run_next()`。
- 等待者恢复时，许可已由 `up` 分配，直接返回 `true`，不能再次减计数或再次 `acquire`。
- 资源暂时不可用不等于检测拒绝；检测关闭或当前状态仍安全时，应按计数规则等待。

### Semaphore::up

description: 增加一个许可，有等待者时把该许可交给队首线程。

```rust
pub fn up(&self);
```

**输入**

`self` 是已创建的信号量。当前线程可以归还自己取得的许可，也可以通过 `up` 向其他线程发送通知。

**输出**

返回 `()`；计数增加一，最多唤醒一个等待线程。

**关键约束**

- 调用 `resource.release()`，再将 `count` 加一。
- 加一后 `count <= 0` 时，取出 FIFO 队首等待线程，先调用 `resource.acquire(&task)` 交付许可，再调用 `wakeup_task(task)`。
- 加一后 `count > 0` 时没有等待者需要交接，许可保留给后续 `down`。
- 不在唤醒时再次改变计数，不为一次 `up` 唤醒多个线程，不检查当前线程是否为先前许可的持有者。

## os::sync::condvar

description: 本模块实现条件等待和单个等待者通知。条件等待关联调用时传入的互斥锁，条件变量自身不加入资源检测器。

### Condvar::wait

description: 释放关联互斥锁并等待通知，被唤醒后重新取得该锁再返回。

```rust
pub fn wait(&self, mutex: Arc<dyn Mutex>);
```

**输入**

`mutex` 是保护共享条件的互斥锁；调用前当前线程已经持有它。

**输出**

返回 `()`；返回时当前线程重新持有传入的同一把锁，可以继续检查共享条件。

**关键约束**

- 按顺序执行：释放关联锁、当前线程加入条件变量等待队尾、释放队列借用、阻塞当前线程。
- 单核且内核态不抢占的假设保证上述过程在显式阻塞之前连续执行。解锁可能使其他线程就绪，但 `wakeup_task` 不立即切换线程。
- 被通知并恢复运行后，调用 `mutex.lock()` 重新获取锁，获取成功才返回；重新获取期间可能再次让出 CPU 或阻塞。
- 不把通知视为已经得到互斥锁，不在重新获取之前返回，也不持有条件变量借用调用阻塞或重新加锁路径。
- 保留重获锁的既有资源记账路径，不为条件变量新增资源列或第二套死锁拒绝逻辑。

### Condvar::signal

description: 通知最早等待在当前条件变量上的一个线程。

```rust
pub fn signal(&self);
```

**输入**

`self` 是已创建的条件变量。

**输出**

返回 `()`；队列非空时一个等待线程变为就绪，队列为空时无状态变化。

**关键约束**

- 从等待队首取出至多一个线程并调用 `wakeup_task`，保留其余等待者的 FIFO 顺序。
- 空队列上的通知不存储，未来的 `wait` 不能消费过去的通知。
- 不自动解锁、获取或转交关联互斥锁，不分配资源，不阻塞通知线程。

## os::sync::deadlock

description: 本模块的资源注册、请求登记与分配记录均已提供，只需补全对当前记录的安全性判断。

### DeadlockDetector::is_safe

description: 在临时状态中尝试安排所有已登记线程完成，判断是否存在满足当前请求的完成顺序。

```rust
pub fn is_safe(&self) -> bool;
```

**输入**

`self` 满足前述向量、矩阵及索引约束。调用方已将需要检查的请求写入 `need`，或正在检查启用检测前的现有状态。

**输出**

能在模型中使所有线程完成时返回 `true`，否则返回 `false`。没有线程行时返回 `true`。

**关键约束**

- 复制 `available` 作为临时可用向量 `work`，为每个线程建立尚未完成的标志，不修改真实的 `available`、`allocation`、`need` 或 `enabled`。
- 对尚未完成的线程，`need[tid] == None` 时可直接模拟完成；`Some(id)` 时仅当 `work[id] > 0` 才可模拟完成。
- 线程完成时将该线程整行已持有资源 `allocation[tid]` 加回 `work`，并标记完成；每行只能归还一次。
- 当前申请的一个单位在模拟中取得后又归还，净变化为零，因此完成时只需加回已有分配，不能额外增加或永久扣减请求单位。
- 重复扫描，直到全部完成，或一轮扫描没有任何进展。不能因某个线程暂时不可完成就立即判定失败，后续线程归还资源可能使它可完成。
- 无进展且仍有未完成线程时返回 `false`。不依据线程编号要求固定完成顺序，不引入最大需求矩阵或未来请求预测。
- 不在此方法里处理检测开关；即使 `enabled == false`，也返回实际模拟结果。是否据此拒绝请求由 `Resource::request` 等调用者决定。

## 已提供的配套功能与范围

| 代码位置 | 已提供内容 |
| --- | --- |
| [os/src/sync](os/src/sync) | 全部结构、构造器、`resource()`、`Resource` 的全部方法、线程记录扩展与清理、模块导出、`UPSafeCell` |
| [os/src/syscall/sync.rs](os/src/syscall/sync.rs) | 同步对象创建和查找、锁请求检查、返回值转换、检测开关、睡眠系统调用 |
| [os/src/syscall/thread.rs](os/src/syscall/thread.rs) | 线程创建、线程 ID 查询和等待线程退出 |
| [os/src/task/process.rs](os/src/task/process.rs) | 同步对象表、两类检测器、线程 ID 分配和回收接入 |
| [os/src/task](os/src/task) | 线程上下文、阻塞与唤醒、FIFO 就绪队列、退出和资源生命周期 |
| [os/src/timer.rs](os/src/timer.rs) | 定时等待、定时器到期唤醒与定时器移除 |
| [os/src/syscall/mod.rs](os/src/syscall/mod.rs) | 已有系统调用编号及分发 |
| [user](user) | 用户态接口与既有测试应用 |

进程内的 `mutex_list`、`semaphore_list`、`condvar_list` 保存同步对象共享引用。`mutex_detector` 与 `semaphore_detector` 分别是 `Arc<UPSafeCell<DeadlockDetector>>`，各自只记录一类资源；新进程中的检测器默认关闭。同步对象访问和线程生命周期接入均由配套代码处理。

现有模型不联合检测互斥锁与信号量之间的跨类型环，不将条件变量等待纳入资源图，不预测其他线程将来的 `up`，也不负责在线程退出时自动释放其持有的锁或许可。应保持这些边界，不通过改变线程退出、定时器或系统调用来扩展九处 TODO 的职责。

已有 `mmap`、`munmap`、`spawn`、`set_priority` 仍保留本分支的占位状态，不要求迁移往章实现，也不作为本次实验的前置工作。调度保持既有 FIFO 方式；线程创建、等待、退出和内核栈回收等配套代码无需重新实现。

## 运行与验收

在已有运行环境和 `user` 测试目录准备好后，从仓库根目录执行：

```bash
cd os
make run BASE=2
```

本分支的 Makefile 支持从 `ch8-api` 分支名提取章节号。`BASE=2` 准备已有第二至第八章的基础与编程测试应用，随后生成文件系统镜像并启动内核。进入用户终端后输入：

```text
ch8_usertest
```

功能验收只使用上述已有入口，不新增测例、压力测试或额外验收工具。`ch8_usertest` 包含 23 个顶层子测例，其中 13 个为往章测例，10 个为本章测例：

| 本章子测例 | 主要检查内容 |
| --- | --- |
| `ch8_deadlock_mutex1` | 同一线程重复获取已持有的阻塞锁，第二次请求应返回 `-0xDEAD` |
| `ch8_deadlock_sem1` | 不安全的信号量请求应被拒绝，内部至少一个工作线程以非零值退出 |
| `ch8_deadlock_sem2` | 可安全完成的信号量请求不应被误拒绝，内部工作线程均正常退出 |
| `ch8b_mpsc_sem`、`ch8b_sync_sem` | 信号量用于生产消费和线程通知 |
| `ch8b_phil_din_mutex`、`ch8b_race_adder_mutex_spin` | 阻塞锁与自旋式锁的互斥效果 |
| `ch8b_test_condvar` | 条件变量等待、通知和重新获取互斥锁 |
| `ch8b_threads`、`ch8b_threads_arg` | 已有线程创建、参数传递和等待功能 |
