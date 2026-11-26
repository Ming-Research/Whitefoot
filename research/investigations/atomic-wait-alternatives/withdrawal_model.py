"""Exhaustive finite-state model of withdrawing a timed atomic statement.

Question: when a statement written `atomic s = &obj when g until d { .. } else { .. }`
reaches its deadline before admission, can the runtime withdraw it so that
every execution has exactly one outcome, the withdrawn statement has no body
effect, nothing it took or registered stays behind, and no other waiter loses
the turn the runtime handed it?  See README.md, "Withdrawal model".

The model follows the acquisition protocol of today's runtime
(compiler/src/backend/completion/bridge.c, design node
compiler/waiting-contexts/state-locks): lock units taken in one order.  The
first unit of a statement parks on the unit's queue.  A later unit is taken
without parking (wf__shared_take): free, or by borrowing a hold an unlock
handed to a parked context, or else by a queued claim that spins.  When the
last hold ends (wf_shared_free_locked), an unlock with the claims' turn set
grants the oldest claim; otherwise it wakes the first parked context the
reservation admits to retry, or hands it the unit, and then grants the oldest
claim if the unit is still free or gives the claims the next turn.  The
oldest claim may also borrow a pending hand-off, which ends the claims' turn;
the borrower's unlock gives the hold back, and a handed context that resumes
while its hold is borrowed claims it back and waits.  A false guard
registers a watch while holding the guard's unit, releases quietly and
parks; a write to that unit notifies the oldest watch whose test holds, and
the notified context owns the notification until its retry releases the
unit (passing it on when the retry wrote nothing) and may also hold a
reservation that refuses newcomers holding nothing.

Each runtime operation on one unit is one atomic step, as it runs under that
unit's lock.  The timer firing is one step that only marks the timed context
expired and makes it runnable; its links stay until the timed context's own
withdrawal step, so every grant, hand-off and notification that races with
the firing is explored.

The timer is armed only once the timed statement first waits, and the
expiry is consulted only at wait points and at admission: a statement that
never waits takes effect without reading the clock.

Run: python3 withdrawal_model.py
Exit status 0 when the protocol passes every configuration and every
deliberately broken protocol (MUTANTS) fails at least one of them.
"""

import sys
from collections import deque

# A deliberately broken withdrawal protocol, each of which the checks must
# reject; None is the proposed protocol.
MUTANTS = (
    "keep_held",         # withdrawal keeps the units it already took
    "stale_links",       # withdrawal leaves its queue, claim and watch links
    "drop_wake",         # a woken waiter that withdraws does not pass its wake on
    "drop_notification", # a notified waiter that withdraws keeps the notification
    "keep_reservation",  # a reserved waiter that withdraws keeps the reservation
    "late_admit",        # admission does not check the expiry
    "keep_grant",        # withdrawal keeps a hand-off it has not taken up
    "keep_turn",         # withdrawing the last claim keeps the claims' turn
)

# Context kinds.  Each has the units it takes (in lock order), whether it has
# the guard `flag` (the state of unit 0), and which unit its body writes.
#   W  writer: takes unit 0, no guard, sets flag
#   T  timed statement: takes every unit (or, in a last-unit configuration,
#      the last unit alone), guard flag or none, writes last unit
#   N  untimed guarded statement: takes unit 0, guard flag, writes nothing on 0
#   M  untimed newcomer: takes every unit, no guard, writes last unit
#   H  holder of the last unit only (forces T to wait on a later unit)
KINDS = ("W", "T", "N", "M", "H")


def kind_units(kind, k, t_last):
    if kind in ("W", "N"):
        return (0,)
    if kind == "H" or (kind == "T" and t_last):
        return (k - 1,)
    return tuple(range(k))


class Config:
    def __init__(self, k, kinds, t_guard, t_last=False):
        # The guard reads unit 0, so a timed statement on the last unit alone
        # has none.
        assert not (t_guard and t_last)
        self.k = k
        self.kinds = kinds
        self.t_guard = t_guard
        self.t_last = t_last
        self.units = tuple(kind_units(x, k, t_last) for x in kinds)

    def guarded(self, c):
        kind = self.kinds[c]
        return kind == "N" or (kind == "T" and self.t_guard)

    def writes(self, c):
        kind = self.kinds[c]
        if kind == "W":
            return 0
        if kind == "N":
            return None
        return self.units[c][-1]

    def __str__(self):
        g = "guard" if self.t_guard else "no-guard"
        last = ",last-unit" if self.t_last else ""
        return f"k={self.k} {'+'.join(self.kinds)} T:{g}{last}"


# Context state fields (a tuple, so states hash):
PC, HELD, STATUS, EXPIRED, WAITED, WOKEN, OUTCOME = range(7)
# pc: 'acq' taking units, 'guard', 'rel' releasing after the body,
#     'relq' releasing quietly after a false guard, 'park' about to park on
#     the watch, 'done'.
# status: 'run', 'parkq' (parked on a unit's queue), 'parkw' (parked on the
#     watch), 'claim' (a queued claim on a later unit, spinning).
# outcome: None, 'body' or 'else'.

# Unit fields: the hold's owner (for a hand-off, the parked context it was
# handed to), the parked queue, the claims oldest first, the claims' turn
# (bridge.c take_turn), the context a hand-off is pending for until it
# resumes (granted), the running statement borrowing that pending hold
# (borrowed), and whether the pending context has claimed it back (claimed).
HOLDER, QUEUE, CLAIMS, TURN, GRANTED, BORROWER, CLAIMED = range(7)


def initial(cfg):
    ctxs = tuple(("acq", 0, "run", False, False, False, None) for _ in cfg.kinds)
    units = tuple((None, (), (), False, None, None, False) for _ in range(cfg.k))
    # flag, effects of T, watches, notification owner, reservation owner,
    # violations
    return (ctxs, units, False, 0, (), None, None, frozenset())


class St:
    """Mutable working copy of one state for building a successor."""

    def __init__(self, s):
        ctxs, units, self.flag, self.teff, watches, self.notif, self.resv, viol = s
        self.ctxs = [list(c) for c in ctxs]
        self.units = [[u[0], list(u[1]), list(u[2])] + list(u[3:]) for u in units]
        self.watches = list(watches)
        self.viol = set(viol)

    def freeze(self):
        return (
            tuple(tuple(c) for c in self.ctxs),
            tuple((u[0], tuple(u[1]), tuple(u[2])) + tuple(u[3:]) for u in self.units),
            self.flag, self.teff, tuple(self.watches), self.notif, self.resv,
            frozenset(self.viol),
        )


def holds(unit, c):
    """Whether c holds the unit now: as its borrower, or as its owner once a
    hand-off to it has been taken up and no one borrows it."""
    if unit[BORROWER] is not None:
        return unit[BORROWER] == c
    return unit[HOLDER] == c and unit[GRANTED] != c


def lendable(unit):
    """bridge.c wf_shared_lendable: a hold handed to a parked context that no
    one borrows and that context has not claimed back."""
    return unit[GRANTED] is not None and unit[BORROWER] is None and not unit[CLAIMED]


def refused(st, c, u):
    """A parking statement that holds nothing is refused by a live
    reservation it does not own; while claims wait, no statement takes the
    unit but through them."""
    if st.ctxs[c][HELD] == 0 and u == 0 and st.resv is not None and st.resv != c:
        return True
    return bool(st.units[u][CLAIMS])


def sweep(st, cfg, reserve):
    """Notify the oldest watch whose test (flag) holds."""
    if st.notif is not None or not st.flag:
        return
    for w in list(st.watches):
        st.watches.remove(w)
        st.notif = w
        if reserve:
            st.resv = w
        if st.ctxs[w][STATUS] == "parkw":
            st.ctxs[w][STATUS] = "run"
        return


def grant(st, u, borrow):
    """bridge.c wf_shared_grant_locked: the oldest claim gets the free unit,
    or borrows the pending hand-off, and the claims' turn ends."""
    unit = st.units[u]
    claimant = unit[CLAIMS].pop(0)
    if borrow:
        unit[BORROWER] = claimant
    else:
        unit[HOLDER] = claimant
    unit[TURN] = False
    st.ctxs[claimant][HELD] += 1
    st.ctxs[claimant][STATUS] = "run"


def wake(st, u, handoff):
    """bridge.c wf_shared_wake_locked: wake the first parked context the
    reservation admits to retry, or hand it the unit.  Returns whether the
    unit was handed on."""
    unit = st.units[u]
    q = unit[QUEUE]
    if not q:
        return False
    first = q[0]
    if u == 0 and st.resv is not None:
        admitted = [x for x in q if x == st.resv or st.ctxs[x][HELD] > 0]
        if not admitted:
            return False
        first = admitted[0]
    q.remove(first)
    ctx = st.ctxs[first]
    ctx[STATUS] = "run"
    if handoff:
        unit[HOLDER] = first
        unit[GRANTED] = first
        return True
    ctx[WOKEN] = True
    return False


def free(st, u, handoff):
    """bridge.c wf_shared_free_locked, after the last hold ends: with the
    claims' turn set, grant the oldest claim; otherwise wake or hand on to
    the first parked context, then grant the oldest claim if the unit is
    still free, or give the claims the next turn."""
    unit = st.units[u]
    unit[HOLDER] = None
    if unit[CLAIMS] and unit[TURN]:
        grant(st, u, borrow=False)
        return
    wake(st, u, handoff)
    if unit[CLAIMS]:
        if unit[HOLDER] is None:
            grant(st, u, borrow=False)
        else:
            unit[TURN] = True


def release(st, c, u, handoff):
    """End c's hold on u: a borrowed hold goes back to the context it was
    handed to (bridge.c wf_shared_give_back_locked); any other ends."""
    unit = st.units[u]
    if unit[BORROWER] == c:
        unit[BORROWER] = None
    else:
        assert holds(unit, c)
        free(st, u, handoff)


def withdraw(st, cfg, c, mutant, handoff):
    """The timed statement leaves before admission and takes the else branch."""
    ctx = st.ctxs[c]
    if mutant != "stale_links":
        for unit in st.units:
            if c in unit[QUEUE]:
                unit[QUEUE].remove(c)
            if c in unit[CLAIMS]:
                unit[CLAIMS].remove(c)
                # A claim leaves today only through a grant, which ends the
                # claims' turn; the last claim withdrawn ends it the same way.
                if not unit[CLAIMS] and mutant != "keep_turn":
                    unit[TURN] = False
        if c in st.watches:
            st.watches.remove(c)
    if mutant != "keep_held":
        # A hand-off not yet taken up is released as an ordinary unlock; the
        # caller has waited until no one borrows it.
        for u, unit in enumerate(st.units):
            if unit[GRANTED] == c and mutant != "keep_grant":
                assert unit[BORROWER] is None
                unit[GRANTED] = None
                unit[CLAIMED] = False
                if u == 0 and st.resv == c and mutant != "keep_reservation":
                    st.resv = None
                free(st, u, handoff)
        while ctx[HELD] > 0:
            u = cfg.units[c][ctx[HELD] - 1]
            ctx[HELD] -= 1
            if u == 0 and st.resv == c and mutant != "keep_reservation":
                st.resv = None
            release(st, c, u, handoff)
    if st.resv == c and mutant != "keep_reservation":
        st.resv = None
        unit = st.units[0]
        if unit[HOLDER] is None and unit[QUEUE]:
            wake(st, 0, False)
    if st.notif == c and mutant != "drop_notification":
        st.notif = None
        sweep(st, cfg, reserve=False)
    if ctx[WOKEN] and mutant != "drop_wake":
        unit = st.units[cfg.units[c][0]]
        if unit[HOLDER] is None and unit[QUEUE]:
            wake(st, cfg.units[c][0], False)
    ctx[WOKEN] = False
    ctx[PC] = "done"
    ctx[OUTCOME] = "else"
    if not ctx[EXPIRED]:
        st.viol.add("else without expiry")


def successors(s, cfg, mutant):
    """Every state one step from s, with no duplicates."""
    out = []
    ctxs = s[0]
    for c, ctx in enumerate(ctxs):
        kind = cfg.kinds[c]
        timed = kind == "T"
        pc, held, status = ctx[PC], ctx[HELD], ctx[STATUS]
        if pc == "done":
            continue
        # The timer fires: only for a waiting timed statement not yet admitted.
        if timed and ctx[WAITED] and not ctx[EXPIRED] and pc in ("acq", "guard", "park"):
            st = St(s)
            st.ctxs[c][EXPIRED] = True
            if st.ctxs[c][STATUS] in ("parkq", "parkw"):
                st.ctxs[c][STATUS] = "run"
            out.append(st.freeze())
        if status in ("parkq", "parkw"):
            continue
        # A context handed a unit that a running statement borrows claims it
        # back and waits for the borrower's unlock to give it back.
        pending = [u for u, unit in enumerate(s[1]) if unit[GRANTED] == c]
        if pending and s[1][pending[0]][BORROWER] is not None:
            if not s[1][pending[0]][CLAIMED]:
                st = St(s)
                st.units[pending[0]][CLAIMED] = True
                out.append(st.freeze())
            continue
        # An expired timed statement withdraws at its next step.
        if timed and ctx[EXPIRED] and pc in ("acq", "guard", "park") and mutant != "late_admit":
            for handoff in (False, True):
                st = St(s)
                withdraw(st, cfg, c, mutant, handoff)
                out.append(st.freeze())
            continue
        units = cfg.units[c]
        if pending:
            # A hand-off is taken up when the context resumes.
            st = St(s)
            unit = st.units[pending[0]]
            unit[GRANTED] = None
            unit[CLAIMED] = False
            st.ctxs[c][HELD] += 1
            st.ctxs[c][WOKEN] = False
            out.append(st.freeze())
            continue
        if status == "claim":
            # A queued claim, if it is the oldest, may borrow a pending
            # hand-off; otherwise it spins until an unlock grants it.
            u = units[held]
            unit = s[1][u]
            if unit[CLAIMS] and unit[CLAIMS][0] == c and lendable(unit):
                st = St(s)
                grant(st, u, borrow=True)
                out.append(st.freeze())
            continue
        if pc == "acq":
            if held == len(units):
                st = St(s)
                st.ctxs[c][PC] = "guard"
                out.append(st.freeze())
                continue
            u = units[held]
            st = St(s)
            me = st.ctxs[c]
            unit = st.units[u]
            if held > 0:
                # bridge.c wf__shared_take: a statement that already holds an
                # earlier unit never parks.
                if not unit[CLAIMS] and lendable(unit):
                    unit[BORROWER] = c
                    me[HELD] += 1
                elif not unit[CLAIMS] and unit[HOLDER] is None:
                    unit[HOLDER] = c
                    me[HELD] += 1
                elif timed and me[EXPIRED]:
                    withdraw(st, cfg, c, mutant, False)
                else:
                    unit[CLAIMS].append(c)
                    me[STATUS] = "claim"
                    me[WAITED] = True
            elif unit[HOLDER] is None and not refused(st, c, u):
                unit[HOLDER] = c
                me[HELD] += 1
                me[WOKEN] = False
            elif timed and me[EXPIRED]:
                withdraw(st, cfg, c, mutant, False)
            else:
                # first unit: park; a woken context re-parks at the head
                if me[WOKEN]:
                    unit[QUEUE].insert(0, c)
                else:
                    unit[QUEUE].append(c)
                me[WOKEN] = False
                me[STATUS] = "parkq"
                me[WAITED] = True
            out.append(st.freeze())
        elif pc == "guard":
            st = St(s)
            me = st.ctxs[c]
            if not cfg.guarded(c) or st.flag:
                if timed and me[EXPIRED]:
                    st.viol.add("admitted after expiry")
                for u in units:
                    if not holds(st.units[u], c):
                        st.viol.add("admitted without holding every unit")
                if kind == "W":
                    st.flag = True
                if timed:
                    st.teff += 1
                    me[OUTCOME] = "body"
                me[PC] = "rel"
            elif timed and me[EXPIRED]:
                withdraw(st, cfg, c, mutant, False)
            else:
                if c not in st.watches:
                    st.watches.append(c)
                me[PC] = "relq"
            out.append(st.freeze())
        elif pc in ("rel", "relq"):
            u = units[held - 1]
            wrote = pc == "rel" and cfg.writes(c) == u
            for handoff in (False, True):
                for reserve in ((False, True) if wrote and u == 0 else (False,)):
                    st = St(s)
                    me = st.ctxs[c]
                    me[HELD] -= 1
                    if u == 0 and st.resv == c:
                        st.resv = None
                    if u == 0 and st.notif == c:
                        st.notif = None
                        if not wrote:
                            sweep(st, cfg, reserve=False)
                    release(st, c, u, handoff)
                    if wrote and u == 0:
                        sweep(st, cfg, reserve)
                    if me[HELD] == 0:
                        me[PC] = "done" if pc == "rel" else "park"
                    out.append(st.freeze())
        elif pc == "park":
            st = St(s)
            me = st.ctxs[c]
            if c in st.watches:
                me[STATUS] = "parkw"
                me[WAITED] = True
                me[PC] = "acq"
            else:
                me[PC] = "acq"  # notified before parking: retry at once
            out.append(st.freeze())
    return list(dict.fromkeys(out))


def is_final(s, cfg):
    ctxs, units, flag, teff, watches, notif, resv, viol = s
    return (
        all(c[PC] == "done" for c in ctxs)
        and all(u[HOLDER] is None and not u[QUEUE] and not u[CLAIMS] and u[GRANTED] is None
                and u[BORROWER] is None and not u[CLAIMED] and not u[TURN] for u in units)
        and not watches and notif is None and resv is None
    )


def explore(cfg, mutant):
    """Breadth-first search of every reachable state.  Returns the failures
    and coverage counters."""
    s0 = initial(cfg)
    seen = {s0: None}
    edges = {}
    order = deque([s0])
    while order:
        s = order.popleft()
        succ = successors(s, cfg, mutant)
        edges[s] = succ
        for t in succ:
            if t not in seen:
                seen[t] = s
                order.append(t)
    failures = []
    cover = {"states": len(seen), "body": 0, "else": 0,
             "expired holding a unit": 0, "expired while claiming": 0,
             "expired while the claims hold the turn": 0,
             "expired with a hand-off not yet taken up": 0,
             "expired while its hand-off is borrowed": 0,
             "expired holding a borrowed unit": 0,
             "expired with notification": 0, "expired with reservation": 0,
             "expired after a wake": 0,
             "expired holding a unit after waiting": 0}
    t = cfg.kinds.index("T")
    for s, succ in edges.items():
        ctxs, units = s[0], s[1]
        me = ctxs[t]
        if me[EXPIRED] and me[OUTCOME] is None:
            if me[HELD] > 0:
                cover["expired holding a unit"] += 1
            if me[STATUS] == "claim":
                cover["expired while claiming"] += 1
                if any(t in u[CLAIMS] and u[TURN] for u in units):
                    cover["expired while the claims hold the turn"] += 1
            if any(u[GRANTED] == t for u in units):
                cover["expired with a hand-off not yet taken up"] += 1
            if any(u[GRANTED] == t and u[BORROWER] is not None for u in units):
                cover["expired while its hand-off is borrowed"] += 1
            if any(u[BORROWER] == t for u in units):
                cover["expired holding a borrowed unit"] += 1
            if s[5] == t:
                cover["expired with notification"] += 1
            if s[6] == t:
                cover["expired with reservation"] += 1
            if me[WOKEN]:
                cover["expired after a wake"] += 1
            if me[HELD] > 0 and me[PC] == "acq" and me[STATUS] == "run" and me[WAITED]:
                cover["expired holding a unit after waiting"] += 1
        for v in s[7]:
            failures.append((v, s))
        if not succ:
            if not is_final(s, cfg):
                failures.append(("stuck: no step and not every context done", s))
            else:
                cover[me[OUTCOME]] += 1
                if me[OUTCOME] == "else" and s[3] != 0:
                    failures.append(("else with a body effect", s))
                if me[OUTCOME] == "body" and s[3] != 1:
                    failures.append(("body effect not exactly once", s))
    # Every reachable state must reach a final state: a lost wake or turn
    # shows as a state from which no completion is reachable, even when some
    # other step is still possible.
    good = {s for s, succ in edges.items() if not succ and is_final(s, cfg)}
    rev = {}
    for s, succ in edges.items():
        for x in succ:
            rev.setdefault(x, []).append(s)
    reach = set(good)
    work = deque(good)
    while work:
        x = work.popleft()
        for p in rev.get(x, ()):
            if p not in reach:
                reach.add(p)
                work.append(p)
    trapped = [s for s in edges if s not in reach]
    if trapped:
        failures.append(("cannot complete from %d states" % len(trapped), trapped[0]))
    return failures, cover, seen


def trace(seen, s):
    path = []
    while s is not None:
        path.append(s)
        s = seen[s]
    return list(reversed(path))


def configs():
    out = []
    for k in (1, 2):
        extra = ("H",) if k == 2 else ()
        for t_guard in (True, False):
            sets = [("W", "T"), ("W", "T", "N"), ("W", "T", "M"), ("W", "T", "N", "M"),
                    ("T", "M")]
            if t_guard:
                sets.append(("T",))
            for kinds in sets:
                out.append(Config(k, kinds, t_guard))
                if extra:
                    out.append(Config(k, kinds + extra, t_guard))
            if extra:
                # two holders of the last unit: one parks behind the other
                # while T claims it, so hand-off, borrowing and the claims'
                # turn are reached
                out.append(Config(k, ("T", "H", "H"), t_guard))
                out.append(Config(k, ("W", "T", "H", "H"), t_guard))
    # the timed statement on the second unit alone parks there, so the
    # newcomer, which takes it as a later unit, borrows the hand-off made to
    # the timed statement while that statement waits
    out.append(Config(2, ("T", "M", "H"), False, t_last=True))
    return out


def main():
    ok = True
    total = {}
    print("proposed protocol")
    for cfg in configs():
        failures, cover, seen = explore(cfg, None)
        for key, v in cover.items():
            total[key] = total.get(key, 0) + v
        status = "ok" if not failures else "FAIL"
        print(f"  {status:4} {cfg}: {cover['states']} states, "
              f"{cover['body']} final with body, {cover['else']} final with else")
        if failures:
            ok = False
            v, s = failures[0]
            print(f"    {v}")
            for step in trace(seen, s):
                print(f"      {step}")
    print("  coverage over all configurations:")
    for key, v in total.items():
        print(f"    {key}: {v}")
        if v == 0:
            print("    FAIL: never reached, so the checks above are vacuous here")
            ok = False
    print("broken protocols (each must fail)")
    for mutant in MUTANTS:
        caught = None
        for cfg in configs():
            failures, cover, seen = explore(cfg, mutant)
            if failures:
                caught = (cfg, failures[0][0])
                break
        if caught:
            print(f"  caught {mutant}: {caught[1]} ({caught[0]})")
        else:
            print(f"  MISSED {mutant}")
            ok = False
    print("PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
