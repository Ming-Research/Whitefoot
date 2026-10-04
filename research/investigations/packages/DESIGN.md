# Package bindings

## Problem

A Whitefoot program could read two packages: its own, whose root is the
directory of its `modules.wfg`, and the standard library the compiler ships.
Firn's scripting work needs a Lua engine, Halo, and JSON and MessagePack
codecs, each meant to become a separate project later. Under the v0.89 rules
firn's graph could not name them:

```
// apps/firn/modules.wfg
pkg::scripting: [halo::vm];   // v0.89: a path begins with pkg or std only
```

Moving them under `apps/firn/` would make them part of firn's package, and
moving them into `std` would make every program carry a Lua engine. The
earlier decision to defer package bindings (language/name-resolution) was
taken because no third-party library had been selected; Halo is that library.

## The rules

The graph binds a package by a location relative to its own directory, and
the name it chooses is a module prefix in its own package only:

```
// apps/firn/modules.wfg
package halo = "../../lib/halo";
package json = "../../lib/json";

pkg::scripting: [pkg::store, halo::vm, json, std::io];
```

```
// lib/halo/modules.wfg: Halo's own name for the same json package
package codec = "../json";

pkg::vm: [codec];
```

```
// lib/halo/vm/vm.wf: Halo's records name themselves pkg and json codec
alias Value = codec::Value;

fn box_number(n: i32) -> v: Value pure {
  return codec::make(n: n);
}
```

```
// apps/firn/scripting/script.wf
let v = halo::vm::box_number(n: 7_i32);
let n = number(v: v);   // number takes json::Value: the same type
```

[MOD-11] in the active specification states the rules; the owner's rulings
on 2026-10-04 chose each of them.

| Question | Ruling | Refused |
|---|---|---|
| Where a dependency is written and located (Q1) | a `package` line in `modules.wfg`, a relative location with `..` allowed | a separate manifest file; command-line or environment roots; absolute paths |
| What a package is (Q2) | its directory: every binding that reaches one directory binds one package | a declared name and version |
| Who names it (Q3) | the binding graph; the library calls itself `pkg`; not `pkg` or `std`, once per graph | a name the library declares |
| What a binding exposes (Q4) | every module the bound graph registers, public declarations only, not transitive | a package export list |
| Versions (Q5) | none: a binding reads what its directory holds | version constraints in the binding |

### The diamond decides identity

```
firn ──> halo ──> json
  └─────────────> json
```

Halo returns a `json` value; firn passes it to its own `json` functions. If
the two bindings bound two packages, `halo::vm::box_number` would return a
type firn's `json::Value` does not name, and every library that shares a
dependency with its user would force a conversion. Identity by directory
makes the two bindings one package with no version rule; a byte-identical
copy in another directory is another package, whose types differ
(`tests/conformance/cases/mod11-neg-distinct-directories-are-distinct-packages`).

### Labels

Each bound package's records need a logical path no other record has, and
diagnostics and symbols need a name for the package. A binding name cannot
serve directly: it is visible only in its own package, like an alias in its
file, so halo may call one directory `json` while the program calls another
directory `json` (`mod11-pos-one-name-two-packages`, where both packages
declare `make` at their roots).

The compiler labels a package by the name of the binding that first reaches
it in package order and, when an earlier package already holds that label,
appends `.2`, `.3` and on. Its records' logical paths are
`package/<label>/...` and its symbols begin `package.<label>.`: `package` is a
keyword, so no module directory of a program's own package can have that
name, `std/` is the standard library's prefix, and a label holding `.` is no
IDENT, so it never equals a module name. A label is never written in source.

The first draft instead required one name to mean one package across the
whole program, which made labels unique without a suffix. The owner ruled
against it on 2026-10-04 (Q6): a binding name is the binder's private choice,
as an alias is a file's, and two independently written libraries should not
have to agree on what they call their dependencies.

## Order and diagnostics

The package order is the program's own package, then each bound package in
the order a depth-first walk of the bindings first reaches it, visiting each
graph's bindings in written order. Every graph passes the syntax stages and
has its bindings judged in that order, a bound graph being read when a
binding first reaches it; cycles are found on the walk's stack. Rows and
entries are then judged in package order, since a row names the modules a
bound graph registers. The bound unit holds the program's modules, the bound
packages' modules in package order, then the selected standard library
modules, so a program record's ordinal never depends on what it binds.

## Implementation notes

- `compiler/src/driver/packages.rs` reads the packages; `graph.rs` forms each
  package's rows and assembles one module graph. Each module record carries
  its package's bindings, which resolution consults for a qualified path's
  root after file-local aliases.
- A bound package's items are keyed by its label, not by its place in
  package order, so binding one more package leaves every other package's
  proof receipts and verdicts reusable (compiler/incremental-compilation's
  stable identities).
- A check's graph facts include its modules' bindings: swapping two binding
  names changes what a bound package's records name while every edge, label
  and record byte stays the same
  (`driver::tests::swapping_binding_names_recomputes_a_bound_packages_verdict`,
  which fails with the bindings left out of the key).
- Symbols of a bound package begin `package.<label>.`.

## Validation

- Conformance: `mod11-pos-diamond-shares-one-package` builds and runs a
  three-package diamond, and `mod11-pos-one-name-two-packages` runs a program
  in which one name binds two packages in two graphs; eleven negative cases
  cover each MOD-11 refusal, the
  MOD-1 row path, the MOD-4 alias name, non-transitive visibility (MOD-5),
  directory identity (TYPE-5) and the FORM-2 layout of bindings.
- Input-envelope failures (a location reaching no directory, a package root
  without its graph record) were exercised by hand; they are not
  source-language verdicts and have no conformance case. A symbolic link at
  a package root is refused by the same discovery check as a program root's,
  not exercised separately.
