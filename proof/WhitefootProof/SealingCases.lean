import WhitefootProof.Sealing

/-!
# Sealed type invariants: worked cases

The sealing component on event sequences taken from conformance cases of
TYPE-11 that firn-req-invariant-through-fields adds, each with the result its
case requires: a sequence the rules reject gives none, an accepted one a
state. `#guard` evaluates each when this file builds and fails the build on a
different result. A sequence keeps only the events the sealing rules see, in
their order; where a case reads a `Slots` window or a cursor's field, the
sequence reads an `Array` or a local, which the component's places cover.
-/

namespace Whitefoot.Seal.Cases

/-- A term of the cases: a numeral, or a value read from a binding, named by
the binding and a label. -/
inductive Term where
  | num (n : Nat)
  | read (root label : String)
  deriving DecidableEq, Repr

/-- The binding a term reads. -/
def Term.root? : Term → Option String
  | .num _ => none
  | .read x _ => some x

/-- A fact state of the cases: the pairs of terms it proves distinct, and the
instances it proves over spelled places. -/
structure Facts where
  ne : List (Term × Term) := []
  inst : List (Place Term × InvId) := []

/-- The fact core of the cases: two numerals are distinct when they differ, and
a term is killed when its batch writes or ends the binding it reads. -/
def core : Core Term Facts where
  distinct f s t :=
    (match s, t with
     | .num a, .num b => a != b
     | _, _ => false) || f.ne.contains (s, t) || f.ne.contains (t, s)
  proves f p I := f.inst.contains (p, I)
  killed _ kb t :=
    match t.root? with
    | some x => kb.writes.any (·.root == x) || kb.ended.contains x
    | none => false
  tracked t := t.root?.isSome

/-! ## Declarations -/

/-- `struct Index { value: u64; invariant bounded(index): index.value < 16_u64; }` -/
def index : StructDecl :=
  ⟨"Index", [.int], [⟨"bounded", .datum [.field 0] 0, .lt, .const 16⟩]⟩

def bounded : InvId := ⟨"Index", "bounded"⟩

/-- `nocopy struct Index { value: u64; }`: `Index` without its invariant. -/
def plainIndex : StructDecl := ⟨"Index", [.int], []⟩

/-- `struct Holder { index: Index; invariant bounded(holder):
holder.index.value < 16_u64; }` -/
def boundedHolder : StructDecl :=
  ⟨"Holder", [.strct "Index"], [⟨"bounded", .datum [.field 0, .field 0] 0, .lt, .const 16⟩]⟩

def holderBounded : InvId := ⟨"Holder", "bounded"⟩

/-- `struct Holder { index: Index; }` -/
def holder : StructDecl := ⟨"Holder", [.strct "Index"], []⟩

/-- `struct Node { next: u32; invariant link(node): node.next < 64_u32; }` -/
def node : StructDecl := ⟨"Node", [.int], [⟨"link", .datum [.field 0] 0, .lt, .const 64⟩]⟩

def link : InvId := ⟨"Node", "link"⟩

/-- A body's sealing context: these structs, and these roots' declared types. -/
def ctx (structs : List StructDecl) (roots : List (String × STy)) : Ctx Term Facts :=
  ⟨⟨structs, []⟩, fun x => (roots.find? (·.1 == x)).map (·.2), core⟩

/-! ## Events -/

/-- The place `x` followed by steps. -/
def var (x : String) (steps : List (SStep Term) := []) : Place Term := ⟨.var x, steps⟩

/-- The place `q^` followed by steps. -/
def deref (q : String) (steps : List (SStep Term) := []) : Place Term := ⟨.deref q, steps⟩

/-- The step `[t]`. -/
def ix (t : Term) : SStep Term := .elem (.cap t)

/-- The kill batch of a statement that writes these bindings. -/
def writing (roots : List String) : Kills Term := ⟨roots.map fun x => ⟨x, []⟩, [], []⟩

/-- The kill batch of a statement that rebinds these reference variables. -/
def rebinding (qs : List String) : Kills Term := ⟨[], [], qs⟩

/-- A kill batch that kills nothing. -/
def quiet : Kills Term := ⟨[], [], []⟩

/-- The fact state that proves nothing. -/
def noFacts : Facts := {}

/-- The state at the start of a body: no debt and no reference. -/
def start : St Term := ⟨[], []⟩

/-- The state after a sequence of events, each judged in its fact state; none
when the rules reject one. -/
def run (X : Ctx Term Facts) (s : St Term) (es : List (Facts × Event Term)) :
    Option (St Term) :=
  es.foldlM (fun t e => X.step e.1 t e.2) s

/-! ## A reference that may name either of two bindings -/

/-- Two `Index` bindings. -/
def pair : Ctx Term Facts := ctx [index] [("left", .strct "Index"), ("right", .strct "Index")]

/-- Two `Holder` bindings whose `Index` has no invariant, and a spare `Index`. -/
def holders : Ctx Term Facts :=
  ctx [plainIndex, boundedHolder]
    [("left", .strct "Holder"), ("right", .strct "Holder"), ("spare", .strct "Index")]

/-- The state after `let left = ...; let right = ...; let target = &left;
if flag { set target = &right; }`: `target` may name `right` or `left`. -/
def picked (X : Ctx Term Facts) : Option (St Term) := do
  let s ← run X start [
    (noFacts, .bind "left" .fresh quiet),
    (noFacts, .bind "right" .fresh quiet),
    (noFacts, .bindRef "target" (var "left") (rebinding ["target"]))]
  let taken ← run X s [(noFacts, .bindRef "target" (var "right") (rebinding ["target"]))]
  pure (X.join [(noFacts, taken), (noFacts, s)])

/-- The facts after `set target^.value = 7_u64;`: the instance at `target^`. -/
def targetProved : Facts := { inst := [(deref "target", bounded)] }

/-- The facts after `set target^.index.value = 7_u64;`. -/
def holderProved : Facts := { inst := [(deref "target", holderBounded)] }

/-- `type11-neg-joined-reference-store-keeps-debt`: `set right.value = 31_u64;`
breaks `right`, and `set target^ = Index(value: 4_u64);` may store into `left`
instead, so the debt at `right` stays and handing `right` off is rejected. -/
def joinedStore : Option (St Term) := do
  let s ← picked pair
  run pair s [
    (noFacts, .store (var "right" [.field 0]) .fresh (writing ["right"])),
    (noFacts, .store (deref "target") .fresh (writing ["right", "left"])),
    (noFacts, .call [var "right"] [] quiet)]

#guard joinedStore.isNone

/-- `type11-pos-joined-reference-store-restores`: `set target^.value = 31_u64;`
breaks whichever binding `target` names, each debt recording `target`, and
`set target^ = fresh();` then removes both. -/
def joinedStoreRestores : Option (St Term) := do
  let s ← picked pair
  run pair s [
    (noFacts, .store (deref "target" [.field 0]) .fresh (writing ["right", "left"])),
    (noFacts, .store (deref "target") .fresh (writing ["right", "left"])),
    (noFacts, .call [var "left"] [] quiet),
    (noFacts, .call [var "right"] [] quiet)]

#guard joinedStoreRestores.isSome

/-- The same without `set target^ = fresh();`: the debt at `left` stays. -/
def joinedWriteOnly : Option (St Term) := do
  let s ← picked pair
  run pair s [
    (noFacts, .store (deref "target" [.field 0]) .fresh (writing ["right", "left"])),
    (noFacts, .call [var "left"] [] quiet)]

#guard joinedWriteOnly.isNone

/-- `type11-neg-reference-rebound-before-restored`: `set target = &left;`
rebinds `target` while debts record it, so they record no reference after it,
and the instance at `target^` that the facts prove after `set target^.value =
7_u64;` does not restore the debt at `right`. -/
def reboundBeforeRestored : Option (St Term) := do
  let s ← picked pair
  run pair s [
    (noFacts, .store (deref "target" [.field 0]) .fresh (writing ["right", "left"])),
    (noFacts, .bindRef "target" (var "left") (rebinding ["target"])),
    (noFacts, .store (deref "target" [.field 0]) .fresh (writing ["left"])),
    (targetProved, .call [var "right"] [] quiet)]

#guard reboundBeforeRestored.isNone

/-- `type11-neg-swap-after-joined-write-restore-keeps-debt`: the swap exchanges
the debts that record `target` onto the other binding, recording no
reference, so the instance at `target^` does not restore the debt at
`right`. -/
def swapThenRestore : Option (St Term) := do
  let s ← picked pair
  run pair s [
    (noFacts, .store (deref "target" [.field 0]) .fresh (writing ["right", "left"])),
    (noFacts, .swap (var "left") (var "right") (writing ["left", "right"])),
    (noFacts, .store (deref "target" [.field 0]) .fresh (writing ["right", "left"])),
    (targetProved, .call [var "right"] [] quiet)]

#guard swapThenRestore.isNone

/-- `type11-neg-swap-after-joined-write-store-keeps-debt`: after the same swap,
`set target^ = fresh();` removes no debt, since none records `target`. -/
def swapThenStore : Option (St Term) := do
  let s ← picked pair
  run pair s [
    (noFacts, .store (deref "target" [.field 0]) .fresh (writing ["right", "left"])),
    (noFacts, .swap (var "left") (var "right") (writing ["left", "right"])),
    (noFacts, .store (deref "target") .fresh (writing ["right", "left"])),
    (targetProved, .call [var "right"] [] quiet)]

#guard swapThenStore.isNone

/-- `type11-pos-joined-reference-swap-write-restored`: `swap(first:
&target^.index, second: &spare);` breaks the `Holder` that `target` names,
each debt recording `target`, and the instance at `target^` the facts prove
after `set target^.index.value = 7_u64;` restores the debt at `right`. -/
def swapWrite (atCall : Facts) : Option (St Term) := do
  let s ← picked holders
  run holders s [
    (noFacts, .bind "spare" .fresh quiet),
    (noFacts, .swap (deref "target" [.field 0]) (var "spare") (writing ["right", "left", "spare"])),
    (noFacts, .store (deref "target" [.field 0, .field 0]) .fresh (writing ["right", "left"])),
    (atCall, .call [var "right"] [] quiet)]

#guard (swapWrite holderProved).isSome
#guard (swapWrite noFacts).isNone

/-- `type11-pos-joined-reference-call-write-restored`: `spoil(index:
&target^.index);` writes `index.value` below its parameter, which breaks the
`Holder` that `target` names; the facts after `set target^.index.value =
7_u64;` restore the debt at `right`. -/
def callWrite (atCall : Facts) : Option (St Term) := do
  let s ← picked holders
  run holders s [
    (noFacts, .call [deref "target" [.field 0]] [(deref "target" [.field 0], [.field 0])]
      (writing ["right", "left"])),
    (noFacts, .store (deref "target" [.field 0, .field 0]) .fresh (writing ["right", "left"])),
    (atCall, .call [var "right"] [] quiet)]

#guard (callWrite holderProved).isSome
#guard (callWrite noFacts).isNone

/-! ## Swaps of elements -/

/-- An `Array` of two `Index` values and a spare `Index`. -/
def indexArray : Ctx Term Facts :=
  ctx [index] [("indexes", .array (.strct "Index") 2), ("x", .strct "Index")]

def iTerm : Term := .read "i" "i"

def jTerm : Term := .read "j" "j"

/-- `type11-neg-swap-possible-alias-keeps-debt`, over an `Array`: after `let r =
&indexes[j]; set r^.value = 31_u64;`, `swap(first: &indexes[i], second: &x);`
may or may not take the broken element, so its debt stays and is also copied
onto `x`. -/
def aliasSwapped : Option (St Term) :=
  run indexArray start [
    (noFacts, .bind "indexes" .fresh quiet),
    (noFacts, .bindRef "r" (var "indexes" [ix jTerm]) (rebinding ["r"])),
    (noFacts, .store (deref "r" [.field 0]) .fresh (writing ["indexes"])),
    (noFacts, .bind "x" .fresh quiet),
    (noFacts, .swap (var "indexes" [ix iTerm]) (var "x") (writing ["indexes", "x"]))]

#guard (aliasSwapped.bind fun s => run indexArray s [(noFacts, .call [var "indexes"] [] quiet)]).isNone
#guard (aliasSwapped.map fun s => indexArray.instance? noFacts s (var "x") bounded) == some false

/-- `type11-pos-swap-elements`, over an `Array`: `swap(first: &indexes[0_u64],
second: &indexes[1_u64]);` exchanges two elements the facts prove apart. -/
def swapElements : Option (St Term) :=
  run indexArray start [
    (noFacts, .bind "indexes" .fresh quiet),
    (noFacts, .swap (var "indexes" [ix (.num 0)]) (var "indexes" [ix (.num 1)])
      (writing ["indexes"])),
    (noFacts, .call [var "indexes"] [] quiet)]

#guard swapElements.isSome

/-! ## Copies, stores and joins -/

/-- An `Index` inside a `Holder` without an invariant of its own, and a copy. -/
def held : Ctx Term Facts := ctx [index, holder] [("holder", .strct "Holder"), ("c", .strct "Index")]

/-- After `set holder.index.value = 31_u64; let c = holder.index;`, from
`type11-neg-stored-field-corrupt-then-read` with the read copied into a
binding. -/
def copied : Option (St Term) :=
  run held start [
    (noFacts, .bind "holder" .fresh quiet),
    (noFacts, .store (var "holder" [.field 0, .field 0]) .fresh (writing ["holder"])),
    (noFacts, .bind "c" (.read (var "holder" [.field 0]) false) quiet)]

-- The broken `holder.index` gives no instance, and neither does its copy.
#guard (copied.map fun s => held.instance? noFacts s (var "holder" [.field 0]) bounded) == some false
#guard (copied.map fun s => held.instance? noFacts s (var "c") bounded) == some false

/-- `set holder.index = Index(value: 4_u64);` after `copied`. -/
def replaced : Option (St Term) :=
  copied.bind fun s =>
    run held s [(noFacts, .store (var "holder" [.field 0]) .fresh (writing ["holder"]))]

-- The store removes the debt at `holder.index`; the copy's stays.
#guard (replaced.map fun s => held.instance? noFacts s (var "holder" [.field 0]) bounded) == some true
#guard (replaced.map fun s => held.instance? noFacts s (var "c") bounded) == some false

/-- One `Index` binding. -/
def single : Ctx Term Facts := ctx [index] [("idx", .strct "Index")]

/-- `if flag { set idx.value = 31_u64; ... }` and then handing `idx` off: the
edge from the branch offers its debt for restoration in its own facts
(`type11-neg-branch-join-broken`, `type11-pos-branch-join-restored`). -/
def branch (stores : Nat) (atJoin : Facts) : Option (St Term) := do
  let s ← run single start [(noFacts, .bind "idx" .fresh quiet)]
  let t ← run single s
    (List.replicate stores (noFacts, Event.store (var "idx" [.field 0]) .fresh (writing ["idx"])))
  run single (single.join [(atJoin, t), (noFacts, s)]) [(noFacts, .call [var "idx"] [] quiet)]

#guard (branch 1 noFacts).isNone
#guard (branch 2 { inst := [(var "idx", bounded)] }).isSome

/-! ## Offsets a statement kills -/

/-- An `Array` of two `Index` values. -/
def cells : Ctx Term Facts := ctx [index] [("cells", .array (.strct "Index") 2)]

def kTerm : Term := .read "k" "k"

/-- The facts after a check `cells[k].value < 16_u64` at the current `k`. -/
def checkedAtK : Facts := { inst := [(var "cells" [ix kTerm], bounded)] }

/-- `set cells[k].value = v;` and then `set k = 1_u64;` in a fact state, from
`type11-neg-field-offset-moved-before-restored` and
`type11-pos-field-offset-restored-before-moved` with the cursor a local. -/
def moved (atMove : Facts) : Option (St Term) :=
  run cells start [
    (noFacts, .bind "cells" .fresh quiet),
    (noFacts, .store (var "cells" [ix kTerm, .field 0]) .fresh (writing ["cells"])),
    (atMove, .bind "k" .fresh (writing ["k"]))]

/-- A hand-off after `moved`. -/
def handAfter (atMove : Facts) (atCall : Facts) (p : Place Term) : Option (St Term) :=
  (moved atMove).bind fun s => run cells s [(atCall, .call [p] [] quiet)]

-- Unrestored when `k` moves, the debt is widened to `cells[?]`: handing
-- `cells` off is rejected, and so is `cells[0]` where only `cells[k]` is
-- proved, while `cells[k]` is accepted where the facts prove its instance.
#guard (handAfter noFacts checkedAtK (var "cells")).isNone
#guard (handAfter noFacts checkedAtK (var "cells" [ix (.num 0)])).isNone
#guard (handAfter noFacts checkedAtK (var "cells" [ix kTerm])).isSome
#guard (handAfter noFacts noFacts (var "cells" [ix kTerm])).isNone
-- Restored before `k` moves, the debt is gone.
#guard (handAfter checkedAtK noFacts (var "cells" [ix (.num 0)])).isSome

/-- An `Array` of 64 `Node` values. -/
def nodes : Ctx Term Facts := ctx [node] [("nodes", .array (.strct "Node") 64)]

/-- The offset `nodes[i].next`, a term that reads `nodes`. -/
def nextTerm : Term := .read "nodes" "nodes[i].next"

/-- `set nodes[nodes[i].next].next = 1000_u32;` where the facts before it prove
the instance at `nodes[nodes[i].next]`: the write breaks that element and kills
its own offset. The debt it adds is widened to `nodes[?]`; it is not offered
for restoration in the facts before the write, which still prove the instance
the write breaks. -/
def selfIndexed : Option (St Term) :=
  run nodes start [
    (noFacts, .bind "nodes" .fresh quiet),
    ({ inst := [(var "nodes" [ix nextTerm], link)] },
      .store (var "nodes" [ix nextTerm, .field 0]) .fresh (writing ["nodes"])),
    (noFacts, .call [var "nodes"] [] quiet)]

#guard selfIndexed.isNone

/-! ## A swap through a reference that may name either of two bindings -/

/-- Two `Index` bindings and a third. -/
def trio : Ctx Term Facts :=
  ctx [index] [("left", .strct "Index"), ("right", .strct "Index"), ("x", .strct "Index")]

/-- From `type11-neg-swap-joined-reference-keeps-debt`: after `set right.value =
31_u64;` breaks `right`, `swap(first: target, second: &x);` may exchange `left`
instead, so the debt at `right` stays where it is and is copied onto `x`. -/
def joinedSwapped : Option (St Term) := do
  let s ← picked trio
  run trio s [
    (noFacts, .store (var "right" [.field 0]) .fresh (writing ["right"])),
    (noFacts, .bind "x" .fresh quiet),
    (noFacts, .swap (deref "target") (var "x") (writing ["right", "left", "x"]))]

-- Handing `right` off is rejected, and the value swapped onto `x`, which may
-- be the broken one, gets no instance.
#guard (joinedSwapped.bind fun s => run trio s [(noFacts, .call [var "right"] [] quiet)]).isNone
#guard (joinedSwapped.map fun s => trio.instance? noFacts s (var "x") bounded) == some false

end Whitefoot.Seal.Cases
