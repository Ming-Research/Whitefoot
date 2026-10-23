import WhitefootProof.Points

/-!
# Accepted programs never stop ill formed

The interpreter stops `illFormed` where a step needs a typing or naming
judgment and the program fails it. The checker's judgments without facts
(`Static.lean`) decide exactly those judgments, so a program they accept never
gets there. This module proves it the standard way, by a typing of the
machine's states that every step keeps:

- a value has a static type when it has that type's shape and every integer in
  it lies in its integer type (`Value.Typed`);
- a frame's bindings have the types of a static context, newest first, and its
  parameters' entry values the types of its parameters (`EnvTyped`);
- a frame's continuation passed the statement judgments from that context, and
  the reachability they computed holds of it: a continuation that ends the
  function's body is reached only where the judgments found no normal exit,
  and every loop a `break` the judgments counted leaves goes on to a
  continuation that accepts a normal arrival (`KontOk`);
- the frames below the running one wait for a result of the running function's
  result type, and each place a result goes takes any value of that type
  (`StackOk`).

`Config.WellTyped` gathers them. `main`'s entry state has it
(`Program.start_wellTyped`), a step from it never stops ill formed
(`step_not_illFormed`) and keeps it (`step_wellTyped`). Hence
`Program.never_illFormed`: an accepted program whose entry holds never stops
`illFormed`, however many steps it runs.
-/

namespace Whitefoot

namespace Typing

/-! ## Judgments that succeed -/

theorem bind_ok {ε α β : Type} {x : Except ε α} {g : α → Except ε β} {b : β}
    (h : (x >>= g) = .ok b) : ∃ a, x = .ok a ∧ g a = .ok b := by
  cases x with
  | error e => cases h
  | ok a => exact ⟨a, rfl, h⟩

theorem reject_ne {α : Type} {r m : String} {b : α} : rejectBy r m ≠ (.ok b : Judge α) := by
  simp [rejectBy]

theorem ok_bind {ε α β : Type} (a : α) (g : α → Except ε β) : (Except.ok a >>= g) = g a := rfl

theorem reject_bind {α β : Type} {r m : String} (g : α → Judge β) :
    (rejectBy r m >>= g) = rejectBy r m := rfl

theorem ite_reject {α : Type} {c : Prop} {_ : Decidable c} {r m : String} {x : Judge α} {b : α}
    (h : (if c then rejectBy r m else x) = .ok b) : ¬c ∧ x = .ok b := by
  by_cases hc : c
  · rw [if_pos hc] at h
    exact absurd h reject_ne
  · rw [if_neg hc] at h
    exact ⟨hc, h⟩

theorem discard_ok {ε α : Type} {x : Except ε α} {u : Unit} (h : Functor.discard x = .ok u) :
    ∃ a, x = .ok a := by
  cases x with
  | error e => cases h
  | ok a => exact ⟨a, rfl⟩

/-- Every element of a list passes the body of a `for` loop that succeeds, when
the body only ever goes on to the next element. -/
theorem forIn_ok {ε α : Type} {body : α → Unit → Except ε (ForInStep Unit)} {P : α → Prop}
    (hb : ∀ a s, body a () = .ok s → P a ∧ s = .yield ()) :
    ∀ {l : List α} {u : Unit}, forIn l () body = .ok u → ∀ a ∈ l, P a
  | [], _, _, a, ha => by simp at ha
  | x :: xs, u, h, a, ha => by
    rw [List.forIn_cons] at h
    obtain ⟨s, hs, h⟩ := bind_ok h
    obtain ⟨hx, rfl⟩ := hb x s hs
    rcases List.mem_cons.mp ha with rfl | ha
    · exact hx
    · exact forIn_ok hb h a ha

/-! ## Evaluations that do not stop ill formed -/

/-- An evaluation that does not stop ill formed, and whose result satisfies `P`. -/
def Ok {α : Type} (P : α → Prop) : Eval α → Prop
  | .ok a => P a
  | .error (.stuck _) => True
  | .error (.illFormed _) => False

theorem Ok.bind {α β : Type} {P : α → Prop} {Q : β → Prop} {x : Eval α} {g : α → Eval β}
    (hx : Ok P x) (hg : ∀ a, P a → Ok Q (g a)) : Ok Q (x >>= g) := by
  match x, hx with
  | .ok a, hx => exact hg a hx
  | .error (.stuck _), _ => exact trivial
  | .error (.illFormed _), hx => exact (hx : False).elim

theorem Ok.mono {α : Type} {P Q : α → Prop} {x : Eval α} (h : Ok P x) (hpq : ∀ a, P a → Q a) :
    Ok Q x := by
  match x, h with
  | .ok a, h => exact hpq a h
  | .error (.stuck _), _ => exact trivial
  | .error (.illFormed _), h => exact (h : False).elim

end Typing

/-! ## Values and bindings of a static type -/

/-- A value has a static type: the type's shape, and every integer in it in its
integer type. Static types are read after alias resolution, so `ExitStatus` is
its resolved path. -/
def Value.Typed : Value → Ty → Prop
  | .int t z, .int t' => t = t' ∧ t.Mem z
  | .bool _, .bool => True
  | .unit, .unit => True
  | .array t es, .array t' n => t = t' ∧ es.length = n ∧ ∀ z ∈ es, t.Mem z
  | .status _, .named path => path = ["std", "process", "ExitStatus"]
  | _, _ => False

namespace Value.Typed

theorem int {v : Value} {t : IntTy} (h : v.Typed (.int t)) : ∃ z, v = .int t z ∧ t.Mem z := by
  cases v <;> simp [Value.Typed] at h
  obtain ⟨rfl, hm⟩ := h
  exact ⟨_, rfl, hm⟩

theorem bool {v : Value} (h : v.Typed .bool) : ∃ b, v = .bool b := by
  cases v <;> simp [Value.Typed] at h
  exact ⟨_, rfl⟩

theorem array {v : Value} {t : IntTy} {n : Nat} (h : v.Typed (.array t n)) :
    ∃ es, v = .array t es ∧ es.length = n ∧ ∀ z ∈ es, t.Mem z := by
  cases v <;> simp [Value.Typed] at h
  obtain ⟨rfl, hl, hm⟩ := h
  exact ⟨_, rfl, hl, hm⟩

/-- A value of a declared type's resolved form has the declared type, as the
interpreter tests it. -/
theorem hasTy {p : Program} {v : Value} {T : Ty} (h : v.Typed (Static.normTy p T)) :
    v.hasTy p T = true := by
  cases v <;> cases T <;>
    simp_all [Value.Typed, Static.normTy, Value.hasTy, Program.isExitStatus]

theorem sameTy {v w : Value} {T : Ty} (hv : v.Typed T) (hw : w.Typed T) : v.sameTy w = true := by
  cases v <;> cases w <;> cases T <;> simp_all [Value.Typed, Value.sameTy]

end Value.Typed

/-- The bindings in scope have the types of a static scope, name by name, newest
first. -/
inductive EnvTyped : Env → Static.Scope → Prop
  | nil : EnvTyped [] []
  | cons {x : String} {v : Value} {t : Ty} {E : Env} {sc : Static.Scope} :
      v.Typed t → EnvTyped E sc → EnvTyped ((x, v) :: E) ((x, t) :: sc)

namespace EnvTyped

open Typing

theorem length {E : Env} {sc : Static.Scope} (h : EnvTyped E sc) : E.length = sc.length := by
  induction h with
  | nil => rfl
  | cons _ _ ih => simp [ih]

theorem names {E : Env} {sc : Static.Scope} (h : EnvTyped E sc) : E.map (·.1) = sc.map (·.1) := by
  induction h with
  | nil => rfl
  | cons _ _ ih => simp [ih]

theorem append {E1 E2 : Env} {s1 s2 : Static.Scope} (h1 : EnvTyped E1 s1) (h2 : EnvTyped E2 s2) :
    EnvTyped (E1 ++ E2) (s1 ++ s2) := by
  induction h1 with
  | nil => exact h2
  | cons hv _ ih => exact .cons hv ih

theorem reverse {E : Env} {sc : Static.Scope} (h : EnvTyped E sc) :
    EnvTyped E.reverse sc.reverse := by
  induction h with
  | nil => exact .nil
  | cons hv _ ih =>
    simp only [List.reverse_cons]
    exact append ih (.cons hv .nil)

/-- A name the scope declares has a value of its type. -/
theorem find {E : Env} {sc : Static.Scope} (h : EnvTyped E sc) (x : String) :
    ∀ {y : String} {t : Ty}, sc.find? (·.1 == x) = some (y, t) →
      ∃ v, E.get x = .ok v ∧ v.Typed t := by
  induction h with
  | nil => intro y t hf; simp at hf
  | @cons z v ty E sc hv _ ih =>
    intro y t hf
    by_cases hz : z = x
    · subst hz
      simp at hf
      obtain ⟨-, rfl⟩ := hf
      exact ⟨v, by simp [Env.get], hv⟩
    · have hf' : sc.find? (·.1 == x) = some (y, t) := by simpa [hz] using hf
      obtain ⟨w, hw, hwt⟩ := ih hf'
      refine ⟨w, ?_, hwt⟩
      simp only [Env.get] at hw ⊢
      simpa [hz] using hw

theorem lookup {E : Env} {sc : Static.Scope} (h : EnvTyped E sc) {x : String} {t : Ty}
    (hl : Static.lookup sc x = .ok t) : ∃ v, E.get x = .ok v ∧ v.Typed t := by
  unfold Static.lookup at hl
  split at hl
  · rename_i y t' hf
    cases hl
    exact h.find x hf
  · exact absurd hl reject_ne

/-- Writing a value of a name's type keeps the bindings typed. -/
theorem put {E : Env} {sc : Static.Scope} (h : EnvTyped E sc) {x : String} {t : Ty} {w : Value}
    (hl : Static.lookup sc x = .ok t) (hw : w.Typed t) :
    ∃ E', E.put x w = .ok E' ∧ EnvTyped E' sc := by
  induction h with
  | nil => simp [Static.lookup, rejectBy] at hl
  | @cons z v ty E sc hv hE ih =>
    by_cases hz : z = x
    · subst hz
      simp [Static.lookup] at hl
      subst hl
      exact ⟨(z, w) :: E, by simp [Env.put], .cons hw hE⟩
    · have hl' : Static.lookup sc x = .ok t := by
        simpa [Static.lookup, hz] using hl
      obtain ⟨E', hE', hT⟩ := ih hl'
      refine ⟨(z, v) :: E', ?_, .cons hv hT⟩
      have hzx : (z == x) = false := by simpa using hz
      simp only [Env.put, hzx, Bool.false_eq_true, ↓reduceIte, hE']
      rfl

theorem drop {E : Env} {new sc : Static.Scope} (h : EnvTyped E (new ++ sc)) :
    EnvTyped (E.drop new.length) sc := by
  induction new generalizing E with
  | nil => simpa using h
  | cons b new ih =>
    cases h with
    | cons _ hE => simpa using ih hE

/-- Leaving scopes keeps the older bindings. -/
theorem restore {E : Env} {new sc : Static.Scope} (h : EnvTyped E (new ++ sc)) :
    EnvTyped (E.restore sc.length) sc := by
  have hl := h.length
  simp only [List.length_append] at hl
  have hd : E.length - sc.length = new.length := by omega
  simp only [Env.restore, hd]
  exact h.drop

end EnvTyped

namespace Typing

/-! ## Integer results lie in their type -/

theorem tdiv_mem {t : IntTy} {n d : Int} (hn : t.Mem n) (hd : t.Mem d)
    (hov : t.signed = false ∨ n ≠ t.minVal ∨ d ≠ -1) : t.Mem (n.tdiv d) := by
  obtain ⟨hn1, hn2⟩ := hn
  obtain ⟨hd1, hd2⟩ := hd
  have hM : 1 ≤ t.maxVal := by cases t <;> simp [IntTy.maxVal, IntTy.signed, IntTy.bits]
  have hm : t.minVal = 0 ∨ t.minVal = -t.maxVal - 1 := by
    cases t <;> simp [IntTy.minVal, IntTy.maxVal, IntTy.signed, IntTy.bits]
  have hov' : t.minVal = 0 ∨ n ≠ t.minVal ∨ d ≠ -1 := by
    rcases hov with hs | hov
    · left
      cases t <;> simp_all [IntTy.minVal, IntTy.signed]
    · exact .inr hov
  unfold IntTy.Mem
  rcases n with a | a <;> rcases d with b | b
  · have h1 := Nat.div_le_self a b
    simp only [Int.tdiv]
    generalize a / b = q at *
    simp only [Int.ofNat_eq_natCast] at *
    omega
  · have h1 := Nat.div_le_self a (b + 1)
    simp only [Int.tdiv, Nat.succ_eq_add_one]
    generalize a / (b + 1) = q at *
    simp only [Int.ofNat_eq_natCast, Int.negSucc_eq] at *
    omega
  · have h1 := Nat.div_le_self (a + 1) b
    simp only [Int.tdiv, Nat.succ_eq_add_one]
    generalize (a + 1) / b = q at *
    simp only [Int.ofNat_eq_natCast, Int.negSucc_eq] at *
    omega
  · have h1 := Nat.div_le_self (a + 1) (b + 1)
    have h2 : b = 0 → (a + 1) / (b + 1) = a + 1 := by intro hb; subst hb; simp
    have h3 : 1 ≤ b → (a + 1) / (b + 1) ≤ (a + 1) / 2 := fun hb =>
      Nat.div_le_div_left (by omega) (by omega)
    simp only [Int.tdiv, Nat.succ_eq_add_one]
    generalize (a + 1) / (b + 1) = q at *
    simp only [Int.ofNat_eq_natCast, Int.negSucc_eq] at *
    omega

theorem tmod_mem {t : IntTy} {n d : Int} (hn : t.Mem n) : t.Mem (n.tmod d) := by
  obtain ⟨hn1, hn2⟩ := hn
  have hm : t.minVal ≤ 0 := by cases t <;> simp [IntTy.minVal, IntTy.signed, IntTy.bits]
  have hM : 0 ≤ t.maxVal := by cases t <;> simp [IntTy.maxVal, IntTy.signed, IntTy.bits]
  unfold IntTy.Mem
  rcases n with a | a <;> rcases d with b | b
  · have h1 := Nat.mod_le a b
    simp only [Int.tmod]
    generalize a % b = q at *
    simp only [Int.ofNat_eq_natCast] at *
    omega
  · have h1 := Nat.mod_le a (b + 1)
    simp only [Int.tmod, Nat.succ_eq_add_one]
    generalize a % (b + 1) = q at *
    simp only [Int.ofNat_eq_natCast] at *
    omega
  · have h1 := Nat.mod_le (a + 1) b
    simp only [Int.tmod, Nat.succ_eq_add_one]
    generalize (a + 1) % b = q at *
    simp only [Int.ofNat_eq_natCast, Int.negSucc_eq] at *
    omega
  · have h1 := Nat.mod_le (a + 1) (b + 1)
    simp only [Int.tmod, Nat.succ_eq_add_one]
    generalize (a + 1) % (b + 1) = q at *
    simp only [Int.ofNat_eq_natCast, Int.negSucc_eq] at *
    omega

theorem divExact_mem {op : DivOp} {t : IntTy} {n d r : Int} (hn : t.Mem n) (hd : t.Mem d)
    (h : op.exactForm t n d = some r) : t.Mem r := by
  unfold DivOp.exactForm at h
  split at h
  · rename_i hdef
    cases h
    have hdef' : d ≠ 0 ∧ (t.signed = false ∨ n ≠ t.minVal ∨ d ≠ -1) := of_decide_eq_true hdef
    cases op
    · exact tdiv_mem hn hd hdef'.2
    · exact tmod_mem hn
  · cases h

/-! ## Expressions -/

/-- The values of a list of operands have the types of the list's types. -/
inductive ValsTyped : List Value → List Ty → Prop
  | nil : ValsTyped [] []
  | cons {v : Value} {t : Ty} {vs : List Value} {ts : List Ty} :
      v.Typed t → ValsTyped vs ts → ValsTyped (v :: vs) (t :: ts)

theorem literalOk_mem {t : IntTy} {v : Int} {u : Unit} (h : Static.literalOk t v = .ok u) :
    t.Mem v := by
  unfold Static.literalOk at h
  split at h
  · rename_i hm
    exact of_decide_eq_true hm
  · exact absurd h reject_ne

theorem getD_mem {l : List Int} {k : Int} (h : k < l.length) (h0 : 0 ≤ k) :
    l.getD k.toNat 0 ∈ l := by
  have hk : k.toNat < l.length := by omega
  rw [List.getD_eq_getElem?_getD, getElem?_pos l k.toNat hk, Option.getD_some]
  exact List.getElem_mem hk

theorem evalAtom_ok {E : Env} {sc : Static.Scope} (hE : EnvTyped E sc) :
    ∀ {a : Atom} {t : Ty}, Static.atomTy sc a = .ok t → Ok (·.Typed t) (evalAtom E a)
  | .int t v, t', h => by
    obtain ⟨u, hl, h⟩ := bind_ok h
    cases h
    exact ⟨rfl, literalOk_mem hl⟩
  | .unit, t', h => by
    cases h
    exact trivial
  | .var x, t', h => by
    obtain ⟨v, hv, hvt⟩ := hE.lookup h
    simp only [evalAtom, hv]
    exact hvt
  | .index b off, t', h => by
    obtain ⟨bt, hb, h⟩ := bind_ok h
    obtain ⟨ot, ho, h⟩ := bind_ok h
    obtain ⟨vb, hvb, hbt⟩ := hE.lookup hb
    have ih := evalAtom_ok hE ho
    cases bt with
    | array et n =>
      simp only at h
      split at h
      · rename_i hot
        cases h
        have hot' : ot = .int .u64 := by simpa using hot
        subst hot'
        obtain ⟨es, rfl, -, hmem⟩ := hbt.array
        simp only [evalAtom, hvb, ok_bind]
        refine Ok.bind ih fun i hi => ?_
        obtain ⟨k, rfl, hk⟩ := hi.int
        have hk0 : 0 ≤ k := by
          simp [IntTy.Mem, IntTy.minVal, IntTy.signed] at hk
          exact hk.1
        simp only
        split
        · rename_i hlt
          exact ⟨rfl, hmem _ (getD_mem hlt hk0)⟩
        · exact trivial
      · exact absurd h reject_ne
    | int _ => exact absurd h reject_ne
    | unit => exact absurd h reject_ne
    | bool => exact absurd h reject_ne
    | named _ => exact absurd h reject_ne

theorem mapM_atoms_ok {E : Env} {sc : Static.Scope} (hE : EnvTyped E sc) :
    ∀ {as : List Atom} {ts : List Ty}, as.mapM (Static.atomTy sc) = .ok ts →
      Ok (fun vs => ValsTyped vs ts) (as.mapM (evalAtom E))
  | [], ts, h => by
    simp only [List.mapM_nil] at h ⊢
    cases h
    exact ValsTyped.nil
  | a :: as, ts, h => by
    rw [List.mapM_cons] at h ⊢
    obtain ⟨t, ht, h⟩ := bind_ok h
    obtain ⟨ts', hts, h⟩ := bind_ok h
    cases h
    exact Ok.bind (evalAtom_ok hE ht) fun v hv =>
      Ok.bind (mapM_atoms_ok hE hts) fun vs hvs => ValsTyped.cons hv hvs

theorem evalInfix_ok {op : InfixOp} {ta tb t : Ty} {va vb : Value}
    (h : Static.infixTy op ta tb = .ok t) (ha : va.Typed ta) (hb : vb.Typed tb) :
    Ok (·.Typed t) (evalInfix op va vb) := by
  rcases op with ⟨o, m⟩ | ⟨o, m⟩ | c
  · cases ta <;> cases tb <;> try exact absurd h reject_ne
    rename_i s s'
    by_cases hss : s = s'
    · subst hss
      obtain ⟨x, rfl, hx⟩ := ha.int
      obtain ⟨y, rfl, hy⟩ := hb.int
      cases m <;> simp [Static.infixTy] at h <;> subst h <;>
        simp only [evalInfix, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte]
      · split
        · rename_i r hr
          exact ⟨rfl, ArithOp.exactForm_mem hr⟩
        · exact trivial
      · exact ⟨rfl, IntTy.wrap_mem _ _⟩
      · exact trivial
    · simp [Static.infixTy, hss, rejectBy] at h
  · cases m
    · cases ta <;> cases tb <;> try exact absurd h reject_ne
      rename_i s s'
      by_cases hss : s = s'
      · subst hss
        obtain ⟨x, rfl, hx⟩ := ha.int
        obtain ⟨y, rfl, hy⟩ := hb.int
        simp [Static.infixTy] at h
        subst h
        simp only [evalInfix, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte]
        split
        · rename_i r hr
          exact ⟨rfl, divExact_mem hx hy hr⟩
        · exact trivial
      · simp [Static.infixTy, hss, rejectBy] at h
    · exact absurd h reject_ne
    · cases ta <;> cases tb <;> try exact absurd h reject_ne
      rename_i s s'
      by_cases hss : s = s'
      · subst hss
        obtain ⟨x, rfl, hx⟩ := ha.int
        obtain ⟨y, rfl, hy⟩ := hb.int
        simp [Static.infixTy] at h
        subst h
        simp only [evalInfix, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte]
        exact trivial
      · simp [Static.infixTy, hss, rejectBy] at h
  · cases c <;> cases ta <;> cases tb <;> try exact absurd h reject_ne
    all_goals first
      | (rename_i s s'
         by_cases hss : s = s'
         · subst hss
           obtain ⟨x, rfl, hx⟩ := ha.int
           obtain ⟨y, rfl, hy⟩ := hb.int
           simp [Static.infixTy] at h
           subst h
           simp only [evalInfix, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte]
           exact trivial
         · simp [Static.infixTy, hss, rejectBy] at h)
      | (obtain ⟨x, rfl⟩ := ha.bool
         obtain ⟨y, rfl⟩ := hb.bool
         simp [Static.infixTy] at h
         subst h
         exact trivial)

theorem evalOp_ok {o : TableOp} {ts : List Ty} {t : Ty} {vs : List Value}
    (h : Static.opTy o ts = .ok t) (hv : ValsTyped vs ts) : Ok (·.Typed t) (evalOp o vs) := by
  rcases hv with _ | ⟨hv1, hv⟩
  · cases o <;> exact absurd h reject_ne
  rcases hv with _ | ⟨hv2, hv⟩
  · rename_i v1 t1
    cases o with
    | unary u m =>
      cases t1 <;> try exact absurd h reject_ne
      rename_i s
      obtain ⟨x, rfl, hx⟩ := hv1.int
      cases hs : s.signed
      · simp [Static.opTy, hs, rejectBy] at h
      · cases m <;> simp [Static.opTy, hs] at h <;> subst h <;>
          simp only [evalOp, hs, Bool.not_true, Bool.false_eq_true, ↓reduceIte]
        · split
          · rename_i r hr
            exact ⟨rfl, UnaryOp.exactForm_mem hs hx hr⟩
          · exact trivial
        · exact ⟨rfl, IntTy.wrap_mem _ _⟩
        · exact trivial
    | bnot =>
      cases t1 <;> try exact absurd h reject_ne
      obtain ⟨x, rfl⟩ := hv1.bool
      simp [Static.opTy] at h
      subst h
      exact trivial
    | band => cases t1 <;> exact absurd h reject_ne
    | bor => cases t1 <;> exact absurd h reject_ne
    | bxor => cases t1 <;> exact absurd h reject_ne
  rcases hv with _ | ⟨hv3, hv⟩
  · rename_i v1 t1 v2 t2
    cases o <;> cases t1 <;> cases t2 <;> try exact absurd h reject_ne
    all_goals
      obtain ⟨x, rfl⟩ := hv1.bool
      obtain ⟨y, rfl⟩ := hv2.bool
      simp [Static.opTy] at h
      subst h
      exact trivial
  · rename_i v1 t1 v2 t2 v3 t3 vs ts
    cases o <;> cases t1 <;> cases t2 <;>
      first | exact absurd h reject_ne | (cases t3 <;> exact absurd h reject_ne)

theorem namedArgs_ok {names : List String} {as : List (String × Atom)} {u : Unit}
    (h : Static.namedArgs names as = .ok u) : as.map (·.1) = names := by
  unfold Static.namedArgs at h
  split at h
  · rename_i hn
    simpa using hn
  · exact absurd h reject_ne

theorem argTypes_cons {sc : Static.Scope} {p : Program} {a : Atom} {t : Ty}
    {rest : List (Atom × Ty)} {u : Unit} (h : Static.argTypes sc p ((a, t) :: rest) = .ok u) :
    Static.atomTy sc a = .ok (Static.normTy p t) ∧ Static.argTypes sc p rest = .ok () := by
  simp only [Static.argTypes, reject_bind, pure_bind] at h
  obtain ⟨ta, hta, h⟩ := bind_ok h
  obtain ⟨hne, h2⟩ := ite_reject h
  have heq : ta = Static.normTy p t := by simpa using hne
  rw [heq] at hta
  exact ⟨hta, h2⟩

theorem atomTy_one {sc : Static.Scope} {p : Program} {as : List (String × Atom)} {name : String}
    {t : Ty} {u v : Unit} (hn : Static.namedArgs [name] as = .ok u)
    (ht : Static.argTypes sc p ((as.map (·.2)).zip [t]) = .ok v) :
    ∃ a, as = [(name, a)] ∧ Static.atomTy sc a = .ok (Static.normTy p t) := by
  have hn' := namedArgs_ok hn
  rcases as with _ | ⟨⟨n, a⟩, _ | ⟨_, _⟩⟩ <;> simp at hn'
  subst hn'
  exact ⟨a, rfl, (argTypes_cons ht).1⟩

theorem evalExpr_ok {p : Program} {E : Env} {sc : Static.Scope} (hE : EnvTyped E sc)
    {e : Expr} {t : Ty} (h : Static.exprTy p sc e = .ok t)
    (hcall : ∀ path targs as g, e = .call path targs as → p.callee? path ≠ some (.fn g)) :
    Ok (·.Typed t) (evalExpr p E e) := by
  rcases e with a | ⟨a, op, b⟩ | ⟨o, as⟩ | ⟨path, targs, as⟩ | b
  · exact evalAtom_ok hE h
  ·
    obtain ⟨ta, ha, h⟩ := bind_ok h
    obtain ⟨tb, hb, h⟩ := bind_ok h
    exact Ok.bind (evalAtom_ok hE ha) fun va hva =>
      Ok.bind (evalAtom_ok hE hb) fun vb hvb => evalInfix_ok h hva hvb
  · obtain ⟨ts, hts, h⟩ := bind_ok h
    exact Ok.bind (mapM_atoms_ok hE hts) fun vs hvs => evalOp_ok h hvs
  · 
    simp only [Static.exprTy] at h
    cases hc : p.callee? path with
    | none =>
      rw [hc] at h
      exact absurd h reject_ne
    | some ce =>
      rw [hc] at h
      cases ce with
      | fn g => exact absurd hc (hcall path targs as g rfl)
      | exitStatus =>
        cases targs with
        | cons _ _ => exact absurd h reject_ne
        | nil =>
          obtain ⟨_, hn, h⟩ := bind_ok h
          obtain ⟨_, ha, h⟩ := bind_ok h
          cases h
          obtain ⟨a, rfl, hat⟩ := atomTy_one hn ha
          simp only [evalExpr, hc]
          refine Ok.bind (evalAtom_ok hE hat) fun v hv => ?_
          obtain ⟨z, rfl, -⟩ := hv.int
          exact rfl
      | arrayFilled =>
        rcases targs with _ | ⟨(et | _ | _ | ⟨_, _⟩ | _) | _, _ | ⟨_ | n, _ | ⟨_, _⟩⟩⟩ <;>
          try exact absurd h reject_ne
        obtain ⟨_, hn, h⟩ := bind_ok h
        obtain ⟨_, ha, h⟩ := bind_ok h
        cases h
        obtain ⟨a, rfl, hat⟩ := atomTy_one hn ha
        simp only [evalExpr, hc]
        refine Ok.bind (evalAtom_ok hE hat) fun v hv => ?_
        obtain ⟨z, rfl, hz⟩ := hv.int
        simp only [beq_self_eq_true, ↓reduceIte]
        refine ⟨rfl, by simp, ?_⟩
        intro w hw
        rw [(List.mem_replicate.mp hw).2]
        exact hz
  · cases h
    exact trivial

theorem evalAffine_ok {E : Env} {sc : Static.Scope} (hE : EnvTyped E sc) {rule : String} :
    ∀ {a : Affine} {u : Unit}, Static.affineOk rule sc a = .ok u →
      Ok (fun _ => True) (evalAffine E a)
  | .atom (.int t v), _, _ => trivial
  | .atom (.var x), _, h => by
    simp only [Static.affineOk] at h
    split at h
    · rename_i y s hf
      obtain ⟨v, hv, hvt⟩ := hE.find x hf
      obtain ⟨z, rfl, -⟩ := hvt.int
      simp only [evalAffine, evalAtom, hv]
      exact trivial
    · exact absurd h reject_ne
    · exact absurd h reject_ne
  | .atom .unit, _, h => absurd h reject_ne
  | .atom (.index _ _), _, h => absurd h reject_ne
  | .add a b, _, h => by
    obtain ⟨_, ha, hb⟩ := bind_ok h
    exact Ok.bind (evalAffine_ok hE ha) fun _ _ => Ok.bind (evalAffine_ok hE hb) fun _ _ => trivial
  | .sub a b, _, h => by
    obtain ⟨_, ha, hb⟩ := bind_ok h
    exact Ok.bind (evalAffine_ok hE ha) fun _ _ => Ok.bind (evalAffine_ok hE hb) fun _ _ => trivial
  | .mul a b, _, h => by
    obtain ⟨_, ha, h⟩ := bind_ok h
    obtain ⟨_, hb, -⟩ := bind_ok h
    exact Ok.bind (evalAffine_ok hE ha) fun _ _ => Ok.bind (evalAffine_ok hE hb) fun _ _ => trivial

/-- Both sides of every relation of a list pass the affine judgment. -/
def RelsOk (rule : String) (sc : Static.Scope) (rs : List Relation) : Prop :=
  ∀ r ∈ rs, (∃ u, Static.affineOk rule sc r.lhs = .ok u) ∧ ∃ u, Static.affineOk rule sc r.rhs = .ok u

theorem allHold_ok {E : Env} {sc : Static.Scope} (hE : EnvTyped E sc) {rule : String} :
    ∀ {rs : List Relation}, RelsOk rule sc rs → Ok (fun _ => True) (allHold E rs)
  | [], _ => trivial
  | r :: rs, h => by
    obtain ⟨⟨_, hl⟩, ⟨_, hr⟩⟩ := h r List.mem_cons_self
    have hh : Ok (fun _ : Bool => True) (r.holds E) :=
      Ok.bind (evalAffine_ok hE hl) fun _ _ => Ok.bind (evalAffine_ok hE hr) fun _ _ => trivial
    simp only [allHold]
    refine Ok.bind hh fun b _ => ?_
    cases b
    · exact trivial
    · exact allHold_ok hE fun r' hr' => h r' (List.mem_cons_of_mem _ hr')

theorem args_ok {p : Program} {E : Env} {sc : Static.Scope} (hE : EnvTyped E sc) :
    ∀ {as : List (String × Atom)} {ps : List Param},
      as.map (·.1) = ps.map (·.name) →
      Static.argTypes sc p ((as.map (·.2)).zip (ps.map (·.ty))) = .ok () →
      Ok (fun args => EnvTyped args (ps.map fun prm => (prm.name, Static.normTy p prm.ty)))
        (as.mapM fun (n, a) => do return (n, ← evalAtom E a))
  | [], [], _, _ => by
    simp only [List.mapM_nil]
    exact EnvTyped.nil
  | [], _ :: _, hn, _ => by simp at hn
  | _ :: _, [], hn, _ => by simp at hn
  | (n, a) :: as, prm :: ps, hn, ht => by
    simp only [List.map_cons, List.cons.injEq] at hn
    obtain ⟨rfl, hn⟩ := hn
    simp only [List.map_cons, List.zip_cons_cons] at ht
    obtain ⟨ha, ht⟩ := argTypes_cons ht
    rw [List.mapM_cons]
    have h1 : Ok (fun b : String × Value => b.1 = prm.name ∧ b.2.Typed (Static.normTy p prm.ty))
        (evalAtom E a >>= fun v => pure (prm.name, v)) :=
      Ok.bind (evalAtom_ok hE ha) fun v hv => ⟨rfl, hv⟩
    refine Ok.bind h1 fun b hb => Ok.bind (args_ok hE hn ht) fun bs hbs => ?_
    obtain ⟨b1, b2⟩ := b
    obtain ⟨rfl, hb2⟩ := hb
    exact EnvTyped.cons hb2 hbs

theorem zip_hasTy {p : Program} :
    ∀ {args : List (String × Value)} {ps : List Param},
      EnvTyped args (ps.map fun prm => (prm.name, Static.normTy p prm.ty)) →
      ∀ x ∈ args.zip ps, x.1.2.hasTy p x.2.ty = true
  | [], _, _, x, hx => by simp at hx
  | _ :: _, [], h, _, _ => by cases h
  | (n, v) :: as, prm :: ps, h, x, hx => by
    simp only [List.map_cons] at h
    cases h with
    | cons hv h' =>
      simp only [List.zip_cons_cons, List.mem_cons] at hx
      rcases hx with rfl | hx
      · exact hv.hasTy
      · exact zip_hasTy h' x hx

/-! ## The judgments without facts, read backward -/

theorem stmts_nil {p : Program} {f : FnDecl} {c : Static.Ctx} {live : Bool} {fr : Static.Flow}
    (h : Static.stmts p f c live [] = .ok fr) : fr = { normal := live, breaks := [] } := by
  simp only [Static.stmts] at h
  exact (Except.ok.inj h).symm

theorem stmts_cons {p : Program} {f : FnDecl} {c : Static.Ctx} {live : Bool} {s : Stmt}
    {ss : List Stmt} {fr : Static.Flow} (h : Static.stmts p f c live (s :: ss) = .ok fr) :
    live = true ∧ ∃ c' fl fr', Static.stmt p f c s = .ok (c', fl) ∧
      Static.stmts p f c' fl.normal ss = .ok fr' ∧
      fr = { normal := fr'.normal, breaks := Static.addBreaks fl.breaks fr'.breaks } := by
  simp only [Static.stmts, reject_bind, pure_bind] at h
  obtain ⟨hl, h⟩ := ite_reject h
  refine ⟨by simpa using hl, ?_⟩
  obtain ⟨⟨c', fl⟩, h1, h⟩ := bind_ok h
  obtain ⟨fr', h2, h⟩ := bind_ok h
  exact ⟨c', fl, fr', h1, h2, (Except.ok.inj h).symm⟩

section stmt
variable {p : Program} {f : FnDecl} {c : Static.Ctx} {r : Static.Ctx × Static.Flow}

theorem stmt_let {x : String} {e : Expr} (h : Static.stmt p f c (.let_ x e) = .ok r) :
    ∃ t, Static.exprTy p c.scope e = .ok t ∧
      r = ({ c with scope := (x, t) :: c.scope, taken := x :: c.taken },
        { normal := true, breaks := [] }) := by
  simp only [Static.stmt] at h
  obtain ⟨_, -, h⟩ := bind_ok h
  obtain ⟨t, ht, h⟩ := bind_ok h
  exact ⟨t, ht, (Except.ok.inj h).symm⟩

theorem stmt_set_var {x : String} {e : Expr} (h : Static.stmt p f c (.set (.var x) e) = .ok r) :
    ∃ t, Static.lookup c.scope x = .ok t ∧ Static.exprTy p c.scope e = .ok t ∧
      r = (c, { normal := true, breaks := [] }) := by
  simp only [Static.stmt, reject_bind, pure_bind] at h
  obtain ⟨-, h⟩ := ite_reject h
  obtain ⟨t, ht, h⟩ := bind_ok h
  obtain ⟨te, hte, h⟩ := bind_ok h
  obtain ⟨hne, h2⟩ := ite_reject h
  have heq : te = t := by simpa using hne
  rw [heq] at hte
  exact ⟨t, ht, hte, (Except.ok.inj h2).symm⟩

theorem stmt_set_index {b : String} {off : Atom} {e : Expr}
    (h : Static.stmt p f c (.set (.index b off) e) = .ok r) :
    ∃ t, Static.atomTy c.scope (.index b off) = .ok t ∧ Static.exprTy p c.scope e = .ok t ∧
      r = (c, { normal := true, breaks := [] }) := by
  simp only [Static.stmt, reject_bind, pure_bind] at h
  obtain ⟨-, h⟩ := ite_reject h
  obtain ⟨t, ht, h⟩ := bind_ok h
  obtain ⟨te, hte, h⟩ := bind_ok h
  obtain ⟨hne, h2⟩ := ite_reject h
  have heq : te = t := by simpa using hne
  rw [heq] at hte
  exact ⟨t, ht, hte, (Except.ok.inj h2).symm⟩

theorem stmt_call {e : Expr} (h : Static.stmt p f c (.call e) = .ok r) :
    (∃ path targs as, e = .call path targs as) ∧ (∃ t, Static.exprTy p c.scope e = .ok t) ∧
      r = (c, { normal := true, breaks := [] }) := by
  cases e
  case call path targs as =>
    simp only [Static.stmt] at h
    obtain ⟨_, hd, h⟩ := bind_ok h
    exact ⟨⟨path, targs, as, rfl⟩, discard_ok hd, (Except.ok.inj h).symm⟩
  all_goals
    simp only [Static.stmt, reject_bind] at h
    exact absurd h reject_ne

theorem stmt_ret {e : Expr} (h : Static.stmt p f c (.ret e) = .ok r) :
    Static.exprTy p c.scope e = .ok (Static.normTy p f.resultTy) ∧
      r = (c, { normal := false, breaks := [] }) := by
  simp only [Static.stmt, reject_bind, pure_bind] at h
  obtain ⟨te, hte, h⟩ := bind_ok h
  obtain ⟨hne, h2⟩ := ite_reject h
  have heq : te = Static.normTy p f.resultTy := by simpa using hne
  rw [heq] at hte
  exact ⟨hte, (Except.ok.inj h2).symm⟩

theorem stmt_if {cond : Expr} {thn els : List Stmt} {he : Bool}
    (h : Static.stmt p f c (.if_ cond thn els he) = .ok r) :
    ∃ a b, Static.exprTy p c.scope cond = .ok .bool ∧ Static.stmts p f c true thn = .ok a ∧
      Static.stmts p f c true els = .ok b ∧
      r = (c, { normal := a.normal || b.normal, breaks := Static.addBreaks a.breaks b.breaks }) := by
  simp only [Static.stmt, reject_bind, pure_bind] at h
  obtain ⟨tc, htc, h⟩ := bind_ok h
  obtain ⟨hne, h⟩ := ite_reject h
  have heq : tc = .bool := by simpa using hne
  subst heq
  obtain ⟨a, ha, h⟩ := bind_ok h
  obtain ⟨b, hb, h⟩ := bind_ok h
  exact ⟨a, b, htc, ha, hb, (Except.ok.inj h).symm⟩

theorem stmt_loop {label : Option String} {invs : List (String × Relation)} {body : List Stmt}
    (h : Static.stmt p f c (.loop label invs body) = .ok r) :
    ∃ fb, Static.stmts p f
        { c with loops := label :: c.loops, invariants := invs.map (·.1) ++ c.invariants }
        true body = .ok fb ∧
      r = (c, { normal := decide (fb.breaks.headD 0 > 0), breaks := fb.breaks.drop 1 }) := by
  cases label with
  | none =>
    simp only [Static.stmt, reject_bind, pure_bind] at h
    obtain ⟨-, h⟩ := ite_reject h
    obtain ⟨_, -, h⟩ := bind_ok h
    obtain ⟨_, -, h⟩ := bind_ok h
    obtain ⟨fb, hfb, h⟩ := bind_ok h
    exact ⟨fb, hfb, (Except.ok.inj h).symm⟩
  | some l =>
    simp only [Static.stmt, reject_bind, pure_bind] at h
    obtain ⟨-, h⟩ := ite_reject h
    obtain ⟨-, h⟩ := ite_reject h
    obtain ⟨_, -, h⟩ := bind_ok h
    obtain ⟨_, -, h⟩ := bind_ok h
    obtain ⟨fb, hfb, h⟩ := bind_ok h
    exact ⟨fb, hfb, (Except.ok.inj h).symm⟩

theorem stmt_brk {label : Option String} (h : Static.stmt p f c (.brk label) = .ok r) :
    Static.resolveLoop c.loops label = .ok () ∧
      r = (c, { normal := false, breaks := Static.breakAt (Static.loopDepth c.loops label) }) := by
  simp only [Static.stmt] at h
  obtain ⟨_, hres, h⟩ := bind_ok h
  exact ⟨hres, (Except.ok.inj h).symm⟩

theorem stmt_cont {label : Option String} (h : Static.stmt p f c (.cont label) = .ok r) :
    Static.resolveLoop c.loops label = .ok () ∧ r = (c, { normal := false, breaks := [] }) := by
  simp only [Static.stmt] at h
  obtain ⟨_, hres, h⟩ := bind_ok h
  exact ⟨hres, (Except.ok.inj h).symm⟩

theorem stmt_invariant {n : String} {rel : Relation}
    (h : Static.stmt p f c (.invariant n rel) = .ok r) :
    r = ({ c with invariants := n :: c.invariants }, { normal := true, breaks := [] }) := by
  simp only [Static.stmt, reject_bind, pure_bind] at h
  obtain ⟨-, h⟩ := ite_reject h
  obtain ⟨_, -, h⟩ := bind_ok h
  exact (Except.ok.inj h).symm

end stmt

/-- The scope of a function's parameters, as its judgment declares them. -/
def paramScope (p : Program) (f : FnDecl) : Static.Scope :=
  f.params.reverse.map fun prm => (prm.name, Static.normTy p prm.ty)

/-- The context a function's body is judged from. -/
def initCtx (p : Program) (f : FnDecl) : Static.Ctx :=
  { scope := paramScope p f, taken := f.params.map (·.name) ++ Static.moduleAliases p,
    loops := [], invariants := [] }

theorem checkFn_inv {p : Program} {f : FnDecl} {u : Unit} (h : Static.checkFn p f = .ok u) :
    RelsOk "FN-8" (paramScope p f) f.requires ∧
    (f.ensures = [] ∨ ∃ t, Static.normTy p f.resultTy = .int t ∧
      RelsOk "FN-9" ((f.result, .int t) :: paramScope p f) f.ensures) ∧
    ∃ fl, Static.stmts p f (initCtx p f) true f.body = .ok fl ∧ fl.normal = false := by
  simp only [Static.checkFn, reject_bind, pure_bind] at h
  obtain ⟨-, h⟩ := ite_reject h
  obtain ⟨_, -, h⟩ := bind_ok h
  obtain ⟨-, h⟩ := ite_reject h
  obtain ⟨-, h⟩ := ite_reject h
  obtain ⟨_, hreq, h⟩ := bind_ok h
  have hR : RelsOk "FN-8" (paramScope p f) f.requires := by
    refine forIn_ok (P := fun (r : Relation) =>
        (∃ u, Static.affineOk "FN-8" (paramScope p f) r.lhs = .ok u) ∧
        ∃ u, Static.affineOk "FN-8" (paramScope p f) r.rhs = .ok u) ?_ hreq
    intro r s hs
    obtain ⟨_, h1, hs⟩ := bind_ok hs
    obtain ⟨_, h2, hs⟩ := bind_ok hs
    exact ⟨⟨⟨_, h1⟩, ⟨_, h2⟩⟩, by cases hs; rfl⟩
  have tail : ∀ {u : Unit}, (Static.stmts p f (initCtx p f) true f.body >>= fun fl =>
      if fl.normal = true then rejectBy "FN-1" s!"the body of `{f.name}` falls through"
      else pure PUnit.unit) = .ok u →
      ∃ fl, Static.stmts p f (initCtx p f) true f.body = .ok fl ∧ fl.normal = false := by
    intro u h
    obtain ⟨fl, hfl, h⟩ := bind_ok h
    obtain ⟨hn, -⟩ := ite_reject h
    exact ⟨fl, hfl, by simpa using hn⟩
  rcases hrt : Static.normTy p f.resultTy with t | _ | _ | ⟨_, _⟩ | _
  · rw [hrt] at h
    obtain ⟨_, hens, h⟩ := bind_ok h
    have hE : RelsOk "FN-9" ((f.result, .int t) :: paramScope p f) f.ensures := by
      refine forIn_ok (P := fun (r : Relation) =>
          (∃ u, Static.affineOk "FN-9" ((f.result, .int t) :: paramScope p f) r.lhs = .ok u) ∧
          ∃ u, Static.affineOk "FN-9" ((f.result, .int t) :: paramScope p f) r.rhs = .ok u)
        ?_ hens
      intro r s hs
      obtain ⟨_, h1, hs⟩ := bind_ok hs
      obtain ⟨_, h2, hs⟩ := bind_ok hs
      exact ⟨⟨⟨_, h1⟩, ⟨_, h2⟩⟩, by cases hs; rfl⟩
    exact ⟨hR, .inr ⟨t, rfl, hE⟩, tail h⟩
  all_goals
    rw [hrt] at h
    obtain ⟨hne, h⟩ := ite_reject h
    exact ⟨hR, .inl (by simpa using hne), tail h⟩

theorem checkProgram_inv {p : Program} (h : Static.checkProgram p = .ok ()) :
    ∀ f ∈ p.fns, ∃ u, Static.checkFn p f = .ok u := by
  simp only [Static.checkProgram] at h
  obtain ⟨_, -, h⟩ := bind_ok h
  obtain ⟨_, h, -⟩ := bind_ok h
  refine forIn_ok (P := fun (f : FnDecl) => ∃ u, Static.checkFn p f = .ok u) ?_ h
  intro f s hs
  obtain ⟨_, h1, hs⟩ := bind_ok hs
  exact ⟨⟨_, h1⟩, by cases hs; rfl⟩

theorem accepts_static {p : Program} (h : p.accepts = true) : Static.checkProgram p = .ok () := by
  unfold Program.accepts at h
  split at h
  · rename_i u hc
    simp only [Program.check] at hc
    obtain ⟨_, h1, -⟩ := bind_ok hc
    exact h1
  · cases h

theorem callee_mem {p : Program} {path : List String} {g : FnDecl}
    (h : p.callee? path = some (.fn g)) : g ∈ p.fns := by
  unfold Program.callee? at h
  split at h
  · split at h
    · rename_i f hf
      cases h
      exact List.mem_of_find?_eq_some hf
    · split at h <;> cases h
  · cases h
  · cases h

theorem exprTy_fn {p : Program} {sc : Static.Scope} {path : List String} {targs : List TArg}
    {as : List (String × Atom)} {t : Ty} {g : FnDecl}
    (h : Static.exprTy p sc (.call path targs as) = .ok t) (hc : p.callee? path = some (.fn g)) :
    as.map (·.1) = g.params.map (·.name) ∧
    Static.argTypes sc p ((as.map (·.2)).zip (g.params.map (·.ty))) = .ok () ∧
    t = Static.normTy p g.resultTy := by
  simp only [Static.exprTy] at h
  rw [hc] at h
  cases targs with
  | cons _ _ => exact absurd h reject_ne
  | nil =>
    obtain ⟨_, hn, h⟩ := bind_ok h
    obtain ⟨_, ha, h⟩ := bind_ok h
    exact ⟨namedArgs_ok hn, ha, (Except.ok.inj h).symm⟩

/-! ## Break counts -/

theorem addBreaks_getD : ∀ (a b : List Nat) (j : Nat),
    (Static.addBreaks a b).getD j 0 = a.getD j 0 + b.getD j 0
  | [], b, j => by simp [Static.addBreaks]
  | _ :: _, [], j => by simp [Static.addBreaks]
  | _ :: _, _ :: _, 0 => by simp [Static.addBreaks]
  | _ :: xs, _ :: ys, j + 1 => by simpa [Static.addBreaks] using addBreaks_getD xs ys j

theorem breakAt_getD : ∀ j : Nat, (Static.breakAt j).getD j 0 = 1
  | 0 => by simp [Static.breakAt]
  | j + 1 => by simpa [Static.breakAt] using breakAt_getD j

theorem drop1_getD (l : List Nat) (j : Nat) : (l.drop 1).getD j 0 = l.getD (j + 1) 0 := by
  cases l <;> simp

/-- Every loop a counted `break` leaves accepts a normal arrival after it:
`ex` holds, innermost first, whether each enclosing loop's continuation does. -/
def BreaksOk (bs : List Nat) (ex : List Bool) : Prop :=
  ∀ j e t, ex.drop j = e :: t → 0 < bs.getD j 0 → e = true

theorem BreaksOk.left {a b : List Nat} {ex : List Bool} (h : BreaksOk (Static.addBreaks a b) ex) :
    BreaksOk a ex := fun j e t hd hp => h j e t hd (by rw [addBreaks_getD]; omega)

theorem BreaksOk.right {a b : List Nat} {ex : List Bool} (h : BreaksOk (Static.addBreaks a b) ex) :
    BreaksOk b ex := fun j e t hd hp => h j e t hd (by rw [addBreaks_getD]; omega)

theorem BreaksOk.nil {bs : List Nat} : BreaksOk bs [] := fun j e t hd _ => by simp at hd

/-! ## Continuations -/

/-- A context extends another: the same loops, and a scope that ends with the
other's. -/
def Extends (c' c : Static.Ctx) : Prop :=
  c'.loops = c.loops ∧ ∃ new, c'.scope = new ++ c.scope

theorem Extends.refl (c : Static.Ctx) : Extends c c := ⟨rfl, [], rfl⟩

theorem Extends.trans {a b c : Static.Ctx} (h1 : Extends a b) (h2 : Extends b c) : Extends a c := by
  obtain ⟨hl1, n1, hs1⟩ := h1
  obtain ⟨hl2, n2, hs2⟩ := h2
  exact ⟨hl1.trans hl2, n1 ++ n2, by rw [hs1, hs2, List.append_assoc]⟩

/-- A continuation passed the statement judgments of function `f`, entered
from context `c`. `a` is false only where no normal arrival can happen; `ex`
holds, innermost first, whether control may go on normally after each loop of
the continuation, as the loop's `break` count says.

- The end of the body: no normal arrival, and no loop left.
- A statement list: the judgments accept it from a reachability that a normal
  arrival makes true; its normal exit continues from any context extending
  `c` (the list's own bindings) with the reachability its judgment returned.
- A scope end: the bindings go back to a context `c` extends.
- A loop marker: its body passes from the loop's inner context `ci` (`c`
  extends it), its exit continuation accepts the loop's normal exit, and every
  loop a `break` in the body leaves accepts the arrival. -/
def KontOk (p : Program) (f : FnDecl) : Static.Ctx → Bool → List Bool → List Kont → Prop
  | c, a, ex, [] => a = false ∧ c.loops = [] ∧ ex = []
  | c, a, ex, .stmts _ _ ss :: k => ∃ a' fr, (a = true → a' = true) ∧
      Static.stmts p f c a' ss = .ok fr ∧ BreaksOk fr.breaks ex ∧
      ∀ c', Extends c' c → KontOk p f c' fr.normal ex k
  | c, a, ex, .scope d :: k => ∃ c0, Extends c c0 ∧ c0.scope.length = d ∧ KontOk p f c0 a ex k
  | c, _, ex, .loop l _ body d :: k => ∃ ci c0 fb e ex0, ex = e :: ex0 ∧ Extends c ci ∧
      ci.scope = c0.scope ∧ ci.loops = l :: c0.loops ∧ c0.scope.length = d ∧
      Static.stmts p f ci true body = .ok fb ∧ e = decide (fb.breaks.headD 0 > 0) ∧
      BreaksOk fb.breaks ex ∧ KontOk p f c0 e ex0 k

theorem KontOk.weaken {p : Program} {f : FnDecl} {k : List Kont} :
    ∀ {c : Static.Ctx} {a a' : Bool} {ex : List Bool},
      KontOk p f c a ex k → (a' = true → a = true) → KontOk p f c a' ex k := by
  induction k with
  | nil =>
    intro c a a' ex h ha
    simp only [KontOk] at h ⊢
    obtain ⟨h1, h2, h3⟩ := h
    refine ⟨?_, h2, h3⟩
    cases a' <;> simp_all
  | cons k0 k ih =>
    intro c a a' ex h ha
    cases k0 with
    | stmts blk i ss =>
      simp only [KontOk] at h ⊢
      obtain ⟨b, fr, hb, h⟩ := h
      exact ⟨b, fr, fun h' => hb (ha h'), h⟩
    | scope d =>
      simp only [KontOk] at h ⊢
      obtain ⟨c0, h1, h2, h3⟩ := h
      exact ⟨c0, h1, h2, ih h3 ha⟩
    | loop l blk body d =>
      exact h

/-- The statement list after a statement, from the context the statement's
judgment returned. -/
theorem KontOk.next {p : Program} {f : FnDecl} {c c' : Static.Ctx} {fl fr' : Static.Flow}
    {ex : List Bool} {blk : Pos} {i : Nat} {ss : List Stmt} {k : List Kont}
    (hfr' : Static.stmts p f c' fl.normal ss = .ok fr')
    (hb : BreaksOk (Static.addBreaks fl.breaks fr'.breaks) ex)
    (ht : ∀ c'', Extends c'' c → KontOk p f c'' fr'.normal ex k) (hext : Extends c' c) :
    KontOk p f c' fl.normal ex (.stmts blk (i + 1) ss :: k) := by
  simp only [KontOk]
  exact ⟨fl.normal, fr', id, hfr', hb.right, fun c'' h => ht c'' (h.trans hext)⟩

theorem findIdx_some : ∀ {ls : Static.Loops} {x : Option String}, ls.contains x = true →
    ∃ i, ls.findIdx? (· == x) = some i
  | [], _, h => by simp at h
  | y :: ys, x, h => by
    by_cases hy : y = x
    · exact ⟨0, by simp [List.findIdx?_cons, hy]⟩
    · have hxy : (x == y) = false := by
        simp only [beq_eq_false_iff_ne, ne_eq]
        exact fun h => hy h.symm
      rw [List.contains_cons, hxy, Bool.false_or] at h
      obtain ⟨i, hi⟩ := findIdx_some h
      exact ⟨i + 1, by simp [List.findIdx?_cons, hy, hi]⟩

theorem loopDepth_cons {l : Option String} {ls : Static.Loops} {l' : String}
    (hne : l ≠ some l') (hc : ls.contains (some l') = true) :
    Static.loopDepth (l :: ls) (some l') = Static.loopDepth ls (some l') + 1 := by
  obtain ⟨i, hi⟩ := findIdx_some hc
  simp [Static.loopDepth, List.findIdx?_cons, hne, hi]

theorem resolve_some {ls : Static.Loops} {l' : String} {u : Unit}
    (h : Static.resolveLoop ls (some l') = .ok u) : ls.contains (some l') = true := by
  simp only [Static.resolveLoop] at h
  split at h
  · assumption
  · exact absurd h reject_ne

/-- A `break` or `continue` that resolves statically finds its loop marker in
the continuation, under the scope the marker's loop was entered from, with the
exit flag the break counts index. -/
theorem findLoop_ok {p : Program} {f : FnDecl} {k : List Kont} :
    ∀ {c : Static.Ctx} {a : Bool} {ex : List Bool} {label : Option String},
      KontOk p f c a ex k → Static.resolveLoop c.loops label = .ok () →
      ∃ l blk body d rest cm exm, findLoop label k = some (l, blk, body, d, rest) ∧
        KontOk p f cm true exm (.loop l blk body d :: rest) ∧
        (∃ new, c.scope = new ++ cm.scope) ∧ ex.drop (Static.loopDepth c.loops label) = exm := by
  induction k with
  | nil =>
    intro c a ex label h hr
    simp only [KontOk] at h
    obtain ⟨-, hl, -⟩ := h
    rw [hl] at hr
    cases label <;> simp [Static.resolveLoop, rejectBy] at hr
  | cons k0 k ih =>
    intro c a ex label h hr
    cases k0 with
    | stmts blk i ss =>
      simp only [KontOk] at h
      obtain ⟨_, fr, -, -, -, ht⟩ := h
      exact ih (ht c (.refl c)) hr
    | scope d =>
      simp only [KontOk] at h
      obtain ⟨c0, ⟨hl, new, hs⟩, -, h⟩ := h
      rw [hl] at hr
      obtain ⟨l, blk, body, d', rest, cm, exm, hf, hm, ⟨new', hs'⟩, hd⟩ := ih h hr
      refine ⟨l, blk, body, d', rest, cm, exm, hf, hm, ⟨new ++ new', ?_⟩, ?_⟩
      · rw [hs, hs', List.append_assoc]
      · rw [hl]; exact hd
    | loop l blk body d =>
      have hm := h
      simp only [KontOk] at h
      obtain ⟨ci, c0, fb, e, ex0, rfl, ⟨hl, new, hs⟩, hsc, hlo, -, -, -, -, hk⟩ := h
      by_cases hmatch : (label.isNone || label == l) = true
      · refine ⟨l, blk, body, d, k, c, e :: ex0, by simp only [findLoop, hmatch, ↓reduceIte],
          ?_, ⟨[], rfl⟩, ?_⟩
        · simp only [KontOk] at hm ⊢
          exact hm
        · cases label with
          | none => rfl
          | some l' =>
            have hl' : l = some l' := by
              simp at hmatch
              first | exact hmatch.symm | exact hmatch
            rw [hl, hlo, hl']
            simp [Static.loopDepth, List.findIdx?_cons]
      · cases label with
        | none => simp at hmatch
        | some l' =>
          have hne : l ≠ some l' := by
            intro he
            simp [he] at hmatch
          rw [hl, hlo] at hr
          have hc0 : c0.loops.contains (some l') = true := by
            have := resolve_some hr
            rw [List.contains_cons] at this
            simpa [Ne.symm hne] using this
          have hr0 : Static.resolveLoop c0.loops (some l') = .ok () := by
            simp only [Static.resolveLoop, hc0, ↓reduceIte]
          obtain ⟨l2, blk2, body2, d2, rest2, cm, exm, hf, hm2, ⟨new', hs'⟩, hd⟩ := ih hk hr0
          refine ⟨l2, blk2, body2, d2, rest2, cm, exm, ?_, hm2, ⟨new ++ new', ?_⟩, ?_⟩
          · simp only [findLoop, hmatch, Bool.false_eq_true, ↓reduceIte]
            exact hf
          · rw [hs, hsc, hs', List.append_assoc]
          · rw [hl, hlo, loopDepth_cons hne hc0]
            simpa using hd

/-! ## Frames and configurations -/

/-- A frame whose next step runs: its function is the program's, its entry
values have its parameters' types, and its bindings have the types of a
context its continuation is accepted from, reached normally. -/
def FrameOk (p : Program) (fr : Frame) : Prop :=
  fr.fn ∈ p.fns ∧ EnvTyped fr.entry (paramScope p fr.fn) ∧
    ∃ c ex, EnvTyped fr.env c.scope ∧ KontOk p fr.fn c true ex fr.kont

/-- The frames below a running frame take its result, of static type `t`, at
its destination. -/
def StackOk (p : Program) (t : Ty) : Dest → List Frame → Prop
  | .exit, _ => True
  | .ret, g :: rest => t = Static.normTy p g.fn.resultTy ∧ g.fn ∈ p.fns ∧
      EnvTyped g.entry (paramScope p g.fn) ∧ StackOk p t g.dest rest
  | .bind x, g :: rest => (∀ v, v.Typed t → FrameOk p { g with env := (x, v) :: g.env }) ∧
      StackOk p (Static.normTy p g.fn.resultTy) g.dest rest
  | .assign tg, g :: rest =>
      (∀ v, v.Typed t → Ok (fun g' => FrameOk p g' ∧ g'.fn = g.fn ∧ g'.dest = g.dest)
        (commitTo g tg v)) ∧
      StackOk p (Static.normTy p g.fn.resultTy) g.dest rest
  | .discard, g :: rest => FrameOk p g ∧ StackOk p (Static.normTy p g.fn.resultTy) g.dest rest
  | .test pos thn els, g :: rest =>
      (∀ v, v.Typed t → Ok (fun g' => FrameOk p g' ∧ g'.fn = g.fn ∧ g'.dest = g.dest)
        (branch g v pos thn els)) ∧
      StackOk p (Static.normTy p g.fn.resultTy) g.dest rest
  | _, [] => False

end Typing

open Typing in
/-- A well-typed machine state: the program passes the judgments without facts,
the running frame is typed, and the frames below take its result. -/
def Config.WellTyped (p : Program) (c : Config) : Prop :=
  Static.checkProgram p = .ok () ∧
  match c.frames with
  | f :: callers => FrameOk p f ∧ StackOk p (Static.normTy p f.fn.resultTy) f.dest callers
  | [] => False

namespace Typing

/-- A step that does not stop ill formed and, when it goes on, reaches a
well-typed state. -/
def StepOk (p : Program) : Step → Prop
  | .next c => c.WellTyped p
  | .done _ => True
  | .halt (.stuck _) => True
  | .halt (.illFormed _) => False

theorem StepOk.ofEval {p : Program} {α : Type} {P : α → Prop} {x : Eval α} {k : α → Step}
    (hx : Ok P x) (hk : ∀ a, P a → StepOk p (k a)) : StepOk p (Step.ofEval x k) := by
  match x, hx with
  | .ok a, hx => exact hk a hx
  | .error (.stuck _), _ => exact trivial
  | .error (.illFormed _), hx => exact (hx : False).elim

theorem checkFn_of_mem {p : Program} (hp : Static.checkProgram p = .ok ()) {g : FnDecl}
    (hg : g ∈ p.fns) : ∃ u, Static.checkFn p g = .ok u :=
  checkProgram_inv hp g hg

/-- A function's body, entered, is accepted from its initial context. -/
theorem kont_entry {p : Program} {g : FnDecl} {fl : Static.Flow}
    (hfl : Static.stmts p g (initCtx p g) true g.body = .ok fl) (hn : fl.normal = false) :
    KontOk p g (initCtx p g) true [] [.stmts [] 0 g.body] := by
  simp only [KontOk]
  refine ⟨true, fl, fun _ => rfl, hfl, BreaksOk.nil, fun c' hc' => ?_⟩
  exact ⟨hn, hc'.1, by trivial⟩

theorem enter_ok {p : Program} (hp : Static.checkProgram p = .ok ()) {g : FnDecl} (hg : g ∈ p.fns)
    {args : List (String × Value)}
    (hargs : EnvTyped args (g.params.map fun prm => (prm.name, Static.normTy p prm.ty)))
    {dest : Dest} {callers : List Frame}
    (hst : StackOk p (Static.normTy p g.resultTy) dest callers) :
    StepOk p (enter p g args dest callers) := by
  obtain ⟨_, hfn⟩ := checkFn_of_mem hp hg
  obtain ⟨hreq, -, fl, hfl, hnorm⟩ := checkFn_inv hfn
  have hnames : args.map (·.1) = g.params.map (·.name) := by
    rw [hargs.names]
    simp
  have hrev : EnvTyped args.reverse (paramScope p g) := by
    have := hargs.reverse
    simpa [paramScope] using this
  unfold enter
  split
  · rename_i hne
    simp [hnames] at hne
  split
  · rename_i hall
    exfalso
    have : (args.zip g.params).all (fun x => x.1.2.hasTy p x.2.ty) = true := by
      rw [List.all_eq_true]
      exact zip_hasTy hargs
    simp_all
  refine StepOk.ofEval (allHold_ok hrev hreq) fun b _ => ?_
  cases b
  · exact trivial
  · exact ⟨hp, ⟨hg, hrev, initCtx p g, [], hrev, kont_entry hfl hnorm⟩, hst⟩

/-- Returning a value of the frame's result type. -/
theorem doReturn_ok {p : Program} (hp : Static.checkProgram p = .ok ()) {v : Value} :
    ∀ {callers : List Frame} {f : Frame}, f.fn ∈ p.fns →
      EnvTyped f.entry (paramScope p f.fn) → v.Typed (Static.normTy p f.fn.resultTy) →
      StackOk p (Static.normTy p f.fn.resultTy) f.dest callers →
      StepOk p (doReturn p v (f :: callers))
  | callers, f, hmem, hentry, hv, hst => by
    obtain ⟨_, hfn⟩ := checkFn_of_mem hp hmem
    obtain ⟨-, hens, -⟩ := checkFn_inv hfn
    have hA : Ok (fun _ => True) (allHold ((f.fn.result, v) :: f.entry) f.fn.ensures) := by
      rcases hens with he | ⟨t, ht, hR⟩
      · rw [he]
        exact trivial
      · rw [ht] at hv
        exact allHold_ok (.cons hv hentry) hR
    simp only [doReturn, hv.hasTy, Bool.not_true, Bool.false_eq_true, ↓reduceIte]
    cases hres : allHold ((f.fn.result, v) :: f.entry) f.fn.ensures with
    | error h =>
      rw [hres] at hA
      cases h with
      | stuck s => exact trivial
      | illFormed w => exact (hA : False).elim
    | ok b =>
      cases b
      · exact trivial
      · cases hd : f.dest with
        | exit => exact trivial
        | ret =>
          rw [hd] at hst
          cases callers with
          | nil => exact hst.elim
          | cons g rest =>
            obtain ⟨ht, hg, hge, hst'⟩ := hst
            rw [ht] at hv hst'
            exact doReturn_ok hp hg hge hv hst'
        | bind x =>
          rw [hd] at hst
          cases callers with
          | nil => exact hst.elim
          | cons g rest => exact ⟨hp, hst.1 v hv, hst.2⟩
        | assign tg =>
          rw [hd] at hst
          cases callers with
          | nil => exact hst.elim
          | cons g rest =>
            refine StepOk.ofEval (hst.1 v hv) fun g' ⟨hF, hfn', hdst⟩ => ?_
            refine ⟨hp, hF, ?_⟩
            rw [hfn', hdst]
            exact hst.2
        | discard =>
          rw [hd] at hst
          cases callers with
          | nil => exact hst.elim
          | cons g rest => exact ⟨hp, hst.1, hst.2⟩
        | test pos thn els =>
          rw [hd] at hst
          cases callers with
          | nil => exact hst.elim
          | cons g rest =>
            refine StepOk.ofEval (hst.1 v hv) fun g' ⟨hF, hfn', hdst⟩ => ?_
            refine ⟨hp, hF, ?_⟩
            rw [hfn', hdst]
            exact hst.2

/-- A right-hand side of static type `t`: its value goes to `fin`, or a call
enters a function whose result the frames below take. -/
theorem evalRhs_ok {p : Program} (hp : Static.checkProgram p = .ok ()) {f : Frame}
    {callers : List Frame} {sc : Static.Scope} (hE : EnvTyped f.env sc) {e : Expr} {t : Ty}
    (het : Static.exprTy p sc e = .ok t) {dest : Dest} {K : List Kont} {fin : Value → Step}
    (hfin : ∀ v, v.Typed t → StepOk p (fin v))
    (hcall : StackOk p t dest ({ f with kont := K } :: callers)) :
    StepOk p (evalRhs p f callers e dest K fin) := by
  cases e
  case call path targs as =>
    cases hc : p.callee? path with
    | some ce =>
      cases ce with
      | fn g =>
        obtain ⟨hn, hat, rfl⟩ := exprTy_fn het hc
        simp only [evalRhs, hc]
        exact StepOk.ofEval (args_ok hE hn hat) fun args hargs =>
          enter_ok hp (callee_mem hc) hargs hcall
      | exitStatus =>
        simp only [evalRhs, hc]
        exact StepOk.ofEval (evalExpr_ok hE het fun _ _ _ g he => by cases he; rw [hc]; simp) hfin
      | arrayFilled =>
        simp only [evalRhs, hc]
        exact StepOk.ofEval (evalExpr_ok hE het fun _ _ _ g he => by cases he; rw [hc]; simp) hfin
    | none =>
      simp only [evalRhs, hc]
      exact StepOk.ofEval (evalExpr_ok hE het fun _ _ _ g he => by cases he; rw [hc]; simp) hfin
  all_goals
    exact StepOk.ofEval (evalExpr_ok hE het fun _ _ _ _ he => nomatch he) hfin

/-- Writing a whole binding. -/
theorem commitTo_var {g : Frame} {sc : Static.Scope} (hE : EnvTyped g.env sc) {x : String}
    {t : Ty} (hl : Static.lookup sc x = .ok t) {v : Value} (hv : v.Typed t) :
    Ok (fun g' => g' = { g with env := g'.env } ∧ EnvTyped g'.env sc) (commitTo g (.var x) v) := by
  obtain ⟨old, hold, hot⟩ := hE.lookup hl
  obtain ⟨E', hput, hE'⟩ := hE.put hl hv
  simp only [commitTo, write, hold, ok_bind, hot.sameTy hv, ↓reduceIte, pure_bind, hput]
  exact ⟨rfl, hE'⟩

/-- Writing one element of an `Array` binding. -/
theorem commitTo_index {g : Frame} {sc : Static.Scope} (hE : EnvTyped g.env sc) {b : String}
    {off : Atom} {t : Ty} (ht : Static.atomTy sc (.index b off) = .ok t) {v : Value}
    (hv : v.Typed t) :
    Ok (fun g' => g' = { g with env := g'.env } ∧ EnvTyped g'.env sc)
      (commitTo g (.index b off) v) := by
  have hread := evalAtom_ok hE ht
  obtain ⟨bt, hb, h⟩ := bind_ok ht
  obtain ⟨ot, ho, h⟩ := bind_ok h
  obtain ⟨vb, hvb, hbt⟩ := hE.lookup hb
  cases bt with
  | array et n =>
    simp only at h
    split at h
    · rename_i hot
      cases h
      have hot' : ot = .int .u64 := by simpa using hot
      subst hot'
      obtain ⟨es, rfl, hlen, hmem⟩ := hbt.array
      obtain ⟨w, rfl, hw⟩ := hv.int
      simp only [commitTo]
      refine Ok.bind hread fun _ _ => Ok.bind (evalAtom_ok hE ho) fun i hi => ?_
      obtain ⟨k, rfl, -⟩ := hi.int
      have hnew : (Value.array et (es.set k.toNat w)).Typed (.array et n) := by
        refine ⟨rfl, by simpa using hlen, fun z hz => ?_⟩
        rcases List.mem_or_eq_of_mem_set hz with hz | rfl
        · exact hmem z hz
        · exact hw
      obtain ⟨E', hput, hE'⟩ := hE.put hb hnew
      simp only [write, hvb, ok_bind, beq_self_eq_true, ↓reduceIte, pure_bind, hput]
      exact ⟨rfl, hE'⟩
    · exact absurd h reject_ne
  | int _ => exact absurd h reject_ne
  | unit => exact absurd h reject_ne
  | bool => exact absurd h reject_ne
  | named _ => exact absurd h reject_ne

/-- Entering the block an `if` selects. -/
theorem branch_ok {p : Program} {g : Frame} {c : Static.Ctx} {ex : List Bool}
    (hmem : g.fn ∈ p.fns) (hentry : EnvTyped g.entry (paramScope p g.fn))
    (hE : EnvTyped g.env c.scope) {thn els : List Stmt} {a b : Static.Flow}
    (ha : Static.stmts p g.fn c true thn = .ok a) (hb : Static.stmts p g.fn c true els = .ok b)
    (hbr : BreaksOk (Static.addBreaks a.breaks b.breaks) ex)
    (hK : KontOk p g.fn c (a.normal || b.normal) ex g.kont) {pos : Pos} {v : Value}
    (hv : v.Typed .bool) :
    Ok (fun g' => FrameOk p g' ∧ g'.fn = g.fn ∧ g'.dest = g.dest) (branch g v pos thn els) := by
  obtain ⟨bv, rfl⟩ := hv.bool
  refine ⟨⟨hmem, hentry, c, ex, hE, ?_⟩, rfl, rfl⟩
  simp only [KontOk]
  cases bv
  · refine ⟨true, b, fun _ => rfl, hb, hbr.right, fun c' hc' => ?_⟩
    exact ⟨c, hc', hE.length.symm, hK.weaken (by intro h; simp [h])⟩
  · refine ⟨true, a, fun _ => rfl, ha, hbr.left, fun c' hc' => ?_⟩
    exact ⟨c, hc', hE.length.symm, hK.weaken (by intro h; simp [h])⟩

/-- One statement of the running frame. -/
theorem stepStmt_ok {p : Program} (hp : Static.checkProgram p = .ok ()) {f : Frame}
    {callers : List Frame} {c : Static.Ctx} {ex : List Bool} {pos : Pos} {s : Stmt}
    {blk : Pos} {i : Nat} {ss : List Stmt} {k : List Kont}
    (hmem : f.fn ∈ p.fns) (hentry : EnvTyped f.entry (paramScope p f.fn))
    (hE : EnvTyped f.env c.scope)
    (hst : StackOk p (Static.normTy p f.fn.resultTy) f.dest callers)
    {c' : Static.Ctx} {fl fr' : Static.Flow}
    (hs : Static.stmt p f.fn c s = .ok (c', fl))
    (hfr' : Static.stmts p f.fn c' fl.normal ss = .ok fr')
    (hb : BreaksOk (Static.addBreaks fl.breaks fr'.breaks) ex)
    (ht : ∀ c'', Extends c'' c → KontOk p f.fn c'' fr'.normal ex k) :
    StepOk p (stepStmt p f callers pos s (.stmts blk (i + 1) ss :: k)) := by
  cases s with
  | let_ x e =>
    obtain ⟨t, het, hr⟩ := stmt_let hs
    obtain ⟨hc, rfl⟩ := Prod.mk.inj hr
    subst c'
    have hK := KontOk.next (blk := blk) (i := i) hfr' hb ht ⟨rfl, [(x, t)], rfl⟩
    have hF : ∀ v, v.Typed t →
        FrameOk p { f with env := (x, v) :: f.env, kont := .stmts blk (i + 1) ss :: k } :=
      fun v hv => ⟨hmem, hentry, _, ex, .cons hv hE, hK⟩
    simp only [stepStmt, stepLet]
    exact evalRhs_ok hp hE het (fun v hv => ⟨hp, hF v hv, hst⟩) ⟨hF, hst⟩
  | set tg e =>
    cases tg with
    | var x =>
      obtain ⟨t, hl, het, hr⟩ := stmt_set_var hs
      obtain ⟨hc, rfl⟩ := Prod.mk.inj hr
      subst c'
      have hK := KontOk.next (blk := blk) (i := i) hfr' hb ht (.refl c)
      have hW : ∀ v, v.Typed t → Ok (fun g' => FrameOk p g' ∧ g'.fn = f.fn ∧ g'.dest = f.dest)
          (commitTo { f with kont := .stmts blk (i + 1) ss :: k } (.var x) v) := fun v hv =>
        (commitTo_var (g := { f with kont := .stmts blk (i + 1) ss :: k }) hE hl hv).mono
          fun g' ⟨hg', hE'⟩ => by
            rw [hg']
            exact ⟨⟨hmem, hentry, c, ex, hE', hK⟩, rfl, rfl⟩
      simp only [stepStmt, stepSet]
      exact evalRhs_ok hp hE het
        (fun v hv => StepOk.ofEval (hW v hv) fun g' ⟨hF, hfn, hd⟩ =>
          ⟨hp, hF, by rw [hfn, hd]; exact hst⟩) ⟨hW, hst⟩
    | index b off =>
      obtain ⟨t, hat, het, hr⟩ := stmt_set_index hs
      obtain ⟨hc, rfl⟩ := Prod.mk.inj hr
      subst c'
      have hK := KontOk.next (blk := blk) (i := i) hfr' hb ht (.refl c)
      have hW : ∀ v, v.Typed t → Ok (fun g' => FrameOk p g' ∧ g'.fn = f.fn ∧ g'.dest = f.dest)
          (commitTo { f with kont := .stmts blk (i + 1) ss :: k } (.index b off) v) := fun v hv =>
        (commitTo_index (g := { f with kont := .stmts blk (i + 1) ss :: k }) hE hat hv).mono
          fun g' ⟨hg', hE'⟩ => by
            rw [hg']
            exact ⟨⟨hmem, hentry, c, ex, hE', hK⟩, rfl, rfl⟩
      simp only [stepStmt, stepSet]
      exact evalRhs_ok hp hE het
        (fun v hv => StepOk.ofEval (hW v hv) fun g' ⟨hF, hfn, hd⟩ =>
          ⟨hp, hF, by rw [hfn, hd]; exact hst⟩) ⟨hW, hst⟩
  | call e =>
    obtain ⟨-, ⟨t, het⟩, hr⟩ := stmt_call hs
    obtain ⟨hc, rfl⟩ := Prod.mk.inj hr
    subst c'
    have hK := KontOk.next (blk := blk) (i := i) hfr' hb ht (.refl c)
    have hF : FrameOk p { f with kont := .stmts blk (i + 1) ss :: k } :=
      ⟨hmem, hentry, c, ex, hE, hK⟩
    simp only [stepStmt]
    exact evalRhs_ok hp hE het (fun _ _ => ⟨hp, hF, hst⟩) ⟨hF, hst⟩
  | ret e =>
    obtain ⟨het, hr⟩ := stmt_ret hs
    simp only [stepStmt]
    exact evalRhs_ok hp hE het (fun v hv => doReturn_ok hp hmem hentry hv hst)
      ⟨rfl, hmem, hentry, hst⟩
  | if_ cond thn els he =>
    obtain ⟨a, b, het, ha, hb', hr⟩ := stmt_if hs
    obtain ⟨hc, rfl⟩ := Prod.mk.inj hr
    subst c'
    have hK := KontOk.next (blk := blk) (i := i) hfr' hb ht (.refl c)
    have hW : ∀ v, v.Typed .bool → Ok (fun g' => FrameOk p g' ∧ g'.fn = f.fn ∧ g'.dest = f.dest)
        (branch { f with kont := .stmts blk (i + 1) ss :: k } v pos thn els) := fun v hv =>
      branch_ok (g := { f with kont := .stmts blk (i + 1) ss :: k }) hmem hentry hE ha hb'
        hb.left hK hv
    simp only [stepStmt, stepIf]
    exact evalRhs_ok hp hE het
      (fun v hv => StepOk.ofEval (hW v hv) fun g' ⟨hF, hfn, hd⟩ =>
        ⟨hp, hF, by rw [hfn, hd]; exact hst⟩) ⟨hW, hst⟩
  | loop label invs body =>
    obtain ⟨fb, hfb, hr⟩ := stmt_loop hs
    obtain ⟨hc, rfl⟩ := Prod.mk.inj hr
    subst c'
    have hK := KontOk.next (blk := blk) (i := i) hfr' hb ht (.refl c)
    have hBO : BreaksOk fb.breaks (decide (fb.breaks.headD 0 > 0) :: ex) := by
      intro j e t hd hpos
      cases j with
      | zero =>
        simp only [List.drop_zero, List.cons.injEq] at hd
        obtain ⟨rfl, -⟩ := hd
        cases hfb' : fb.breaks with
        | nil => rw [hfb'] at hpos; simp at hpos
        | cons x xs => rw [hfb'] at hpos; simpa using hpos
      | succ j =>
        simp only [List.drop_succ_cons] at hd
        exact hb.left j e t hd (by rw [drop1_getD]; exact hpos)
    simp only [stepStmt, stepStmt.stepLoop]
    refine ⟨hp, ⟨hmem, hentry,
      { c with loops := label :: c.loops, invariants := invs.map (·.1) ++ c.invariants },
      decide (fb.breaks.headD 0 > 0) :: ex, hE, ?_⟩, hst⟩
    simp only [KontOk]
    refine ⟨true, fb, fun _ => rfl, hfb, hBO, fun c'' hc'' => ?_⟩
    exact ⟨_, c, fb, _, ex, rfl, hc'', rfl, rfl, hE.length.symm, hfb, rfl, hBO, hK⟩
  | brk label =>
    obtain ⟨hres, hr⟩ := stmt_brk hs
    obtain ⟨hc, rfl⟩ := Prod.mk.inj hr
    subst c'
    obtain ⟨l, blk', body, d, rest, cm, exm, hfind, hm, ⟨new, hsc⟩, hdrop⟩ :=
      findLoop_ok (ht c (.refl c)) hres
    simp only [KontOk] at hm
    obtain ⟨ci, c0, fb, e, ex0, rfl, ⟨-, new2, hci⟩, hsc0, -, rfl, -, -, -, hk0⟩ := hm
    have he : e = true :=
      hb _ e ex0 hdrop (by rw [addBreaks_getD, breakAt_getD]; omega)
    subst he
    have hfind' : findLoop label (.stmts blk (i + 1) ss :: k) =
        some (l, blk', body, c0.scope.length, rest) := hfind
    simp only [stepStmt, stepStmt.stepBreak, hfind']
    have hs' : c.scope = (new ++ new2) ++ c0.scope := by
      rw [hsc, hci, hsc0, List.append_assoc]
    rw [hs'] at hE
    exact ⟨hp, ⟨hmem, hentry, c0, ex0, hE.restore, hk0⟩, hst⟩
  | cont label =>
    obtain ⟨hres, hr⟩ := stmt_cont hs
    obtain ⟨hc, rfl⟩ := Prod.mk.inj hr
    subst c'
    obtain ⟨l, blk', body, d, rest, cm, exm, hfind, hm, ⟨new, hsc⟩, -⟩ :=
      findLoop_ok (ht c (.refl c)) hres
    simp only [KontOk] at hm
    obtain ⟨ci, c0, fb, e, ex0, rfl, ⟨-, new2, hci⟩, hsc0, hlo, rfl, hfb, he, hBO, hk0⟩ := hm
    have hfind' : findLoop label (.stmts blk (i + 1) ss :: k) =
        some (l, blk', body, c0.scope.length, rest) := hfind
    simp only [stepStmt, stepStmt.stepContinue, hfind']
    have hs' : c.scope = (new ++ new2) ++ ci.scope := by
      rw [hsc, hci, List.append_assoc]
    rw [hs'] at hE
    have hE' := hE.restore
    have hlen : ci.scope.length = c0.scope.length := by rw [hsc0]
    rw [hlen] at hE'
    refine ⟨hp, ⟨hmem, hentry, ci, e :: ex0, hE', ?_⟩, hst⟩
    simp only [KontOk]
    refine ⟨true, fb, fun _ => rfl, hfb, hBO, fun c'' hc'' => ?_⟩
    exact ⟨ci, c0, fb, e, ex0, rfl, hc'', hsc0, hlo, rfl, hfb, he, hBO, hk0⟩
  | invariant n rel =>
    have hr := stmt_invariant hs
    obtain ⟨hc, rfl⟩ := Prod.mk.inj hr
    subst c'
    have hK := KontOk.next (blk := blk) (i := i) hfr' hb ht ⟨rfl, [], rfl⟩
    simp only [stepStmt]
    exact ⟨hp, ⟨hmem, hentry, { c with invariants := n :: c.invariants }, ex, hE, hK⟩, hst⟩

/-- One step from a well-typed state. -/
theorem step_ok {p : Program} {cfg : Config} (h : cfg.WellTyped p) : StepOk p (step p cfg) := by
  obtain ⟨frames⟩ := cfg
  obtain ⟨hp, h⟩ := h
  cases frames with
  | nil => exact (h : False).elim
  | cons f callers =>
    obtain ⟨fn, env, entry, kont, dest⟩ := f
    obtain ⟨⟨hmem, hentry, c, ex, hE, hK⟩, hst⟩ := h
    cases kont with
    | nil => simp [KontOk] at hK
    | cons k0 k =>
      cases k0 with
      | stmts blk i ss =>
        simp only [KontOk] at hK
        obtain ⟨a', fr, ha', hfr, hb, ht⟩ := hK
        have ha : a' = true := ha' trivial
        subst ha
        cases ss with
        | nil =>
          have hfr' := stmts_nil hfr
          subst hfr'
          exact ⟨hp, ⟨hmem, hentry, c, ex, hE, ht c (.refl c)⟩, hst⟩
        | cons s ss =>
          obtain ⟨-, c', fl, fr', hs, hfr', rfl⟩ := stmts_cons hfr
          exact stepStmt_ok hp hmem hentry hE hst hs hfr' hb ht
      | scope d =>
        simp only [KontOk] at hK
        obtain ⟨c0, ⟨-, new, hsc⟩, rfl, hK0⟩ := hK
        rw [hsc] at hE
        exact ⟨hp, ⟨hmem, hentry, c0, ex, hE.restore, hK0⟩, hst⟩
      | loop l blk body d =>
        simp only [KontOk] at hK
        obtain ⟨ci, c0, fb, e, ex0, rfl, ⟨-, new, hsc⟩, hsc0, hlo, rfl, hfb, he, hBO, hk0⟩ := hK
        have hlen : ci.scope.length = c0.scope.length := by rw [hsc0]
        rw [hsc] at hE
        have hE' := hE.restore
        rw [hlen] at hE'
        refine ⟨hp, ⟨hmem, hentry, ci, e :: ex0, hE', ?_⟩, hst⟩
        simp only [KontOk]
        refine ⟨true, fb, fun _ => rfl, hfb, hBO, fun c'' hc'' => ?_⟩
        exact ⟨ci, c0, fb, e, ex0, rfl, hc'', hsc0, hlo, rfl, hfb, he, hBO, hk0⟩

theorem runFrom_not_illFormed {p : Program} :
    ∀ (n : Nat) {c : Config}, c.WellTyped p → ∀ w, runFrom p n c ≠ .illFormed w
  | 0, c, _, w => by simp [runFrom]
  | n + 1, c, h, w => by
    have hs := step_ok h
    simp only [runFrom]
    split
    · rename_i c' he
      rw [he] at hs
      exact runFrom_not_illFormed n hs w
    · simp
    · simp
    · rename_i w' he
      rw [he] at hs
      exact (hs : False).elim

/-- `main`, entered with no argument, when it takes none. -/
theorem enter_main {p : Program} (hp : p.accepts = true) {f : FnDecl}
    (hf : p.fn? "main" = some f) (hparams : f.params = []) :
    StepOk p (enter p f [] .exit []) :=
  enter_ok (accepts_static hp) (List.mem_of_find?_eq_some hf) (by rw [hparams]; exact .nil) trivial

end Typing

open Typing

/-- The state a run of an accepted program starts from is well typed. -/
theorem Program.start_wellTyped {p : Program} (hp : p.accepts = true) (hentry : p.EntryHolds)
    {c : Config} (h : p.start = some c) : c.WellTyped p := by
  obtain ⟨f, hf, hparams, -⟩ := hentry
  have hE := enter_main hp hf hparams
  unfold Program.start at h
  rw [hf] at h
  simp only at h
  split at h
  · rename_i c' he
    cases h
    rw [he] at hE
    exact hE
  · cases h

/-- A step from a well-typed state never stops ill formed. -/
theorem step_not_illFormed {p : Program} {c : Config} (h : c.WellTyped p) (w : String) :
    step p c ≠ .halt (.illFormed w) := by
  intro he
  have hs := step_ok h
  rw [he] at hs
  exact hs

/-- A step from a well-typed state reaches a well-typed state. -/
theorem step_wellTyped {p : Program} {c c' : Config} (h : c.WellTyped p)
    (hs : step p c = .next c') : c'.WellTyped p := by
  have hk := step_ok h
  rw [hs] at hk
  exact hk

/-- An accepted program whose entry holds never stops ill formed. -/
theorem Program.never_illFormed (p : Program) (hp : p.accepts = true) (hentry : p.EntryHolds)
    (steps : Nat) (w : String) : p.run steps ≠ .illFormed w := by
  obtain ⟨f, hf, hparams, -⟩ := hentry
  have hE := enter_main hp hf hparams
  unfold Program.run
  rw [hf]
  simp only
  split
  · rename_i c he
    rw [he] at hE
    exact runFrom_not_illFormed steps hE w
  · simp
  · simp
  · rename_i w' he
    rw [he] at hE
    exact (hE : False).elim

end Whitefoot
