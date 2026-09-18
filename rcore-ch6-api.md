# rCore ch6 API 实验：文件、目录与硬链接管理

## 实验内容

本实验基于 `ch6-api` 分支，要求同学借助 AI 完成第六章的文件管理模块。完成后的模块需要支持目录项查询、文件创建、读写与截断、硬链接和元数据查询，并将磁盘上的文件包装为内核可以使用的文件对象。

实验分为两层：`easy-fs::Inode` 管理目录项、磁盘 inode、文件内容及资源回收；`os::fs::OSInode` 管理一次打开操作的访问权限、读写偏移和用户缓冲区。系统调用负责文件描述符检查、用户地址转换和结果写回，相关代码已完整提供。

学生只修改下表中的两个文件，共完成十四处 TODO。各接口的名称、可见性、参数和返回类型保持原样；保留已有数据结构和配套函数的语义，可以在这两个文件中自行设计内部辅助函数。

| 实现文件 | 待实现接口 | 数量 |
| --- | --- | --- |
| [easy-fs/src/vfs.rs](easy-fs/src/vfs.rs) | `Inode::find`、`create`、`ls`、`read_at`、`write_at`、`clear`、`stat`、`link`、`unlink` | 9 |
| [os/src/fs/inode.rs](os/src/fs/inode.rs) | `open_file`、`OSInode::read_all`、`File for OSInode` 中的 `read`、`write`、`stat` | 5 |

本文沿用前几章 API 实验按模块和接口组织 `description`、签名、输入、输出与关键约束的形式。以下契约以本分支代码为准，不要求新增公共接口或实现完整的 POSIX 文件系统语义。

## 提供的数据结构

### Inode 与磁盘 inode

description: [vfs.rs](easy-fs/src/vfs.rs) 中的 `Inode` 是磁盘 inode 的访问句柄，保存其所在块及块内偏移，并共享文件系统与块设备。文件内容、大小、类型和硬链接数保存在磁盘 inode 中。

```rust
pub struct Inode {
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}
```

[layout.rs](easy-fs/src/layout.rs) 已提供 `DiskInode`、`DiskInodeType` 和 `DirEntry`。本分支磁盘布局固定为：

| 项目 | 约定 |
| --- | --- |
| 磁盘块 | 512 字节 |
| `DiskInode` | 128 字节，包含大小、`nlink`、类型和块索引 |
| 块索引 | 27 个直接索引，以及一级、二级间接索引 |
| 目录项 | `DIRENT_SZ = 32` 字节，保存文件名与 inode 编号 |
| 文件名 | UTF-8 编码最多 27 字节；长度按字节计算 |
| 根目录 | inode 编号为 `0` |
| 已删除目录项 | 使用 `DirEntry::empty()`，其 inode 编号为 `0` |

本实验使用根目录下的平面文件名，不实现路径遍历和子目录创建。目录项的 inode 编号为 `0` 时表示空槽，不表示指向根目录的有效名称。查找和列举应跳过这些空槽，插入目录项时复用它们。

多个目录项可以引用同一个磁盘 inode。`DiskInode::nlink` 记录该 inode 的硬链接数，与内存中的 `Arc` 强引用数量没有对应关系。同一文件经不同名称查找得到的 `Inode` 句柄，也必须访问相同的磁盘 inode 和文件数据。

### OSInode 与 OSInodeInner

description: [os/src/fs/inode.rs](os/src/fs/inode.rs) 中的 `OSInode` 表示一次打开操作。访问权限属于打开对象，偏移和底层 inode 由 `UPSafeCell` 管理。

```rust
pub struct OSInode {
    readable: bool,
    writable: bool,
    inner: UPSafeCell<OSInodeInner>,
}

pub struct OSInodeInner {
    offset: usize,
    inode: Arc<Inode>,
}
```

已提供的 `OSInode::new` 将偏移初始化为 `0`。分别打开同一文件会得到不同的 `OSInode`，各自维护偏移；`fork` 复制文件描述符表中的 `Arc<dyn File>`，父子进程因而共享对应打开对象的偏移。不能将该偏移保存在 `easy-fs::Inode` 或磁盘 inode 中。

`ROOT_INODE` 的初始化已提供，它打开块设备上的文件系统并取得根目录。`open_file`、`link_file`、`unlink_file` 都通过该根目录处理文件名。

### File、UserBuffer 与 Stat

description: [os/src/fs/mod.rs](os/src/fs/mod.rs) 定义统一的 `File` trait，文件描述符表保存 `Arc<dyn File>`。标准输入输出已有实现，本实验只补全 `OSInode` 的三个待实现方法。

```rust
pub trait File: Send + Sync {
    fn readable(&self) -> bool;
    fn writable(&self) -> bool;
    fn read(&self, buf: UserBuffer) -> usize;
    fn write(&self, buf: UserBuffer) -> usize;
    fn stat(&self) -> Option<Stat>;
}
```

上面的签名用于说明接口；代码中 `File::stat` 的默认实现返回 `None`，供没有文件系统元数据的对象使用。`OSInode` 应提供自己的实现。

`UserBuffer` 由已提供的用户地址转换代码生成，内部的 `buffers` 按用户虚拟地址顺序保存多个可访问的字节切片。用户缓冲区跨页时，物理地址不一定连续；文件读写必须按切片顺序处理。

`Stat` 是系统调用使用的元数据结构，字段与布局已固定：

```rust
#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    pub dev: u64,
    pub ino: u64,
    pub mode: StatMode,
    pub nlink: u32,
    pad: [u64; 7],
}
```

`StatMode::DIR` 表示目录，`StatMode::FILE` 表示普通文件。这里的 `mode` 表示文件类型，不用于描述 `OSInode` 的读写权限。

## easy_fs::vfs

description: 本模块对磁盘 inode 和目录项提供文件级操作，协调文件系统锁、块缓存、空间分配与资源回收。以下九个方法是固定接口；它们不维护打开文件的偏移，也不访问用户虚拟地址。

### Inode::find

description: 在当前目录中按名称查找有效目录项，返回其对应的 inode 句柄。

```rust
pub fn find(&self, name: &str) -> Option<Arc<Inode>>;
```

**输入**

`self` 是目录 inode；`name` 是有效的单分量文件名，UTF-8 编码不超过 27 字节。

**输出**

存在对应目录项时返回 `Some(Arc<Inode>)`，否则返回 `None`。

**关键约束**

- 跳过已删除的空目录项，只匹配有效名称。
- 返回句柄指向目录项记录的磁盘 inode，并共享原文件系统和块设备。
- 不分配新的磁盘 inode，不复制文件内容，也不改变硬链接数。
- 查找过程不改变目录、文件元数据或内容。

### Inode::create

description: 在当前目录中创建一个空的普通文件，并建立指向它的目录项。

```rust
pub fn create(&self, name: &str) -> Option<Arc<Inode>>;
```

**输入**

`self` 是目录 inode；`name` 是有效的单分量文件名，UTF-8 编码不超过 27 字节。调用时有足够的 inode 和数据块空间。

**输出**

成功返回新文件的 `Some(Arc<Inode>)`。有效目录项中已有同名文件时返回 `None`，不修改该文件或目录。

**关键约束**

- 新文件拥有新分配的 inode 编号，类型为普通文件、大小为 `0`、硬链接数为 `1`。
- 新目录项引用该 inode；不能为同一名称重复建立有效目录项。
- 优先复用已有空槽，没有空槽时才扩展目录以容纳新目录项。
- 文件系统位图、inode 和目录项的变化必须一致，成功返回前同步块缓存。
- 不将新文件解释为打开对象；读写偏移由 OS 层负责。

### Inode::ls

description: 列举当前目录中的有效文件名，供应用列表等已有调用方使用。

```rust
pub fn ls(&self) -> Vec<String>;
```

**输入**

`self` 是目录 inode，其数据按固定大小的目录项存放。

**输出**

按目录项在目录中的顺序返回所有有效名称。没有有效目录项时返回空向量。

**关键约束**

- 跳过 inode 编号为 `0` 的空槽，不将空名称加入结果。
- 硬链接是独立名称，指向相同 inode 的不同有效目录项都应列出。
- 不排序、压缩目录或修改任何磁盘内容。

### Inode::read_at

description: 从给定文件偏移读取数据，将实际可读部分填入缓冲区。

```rust
pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize;
```

**输入**

`self` 是有效 inode，`offset` 是字节偏移，`buf` 是可写缓冲区。参数满足本分支底层读写接口的有效数值范围。

**输出**

返回实际读取的字节数，不超过 `buf.len()` 和从 `offset` 到文件末尾的可用字节数。偏移达到或超过文件末尾时返回 `0`。

**关键约束**

- 返回长度范围内的数据与文件内容一致，缓冲区其余部分不被写入。
- 支持跨磁盘块读取，通过已有磁盘 inode 和块缓存接口访问内容。
- 不修改文件大小、硬链接数或文件数据，也不保存下一次读取偏移。

### Inode::write_at

description: 从给定偏移写入缓冲区内容，按需要扩展文件并同步缓存。

```rust
pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize;
```

**输入**

`self` 是有效 inode；`offset` 是字节偏移，`buf` 是待写入字节。`offset + buf.len()` 不溢出，目标大小在现有磁盘布局的容量范围内，并且空间足够。

**输出**

返回实际写入长度。在上述前提下完整写入 `buf`，返回 `buf.len()`。

**关键约束**

- 写入完成后，相应字节范围与 `buf` 相同；覆盖文件内部数据时不缩短文件。
- 需要扩展时正确维护文件大小以及数据块和间接索引块，不破坏已有范围内未被覆盖的内容。
- 所有硬链接指向同一份文件内容，其他名称访问时应观察到本次写入。
- 成功返回前同步块缓存；不保存打开对象的偏移。
- 不增加磁盘空间不足的事务处理或稀疏文件支持要求，沿用已有分配和布局能力。

### Inode::clear

description: 将文件截断为空文件，回收其内容占用的空间，保留文件身份和名称。

```rust
pub fn clear(&self);
```

**输入**

`self` 是要截断的有效文件 inode。已有 `open_file` 在相应打开标志下调用此方法。

**输出**

返回 `()`。文件大小变为 `0`，后续从文件起始位置读取返回 `0`。

**关键约束**

- 回收全部数据块及一级、二级间接索引所占的块，不能遗漏或重复释放。
- 重置文件内容的块索引，允许该 inode 后续重新写入和扩展。
- 保留 inode 编号、类型、硬链接数及所有目录项，不释放 inode 位图中的编号。
- 对一个硬链接执行截断后，其他链接看到的也是同一个空文件。
- 成功返回前同步块缓存；已有空文件可以再次清空。

### Inode::stat

description: 查询当前 inode 的身份、最新硬链接数和类型，供 OS 层构造 `Stat`。

```rust
pub fn stat(&self) -> (u32, u32, bool);
```

**输入**

`self` 是有效 inode 句柄。

**输出**

返回 `(inode_id, nlink, is_dir)`；`is_dir` 为 `true` 表示目录，为 `false` 表示普通文件。

**关键约束**

- inode 编号来自该磁盘 inode 在文件系统中的位置，不能使用内存地址或新分配的编号。
- `nlink` 来自磁盘 inode 的最新字段，不能使用 `Arc::strong_count`，也不能缓存为创建时的值。
- 同一 inode 的不同硬链接查询到相同编号和链接数。
- 查询不改变文件系统内容。

### Inode::link

description: 为当前目录中的普通文件增加一个名称，使新旧目录项共享同一 inode。

```rust
pub fn link(&self, old_name: &str, new_name: &str) -> Option<()>;
```

**输入**

`old_name` 是现有名称，`new_name` 是请求增加的名称；名称的 UTF-8 编码长度不超过 27 字节。有效操作有足够的目录扩容空间。

**输出**

成功返回 `Some(())`。以下情况返回 `None`，不修改目录、文件内容或链接数：

- `self` 不是目录，或者旧名称不存在。
- 新名称已存在，包括与旧名称相同的情况。
- 新名称为空、包含 NUL 字节或 `/`，或者等于 `.`、`..`。
- 目标不是普通文件，或者硬链接数加一会溢出。

**关键约束**

- 新目录项引用原 inode，原文件 `nlink` 恰好增加一次。
- 不分配新文件 inode，不复制文件数据，不改变原文件大小和内容。
- 插入新名称时复用空目录项，必要时扩展目录。
- 检查条件与目录项、链接数更新受同一文件系统锁保护，避免状态不一致。
- 成功返回前同步块缓存。

### Inode::unlink

description: 删除当前目录中的一个文件名称，减少硬链接数，并在最后一个名称删除后回收文件资源。

```rust
pub fn unlink(&self, name: &str) -> Option<()>;
```

**输入**

`name` 是待删除的单分量名称。调用方遵守本实验的资源回收约定：最后一个硬链接删除后，不再通过旧的打开对象或 inode 句柄访问该文件。

**输出**

成功返回 `Some(())`。当前 inode 不是目录、名称不存在、目标不是普通文件或链接数无法合法减一时返回 `None`，不修改状态。

**关键约束**

- 将目标目录项变为空槽，后续 `find` 和 `ls` 不再找到该名称，后续插入可以复用该位置。
- 原 inode 的 `nlink` 恰好减少一次。仍有硬链接时保留 inode、文件内容及全部数据块。
- 链接数变为 `0` 时，回收全部数据块和间接索引块，并归还 inode 位图中的编号。
- 不缩短或重排目录中的其他目录项，不破坏其他文件及其他硬链接。
- 成功返回前同步块缓存。
- 本实验沿用最后一个链接删除后立即回收的策略，不增加“文件仍打开时延迟删除”的机制；不能使用 `Arc` 引用数代替硬链接数决定回收。

## os::fs::inode

description: 本模块将 `easy-fs::Inode` 包装为内核文件对象，处理打开标志、读写偏移和分片缓冲区。以下五个接口为 TODO；根目录初始化、对象构造、权限查询和硬链接包装函数已提供。

### open_file

description: 在根目录中按名称打开文件，依据标志创建或截断，并返回一个新的打开对象。

```rust
pub fn open_file(name: &str, flags: OpenFlags) -> Option<Arc<OSInode>>;
```

**输入**

`name` 是有效的单分量文件名，UTF-8 编码不超过 27 字节。`flags` 使用已有 `OpenFlags`，权限解释由已提供的 `read_write()` 决定。

**输出**

成功返回 `Some(Arc<OSInode>)`，新打开对象的偏移为 `0`。没有 `CREATE` 且文件不存在，或创建失败时返回 `None`。

**关键约束**

| 条件 | 本分支要求的行为 |
| --- | --- |
| 有 `CREATE`，文件不存在 | 创建空文件并打开 |
| 有 `CREATE`，文件已存在 | 清空已有文件并打开，即使没有 `TRUNC` |
| 无 `CREATE`，文件已存在且有 `TRUNC` | 清空已有文件并打开 |
| 无 `CREATE`，文件已存在且无 `TRUNC` | 保留内容并打开 |
| 无 `CREATE`，文件不存在 | 返回 `None` |

- 清空已有文件保留其 inode 编号和硬链接关系。
- 每次成功调用产生独立的 `OSInode`，不复用其他打开对象的偏移。
- 直接使用 `OpenFlags::read_write()`：标志为空时只读，包含 `WRONLY` 时只写，其余情况可读写。不得自行替换为其他权限解释。
- 不分配文件描述符，不访问用户指针；这些工作由已有系统调用完成。

### OSInode::read_all

description: 从当前偏移读取文件剩余内容，供 `exec`、`spawn` 和初始进程加载等已有路径取得 ELF 数据。

```rust
pub fn read_all(&self) -> Vec<u8>;
```

**输入**

`self` 是可供调用方读取的有效打开对象，内部记录底层 inode 和当前偏移。

**输出**

按文件顺序返回当前偏移至 EOF 的全部字节，并按实际读取长度推进偏移。已经位于 EOF 时返回空向量。

**关键约束**

- 从现有偏移开始，不在接口内部无条件归零，也不在读取结束后恢复旧偏移。
- 结果仅包含实际文件数据，不包含读取缓冲区中超出实际长度的内容。
- 不修改文件内容、大小或硬链接数。
- 程序加载使用新打开且偏移为 `0` 的对象，因此能够取得完整 ELF。

### File for OSInode::read

description: 将文件数据读入按用户地址顺序排列的多个缓冲区切片，并更新该打开对象的偏移。

```rust
fn read(&self, buf: UserBuffer) -> usize;
```

**输入**

`buf` 是已由 syscall 转换的用户缓冲区；调用方已检查文件描述符有效且可读。

**输出**

返回所有切片实际读入字节数之和，并将打开对象的偏移增加该长度。到达 EOF 后返回实际读取量，后续读取返回 `0`。

**关键约束**

- 按 `buf.buffers` 的顺序连续读取文件，不能假设切片的物理地址连续。
- 每次读取使用当前文件偏移，偏移只按实际读取长度推进，不能按请求长度推进。
- 不向实际读取范围外写入数据，不修改文件内容。
- 同一打开对象被多个文件描述符共享时，通过同一 `OSInodeInner` 观察更新后的偏移。

### File for OSInode::write

description: 按切片顺序将用户缓冲区写入文件，并更新该打开对象的偏移。

```rust
fn write(&self, buf: UserBuffer) -> usize;
```

**输入**

`buf` 是已转换的用户缓冲区；调用方已检查文件描述符有效且可写。写入满足底层 `Inode::write_at` 的容量和空间前提。

**输出**

返回写入字节总数，将打开对象偏移增加同样的长度。在本实验前提下完整写入各切片。

**关键约束**

- 按切片顺序连续写入，不能将每个切片都写到同一个初始偏移。
- 保留本分支对完整写入的要求；底层未写完请求切片不能被当作成功完成。
- 文件扩容与缓存同步由底层 `Inode::write_at` 负责，OS 层不直接操作块设备或分配磁盘块。
- 共享打开对象的后续读写使用更新后的偏移。

### File for OSInode::stat

description: 将底层 inode 的最新元数据转换为系统调用使用的 `Stat`。

```rust
fn stat(&self) -> Option<Stat>;
```

**输入**

`self` 是有效打开对象，其 inode 尚未按最后一次 `unlink` 的约定回收。

**输出**

返回 `Some(Stat)`：`dev = 0`，`ino` 为底层 inode 编号，`nlink` 为最新硬链接数；目录使用 `StatMode::DIR`，普通文件使用 `StatMode::FILE`，`pad = [0; 7]`。

**关键约束**

- 每次查询读取底层最新元数据，已有打开对象能观察后续 `link` 或未删除最后一个链接的 `unlink` 引起的链接数变化。
- 不改变文件内容或打开对象的偏移。
- 用户缓冲区检查和 `Stat` 写回由已有 `sys_fstat` 负责，本接口不接受用户指针。

## 已提供的配套功能与共同约束

以下实现完整提供，学生保持其数据结构、语义和调用关系，实现范围仍限于开头列出的两个文件。

| 代码位置 | 已提供内容 |
| --- | --- |
| [easy-fs/src/vfs.rs](easy-fs/src/vfs.rs) | `Inode` 结构和 `new`，磁盘 inode 访问、目录项查询与空槽复用、扩容和数据块释放辅助函数 |
| [easy-fs/src/efs.rs](easy-fs/src/efs.rs) | 文件系统格式化、挂载、根目录查询、inode 编号与位置转换、inode 和数据块分配回收 |
| [easy-fs/src/layout.rs](easy-fs/src/layout.rs) | 磁盘布局、目录项、inode 读写、索引块管理和大小变更 |
| [easy-fs/src/bitmap.rs](easy-fs/src/bitmap.rs)、[block_cache.rs](easy-fs/src/block_cache.rs) | 位图、块缓存及 `block_cache_sync_all` |
| [easy-fs-fuse](easy-fs-fuse) | 在宿主机上构建和填充文件系统镜像的工具 |
| [os/src/fs/inode.rs](os/src/fs/inode.rs) | `OSInode` 结构与构造器、`ROOT_INODE`、`OpenFlags::read_write`、权限查询、`list_apps`、`link_file` 和 `unlink_file` |
| [os/src/fs/mod.rs](os/src/fs/mod.rs)、[stdio.rs](os/src/fs/stdio.rs) | `File`、`Stat`、`StatMode`、模块导出及标准输入输出 |
| [os/src/drivers](os/src/drivers) | 块设备驱动和设备初始化 |
| [os/src/syscall](os/src/syscall) | 文件描述符、权限检查、用户地址转换、文件与进程相关系统调用 |
| [os/src/task](os/src/task)、[os/src/mm](os/src/mm) | 进程管理、文件描述符表、地址空间及已迁移的往章功能 |

`Inode` 中已有的私有辅助函数包括 `read_disk_inode`、`modify_disk_inode`、`find_inode_id`、`find_dirent`、`increase_size`、`append_dirent`、`clear_inode_data`。它们已经处理对应的底层操作，可以按现有契约复用；不要求重新实现磁盘布局和块索引算法。

文件系统使用 `spin::Mutex`，不能重入。同一操作已持有 `self.fs` 的锁时，不得再调用会获取同一锁的公共接口。目录 inode 和目标文件 inode 可能位于同一个缓存块，访问二者时应避免重叠持有同一块的缓存锁。同步全部缓存前释放已持有的块缓存锁，防止同步路径再次锁定相同块。

`OSInodeInner` 使用 `UPSafeCell` 的动态独占借用。同一对象不能发生重叠的独占借用；一个打开对象的 inode 访问和偏移更新需要保持一致。内存引用、磁盘 inode 编号、硬链接数和文件描述符分别具有不同的生命周期，不能相互替代。

`clear` 与 `unlink` 的资源归属不同：`clear` 只清空文件内容；最后一次 `unlink` 还归还 inode 编号。`close` 由已有 syscall 移除文件描述符持有的引用，不删除目录项。本实验明确约定最后一个硬链接删除后不继续通过旧句柄访问，不扩展为打开文件的延迟回收机制。

上一章的进程、调度和地址空间功能已作为配套代码提供。`spawn` 和 `exec` 通过文件系统读取 ELF，`fork` 共享已有打开对象的 `Arc`；学生无需重新实现这些调用路径，也不要求维护 `sys_trace`。

## 运行与验收

在已有运行环境和 `user` 测试目录准备好后，从仓库根目录执行：

```bash
cd os
make run BASE=2
```

本分支的 Makefile 支持从 `ch6-api` 分支名提取章节号。`BASE=2` 准备已有第二至第六章的基础与编程测试应用，随后生成文件系统镜像并启动内核。进入用户终端后输入：

```text
ch6_usertest
```

验收只使用这一已有测试入口，不新增验证工作。实现完成后，内核应启动到用户终端，现有测例应完成文件打开、读写、创建、截断、硬链接和元数据检查，以及所调用的往章功能。

结果以各子测例的实际输出、原有断言和退出码为准。总测例中 `ch4b_sbrk` 会主动访问已回收的页，`ch4_mmap1`、`ch4_mmap2` 也包含预期访问异常，这三个顶层子测例应以 `-2` 退出；其余顶层子测例应成功退出，退出码为 `0`。进程创建必须返回有效 PID，等待返回的 PID 应与目标进程一致。

现有 `ch6_usertest` 只检查等待返回值与创建返回值相等，没有独立检查 PID 有效性，也没有断言所有预期退出码。因此，最后的 `ch6 Usertests passed!` 标语不能替代子测例的实际执行结果。
