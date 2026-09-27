# Bounds-check spellings: Rust and Whitefoot

Dated 2026-09-25. This record supports the Rust comparison in the root
README's [first example](../../../README.md#a-bounds-check-proved-away).

## Question

The loop keeps the non-space bytes of a buffer in place. Its store
`buf[kept] = b` is always in range because `kept` never passes `i`, but seeing
that takes induction over the loop. Which ways of writing the loop in Rust
leave a compare of `kept` against the buffer length in the compiled loop,
what does that compare lead to, and which spellings that remove it need
`unsafe`? What does the Whitefoot version with the invariant
`kept <= i` compile to?

## Environment

- rustc 1.98.1 (48a229cea 2026-09-01), LLVM 22.1.8, host and target
  `x86_64-unknown-linux-gnu`; flags `--edition 2021 --crate-type lib
  -C opt-level=2` and `-C opt-level=3`, `--emit asm`.
- Ubuntu clang 18.1.3 (1ubuntu1), target `x86_64-pc-linux-gnu`; `-O2 -S` on
  the LLVM module that `whitefootc --emit-llvm` writes. `-O2` is the level
  `whitefootc` links every executable with (`HOST_OPTIMIZATION_ARGUMENTS` in
  `compiler/src/driver.rs`).
- `whitefootc` built from the compiler sources of `main` at `da368080f` with
  the `gate` Cargo profile, which changes how the compiler itself is compiled
  and not the LLVM it emits.

## Method

[`spellings.rs`](spellings.rs) holds eleven spellings, `s1` to `s11`, each a
`#[no_mangle]` function so that its loop stays a separate symbol. `s1` to `s9`
keep the loop's shape; `s10` and `s11` change the algorithm.
[`squeeze.wf`](squeeze.wf) holds the README's Whitefoot function and a `main`
that squeezes `" h i "` to `"hi"` and exits 0 only when the count and both kept
bytes are right; `wf_squeeze` has external linkage in the emitted module, so
it also stays a separate symbol.

```sh
WHITEFOOTC=<repository-root>/compiler/target/release/whitefootc \
  research/experiments/bounds-check-spellings/reproduce.sh <scratch-root>/bcs
```

[`reproduce.sh`](reproduce.sh) writes `spellings-O2.s`, `spellings-O3.s`,
`squeeze.ll`, `squeeze.s`, the rejection of the function without its invariant
(`rejection.txt`) and the tool versions. Each function's loop was read by hand
in both Rust listings and in `wf_squeeze`.

Every loop also compares its counter with the end of its range; that is the
loop's own exit test, present in the Whitefoot loop as well, and is not
counted below. A *compare against the length* means a compare of `kept`, the
store index, with the buffer length.

## Results

The two optimization levels give the same answer for every spelling. `-C
opt-level=3` emits the same code as `-C opt-level=2` for `s1` to `s7`; in
`s8`, `s9` and `s10` it duplicates one byte test (`cmpb $32`) of the unrolled
loop and adds no other compare, and in `s11` it duplicates the block that
clamps the copy length before `memcpy`.

| Function | Spelling | Compare against the length in the loop | Where the compare leads | Needs `unsafe` |
|---|---|---|---|---|
| `s1_index` | `buf[kept] = b` | yes | `panic_bounds_check` | no |
| `s2_assert_in_loop` | `assert!(kept <= i)` at the top of the loop body | no; `kept` is compared with `i` instead | `core::panicking::panic` from `assert!` | no |
| `s3_get_mut` | `if let Some(slot) = buf.get_mut(kept)` | yes | a conditional skip of the store | no |
| `s4_branchless` | unconditional `buf[kept] = b`, `kept += (b != b' ') as usize` | yes, on every byte | `panic_bounds_check` | no |
| `s5_while` | `while i < buf.len()` | yes | `panic_bounds_check` | no |
| `s6_enumerate_copy` | `buf.copy_within(i..i + 1, kept)` | yes, against `len - 1` | `panic_fmt` from `copy_within` ("dest is out of bounds") | no |
| `s7_cells` | `Cell::from_mut(buf).as_slice_of_cells()` | yes | `panic_bounds_check` | no |
| `s8_unsafe_unchecked` | `unsafe { *buf.get_unchecked_mut(kept) = b }` | no | none | yes |
| `s9_unsafe_assume` | `unsafe { std::hint::assert_unchecked(kept <= i) }` | no | none | yes |
| `s10_vec_retain` | `v.retain(\|&b\| b != b' ')` on `&mut Vec<u8>` | no | none | no |
| `s11_filter_copy_back` | `collect` the non-space bytes into a new `Vec<u8>`, then `zip` it with `buf.iter_mut()` | no | none | no |
| `wf_squeeze` | Whitefoot, `invariant behind: kept <= i` | no | none | no |

The key instructions, from `spellings-O2.s` and `squeeze.s`, with comments
added and the register roles read from each function (`%rsi` holds the slice length; the
store index is `%rax` or `%rcx`):

`s1_index`; `s5_while` and `s7_cells` have the same three lines with their
own labels:

```text
	cmpq	%rsi, %rax
	jae	.LBB1_7          # calls panic_bounds_check
	movb	%dl, (%rdi,%rax)
```

`s2_assert_in_loop`, where the assertion's compare replaces the bounds check
(`%rcx` is `i`):

```text
	cmpq	%rcx, %rax
	ja	.LBB2_7          # calls core::panicking::panic, "assertion failed: kept <= i"
	...
	movb	%dl, (%rdi,%rax)
```

`s3_get_mut`, where the store is skipped when either the byte is a space or
`kept` is not below the length:

```text
	cmpb	$32, %r8b
	setne	%r9b
	cmpq	%rsi, %rax
	setb	%r10b
	testb	%r10b, %r9b
	je	.LBB3_10
	movb	%r8b, (%rdi,%rax)
```

`s4_branchless`, where the check runs on every byte because every byte is
stored (`%rcx` is `kept`):

```text
	cmpq	%rsi, %rcx
	jae	.LBB4_6          # calls panic_bounds_check
	movzbl	(%rdi,%rdx), %r8d
	incq	%rdx
	movb	%r8b, (%rdi,%rcx)
```

`s6_enumerate_copy`, with `%rcx = len - 1` set before the loop
(`leaq -1(%rsi), %rcx`):

```text
	cmpq	%rcx, %rax
	ja	.LBB6_8          # calls panic_fmt, "dest is out of bounds"
	movb	%r8b, (%rdi,%rax)
```

`s8_unsafe_unchecked`, and `s9_unsafe_assume` with the same lines, unrolled
twice:

```text
	movzbl	(%rdi,%rcx), %r8d
	cmpb	$32, %r8b
	je	.LBB8_10
	movb	%r8b, (%rdi,%rax)
	incq	%rax
```

`wf_squeeze`, unrolled twice like `s8`:

```text
	movzbl	(%rdi,%rcx), %r8d
	cmpb	$32, %r8b
	je	.LBB2_11
	movb	%r8b, (%rdi,%rax)
	incq	%rax
```

The emitted LLVM for `wf_squeeze` has no compare on the store index either:
the proof leaves no check to optimize away, and `kept + 1` is emitted as
`add nuw` because it is proved not to overflow.

`Vec::retain` (`s10`) first scans to the first space, then moves the kept
bytes down with no compare of the write index; only an owned `Vec` has it, and
a `&mut [u8]` borrowed from a caller has no `retain`.

`s11` works on the borrowed slice by giving up the single in-place pass. It
allocates a vector (`__rust_alloc`, growing it through `reserve` as bytes are
kept), copies it back with one `memcpy` whose length is the smaller of the two
lengths (`cmpq`, `cmovbq`), and frees it. No index is compared with the
length, but the function allocates, reads the kept bytes twice, and has a path
for a failed allocation (`handle_error`) that the in-place loops do not have.

Without its invariant the Whitefoot function is rejected. Canonical layout
puts a loop header without an invariant on one line, so the header becomes
`for (i in 0_u64..deref(buf).len) {` and the store moves to line 6
(`rejection.txt`):

```text
squeeze.wf:6:21: error[OP-4]: UndischargedBoundsObligation
  source:       set deref(buf)[kept] = byte;
  marker:                     ^^^^^^
  residual: kept < deref(buf).len
  mechanical_fix: when the relation must hold, establish the residual with a verified requirement, a source invariant, or explicit finite proof steps; use a dominating branch only when its false edge is intended program behavior; otherwise restructure the access
```

Deleting only the `invariant` line leaves a trailing comma, a GRAM-5 parse
error; keeping the header on three lines without an invariant is a FORM-2
layout error.

## Limitations

- One loop, one target (x86-64), one rustc and one clang version. Another
  loop, target or LLVM version can keep or remove different checks.
- Code generation only. Whether a compare that is left costs measurable time
  depends on branch prediction and on the surrounding program; no timing is
  claimed here.
- The Rust functions take `&mut [u8]` and are compiled as a library, with no
  caller for the optimizer to specialize on; an inlined call with a known
  length could compile differently.
