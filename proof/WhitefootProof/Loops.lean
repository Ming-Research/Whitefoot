import WhitefootProof.Blocks
import WhitefootProof.Typing

/-!
# Loops and the two theorems

A `loop` runs its body from the head state the walk computed: on entry, after
the body's end and after a `continue` to it. The head holds each time because
the frame differs from the loop's entry frame only at the newest binding of a
name the head kills, which the loop's own watch tracks, and the invariants hold
again where the walk proved them. With the loop's triple every kind of
statement has one; an induction over the syntax gives every statement list's
triple, and with it entering any function of an accepted program is safe for
every step count. The two first-phase theorems then follow from the laws.
-/

namespace Whitefoot

namespace Checker

/-! ## Taking apart a loop's walk and a function's check -/

/-- A `for` loop whose body only checks each element and goes on: a successful
run left the generator as it was and checked every element. -/
theorem run_forIn_checks {α : Type} {f : α → Unit → M (ForInStep Unit)} {P : α → Prop}
    (hf : ∀ a u g g' r, (f a u).run g = .ok (r, g') → r = .yield () ∧ g' = g ∧ P a) :
    ∀ {xs : List α} {g g' : Gen} {u : Unit}, (forIn xs () f).run g = .ok (u, g') →
      g' = g ∧ ∀ a ∈ xs, P a
  | [], g, g', _, h => by
    rw [List.forIn_nil] at h
    obtain ⟨-, h2⟩ := run_pure_ok h
    exact ⟨h2, fun a ha => by cases ha⟩
  | x :: xs, g, g', _, h => by
    rw [List.forIn_cons] at h
    obtain ⟨r, g1, hx, h1⟩ := run_bind_ok h
    obtain ⟨hr, hg1, hPx⟩ := hf x () g g1 r hx
    subst hr
    try dsimp only at h1
    obtain ⟨hg', hall⟩ := run_forIn_checks hf h1
    refine ⟨hg'.trans hg1, fun a ha => ?_⟩
    rcases List.mem_cons.mp ha with rfl | ha'
    · exact hPx
    · exact hall a ha'

theorem run_lift_ok {α : Type} {j : Judge α} {g g' : Gen} {a : α}
    (h : (StateT.lift j : M α).run g = .ok (a, g')) : g' = g ∧ j = .ok a := by
  cases j with
  | error e => cases h
  | ok b =>
    cases h
    exact ⟨rfl, rfl⟩

theorem run_discard_ok {α : Type} {x : M α} {g g' : Gen} {u : Unit}
    (h : (Functor.discard x : M Unit).run g = .ok (u, g')) : ∃ a, x.run g = .ok (a, g') := by
  rw [discard_eq, map_eq_pure_bind] at h
  obtain ⟨a, g1, hx, h1⟩ := run_bind_ok h
  obtain ⟨-, hg⟩ := run_pure_ok h1
  subst hg
  exact ⟨a, hx⟩

/-- A successful run of a choice ran the side its condition selects. -/
theorem run_ite_cases {α : Type} {c : Prop} [Decidable c] {A B : M α} {g g' : Gen} {a : α}
    (h : (if c then A else B).run g = .ok (a, g')) :
    (c ∧ A.run g = .ok (a, g')) ∨ (¬c ∧ B.run g = .ok (a, g')) := by
  by_cases hc : c
  · rw [ite_eq_left hc] at h
    exact Or.inl ⟨hc, h⟩
  · rw [ite_eq_right hc] at h
    exact Or.inr ⟨hc, h⟩

/-- A successful walk of a `loop`: its invariants proved on entry, a fresh
identity, the head, the body's walk from the head inside the loop, the
invariants proved again on every edge back to the head that is not
contradictory, and the join of the `break`s that leave it. -/
theorem walkStmt_loop_ok {env : Env} {st : State} {pos : Pos} {label : Option String}
    {invs : List (String × Relation)} {body : List Stmt} {g g' : Gen} {env' : Env} {o : Out}
    (h : (walkStmt env st pos (.loop label invs body)).run g = .ok ((env', o), g')) :
    ∃ head g2 ob g3 mo,
      (∀ ir ∈ invs, relHolds env g st ir.2 = true) ∧
      (loopHead env st (killList { normal := true, loops := [(label, false, true)] } body).eraseDups
        invs).run { g with next := g.next + 1 } = .ok (head, g2) ∧
      (walkList
          { env with
            loops := { id := g.next, label, headers := invs.map (·.2), outside := env.vars } ::
              env.loops }
          head (pos ++ [0]) 0 body).run g2 = .ok (ob, g3) ∧
      (∀ s ∈ (ob.normal ++ (ob.conts.filter (·.1 == g.next)).map (·.2)).map (·.exit env.vars),
        s.contra = false → ∀ ir ∈ invs, relHolds env g3 s ir.2 = true) ∧
      (merge env ((ob.breaks.filter (·.1 == g.next)).map (·.2.exit env.vars))).run g3 =
        .ok (mo, g') ∧
      env' = env ∧
      o = { normal := mo.toList, breaks := ob.breaks.filter (·.1 != g.next),
            conts := ob.conts.filter (·.1 != g.next) } := by
  rw [walkStmt] at h
  obtain ⟨g0, g1, hget, h1⟩ := run_bind_ok h
  obtain ⟨hg0, hg1⟩ := run_get_ok hget
  subst g0
  subst g1
  obtain ⟨_, g2, hfor1, h2⟩ := run_bind_ok h1
  obtain ⟨hg2, hinv0⟩ := run_forIn_checks
    (P := fun ir : String × Relation => relHolds env g st ir.2 = true)
    (fun a _ ga gb r hr => by
      obtain ⟨nm, rel⟩ := a
      try dsimp only at hr
      obtain ⟨hc, h'⟩ := run_ite_fail_ok hr
      obtain ⟨hr1, hr2⟩ := run_pure_ok h'
      exact ⟨hr1, hr2, bool_of_not_not hc⟩) hfor1
  subst g2
  obtain ⟨nid, g3, hid, h3⟩ := run_bind_ok h2
  obtain ⟨hnid, hg3⟩ := run_freshId_ok hid
  subst nid
  subst g3
  obtain ⟨head, g4, hlh, h4⟩ := run_bind_ok h3
  obtain ⟨ob, g5, hwb, h5⟩ := run_bind_ok h4
  obtain ⟨g6, g7, hget2, h6⟩ := run_bind_ok h5
  obtain ⟨hg6, hg7⟩ := run_get_ok hget2
  subst g6
  subst g7
  obtain ⟨_, g8, hfor2, h7⟩ := run_bind_ok h6
  obtain ⟨hg8, hpres⟩ := run_forIn_checks
    (P := fun s : State => s.contra = false → ∀ ir ∈ invs, relHolds env g5 s ir.2 = true)
    (fun s _ ga gb r hr => by
      rcases run_ite_cases hr with ⟨_, hA⟩ | ⟨hc, hB⟩
      · obtain ⟨_, gc, hfor3, hr3⟩ := run_bind_ok hA
        obtain ⟨hgc, hall⟩ := run_forIn_checks
          (P := fun ir : String × Relation => relHolds env g5 s ir.2 = true)
          (fun a _ gx gy r' hr' => by
            obtain ⟨nm, rel⟩ := a
            try dsimp only at hr'
            obtain ⟨hc', h'⟩ := run_ite_fail_ok hr'
            obtain ⟨hr1, hr2⟩ := run_pure_ok h'
            exact ⟨hr1, hr2, bool_of_not_not hc'⟩) hfor3
        obtain ⟨hr1, hr2⟩ := run_pure_ok hr3
        exact ⟨hr1, hr2.trans hgc, fun _ => hall⟩
      · obtain ⟨hr1, hr2⟩ := run_pure_ok hB
        refine ⟨hr1, hr2, fun hs => ?_⟩
        rw [hs] at hc
        exact (hc rfl).elim) hfor2
  subst g8
  obtain ⟨mo, g9, hmerge, h8⟩ := run_bind_ok h7
  obtain ⟨h9, hg9⟩ := run_pure_ok h8
  subst g9
  exact ⟨head, g4, ob, g5, mo, hinv0, hlh, hwb, hpres, hmerge, congrArg Prod.fst h9,
    congrArg Prod.snd h9⟩

/-- A function the checker accepts had its body walked from its entry state. -/
theorem checkFn_walk {p : Program} {f : FnDecl} {u : Unit} {gF : Gen}
    (h : (checkFn p f).run {} = .ok (u, gF)) :
    ∃ env st g1 o, (entryState p f).run {} = .ok ((env, st), g1) ∧
      (walkList env st [] 0 f.body).run g1 = .ok (o, gF) := by
  unfold checkFn at h
  obtain ⟨_, g0, hfor, h1⟩ := run_bind_ok h
  obtain ⟨hg0, -⟩ := run_forIn_checks (P := fun _ => True)
    (fun a _ ga gb r hr => by
      obtain ⟨_, gc, hl, hr2⟩ := run_bind_ok hr
      obtain ⟨hgc, -⟩ := run_lift_ok hl
      obtain ⟨hr3, hgb⟩ := run_pure_ok hr2
      exact ⟨hr3, hgb.trans hgc, True.intro⟩) hfor
  subst g0
  obtain ⟨_, g2, hcons, h2⟩ := run_bind_ok h1
  obtain ⟨hg2, -⟩ := run_lift_ok hcons
  subst g2
  obtain ⟨⟨env, st⟩, g3, hent, h3⟩ := run_bind_ok h2
  try dsimp only at h3
  obtain ⟨-, h4⟩ := run_ite_fail_ok h3
  obtain ⟨o, hwalk⟩ := run_discard_ok h4
  exact ⟨env, st, g3, o, hent, hwalk⟩

/-- The checker walked every function of a program it accepts. -/
theorem checkFn_ok {p : Program} {f : FnDecl} (hp : p.accepts = true) (hf : f ∈ p.fns) :
    ∃ u gF, (checkFn p f).run {} = .ok (u, gF) := by
  unfold Program.accepts at hp
  split at hp
  · rename_i u hc
    simp only [Program.check] at hc
    obtain ⟨_, -, h1⟩ := Typing.bind_ok hc
    obtain ⟨_, h2, -⟩ := Typing.bind_ok h1
    refine Typing.forIn_ok (P := fun f : FnDecl => ∃ u gF, (checkFn p f).run {} = .ok (u, gF))
      ?_ h2 f hf
    intro f' s hs
    obtain ⟨r, hr, hs'⟩ := Typing.bind_ok hs
    obtain ⟨a, ha⟩ := Typing.discard_ok hr
    exact ⟨⟨a.1, a.2, ha⟩, by cases hs'; rfl⟩
  · cases hp

end Checker

namespace Sim

/-- A `loop`'s invariants passed the judgments without facts, in the scope
the loop is judged in. -/
theorem static_loop {p : Program} {fn : FnDecl} {c : Static.Ctx} {label : Option String}
    {invs : List (String × Relation)} {body : List Stmt} {r : Static.Ctx × Static.Flow}
    (h : Static.stmt p fn c (.loop label invs body) = .ok r) :
    ∀ ir ∈ invs, Static.invariantOk c.scope ir.2 = .ok () := by
  cases label with
  | none =>
    simp only [Static.stmt, Typing.reject_bind] at h
    obtain ⟨-, h⟩ := Typing.ite_reject h
    obtain ⟨_, -, h⟩ := Typing.bind_ok h
    obtain ⟨_, hfor, -⟩ := Typing.bind_ok h
    refine Typing.forIn_ok (P := fun ir : String × Relation =>
      Static.invariantOk c.scope ir.2 = .ok ()) (fun a s hs => ?_) hfor
    obtain ⟨nm, rel⟩ := a
    obtain ⟨u, hu, hs'⟩ := Typing.bind_ok hs
    exact ⟨by cases u; exact hu, by cases hs'; rfl⟩
  | some l =>
    simp only [Static.stmt, Typing.reject_bind] at h
    obtain ⟨-, h⟩ := Typing.ite_reject h
    obtain ⟨-, h⟩ := Typing.ite_reject h
    obtain ⟨_, -, h⟩ := Typing.bind_ok h
    obtain ⟨_, hfor, -⟩ := Typing.bind_ok h
    refine Typing.forIn_ok (P := fun ir : String × Relation =>
      Static.invariantOk c.scope ir.2 = .ok ()) (fun a s hs => ?_) hfor
    obtain ⟨nm, rel⟩ := a
    obtain ⟨u, hu, hs'⟩ := Typing.bind_ok hs
    exact ⟨by cases u; exact hu, by cases hs'; rfl⟩

theorem holds_length {F : Model} (L : Laws F) {vars : List Checker.Var} {st : Checker.State}
    {A : List (Nat × Int × Int)} {E E0 : Env} (h : F.Holds ⟨vars, st, A⟩ (E, E0)) :
    E.length = vars.length := by
  have h' := congrArg List.length (L.names h)
  simp only [List.length_map] at h'
  exact h'

theorem reachStmt_loop (k : Checker.Reach) (l : Option String) (invs : List (String × Relation))
    (body : List Stmt) :
    Checker.reachStmt k (.loop l invs body) =
      Checker.reachList { normal := false, loops := (l, k.normal, false) :: k.loops } body := by
  first | rfl | rw [Checker.reachStmt] | simp only [Checker.reachStmt]

theorem killStmt_loop (k : Checker.Reach) (l : Option String) (invs : List (String × Relation))
    (body : List Stmt) :
    Checker.killStmt k (.loop l invs body) =
      Checker.killList
        { normal := Checker.reachStmt k (.loop l invs body),
          loops := (l, k.normal, Checker.reachStmt k (.loop l invs body)) :: k.loops } body := by
  first | rfl | rw [Checker.killStmt] | simp only [Checker.killStmt]

/-! ## The watches of a loop's body -/

/-- The loop's own watch from its body: the frame it was entered with, the
names its head kills, the bindings in scope at the loop, and a continuation
whose end and `continue` reach the head and whose `break` does not. -/
def ownWatch (label : Option String) (body : List Stmt) (vars : List Checker.Var) (E1 : Env) :
    Watch :=
  { E1,
    killed := (Checker.killList { normal := true, loops := [(label, false, true)] } body).eraseDups,
    vars, reach := { normal := true, loops := [(label, false, true)] } }

/-- An enclosing loop's watch from the body of a `loop` inside it: the body's
end and its `continue` go to the inner head, which reaches the outer head when
the inner loop does; its `break` goes where the inner loop's exit does. -/
def Watch.inside (label : Option String) (invs : List (String × Relation)) (body : List Stmt)
    (w : Watch) : Watch :=
  { w with
    reach :=
      { normal := Checker.reachStmt w.reach (.loop label invs body),
        loops := (label, w.reach.normal, Checker.reachStmt w.reach (.loop label invs body)) ::
          w.reach.loops } }

theorem Watch.inside_reach {w : Watch} {label : Option String} {invs : List (String × Relation)}
    {body : List Stmt} (hr : Checker.reachList (w.inside label invs body).reach body = true) :
    Checker.reachStmt w.reach (.loop label invs body) = true := by
  cases hh : Checker.reachStmt w.reach (.loop label invs body) with
  | true => rfl
  | false =>
    have h1 := hh
    rw [reachStmt_loop] at h1
    have hr' : Checker.reachList
        { normal := Checker.reachStmt w.reach (.loop label invs body),
          loops := (label, w.reach.normal, Checker.reachStmt w.reach (.loop label invs body)) ::
            w.reach.loops } body = true := hr
    rw [hh, h1] at hr'
    cases hr'

theorem Watch.inside_kill (w : Watch) (label : Option String) (invs : List (String × Relation))
    (body : List Stmt) :
    Checker.killList (w.inside label invs body).reach body =
      Checker.killStmt w.reach (.loop label invs body) :=
  (killStmt_loop w.reach label invs body).symm

theorem Watch.Agree.inside {w : Watch} {E : Env} (label : Option String)
    (invs : List (String × Relation)) (body : List Stmt) (h : w.Agree E) :
    (w.inside label invs body).Agree E := by
  obtain ⟨top, R, h1, h2⟩ := h
  exact ⟨top, R, h1, h2⟩

theorem Watch.Agree.of_inside {w : Watch} {E : Env} {label : Option String}
    {invs : List (String × Relation)} {body : List Stmt} (h : (w.inside label invs body).Agree E) :
    w.Agree E := by
  obtain ⟨top, R, h1, h2⟩ := h
  exact ⟨top, R, h1, h2⟩

theorem Watch.Agree.length_le {w : Watch} {E : Env} (h : w.Agree E) : w.E1.length ≤ E.length := by
  obtain ⟨top, R, rfl, hR⟩ := h
  have hlen := hR.length
  simp only [List.length_append]
  omega

/-- At each slot the bindings hold the entry frame's value, or belong to a
killed name no binding above has, which the walk's bindings find there. -/
theorem Agrees.slot {killed : List String} :
    ∀ {vars : List Checker.Var} {seen : List String} {R E1 : Env}, Agrees killed seen vars R E1 →
      ∀ j (h : j < R.length) (h1 : j < E1.length), (R[j]'h).2 = (E1[j]'h1).2 ∨
        ∃ x ∈ killed, x ∉ seen ∧ vars.findIdx? (·.name == x) = some j
  | [], _, _, _, hA, j, h, _ => by
    obtain ⟨rfl, rfl⟩ := hA
    exact absurd h (Nat.not_lt_zero _)
  | v :: vs, seen, _, _, hA, j, h, h1 => by
    obtain ⟨a, b, R', E1', rfl, rfl, hab, hA'⟩ := hA
    cases j with
    | zero =>
      rcases hab with hab | ⟨hk, hns⟩
      · exact Or.inl hab
      · refine Or.inr ⟨v.name, hk, hns, ?_⟩
        first
          | (rw [List.findIdx?_cons, ite_eq_left (beq_self_eq_true _)]; all_goals rfl)
          | simp [List.findIdx?_cons]
    | succ j =>
      rcases Agrees.slot hA' j (by simp only [List.length_cons] at h; omega)
          (by simp only [List.length_cons] at h1; omega) with
        he | ⟨x, hx, hns, hfi⟩
      · exact Or.inl he
      · have hne : ¬((v.name == x) = true) := fun hb =>
          hns (List.mem_cons.mpr (Or.inl (beq_iff_eq.mp hb).symm))
        have hns' : x ∉ seen := fun hm => hns (List.mem_cons.mpr (Or.inr hm))
        refine Or.inr ⟨x, hx, hns', ?_⟩
        first
          | (rw [List.findIdx?_cons, ite_eq_right hne, hfi]; all_goals rfl)
          | simp [List.findIdx?_cons, hne, hfi]

/-- A frame as long as the loop's entry frame that agrees with it differs from
it only at the slots of killed names. -/
theorem Watch.Agree.slots {w : Watch} {E : Env} (h : w.Agree E) (hlen : E.length = w.E1.length) :
    ∀ j (h : j < E.length) (h1 : j < w.E1.length), (E[j]'h).2 = (w.E1[j]'h1).2 ∨
      ∃ x ∈ w.killed, w.vars.findIdx? (·.name == x) = some j := by
  obtain ⟨top, R, rfl, hR⟩ := h
  cases top with
  | nil =>
    intro j hj hj1
    rcases Agrees.slot hR j hj hj1 with he | ⟨x, hx, _, hfi⟩
    · exact Or.inl he
    · exact Or.inr ⟨x, hx, hfi⟩
  | cons a top' =>
    exfalso
    have hl := hR.length
    simp only [List.cons_append, List.length_cons, List.length_append] at hlen
    omega

theorem find_hit {k : Checker.Reach} {lab label : Option String} {a b : Bool}
    {ls : List (Option String × Bool × Bool)} (hl : (lab.isNone || lab == label) = true)
    (hk : k.loops = (label, a, b) :: ls) : k.find lab = some (a, b) := by
  cases lab with
  | none =>
    show (k.loops.head?).map (·.2) = some (a, b)
    rw [hk]
    all_goals rfl
  | some l =>
    have h1 : (some l == label) = true := hl
    have h2 : (label == some l) = true := by
      rw [Bool.beq_comm]
      exact h1
    have h3 : ((label, a, b) :: ls).find? (fun e : Option String × Bool × Bool => e.1 == some l) =
        some (label, a, b) := List.find?_cons_of_pos h2
    show (k.loops.find? (·.1 == some l)).map (·.2) = some (a, b)
    rw [hk, h3]
    all_goals rfl

theorem find_miss {k k' : Checker.Reach} {lab label : Option String} {a b : Bool}
    (hl : (lab.isNone || lab == label) = false) (hk : k.loops = (label, a, b) :: k'.loops) :
    k.find lab = k'.find lab := by
  cases lab with
  | none => cases hl
  | some l =>
    have h1 : (some l == label) = false := hl
    have h2 : (label == some l) = false := by
      rw [Bool.beq_comm]
      exact h1
    have h3 : ((label, a, b) :: k'.loops).find?
        (fun e : Option String × Bool × Bool => e.1 == some l) =
        k'.loops.find? (fun e : Option String × Bool × Bool => e.1 == some l) :=
      List.find?_cons_of_neg fun h' => by cases h'.symm.trans h2
    show (k.loops.find? (·.1 == some l)).map (·.2) = (k'.loops.find? (·.1 == some l)).map (·.2)
    rw [hk, h3]

theorem Watch.inside_find_hit {w : Watch} {label lab : Option String}
    {invs : List (String × Relation)} {body : List Stmt}
    (hl : (lab.isNone || lab == label) = true) :
    (w.inside label invs body).reach.find lab =
      some (w.reach.normal, Checker.reachStmt w.reach (.loop label invs body)) :=
  find_hit (ls := w.reach.loops) hl rfl

theorem Watch.inside_find_miss {w : Watch} {label lab : Option String}
    {invs : List (String × Relation)} {body : List Stmt}
    (hl : (lab.isNone || lab == label) = false) :
    (w.inside label invs body).reach.find lab = w.reach.find lab :=
  find_miss (a := w.reach.normal) (b := Checker.reachStmt w.reach (.loop label invs body)) hl rfl

theorem ownWatch_find_hit {label lab : Option String} {body : List Stmt}
    {vars : List Checker.Var} {E1 : Env} (hl : (lab.isNone || lab == label) = true) :
    (ownWatch label body vars E1).reach.find lab = some (false, true) :=
  find_hit (ls := []) hl rfl

theorem sel1_of_find {k : Checker.Reach} {lab : Option String} {a b : Bool}
    (h : k.find lab = some (a, b)) : ((k.find lab).map (·.1)).getD false = a := by
  rw [h]
  all_goals rfl

theorem sel2_of_find {k : Checker.Reach} {lab : Option String} {a b : Bool}
    (h : k.find lab = some (a, b)) : ((k.find lab).map (·.2)).getD false = b := by
  rw [h]
  all_goals rfl

theorem sel_congr {k k' : Checker.Reach} {lab : Option String} (h : k.find lab = k'.find lab)
    (f : Bool × Bool → Bool) :
    ((k.find lab).map f).getD false = ((k'.find lab).map f).getD false := by
  rw [h]

/-- The loop a `break` or `continue` resolves to is the innermost one its
label selects, or one further out. -/
theorem target_cons {lc0 lc : Checker.LoopCtx} {ls : List Checker.LoopCtx} {lab : Option String}
    (h : Checker.target (lc0 :: ls) lab = some lc) :
    ((lab.isNone || lab == lc0.label) = true ∧ lc = lc0) ∨
      ((lab.isNone || lab == lc0.label) = false ∧ Checker.target ls lab = some lc) := by
  cases lab with
  | none =>
    have h' : some lc0 = some lc := h
    cases h'
    exact Or.inl ⟨rfl, rfl⟩
  | some l =>
    cases hb : (lc0.label == some l) with
    | true =>
      have h1 : Checker.target (lc0 :: ls) (some l) = some lc0 :=
        (List.find?_cons_of_pos hb :
          (lc0 :: ls).find? (fun x : Checker.LoopCtx => x.label == some l) = some lc0)
      rw [h1] at h
      cases h
      refine Or.inl ⟨?_, rfl⟩
      have hb' := hb
      rw [Bool.beq_comm] at hb'
      exact hb'
    | false =>
      have h1 : Checker.target (lc0 :: ls) (some l) = Checker.target ls (some l) :=
        (List.find?_cons_of_neg (fun h' => by cases h'.symm.trans hb) :
          (lc0 :: ls).find? (fun x : Checker.LoopCtx => x.label == some l) =
            ls.find? (fun x : Checker.LoopCtx => x.label == some l))
      rw [h1] at h
      refine Or.inr ⟨?_, h⟩
      have hb' := hb
      rw [Bool.beq_comm] at hb'
      exact hb'

theorem target_mem {loops : List Checker.LoopCtx} {lab : Option String} {lc : Checker.LoopCtx}
    (h : Checker.target loops lab = some lc) : lc ∈ loops := by
  cases lab with
  | none =>
    cases loops with
    | nil => cases h
    | cons l ls =>
      have h' : some l = some lc := h
      cases h'
      exact List.Mem.head _
  | some l => exact List.mem_of_find?_eq_some h

/-! ## The machine's loop -/

theorem stepStmt_loop (p : Program) (f : Frame) (callers : List Frame) (pos : Pos)
    (label : Option String) (invs : List (String × Relation)) (body : List Stmt) (k : List Kont) :
    stepStmt p f callers pos (.loop label invs body) k =
      .next
        { frames :=
            { f with
              kont := .stmts (pos ++ [0]) 0 body :: .loop label (pos ++ [0]) body f.env.length :: k } ::
              callers } := rfl

theorem findLoop_loop (lab l : Option String) (blk : Pos) (body : List Stmt) (d : Nat)
    (rest : List Kont) :
    findLoop lab (.loop l blk body d :: rest) =
      if (lab.isNone || lab == l) = true then some (l, blk, body, d, rest)
      else findLoop lab rest := by
  first | rfl | rw [findLoop] | simp only [findLoop]

theorem findLoop_hit {lab l : Option String} {blk : Pos} {body : List Stmt} {d : Nat}
    {rest : List Kont} (h : (lab.isNone || lab == l) = true) :
    findLoop lab (.loop l blk body d :: rest) = some (l, blk, body, d, rest) := by
  rw [findLoop_loop, ite_eq_left h]

theorem findLoop_miss {lab l : Option String} {blk : Pos} {body : List Stmt} {d : Nat}
    {rest : List Kont} (h : (lab.isNone || lab == l) = false) :
    findLoop lab (.loop l blk body d :: rest) = findLoop lab rest := by
  have hn : ¬((lab.isNone || lab == l) = true) := fun h' => by rw [h] at h'; cases h'
  rw [findLoop_loop, ite_eq_right hn]

theorem stepBreak_of_findLoop {f : Frame} {callers : List Frame} {lab : Option String}
    {k : List Kont} {l : Option String} {blk : Pos} {body : List Stmt} {d : Nat}
    {rest : List Kont} (h : findLoop lab k = some (l, blk, body, d, rest)) :
    stepStmt.stepBreak f callers lab k =
      .next { frames := { f with env := f.env.restore d, kont := rest } :: callers } := by
  unfold stepStmt.stepBreak
  rw [h]
  all_goals rfl

theorem stepContinue_of_findLoop {f : Frame} {callers : List Frame} {lab : Option String}
    {k : List Kont} {l : Option String} {blk : Pos} {body : List Stmt} {d : Nat}
    {rest : List Kont} (h : findLoop lab k = some (l, blk, body, d, rest)) :
    stepStmt.stepContinue f callers lab k =
      .next
        { frames :=
            { f with
              env := f.env.restore d,
              kont := .stmts blk 0 body :: .loop l blk body d :: rest } :: callers } := by
  unfold stepStmt.stepContinue
  rw [h]
  all_goals rfl

theorem stepBreak_loop_hit {f : Frame} {callers : List Frame} {lab l : Option String} {blk : Pos}
    {body : List Stmt} {d : Nat} {rest : List Kont} (h : (lab.isNone || lab == l) = true) :
    stepStmt.stepBreak f callers lab (.loop l blk body d :: rest) =
      .next { frames := { f with env := f.env.restore d, kont := rest } :: callers } :=
  stepBreak_of_findLoop (findLoop_hit h)

theorem stepBreak_loop_miss {f : Frame} {callers : List Frame} {lab l : Option String} {blk : Pos}
    {body : List Stmt} {d : Nat} {rest : List Kont} (h : (lab.isNone || lab == l) = false) :
    stepStmt.stepBreak f callers lab (.loop l blk body d :: rest) =
      stepStmt.stepBreak f callers lab rest := by
  unfold stepStmt.stepBreak
  rw [findLoop_miss h]

theorem stepContinue_loop_hit {f : Frame} {callers : List Frame} {lab l : Option String}
    {blk : Pos} {body : List Stmt} {d : Nat} {rest : List Kont}
    (h : (lab.isNone || lab == l) = true) :
    stepStmt.stepContinue f callers lab (.loop l blk body d :: rest) =
      .next
        { frames :=
            { f with
              env := f.env.restore d,
              kont := .stmts blk 0 body :: .loop l blk body d :: rest } :: callers } :=
  stepContinue_of_findLoop (findLoop_hit h)

theorem stepContinue_loop_miss {f : Frame} {callers : List Frame} {lab l : Option String}
    {blk : Pos} {body : List Stmt} {d : Nat} {rest : List Kont}
    (h : (lab.isNone || lab == l) = false) :
    stepStmt.stepContinue f callers lab (.loop l blk body d :: rest) =
      stepStmt.stepContinue f callers lab rest := by
  unfold stepStmt.stepContinue
  rw [findLoop_miss h]

/-! ## The loop's triple -/

/-- A `loop` is safe for `n` steps from a frame where its entering snapshot
holds: each run of its body starts from a frame where the head holds, by
induction on the steps left, and its `break`s leave as the loop does. -/
theorem stmt_loop {F : Model} {p : Program} {fn : FnDecl} (L : Laws F) (label : Option String)
    (invs : List (String × Relation)) (body : List Stmt) (hB : ListOK F p fn body)
    (hBF : ListFresh F body) : StmtOK F p fn (.loop label invs body) := by
  intro env st pos g g' env' o n W E E0 k0 k D callers henv hlp hw hst0 hpts hH hF hW hN hBk hC hR
    hFn
  obtain ⟨head, g2, ob, g3, mo, hinv0, hlh, hwb, hpres, hmerge, henv', ho⟩ :=
    Checker.walkStmt_loop_ok hw
  subst env'
  subst o
  obtain ⟨cx, rx, hcxk, hcx⟩ := hst0
  have hinvs := static_loop hcx
  obtain ⟨fb, hfb, -⟩ := Typing.stmt_loop hcx
  have hinvs' : ∀ ir ∈ invs, Static.invariantOk (Checker.scopeOf env.vars) ir.2 = .ok () := by
    intro ir hir
    rw [← hcxk.scope]
    exact hinvs ir hir
  have hlenv : E.length = env.vars.length := holds_length L hH
  have hF1 : F.Fresh ⟨env.vars, st, g.atoms⟩ (g.next + 1) :=
    fresh_lift L hF (Checker.freshId_grows (Checker.freshId_run g)).1
  have hHF : F.Fresh ⟨env.vars, head, g2.atoms⟩ g2.next :=
    L.loopHead_fresh (g := { g with next := g.next + 1 }) hF1 hlh
  have hgm := Checker.merge_grows hmerge
  have h12 : g.next + 1 ≤ g2.next := (Checker.loopHead_grows hlh).1.1
  have h23 : g2.next ≤ g3.next := (Checker.walkList_grows hwb).1.1
  have h3' : g3.next ≤ g'.next := hgm.1.1
  have hfo := hBF _ _ _ _ _ _ _ hwb hHF
  have hvars3 : ∀ v ∈ env.vars, v.id < g3.next := ids_mono (L.fresh_ids hF) (by omega)
  have hpts3 : PtsOK p fn g3 := PtsOK.of_prefix hpts ⟨[], by rw [List.append_nil, hgm.2]⟩
  have henvB : EnvOK p fn
      { env with
        loops := { id := g.next, label, headers := invs.map (·.2), outside := env.vars } ::
          env.loops } := by
    refine ⟨henv.prog, henv.fn_eq, henv.accepts, henv.fn_of, fun lc hlc rel hrel => ?_⟩
    rcases List.mem_cons.mp hlc with rfl | hlc'
    · obtain ⟨ir, hir, rfl⟩ := List.mem_map.mp hrel
      exact ⟨cx.scope, hinvs ir hir⟩
    · exact henv.headers lc hlc' rel hrel
  have hlpB : ∀ lc ∈ ({ env with
        loops := { id := g.next, label, headers := invs.map (·.2), outside := env.vars } ::
          env.loops } : Checker.Env).loops, lc.id < g2.next := by
    intro lc hlc
    rcases List.mem_cons.mp hlc with rfl | hlc'
    · show g.next < g2.next
      omega
    · have := hlp lc hlc'
      omega
  -- Every run of the body, with at most `n` steps left, starts from a frame
  -- where the head holds, that agrees with the loop's entry frame, and that
  -- every enclosing loop's watch accepts wherever the loop reaches its head.
  have key : ∀ m, m ≤ n → ∀ E' : Env, F.Holds ⟨env.vars, head, g2.atoms⟩ (E', E0) →
      (ownWatch label body env.vars E).Agree E' →
      (∀ w ∈ W, w.E1.length ≤ E'.length ∧
        (Checker.reachStmt w.reach (.loop label invs body) = true → w.Agree E')) →
      SafeAt F p m (cfg fn E' E0 (.stmts (pos ++ [0]) 0 body ::
        .loop label (pos ++ [0]) body E.length :: k) D callers) := by
    intro m
    induction m with
    | zero => intro _ _ _ _ _; exact SafeAt.zero
    | succ m ih =>
      intro hmn E' hH' hown hout
      -- The body's end and a `continue` to this loop both run the body again
      -- from the bindings in scope at the loop.
      have next : ∀ (new : List Checker.Var) (s : Checker.State) (E2 : Env),
          s.exit env.vars ∈ (ob.normal ++ (ob.conts.filter (·.1 == g.next)).map (·.2)).map
            (·.exit env.vars) →
          F.Holds ⟨new ++ env.vars, s, g3.atoms⟩ (E2, E0) →
          (ownWatch label body env.vars E).Agree E2 →
          (∀ w ∈ W, Checker.reachStmt w.reach (.loop label invs body) = true → w.Agree E2) →
          SafeAt F p m (cfg fn (E2.restore E.length) E0 (.stmts (pos ++ [0]) 0 body ::
            .loop label (pos ++ [0]) body E.length :: k) D callers) := by
        intro new s E2 hin hH2 hown2 hag
        have hlE2 : E.length ≤ E2.length := hown2.length_le
        have hlen2 : (E2.restore E.length).length = E.length := by
          simp only [Env.restore, List.length_drop]
          omega
        have hx : F.Holds ⟨env.vars, s.exit env.vars, g3.atoms⟩ (E2.restore E.length, E0) := by
          rw [hlenv]
          exact L.exit_sound hH2
        have hown3 := hown2.restore (Nat.le_refl E.length)
        have hHead := L.loopHead_sound (g := { g with next := g.next + 1 }) hH hF1 hlh hx
          (hown3.slots hlen2) fun ir hir =>
            ⟨L.relHolds_sound hx (hinvs' ir hir) (hpres _ hin (L.not_contra hx) ir hir),
              cx.scope, hinvs ir hir⟩
        refine ih (by omega) _ hHead hown3 fun w hw => ?_
        have hl0 : w.E1.length ≤ E.length := (hW w hw).1
        exact ⟨by rw [hlen2]; exact hl0, fun hr => (hag w hw hr).restore hl0⟩
      refine hB _ head (pos ++ [0]) 0 g2 g3 ob m
        (ownWatch label body env.vars E :: W.map (Watch.inside label invs body)) E' E0
        (.loop label (pos ++ [0]) body E.length :: k) D callers henvB hlpB hwb
        ⟨{ cx with loops := label :: cx.loops, invariants := invs.map (·.1) ++ cx.invariants },
          true, fb, ⟨hcxk.scope, hcxk.taken, hcxk.params⟩, hfb⟩
        hpts3 hH' hHF ?_ ?_ ?_ ?_ (hR.mono (by omega)) (hFn.mono (by omega))
      · -- The watches at the body's start.
        intro w hw
        rcases List.mem_cons.mp hw with rfl | hw'
        · exact ⟨hown.length_le, fun _ => hown, fun x hx => List.mem_eraseDups.mpr hx⟩
        · obtain ⟨w0, hw0, rfl⟩ := List.mem_map.mp hw'
          obtain ⟨hl, hag⟩ := hout w0 hw0
          exact ⟨hl, fun hr => (hag (Watch.inside_reach hr)).inside label invs body,
            fun x hx => (hW w0 hw0).2.2 x (by rw [← Watch.inside_kill]; exact hx)⟩
      · -- The body's end: the loop's marker runs the body again.
        intro new _ s hs E2 hH2 hW2
        cases m with
        | zero => exact SafeAt.zero
        | succ j =>
          refine Safe.of_next (point_loop _ _ _ _ _ _ _ _ _ _) (step_loop _ _ _ _ _ _ _ _ _ _ _) ?_
          refine SafeAt.mono (Nat.le_succ j) (next new s E2 ?_ hH2
            ((hW2 _ (List.Mem.head _)).2 rfl) fun w hw hr => ?_)
          · exact List.mem_map.mpr ⟨s, List.mem_append.mpr (Or.inl hs), rfl⟩
          · obtain ⟨-, hag⟩ := hW2 (w.inside label invs body)
              (List.mem_cons_of_mem _ (List.mem_map.mpr ⟨w, hw, rfl⟩))
            exact (hag hr).of_inside
      · -- A `break`: to this loop it leaves as the loop does; to a loop
        -- further out it goes on as the loop's own `break` would.
        intro lab lc s ht hs new hnew E2 kk hH2 hW2
        rcases target_cons ht with ⟨hhit, rfl⟩ | ⟨hmiss, ht'⟩
        · have hhit' : (lab.isNone || lab == label) = true := hhit
          rw [stepBreak_loop_hit hhit']
          show SafeAt F p m (cfg fn (E2.restore E.length) E0 k D callers)
          have hmemx : s.exit env.vars ∈
              (ob.breaks.filter (·.1 == g.next)).map (·.2.exit env.vars) :=
            List.mem_map.mpr ⟨(g.next, s), List.mem_filter.mpr ⟨hs, beq_self_eq_true _⟩, rfl⟩
          obtain ⟨st', rfl⟩ : ∃ st', mo = some st' := by
            cases mo with
            | none => exact absurd (Checker.merge_none hmerge) (List.ne_nil_of_mem hmemx)
            | some st' => exact ⟨st', rfl⟩
          have hlE2 : E.length ≤ E2.length := (hW2 _ (List.Mem.head _)).1
          have hlen2 : (E2.restore E.length).length = E.length := by
            simp only [Env.restore, List.length_drop]
            omega
          have hx2 : F.Holds ⟨env.vars, s.exit env.vars, g3.atoms⟩ (E2.restore E.length, E0) := by
            rw [hlenv]
            exact L.exit_sound (vars := env.vars) hH2
          have hfrx : ∀ s' ∈ (ob.breaks.filter (·.1 == g.next)).map (·.2.exit env.vars),
              F.Fresh ⟨env.vars, s', g3.atoms⟩ g3.next := by
            intro s' hs'
            obtain ⟨b, hb, rfl⟩ := List.mem_map.mp hs'
            exact L.exit_fresh (new := []) (vars := env.vars)
              (StFresh.at L (hfo.breaks b (List.mem_filter.mp hb).1) hvars3)
          have hHm := L.merge_sound (env := env) hmemx hx2 hfrx henv.headers hmerge
          refine SafeAt.mono (by omega)
            (hN st' (List.Mem.head _ : st' ∈ [st']) _ hHm fun w hw => ?_)
          have hl0 : w.E1.length ≤ E.length := (hW w hw).1
          obtain ⟨-, hag⟩ := hW2 (w.inside label invs body)
            (List.mem_cons_of_mem _ (List.mem_map.mpr ⟨w, hw, rfl⟩))
          refine ⟨by rw [hlen2]; exact hl0, fun hn => ?_⟩
          exact ((hag ((sel1_of_find (Watch.inside_find_hit (w := w) (invs := invs)
            (body := body) hhit')).trans hn)).of_inside).restore hl0
        · have hmiss' : (lab.isNone || lab == label) = false := hmiss
          rw [stepBreak_loop_miss hmiss']
          have hne : lc.id ≠ g.next := by
            have := hlp lc (target_mem ht')
            omega
          have hsB : (lc.id, s) ∈ ob.breaks.filter (·.1 != g.next) :=
            List.mem_filter.mpr ⟨hs, bne_iff_ne.mpr hne⟩
          have hH2' : F.Holds ⟨new ++ env.vars, s, g'.atoms⟩ (E2, E0) :=
            L.lift hH2 (StFresh.at L (hfo.breaks _ hs) (ids_append hnew hvars3)) hgm.1
          refine StepOK.safeAt_mono (by omega) (hBk lab lc s ht' hsB new (ids_mono hnew h3') E2 kk
            hH2' fun w hw => ?_)
          obtain ⟨hl, hag⟩ := hW2 (w.inside label invs body)
            (List.mem_cons_of_mem _ (List.mem_map.mpr ⟨w, hw, rfl⟩))
          exact ⟨hl, fun hsel => (hag ((sel_congr (Watch.inside_find_miss (w := w) (invs := invs)
            (body := body) hmiss') (·.1)).trans hsel)).of_inside⟩
      · -- A `continue`: to this loop it runs the body again; to a loop
        -- further out it goes on as the loop's own `continue` would.
        intro lab lc s ht hs new hnew E2 kk hH2 hW2
        rcases target_cons ht with ⟨hhit, rfl⟩ | ⟨hmiss, ht'⟩
        · have hhit' : (lab.isNone || lab == label) = true := hhit
          rw [stepContinue_loop_hit hhit']
          show SafeAt F p m (cfg fn (E2.restore E.length) E0 (.stmts (pos ++ [0]) 0 body ::
            .loop label (pos ++ [0]) body E.length :: k) D callers)
          refine next new s E2 ?_ hH2
            ((hW2 _ (List.Mem.head _)).2 (sel2_of_find (ownWatch_find_hit (body := body)
              (vars := env.vars) (E1 := E) hhit'))) fun w hw hr => ?_
          · exact List.mem_map.mpr ⟨s, List.mem_append.mpr (Or.inr (List.mem_map.mpr
              ⟨(g.next, s), List.mem_filter.mpr ⟨hs, beq_self_eq_true _⟩, rfl⟩)), rfl⟩
          · obtain ⟨-, hag⟩ := hW2 (w.inside label invs body)
              (List.mem_cons_of_mem _ (List.mem_map.mpr ⟨w, hw, rfl⟩))
            exact (hag ((sel2_of_find (Watch.inside_find_hit (w := w) (invs := invs)
              (body := body) hhit')).trans hr)).of_inside
        · have hmiss' : (lab.isNone || lab == label) = false := hmiss
          rw [stepContinue_loop_miss hmiss']
          have hne : lc.id ≠ g.next := by
            have := hlp lc (target_mem ht')
            omega
          have hsC : (lc.id, s) ∈ ob.conts.filter (·.1 != g.next) :=
            List.mem_filter.mpr ⟨hs, bne_iff_ne.mpr hne⟩
          have hH2' : F.Holds ⟨new ++ env.vars, s, g'.atoms⟩ (E2, E0) :=
            L.lift hH2 (StFresh.at L (hfo.conts _ hs) (ids_append hnew hvars3)) hgm.1
          refine StepOK.safeAt_mono (by omega) (hC lab lc s ht' hsC new (ids_mono hnew h3') E2 kk
            hH2' fun w hw => ?_)
          obtain ⟨hl, hag⟩ := hW2 (w.inside label invs body)
            (List.mem_cons_of_mem _ (List.mem_map.mpr ⟨w, hw, rfl⟩))
          exact ⟨hl, fun hsel => (hag ((sel_congr (Watch.inside_find_miss (w := w) (invs := invs)
            (body := body) hmiss') (·.2)).trans hsel)).of_inside⟩
  have hHead0 := L.loopHead_sound (g := { g with next := g.next + 1 }) hH hF1 hlh hH
    (fun _ _ _ => Or.inl rfl) fun ir hir =>
      ⟨L.relHolds_sound hH (hinvs' ir hir) (hinv0 ir hir), cx.scope, hinvs ir hir⟩
  rw [stepStmt_loop]
  show SafeAt F p n (cfg fn E E0 (.stmts (pos ++ [0]) 0 body ::
    .loop label (pos ++ [0]) body E.length :: k) D callers)
  exact key n (Nat.le_refl n) E hHead0 ⟨[], E, rfl, Agrees.self (L.names hH)⟩
    fun w hw => ⟨(hW w hw).1, (hW w hw).2.1⟩

theorem env_loop (label : Option String) (invs : List (String × Relation)) (body : List Stmt) :
    StmtEnv (.loop label invs body) := by
  intro env st pos g g' env' o hw
  obtain ⟨_, _, _, _, _, _, _, _, _, _, henv', _⟩ := Checker.walkStmt_loop_ok hw
  exact env_same henv'

theorem fresh_loop {F : Model} (L : Laws F) (label : Option String)
    (invs : List (String × Relation)) (body : List Stmt) (hBF : ListFresh F body) :
    StmtFresh F (.loop label invs body) := by
  intro env st pos g g' env' o hw hF
  obtain ⟨head, g2, ob, g3, mo, _, hlh, hwb, _, hmerge, _, ho⟩ := Checker.walkStmt_loop_ok hw
  subst o
  have hF1 : F.Fresh ⟨env.vars, st, g.atoms⟩ (g.next + 1) :=
    fresh_lift L hF (Checker.freshId_grows (Checker.freshId_run g)).1
  have hHF : F.Fresh ⟨env.vars, head, g2.atoms⟩ g2.next :=
    L.loopHead_fresh (g := { g with next := g.next + 1 }) hF1 hlh
  have hfo := hBF _ _ _ _ _ _ _ hwb hHF
  have hgm := Checker.merge_grows hmerge
  have h12 : g.next + 1 ≤ g2.next := (Checker.loopHead_grows hlh).1.1
  have h23 : g2.next ≤ g3.next := (Checker.walkList_grows hwb).1.1
  have hvars3 : ∀ v ∈ env.vars, v.id < g3.next := ids_mono (L.fresh_ids hF) (by omega)
  refine ⟨fun s hs => ?_, fun b hb => ?_, fun b hb => ?_⟩
  · cases mo with
    | none =>
      change s ∈ ([] : List Checker.State) at hs
      cases hs
    | some st' =>
      change s ∈ [st'] at hs
      have hs' : s = st' := List.mem_singleton.mp hs
      subst hs'
      refine StFresh.of L (L.merge_fresh (env := env) (fun s' hs' => ?_) hmerge)
      obtain ⟨b, hb, rfl⟩ := List.mem_map.mp hs'
      exact L.exit_fresh (new := []) (vars := env.vars)
        (StFresh.at L (hfo.breaks b (List.mem_filter.mp hb).1) hvars3)
  · exact StFresh.lift L (hfo.breaks b (List.mem_filter.mp hb).1) hgm.1
  · exact StFresh.lift L (hfo.conts b (List.mem_filter.mp hb).1) hgm.1

/-! ## Every statement list's triple -/

/-- Every statement's walk and judgment keep a context in line: a `let` by
`align_let`, and every other statement leaves the walk's bindings and the
context's scope and names as they were. -/
theorem align_stmt {p : Program} {fn : FnDecl} (s : Stmt) : StmtAlign p fn s := by
  intro env st pos g g' env' o c c' fl hprog hw hc hcx
  cases s with
  | let_ x e => exact align_let x e env st pos g g' env' o c c' fl hprog hw hc hcx
  | set t e =>
    cases t with
    | var x =>
      obtain ⟨_, _, _, _, _, _, _, henv', -⟩ := Checker.walkStmt_setVar_ok hw
      obtain ⟨_, _, _, hr⟩ := Typing.stmt_set_var hc
      subst henv'
      cases hr
      exact hcx
    | index b off =>
      obtain ⟨_, _, _, _, _, _, _, _, _, _, henv', -⟩ := Checker.walkStmt_setIndex_ok hw
      obtain ⟨_, _, _, hr⟩ := Typing.stmt_set_index hc
      subst henv'
      cases hr
      exact hcx
  | call e =>
    obtain ⟨_, _, _, _, _, henv', -⟩ := Checker.walkStmt_call_ok hw
    obtain ⟨_, _, hr⟩ := Typing.stmt_call hc
    subst henv'
    cases hr
    exact hcx
  | ret e =>
    obtain ⟨_, _, _, _, _, _, _, _, _, _, _, henv', -⟩ := Checker.walkStmt_ret_ok hw
    obtain ⟨_, hr⟩ := Typing.stmt_ret hc
    subst henv'
    cases hr
    exact hcx
  | if_ cnd thn els he =>
    obtain ⟨_, _, _, _, _, _, _, _, _, _, _, henv', -⟩ := Checker.walkStmt_if_ok hw
    obtain ⟨_, _, _, _, _, hr⟩ := Typing.stmt_if hc
    subst henv'
    cases hr
    exact hcx
  | loop label invs body =>
    obtain ⟨_, _, _, _, _, _, _, _, _, _, henv', -⟩ := Checker.walkStmt_loop_ok hw
    obtain ⟨_, _, hr⟩ := Typing.stmt_loop hc
    subst henv'
    cases hr
    exact hcx
  | brk label =>
    obtain ⟨_, _, henv', -, -⟩ := Checker.walkStmt_brk_ok hw
    obtain ⟨_, hr⟩ := Typing.stmt_brk hc
    subst henv'
    cases hr
    exact hcx
  | cont label =>
    obtain ⟨_, _, henv', -, -⟩ := Checker.walkStmt_cont_ok hw
    obtain ⟨_, hr⟩ := Typing.stmt_cont hc
    subst henv'
    cases hr
    exact hcx
  | invariant nm rel =>
    obtain ⟨_, henv', -, -⟩ := Checker.walkStmt_invariant_ok hw
    have hr := Typing.stmt_invariant hc
    subst henv'
    cases hr
    exact ⟨hcx.scope, hcx.taken, hcx.params⟩

mutual
theorem stmt_all {F : Model} {p : Program} {fn : FnDecl} (L : Laws F) (s : Stmt) :
    StmtOK F p fn s ∧ StmtEnv s ∧ StmtFresh F s :=
  match s with
  | .let_ x e => ⟨stmt_let L x e, env_let x e, fresh_let L x e⟩
  | .set (.var x) e => ⟨stmt_setVar L x e, env_setVar x e, fresh_setVar L x e⟩
  | .set (.index b off) e =>
    ⟨stmt_setIndex L b off e, env_setIndex b off e, fresh_setIndex L b off e⟩
  | .call e => ⟨stmt_call L e, env_call e, fresh_call L e⟩
  | .ret e => ⟨stmt_ret L e, env_ret e, fresh_ret e⟩
  | .if_ c thn els he =>
    have hT := list_all (p := p) (fn := fn) L thn
    have hE := list_all (p := p) (fn := fn) L els
    ⟨stmt_if L c thn els he hT.1 hE.1 hT.2 hE.2, env_if c thn els he,
      fresh_if L c thn els he hT.2 hE.2⟩
  | .loop label invs body =>
    have hB := list_all (p := p) (fn := fn) L body
    ⟨stmt_loop L label invs body hB.1 hB.2, env_loop label invs body,
      fresh_loop L label invs body hB.2⟩
  | .brk label => ⟨stmt_brk label, env_brk label, fresh_brk L label⟩
  | .cont label => ⟨stmt_cont label, env_cont label, fresh_cont L label⟩
  | .invariant nm rel => ⟨stmt_invariant L nm rel, env_invariant nm rel, fresh_invariant L nm rel⟩
termination_by structural s

theorem list_all {F : Model} {p : Program} {fn : FnDecl} (L : Laws F) (ss : List Stmt) :
    ListOK F p fn ss ∧ ListFresh F ss :=
  match ss with
  | [] => ⟨list_nil, list_fresh_nil L⟩
  | s :: rest =>
    have hs := stmt_all (p := p) (fn := fn) L s
    have hr := list_all (p := p) (fn := fn) L rest
    ⟨list_cons L hs.1 hs.2.1 hs.2.2 (align_stmt s) hr.1, list_fresh_cons L hs.2.2 hs.2.1 hr.2⟩
termination_by structural ss
end

/-! ## Entering a function -/

theorem stepOK_ofEval_ok {P : Config → Prop} {α : Type} {e : Eval α} {a : α} {k : α → Step}
    (he : e = .ok a) (h : StepOK P (k a)) : StepOK P (Step.ofEval e k) := by
  rw [he]
  exact h

theorem ofEval_next {α : Type} {e : Eval α} {k : α → Step} {c : Config}
    (h : Step.ofEval e k = .next c) : ∃ a, e = .ok a ∧ k a = .next c := by
  cases e with
  | ok a => exact ⟨a, rfl, h⟩
  | error x => cases h

/-- Entering a function whose requirements hold over its arguments goes to its
body's start, or halts ill formed. -/
theorem stepOK_enter {P : Config → Prop} {p : Program} {g : FnDecl}
    {args : List (String × Value)} {D : Dest} {callers : List Frame}
    (hreq : args.map (·.1) = g.params.map (·.name) → allHold args.reverse g.requires = .ok true)
    (h : args.map (·.1) = g.params.map (·.name) →
      (∀ x ∈ args.zip g.params, x.1.2.hasTy p x.2.ty = true) →
      P { frames := { fn := g, env := args.reverse, entry := args.reverse,
                      kont := [.stmts [] 0 g.body], dest := D } :: callers }) :
    StepOK P (enter p g args D callers) := by
  unfold enter
  split
  · exact True.intro
  · rename_i hn
    have hn' : args.map (·.1) = g.params.map (·.name) :=
      Classical.byContradiction fun hne => hn (bne_iff_ne.mpr hne)
    split
    · exact True.intro
    · rename_i ht
      have ht' : ∀ x ∈ args.zip g.params, x.1.2.hasTy p x.2.ty = true := by
        intro x hx
        obtain ⟨⟨nm, v⟩, prm⟩ := x
        exact List.all_eq_true.mp (Checker.bool_of_not_not ht) _ hx
      exact stepOK_ofEval_ok (hreq hn') (h hn' ht')

/-- A function entered at all had its requirements hold. -/
theorem enter_requires {p : Program} {g : FnDecl} {args : List (String × Value)} {D : Dest}
    {callers : List Frame} {c : Config} (h : enter p g args D callers = .next c) :
    allHold args.reverse g.requires = .ok true := by
  unfold enter at h
  split at h
  · cases h
  · split at h
    · cases h
    · obtain ⟨b, hb, hk⟩ := ofEval_next h
      cases b with
      | true => exact hb
      | false => cases hk

/-- The entry's return of a value whose postconditions hold ends the run. -/
theorem stepOK_doReturn_exit {P : Config → Prop} {p : Program} {v : Value} {f : Frame}
    (hd : f.dest = .exit) (hens : allHold ((f.fn.result, v) :: f.entry) f.fn.ensures = .ok true) :
    StepOK P (doReturn p v [f]) := by
  rw [doReturn.eq_def]
  dsimp only
  cases hty : v.hasTy p f.fn.resultTy with
  | false => exact True.intro
  | true =>
    rw [hens, hd]
    exact True.intro

theorem retOK_exit {F : Model} {p : Program} {n : Nat} {f : FnDecl} {E0 : Env} :
    RetOK F p n f E0 .exit [] :=
  fun _ _ _ _ hens => stepOK_doReturn_exit rfl hens

/-- A function body's end is never reached: its continuation is empty there,
and that step halts ill formed. -/
theorem safeAt_kont_nil {F : Model} {p : Program} {n : Nat} {fn : FnDecl} {E E0 : Env}
    {D : Dest} {callers : List Frame} : SafeAt F p n (cfg fn E E0 [] D callers) := by
  cases n with
  | zero => exact SafeAt.zero
  | succ m =>
    refine Safe.intro (PointOK.of_none ?_) ?_
    · rfl
    · exact True.intro

theorem breakOK_nil {F : Model} {p : Program} {n : Nat} {W : List Watch}
    {vars : List Checker.Var} {A : List (Nat × Int × Int)} {N : Nat}
    {breaks : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont} {D : Dest}
    {callers : List Frame} : BreakOK F p n W [] vars A N breaks fn E0 k D callers := by
  intro label lc s ht
  cases label <;> cases ht

theorem contOK_nil {F : Model} {p : Program} {n : Nat} {W : List Watch}
    {vars : List Checker.Var} {A : List (Nat × Int × Int)} {N : Nat}
    {conts : List (Nat × Checker.State)} {fn : FnDecl} {E0 : Env} {k : List Kont} {D : Dest}
    {callers : List Frame} : ContOK F p n W [] vars A N conts fn E0 k D callers := by
  intro label lc s ht
  cases label <;> cases ht

theorem fnSafe_zero {F : Model} {p : Program} : FnSafe F p 0 :=
  fun _ _ _ _ _ _ hreq _ => stepOK_enter hreq fun _ _ => SafeAt.zero

/-- Entering a function is safe for one step more than the calls its body
makes: the checker walked its body from the entry state, which holds on the
arguments. -/
theorem fnSafe_succ {F : Model} {p : Program} (L : Laws F) (hp : p.accepts = true) {m : Nat}
    (hFn : FnSafeTo F p m) : FnSafe F p (m + 1) := by
  intro g args D callers hfn hin hreq hR
  refine stepOK_enter hreq fun hn ht => ?_
  have hg : g ∈ p.fns := List.mem_of_find?_eq_some (show p.fns.find? _ = some g from hfn)
  obtain ⟨u, gF, hck⟩ := Checker.checkFn_ok hp hg
  obtain ⟨env, st, g1, o, hent, hwalk⟩ := Checker.checkFn_walk hck
  obtain ⟨hF0, hprog, hfneq, hloops⟩ := L.entry_fresh hent
  have hH0 := L.entry_sound hp hfn hent hn ht hin (hreq hn)
  -- The body is judged from a context in line with the entry's bindings, and
  -- the arguments have the parameters' types.
  have hfit : EnvTyped args.reverse (Checker.scopeOf env.vars) := L.fits hH0
  have hsc : Checker.scopeOf env.vars = Typing.paramScope p g := scopeOf_entry hfit hn ht
  have hE0 : EnvTyped args.reverse (Typing.paramScope p g) := by
    rw [← hsc]
    exact hfit
  have hctx : CtxOK g env (Typing.initCtx p g) :=
    ctxOK_entry hsc (Checker.entryState_params hent)
  obtain ⟨_, hs⟩ := Typing.checkProgram_inv (Typing.accepts_static hp) g hg
  obtain ⟨-, -, fl, hfl, -⟩ := Typing.checkFn_inv hs
  have hpts : p.points g = gF.points := by
    unfold Program.points
    rw [hck]
    all_goals rfl
  have henv : EnvOK p g env :=
    ⟨hprog, hfneq, hp, hfn, fun lc hlc => by rw [hloops] at hlc; cases hlc⟩
  exact (list_all (p := p) (fn := g) L g.body).1 env st [] 0 g1 gF o m [] args.reverse
    args.reverse [] D callers henv (fun lc hlc => by rw [hloops] at hlc; cases hlc) hwalk
    ⟨_, true, fl, hctx, hfl⟩
    ⟨by rw [hpts]; exact ⟨[], List.append_nil _⟩,
      by rw [hpts]; exact Checker.checkFn_points hck⟩
    hH0 hF0 (fun w hw => by cases hw) (fun _ _ _ _ _ _ _ => safeAt_kont_nil)
    (by rw [hloops]; exact breakOK_nil) (by rw [hloops]; exact contOK_nil)
    ⟨hE0, hR.mono (Nat.le_succ m)⟩ hFn

theorem fnSafeTo_all {F : Model} {p : Program} (L : Laws F) (hp : p.accepts = true) :
    ∀ n, FnSafeTo F p n
  | 0 => fun m hm => by
    have h0 : m = 0 := by omega
    subst h0
    exact fnSafe_zero
  | n + 1 => fun m hm => by
    by_cases h : m ≤ n
    · exact fnSafeTo_all L hp n m h
    · have h1 : m = n + 1 := by omega
      subst h1
      exact fnSafe_succ L hp (fnSafeTo_all L hp n)

/-! ## The two theorems from the laws -/

theorem start_some {p : Program} {c0 : Config} (h : p.start = some c0) :
    ∃ f, p.fn? "main" = some f ∧ enter p f [] .exit [] = .next c0 := by
  unfold Program.start at h
  split at h
  · rename_i f hf
    split at h
    · rename_i c hc
      cases h
      exact ⟨f, hf, hc⟩
    · cases h
  · cases h

/-- The start of an accepted program is safe for every number of steps. -/
theorem start_safe {F : Model} {p : Program} (L : Laws F) (hp : p.accepts = true) :
    ∀ c0, p.start = some c0 → ∀ n, SafeAt F p n c0 := by
  intro c0 hs n
  obtain ⟨f, hf, he⟩ := start_some hs
  have hf' : p.fns.find? (fun x => x.name == "main") = some f := hf
  have hname : f.name = "main" := by
    have hb := List.find?_some hf'
    simpa using hb
  have hfn : p.fn? f.name = some f := by
    rw [hname]
    exact hf
  have h := (fnSafeTo_all L hp n).now f [] .exit [] hfn (fun _ ha => by cases ha)
    (fun _ => enter_requires he) retOK_exit
  rw [he] at h
  exact h

/-- The second theorem from the laws: the checker's facts hold at every point
a run of an accepted program reaches. -/
theorem facts_hold_of_laws {F : Model} {p : Program} (L : Laws F) (hp : p.accepts = true) :
    ∀ pt st, p.Reaches pt st → F.Holds (p.factsAt pt) st :=
  facts_hold_of_safe (Holds := F.Holds) (start_safe L hp)

/-- The first theorem from the laws: an accepted program whose entry holds
ends or runs out of steps, never stuck. -/
theorem accepted_never_stuck_of_laws {F : Model} {p : Program} (L : Laws F)
    (hp : p.accepts = true) (hentry : p.EntryHolds) (steps : Nat) :
    (∃ v, p.run steps = .done v) ∨ p.run steps = .outOfSteps :=
  accepted_never_stuck_of_safe (Holds := F.Holds) hentry (start_safe L hp)
    (fun steps w => Program.never_illFormed p hp hentry steps w) steps

end Sim

end Whitefoot
