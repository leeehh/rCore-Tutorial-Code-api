# rCore ch4 API 实验：页表与地址空间

## 实验内容

本实验基于 `ch4-api` 分支，要求同学借助 AI 完成第四章 `mm` 模块中的页表和地址空间管理。完成后的模块需要为内核和各应用建立符合权限要求的虚拟地址空间，支持用户地址访问、动态映射、解除映射和堆空间调整，并与已有任务管理和系统调用代码配合运行。

实验提供数据结构和必要的接口签名。实现范围是 [os/src/mm/page_table.rs](os/src/mm/page_table.rs) 和 [os/src/mm/memory_set.rs](os/src/mm/memory_set.rs)，共十一处 TODO：十个固定签名的函数，以及 `KERNEL_SPACE` 的初始化表达式。源码还给出五个函数签名作为内部实现提示，供学生选择和调整。

本文采用 [rCore-Tutorial-v3 接口文档](https://github.com/rcore-os/rCore-Tutorial-v3-api-doc/blob/main/rCore-Tutorial-v3.md) 按模块和接口组织 `description` 与代码声明的形式。具体输入、输出和关键约束以本仓库第四章代码为准。

## 实验要求

1. **代码修改范围**：仅允许在 [os/src/mm/page_table.rs](os/src/mm/page_table.rs) 和 [os/src/mm/memory_set.rs](os/src/mm/memory_set.rs) 中完成十一处 TODO，并按需要设计内部辅助函数。保留数据结构、固定接口签名，以及已提供的 `MapArea`、页表项操作和其他配套实现。不得修改其他任何代码文件、汇编、链接脚本、构建配置或测试文件；可以新增实验报告等说明文档。

2. **静态分析与动态跟踪**：阅读 `ch4` 的参考实现，使用 GDB 跟踪内核或用户地址空间的建立和用户缓冲区转换，分析页表映射、权限、页内偏移、逻辑段与页帧所有权之间的关系，并通过源码阅读说明动态映射、解除映射和堆空间调整。操作可参考 [ch4 源代码分析与动态跟踪文档](../../blob/ch4/rcore-ch4-analyze.md)。报告应结合源代码说明调用顺序及其原因，记录关键断点、使用的 GDB 命令、观察到的状态和分析结论；可附必要的代码片段或源码链接。

3. **独立实现与对比**：依据本文接口契约完成页表和地址空间管理，并按“运行与验收”小节执行验收。在报告中比较自己的实现与参考实现的相同点和不同点，说明映射与权限检查、地址空间布局、段调整、页帧管理和辅助函数组织的处理，以及采用相关实现方式的原因；不能只列出代码文本的差异。

4. **主要问题与解决思路**：总结实验中遇到的主要问题，描述问题现象、原因分析、排查过程、解决思路及处理结果，结合代码、GDB 跟踪记录或运行输出说明依据。尚未解决的问题也应如实记录。

实验报告采用 Markdown（`.md`）格式，统一保存到仓库根目录下的 `reports/lab4.md`，至少包含静态分析与动态跟踪、独立实现对比、主要问题与解决思路三部分，并随实验代码提交。GDB 动态跟踪用于参考实现分析，完成后的功能验收仍使用本文规定的运行命令。

## 提供的数据结构

### 地址、页号与物理页帧

description: [address.rs](os/src/mm/address.rs) 提供 `PhysAddr`、`VirtAddr`、`PhysPageNum`、`VirtPageNum` 和页号区间 `VPNRange`。地址表示字节位置，页号表示页的位置。本章页大小为 `PAGE_SIZE`，即 4 KiB，采用 Sv39 地址转换。地址与页号转换、页内偏移、页表索引、区间访问和物理页内存访问均已提供。

[frame_allocator.rs](os/src/mm/frame_allocator.rs) 提供物理页帧分配器和以下类型：

```rust
pub struct FrameTracker {
    pub ppn: PhysPageNum,
}
```

`frame_alloc()` 返回 `Option<FrameTracker>`，成功分配的物理页内容初始为零。`FrameTracker` 持有物理页帧的所有权，其析构负责归还页帧。页表页和用户数据页都需要有与其实际用途一致的所有权归属。

### PTEFlags 与 PageTableEntry

description: `PTEFlags` 描述页表项的有效性、访问权限及其他硬件标志；`PageTableEntry` 保存一个页表项的编码。定义位于 [page_table.rs](os/src/mm/page_table.rs)，编码、解码和标志查询方法均已提供。

```rust
bitflags! {
    pub struct PTEFlags: u8 {
        const V = 1 << 0;
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
        const G = 1 << 5;
        const A = 1 << 6;
        const D = 1 << 7;
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct PageTableEntry {
    pub bits: usize,
}
```

`V` 表示有效，`R`、`W`、`X` 分别表示可读、可写和可执行，`U` 表示用户态可访问。其余标志保持现有定义。本章使用 4 KiB 页映射，最终映射由末级页表项表示。

### PageTable

description: `PageTable` 表示一个 Sv39 页表。`root_ppn` 标识根页表所在的物理页，`frames` 持有该页表拥有的根页表和中间页表页帧。数据页帧由相应的逻辑段管理。

```rust
pub struct PageTable {
    root_ppn: PhysPageNum,
    frames: Vec<FrameTracker>,
}
```

已提供的 `new()` 创建空页表并持有根页帧；`from_token()` 根据已有 token 建立查询视图，其 `frames` 为空，不取得原页表页帧的所有权；`token()` 返回供地址空间切换和用户地址查询使用的 Sv39 页表标识。

### MapType 与 MapPermission

description: `MapType` 约定逻辑段的映射方式；`MapPermission` 约定逻辑段的访问权限。二者定义位于 [memory_set.rs](os/src/mm/memory_set.rs)。

```rust
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MapType {
    Identical,
    Framed,
}

bitflags! {
    pub struct MapPermission: u8 {
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
    }
}
```

`Identical` 表示虚拟页号与物理页号相同的恒等映射，不取得所映射物理内存的分配所有权。`Framed` 表示由逻辑段持有独立分配的数据页帧。`MapPermission` 中的权限位与 `PTEFlags` 的对应权限位保持一致，页表项的有效性由页表管理维护。

### MapArea

description: `MapArea` 描述一个连续的虚拟页号区间及其映射属性。`vpn_range` 是左闭右开的页号范围，`data_frames` 保存 Framed 逻辑段持有的数据页帧，`map_type` 和 `map_perm` 分别表示映射方式与权限。

```rust
pub struct MapArea {
    vpn_range: VPNRange,
    data_frames: BTreeMap<VirtPageNum, FrameTracker>,
    map_type: MapType,
    map_perm: MapPermission,
}
```

逻辑段的范围、权限、数据页帧归属与实际页表映射需要保持一致。`MapArea` 的 `new()`、`map_one()`、`unmap_one()`、`map()`、`unmap()`、`shrink_to()`、`append_to()` 和 `copy_data()` 均已完整提供，可供地址空间管理直接使用。它们依赖 `PageTable::map()`、`unmap()` 和 `translate()` 提供页表操作。

### MemorySet

description: `MemorySet` 表示一个地址空间，包含该空间的页表和逻辑段集合。内核通过 `KERNEL_SPACE` 共享内核地址空间；每个任务的控制块持有自己的用户地址空间。

```rust
pub struct MemorySet {
    page_table: PageTable,
    areas: Vec<MapArea>,
}
```

`page_table` 描述虚拟页到物理页的实际映射，`areas` 描述这些映射中由逻辑段管理的部分。跳板页的映射独立于 `areas`，内核和各应用通过它访问同一份陷阱处理代码。

## os::mm::page_table

description: 该子模块提供页表映射、解除映射和地址查询，并将用户虚拟缓冲区转换为内核可以访问的物理内存切片。需要完成五个接口。`translate_user()` 和 `translated_byte_buffer()` 有 `mm` 外部调用方；`PageTable::map()`、`unmap()` 和 `translate()` 供已提供的 `MapArea`、`MemorySet` 查询方法及 `remap_test()` 使用，也属于固定接口。

### PageTable::map

description: 将一个虚拟页映射到指定物理页，并赋予相应的访问权限。已提供的 `MapArea::map_one()` 使用该接口建立恒等映射或 Framed 映射，将数据页帧管理与页表操作连接起来。

```rust
pub fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, flags: PTEFlags) -> Option<()> {
    todo!("mm::PageTable::map")
}
```

**输入**

`vpn` 是目标虚拟页号，`ppn` 是对应物理页号，`flags` 指定页表项的访问权限。`self` 是需要建立映射的页表，物理数据页由调用方提供。

**输出**

成功建立映射时返回 `Some(())`；虚拟页已有有效映射，或所需页表页帧无法分配时返回 `None`。

**关键约束**

- 建立 4 KiB 的 Sv39 页映射，页表项具有 `V` 和传入的权限标志。
- 中间页表页帧由该 `PageTable` 的 `frames` 持有。
- 数据页帧的所有权由调用方管理，与页表页帧的所有权分开。

### PageTable::unmap

description: 解除指定虚拟页的有效映射，供已提供的 `MapArea::unmap_one()` 使用。逻辑段的数据页帧由 `MapArea` 管理，本接口负责页表中的映射状态。

```rust
pub fn unmap(&mut self, vpn: VirtPageNum) {
    todo!("mm::PageTable::unmap")
}
```

**输入**

`vpn` 是当前具有有效映射的虚拟页号，`self` 是该映射所在的页表。

**输出**

返回 `()`，目标虚拟页不再具有有效映射。

**关键约束**

- 其他虚拟页的映射保持有效。
- 数据页帧的分配与回收由调用方负责。

### PageTable::translate

description: 根据虚拟页号查询末级页表项，为地址空间查询、映射检查和用户地址转换提供共同的页表查询结果。返回值保留页表项原有的物理页号和标志，调用方能够据此判断映射是否有效以及允许哪些访问。

```rust
pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
    todo!("mm::PageTable::translate")
}
```

**输入**

`self` 是当前查询的页表，也可以是通过 `from_token()` 获得的查询视图。`vpn` 是需要查询的虚拟页号。

**输出**

存在可到达的末级页表项时返回其副本；页表路径无法到达末级页表项时返回 `None`。返回 `Some` 本身不表示该项的 `V` 位有效，调用方仍可通过已有的标志查询方法判断其含义。

**关键约束**

- 遵守本章 Sv39 页表布局和 4 KiB 页映射约定。
- 返回存储的页表项内容，保留其物理页号与标志。
- 与已提供的 `MapArea::copy_data()`、`MemorySet::translate()`、`has_mapped_pages()`、`has_unmapped_pages()` 和 `remap_test()` 的调用约定兼容。

### PageTable::translate_user

description: 根据用户虚拟地址和要求的权限，提供可供内核访问的物理字节地址。已完成的 `sys_get_time` 和 `sys_trace` 依赖该接口检查用户访问权限，因此它同时承担地址转换和访问资格判断。

```rust
pub fn translate_user(&self, addr: usize, permission: PTEFlags) -> Option<PhysAddr> {
    todo!("mm::PageTable::translate_user")
}
```

**输入**

`addr` 是用户虚拟字节地址。`permission` 指定本次访问要求的权限，例如读取需要 `R`，写入需要 `W`。`self` 表示该地址所属的用户页表。

**输出**

允许本次访问时返回 `Some(physical_address)`；地址或映射不满足访问要求时返回 `None`。返回的地址精确对应输入字节，而不只是所在物理页的起始地址。

**关键约束**

- 输入地址符合 Sv39 的规范地址表示。
- 映射同时具有 `V`、`U` 和本次要求的全部权限位。
- 转换结果保留输入地址的页内偏移。

### translated_byte_buffer

description: 将用户地址空间中的连续虚拟字节范围表示为一组物理内存切片，供现有系统调用读写用户缓冲区。虚拟地址连续的页面不要求在物理内存中连续，返回值需要保留用户缓冲区的字节顺序。

```rust
pub fn translated_byte_buffer(
    token: usize,
    ptr: *const u8,
    len: usize,
) -> Vec<&'static mut [u8]> {
    todo!("mm::translated_byte_buffer")
}
```

**输入**

`token` 标识用户页表，`ptr` 是用户虚拟缓冲区的首地址，`len` 是字节长度。调用方提供已经映射的范围，且该范围的地址计算不会溢出。

**输出**

返回按虚拟地址顺序排列的可变字节切片。所有切片合计覆盖且仅覆盖 `[ptr, ptr + len)`，切片直接对应原有数据的物理存储。

**关键约束**

- 支持缓冲区首尾的页内偏移以及跨页范围。
- 支持物理页不连续的映射，每个切片都位于其对应的物理页内。
- 返回值不复制用户数据；切片使用期间，对应的物理页帧保持有效。

## os::mm::memory_set

description: 该子模块以逻辑段组织页表映射，为内核启动、应用加载、内核栈分配和动态内存操作提供地址空间管理能力。需要完成 `KERNEL_SPACE` 初始化，以及五个已有外部调用方的方法。

| 固定实现点 | 调用方 | 用途 |
| --- | --- | --- |
| `KERNEL_SPACE` 初始化 | `mm::init()`、任务管理 | 共享内核地址空间 |
| `MemorySet::from_elf()` | `TaskControlBlock::new()` | 创建应用地址空间与初始布局 |
| `MemorySet::insert_framed_area()` | 任务创建、`TaskControlBlock::mmap()` | 分配内核栈或匿名映射 |
| `MemorySet::remove_framed_area()` | `TaskControlBlock::munmap()` | 解除完整逻辑段的映射 |
| `MemorySet::shrink_to()`、`append_to()` | `TaskControlBlock::change_program_brk()` | 支持既有 `sbrk` 路径 |

### KERNEL_SPACE

description: `KERNEL_SPACE` 是内核共享地址空间。其初始化表达式需要产生完整的内核映射，供现有 `mm::init()` 调用已提供的 `activate()` 激活。任务创建使用同一个地址空间分配各任务的内核栈。

```rust
lazy_static! {
    pub static ref KERNEL_SPACE: Arc<UPSafeCell<MemorySet>> = {
        todo!("mm::KERNEL_SPACE")
    };
}
```

**输入**

没有显式参数。输入来自 `memory_set.rs` 中已经声明的链接符号，以及 `MEMORY_END`、`TRAMPOLINE` 等布局常量。链接符号标识内核各段范围、内核结束位置和跳板代码的物理位置。

**输出**

返回 `Arc<UPSafeCell<MemorySet>>`，包含可由现有启动代码激活的内核地址空间。是否为这一过程定义构造函数由学生决定，`new_kernel()` 只是注释中的候选接口。

**关键约束**

| 映射范围 | 映射关系 | 权限 |
| --- | --- | --- |
| `[stext, etext)` | 恒等映射 | RX |
| `[srodata, erodata)` | 恒等映射 | R |
| `[sdata, edata)` | 恒等映射 | RW |
| `[sbss_with_stack, ebss)` | 恒等映射 | RW |
| `[ekernel, MEMORY_END)` | 恒等映射 | RW |
| `TRAMPOLINE` 所在页 | 对应 `strampoline` 所在物理页 | RX |

以上映射均不设置 `U`。内核段和物理内存范围由相应逻辑段描述，跳板页不纳入 `areas`。各任务内核栈由已有任务创建代码通过 `insert_framed_area()` 分配，栈布局由已提供的 `kernel_stack_position()` 确定。

### MemorySet::from_elf

description: 根据一个应用的 ELF 镜像创建独立用户地址空间，同时返回初始用户栈顶和入口地址。任务管理使用这三个返回值初始化任务控制块和陷阱上下文，因此返回值顺序和地址布局都是固定约定。仓库已有 `xmas-elf` 依赖可用于读取 ELF 信息。

```rust
pub fn from_elf(elf_data: &[u8]) -> (Self, usize, usize) {
    todo!("mm::MemorySet::from_elf")
}
```

**输入**

`elf_data` 是加载器提供的有效应用 ELF 镜像，包含可加载段的虚拟地址、文件内容、内存大小、访问权限和应用入口地址。

**输出**

返回 `(memory_set, user_stack_top, entry_point)`。`memory_set` 包含应用运行和陷阱处理所需的映射，`user_stack_top` 是初始用户栈顶，同时作为已有任务代码记录的堆起点，`entry_point` 对应应用的 ELF 入口地址。

**关键约束**

- 可加载段的虚拟地址和权限与 ELF 描述一致，设置 `U`，段中的文件数据完整保留，其余内存内容为零。
- 用户栈位于最高程序段页之后，中间保留一个未映射的保护页。用户栈大小为 `USER_STACK_SIZE`，权限为 RWU。
- 初始堆逻辑段位于用户栈顶，长度为零，权限为 RWU，可供 `append_to()` 和 `shrink_to()` 调整。
- `[TRAP_CONTEXT_BASE, TRAMPOLINE)` 使用独立分配的数据页帧，权限为 RW，不设置 `U`。
- `TRAMPOLINE` 映射到 `strampoline` 所在物理页，权限为 RX，不设置 `U`，不纳入 `areas`。
- 各应用分别持有自己的数据页帧，页表及逻辑段的所有权与映射保持一致。

### MemorySet::insert_framed_area

description: 在地址空间中加入一个由新分配的数据页帧支撑的逻辑段。现有任务创建通过它分配内核栈，`mmap` 通过它建立用户匿名映射，两类调用分别使用各自的权限组合。

```rust
pub fn insert_framed_area(
    &mut self,
    start_va: VirtAddr,
    end_va: VirtAddr,
    permission: MapPermission,
) -> Option<()> {
    todo!("mm::MemorySet::insert_framed_area")
}
```

**输入**

`[start_va, end_va)` 是目标虚拟字节范围，覆盖的页面没有已有映射冲突。`permission` 是新逻辑段的权限，`self` 是目标地址空间。

**输出**

成功建立并记录逻辑段时返回 `Some(())`；所需页帧无法分配时返回 `None`。

**关键约束**

- 逻辑段覆盖 `[start_va.floor(), end_va.ceil())` 内的虚拟页，使用 4 KiB 页映射。
- 新数据页初始内容为零，由该 Framed 逻辑段持有。
- 逻辑段记录的范围和权限与实际页表项一致，满足内核栈和用户匿名映射的既有调用约定。

### MemorySet::remove_framed_area

description: 解除一个完整 Framed 逻辑段的映射，供现有 `munmap` 路径使用。本仓库第四章实现以完整逻辑段为处理单位，输入范围与目标逻辑段范围一致。

```rust
pub fn remove_framed_area(&mut self, start: VirtPageNum, end: VirtPageNum) -> Option<()> {
    todo!("mm::MemorySet::remove_framed_area")
}
```

**输入**

`start` 和 `end` 是待移除逻辑段的虚拟页号范围，分别对应左闭右开区间的起点和终点。`self` 是该逻辑段所属地址空间。

**输出**

成功移除匹配逻辑段时返回 `Some(())`；没有起止页号完全匹配的逻辑段时返回 `None`。

**关键约束**

- 处理单位为完整逻辑段。
- 成功后的页表和 `areas` 不再保留该逻辑段的有效映射及记录，其拥有的数据页帧已释放。
- 其他逻辑段的映射、内容和所有权保持有效。

### MemorySet::shrink_to

description: 缩小指定逻辑段，用于已有 `sbrk` 路径降低程序的堆结束地址。逻辑段起点保持不变，其最终页范围与新的结束地址一致。

```rust
pub fn shrink_to(&mut self, start: VirtAddr, new_end: VirtAddr) -> bool {
    todo!("mm::MemorySet::shrink_to")
}
```

**输入**

`start` 标识目标逻辑段的起始页，即 `start.floor()`。`new_end` 是该逻辑段当前地址范围内的新结束地址。

**输出**

找到目标逻辑段并完成调整时返回 `true`；目标逻辑段不存在时返回 `false`。

**关键约束**

- 调整后的逻辑段终止页号为 `new_end.ceil()`，起始页号保持不变。
- 保留页面的数据与权限保持不变。
- 截去页面不再具有有效映射，逻辑段不再持有这些页面的数据页帧，页表状态和逻辑段记录一致。

### MemorySet::append_to

description: 扩展指定逻辑段，用于已有 `sbrk` 路径增加程序的堆结束地址。新增页面沿用原逻辑段的映射属性和访问权限。

```rust
pub fn append_to(&mut self, start: VirtAddr, new_end: VirtAddr) -> bool {
    todo!("mm::MemorySet::append_to")
}
```

**输入**

`start` 标识目标逻辑段的起始页，即 `start.floor()`。`new_end` 不小于逻辑段当前的结束地址，新增页范围没有已有映射冲突。

**输出**

成功扩展目标逻辑段时返回 `true`；目标逻辑段不存在或所需页帧无法分配时返回 `false`。

**关键约束**

- 调整后的逻辑段终止页号为 `new_end.ceil()`，起始页号保持不变。
- 既有页面的数据、映射和权限保持不变。
- 新增 Framed 数据页初始内容为零，具有原逻辑段的权限，并归该逻辑段持有。

## 内部实现提示

以下函数签名以注释形式放在对应的 `impl` 块中，作为内部实现提示。学生可以按自己的设计选择、调整或省略这些辅助函数。它们不属于必须逐个实现的固定接口。

| 所属类型 | 注释中的候选接口 | 数量 |
| --- | --- | --- |
| `PageTable` | `find_pte_create`、`find_pte` | 2 |
| `MemorySet` | `push`、`map_trampoline`、`new_kernel` | 3 |

学生需要完成固定接口约定的页表操作和地址空间管理，可以直接使用已提供的 `MapArea` 方法。内部辅助函数不必采用上述划分，固定要求是十一处 TODO 的约定，以及数据结构与配套代码之间的一致性。

## 已提供的配套功能

| 代码位置 | 直接提供的内容 |
| --- | --- |
| [mm/address.rs](os/src/mm/address.rs) | 地址和页号操作、页号区间、物理页内容访问 |
| [mm/frame_allocator.rs](os/src/mm/frame_allocator.rs) | 物理页帧分配、清零及回收 |
| [mm/heap_allocator.rs](os/src/mm/heap_allocator.rs) | 内核堆分配器 |
| [mm/mod.rs](os/src/mm/mod.rs) | 模块导出和内存管理初始化 |
| [mm/page_table.rs](os/src/mm/page_table.rs) | 页表项操作、`PageTable::new()`、`from_token()`、`token()` |
| [mm/memory_set.rs](os/src/mm/memory_set.rs) | `MapArea` 的全部八个方法，`MemorySet::new_bare()`、`token()`、`activate()`、`translate()`、两个映射范围查询、内核栈位置计算和 `remap_test()` |
| [task](os/src/task)、[syscall](os/src/syscall) | 完整任务管理和系统调用，包括 `sys_get_time`、`sys_trace`、`mmap`、`munmap`、`sbrk` 及系统调用计数 |
| [trap](os/src/trap)、[timer.rs](os/src/timer.rs) | 陷阱处理和时钟支持 |

`MemorySet::has_mapped_pages()` 和 `has_unmapped_pages()` 已被任务模块直接调用，虽然属于查询辅助方法，其签名和实现仍直接提供。它们与 `MemorySet::translate()` 均依赖待实现的 `PageTable::translate()`。

启动汇编、trap汇编、上下文切换汇编及链接脚本均直接提供。学生的代码实现范围是上述两个 `mm` 源文件，其他源码保持原样。

## 运行与验收

使用仓库现有的 `make run`，从仓库根目录执行：

```bash
cd os
make run CHAPTER=4 BASE=2
```

`CHAPTER=4` 显式指定第四章，适用于带有后缀的 `ch4-api` 分支名。`BASE=2` 使用已有第二至第四章的测试应用。验收以这些应用的运行结果和原有断言为准，包括地址空间权限检查、用户缓冲区访问、`mmap`、`munmap`、`sbrk` 和 `sys_trace`。

未完成的骨架会在启动时触发 `KERNEL_SPACE` 的 TODO。能够编译并运行到该位置仅说明骨架可以构建；完成实验后的代码应继续通过已有 `remap_test()` 并运行现有应用。
