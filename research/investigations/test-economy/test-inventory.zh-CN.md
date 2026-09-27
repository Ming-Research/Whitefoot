# 测试体系清单

本文是 [英文清单](test-inventory.md) 的完整中文版本，保留相同的分类、命令、路径、数字和测量限制。两版共同保留重构前的测量记录。

[重构结果和当前测试地图](redesign.md#delivered-test-map) 描述替代体系。下文的数量、路径和耗时是重构前的历史基线，不是当前命令或 target 清单。

这份清单列出 PR #66 所测测试体系的源码、构建产物、资源和执行过程，供重新设计测试体系使用。它描述的是实现版本 `f3858780`；并不表示现有的 target 划分都有必要，也不表示现有的每一条断言都有价值。所有路径都相对于仓库根目录。保留这份带版本的清单及测量，用于解释重构前后的差异；当前职责与命令以重构后的测试地图为准。

[构建和测试成本报告](build-and-test.md) 保留带版本和日期的测量、耗时归因方法、实现变更及其限制。[test-times.tsv](test-times.tsv) 记录编译器和语料测试的逐次执行。本次分类没有改变构建方式、测试选择、规范或判定结果，也没有为此重跑长时间测量。

## 目录

- [数量和时间分别在统计什么](#数量和时间分别在统计什么)
- [编译器与测试的构建产物](#编译器与测试的构建产物)
- [1. 编译器前端与证明实现](#1-编译器前端与证明实现)
- [2. 后端代码生成与生成程序检查](#2-后端代码生成与生成程序检查)
- [3. 调度、循环拆分与资源耗尽采样](#3-调度循环拆分与资源耗尽采样)
- [4. 命令行工具及其自身的单元测试](#4-命令行工具及其自身的单元测试)
- [5. 源码语料：规范形式、规范符合性与快照](#5-源码语料规范形式规范符合性与快照)
- [6. 真实程序集成测试](#6-真实程序集成测试)
- [7. 直接测试 C 运行时](#7-直接测试-c-运行时)
- [8. 研究模型与编译器验证样例](#8-研究模型与编译器验证样例)
- [9. 基准程序构建与小规模正确性检查](#9-基准程序构建与小规模正确性检查)
- [10. 仓库、构建工具和测试执行器检查](#10-仓库构建工具和测试执行器检查)
- [11. 性能测量流程](#11-性能测量流程)
- [12. 历史工具与其他需显式调用的实验执行器](#12-历史工具与其他需显式调用的实验执行器)
- [本地入口](#本地入口)
- [CI 入口](#ci-入口)
- [重复工作与重新设计时要回答的问题](#重复工作与重新设计时要回答的问题)
- [尚未测量清楚的边界](#尚未测量清楚的边界)

## 数量和时间分别在统计什么

之前的“测试程序”混用了四种不同的对象：

| 对象 | 具体例子 | 数量代表什么 |
|---|---|---|
| Rust 测试用例 | `compiler/tests/programs/wfgrep.rs` 中的一个 `#[test]` 函数 | 一段执行断言的测试过程；可以编译多个程序，也可以反复运行同一个程序 |
| Rust 测试可执行文件 | `compiler/target/gate/deps/programs-<hash>` | 一个进程承载 110 个 Rust 测试用例，并非 110 个 Rust 可执行文件 |
| WF 输入文件 | `tests/programs/wfgrep.wf` | 交给编译器的源码；同一个输入会出现在多个检查中 |
| 被测的原生程序 | 由生成的 LLVM 和 C 运行时目标文件链接得到的临时 `program` | 作为子进程运行，测试观察其输出、退出状态和行为；它不是另一个 Rust 测试执行器 |

被测版本的 **`tests/programs/` 下有 39 个 WF 文件**，其中包括多文件程序的组成部分；`programs` 测试可执行文件里有 **110 个 Rust 用例**。两者并非一一对应。另外两个大型源码集合分别包含 805 个规范符合性文件和 484 个快照文件。

一个典型原生程序测试的阶段边界如下：

```text
构建 Rust 产物：只在对应 Cargo target 缺失或过期时发生
  compiler/src/*.rs + #[test] 函数 -> rustc -> Rust 测试可执行文件

执行 Rust 测试
  开始执行选中的一个 #[test] 函数
    WF 字节 -> whitefoot 编译器函数 -> 已检查程序 / LLVM 文本
    LLVM + 运行时 C/LLVM + 观测代码 -> Clang 和链接器 -> 原生可执行文件
    原生可执行文件 + 输入/文件/环境 -> 子进程 -> 观测结果
    Rust 断言比较观测结果与预期结果
```

外层测试耗时包含内部的 WF 编译、原生程序构建和子进程执行。这些测试通常以 Rust 库的形式调用编译器，不一定启动 `whitefootc` 子进程。研究目录里的 shell 构建规则和平台工作流则经常使用真正的 `whitefootc` 可执行文件。

**全文统一采用以下计时口径：**

- 所有数值都是实际经过的秒数，即墙钟时间，不是 CPU 秒数，也不是测试数量。
- Rust 冷构建数据来自 `277a1844`：使用全新的 Cargo target 目录、两个构建任务，依赖缓存可用且不需要下载。测的是“先构建编译器，再构建测试”，不是在报告版本上重新冷跑。
- 本地套件执行数据来自成功的 `f3858780` gate：Rust 产物已经构建，使用两个测试线程。该次 gate 总耗时 **786.97 s**，其中仍需重新构建 IO 程序。另有完整冷/热 gate 的 1353.25/648.54 s；其构建产物状态见测量报告。
- 逐用例归因和原生命令归因来自同一实现上的单独运行，使用时会注明。不把不完整或失败的尝试当作整个套件成功运行的耗时。
- 父阶段耗时包含子阶段。并发 Cargo 编译单元和并发测试会重叠。两类中列出的同一个共享构建耗时，只能算一次，不能相加两次。
- **“未单独测量”就是没有对应测量，不表示耗时为零。** 即使源码已经读清楚，也不能凭空得出某个阶段的耗时。

## 编译器与测试的构建产物

定义这些产物的文件：[compiler/Cargo.toml](../../../compiler/Cargo.toml)、[compiler/Makefile](../../../compiler/Makefile)。

只有一个 Cargo package：`whitefoot`，没有外部 crate 依赖。普通工具和测试可执行文件属于不同的 Rust target。下面不计入测试执行期间生成的临时原生程序。

```text
compiler/build.rs
  -> 一个构建脚本可执行文件，随后执行它以生成构建元数据

compiler/src/lib.rs 及其模块
  -> 普通编译器库：target/gate/deps/libwhitefoot-<hash>.rlib
  -> 启用测试的库：target/gate/deps/whitefoot-<hash>
       一个可执行文件，包含 1662 个 #[test] 用例

compiler/src/bin/whitefootc.rs
  -> 工具：target/gate/whitefootc
  -> 测试可执行文件：target/gate/deps/whitefootc-<hash>，14 个用例

compiler/src/bin/spec.rs
  -> 工具：target/gate/whitefoot-spec
  -> 测试可执行文件：target/gate/deps/whitefoot_spec-<hash>，9 个用例

compiler/src/bin/grammar_tables/main.rs 及其同级模块
  -> 工具：target/gate/whitefoot-grammar-tables
  -> 测试可执行文件：target/gate/deps/whitefoot_grammar_tables-<hash>，1 个用例

compiler/tests/programs.rs 及 programs/*.rs
  -> target/gate/deps/programs-<hash>，110 个用例
compiler/tests/canonical_corpus.rs
  -> target/gate/deps/canonical_corpus-<hash>，3 个用例
compiler/tests/conformance.rs 及 conformance/*.rs
  -> target/gate/deps/conformance-<hash>，1 个驱动整套语料的用例
compiler/tests/snapshot.rs
  -> target/gate/deps/snapshot-<hash>，1 个驱动整套语料的用例
```

树中的产物路径从 `compiler/` 开始；Windows 工具还带 `.exe` 后缀。总共是 **8 个 Rust 测试可执行文件**、**3 个普通工具**、普通编译器库，以及 Cargo 构建脚本产物。这不等于独立构建了 8 份普通编译器库。

在 `compiler/` 下，两个分开计时的构建命令是：

```sh
cargo build --profile gate --bin whitefootc --locked --offline
cargo test --profile gate --all-targets --no-run --locked --offline
```

带资源保护的等价入口是 `make -C compiler build` 和 `make -C compiler test-build`。后续分类中的命令描述底层构建过程；有边界限制的本地调用方式以 [验证入口](../../../README.md#verification) 为准。

| 操作 | Gate 构建 | Dev/默认 test 构建 |
|---|---:|---:|
| 构建 `whitefootc`，包含它依赖的普通编译器库 | 43.34 | 15.43 |
| 然后构建全部测试 target 及剩余普通工具 | 76.79 | 26.73 |
| 顺序执行的总计，未运行任何测试 | 120.13 | 42.16 |

普通库的 Cargo 编译单元在 gate 下耗时 41.93 s，在 dev 下耗时 14.19 s；随后的 CLI 编译单元为 0.72/0.45 s。`build.rs` 的编译/执行在 gate 下为 0.22/0.32 s，在 dev 下为 0.29/0.46 s；其余差额是命令开销。启用测试的库是另一个编译单元，耗时 **69.36/23.38 s，已包含在第二个命令的耗时内**。其他 target 的编译单元耗时列在对应分类中；两个 Cargo 构建任务会使这些时间发生重叠。

`gate` 继承经过优化的 Rust release 代码生成，同时保留调试断言和溢出检查。`dev` 则构建未优化的 Rust 编译器。它们不是 WF 源码的优化开关。同一次 wfgrep LLVM 生成操作，使用已构建的 gate `whitefootc` 耗时 39.49 s，使用已构建的 dev `whitefootc` 耗时 535.38 s；两次都没有执行 wfgrep，输出逐字节相同。Rust 冷构建中具体由哪些 rustc 优化 pass 占据主要时间，尚未测量。

## 1. 编译器前端与证明实现

**目的。** 在原生程序执行之前，检查编译器内部表示、判定和诊断：token、语法树、规范形式输出、名字解析、类型、所有权、证明推导和 lowering。

**源码与用例。** 库中共 **1318 个 `#[test]` 用例**，分布如下。测试源码可以是 Rust 中的字节/字符串字面量，也可以通过 `include_bytes!`/`include_str!` 引用 WF 文件。引用的语料和研究文件是测试资源，不会因此产生额外的 Rust target。

| 模块与源码位置 | 用例数 | 检查内容 |
|---|---:|---|
| `compiler/src/source/tests.rs` | 12 | 源码包、路径和源码限制 |
| `compiler/src/lexer/tests/` | 33 | token、非法字节、字面量和位置处理 |
| `compiler/src/syntax/tests.rs`、`syntax/grammar/tests.rs`、`syntax/parser/tests.rs`、`syntax/parser/finalize/tests/` | 88 | 解析、语法分类、语法树最终整理和规范形式输出 |
| `compiler/src/resolution/tests.rs` | 71 | 名字查找、作用域、导入、重复名字和未知名字 |
| `compiler/src/semantic/tests.rs`、`semantic/tests/` | 1042 | 类型、契约、所有权、借用、部分操作的定义域、证明事实与记录 |
| `compiler/src/lowering/tests.rs` | 28 | 从已检查程序到可执行 IR、所有权清理和循环形式 |
| `compiler/src/driver.rs` 及 `driver/` | 38 | 编译流水线、编译选项、诊断/报告和拒绝边界 |
| `compiler/src/prelude.rs` | 1 | 内嵌 prelude 的一致性 |
| `compiler/src/spec.rs` 及 `spec/` | 5 | 内嵌规范身份及摘要算法实现 |

**构建。** `cargo test --profile gate --lib --no-run --locked --offline` 构建唯一的 `whitefoot-<hash>` 测试可执行文件。它还包含第 2、3 类。69.36 s 的冷构建成本由这三类共同承担；过滤执行哪些用例，并不会生成更小的测试可执行文件。

**运行与资源。** 用例在当前进程里调用编译器函数，然后对结果做断言。引用的文件包括 `tests/conformance/cases/`、`tests/programs/`、`research/experiments/compute-bench/programs/` 和特定调查目录里的 WF 测试输入。本类获得编译器判定不需要 Clang，也不需要运行 WF 程序；但整个库测试可执行文件还装有需要原生构建资源的第 2、3 类用例。

**耗时与调用入口。** `make -C compiler test-unit` 一起运行第 1、2 类：**1588 个用例，测试执行 196.09 s**，外层命令 196.13 s。1318 个用例独占的总墙钟时间未单独测量。本地编译器 gate 和根目录 gate 包含该命令；CI 的 `unit` job 也运行它。

**个别昂贵工作。** 在 `semantic/tests/entailment.rs` 中，`frozen_real_sources_retain_complete_proof_roots_without_counted_false_positives` 在逐用例归因测量中耗时 58.881 s：它分析 UTF-8、由四个文件组成的 DEFLATE 源码包，以及 wfgrep，再检查证明根；没有执行这些 WF 程序。`generic_counted_roots_are_deterministic_across_twenty_analyses` 耗时 9.371 s，重复分析二十次以比较规范化的证明记录。现有数据没有证明二十次是最少的有效重复次数。

## 2. 后端代码生成与生成程序检查

**目的。** 检查生成的 LLVM 结构、ABI、资源处理，以及从 WF 生成的机器码行为。有些测试只检查 IR；另一些会编译并运行原生子进程。目前的 `test-unit` 名称掩盖了这个区别。

**源码与用例。** [compiler/src/backend/tests.rs](../../../compiler/src/backend/tests.rs) 和 `compiler/src/backend/tests/` 中有 **270 个 `#[test]` 用例**，不含第 3 类的 `exhaustion.rs`、`loop_split.rs`、`parallel.rs`。例如 `cost_shape.rs`、`slices.rs`、`base64.rs`、`sched.rs`、`completion.rs`、`system_io.rs`。输入包括内联 WF、保留的 WF 语料文件、LLVM 文本、C 观测代码，以及为测试而刻意修改的运行时构建。

**构建。** 与第 1 类共用 `whitefoot-<hash>` 库测试可执行文件；本地不再支付第二次 69.36 s 的库测试冷构建成本。

**运行。** 用例可能调用进程内的编译器、检查生成的 IR、调用 `/usr/bin/clang` 优化和链接、执行原生子进程，再用独立的预期判定逻辑比较字节、退出状态或计数器。另一些用例不输入 WF，而是直接构建 C 运行时探针。辅助代码位于 `backend/tests.rs` 和 [compiler/tests/support/mod.rs](../../../compiler/tests/support/mod.rs)；后者在测试配置下也会被库包含。

**资源与产物。** 需要可执行的临时存储空间、Clang、宿主链接器、检查符号时使用的 `nm`、pthreads，以及 `compiler/src/backend/`、`backend/sched/`、`backend/completion/` 下的平台 C/LLVM 运行时源码。IO 用例还需要真实文件、管道和宿主 IO 设施。每次原生调用会准备模块/观测代码，并在临时目录中链接出一个原生可执行文件。通常的库调用路径不需要启动编译器 CLI。

**复用。** 共享辅助代码按“进程和 C 方言”各编译一次十二个不可变运行时翻译单元，通过 `OnceLock` 保存目标文件字节。默认 C 和显式 C11 是两个独立变体。模块、观测代码和可执行程序仍按用例生成；通过宏替换运行时行为的测试继续独立构建。Rust 测试进程退出后，这种复用就结束，不是跨套件或跨 CI 的持久缓存。

**耗时与调用入口。** 第 1 类加本类在 `test-unit` 下执行 196.09 s；本类独占的套件墙钟时间未测量。两个 wfgrep `cost_shape` 用例各计时 41.738/39.449 s，但通过 `OnceLock` 共享同一次昂贵的代码生成/优化；两个计时都可能包含等待。把它们相加会重复计算共享等待。该次归因测量中，原生 scatter 对照用例为 6.342 s，nominal-data 执行为 4.913 s，Base64 RFC 向量用例为 4.534 s。

## 3. 调度、循环拆分与资源耗尽采样

**目的。** 覆盖单次原生执行无法充分覆盖的运行时策略和交错：worker 数量、并行授权/拒绝、递归返回、清理、栈和资源限制，以及拆分循环的结果等价性。

**源码与用例。** 位于 `compiler/src/backend/tests/` 的三个模块：

| 源码 | `#[test]` 用例数 | 主要观测内容 |
|---|---:|---|
| `exhaustion.rs` | 23 | 资源耗尽分类、栈下限、共享故障记录和清理 |
| `loop_split.rs` | 17 | 拆分和粒度选择、满足结合律的合并、worker 数量及发布的字节 |
| `parallel.rs` | 34 | 授权/join/拒绝、递归对照、所有权、标量及聚合值 ABI |

**构建。** 这 **74 个用例仍在同一个含 1662 个用例的库测试可执行文件中**。`make -C compiler test-sampling` 按这三个模块名选择执行范围，不会构建三个 Rust 测试二进制。但在独立的冷 CI 虚拟机上，仍然需要完整构建整个库测试可执行文件。

**运行与资源。** 内联/生成的 WF 和 C 观测代码 -> 编译器库 -> LLVM -> Clang/链接 -> 在指定 `WF_WORKERS` 和策略下执行原生子进程。有些测试检查符号，或者通过 shell 调整资源限制。需要临时存储、原生线程、Clang/链接器、宿主进程/资源 API，以及共享运行时源码。被刻意停止的原生子进程由该测试的判定逻辑分类，不代表 WF 源码被拒绝。

**耗时与调用入口。** 本地 `compiler check` 和根目录 `make check` 中，**测试执行为 29.23 s**，外层命令为 29.27 s。CI 有独立的 `sampling` job。另一次逐用例测量耗时 30.562 s，这是另一个观测结果。

**为什么用例少，工作量仍然可能很大：**

- `recursive_controls_preserve_scalar_and_destination_results`：12.453 s；2 种递归形式 × 2 种返回形式 × 9 种策略 = **36 个原生程序**；每个按 2 种预算 × 2 种授权结果运行，共 **144 次执行**。
- `an_overlapped_program_reports_one_byte_sequence_at_every_worker_count`：4 种 worker 设置，每种执行 5 次，外加顺序参考程序。
- 拆分循环的字节等价性测试检查 9 种 worker 设置以及不设置时的默认值。有些输入包含 400,000 次循环迭代。
- 为观察到一次授权，循环最多尝试 32 次，观察到后提前结束。一个始终拒绝并行授权的运行时，不能仅凭顺序结果正确就通过测试。

这些是重复执行的具体理由，但不是每个矩阵组合或重复次数都已最小化的证明。它们仍是重新设计时需要回答的问题。

## 4. 命令行工具及其自身的单元测试

**目的与定义位置。** `compiler/Cargo.toml` 中的三个 `[[bin]]` 条目提供三个实际工具。各工具源码里的 `#[test]` 函数又会被编译成三个额外的测试可执行文件。运行 bin-test 时进入的是测试执行器，不是工具正常的 `main`。

### 4.1 编译器命令行接口

- **源码：** `compiler/src/bin/whitefootc.rs`。
- **普通产物：** `compiler/target/gate/whitefootc`，由 `cargo build --profile gate --bin whitefootc --locked --offline` 构建。它读取 WF 输入并调用编译器库；原生输出模式还会调用 Clang/链接器。`--emit-llvm` 在原生构建之前结束。
- **测试产物：** `target/gate/deps/whitefootc-<hash>`，来自 `cargo test --profile gate --bin whitefootc --no-run --locked --offline`。**14 个 `#[test]` 用例**检查选项组合、lowering 选择、源码显示名称、使用说明、运行时单元选择和 include 闭包。
- **资源：** 普通编译器 `.rlib`、内嵌运行时/头文件文本，以及内存中的参数列表。这些单元用例不会运行 14 个 WF 程序。实际 CLI/原生行为由其他位置验证，包括第 8、9 类及 Windows 平台检查。
- **耗时：** 普通库可用后，普通 CLI 编译单元 0.72 s；bin-test 编译单元 1.06 s。gate 的计时精度下，测试执行显示为 0.00 s，不代表没有工作。完整编译器冷构建命令仍是 43.34 s，不能说成 0.72 s。
- **调用入口：** `test-corpus` 执行测试。研究和平台构建规则使用普通工具。两种产物都复用普通编译器库。

### 4.2 规范扫描与身份检查工具

- **源码：** [compiler/src/bin/spec.rs](../../../compiler/src/bin/spec.rs)。
- **普通产物：** `compiler/target/gate/whitefoot-spec`。`cargo run --profile gate --bin whitefoot-spec --locked --offline` 在必要时构建它，然后执行。无参数运行时检查当前规范的派生身份、版本声明、编号规则和引用；`--index`、`--counts` 提供查询功能。
- **测试产物：** `target/gate/deps/whitefoot_spec-<hash>`，由 `cargo test --profile gate --bin whitefoot-spec --no-run --locked --offline` 构建。**9 个 `#[test]` 用例**覆盖身份不匹配、缺少状态、规则 ID 格式、无效引用，以及索引/计数输出的一致性。
- **资源：** 通过编译器库嵌入的 `spec/kernel-spec.md`、派生的构建元数据，以及刻意写坏的规范字符串。不需要 WF 编译、Clang 或原生 WF 程序执行。
- **耗时：** 普通库可用后，普通 bin 编译单元 0.96 s，bin-test 编译单元 1.63 s。测试执行显示为 0.00 s。实际运行检查器的热 `spec` 命令为 0.34 s。独立 CI `static` job 必须先构建自己的普通库：Linux/macOS 上这次构建为 51.13/55.58 s。
- **调用入口：** `test-corpus` 执行九个测试；`compiler check` 和 `compiler static` 也运行真正的检查器来检查当前规范。部分当前规范检查与正向测试断言重叠；单独存在一个 bin，并不能证明它带来了有价值的额外覆盖。

### 4.3 语法表生成器

- **源码：** `compiler/src/bin/grammar_tables/{main,ebnf,model}.rs`；纳入版本控制的输出是 `compiler/src/syntax/grammar/generated.rs`。
- **普通产物：** `compiler/target/gate/whitefoot-grammar-tables`。`cargo build --profile gate --bin whitefoot-grammar-tables --locked --offline` 构建这个维护工具。它的 `main` 读取规范中的语法并输出生成的 Rust 表；也可用 `--output` 写入文件，或用 `--check` 对比。
- **测试产物：** `target/gate/deps/whitefoot_grammar_tables-<hash>`，来自 `cargo test --profile gate --bin whitefoot-grammar-tables --no-run --locked --offline`。唯一的 **1 个 `#[test]` 用例**在内存中调用生成器，把全部输出字节与版本控制中的表比较。
- **资源：** 当前规范文本、已提交的生成表、生成器模块和普通编译器库。不需要 Clang 或 WF 可执行程序。
- **耗时：** 普通工具编译单元 3.31 s；bin-test 编译单元 3.29 s；测试执行 0.32 s。这是两种不同产物，不是把同一个测试执行了两遍。
- **调用入口：** `test-corpus` 运行测试版本。全 target 构建实验也构建普通工具，但 gate 不会调用该工具的 `main`。生成语法表是显式维护操作。

这些工具的单元测试可以放进共享模块或另一个测试 target。保留有价值的测试身份，并不要求保留三个物理测试可执行文件。合并会改变构建依赖、链接和代码生成工作量，实际节省尚未测量。反过来，Cargo 会构建普通语法工具，本身也不构成每条验证路径都必须构建它的理由。

## 5. 源码语料：规范形式、规范符合性与快照

这三个 integration target 对 WF 输入回答不同的问题。目前它们的顶层 `.rs` 文件是独立 Cargo 测试 target；这是组织方式，不是必须使用三个独立操作系统进程的要求。它们都链接普通编译器库。

### 5.1 源码规范形式与规范示例检查

- **执行器：** [compiler/tests/canonical_corpus.rs](../../../compiler/tests/canonical_corpus.rs)，另使用 `compiler/tests/conformance/` 下的 manifest/JSON 读取模块。
- **用例/资源：** **3 个 `#[test]` 函数**扫描 `tests/conformance/cases/` 和 `tests/programs/` 的 `.wf` 文件，查阅 `tests/conformance/manifest.jsonl`，并读取当前规范里的完整示例。
- **构建：** `cargo test --profile gate --test canonical_corpus --no-run --locked --offline` -> `compiler/target/gate/deps/canonical_corpus-<hash>`；普通库构建后，这个 target 的冷编译单元为 **1.93 s**。
- **运行：** 对每份源码做词法分析、解析、语法树最终整理和渲染，比较规范形式字节与渲染幂等性；检查 manifest 只包含允许的排除项，以及规范示例字节完全一致。刻意非法的语料输入按这些排除规则处理。不进行语义证明、Clang 构建或原生程序执行。
- **耗时/调用入口：** `test-corpus` 中执行 **0.72 s**，包含在本地编译器/根目录 gate 和 CI `corpus` 中。它是一个可执行文件，不是每份源码各一个。
- **独立目的：** 程序即使被正确接受，源码渲染器仍可能出错，示例副本也可能过期。本类检查这些性质，不重复完整的规范符合性语义判定。

### 5.2 通过编译器和宿主工具链检查规范符合性

- **执行器：** `compiler/tests/conformance.rs`，配合 `compiler/tests/conformance/{adapter,corpus,json}.rs` 和 `compiler/tests/support/mod.rs` 中的共享原生辅助代码。
- **用例/资源：** `tests/conformance/manifest.jsonl` 与 `tests/conformance/cases/*.wf`。manifest 有 **821 条记录**：16 条规则注释和 805 个用例。预期的接受/拒绝/运行结果来自规范符合性语料；Python 结构检查器属于第 10 类。
- **构建：** `cargo test --profile gate --test conformance --no-run --locked --offline` -> `target/gate/deps/conformance-<hash>`；普通库构建后，冷编译单元 **2.06 s**。里面只有 **1 个 `#[test]` 适配器**，由它遍历 manifest，并非 805 个 Rust 测试函数。
- **运行：** 根目录 `make conformance-run` 用 `--ignored` 调用适配器。普通 `test-corpus` 会构建/选择这个 target，但让唯一的昂贵测试保持 ignored；真正执行它的是根目录的显式命令。适配器对 804 个非 pending 用例调用 `whitefoot::compile`；对其中 330 个 `run` 用例，还按 manifest 给出的文件、参数、stdin 和重定向，链接并执行原生程序。
- **资源：** 编译器库、manifest、WF 源码；Clang/链接器、原生运行时目标文件、可执行的临时目录，以及真实宿主进程和文件系统行为。运行时目标文件在适配器进程内复用；判定结果始终来自当前编译器调用。
- **耗时/结果：** 完整 gate 中，**测试执行 118.75 s**，外层命令 118.79 s。结果为 803 个通过、1 个已跟踪 Xfail、1 个 pending 跳过。Xfail 的预期是 `Reject(OP-4)`，实际到达 `Unsupported`；Unsupported 不等于规范意义上的源码拒绝。
- **单独的阶段测量：** 临时只加计时的适配器耗时 120.54 s：804 次 WF 检查/代码生成 **25.514 s**，330 次原生构建 **23.001 s**，330 次原生启动/等待 **70.567 s**，其余约 1.46 s。这里各阶段串行，可以相加。原生执行墙钟时间包含宿主启动和等待开销，不只是算法的 CPU 工作。
- **调用入口：** 根目录 `make check` 和 CI `conformance`。仅运行 Cargo 中未被 ignored 的套件，不会执行这一适配器。

### 5.3 编译器历史判定快照

- **执行器：** [compiler/tests/snapshot.rs](../../../compiler/tests/snapshot.rs)。
- **用例/资源：** `tests/snapshot/index.tsv`、`tests/snapshot/cases/<family>/*.wf` 和编译器库。**484 行记录，对应 484 个 WF 文件**；一个 `#[test]` 函数遍历整个集合。
- **构建：** `cargo test --profile gate --test snapshot --no-run --locked --offline` -> `target/gate/deps/snapshot-<hash>`；普通库构建后，冷编译单元 **0.42 s**。
- **运行：** 对每份源码调用普通编译器，把语义 `accept`/`reject` 与记录比较；不比较精确的诊断规则编号。更早的语法/源码错误，以及编译器/目标平台失败，不会被悄悄改标成语义拒绝。不调用 Clang，不链接，不执行原生程序。
- **耗时/结果：** **执行 19.17 s**，根目录外层命令 19.21 s；484 个通过，0 个判定翻转。单独计时适配器为 19.06 s，其中逐次读取/编译调用占 19.007 s。最慢用例是二分查找证明，1.245 s；没有执行二分查找程序。
- **调用入口：** 根目录 `make snapshot-run` 传入 `--ignored`；根目录 `make check` 和 CI `conformance` 调用它。普通 `test-corpus` 让它保持 ignored。
- **含义：** 历史判定变化是需要调查的信号，本身不是正确性回归的证明。应由当前规范判断旧行为和新行为谁正确。每一行快照相对于第 1 类和规范符合性测试，到底提供什么独有的缺陷覆盖，尚未逐项建立。

## 6. 真实程序集成测试

**执行器与输入位置。** [compiler/tests/programs.rs](../../../compiler/tests/programs.rs) 包含 `compiler/tests/programs/*.rs`；辅助代码在 `compiler/tests/programs/support.rs` 和 `compiler/tests/support/mod.rs`。WF 源码主要来自 **`tests/programs/` 中的 39 个文件**，包含多文件程序的组成部分；有些测试另外构造源码字符串或修改后的负向对照。输入文件、目录和流由用例创建。

**构建。** `cargo test --profile gate --test programs --no-run --locked --offline` 将普通编译器库链接进**一个** `programs-<hash>` 可执行文件。库可用后，该 target 冷构建 **4.33 s**。其中包含 **110 个 `#[test]` 用例**：

| `compiler/tests/programs/` 下的模块 | 用例数 | 主要对象 |
|---|---:|---|
| `parallel.rs` | 43 | 并行形式、顺序对照、实际并行授权及输出等价性 |
| `wfgrep.rs` | 12 | 搜索输出、行/读取边界、目录/文件行为 |
| `network.rs` | 12 | TCP 客户端/服务器、拒绝、路径选择和并发行为 |
| `runs.rs` | 6 | 固定 run 库的构建、转置、边界和排空 |
| `traversal.rs` | 6 | 目录枚举、嵌套树、链接和拒绝访问的路径 |
| `generics.rs` | 5 | 泛型程序及普通/并行形式 |
| `heap.rs` | 5 | 具有所有权的堆程序和清理行为 |
| `numerics.rs` | 4 | 数值程序结果 |
| `raw_deflate.rs` | 4 | 独立检查 DEFLATE 结果的字节和退出状态 |
| `stream.rs` | 4 | 文件/管道、读取和输出行为 |
| `text.rs` | 3 | 文本与字节处理 |
| `binary.rs`、`hashing.rs`、`image.rs`、`signal.rs`、`support.rs`、`wide_scan.rs` | 各 1 | 对应程序行为，以及辅助代码的精确符号选择器测试 |

**运行。** 用例调用编译器库生成 LLVM，用 Clang、原生运行时和观测目标文件生成一个或多个可执行程序，在不同输入及 worker/IO 策略下运行，再检查字节、退出码、文件系统状态或观测到的并行授权。有些用例只检查编译/IR 或拒绝结果。测试可能通过 `OnceLock` 共享已编译程序，也可能重复运行同一程序；110 个用例和 39 份源码都不意味着 110 次全新构建。

**资源。** Clang/链接器和当前编译器库；ordinary values、floor、scheduler、completion 运行时源码；可执行的临时存储；相关用例需要管道、符号链接、文件权限和按字节处理的 POSIX 名称；网络测试需要 loopback socket 权限；深层遍历需要足够的打开文件数上限。拒绝路径用例要求当前用户确实被权限位拒绝，不能以会绕过权限的特权进程运行。`WF_WORKERS` 和 `WF_IO_NO_NATIVE_RING` 用于选择被断言的场景。

**耗时与调用入口。** 成功的完整 gate 中，全部 110 个用例执行 **241.74 s**。根目录/编译器 gate 和 CI `corpus` 通过 `test-corpus` 调用它们；该命令还运行 24 个 bin 测试和 3 个 canonical 测试，总共 137 个非 ignored 的 Rust 用例。

逐用例归因尝试不是上面的成功套件：它耗时 247.607 s，101 个通过、9 个环境失败，其中 8 个 loopback bind 被拒绝，1 个达到文件描述符限制。开放 loopback 并将描述符上限设为 4096 后，这 9 个用例在单独复查中全部通过，耗时 46.535 s。不能把失败尝试和重试相加后当作正常的 110 用例耗时。

| 单独归因运行中的成功用例 | WF 编译 | 原生构建 | 原生执行 | 整个用例 |
|---|---:|---:|---:|---:|
| `runs::the_fixed_run_library_proves_and_runs` | 90.222 | 0.106 | 0.262 | 90.593 |
| `parallel::corpus_par_fixed_run_library` | 85.890 | 0.114 | 辅助代码未观测到 | 86.038 |
| `parallel::corpus_par_wfgrep` | 85.225 | 0.544 | 0.434 | 86.208 |
| `wfgrep::a_match_across_a_read_boundary_keeps_its_line_number` | 46.178 | 0.236 | 0.224 | 46.643 |
| `parallel::the_default_compilation_of_the_demo_names_no_runtime` | 0.262 | 0.106 | 9.839 | 10.208 |

固定 run 的源码只有 375 行，运行时容量为四个元素：耗时在证明编译，不在庞大的原生运行工作量。最后一行使用 `tests/programs/par_layout.wf`，每次 fold 做 800 次树遍历，每批两次 fold，并有多次程序调用。它较长的原生执行时间有不同原因。逐用例时间区间会并发重叠，不能相加成为整个套件的墙钟时间。

## 7. 直接测试 C 运行时

**目的。** 直接测试原生调度器、completion 和 ordinary values 运行时。C 的 `main`/断言测试程序可以强制形成特定运行时状态并检查链接边界，不必先编译 WF 程序。这些不是 Rust `#[test]` 用例，也不会构建 Rust 编译器。

**定义位置与构建。** [compiler/Makefile](../../../compiler/Makefile) 中的 `completion-test` target 及其前置依赖。下表源码路径从 `compiler/src/backend/` 开始。构建规则使用 `cc -std=c11 -O2`、严格警告、pthreads 和 Makefile 中各探针对应的宏定义；按需链接 scheduler/completion C 单元，在 `$(COMPLETION_TMP)` 下生成原生可执行文件，通常是 `$(WHITEFOOT_SCRATCH_ROOT)/whitefoot-completion-test`。

**资源。** 宿主 C 编译器/链接器、`nm`、原生线程、可执行的临时目录；IO 探针还需要真实文件和 loopback 网络。Linux 原生适配器额外需要 Linux 头文件和可用的 `io_uring`。macOS 无法提供 Linux 内核执行证据。C 输入和脚本化回调在探针源码里，这里没有单独的 WF 语料。

| 探针源码 -> 可执行文件 | 执行和检查什么 |
|---|---|
| `completion/core_read_probe.c` -> `core-read-probe` | 隔离的 completion core/read 协议；真实定位读取的边界和受控逆序完成，用转发钩子统计宿主读取次数；还用 `nm` 检查是否意外引入 bridge 依赖 |
| `completion/bridge_default_probe.c` -> `bridge-default-probe` | 已交付 helper 策略下的文件/TCP 行为；Linux 额外强制测试非 ring 路径 |
| `ordinary_values_probe.c` 加 `ordinary_values.c` -> `ordinary-values-probe` | 文本、范围、文件、目录、TCP、关闭/复用及 credit 转移；分别用 0 和 2 个 helper 运行 |
| `sched/smoke.c` -> `sched-smoke` | 4 个 worker 下的 7 种启动/失败/生命周期模式，随后单 worker 执行 |
| `sched/deque_probe.c` -> `sched-deque-probe` 和 `sched-deque-probe-no-stats` | 两种宏配置的构建，分别启用/禁用统计；各测试 200,000 个任务和 deque 并发复用 |
| `completion/harness.c` -> `harness` | 用 0、1、4 个 helper 运行 completion/原生契约断言，另跑一次禁用缓存策略；检查已移除运行时接口的符号 |
| `completion/pure_compute_probe.c` -> `pure-compute` | 构建可执行文件，用 `nm` 检查禁止出现的 completion 依赖；**不执行这个程序** |
| `completion/native_adapter_probe.c` 加 Linux 运行时单元 -> `linux-native-probe` | Linux 上执行原生 ring 探针，并在可用时执行额外的 required-ring 测试程序；macOS 上只对 `linux_io_uring.c` 做语法检查 |

**耗时。** 普通本地 `completion-test` 命令**总计 5.75 s**。另一次 shell/CC 观测记录了以下子命令耗时；观测器自身的启动使带插桩的整个命令变成 9.08 s。不能把下表加到普通运行的 5.75 s 上，也不能把差值称为运行时性能回归。

| 产物 | C 构建 | 执行或检查 |
|---|---:|---|
| `core-read-probe` | 0.468 | 执行 0.247 |
| `bridge-default-probe` | 0.496 | 执行 0.315 |
| `ordinary-values-probe` | 0.590 | helper 为 0/2 时分别 0.234/0.009 |
| `sched-smoke` | 0.214 | 7 种模式 0.274；单 worker 0.007 |
| 两个 deque 探针 | 0.207/0.207 | 构建与运行混合的 shell 循环总计 1.157；未单独隔离执行耗时 |
| Completion `harness` | 0.683 | helper 为 0/1/4 时：0.346/0.100/0.096；禁用缓存 0.097 |
| `pure-compute` | 0.053 | 符号检查 0.018；没有原生执行 |
| macOS 上的 Linux ring 语法检查 | 0.024 | 不生成可执行文件 |

**调用入口与重复构建。** `compiler check` 和 `compiler static` 都包含 `completion-test`；根目录 `make check` 通过 `compiler check` 调用一次。CI `static` 在 Linux/macOS 上运行它，Linux IO-host job 又在自己的虚拟机上运行一遍。这些规则每次调用都编译 C 探针，没有按单个产物的依赖新旧程度决定是否重建。第 2 类后端 Rust 用例也检查原生运行时行为，但共享源码文件并不能证明具体断言等价。

### 7.1 Linux sanitizer 与必需的平台能力检查

- **定义位置：** `.github/workflows/io-hosts.yml` 的 `completion-linux` job，以及 `compiler/Makefile` 的 `completion-*sanitize`、`completion-*tsan`、`sched-deque-tsan` target。
- **构建/运行：** 用 `-fsanitize=address,undefined` 或 `-fsanitize=thread` 重建相关 C 探针/测试程序；ASan/UBSan completion 规则使用 `-O1 -g`。带 sanitizer 失败选项执行插桩进程。不涉及 Rust 测试或 WF 编译器。
- **资源：** 真实 Linux runner、Clang、已安装的 sanitizer 运行库、线程/文件/socket，以及可用的原生 `io_uring`。该 job 将 ring 不可用视为失败；通用本地探针则可以报告不可用。
- **耗时：** Linux job **总计 43 s**，包含准备阶段。原生 ring 探针 1 s；普通 completion target 8 s；required-ring 执行不到 1 s；ASan/UBSan 构建加运行 7 s；core/default/deque TSan 步骤 5 s；完整 bridge/ring TSan 步骤 3 s。这些 sanitizer 步骤没有单独记录内部构建与运行拆分。
- **覆盖：** Sanitizer 用不同插桩检查原生内存/线程缺陷，不等于再跑一遍未插桩程序。不过该 job 确实也重建、重复了一些 Linux 主 gate 中已有的普通探针；其额外覆盖需要逐条断言审查。

### 7.2 实际 Windows 平台检查

**定义位置。** `.github/workflows/io-hosts.yml` 的 `completion-windows` job。工作流本身就是测试源码的一部分：Bash/PowerShell 准备数据、编译 C/WF、运行子进程并断言结果。它不是在 Windows 上运行那个大型 Rust 库测试可执行文件。

**资源与构建。** 真实 Windows、Clang、Rust/Cargo、PowerShell、Bash、可写临时存储、IOCP、Winsock/loopback 及原生线程。Windows 运行时链接集合在共享单元之外，还使用 `sched/prim_windows.c`、`completion/wait_windows.c`、`completion/file_windows.c`、`completion/windows_iocp.c`、`windows_runtime.c` 和 `wf_floor_windows.c`。宿主链接库包含 `ws2_32`、`shell32`。构建一次 gate profile 的 `whitefootc.exe`，后续 WF 测试复用它；原生产物放在 `RUNNER_TEMP` 下，使用 `.exe` 名称。

| 步骤、源码及生成产物 | 构建/执行观测与整个步骤耗时 |
|---|---|
| `sched/deque_probe.c` -> 开启/关闭统计的探针 | C 构建及原生执行，**9 s** |
| `sched/cpu_levels_probe.c`；`sched/smoke.c` -> CPU/启动探针 | C 构建及真实 Windows 拓扑/启动断言，**1/1 s** |
| `completion/bridge_default_probe.c` -> 默认路径探针 | **构建 3 s + 执行 1 s**，覆盖 IOCP 和 ring 被拒绝时的路径 |
| `completion/native_adapter_probe.c` -> 适配器探针 | **构建 2 s + 执行不到 1 s** |
| `windows_namespace_probe.c` -> 名称空间探针 | **构建 1 s + 执行不到 1 s**，检查原生名称/编码单元行为 |
| `ordinary_values_probe.c` 加运行时单元 | **构建/执行 4 s**；另对所有运行时单元做严格 C 警告/语法检查 **2 s** |
| `completion/windows_bridge_init_fail_stop_probe.c` 加通过宏替换行为的 bridge 目标文件 | **构建 3 s + 执行不到 1 s**；初始化被拒绝必须停止执行 |
| `tests/programs/completion_read_boundary.wf` -> IOCP 程序 | **94 s**，包含 **88 s Rust 编译器构建**和约 6 s WF/原生构建及运行 |
| `tests/programs/{tcp_echo,tcp_refused}.wf` -> 普通/并行 TCP 程序 | **24 s**，使用真实 loopback 对端和两种运行时引擎 |
| `tests/programs/host_string_bytes.wf` -> HostString 程序 | **4 s**，检查原生编码单元/字节边界 |
| `research/experiments/io-completion-bench/programs/windows_component_open.wf` -> 直接/completion 程序 | **8 s**，真实宽字符名称加 ASCII 干扰项；通过预期文件字节区分实际路径 |
| `tests/programs/par_layout.wf` 加 C 授权观测代码 -> worker/对照程序 | **14 s**，实际非 owner worker、授权/窃取、非法配置和输出等价性 |
| 工作流内联生成的 `runaway.wf` -> floor 测试 | **13 s**，使用 Windows floor/运行时检查普通线程的溢出分类 |

整个 job **207 s**，包含 checkout、工具链及其他准备工作。后续 WF 步骤的总时间没有进一步拆分 WF 证明、Clang/链接和执行。交叉编译或 Wine 执行不能替代这些宿主观测。

## 8. 研究模型与编译器验证样例

**入口。** 根目录 `make research-tests`，定义在 [根 Makefile](../../../Makefile)。测得热运行 **15.94 s**。这个标签目前混合了独立 Rust 模型、直接运行的 C 程序、实际 WF 编译，以及 Python/shell 判定工具自身的测试，不是另一个大型 Cargo 单元测试 crate。

### 8.1 显式证明检查成本样例

- **目录/定义位置：** `research/experiments/proof-use-cost/`、`Makefile`、`runner.rs`。
- **用例：** Rust 执行器生成 WF 输入，包含七个应接受的检查，其中一个达到 4096 次 use 的上限，以及两个拒绝对照。这些是执行器定义的检查，不是七个 `#[test]` 函数。
- **构建：** `rustfmt` 检查 `runner.rs`；`rustc --edition=2024 --forbid unsafe_code --deny warnings -C opt-level=2 runner.rs` 生成 `$(WORK_ROOT)/runner`。Cargo 刷新 gate `whitefootc`。
- **运行/资源：** 执行器对临时目录里生成的 WF 文件启动该编译器，检查判定和诊断。需要 Rust、可执行的临时存储和编译器 CLI，不需要原生 WF 工作负载或外部求解器。
- **耗时/调用入口：** 根目录和 CI `research` 中**热运行 1.57 s**。另一次观测将 0.410 s 归于 Rust 执行器构建，0.960 s 归于输入检查。`bench`/`compare` 是独立的计时流程。通过 phony 构建规则，每次 `check` 都会重建执行器。

### 8.2 容器表示实验

**共同定义位置/资源。** `research/experiments/container-representation/Makefile` 刷新 gate `whitefootc`，随后调用下面六个子目录的 `check`。独立模型直接使用 `rustc`，不用 Cargo。C 程序用 `cc`/Clang；WF 程序用编译器 CLI 和它的原生链接路径。`native.mk` 为 dense/families 和 IO 实验共享运行时源码列表，但目标文件仍放在各调用者自己的构建目录，不是全局二进制缓存。最终 gate 中父命令热运行 **13.36 s**；更早冷 gate 的产物状态下，容器阶段为 **142.19 s**。

#### Authority 与成员关系

- **位置：** `container-representation/authority/`，具体为 `model.rs`、`membership.rs`、`Makefile`。
- **构建：** 每个 `.rs` 用 `rustc -C opt-level=2` 编译两遍，一遍生成普通可执行文件，一遍带 `--test`。输出为 `.build/{model,tests,membership,membership-tests}`。
- **运行：** **7 + 2 个 `#[test]` 用例**，以及两个模型的 `main`；枚举 510 个有界存活集合和 31 条身份/代际/生命周期轨迹。`rustfmt` 也检查 membership 源码。子目录自身的检查不需要 WF 或 C 编译器。
- **耗时：** 单独插桩观测中的子 make 热运行 **0.428 s**；构建/运行未分开计时。模型/测试产物按源码依赖缓存。各 Rust 测试套件在本地显示为 0.00 s。

#### Foundation 与构造布局

- **位置：** `container-representation/foundation/`；`model.rs`、`construction.rs`、`rust-baseline.rs`、`layout.c`、`large-result.wf`。
- **构建：** 三份 Rust 源码各用 `-C opt-level=2` 生成 `.build/` 下的普通和 `--test` 可执行文件；`layout.c` 用 C `-O2` 生成 `.build/layout`；WF CLI 通过两条独立规则生成 `large-result.ll` 和原生 `large-result` 程序。
- **运行：** **5 + 1 + 2 个 Rust 测试用例**、三个 Rust 模型、C layout 程序和 WF large-result 程序，检查构造/布局行为；独立的 `measure` target 还生成保留形式/优化后 IR。
- **资源：** Rust、Clang/cc、当前 `whitefootc`、模型常量和本地 WF 源码；无需外部数据集或网络服务。
- **耗时：** 插桩后的子 make 热运行 **1.041 s**，没有阶段拆分。`layout` 自身 **0.323 s**，其中包含 **81 个复制计时样本，总计 0.224 s**，来自各组的九次采样，迭代数为 1200。这里没有性能回归判定，目前把探索性测量混进了 `check`。这些样本不是额外的 81 个正确性测试用例。
- **待审视的重复工作：** `.ll` 和原生程序都缺失时，同一 WF 源码可能触发两次编译器调用。`check` 构建这个 `.ll`，却不运行 `measure` 提供的保留形式/优化后 IR 检查。普通模型与 `--test` 中的断言，也需要在合并产物前比较。

#### 生命周期、所有权和负向对照

- **位置：** `container-representation/lifecycle/`；应接受的源码为 `pool_known_capacity`、`pool_conservation`、`pool_static_capacity`、`pool_boxed_helper`、`pool_boxed_capacity`、`linear_failure_cleanup`、`ring_indexed`，均为 `.wf`；按需使用 `pool_helpers.wf`、`linear_helpers.wf`、`ring_helpers.wf`。
- **构建/运行：** shell 的 `check` 规则无条件调用 `whitefootc --no-overlap ... -o build/<name>`，并运行这**七个**程序。随后以 `--emit-llvm` 模式编译**八个负向输入**，检查退出状态、精确规则/详情，以及没有输出，并写入 `build/outcomes.tsv`。其中一个负向源码是在构建目录中修改 `pool_static_capacity.wf` 得到的。
- **资源：** WF 文件、gate 编译器 CLI、Clang/链接器、普通原生运行时及可写且可执行的存储。没有 Rust `#[test]` 可执行文件。
- **耗时：** 插桩后的子 make 热运行 **12.723 s**，内部编译/运行/负向检查循环 **12.503 s**。未完全拆分 WF 构建与原生执行。七个应接受程序即使没变化也会重建；这是已测出的残余成本，不是 PR #66 已修复的内容。

#### Dense 容器与原生布局对照

- **位置：** `container-representation/dense/`；`driver.rs`、`dense.wf.in`、`inline-view.wf`、`reference.c`、`harness.c`、`Makefile`。
- **构建：** `rustc -C opt-level=2` 生成 `build/driver`；该工具生成尺寸为 16/256/4096 及 16 × 4 lane 形式的 WF，并适配生成的 LLVM。`whitefootc` 生成 LLVM，另外构建 smoke 程序。Clang 生成优化后 IR、汇编、WF/原生目标文件、链接后的 C 比较程序，以及保留调用边界的对照。目标文件来自 `../native.mk`。
- **运行：** Rust 格式检查、WF inline-view 和 smoke 可执行文件，以及用固定种子在 `verify` 模式运行的原生比较程序。前置依赖 `shape` 另外生成供检查的 `optimized*.ll`、`native*.opt.ll`、`native*.s`、`boundary256.ll`；所列 `check` 规则不会自动比较这些文件。也会生成 WF 的 `dense*.s` 汇编，但它会继续用于 `dense*.o`，再链接成 `check` 实际执行的比较程序。本子目录没有 `rustc --test` 可执行文件。
- **资源：** Rust、编译器 CLI、Clang、C 运行时目标文件、生成的 WF 和参考源码；不需要大型外部语料。
- **耗时：** 插桩后的子 make 热运行 **0.675 s**；Rust/WF/C 冷构建未单独测量。Make 前置依赖复用已有产物；显式 `measure` 模式增加重复计时运行，不属于根目录 `make check`。

#### 正确性模式下的 map 布局与稀疏所有权成本

- **位置：** `container-representation/costs/`；`map-layout.c`、`sparse-owned.c`、`rust-map.rs`。
- **构建：** C `-O2` 生成 `.build/map-layout`、`.build/sparse-owned`；`rustc -C opt-level=2` 生成 `.build/rust-map`。该 Makefile 没有选择 `#[test]` 函数，也没有 WF 编译。
- **运行：** `rustfmt`，然后带 `check` 参数运行各程序，对成员关系、payload、分配和所有权观测做断言。计时的 `measure` 命令是独立规则。
- **资源/耗时：** 宿主 Rust/C 工具链和进程内存；插桩后的子 make 热运行 **0.390 s**，未拆分构建与运行。

#### 容器家族、ABI 适配器和分配观测程序

- **位置：** `container-representation/families/` 及其 `Makefile`、本地 `.wf`、`.rs`、`.c` 文件；共享 `../linkage.rs`、`../native.mk`。
- **WF 输入：** `hashmap`、`owning-map`、`owning-growth`、`owning-behavior`、`ordered`、`priority`、`priority-borrowed`、`priority-behavior`、`priority-behavior-direct`、`packed-page`、`growth`、`boxed-migration`、`ordered-runtime-gap`、`boxed-helper-gap`、`shared-option-view`。15 份源码 × `default`/`--par`/`--no-overlap` 三种形式 = **45 个原生程序**。
- **构建：** 当前 `whitefootc` 将这些程序编译到 `.build/`。Rust 的 `abi.rs`、`growth-abi.rs`、`priority-interface.rs`、`owning-growth-abi.rs` 构建适配/检查工具，暴露或转换 LLVM 以供 C 测试程序使用。Clang 将 priority、owning-growth、owning-behavior、owning-map、growth 的观测程序/成本对照与真实运行时链接。保留调用/内联变体和优化后 IR 是额外产物。
- **Rust 测试：** `.build/priority-interface-tests` 包含 **1 个用例**；`.build/owning-growth-abi-tests` 包含 **3 个用例**，其中包括通过 `linkage.rs` 引入的测试。这两条 `rustc --test` 规则没有指定 `-C opt-level`，不是 Cargo gate profile 测试。普通 Rust 适配工具使用 `-C opt-level=2`。
- **运行：** 全部 45 个程序、接口/ABI 测试和检查、分配/增长/行为观测程序、保留调用/内联形式的 `check` 对照，以及 `rejected-wrapper.wf` 负向编译用例。二进制名字中有 `costs`/`bench`，不代表它的 `check` 调用就是计时流程。
- **资源：** Rust、当前 WF CLI、Clang/链接器、C 观测代码、原生运行时目标文件，以及可执行的构建存储。这些检查不需要 oneTBB 等外部调度库。
- **耗时：** PR #66 中，匹配条件下的热检查从 **81.86 -> 1.89 s**：保留已构建 WF 程序，同时仍然运行全部 45 个。单独观测器运行中，子 make 为 3.697 s，已缓存的 45 个程序执行循环为 **0.229 s**；观测器启动增大了外层命令耗时。如今按源码、编译器和 Makefile 依赖控制程序重建。

六个子目录合计包含 **7 个独立 Rust 测试可执行文件、21 个用例**：authority 2 个可执行文件、foundation 3 个、families 2 个。普通模型/适配器是另外的产物。原生 WF/C 程序和模型枚举是额外工作，不是额外的 Rust 用例。上面六个插桩子 make 的热运行时间，不能相加成普通父命令的 13.36 s。

### 8.3 独立判定工具与回归规则自身的测试

| 定义位置与用例 | 构建、执行和资源 | 本地热运行耗时 |
|---|---|---:|
| `research/experiments/ripgrep/test_runner.py`，配合 `runner.py` | `make ... test` 执行 Python `unittest`：**22 个用例**，检查外部执行器/判定工具、测试输入和结果处理。这个 target 不构建 Rust 编译器；需要临时输入存储和 Python。 | 0.16 |
| `research/experiments/raw-deflate-default-shape/test_oracle.py` 及其判定模块 | 直接运行 Python：**19 个用例**，检查独立 DEFLATE 解码/判定行为。本次调用不进行 Cargo 或 WF 构建。 | 0.27 |
| `research/experiments/compute-bench/verdict-test.sh`、`verdict.awk` | **14 个手工构造的表格用例**，用 shell/AWK 断言检查性能回归分类规则。不调用编译器、不构建 kernel 程序，也不做性能计时。 | 最终 gate 未单独计时；另一次观测为 0.118 |

三者都由根目录 `research-tests` 和 CI `research` 调用。测试基准工具的判定规则，是对规则本身的正确性测试；它不同于第 11 类中计时运行计算 kernel。

## 9. 基准程序构建与小规模正确性检查

**入口。** 根目录 `make bench-programs` 先调用 IO 实验的 `programs-check`，再调用 compute 实验的同名 target。在 `f3858780` 上，Rust 产物已存在、IO 程序已失效时，组合命令的本地耗时为 **133.85 s**。那次运行没有分别计时两个子命令，也没有拆分其 WF/原生阶段。CI 在两个平台上都有独立的 `bench-programs` job。

### 9.1 IO 源码构建与参考实现检查

- **定义位置：** `research/experiments/io-completion-bench/Makefile`。
- **WF 资源：** 全部 **12 个** `programs/*.wf`：`many_files_loop`、`many_files_narrow`、`many_files_wide`、`many_files_wide8`、`read_heavy_narrow`、`read_heavy_narrow_4k`、`read_heavy_wide8`、`read_heavy_wide8_4k`、`pipe_relay`、`tcp_echo_server`、`windows_component_open`、`windows_runtime_mixed`。
- **构建：** Cargo 刷新 `compiler/target/gate/whitefootc`，随后 `whitefootc -o $(BUILD)/checked_<name> programs/<name>.wf` 执行证明、LLVM 生成和原生链接。这是 **12 个 WF 原生产物**，不是 12 个 Rust 测试。产物依赖源码、编译器和 Makefile。
- **运行：** 本 target **只构建、不执行**这 12 个 `checked_*` 可执行文件。构建会发现源码/代码生成/链接失败；宿主特有的运行行为由其他调用方检查，例如 Windows IO-host 测试。这个 target 不是完整存储/网络测量流程。
- **小规模原生正确性路径：** `ordinary-check` 通过 `../container-representation/native.mk` 构建 `gen.c`、`ordinary-caller.c`、`ordinary-caller.ll` 和真实运行时。生成数据生成器及 `ordinary-public`/`ordinary-body`，创建一个有四个文件、每文件上限 16 KiB 的目录树，然后以 `check` 模式执行两个 caller。其前置依赖 `ordinary-build` 还会把 `runner.c` 构建成 `runner`，尽管 `ordinary-check` 不执行这个计时执行器。
- **Linux 参考路径：** `uring-check` 将包含参考实现 `uring_echo.c` 的 `uring_echo_check.c` 按 `WF_BENCH_URING_INLINE_SEND=0/1` 构建两遍。两个原生可执行文件**各执行七条确定性轨迹**，断言提交/completion/拒绝行为。需要 Linux 头文件，但不需要真实 ring 或网络服务。Linux CI 日志中，两次构建加十四条轨迹约 **0.582 s**。macOS 不运行这些仅限 Linux 的轨迹。
- **资源/耗时：** Rust/gate CLI、Clang、运行时源码、临时存储和很小的生成文件输入。不需要 512 MiB 数据集或外部调度器依赖。单独对 `read_heavy_wide8.wf` 做证明/代码生成耗时 58.29 s，峰值 RSS 2.88 GB；它反映了部分源码编译成本，不是整个 target 或原生读取的测量。

### 9.2 Compute lowering、发布操作与链接验证样例

- **定义位置：** `research/experiments/compute-bench/Makefile`、`programs-check`、`module-symbols-test.sh`、`baseline-entry-test.sh`。
- **源码：** `programs/{mandelbrot,quadrature,records,fir,stencil,prefix,histogram,merge_sort,bfs,radix_scatter,range_split}.wf`，共 **11 份**。
- **构建：** 刷新 gate `whitefootc`。对十个 kernel 源码生成 `--par` 和 `--no-overlap` 模块，构建其目标文件及真实原生运行时目标文件。将一个很小的 C 链接验证程序与两种模块一起编译，检查 kernel/运行时的强符号。生成的链接可执行文件有一个空 `main`，**没有运行 kernel 的实际工作负载**。`range_split` 另外通过 `whitefootc` 获得两种普通可执行形式。
- **运行/检查：** 检查并行/顺序 IR 中的发布调用、符号及与实际运行时的链接；运行两种 `range_split`。两个 shell 自测检查符号验证和 baseline 入口连接。这里不运行五 kernel 性能榜，也不执行配对性能回归判定。
- **资源：** Gate 编译器 CLI、Clang/cc/链接器、`nm`/shell/文本工具、scheduler/ordinary 运行时源码和本地 WF 文件。主 `programs-check` 路径不需要构建 oneTBB/Rayon/Parlay。
- **产物新旧判断细节：** `checked_*` 是依赖源码、Makefile 和编译器的标记文件。标记仍有效时，规则中的检查/链接断言会跳过；同一规则里的 `range_split` 执行也会跳过。这与复用程序但仍重跑它们的容器 families 不同。两个显式 shell 自测 target 仍执行。这里只描述已有行为，没有把它选为替代 gate 的策略。
- **耗时：** 包含在本地父命令的 133.85 s 中；该次运行没有独立冷/热阶段总计。CI 总耗时见入口对照表。

## 10. 仓库、构建工具和测试执行器检查

这些检查验证项目、工具或测试集合自身的有效性。这里逐项展开，因为旧标签 `static` 无法说明实际执行什么；其中一些会启动真实子进程或执行 Python 测试。

| 类别与源码定义位置 | 构建/执行、输入和输出 | 本地 gate 耗时 |
|---|---|---:|
| Rust 格式：`compiler/Makefile` 的 `format` | `cargo fmt --all -- --check`；读取 Rust 源码并返回格式检查状态。没有类型检查、测试可执行文件或 WF 程序。需要 rustfmt。 | 1.10 |
| Rust 类型/lint 检查：`compiler/Makefile` 的 `lint` | `cargo clippy --all-targets --locked --offline -- -D warnings`；检查库、bin 和测试。需要编译器/规范/生成输入及 Cargo 产物；不执行 Rust 测试用例。本次测量已有缓存。 | 0.13 |
| Rust API 文档：`compiler/Makefile` 的 `docs` | `RUSTDOCFLAGS=-D warnings cargo doc --no-deps --locked --offline`；rustdoc 生成 `compiler/target/doc/` 并检查文档警告。不是 Markdown lint；`doctest=false`，因此没有 doctest 可执行文件。 | 2.55 |
| 验证进程保护：`.github/test-run-check.sh` 测试 `.github/run-check.pl` | Shell/Perl 子进程输入检查退出状态、嵌套、竞争的任务所有者、默认并发、超时、信号和孤儿进程清理。需要进程控制和临时存储；不需要 Rust/WF 编译器。属于 `repository-invariants`。 | 包含在 11.18 中 |
| 仓库不变量：根 `Makefile` | 除进程保护测试外，比较 `AGENTS.md`/`CLAUDE.md`，扫描已跟踪文件/路径中是否出现机器本地名称。使用 Git/shell/cmp/搜索。输出为通过/失败，不生成原生二进制。 | 总计 11.18 |
| 规范归档不可变性：根 `Makefile` 的 `spec-append-only` | 与本地 `main` 做 Git diff，查找已发布规范归档是否被修改或删除；不编译。需要有效的 `main` ref。 | 0.05 |
| 规范相关文字完整性：根 `Makefile` 的 `spec-prose-integrity` | 扫描 README/agent/docs 文字中的规范摘要副本和过期的当前版本声明；不编译。 | 0.08 |
| 设计树 linter：`design/skill/test_lint.py`、`design/skill/lint.py` | **17 个 Python unittest 用例**，然后检查设计树/修订草案，以及相对于审查基线的变更边界。需要 Python、Git、设计树文件。不构建 Rust 或 WF。 | 6.38；其中 Python 测试 5.128 |
| 规范符合性结构/执行器：`tests/conformance/test_runner.py`、`runner.py` | **25 个 Python 测试**，随后验证 manifest/源码对应关系、schema 和规则覆盖：129/129 条规则。输入是 manifest、WF 文件和规范。不执行实际 WF 编译器判定或原生程序。 | 0.21；其中 Python 测试 0.087 |
| 库/integration 测试集合：`compiler/Makefile` 的 `test-partition` | 六次 `cargo test --profile gate --lib ... -- --list` 选择，比较并集/数量、采样模块非空，以及声明的 integration target 列表。冷调用会构建**整个库测试可执行文件**，随后只列出测试。 | Rust 产物已构建时 0.32 |

根目录 `make static` 只包含 repository-invariants、spec-append-only、spec-prose-integrity、design-lint。`make -C compiler static` 包含 format、lint、docs、实际规范检查器（第 4.2 节）和直接 C completion 检查（第 7 类）。两个别名都不表示“什么也不执行”。这些计数都不属于库中的 1662 个 Rust 测试。

## 11. 性能测量流程

这些命令即使不运行任何 Rust 单元测试套件，也可能耗时很长。它们构建依赖、程序和数据，验证输出，然后重复执行原生工作负载并生成计时表。只有具备明确回归判定的流程，才提供这里所说的自动性能判定。

### 11.1 不同实现与 worker 宽度的 compute 正确性

- **定义位置：** `research/experiments/compute-bench/{Makefile,harness.c}`、`backend_*.c`/C++ 源码、`rayon/`、`programs/` 及该目录里的依赖构建脚本。使用与第 9.2 节相同的 WF kernel 输入，但具有真实基准入口和原生参考实现。
- **构建：** `make deps` 获取/构建固定版本的 oneTBB、Rayon 和可用的 Parlay 支持；`make build` 刷新 WF 编译器、生成不同 WF 形式、编译 C/C++/Rust 后端及运行时目标文件，并链接 kernel 程序。这些依赖构建独立于没有外部依赖的编译器 crate。
- **运行/资源：** `make verify` 按 kernel 判定逻辑执行不同形式与 worker 宽度。需要 C/C++/Rust 工具链、CMake、固定版本源码/crate、原生 worker 线程、临时存储和足够的 CPU 执行时间。缺少源码 checkout 时，可能在任何编译器开始运行之前就陷入网络等待。
- **耗时，独立本地流程：** 编译器和固定版本源码 checkout 已存在，但依赖产物全新时：**依赖 13.78 s**、**程序构建 9.45 s**、**正确性执行命令 70.07 s**。这是特定产物状态下的顺序命令时间。
- **调用入口：** `compute-bench.yml` 在 Linux/macOS 的自动 push job 中构建并验证；根目录 `make check` 运行第 9 类中更小的验证样例。这里较广的正确性检查会执行实际 kernel，不同于 `programs-check` 中大多数仅链接的样例。

### 11.2 配对 compute 性能回归

- **定义位置：** `.github/workflows/compute-regression.yml`，以及 compute-bench 的 `compare`、`verdict`、`verdict.awk`。工作流检查 PR 是否符合运行条件，导出 merge base，构建基线和候选两边的编译器/程序，并在测量前验证两边。它独立于根目录 `make check`。
- **输入/产物：** Merge base 与候选的编译器源码树/可执行文件、共享的固定版本依赖、匹配的 WF/原生 kernel 程序、配对计时表和判定。这不是 `cargo test`，没有 `#[test]` 工作负载。
- **运行/资源：** 在同一 Linux 虚拟机上做五轮配对测量，worker 宽度为 1/2/4，匹配 CPU 放置和环境。判定工具自身的人工表格测试属于第 8.3 节，不是这里的工作负载执行。
- **耗时/结果：** CI job **总计 350 s**。Merge base 编译器构建 49 s；依赖 27 s；两套程序 61 s；正确性验证 28 s；配对计时 152 s；准备及其他工作也包含在总时间内。15 个 kernel/宽度区块通过，0 个 suspect，0 个 CPU report。
- **判定规则：** 一个 kernel 在两个宽度上都满足“配对中位数减速大于 3%，且五对中至少四对变慢”时判失败。仅一个宽度出现信号时报告 suspect；CPU 信号只报告。这些是现有规则的条件，不是本清单新选定的策略。

### 11.3 完整 compute 对比性能榜

- **定义位置/命令：** 同一套 compute-bench 源码，`make compare`，以及 `compute-bench.yml` 中的表格发布；该工作流仅在手动 dispatch 时启用这些步骤。
- **运行：** 五个 kernel、139 个实现/宽度组合、五轮：本地流程共 **695 次进程调用，记录 4170 次首次/热调用**。结果是对比表，不会自动等同于配对性能回归判定。
- **构建/资源：** 与第 11.1 节相同的程序/依赖前置条件；反复启动进程/worker，并要求稳定的测量条件。
- **耗时：** 上述构建/验证完成后，本地 `compare` 命令 **229.62 s**。另一次源码获取尝试在 459.41 s 后停止，只有 0.19 user 和 0.08 system CPU 秒；这是构建前的网络等待，不是慢 kernel 或慢 Rust 编译器。

### 11.4 IO 多文件、密集读取、管道和网络流程

- **定义位置：** `research/experiments/io-completion-bench/Makefile`、`linux-bench.sh`、Linux/macOS 共用的 `read-bench.sh`、`linux-net-bench.sh`、`windows-bench.ps1`，配合 `gen.c`、`runner.c`、`baseline.c`、`read_baseline.c`、`pipe_producer.c`、`pipe_harness.c`、`uring_echo.c`、`epoll_echo.c`、`netload.c`、`programs/*.wf`。平台工作流调用方在 `.github/workflows/io-bench.yml`。
- **构建：** 刷新 gate `whitefootc`；C `-O2` 构建数据生成器、计时执行器和原生参考实现；编译器生成 WF/原生形式。有缓存时可能复用程序。这不是构建 Rust 单元测试。
- **运行/资源：** 生成真实文件、验证预期输出、执行进程计时轮次，并检查缓存策略探针。many-files 默认使用 8192 个文件，每个最大 16 KiB；测得的 read-heavy 数据集为 512 MiB。read 流程每次调用执行 32,768 次定位读取。管道测试需要真实管道和延迟生产者；网络测试需要 Linux event/ring API、loopback 对端和负载生成器。完整的 uncached 存储测量，要求宿主的缓存探针能验证这一标签。
- **Many-files 耗时：** 依赖修复后，必需的程序重建 128.19 s；热 `build` **0.22 s**，热 `verify` **6.46 s**；保持七次测量/两次预热的完整命令为 **70.62 s**，之前是 200.86 s。节省来自减少构建，不是减少原生 IO 指令。
- **Read-heavy 耗时：** Rust 已构建时，本地完整命令 **1288.52 s，即 21m28.52s**；约 4 s 用于原生参考程序/数据准备，349 s 用于八个 WF/原生程序，其余为验证和反复读取。在测得脚本中，macOS 17 个或 Linux 20 个组合 × 4 张表 × 7 次测量加 2 次预热 = **612/720 次调用**。这些流程参数不能与同一实验里其他 Make target 的默认值混用。
- **其他边界：** 本次审计没有单独测量本地管道/网络的构建与运行总计。下面的手动 Linux 网络步骤是整个 CI 步骤的证据，不是只计原生执行。
- **调用入口：** 完整 `io-bench.yml` 为手动。根 gate 只调用第 9.1 节；自动平台正确性见第 7 类。这里没有声称完整 IO 计时提供了自动存储性能回归判定。

较早版本 **`35227ab3`** 上完成的手动 IO 工作流：

| 手动 job | Job 耗时 | 主要流程工作 |
|---|---:|---|
| Linux 多文件/网络 | 355 | 文件流程 293；网络 30；其余为准备和其他工作 |
| Linux 密集读取 | 3995，即 66m35s | Rust 36.70；程序/数据/验证约 570；uncached 表 1068/2239；warm 表 46/5 |
| macOS 读取和多文件 | 1146 | 读取流程 929；多文件 199 |
| Windows 测量 | 463 | 含构建的原生测量流程 418 |

Linux 64 KiB 表的后置探针拒绝了 uncached 标签：工作流显示绿色，不代表这张表可作为有效的 uncached 证据。本次分类没有重跑这些 job，也没有再跑一次一小时测量。

## 12. 历史工具与其他需显式调用的实验执行器

### 12.1 保留的研究工具自测

**调用入口。** 根目录 `make historical-tool-tests` 用于显式复现，不在根目录 `make check` 或自动 CI 内。保留目的在于复现已完成的研究工具，不是再检查一次当前编译器。

| 目录和源码形式 | 构建与执行 | 资源 |
|---|---|---|
| `research/experiments/frequency-study/`：`tests/`、`bounds-ir/`、`alias-versioning/`、`effect-attrs/tests/` | 通过 `make check` 做 Python unittest discovery，测试挖掘器/判定工具/分类器行为 | Python 和合成输入 |
| `research/experiments/frequency-study/reassociation/` | 自己的 Cargo crate：默认 profile 的 `cargo test`、Clippy 和 rustfmt | Rust、锁定的 crate 输入及独立 Cargo target 目录 |
| `research/experiments/default-floor/tests/` | Python unittest discovery，检查生成/模型研究工具 | Python 和测试输入 |
| `research/experiments/default-floor/utf8parse/{rust-baseline,harness}/` | 两个独立 Cargo manifest；默认 profile 的 `cargo test` 构建和执行各自的测试 | Rust、锁定依赖及各 crate 自己的 target 存储 |
| `research/experiments/default-floor/percent-decode/{rust-baseline,harness}/` | 另外两个 Cargo manifest，也使用默认 profile 的 `cargo test` | 同类资源，独立 target 存储 |

保留的复现命令在 **Rust 产物已存在时，用时 8.16 s 并通过**。那次运行没有测每个 crate 的冷编译、每个套件的执行时间和完整的逐用例数量。这里的默认 test profile 属于外部基线/研究工具 crate，不是生产编译器的第二次 dev 构建。当前 UTF-8/DEFLATE/wfgrep 编译器验证样例仍保留在前面的活跃分类中。

### 12.2 仓库中其他显式实验入口

下列入口不由本文映射的根目录/CI 命令调用；列出它们，是因为 agent 仍可能直接调用。成本**尚未测量**，存在 Make target 也不代表其历史输入仍兼容当前编译器。规则中的预算是上限，不是观测到的耗时。

| 目录/入口 | 构建和执行什么；输入/资源边界 |
|---|---|
| `research/experiments/differential-fuzz/`：`smoke`、`campaign`、`probes` | Cargo release 的 `wf-difffuzz` 生成器/判定工具，加 gate `whitefootc`；生成 WF，按 lowering × worker × helper 组合编译/运行，使用临时文件和真实 IO。Smoke 请求 20 个应接受程序，campaign 预算 600 s；默认完整 campaign 请求 2000 个，预算 5400 s，4 个任务。这些预算本身不能证明它们适合作为本地默认值。 |
| `research/experiments/buffer-initialization-cost/`：`check`、`bench` | 直接 `rustc` 构建 `runner.rs`；Clang 构建 `control.c`；**未指定 profile 的 `cargo run --bin whitefootc` 在独立 target 存储中构建/使用 dev 编译器**处理 `drain.wf`；检查优化后 LLVM，执行原生对照。 |
| `research/experiments/wfgrep-baseline/`：`verify`/`check`、`bench`、`profile` | 直接 Rust 执行器；生成文件系统语料；普通/原生输出、原始/优化后 LLVM 和汇编。**未指定 profile 的 Cargo 编译器调用使用 dev**，可能反复执行昂贵证明。 |
| `research/experiments/wide-scan-lowering/`：`verify`/`check` 及测量 target | Rust 执行器、本地 WF 形式、C/原生比较、生成输入数据及 LLVM 检查。**未指定 profile 的 Cargo 编译器调用使用 dev**，独立于带保护的根 gate。 |
| `research/experiments/wfgrep-double-walk/`：`verify`/`check` | Gate `whitefootc`、直接 Rust 执行器、多个 WF/原生二进制，以及生成语料的标记文件；执行输出验证。这里没有测得完整耗时。 |
| `research/experiments/park-on-miss-switch-cost/`：`run` | `cc -O2 -pthread switch.c` 生成原生计时可执行文件，运行历史切换成本实验。没有 Rust/WF 测试执行器。 |
| `research/experiments/port-study/wc-chunk-summary/`：`check`、`bench` | 历史 Python 原型编译器调用、WF/Clang 产物、Rust/C 对照，以及 Python 代数/输出检查。规则引用旧的 `prototype/democ` 路径；未验证当前兼容性。 |
| `research/experiments/zlib-core-kernels/test_guarded_bit_window.py` | 通过旧原型编译器导入，做 Python unittest 输入转换和 LLVM 字符串断言，不使用当前 `whitefoot`；不声称有当前工具链执行证据。 |
| `research/experiments/zlib-core-kernels/compiler-prototypes/periodic/test_periodic_copy_experiment.py` | Python 测试导入旁边的原型 `democ.py` 和历史 WF 输入路径；检查实验特定 lowering，不是 Rust 接受路径。当前兼容性未验证。 |

`research/experiments/literal-line-floor/ceiling/` 和 `research/investigations/proof-derived-parallelism/bench/rust/` 下另有 Cargo manifest，定义独立性能/参考产物。当前根目录/CI 测试命令都没有选择它们。存在这些文件，并不意味着当前根目录/CI 的正式测试集合中另有一套 Rust 测试套件，也不代表上面的总计已包含它们的测量成本。本清单覆盖维护中的根目录/CI 集合，以及这里明确列出的实验入口，不覆盖历史研究文字里的每一段可执行代码。

### 12.3 显式原生开发 target

`compiler/Makefile` 还提供 `completion-core-read-stress`：构建 core 探针后重复运行 200 次；`completion-windows-cross`：用交叉工具链构建并检查 Windows PE/运行时目标文件；`completion-windows-wine`：用 Wine 运行这些产物。它们不在根目录 `make check` 内，需要对应编译器/模拟器及临时/宿主资源。完整耗时未测量。交叉链接/符号检查成功，不能替代第 7.2 节的真实 Windows 测试。Sanitizer target 的实际 CI 调用方和计时见第 7.1 节。

## 本地入口

下面是测得的完整根 gate 执行顺序，并列出内部命令。构建在相应 Cargo/Make 命令内部按需发生；`build` 和 `test-build` 不是在此之前额外无条件执行的步骤。

```text
make check                                         786.97 s，测量版本 f3858780
  1. repository-invariants                           11.18 s
  2. spec-append-only                                 0.05 s
  3. spec-prose-integrity                             0.08 s
  4. design-lint                                      6.38 s
  5. conformance                                     0.21 s（Python 结构检查）
  6. make -C compiler check                         479.93 s，内部包含：
       format                                        1.10 s
       lint                                          0.13 s（已缓存的 Rust 检查）
       test-partition                                0.32 s（只列出测试）
       test-unit                                   196.13 s（1588 个测试）
       test-sampling                                29.27 s（74 个测试）
       test-corpus                                 242.93 s（137 个非 ignored 测试）
       docs                                          2.55 s
       spec                                          0.34 s（实际规范扫描工具）
       completion-test                               5.75 s（C 构建/运行）
  7. research-tests                                  15.94 s
  8. bench-programs                                 133.85 s
  9. conformance-run                                118.79 s（原生适配器）
 10. snapshot-run                                    19.21 s
```

这些是外层命令读数；与各行之和的少量差别来自包装器和阶段切换开销。缩进的 compiler 行是对 479.93 s 的拆解，不应再加到它上面。编译器测试用例自身的执行计时为 196.09、29.23、241.74、118.75、19.17 s，不包含外层命令开销。

| 其他本地入口 | 准确选择哪些内容 |
|---|---|
| 根目录 `make static` | 仅根阶段 1–4；包含 shell/Perl/Python 执行 |
| `make -C compiler static` | 格式、Clippy、rustdoc、普通规范检查器，以及直接 C completion 构建/执行 |
| `make -C compiler test` | Partition、普通库测试、采样及 bin/integration `test-corpus`；不执行两个 ignored 适配器 |
| `make -C compiler build` | 构建 gate 编译器；不执行其 Rust 测试或任何 WF 程序 |
| `make -C compiler test-build` | 构建全部 gate 测试 target 和 Cargo 选中的 bin 产物；不执行 Rust 测试用例 |
| 根目录 `make historical-tool-tests` | 仅第 12.1 节保留的研究工具测试 |
| Compute/IO 的 `compare`、`bench`、平台 read/network 脚本 | 第 11 类中明确命名的流程，不是根 gate 的别名 |

## CI 入口

工作流定义位于 [.github/workflows/](../../../.github/workflows/)。下面计时来自成功的 **`f3858780` job**。每行是独立虚拟机/job；job 总时间包含准备，不包含排队。并发 job 不能相加成为用户看到的工作流等待时间。本地测量机器与 CI 宿主的执行/启动成本也不同。

### Gate 工作流

`gate.yml` 在 Linux/macOS 各有七个 job。每个 job 安装或记录自己的工具链，并构建所需 Rust/原生产物。`research`、`unit` 或其他 job 构建的编译器不会跨 job 共享。Cargo 使用两个构建任务；CI 的测试线程数按其专用宿主设置。

| Job 与命令 | Linux 总计 / 主步骤 / 准备及其他 | macOS 总计 / 主步骤 / 准备及其他 |
|---|---|---|
| `static`：根目录 `make static`，然后 `make -C compiler static` | 119 / 98 / 21 | 132 / 117 / 15 |
| `unit`：`make -C compiler test-partition test-unit` | 318 / 298 / 20 | 327 / 311 / 16 |
| `sampling`：`make -C compiler test-sampling` | 194 / 155 / 39 | 152 / 140 / 12 |
| `corpus`：`make -C compiler test-corpus` | 239 / 220 / 19 | 309 / 296 / 13 |
| `conformance`：根目录 `make conformance`、`make conformance-run`、`make snapshot-run` | 134 / 110 / 24 | 174 / 158 / 16 |
| `research`：根目录 `make research-tests` | 289 / 268 / 21 | 254 / 237 / 17 |
| `bench-programs`：根目录 `make bench-programs` | 215 / 191 / 24 | 361 / 341 / 20 |

各步骤内部的构建与执行拆分，按 Linux / macOS 顺序：

- **Static：** Clippy 16.30/24.65 s；rustdoc 8.23/11.38 s；gate 规范工具/库构建 51.13/55.58 s。其余包括根检查和直接 C 探针构建/运行。Clippy/rustdoc 并不是再构建一个用来处理 WF 的 dev 编译器。
- **Unit：** 库测试构建 **99/137 s**；1588 个测试执行 **197.60/172.54 s**。要列出测试，必须先有这个可执行文件。
- **Sampling：** 在另一台虚拟机上构建同一个库测试 target，耗时 **130/123 s**；随后 74 个用例执行 **24.99/15.38 s**。
- **Corpus：** Rust bin/integration 构建 **51.10/59.35 s**；110 个程序测试执行 **167.97/234.38 s**；其余 27 个用例 0.79/1.15 s。这些用例时间包含内部 WF/原生子进程工作。
- **Conformance：** 普通库/适配器构建 **46.77/75 s**；适配器执行 **43.78/54.87 s**；snapshot target 构建 0.29/0.63 s、执行 **19.20/25.18 s**，另有 Python 结构检查。
- **Research：** proof-use target 的 63.69/67.78 s 中，包含编译器构建 **53.42/63 s**。这些冷虚拟机上的容器构建/验证为 **203.01/166.58 s**；其余小型判定工具也运行。
- **Bench programs：** 编译器构建 **41.19/81 s**，随后进行第 9 类的 IO/compute WF/原生构建及小规模检查。

本次运行每个 job 的 checkout 约 5–8 s，Linux 工具链准备 10–26 s，macOS 工具链准备 1–3 s。“准备及其他”不全是网络等待。日志里的“十个最大间隔”输出，也不是逐测试耗时统计。

### 其他自动工作流

| 工作流 | 实际阶段与资源 | 观测总计 |
|---|---|---|
| 满足条件的 push 上的 `compute-bench.yml` | 固定版本依赖 -> kernel 程序 -> 实际不同形式/宽度的正确性。Linux/macOS、C/C++/Rust/CMake 和原生线程。计时/表格步骤需 dispatch。 | Linux 151 s；macOS 104 s |
| 符合条件的 PR 或 dispatch 上的 `compute-regression.yml` | 基线/候选编译器构建 -> 两套程序 -> 验证 -> 配对计时 -> 回归判定；同一 Linux 宿主 | 350 s |
| `io-hosts.yml` push/dispatch，Linux | 必需的原生 ring + 普通 C 测试程序 + sanitizer 变体 | 43 s |
| `io-hosts.yml` push/dispatch，Windows | 原生平台 C 探针 + 一个 gate 编译器 + 实际 WF 平台程序 | 207 s |

Compute 正确性中，依赖构建 41/17 s、程序构建 54/59 s、验证 24/13 s，顺序为 Linux/macOS。第 11.2 节和第 7 类列出配对回归与平台步骤细节。完整 `io-bench.yml` 为手动；第 11.4 节明确给出了较早测量 job 的版本。

证据：[gate](https://github.com/mbbill/Whitefoot/actions/runs/34924294748)、[compute 正确性](https://github.com/mbbill/Whitefoot/actions/runs/34924294745)、[配对回归](https://github.com/mbbill/Whitefoot/actions/runs/34924297881)、[IO 宿主](https://github.com/mbbill/Whitefoot/actions/runs/34924294754)、[较早的手动 IO](https://github.com/mbbill/Whitefoot/actions/runs/34912406944)。

## 重复工作与重新设计时要回答的问题

本清单区分**已经观测到的重复工作**与**等价断言**。前者可以从源码/日志看到；后者需要比较被保护的性质、输入、故障模型和判定逻辑。使用同一 WF 文件的两个测试，可能保护不同编译阶段；反过来，不同测试名或可执行文件也不能证明覆盖不同。

| 已观测的重叠或额外工作 | 证据与边界 | 替代体系需要回答的问题 |
|---|---|---|
| 便宜的编译器断言与昂贵的原生测试共用一次大型库测试编译 | 第 1–3 类共用含 1662 个用例的 Rust 可执行文件；顺序构建实验中，冷编译单元 69.36 s | 哪些测试需要访问编译器私有实现，哪些可使用预构建的公共编译器/运行时产物，而不再增大这个编译单元？ |
| CI unit/sampling 各自构建同一个库测试 target | 第 3 类和 gate CI：各 job 自身构建为 99–137 s | 构建一次并复用产物，节省是否大于独立 job 带来的等待时间收益？需要实测 runner/产物方案。 |
| 八个 Rust 测试可执行文件，外加普通工具 | 产物图和第 4–6 类；普通与测试版本共享普通库 | 哪些物理测试边界有价值？减少可执行文件可能减少链接/辅助工作，但不会自动减少 WF 证明调用。 |
| Gate 使用测试版语法工具，Cargo 却还构建普通版 | 第 4.3 节、Cargo 全 target 构建 | 相比已检查的测试实现和类型检查，多链接一个普通可执行文件增加了什么覆盖？应测量更精简的构建选择。 |
| 规范扫描器在测试和普通工具中都检查当前身份/完整性 | 第 4.2 节、`active_spec_has_complete_internal_integrity` 和 `run_gate` | 明确重叠范围，同时保留对非法输入的检测和对实际当前规范的检查。 |
| 运行时目标文件复用在进程退出时结束 | `compiler/tests/support/mod.rs` 的 `OnceLock`；unit、sampling、programs、conformance 是独立调用 | 本地 unit/sampling 复用同一个库测试可执行文件，但不共享内存缓存。是否值得建立依赖正确的跨进程原生产物复用？ |
| 同一真实 WF 源码被多个类别分析 | `semantic/tests/entailment.rs`、`backend/tests/cost_shape.rs`、`tests/programs/wfgrep.rs` 分别用 wfgrep 检查证明根、IR 成本形态、运行行为 | 能否由一次当前编译暴露所需观测，而不缓存或替代语义判定？必须核对具体行为/选项差异。 |
| 固定 run 在普通/并行语料检查中重复 | 第 6 类：两个独立用例的 WF 编译为 90.222/85.890 s | 哪些证明/前端工作相同，哪些 lowering/结果性质不同？应区分编译复用和删除一种模式。 |
| 快照判定与广泛的语义/规范符合性主题重叠 | 第 5.3 节：484 条历史接受/拒绝比较，执行 19.17 s | 哪些行能捕获当前规范下的独有缺陷？仅历史判定变化不一定是回归。目前没有逐行的冗余证明。 |
| 二十次分析及大型原生采样矩阵 | 第 1、3 类：20 次证明重复、36 个程序/144 次执行的递归用例、worker/重复循环 | 每次重复或每条矩阵轴区分什么故障？什么更便宜的反例检查可以保留这些证据？最少次数尚未建立。 |
| C completion 探针每次都重建，并在 Linux IO-host CI 重复 | 第 7 类及 `compiler/Makefile`；sanitizer/required-ring 变体有额外条件 | 分开处理新构建与执行，区分重复普通断言和仅由 sanitizer/平台提供的证据。 |
| 研究目录同时生成普通模型和 `--test` 产物 | 第 8.2 节的 authority/foundation/families | 哪些普通 `main` 枚举增加了 `#[test]` 缺少的用例，哪些仅重复同一断言？仅靠数量无法回答。 |
| Lifecycle 无条件重建七个应接受的原生程序 | 第 8.2 节：热运行内部循环约 12.5 s | 能否按源码/编译器/调用依赖复用程序，同时仍执行必需用例并重新检查负向判定？ |
| Foundation layout 在正确性检查中输出计时；普通 IO 构建包含检查不会使用的计时执行器 | 第 8.2、9.1 节：复制样本 0.224 s；`ordinary-check` 从不调用 `runner` | 将探索性测量移出正确性调用，只构建断言会消耗的资源。本清单没有实现此变更。 |
| 研究检查构建了超出实际执行断言需要的检查产物 | 第 8.2 节：foundation 的 `large-result.ll`；dense 的 `optimized*.ll`、`native*.opt.ll`、`native*.s`、`boundary256.ll`。WF `dense*.s` 还用于实际执行的比较程序，不在这组额外输出内。 | 哪些产物参与自动正确性/性能判定，哪些可以只在实际使用它们的检查或测量中构建？ |
| Compute checked 标记仍有效时，规则断言和 range-split 执行会跳过 | 第 9.2 节的 `checked_*` 规则 | 哪些是构建验证，哪些运行时断言应在每次验证中执行？应明确何时可以复用既有结果。 |
| IO gate 构建十二个程序却不运行工作负载；完整 compute 正确性与小型构建样例重叠 | 第 9、11 类及工作流对照表 | 为每项必需的源码/宿主行为和性能判定指定明确调用入口，同时避免不必要重建与未经执行验证的运行时声明。 |
| 显式实验路径仍调用未优化编译器 | 第 12.2 节的 buffer-init/wfgrep-baseline/wide-scan Makefile | 保留的当前实验需要明确编译器 profile 和资源约定；不能让历史规则悄悄成为本地验证默认值。 |

这些问题是重新设计需要补齐的证据缺口，不是删除检查的指令，也不要求保留现有架构。项目所有者给出的选择标准是：检测正确性问题或性能回归。每项拟保留、替换或退役的检查，都可以按本文列出的具体输入、产物、判定逻辑、资源和成本评估。本文没有选择新的 target 布局，也没有修订正式设计树。

## 尚未测量清楚的边界

- 双测试线程、1588 用例套件中，前端/证明与普通后端用例各自独占的墙钟时间。逐用例时间会重叠，共享初始化可能被两个等待测试重复计时。
- 六个研究子目录逐 target 的完整冷构建/运行拆分、21 个独立 Rust 用例各自的耗时，以及 Windows/sanitizer job 内完整的原生编译/运行拆分。
- 整个 gate 中临时原生程序和子进程调用的总数量。部分辅助路径和详细用例有计数，但自定义构建/运行路径没有全部观测。`#[test]` 或 WF 文件数不能替代这个缺失数字。
- 库、规范符合性、快照、真实程序、C 运行时、研究断言之间的覆盖等价性。本清单指出交集，但尚未建立最小替代套件。
- 大型优化 Rust 编译单元具体慢在哪个 rustc/LLVM pass；冷语料环境异常缓慢的归因；首次启动延迟背后的宿主服务。给阶段命名本身，不能建立新的因果结论。
- 维护中的根目录/CI 集合之外，显式历史实验 target 的完整耗时与当前兼容性。

已有的原始计时附表用 `NA` 标记未观测阶段，并单独保留失败尝试。原报告记录了测量版本、宿主配置、成功的完整 gate，以及待定的验证成本修订草案。重新设计应以具体证据消除这些不确定性，不能把未知成本或旧判定重新标成已验证事实。
