# Whitefoot

本文译自 [README.md](README.md)，如有出入，以英文版为准。文中链接的文档均为英文。

Whitefoot 是一门研究性质的系统编程语言，围绕三个特性设计：

- **安全。** 编译器接受的程序没有未定义行为（undefined behavior），前提是它所依赖的软件是正确的：编译器、LLVM、运行时和操作系统，以及下文列出的其他部分。程序不会 panic，程序里也不运行任何边界检查、溢出检查或转换检查。语言里没有可以用来绕开检查的 `unsafe`，程序还可以声明自己完全不使用堆。
- **快。** 安全来自编译期检查的证明，而不是运行时的检查；同样的证明让编译器可以去掉边界检查和溢出检查、告诉 LLVM 哪些引用互不别名（alias），并把互相独立的代码并行执行。
- **小。** 函数、结构体、枚举和显式泛型，接近 C。没有生命周期（lifetime），没有方法，没有 trait，没有异常。

大多数证明由编译器自己找到。它用的是一套固定的过程，而不是 SMT 求解器（SMT solver），也就是 SPARK、Dafny 和 Verus 等工具背后的自动定理证明器。判定过程中没有超时，也没有工作量上限，所以两台机器对一个程序是否被接受永远不会给出不同的结论（[ENT-1](spec/kernel-spec.md#15-obligation-discharge-deterministic-facts-invariants-and-local-certificates-normative)）。剩下的部分由你写成循环不变式（loop invariant），偶尔再写一小段证明步骤，由编译器来检查。

## 安全：没有未定义行为，没有 panic，没有会失败的检查

每一个在运行时可能出错的操作，比如下标、整数运算、收窄转换、除法或分配大小，都必须在程序被接受之前证明在合法范围内。语言没有 `unsafe`、没有 panic、没有异常，也没有栈展开（unwinding）；预期中的失败是一个由调用方处理的值（`Result`、`Option`）。只要可信基（trusted base）是正确的，被接受的程序就不会：

- 越界读写、使用已释放的内存，或读取未初始化的内存；
- 悄无声息地发生整数溢出。每个运算都写明自己的含义（`+wrap`、`+checked`、`+sat`），不带后缀的 `+` 则必须证明不会溢出；
- 在收窄转换中丢失数值，或者除以零；
- 发生数据竞争：并行执行只来自已经证明的独立性，其结果等于顺序执行的结果；
- panic、abort、抛出异常或栈展开。语言里没有这样的构造；
- 在 debug 构建和 release 构建之间表现不同。构建只有一种。

它仍然可能：

- 耗尽栈空间。这时程序以固定的记录 `{"resource":"stack"}` 停止，每次运行都一样；`--stack-ledger` 会报告每个函数的栈帧，以及每个递归环能容纳多少层；
- 耗尽堆内存。分配器会停止程序；在开启了 overcommit 的 Linux 上，内核的 OOM killer 可能会先动手；
- 陷入死循环，或者算出错误的结果。契约（contract）描述的是写下来的东西，而不是本来想要的东西；
- 被错误地编译。可信基包括 Whitefoot 编译器及其检查器、LLVM 和 clang、运行时和分配器、作为可信定义链接进来的 C 函数、libc 以及操作系统（[SCOPE-3](spec/kernel-spec.md#1-scope-and-conformance)）。

其他系统用别的方式证明同样的无运行时错误。SPARK 是 Ada 的一个子集，面向高完整性软件，它在一个独立于编译的分析中使用 SMT 求解器：无论程序是否已被证明，Ada 编译器都会把它构建出来。Wuffs 不用求解器也能检查类似的证明，但它是一门用来编写解析、解码和编码文件格式的库的语言，它的代码不能进行系统调用，也不能分配内存。

### 从内存到资源

内存安全是证明的起点，而不是终点。目标是让 Whitefoot 在证明所能做到的范围内，成为尽可能安全、健壮的系统语言，足以承载最关键的基础设施；下一步是程序的资源：它使用的内存、它的栈、它的时间，以及它驱动的设备。

目前：

- **堆是可选的。** 以 `program no_heap;` 开头的程序，或者模块程序中用 `no_heap` 声明的入口（entry），都不能分配内存：在该程序或入口所运行的代码中，编译器会拒绝所有堆类型和所有会分配内存的调用（[STOR-8](spec/kernel-spec.md#6-storage)）。
- **必须释放的资源是线性的。** 线性值（linear value）不能被复制，编译器也从不自行丢弃它：程序必须把它传下去，或者交给一个会消耗它的函数。标准库里的文件、目录、监听器以及连接的收发两端都是线性的，所以每一个都恰好被关闭一次，由 `close_read` 这样的显式调用完成；可能丢失其中任何一个的代码都会被拒绝。
- **栈的使用会被报告，尾调用不会让栈增长。** 栈耗尽时，程序以上文那条固定记录停止；`--stack-ledger` 报告每个函数的栈帧；标注了 `musttail` 的自调用会直接跳转，不会让栈增长。

计划中：一种最高安全模式，用于不能接受失败的系统。在这种模式下编译的程序将具备：

- 没有堆，也没有其他任何动态资源；
- 栈峰值经过证明，不超过以字节给定的容量；
- 每个循环和每次递归都经过证明会结束；
- 硬件外设映射为线性值，因此设备的占有、使用和释放，与文件受同样的证明约束；
- 没有在运行时调度的并行；
- 经过证明的时间上界：每个外设需要多长时间响应，程序需要多长时间启动。

这个模式目前还完全没有实现。[固定资源执行的研究记录](research/investigations/fixed-resource-execution/README.md)记录了到目前为止关于堆、栈和终止性的设计，以及每一部分还缺什么；外设、并行和时间这几部分尚未设计。

## 快：证明换来速度

证明一个操作在合法范围内，也就让它的运行时检查变得多余；证明两段代码访问的是不同的内存，就可以让它们同时运行。

### 被证明去掉的边界检查

这个循环原地保留缓冲区中的非空格字节。`invariant behind: kept <= i` 这一行声明了一个名为 `behind` 的不变式（invariant），说明为什么写入 `buf[kept]` 不会越界。编译器在第一次迭代之前和每次迭代之后证明它，再结合 `i < buf.len` 得出 `kept < buf.len`，所以这次写入被编译成一条普通的存储指令：

```
fn squeeze(buf: &[u8]) -> kept: u64 writes(buf) {
  let kept = 0_u64;
  for (
    i in 0_u64..deref(buf).len,
    invariant behind: kept <= i
  ) {
    let byte = deref(buf)[i];
    if byte != 32_u8 {
      set deref(buf)[kept] = byte;
      set kept = kept + 1_u64;
    }
  }
  return kept;
}
```

我们测量了这个循环在 Rust 中的七种安全写法（rustc 1.98.1，x86-64），每一种编译出来的循环都保留了对 `kept` 的运行时检查；测得的写法中没有这项检查的，用的是 `unsafe`、对自有的 `Vec` 调用 `retain`，或者第二个缓冲区（[测量结果](research/experiments/bounds-check-spellings/README.md#results)）。如果没有这个不变式，Whitefoot 会拒绝这个函数，并指出缺少的事实 `kept < deref(buf).len`。[不用求解器，手工证明](docs/articles/proofs-by-hand.md)一文跟着编译器一步一步走完了这个证明。

### 顺序的代码，并行的结果

每个函数都用 `reads(...)` 和 `writes(...)` 写明它读取和写入什么，编译器会对照函数体检查这个声明。所以它知道每次调用会访问哪些内存；当它证明两个调用访问的是不同的内存时，就可以让它们同时运行。这段源代码里没有任何地方要求并行：

```
fn quicksort(v: &[u64]) -> result: unit writes(v) {
  let n = deref(v).len;
  if n <= 1_u64 {
    return unit;
  }
  let p = partition(v: v);
  let after = p + 1_u64;
  let smaller = &deref(v)[0_u64..p];
  let larger = &deref(v)[after..n];
  quicksort(v: smaller);
  quicksort(v: larger);
  return unit;
}
```

用 `--par` 编译时，这两个递归调用会并行执行，直到一个由工作线程（worker）数量决定的深度，因为编译器证明了 `[0, p)` 和 `[p + 1, n)` 互不重叠，而结果就是顺序程序算出的那个结果。对 200 万个数排序，顺序执行用时 0.18 秒，4 个工作线程用时 0.07 秒，取的是在一台共享机器上七次运行中最好的一次（[测量记录](research/experiments/par-quicksort/README.md)）；`--par-ledger` 会打印每一个决定及其理由。在 Rust 里，对源代码做一点小改动，`rayon::join` 就能让这两个调用并行执行，而且它的类型能排除数据竞争。这里没有任何一行要求并行，编译器只在证明了结果等于顺序执行的结果时，才会并行执行调用。[写顺序的代码，得到并行的结果](docs/articles/sequential-code-parallel-results.md)一文从经过检查的效应行（effect row）一直跟到生成的并行代码。

### 同一批证明的其他用途

- 经过证明的 `+` 会编译成一条带有 LLVM no-wrap 标志的普通加法（无符号为 `nuw`，有符号为 `nsw`），优化器可以利用这个标志。
- 每个引用参数传给 LLVM 时都带有 `noalias`（相当于 C 的 `restrict`），因为只有在证明了一个参数写入的内容其他参数都访问不到时，编译器才会接受这次调用。例外是 `swap`，它的两个参数可以是同一个位置。
- 如果循环的每次迭代只写自己的元素，或者用一组固定的、满足结合律和交换律的运算（如 `+wrap`）合并出一个值，这个循环就可以拆分给多个工作线程执行。

## 小：C 的简单结构，部分 Rust 语法

Whitefoot 保留了 C 的简单结构，借用了部分 Rust 语法。程序由函数、结构体、枚举和数组组成。下面这个函数返回缓冲区中的下一个字节，并让游标（cursor）前进一位：

```
struct Cursor {
  position: u64;
}

fn next_byte(input: &[u8], cursor: &Cursor) -> result: Option<u8> reads(input), writes(cursor) {
  let at = deref(cursor).position;
  if at < deref(input).len {
    let byte = deref(input)[at];
    set deref(cursor).position = at + 1_u64;
    return Some<u8>(value: byte);
  }
  return None<u8>();
}
```

C 程序员一眼就能读懂其中大部分。不同之处在于，Whitefoot 要求你把下面这些都写出来：

- `reads(input), writes(cursor)`：函数可以读和写什么，也就是它的效应（effects），写在签名里。没有 `&mut`：只有效应中写明了，函数才能通过引用写入；
- `deref(cursor)` 和 `set`：每一次通过引用读取，以及每一次赋值；
- `1_u64` 和 `value: byte`：每个数字的类型，以及传给函数或构造器的每个参数的名字；
- 每个表达式只做一步运算，较长的计算每一步用一个 `let`，所以没有运算符优先级（[GRAM-6](spec/kernel-spec.md#3-grammar)）。

写出来的代码比同样的 C 代码长，而每种构造只有一种写法。

没有生命周期。引用可以绑定到局部变量，也可以作为参数传给调用，但永远不会被存进结构体或被返回（[REF-3](spec/kernel-spec.md#5-ownership-and-references)），所以它不可能比它指向的东西活得更久。这就是为什么 `Cursor` 保存的是位置而不是缓冲区，也是为什么查找某样东西的函数返回的是下标而不是引用。按这种方式写的 Rust 代码同样不需要生命周期标注。Rust 也允许游标持有它的缓冲区，即 `struct Cursor<'a> { input: &'a [u8], position: usize }`，这时每个包含这种游标的结构体也都需要生命周期标注。Whitefoot 只有第一种方式，所以没有生命周期需要学习。代价是函数不能返回指向其输入内部的引用：词法分析器（tokenizer）返回的是各个 token 的位置，而不是借用的切片，由调用方来构造引用。

泛型是显式的：泛型函数在每次调用时都接收类型参数，比如 `array_filled::<u8, 4>(value: 0_u8)`，并且针对每一组参数各编译一次（[FN-2](spec/kernel-spec.md#8-functions-generics-contracts)）。

语言中没有：

- 方法、trait 和动态分派。一次调用只指名一个函数；泛型代码把它用到的函数作为显式的编译期参数接收，`interface` 为这样一组函数命名，不会根据类型去查找任何东西；
- 运算符重载和隐式转换。两个 `u64` 值上的 `+` 只有一种含义，转换要写成 `cvt`；
- 异常、栈展开和空值（null）。错误是一个 `Result` 值，缺失是一个 `Option`；
- 闭包和函数值。运行时的选择用对枚举的 `match` 来表达。

## 亮点

安全、快、小是核心。下面是其他值得了解的内容。

### 现在可用

- **容易写，也容易审查。** 一门没有生命周期、每种构造只有一种写法的小语言，学起来快，一段代码也只有一种读法。每个签名都写明函数读取和写入什么，契约写明它要求什么、保证什么，所以审查者读到一次调用时，不用打开函数就知道它可能访问什么。每次拒绝都指出一条规则和一个位置，其中很多还会给出修复建议，这些信息都可以以 JSON 形式输出（`--diagnostic-format json`）；测试把最常见的修复固定到一个修复后能编译通过的程序上。让这门语言便于人编写和审查的特性，同样让 AI agent 容易编写和审查。
- **并行度在运行时决定。** 程序从不规定同时运行多少个任务。在 `--par` 下，编译器把独立的调用和循环区间变成空闲工作线程可以领取的工作，运行时根据工作线程数（`WF_WORKERS`）决定一次递归展开到多深；没有工作线程领取的调用就在发起调用的线程上执行。无论运行时怎么决定，结果都等于顺序执行的结果，`--par-ledger` 会解释编译器做出的每一个决定。
- **增量构建。** 程序按模块逐个检查和编译。使用 `--cache DIR` 时，只要输入不变，模块的判定结果和每个函数的证明都会被复用，编译好的代码也会被缓存，因此一次修改需要重新证明和重新编译的，基本只是它改动的部分；每次构建仍然会对整个程序做类型检查。构建速度还没有系统地测量过。

### 进行中

- **不用 async 的并发 I/O。** 语言里没有 `async`、`await`、future 或回调：文件和套接字都是普通的值，I/O 操作就是普通的调用。做 I/O 的函数在效应行后面声明 `waits`，只有同样声明了 `waits` 的函数才能调用它，所以程序可能暂停的每个地方都写在函数签名上。等待调用在操作完成后才返回；它等待时，线程去运行别的上下文。在等待调用前加 `mustpar`，就在一个有自己栈的新上下文里启动它，于是服务器可以同时服务每一个连接（`tests/programs/tcp_contexts.wf`）；每个启动的上下文都在启动它的函数返回之前结束。编译出的程序通过基于完成通知的运行时（completion runtime；Linux 上是 io_uring，Windows 上是 I/O 完成端口）执行 I/O。`--par` 让计算重叠，但被重叠的计算从不等待 I/O。尚未完成的是：从启动的调用拿回结果，以及让上下文运行在多个线程上。

### 计划中

- **最高安全模式**，见[从内存到资源](#从内存到资源)一节：没有动态资源，栈上界经过证明，终止性经过证明，外设作为线性值，没有运行时调度的并行，响应时间和启动时间经过证明。
- **裸机目标。** 目前编译器构建的程序运行在 Linux、macOS 和 Windows 上。不依赖操作系统运行的程序（例如固件）在计划之中。

### 研究方向

尚未开始。每一项都建立在证明已经确立的东西之上。

- **安全的 GPU kernel。** 只有当任意两个线程都不写同一个元素时，kernel 才是正确的。Whitefoot 已经能证明同一个数组的 `[0, p)` 和 `[p + 1, n)` 这类区间互不重叠，上文的快速排序正是这样被 `--par` 拆分的。同样的证明可以说明每个 GPU 线程只写数组中属于自己的那一部分，包括根据线程下标计算出来的部分。Rust 的借用检查器看不出两个计算得到的区间互不相交，所以 Rust 写的 kernel 通常按固定的模式（比如等分成块）划分数据，或者使用 `unsafe`。
- **根据 profile 调整并行度。** 因为程序从不规定运行多少个任务，并行度可以根据 profile 针对具体负载调优，也可以在程序运行时调整，都不需要修改源代码。
- **由效应生成沙箱策略。** 程序经过检查的效应可以变成它的 seccomp 过滤器、WASI 能力集，或者文件与网络白名单，让部署后的程序只能做它的签名所声明的事。
- **常数时间代码。** 一套面向密码学代码的约束，保证秘密数据不会决定分支、地址，或者延迟可变的指令。
- **给 C 用的安全库。** 把 Whitefoot 模块作为带有不透明、经过校验的句柄的 C 头文件发布，让 C 程序可以用经过证明的代码替换其中风险最高的部分，比如解析器。

## 文章

短文，每篇只讲一个想法，附带能编译的程序。前三篇从上面的例子出发：

1. [要么证明，要么写分支](docs/articles/prove-it-or-write-a-branch.md)——边界检查和溢出检查因为被证明而消失。
2. [写顺序的代码，得到并行的结果](docs/articles/sequential-code-parallel-results.md)——编译器如何在普通代码（包括递归）中找到独立性，并把它交给工作线程。
3. [不用求解器，手工证明](docs/articles/proofs-by-hand.md)——差分约束（difference bounds）、约束闭包和循环不变式，在纸上演算一遍。
4. [拒绝告诉你什么](docs/articles/what-a-rejection-tells-you.md)——为负责修复代码的 agent 编写的诊断信息。
5. [整数](docs/articles/integers.md)——每个运算都写明自己的含义。
6. [只有一种构建](docs/articles/one-build.md)——没有 panic，没有 debug/release 之分，资源耗尽时输出固定的记录。
7. [从内存到资源](docs/articles/beyond-memory.md)——不用堆、线性资源，以及最高安全模式的计划。
8. [审查者读什么](docs/articles/what-a-reviewer-reads.md)——把契约和效应行作为审查的对象。
9. 可信基——哪些东西被信任，以及缩小它的计划。
10. [速度从哪里来](docs/articles/where-the-speed-comes-from.md)——证明被利用的每一种方式。
11. 不用 async 的 I/O——普通的调用，以及它们何时可以重叠。
12. 一个布局引擎——第一个大型程序。
13. 这个项目如何借助 agent 构建。

其余文章正在撰写中；文章发表后，对应的标题会变成链接。

## 试一试

你需要 Rust stable（版本不低于 [compiler/Cargo.toml](compiler/Cargo.toml) 中的 `rust-version`）和 clang：Linux 和 macOS 上是 `/usr/bin/clang`，Windows 上是 `PATH` 中的 `clang`。

```sh
git clone https://github.com/mbbill/Whitefoot.git && cd Whitefoot
cargo build --release --manifest-path compiler/Cargo.toml
compiler/target/release/whitefootc tests/programs/wfgrep.wf -o wfgrep
./wfgrep invariant tests/programs
compiler/target/release/whitefootc tests/conformance/cases/op4-neg-index-undischarged.wf
```

构建编译器大约需要一分钟，编译这个 grep 大约需要四秒。最后一条命令展示了一次拒绝：位置、引用的规则及其类别、标出问题的源代码行，以及每个载荷字段，都带有固定的标签。

```text
tests/conformance/cases/op4-neg-index-undischarged.wf:6:18: error[OP-4]: UndischargedBoundsObligation
  source:   return deref(b)[i];
  marker:                  ^^^
  residual: i < deref(b).len
  disposition: Unproved
  mechanical_fix: add `requires i < deref(b).len;` to the `contract` of `get`, which each caller then establishes; or guard the access with `if i < deref(b).len` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare
```

其他选项：

- `--par` 构建并行版本，`--par-ledger` 打印每一个并行决定及其理由。运行时由 `WF_WORKERS` 设置使用多少个工作线程；
- `--stack-ledger` 报告每个函数的栈帧，以及每个递归环能容纳多少层；
- `--emit-llvm` 输出 LLVM IR；
- `--diagnostic-format json` 把每次拒绝输出为一行一个的 JSON 对象；
- `whitefootc --help` 列出其余选项。

如果要参与语言或编译器的开发，请从 [AGENTS.md](AGENTS.md) 和[工作流程导览](docs/workflow.md)开始。

## 相关工作

| | 借鉴了什么 | 有什么不同 |
|---|---|---|
| Rust | 所有权、`Result`、没有 null | 源代码中没有 `unsafe`，也没有生命周期；边界和溢出靠证明，而不是在运行时检查 |
| C | 以函数和结构体为主要构件 | 没有未定义行为；每个部分操作（partial operation）都经过证明；枚举可以携带载荷 |
| SPARK | 证明不存在运行时错误 | 没有 SMT 求解器，被接受本身就是证明；固定过程证明不了的部分，写成显式的步骤 |
| Wuffs | 用证明检查器而不是求解器 | 通用语言，有堆上的数据和效应 |
| Astrée、Frama-C (Eva) | 固定的、保证终止的分析，不用求解器即可证明不存在运行时错误 | 它们在编译器之外分析 C 程序，编译器无论如何都会构建程序；在 Whitefoot 中，证明是编译的前提 |
| Dafny、Verus | 契约和不变式 | 目标是运行时安全，而不是完整的功能正确性 |

## 免责声明

Whitefoot 是一门研究用的语言和编译器，不是产品。不要把它用在任何重要的地方。

## 许可证

Whitefoot 以 [MIT 许可证](LICENSE)发布。
