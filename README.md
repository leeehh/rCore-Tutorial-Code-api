# rCore-Tutorial-Code

## ch3-api 实验

本分支提供第三章的接口实验骨架。学生借助 AI，根据给定的数据定义与函数契约，
完成任务管理和时钟功能。函数注释只描述输入、输出和关键约束，内部实现方式由学生决定。

| 文件 | 提供内容 | 待实现内容 |
| --- | --- | --- |
| `os/src/task/task.rs` | 任务控制块与任务状态定义 | 无 |
| `os/src/task/context.rs` | 上下文布局与汇编入口声明 | 2 个上下文构造函数 |
| `os/src/task/mod.rs` | 管理器类型、全局实例与接口定义 | 9 个初始化及任务管理函数 |
| `os/src/task/switch.rs` | 汇编切换接口与调用约束 | 无 |
| `os/src/timer.rs` | 时钟常量与接口定义 | 4 个时间读取及中断设置函数 |

共 15 处函数体使用 `todo!()`。任务管理器的初始化从原 `lazy_static!` 块提取为
`TaskManager::new()`。模块对外接口与框架兼容，学生可自行组织内部辅助函数。

`switch.S`、其他汇编代码、链接脚本及配套内核模块均直接提供。
本实验的 Rust 实现范围为 `task` 和 `timer`。

环境配置沿用下文说明。在 `ch3-api` 分支上运行：

```bash
cd os
make run
```

命令沿用第三章现有基础应用，默认 `BASE=1`。当前骨架会在未实现接口处停止；
补全后使用同一命令验收，现有应用应执行完成。原内核以
`All applications completed!` 消息结束全部应用的运行。

接口契约以本仓库第三章代码为依据，阅读材料为
[rCore 实验指导书](https://learningos.cn/rCore-Tutorial-Guide/)；
描述粒度参考
[rCore-Tutorial-v3 API 文档](https://github.com/rcore-os/rCore-Tutorial-v3-api-doc/blob/main/rCore-Tutorial-v3.md)。

## Code

- [Soure Code of labs](https://github.com/LearningOS/rCore-Tutorial-Code)

## Documents

- Concise Manual: [rCore-Tutorial-Guide](https://LearningOS.github.io/rCore-Tutorial-Guide/)

- Detail Book [rCore-Tutorial-Book-v3](https://rcore-os.github.io/rCore-Tutorial-Book-v3/)

## OS API docs of rCore Tutorial Code

- [OS API docs of ch1](https://learningos.github.io/rCore-Tutorial-Code/ch1/os/index.html)
  AND [OS API docs of ch2](https://learningos.github.io/rCore-Tutorial-Code/ch2/os/index.html)
- [OS API docs of ch3](https://learningos.github.io/rCore-Tutorial-Code/ch3/os/index.html)
  AND [OS API docs of ch4](https://learningos.github.io/rCore-Tutorial-Code/ch4/os/index.html)
- [OS API docs of ch5](https://learningos.github.io/rCore-Tutorial-Code/ch5/os/index.html)
  AND [OS API docs of ch6](https://learningos.github.io/rCore-Tutorial-Code/ch6/os/index.html)
- [OS API docs of ch7](https://learningos.github.io/rCore-Tutorial-Code/ch7/os/index.html)
  AND [OS API docs of ch8](https://learningos.github.io/rCore-Tutorial-Code/ch8/os/index.html)
- [OS API docs of ch9](https://learningos.github.io/rCore-Tutorial-Code/ch9/os/index.html)

## Related Resources

- [Learning Resource](https://github.com/LearningOS/rust-based-os-comp2025/blob/main/relatedinfo.md)

## Build & Run

```bash
# setup build&run environment first
$ git clone https://github.com/LearningOS/rCore-Tutorial-Code.git
$ cd rCore-Tutorial-Code
$ git clone https://github.com/LearningOS/rCore-Tutorial-Test.git user
$ git checkout ch$ID
$ cd os
# run OS in ch$ID
$ make run
```

If you want to use docker to build and run, you can use the following command:
```bash
# After clone the `rCore-Tutorial-Test` repository to your local machine, you can use the following command to build and run:
$ make build_docker
$ make docker
```

If you experience network issues when accessing foreign resources such as GitHub in Docker, you can follow the following suggestions according to your stage:

- Docker pull:
  1. use proxy: https://docs.docker.com/reference/cli/docker/image/pull/#proxy-configuration

  2. use available domestic source (self-search)

- Docker build: use proxy https://docs.docker.com/engine/cli/proxy/#build-with-a-proxy-configuration

- Docker run: use proxy option, related operations are similar to `Docker build`, can refer to the relevant materials by yourself


Notice: $ID is from [1-9]

## Grading

```bash
# setup build&run environment first
$ git clone https://github.com/LearningOS/rCore-Tutorial-Code.git
$ cd rCore-Tutorial-Code
$ rm -rf ci-user
$ git clone https://github.com/LearningOS/rCore-Tutorial-Checker.git ci-user
$ git clone https://github.com/LearningOS/rCore-Tutorial-Test.git ci-user/user
$ git checkout ch$ID
# check&grade OS in ch$ID with more tests
$ cd ci-user && make test CHAPTER=$ID
```

Notice: $ID is from [3,4,5,6,8]
