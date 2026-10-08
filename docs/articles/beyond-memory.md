# Beyond memory

A program uses more than the memory it is given. It takes memory from the
heap, opens files and connections, and grows its stack. Whitefoot's proofs
start with memory safety, and the aim is to carry them to these resources
too. Two parts are checked today: a program can declare that it uses no
heap, and a resource that must be released cannot be lost. This article
shows both, and then the plan for the rest: a maximum-safety mode that would
also prove how much stack a program uses, that it finishes, and how it uses
its devices.

Opening a file and not closing it:

```text
C          fopen(path, "r")     a path that never calls fclose leaks the file
Rust       File::open(path)?    the file is closed when its owner goes out of
                                scope, and an error from closing is ignored
Whitefoot  open_read(...)       a path that loses the file does not compile
```

## 1. A program without a heap

A program that begins with `program no_heap;` cannot allocate. The compiler
rejects every heap type and every call that allocates. The heap type is
`Box`. An `Array`, `Slots` or `Ring` whose capacity is chosen at run time
lives only inside a `Box`, so it goes too.

```
program no_heap;

fn total(n: u64) -> sum: u64 pure contract {
  requires n <= 1024_u64;
} {
  let cells = box_array_filled::<u64>(count: n, value: 1_u64);
  let sum = 0_u64;
  for (i in 0_u64..cells.inner.len) {
    set sum = sum +wrap cells.inner[i];
  }
  return sum;
}
```

```text
noheap.wf:9:15: error[STOR-8]: HeapTypeUnderNoHeap
  source:   let cells = box_array_filled::<u64>(count: n, value: 1_u64);
  marker:               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  spelling: box_array_filled
  mechanical_fix: use a constant-capacity shape, or withdraw the no-heap declaration
```

Without the first line, this function is accepted. The repair offers two
routes: withdraw the declaration, or use a shape whose capacity is a
constant. Such an array lives in the function's frame, and keeping one there
is not an allocation. The largest count the function accepts becomes the
array's size:

```
fn total(n: u64) -> sum: u64 pure contract {
  requires n <= 1024_u64;
} {
  let cells = array_filled::<u64, 1024>(value: 1_u64);
  let sum = 0_u64;
  for (i in 0_u64..n) {
    set sum = sum +wrap cells[i];
  }
  return sum;
}
```

This version is accepted and runs. The index is proved as usual: the loop
gives `i < n`, the requirement gives `n <= 1024`, and together they give
`i < cells.len`.

Many programs need no heap at all. With `program no_heap;` added and nothing
else changed, the [DEFLATE decoder](../../tests/programs/raw_deflate.wf) in
the compiler's tests, about a thousand lines in three files, compiles and
passes its test vectors, and so do the SHA-256, IPv4 checksum and UTF-8
decoding programs next to it.

A program built from modules states the requirement on an entry, in its
module graph `modules.wfg`, instead of on the whole program:

```text
entry small = pkg::main {
  no_heap;
}
```

The compiler then follows the calls from that entry and rejects the first
function they reach that uses the heap, with the path of calls that reaches
it:

```text
small: rejected: tests/conformance/cases/stor8-neg-entry-closure-allocates/main.wf:4:4: error[STOR-8]: HeapInClosure
  source: fn boxed() -> result: u8 pure {
  marker:    ^^^^^
  path: [pkg::main, pkg::boxed]
small: further failures may follow the last listed one
whitefootc: rejected: small
```

The same module is accepted when `main` does not call `boxed`
([`stor8-pos-unused-allocating-helper`](../../tests/conformance/cases/stor8-pos-unused-allocating-helper/main.wf)):
a function the entry cannot reach may allocate, and another entry of the
same modules may use the heap freely.

## 2. Resources that must be released

A type has two capabilities, copy and drop. A type with both, such as `u64`,
is *copy*. A type with drop alone is *affine*: a value is moved at most once,
and if it is still there when its owner's scope ends, the compiler releases
it, as it frees a `Box`. A type with neither is *linear*: the compiler never
releases the value, so the program must consume it exactly once. It leaves a
scope only by being moved out whole, into a function that takes it by value
or back to the caller, or by being taken apart.

The standard library's files, directories, directory listings, listeners
and the two halves of a connection are linear. Each is declared with the
`nodrop` modifier, as `std::fs` declares a file opened for reading:

```
public opaque nodrop struct ReadFile {
}
```

This function reads the first byte of a file and forgets to close it when
the read fails:

```
fn first_byte(files: &HandleFactory, root: &DirectoryRead, path: &RelativePath) -> result: Option<u8> reads(root), reads(path), writes(files) {
  match open_read(factory: files, root: root, path: path) {
    Ok(value: file) => {
      let bytes = array_filled::<u8, 1>(value: 0_u8);
      let window = &bytes[0_u64..1_u64];
      match read_at(factory: files, file: &file, destination: window, file_offset: 0_u64, start: 0_u64, end: 1_u64) {
        Ok(value: next) => {
          close_read(factory: files, file: move file);
          return Some<u8>(value: bytes[0_u64]);
        }
        Err(error: stop) => {
          return None<u8>();
        }
      }
    }
    Err(error: problem) => {
      return None<u8>();
    }
  }
}
```

```text
leak.wf:21:11: error[PROV-6]: LinearValueNotConsumed
  source:           return None<u8>();
  marker:           ^^^^^^^^^^^^^^^^^^
  binding: file
  obligation: ReadFile
  mechanical_fix: move the value out whole, or take it apart with let N(f: a, ...) = move v;
```

The rejection is at the `return` that would leave the scope with `file`
still open, and `obligation` names the declaration that makes it linear. Of
the two routes the repair offers, only the first applies here: `ReadFile` is
opaque, so no program can take it apart, and the file leaves by being moved
whole. Here that means closing it in the failure arm too:

```
        Err(error: stop) => {
          close_read(factory: files, file: move file);
          return None<u8>();
        }
```

With that line the function is accepted. `close_read` takes the file by
value, so the call consumes it, and it returns `Result<unit, IoError>`: a
failure to close is a value the caller can match, like any other.

The check covers every path, including the paths that meet again after a
branch. Closing a file on one side of an `if` and not on the other is
rejected where the two sides join:

```
fn maybe_close(files: &HandleFactory, file: ReadFile, now: Bool) -> result: unit writes(files) {
  if now {
    close_read(factory: files, file: move file);
  }
  return unit;
}
```

```text
join.wf:8:3: error[LIV-1]: LivenessJoinDisagreement
  source:   if now {
  marker:   ^^^^^^^^
  binding: file
  live_predecessor: the `else` branch
  dead_predecessor: the `if` branch
  mechanical_fix: every predecessor of a join agrees on a binding's live-or-dead status: consume it on every predecessor, on none, or commit a value back into it before the predecessor that consumed it reaches the join
```

Closing a file twice is rejected too: the first `close_read` consumed
`file`, so the second `move file` is a use after a move (OWN-1).

Every program that takes the standard `Inputs` meets this rule at once. The
directory the program starts in, `Inputs.cwd`, is a `DirectoryRead`, so
`main` takes `Inputs` apart and closes it:

```
let Inputs(args: args, cwd: cwd, stdout: out, stderr: err, handles: files, stdin: input) = move inputs;
close_directory(factory: &files, directory: move cwd);
```

Leaving out the second line is rejected at `main`'s `return`, naming `cwd`.

Closing is an ordinary call written in the source, not code that the
compiler adds at the end of a scope. A reader sees every place a resource is
released, and the compiler checks that there is one on every path.

## 3. Linear types of your own

`nodrop` is not reserved for the standard library. Any struct or enum can
carry it, and the same check then applies to its values. A request that
must be answered can be one:

```
nodrop struct Pending {
  id: u64;
}

fn respond(request: Pending, code: u16) -> result: u64 pure {
  let Pending(id: id) = move request;
  return id;
}

fn handle(id: u64, valid: Bool) -> result: u64 pure {
  let request = Pending(id: id);
  if valid {
    return respond(request: move request, code: 200_u16);
  }
  return 0_u64;
}
```

```text
pending.wf:18:3: error[PROV-6]: LinearValueNotConsumed
  source:   return 0_u64;
  marker:   ^^^^^^^^^^^^^
  binding: request
  obligation: Pending
  mechanical_fix: move the value out whole, or take it apart with let N(f: a, ...) = move v;
```

The path that rejects the request without answering it does not compile.
`respond` ends the obligation by taking the value apart, which is the
repair's second route; the first is to pass the request to `respond`.

The class follows ownership. A struct, an enum or an array that holds a
linear value is linear itself. A container whose elements are linear is
emptied element by element, and its storage is then released by
`free_empty`, whose requirement is that the container is proved empty. A
generic function states what it does with a value of a type parameter `T`:
`T: copy` may copy it, `T: drop` may drop it, and with no bound the body must
consume it exactly once, so a linear argument is accepted only where no bound
is written.

## 4. What is not proved yet

- **The declaration covers the program's own code.** The runtime that the
  compiler links into every program still reserves memory of its own, such
  as the 1 GiB stack it runs the program on.
- **The stack is reported, not proved.** `--stack-ledger` shows how deep
  each recursion can go before the stack runs out, and running out stops the
  program with a fixed record ([One build](one-build.md)). No program is
  rejected for how much stack it could use.
- **Loops and recursion are not proved to finish.** This program declares no
  heap, and its recursion counts `remaining` down to zero:

  ```
  program no_heap;

  fn depth(remaining: u64, acc: u64) -> result: u64 pure {
    if remaining == 0_u64 {
      return acc;
    }
    let next = remaining - 1_u64;
    let a = acc +wrap 3_u64;
    let d = depth(remaining: next, acc: a);
    return d *wrap a;
  }
  ```

  Writing `remaining` instead of `next` in the recursive call is accepted as
  well, and then a call with a positive `remaining` never returns. It does
  not even run out of stack: the optimizer turns this recursion into a loop.

## 5. The plan: a maximum-safety mode

The [README](../../README.md#beyond-memory-resources) lists what a program
compiled in the planned maximum-safety mode would have:

- no heap and no other dynamic resource;
- a peak stack proved to fit a capacity given in bytes;
- every loop and every recursion proved to finish;
- hardware peripherals mapped as linear values, so that a device is owned,
  used and released under the same proofs as a file;
- no parallelism scheduled at run time;
- proved bounds on how long each peripheral takes to respond and how long
  the program takes to start.

None of this mode is implemented. The [fixed-resource
investigation](../../research/investigations/fixed-resource-execution/README.md)
studied the first three and is paused. It records what exists, what each
part still needs, and where to resume:

- **Termination.** Its proposal gives each recursive function, and each
  `loop` that is not a counted `for`, a rank: an integer proved nonnegative
  and proved smaller at every recursive call and at every return to the
  loop's header. The comparisons are proved by the same checker as every
  other goal in this article; what is new is checking that every cycle has a
  rank. With a rank on `remaining`, the call that passes `remaining`
  unchanged would be rejected.
- **Stack.** The deployment would give the stack's size in bytes, and the
  compiler would prove that every run fits it. The proof combines a bound on
  how deep each recursion goes, from its rank, with the frame sizes of the
  machine code that is actually delivered, including the runtime and every
  linked function.
- **No dynamic resource.** `program no_heap;` covers the program's code. The
  mode would also have to account for the runtime and every function linked
  in, and for any memory the deployment hands the program before it starts.

Peripherals as linear values would carry the idea of section 2 over to
hardware: a device the program owns would be a value it must release exactly
once. That part, like run-time parallelism and the timing bounds, is not
designed yet.

## The rules behind each step

| Step | Rule in the [specification](../../spec/kernel-spec.md) or design |
|---|---|
| `program no_heap;` and entry `no_heap`; allocation is total | STOR-8, MOD-9 |
| Constant-capacity arrays live in the frame; runtime-capacity ones only in a `Box` | TYPE-9, STOR-1 |
| Copy, affine and linear; `nodrop` | OWN-1, PROV-6 |
| A linear value leaves a scope moved whole or taken apart | PROV-6, WIN-3 |
| An opaque struct cannot be taken apart | TYPE-2 |
| Every join agrees on what is live | LIV-1 |
| The standard library's handles and `Inputs` | PRE-2 |
| Handles close through explicit calls, with no hidden finalizer | [`design/language/system-interface.md`](../../design/language/system-interface.md) |
| An emptied container is released by `free_empty` | OP-14 |
| Resource exhaustion stops the program, outside its meaning | SCOPE-3 |

Every program in this article is accepted, or rejected as shown, by the
compiler at commit `e041772d8`, with `whitefootc --check file.wf`; the module
program is checked with `whitefootc --check --graph modules.wfg --entry
small`. Each fragment is compiled inside a file that begins with one `alias`
line for each standard library name it uses, including `ExitStatus` and
`exit_status`, in sorted order, then a blank line, and that ends with a
`main` returning `exit_status(code: 0_u8)`; where `program no_heap;` is
shown, it comes first. The second `total` was also built and run under
`program no_heap;`, from a `main` that checks that `total(n: 8_u64)` is 8,
and so were the DEFLATE decoder and the other test programs named in
section 1, with the declaration added; all exit with 0. The `depth` that
passes `remaining` was run from a `main` that calls it with 32, and was still
running when it was stopped after 20 seconds.
