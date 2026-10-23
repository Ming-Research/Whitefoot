import WhitefootProof.Walk
import WhitefootProof.FactsSound

/-!
# The walk only grows the generator

The checker's walk runs in `M`, a state monad over the generator: the counter
that numbers bindings, commit values, call datums and atoms, the atoms minted
with their intervals, and the table of snapshots taken on entering each
statement. A join reads states that come from every path, the ones a run did
not take included, so the facts a later proof carries from where they arise to
where they are used need, of the checker alone, that nothing it does later
reuses an identity or forgets an atom. This module proves that:

- every function the walk calls only grows the generator (`Gen.Grows`): the
  counter never moves back, every atom it registers takes a number between the
  old and the new counter and a nonempty interval, and the snapshot table is
  unchanged;
- the walk itself also only appends to the snapshot table, at positions inside
  the block or statement it walks, each once; so the table of a whole function
  holds every position once (`checkFn_points`).

A successful run of a sequence is a successful run of its first part followed
by one of the rest, so each property is proved by taking a run apart along the
definition's binds, matches and loops. The tactics `grows` and `spec_step` do
that mechanically.
-/

namespace Whitefoot

theorem IntTy.minVal_le_maxVal (t : IntTy) : t.minVal ≤ t.maxVal := by
  cases t <;> decide

namespace Checker

open Fact Aff

/-! ## Growth of the generator -/

/-- From `g` to `g'` the counter did not move back, and every atom registered
in between has a number from `g.next` up to below `g'.next` and a nonempty
interval. -/
def Gen.Mints (g g' : Gen) : Prop :=
  g.next ≤ g'.next ∧
  ∃ new, g'.atoms = new ++ g.atoms ∧ ∀ a ∈ new, g.next ≤ a.1 ∧ a.1 < g'.next ∧ a.2.1 ≤ a.2.2

/-- `Mints`, with the snapshot table unchanged. -/
def Gen.Grows (g g' : Gen) : Prop := g.Mints g' ∧ g'.points = g.points

theorem Gen.Mints.refl (g : Gen) : g.Mints g :=
  ⟨Nat.le_refl _, [], rfl, fun _ h => by simp at h⟩

theorem Gen.Mints.trans {g1 g2 g3 : Gen} (h1 : g1.Mints g2) (h2 : g2.Mints g3) : g1.Mints g3 := by
  obtain ⟨hn1, new1, ha1, hb1⟩ := h1
  obtain ⟨hn2, new2, ha2, hb2⟩ := h2
  refine ⟨Nat.le_trans hn1 hn2, new2 ++ new1, by rw [ha2, ha1, List.append_assoc], ?_⟩
  intro a ha
  rcases List.mem_append.mp ha with h | h
  · obtain ⟨x, y, z⟩ := hb2 a h
    exact ⟨Nat.le_trans hn1 x, y, z⟩
  · obtain ⟨x, y, z⟩ := hb1 a h
    exact ⟨x, Nat.lt_of_lt_of_le y hn2, z⟩

theorem Gen.Grows.refl (g : Gen) : g.Grows g := ⟨Gen.Mints.refl g, rfl⟩

theorem Gen.Grows.trans {g1 g2 g3 : Gen} (h1 : g1.Grows g2) (h2 : g2.Grows g3) : g1.Grows g3 :=
  ⟨h1.1.trans h2.1, h2.2.trans h1.2⟩

/-- A step that changes neither the counter, the atoms nor the table, such as
recording a delta atom, grows the generator. -/
theorem Gen.Grows.of_eq {g g' : Gen} (hn : g'.next = g.next) (ha : g'.atoms = g.atoms)
    (hp : g'.points = g.points) : g.Grows g' :=
  ⟨⟨Nat.le_of_eq hn.symm, [], by rw [ha]; rfl, fun _ h => by simp at h⟩, hp⟩

/-- Registering one atom at the counter with a nonempty interval, and moving the
counter past it, grows the generator. -/
theorem Gen.Grows.mint {g g' : Gen} (lo hi : Int) (hlh : lo ≤ hi) (hn : g'.next = g.next + 1)
    (ha : g'.atoms = (g.next, lo, hi) :: g.atoms) (hp : g'.points = g.points) : g.Grows g' := by
  refine ⟨⟨by omega, [(g.next, lo, hi)], by rw [ha]; rfl, fun a h => ?_⟩, hp⟩
  simp only [List.mem_singleton] at h
  subst h
  exact ⟨Nat.le_refl _, by show g.next < g'.next; omega, hlh⟩

/-! ## Taking a run apart -/

/-- A successful run of a sequence is a successful run of its first part
followed by one of the rest. -/
theorem ok_of_bind {α β : Type} {x : M α} {f : α → M β} {g g' : Gen} {b : β}
    (h : (x >>= f).run g = .ok (b, g')) :
    ∃ a g1, x.run g = .ok (a, g1) ∧ (f a).run g1 = .ok (b, g') := by
  rw [StateT.run_bind] at h
  cases hx : x.run g with
  | error e =>
    rw [hx] at h
    cases h
  | ok r =>
    rw [hx] at h
    exact ⟨r.1, r.2, rfl, h⟩

/-- `m` only grows the generator: every successful run of it ends in a
generator that grows the one it started from. -/
structure OnlyGrows {α : Type} (m : M α) : Prop where
  run : ∀ g a g', m.run g = .ok (a, g') → g.Grows g'

theorem OnlyGrows.of_pure {α : Type} (a : α) : OnlyGrows (pure a : M α) :=
  ⟨fun _ _ _ h => by
    cases h
    exact Gen.Grows.refl _⟩

theorem OnlyGrows.of_bind {α β : Type} {m : M α} {f : α → M β} (hm : OnlyGrows m)
    (hf : ∀ a, OnlyGrows (f a)) : OnlyGrows (m >>= f) :=
  ⟨fun g b g' h => by
    obtain ⟨a, g1, h1, h2⟩ := ok_of_bind h
    exact (hm.run g a g1 h1).trans ((hf a).run g1 b g' h2)⟩

theorem OnlyGrows.of_get : OnlyGrows (get : M Gen) :=
  ⟨fun _ _ _ h => by
    cases h
    exact Gen.Grows.refl _⟩

theorem OnlyGrows.of_fail {α : Type} (rule msg : String) : OnlyGrows (fail rule msg : M α) :=
  ⟨fun _ _ _ h => by cases h⟩

theorem OnlyGrows.of_lift {α : Type} (j : Judge α) : OnlyGrows (StateT.lift j : M α) :=
  ⟨fun g a g' h => by
    rw [StateT.run_lift] at h
    cases j with
    | error e => cases h
    | ok v =>
      cases h
      exact Gen.Grows.refl _⟩

theorem OnlyGrows.of_modify {f : Gen → Gen} (hf : ∀ g : Gen, g.Grows (f g)) :
    OnlyGrows (modify f : M PUnit) :=
  ⟨fun g _ _ h => by
    cases h
    exact hf g⟩

theorem OnlyGrows.of_forIn {α β : Type} (xs : List α) (b : β) {f : α → β → M (ForInStep β)}
    (hf : ∀ a b, OnlyGrows (f a b)) : OnlyGrows (forIn xs b f) := by
  induction xs generalizing b with
  | nil =>
    rw [List.forIn_nil]
    exact OnlyGrows.of_pure b
  | cons a as ih =>
    rw [List.forIn_cons]
    refine OnlyGrows.of_bind (hf a b) (fun r => ?_)
    cases r with
    | done b' => exact OnlyGrows.of_pure b'
    | yield b' => exact ih b'

theorem OnlyGrows.of_mapM {α β : Type} {f : α → M β} (hf : ∀ a, OnlyGrows (f a)) :
    ∀ xs : List α, OnlyGrows (xs.mapM f)
  | [] => by
    rw [List.mapM_nil]
    exact OnlyGrows.of_pure _
  | a :: as => by
    rw [List.mapM_cons]
    exact OnlyGrows.of_bind (hf a) fun _ =>
      OnlyGrows.of_bind (OnlyGrows.of_mapM hf as) fun _ => OnlyGrows.of_pure _

theorem OnlyGrows.of_map {α β : Type} (f : α → β) {m : M α} (hm : OnlyGrows m) :
    OnlyGrows (f <$> m) := by
  rw [map_eq_pure_bind]
  exact OnlyGrows.of_bind hm fun _ => OnlyGrows.of_pure _

/-- Takes apart a computation built from binds, matches, conditionals, loops
and the functions this module has already proved to only grow the generator.
Goals it cannot close, such as the interval of a delta atom, are left. -/
syntax "grows" : tactic

set_option hygiene false in
macro_rules
  | `(tactic| grows) => `(tactic| repeat' (first
      | assumption
      | exact OnlyGrows.of_pure _
      | exact OnlyGrows.of_get
      | exact OnlyGrows.of_fail _ _
      | exact OnlyGrows.of_lift _
      | exact freshId_onlyGrows
      | exact typeAtom_onlyGrows
      | exact evalAtom_onlyGrows
      | exact evalRhs_onlyGrows
      | exact joinImage_onlyGrows
      | exact joinStates_onlyGrows
      | exact merge_onlyGrows
      | exact publish_onlyGrows
      | exact bindLet_onlyGrows
      | exact setVar_onlyGrows
      | exact checkReturn_onlyGrows
      | exact loopHead_onlyGrows
      | exact entryState_onlyGrows
      | exact IntTy.minVal_le_maxVal _
      | (refine OnlyGrows.of_bind (freshAtom_onlyGrows ?_) (fun _ => ?_))
      | (refine OnlyGrows.of_modify (fun _ => Gen.Grows.of_eq rfl rfl rfl))
      | (refine OnlyGrows.of_forIn _ _ (fun _ _ => ?_))
      | (refine OnlyGrows.of_mapM (fun _ => ?_) _)
      | (refine OnlyGrows.of_map _ ?_)
      | (refine OnlyGrows.of_bind ?_ (fun _ => ?_))
      | dsimp only
      | split))

/-! ## The functions the walk calls -/

theorem freshId_run (g : Gen) : freshId.run g = .ok (g.next, { g with next := g.next + 1 }) := rfl

theorem freshId_onlyGrows : OnlyGrows freshId :=
  ⟨fun g a g' h => by
    rw [freshId_run] at h
    cases h
    exact ⟨⟨Nat.le_succ _, [], rfl, fun _ h => by simp at h⟩, rfl⟩⟩

theorem freshAtom_run (lo hi : Int) (g : Gen) :
    (freshAtom lo hi).run g =
      .ok (Form.atom g.next, { g with next := g.next + 1, atoms := (g.next, lo, hi) :: g.atoms }) :=
  rfl

theorem freshAtom_onlyGrows {lo hi : Int} (hlh : lo ≤ hi) : OnlyGrows (freshAtom lo hi) :=
  ⟨fun g a g' h => by
    rw [freshAtom_run] at h
    cases h
    exact Gen.Grows.mint lo hi hlh rfl rfl rfl⟩

theorem typeAtom_onlyGrows {t : IntTy} : OnlyGrows (typeAtom t) :=
  freshAtom_onlyGrows (IntTy.minVal_le_maxVal t)

theorem evalAtom_onlyGrows' (env : Env) (st : State) : ∀ a : Atom, OnlyGrows (evalAtom env st a)
  | .int t v => by rw [evalAtom]; grows
  | .unit => by rw [evalAtom]; grows
  | .var x => by rw [evalAtom]; grows
  | .index b off => by
    have ih := evalAtom_onlyGrows' env st off
    rw [evalAtom]
    grows

theorem evalAtom_onlyGrows {env : Env} {st : State} {a : Atom} : OnlyGrows (evalAtom env st a) :=
  evalAtom_onlyGrows' env st a

theorem evalRhs_onlyGrows {env : Env} {st : State} {e : Expr} : OnlyGrows (evalRhs env st e) := by
  unfold evalRhs
  grows

/-- A fold that never raises its accumulator ends at or below where it
started. -/
theorem foldl_le_init {α : Type} (f : Int → α → Int) (hf : ∀ acc x, f acc x ≤ acc) :
    ∀ (xs : List α) (a : Int), xs.foldl f a ≤ a
  | [], a => Int.le_refl a
  | x :: xs, a => Int.le_trans (foldl_le_init f hf xs (f a x)) (hf a x)

/-- A fold that never lowers its accumulator ends at or above where it
started. -/
theorem init_le_foldl {α : Type} (f : Int → α → Int) (hf : ∀ acc x, acc ≤ f acc x) :
    ∀ (xs : List α) (a : Int), a ≤ xs.foldl f a
  | [], a => Int.le_refl a
  | x :: xs, a => Int.le_trans (hf a x) (init_le_foldl f hf xs (f a x))

theorem joinImage_onlyGrows {t : IntTy} {fs : List Form} : OnlyGrows (joinImage t fs) := by
  unfold joinImage
  grows
  all_goals
    refine Int.le_trans (foldl_le_init _ (fun acc x => ?_) _ _)
      (Int.le_trans ?_ (init_le_foldl _ (fun acc x => ?_) _ _))
  all_goals first
    | (rcases x with ⟨_, _, _⟩; exact Int.min_le_left _ _)
    | (rcases x with ⟨_, _, _⟩; exact Int.le_max_left _ _)
    | skip

theorem joinStates_onlyGrows {env : Env} {inputs : List State} :
    OnlyGrows (joinStates env inputs) := by
  unfold joinStates
  grows

theorem merge_onlyGrows {env : Env} {ss : List State} : OnlyGrows (merge env ss) := by
  unfold merge
  grows

theorem publish_onlyGrows {env : Env} {st : State} {f : FnDecl} {args : List (String × Val)}
    {dest : Option (Term × Form)} {skip : Option Nat} {kill : State → State} :
    OnlyGrows (publish env st f args dest skip kill) := by
  unfold publish
  grows

theorem bindLet_onlyGrows {env : Env} {st : State} {x : Var} {r : Rhs} :
    OnlyGrows (bindLet env st x r) := by
  unfold bindLet
  grows

theorem setVar_onlyGrows {env : Env} {st : State} {x : Var} {r : Rhs} :
    OnlyGrows (setVar env st x r) := by
  unfold setVar
  grows

theorem checkReturn_onlyGrows {env : Env} {st : State} {ret : Val} :
    OnlyGrows (checkReturn env st ret) := by
  unfold checkReturn
  grows

theorem loopHead_onlyGrows {env : Env} {st : State} {killed : List String}
    {invs : List (String × Relation)} : OnlyGrows (loopHead env st killed invs) := by
  unfold loopHead
  grows

theorem entryState_onlyGrows {p : Program} {f : FnDecl} : OnlyGrows (entryState p f) := by
  unfold entryState
  grows

/-! ### Each function, as a statement about one run -/

theorem freshId_grows {g g' : Gen} {n : Nat} (h : freshId.run g = .ok (n, g')) : g.Grows g' :=
  freshId_onlyGrows.run g n g' h

theorem freshAtom_grows {lo hi : Int} {g g' : Gen} {f : Form} (hlh : lo ≤ hi)
    (h : (freshAtom lo hi).run g = .ok (f, g')) : g.Grows g' :=
  (freshAtom_onlyGrows hlh).run g f g' h

theorem typeAtom_grows {t : IntTy} {g g' : Gen} {f : Form} (h : (typeAtom t).run g = .ok (f, g')) :
    g.Grows g' :=
  typeAtom_onlyGrows.run g f g' h

theorem evalAtom_grows {env : Env} {st : State} {a : Atom} {g g' : Gen} {v : Val}
    (h : (evalAtom env st a).run g = .ok (v, g')) : g.Grows g' :=
  evalAtom_onlyGrows.run g v g' h

theorem evalRhs_grows {env : Env} {st : State} {e : Expr} {g g' : Gen} {r : Rhs}
    (h : (evalRhs env st e).run g = .ok (r, g')) : g.Grows g' :=
  evalRhs_onlyGrows.run g r g' h

theorem joinImage_grows {t : IntTy} {fs : List Form} {g g' : Gen} {f : Form}
    (h : (joinImage t fs).run g = .ok (f, g')) : g.Grows g' :=
  joinImage_onlyGrows.run g f g' h

theorem joinStates_grows {env : Env} {inputs : List State} {g g' : Gen} {st : State}
    (h : (joinStates env inputs).run g = .ok (st, g')) : g.Grows g' :=
  joinStates_onlyGrows.run g st g' h

theorem merge_grows {env : Env} {ss : List State} {g g' : Gen} {o : Option State}
    (h : (merge env ss).run g = .ok (o, g')) : g.Grows g' :=
  merge_onlyGrows.run g o g' h

theorem publish_grows {env : Env} {st : State} {f : FnDecl} {args : List (String × Val)}
    {dest : Option (Term × Form)} {skip : Option Nat} {kill : State → State} {g g' : Gen}
    {st' : State} (h : (publish env st f args dest skip kill).run g = .ok (st', g')) :
    g.Grows g' :=
  publish_onlyGrows.run g st' g' h

theorem bindLet_grows {env : Env} {st : State} {x : Var} {r : Rhs} {g g' : Gen} {st' : State}
    (h : (bindLet env st x r).run g = .ok (st', g')) : g.Grows g' :=
  bindLet_onlyGrows.run g st' g' h

theorem setVar_grows {env : Env} {st : State} {x : Var} {r : Rhs} {g g' : Gen} {st' : State}
    (h : (setVar env st x r).run g = .ok (st', g')) : g.Grows g' :=
  setVar_onlyGrows.run g st' g' h

theorem checkReturn_grows {env : Env} {st : State} {ret : Val} {g g' : Gen} {u : Unit}
    (h : (checkReturn env st ret).run g = .ok (u, g')) : g.Grows g' :=
  checkReturn_onlyGrows.run g u g' h

theorem loopHead_grows {env : Env} {st : State} {killed : List String}
    {invs : List (String × Relation)} {g g' : Gen} {head : State}
    (h : (loopHead env st killed invs).run g = .ok (head, g')) : g.Grows g' :=
  loopHead_onlyGrows.run g head g' h

theorem entryState_grows {p : Program} {f : FnDecl} {g g' : Gen} {es : Env × State}
    (h : (entryState p f).run g = .ok (es, g')) : g.Grows g' :=
  entryState_onlyGrows.run g es g' h

/-! ## The snapshot table

The walk records a snapshot on entering each statement. Positions follow the
nesting: the statement at index `j` of the block at `blk` is at `blk ++ [j]`,
and an `if` or `loop` at `pos` has its blocks at `pos ++ [0]` and `pos ++ [1]`.
-/

/-- Positions of statements of the block at `blk` from index `i` on, or nested
in them. -/
def InBlock (blk : Pos) (i : Nat) (q : Pos) : Prop := ∃ j r, i ≤ j ∧ q = blk ++ j :: r

/-- Positions strictly nested in the statement at `pos`. -/
def Below (pos q : Pos) : Prop := ∃ r, r ≠ [] ∧ q = pos ++ r

/-- From `g` to `g'` the generator minted as `Mints` says, and the snapshot
table only gained entries at the end, at distinct positions satisfying `P`. -/
def Gen.Records (P : Pos → Prop) (g g' : Gen) : Prop :=
  g.Mints g' ∧ ∃ L : List (Pos × Snapshot), g'.points = g.points ++ L ∧
    (L.map (·.1)).Nodup ∧ ∀ q ∈ L.map (·.1), P q

theorem Gen.Records.refl (P : Pos → Prop) (g : Gen) : g.Records P g :=
  ⟨Gen.Mints.refl g, [], by simp, by simp, by simp⟩

theorem Gen.Records.of_grows {P : Pos → Prop} {g g' : Gen} (h : g.Grows g') : g.Records P g' :=
  ⟨h.1, [], by rw [h.2, List.append_nil], by simp, by simp⟩

theorem Gen.Records.mono {P Q : Pos → Prop} {g g' : Gen} (hPQ : ∀ q, P q → Q q)
    (h : g.Records P g') : g.Records Q g' := by
  obtain ⟨hm, L, hp, hn, hq⟩ := h
  exact ⟨hm, L, hp, hn, fun q hmem => hPQ q (hq q hmem)⟩

theorem Gen.Records.comp {P Q : Pos → Prop} {g1 g2 g3 : Gen} (h1 : g1.Records P g2)
    (h2 : g2.Records Q g3) (hd : ∀ q, P q → Q q → False) :
    g1.Records (fun q => P q ∨ Q q) g3 := by
  obtain ⟨hm1, L1, hp1, hn1, hq1⟩ := h1
  obtain ⟨hm2, L2, hp2, hn2, hq2⟩ := h2
  refine ⟨hm1.trans hm2, L1 ++ L2, by rw [hp2, hp1, List.append_assoc], ?_, ?_⟩
  · rw [List.map_append, List.nodup_append]
    exact ⟨hn1, hn2, fun a ha b hb hab => hd a (hq1 a ha) (by rw [hab]; exact hq2 b hb)⟩
  · intro q hmem
    rw [List.map_append, List.mem_append] at hmem
    rcases hmem with h | h
    · exact Or.inl (hq1 q h)
    · exact Or.inr (hq2 q h)

/-- Recording one snapshot at `q`. -/
theorem Gen.Records.of_modify_point {q : Pos} {s : Snapshot} :
    ∀ g (u : PUnit) g',
      (modify (fun g : Gen => { g with points := g.points ++ [(q, s)] }) : M PUnit).run g =
        .ok (u, g') → g.Records (fun p => p = q) g' := by
  intro g u g' h
  cases h
  exact ⟨Gen.Mints.refl g, [(q, s)], rfl, by simp, by simp⟩

/-- A property of the generator kept by every step that only grows it. -/
def Stable (P : Gen → Prop) : Prop := ∀ g g', P g → g.Grows g' → P g'

theorem Gen.Records.stable {P : Pos → Prop} {g0 : Gen} : Stable (Gen.Records P g0) := by
  intro g g' h hg
  obtain ⟨hm, L, hp, hn, hq⟩ := h
  exact ⟨hm.trans hg.1, L, by rw [hg.2, hp], hn, hq⟩

/-- From every generator satisfying `P`, every successful run of `m` ends in
one satisfying `Q`. -/
structure Spec {α : Type} (P : Gen → Prop) (m : M α) (Q : Gen → Prop) : Prop where
  run : ∀ g a g', P g → m.run g = .ok (a, g') → Q g'

theorem Spec.of_fail {α : Type} {P Q : Gen → Prop} (rule msg : String) :
    Spec P (fail rule msg : M α) Q :=
  ⟨fun _ _ _ _ h => by cases h⟩

theorem Spec.of_pure {α : Type} {P Q : Gen → Prop} (a : α) (h : ∀ g, P g → Q g) :
    Spec P (pure a : M α) Q :=
  ⟨fun _ _ _ hg hr => by
    cases hr
    exact h _ hg⟩

theorem Spec.of_bind_grows {α β : Type} {P Q : Gen → Prop} {m : M α} {f : α → M β}
    (hP : Stable P) (hm : OnlyGrows m) (hf : ∀ a, Spec P (f a) Q) : Spec P (m >>= f) Q :=
  ⟨fun g b g' hg h => by
    obtain ⟨a, g1, h1, h2⟩ := ok_of_bind h
    exact (hf a).run g1 b g' (hP g g1 hg (hm.run g a g1 h1)) h2⟩

/-- A first part that records snapshots at positions `B`, disjoint from the
positions `A` recorded so far. -/
theorem Spec.of_bind_records {α β : Type} {A B : Pos → Prop} {g0 : Gen} {Q : Gen → Prop}
    {m : M α} {f : α → M β} (hm : ∀ g a g', m.run g = .ok (a, g') → g.Records B g')
    (hd : ∀ q, A q → B q → False)
    (hf : ∀ a, Spec (Gen.Records (fun q => A q ∨ B q) g0) (f a) Q) :
    Spec (Gen.Records A g0) (m >>= f) Q :=
  ⟨fun g b g' hg h => by
    obtain ⟨a, g1, h1, h2⟩ := ok_of_bind h
    exact (hf a).run g1 b g' (Gen.Records.comp hg (hm g a g1 h1) hd) h2⟩

theorem discard_eq {α : Type} (x : M α) : (discard x : M PUnit) = (fun _ => PUnit.unit) <$> x :=
  rfl

/-! ### Positions -/

theorem incl_point_inBlock {blk : Pos} {i : Nat} {q : Pos} (h : q = blk ++ [i]) :
    InBlock blk i q :=
  ⟨i, [], Nat.le_refl i, h⟩

theorem incl_below_inBlock {blk : Pos} {i : Nat} {q : Pos} (h : Below (blk ++ [i]) q) :
    InBlock blk i q := by
  obtain ⟨r, _, h⟩ := h
  exact ⟨i, r, Nat.le_refl i, by rw [h, List.append_assoc]; rfl⟩

theorem incl_inBlock_succ {blk : Pos} {i : Nat} {q : Pos} (h : InBlock blk (i + 1) q) :
    InBlock blk i q := by
  obtain ⟨j, r, hj, h⟩ := h
  exact ⟨j, r, by omega, h⟩

theorem incl_inBlock_below {pos : Pos} {k : Nat} {q : Pos} (h : InBlock (pos ++ [k]) 0 q) :
    Below pos q := by
  obtain ⟨j, r, _, h⟩ := h
  exact ⟨k :: j :: r, List.cons_ne_nil _ _, by rw [h, List.append_assoc]; rfl⟩

theorem disj_false {Y : Prop} (h : False) (_ : Y) : False := h

theorem disj_point_below {pos q : Pos} (h : q = pos) (h' : Below pos q) : False := by
  obtain ⟨r, hr, h'⟩ := h'
  rw [h] at h'
  have hl := congrArg List.length h'
  rw [List.length_append] at hl
  exact hr (List.eq_nil_of_length_eq_zero (by omega))

theorem disj_point_succ {blk : Pos} {i : Nat} {q : Pos} (h : q = blk ++ [i])
    (h' : InBlock blk (i + 1) q) : False := by
  obtain ⟨j, r, hj, h'⟩ := h'
  rw [h] at h'
  have := (List.cons.inj (List.append_cancel_left h')).1
  omega

theorem disj_below_succ {blk : Pos} {i : Nat} {q : Pos} (h : Below (blk ++ [i]) q)
    (h' : InBlock blk (i + 1) q) : False := by
  obtain ⟨r0, _, h⟩ := h
  obtain ⟨j, r, hj, h'⟩ := h'
  rw [h, List.append_assoc] at h'
  have := (List.cons.inj (List.append_cancel_left h')).1
  omega

theorem disj_if {pos q : Pos} (h : InBlock (pos ++ [0]) 0 q) (h' : InBlock (pos ++ [1]) 0 q) :
    False := by
  obtain ⟨j, r, _, h⟩ := h
  obtain ⟨j', r', _, h'⟩ := h'
  rw [h, List.append_assoc, List.append_assoc] at h'
  have := (List.cons.inj (List.append_cancel_left h')).1
  exact absurd this (by decide)

/-- Proves that positions of one kind are positions of another, splitting the
disjunctions the walk accumulates. -/
syntax "region_incl" : tactic

set_option hygiene false in
macro_rules
  | `(tactic| region_incl) => `(tactic| first
      | exact False.elim
      | exact incl_point_inBlock
      | exact incl_below_inBlock
      | exact incl_inBlock_succ
      | exact incl_inBlock_below
      | (refine fun h => Or.elim h ?_ ?_ <;> region_incl))

/-- Proves that positions recorded so far and positions a step records are
disjoint. -/
syntax "region_sep" : tactic

set_option hygiene false in
macro_rules
  | `(tactic| region_sep) => `(tactic| first
      | exact disj_false
      | exact disj_point_below
      | exact disj_point_succ
      | exact disj_below_succ
      | exact disj_if
      | (refine fun h => Or.elim h ?_ ?_ <;> region_sep))

set_option hygiene false in
macro "region_close" : tactic => `(tactic| first
  | (intro _ h; refine Gen.Records.mono (fun q => ?_) h; region_incl)
  | (intro q; region_sep))

/-! ### The walk -/

/-- One step of taking apart a walk: a statement or statement list recording
its snapshots, the walk's own snapshot, a part that only grows the generator,
or the end. Leaves the disjointness of positions and the final inclusion. -/
syntax "spec_step" : tactic

set_option hygiene false in
macro_rules
  | `(tactic| spec_step) => `(tactic| first
      | exact Spec.of_fail _ _
      | (refine Spec.of_bind_records (walkList_records _ _ _ _ _) ?_ (fun _ => ?_))
      | (refine Spec.of_bind_records (walkStmt_records _ _ _ _) ?_ (fun _ => ?_))
      | (refine Spec.of_bind_records Gen.Records.of_modify_point ?_ (fun _ => ?_))
      | (refine Spec.of_bind_grows Gen.Records.stable ?_ (fun _ => ?_); focus (grows; done))
      | (refine Spec.of_pure _ ?_)
      | (rw [discard_eq, map_eq_pure_bind])
      | dsimp only
      | split)

mutual
/-- A statement list's walk only mints fresh atoms and records each position
inside its block from its first index on once. -/
theorem walkList_records (env : Env) (st : State) (blk : Pos) (i : Nat) (ss : List Stmt) :
    ∀ g o g', (walkList env st blk i ss).run g = .ok (o, g') → g.Records (InBlock blk i) g' :=
  match ss with
  | [] => by
    intro g o g' h
    rw [walkList] at h
    cases h
    exact Gen.Records.refl _ _
  | [s] => by
    intro g o g' h
    have hs : Spec (Gen.Records (fun _ => False) g) (walkList env st blk i [s])
        (Gen.Records (InBlock blk i) g) := by
      rw [walkList]
      repeat' spec_step
      all_goals region_close
    exact hs.run g o g' (Gen.Records.refl _ g) h
  | s :: s' :: rest => by
    intro g o g' h
    have hs : Spec (Gen.Records (fun _ => False) g) (walkList env st blk i (s :: s' :: rest))
        (Gen.Records (InBlock blk i) g) := by
      rw [walkList]
      repeat' spec_step
      all_goals region_close
    exact hs.run g o g' (Gen.Records.refl _ g) h
termination_by structural ss

/-- A statement's walk only mints fresh atoms and records each position nested
in it once. -/
theorem walkStmt_records (env : Env) (st : State) (pos : Pos) (s : Stmt) :
    ∀ g eo g', (walkStmt env st pos s).run g = .ok (eo, g') → g.Records (Below pos) g' :=
  match s with
  | .let_ x e => by
    intro g eo g' h
    have hm : OnlyGrows (walkStmt env st pos (.let_ x e)) := by
      rw [walkStmt]
      grows
    exact Gen.Records.of_grows (hm.run g eo g' h)
  | .set t e => by
    intro g eo g' h
    have hm : OnlyGrows (walkStmt env st pos (.set t e)) := by
      cases t <;> rw [walkStmt] <;> grows
    exact Gen.Records.of_grows (hm.run g eo g' h)
  | .call e => by
    intro g eo g' h
    have hm : OnlyGrows (walkStmt env st pos (.call e)) := by
      rw [walkStmt]
      grows
    exact Gen.Records.of_grows (hm.run g eo g' h)
  | .ret e => by
    intro g eo g' h
    have hm : OnlyGrows (walkStmt env st pos (.ret e)) := by
      rw [walkStmt]
      grows
    exact Gen.Records.of_grows (hm.run g eo g' h)
  | .if_ c thn els he => by
    intro g eo g' h
    have hs : Spec (Gen.Records (fun _ => False) g) (walkStmt env st pos (.if_ c thn els he))
        (Gen.Records (Below pos) g) := by
      rw [walkStmt]
      repeat' spec_step
      all_goals region_close
    exact hs.run g eo g' (Gen.Records.refl _ g) h
  | .loop label invs body => by
    intro g eo g' h
    have hs : Spec (Gen.Records (fun _ => False) g) (walkStmt env st pos (.loop label invs body))
        (Gen.Records (Below pos) g) := by
      rw [walkStmt]
      repeat' spec_step
      all_goals region_close
    exact hs.run g eo g' (Gen.Records.refl _ g) h
  | .brk label => by
    intro g eo g' h
    have hm : OnlyGrows (walkStmt env st pos (.brk label)) := by
      rw [walkStmt]
      grows
    exact Gen.Records.of_grows (hm.run g eo g' h)
  | .cont label => by
    intro g eo g' h
    have hm : OnlyGrows (walkStmt env st pos (.cont label)) := by
      rw [walkStmt]
      grows
    exact Gen.Records.of_grows (hm.run g eo g' h)
  | .invariant n r => by
    intro g eo g' h
    have hm : OnlyGrows (walkStmt env st pos (.invariant n r)) := by
      rw [walkStmt]
      grows
    exact Gen.Records.of_grows (hm.run g eo g' h)
termination_by structural s
end

/-- The walk of a statement list only mints fresh atoms; its snapshots are
appended at distinct positions of statements of its block from its first index
on, or nested in them. -/
theorem walkList_grows {env : Env} {st : State} {blk : Pos} {i : Nat} {ss : List Stmt}
    {g g' : Gen} {o : Out} (h : (walkList env st blk i ss).run g = .ok (o, g')) :
    g.Mints g' ∧ ∃ L, g'.points = g.points ++ L ∧ (L.map (·.1)).Nodup ∧
      ∀ q ∈ L.map (·.1), ∃ j r, i ≤ j ∧ q = blk ++ j :: r :=
  walkList_records env st blk i ss g o g' h

/-- The walk of one statement only mints fresh atoms; its snapshots are
appended at distinct positions strictly nested in it. -/
theorem walkStmt_grows {env env' : Env} {st : State} {pos : Pos} {s : Stmt} {g g' : Gen}
    {o : Out} (h : (walkStmt env st pos s).run g = .ok ((env', o), g')) :
    g.Mints g' ∧ ∃ L, g'.points = g.points ++ L ∧ (L.map (·.1)).Nodup ∧
      ∀ q ∈ L.map (·.1), ∃ r, r ≠ [] ∧ q = pos ++ r :=
  walkStmt_records env st pos s g (env', o) g' h

/-- The snapshot table of one function holds each position once. -/
theorem checkFn_points {p : Program} {f : FnDecl} {g : Gen} {u : Unit}
    (h : (checkFn p f).run {} = .ok (u, g)) : (g.points.map (·.1)).Nodup := by
  have hs : Spec (Gen.Records (fun _ => False) {}) (checkFn p f) (Gen.Records (InBlock [] 0) {}) := by
    unfold checkFn
    repeat' spec_step
    all_goals region_close
  obtain ⟨_, L, hp, hn, _⟩ := hs.run {} u g (Gen.Records.refl _ _) h
  rw [hp]
  simpa using hn

end Checker

end Whitefoot
