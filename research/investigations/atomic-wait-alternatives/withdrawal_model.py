"""Exhaustive finite-state model of withdrawing a timed atomic statement.

Question: when a statement written `atomic s = &obj when g until d { .. } else { .. }`
reaches its deadline before admission, can the runtime withdraw it so that
every execution has exactly one outcome, the withdrawn statement has no body
effect, nothing it took or registered stays behind, and no other waiter loses
the turn the runtime handed it?  See README.md, "Withdrawal model".

The model follows the acquisition protocol of today's runtime
(compiler/src/backend/completion/bridge.c, design node
compiler/waiting-contexts/state-locks): lock units taken in one order; the
first unit of a statement parks on the unit's queue, and an unlock either
wakes the queue's head, which retries, or hands the unit to it directly; a
later unit is taken by a claim, which an unlock grants before any parked
context; a false guard registers a watch while holding the guard's unit,
releases quietly and parks; a write to that unit notifies the oldest watch
whose test holds, and the notified context owns the notification until its
retry releases the unit (passing it on when the retry wrote nothing) and may
also hold a reservation that refuses newcomers holding nothing.

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
)

# Context kinds.  Each has the units it takes (in lock order), whether it has
# the guard `flag` (the state of unit 0), and which unit its body writes.
#   W  writer: takes unit 0, no guard, sets flag
#   T  timed statement: takes every unit, guard flag or none, writes last unit
#   N  untimed guarded statement: takes unit 0, guard flag, writes nothing on 0
#   M  untimed newcomer: takes every unit, no guard, writes last unit
#   H  holder of the last unit only (forces T to wait on a later unit)
KINDS = ("W", "T", "N", "M", "H")


def kind_units(kind, k):
    if kind in ("W", "N"):
        return (0,)
    if kind == "H":
        return (k - 1,)
    return tuple(range(k))


class Config:
    def __init__(self, k, kinds, t_guard):
        self.k = k
        self.kinds = kinds
        self.t_guard = t_guard
        self.units = tuple(kind_units(x, k) for x in kinds)

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
        return f"k={self.k} {'+'.join(self.kinds)} T:{g}"


# Context state fields (a tuple, so states hash):
PC, HELD, STATUS, EXPIRED, WAITED, WOKEN, OUTCOME = range(7)
# pc: 'acq' taking units, 'guard', 'rel' releasing after the body,
#     'relq' releasing quietly after a false guard, 'park' about to park on
#     the watch, 'done'.
# status: 'run', 'parkq' (parked on unit 0's queue), 'parkw' (parked on the
#     watch), 'claim' (waiting for a claim on a later unit).
# outcome: None, 'body' or 'else'.


def initial(cfg):
    ctxs = tuple(("acq", 0, "run", False, False, False, None) for _ in cfg.kinds)
    units = tuple((None, (), ()) for _ in range(cfg.k))  # holder, queue, claims
    # flag, effects of T, watches, notification owner, reservation owner,
    # violations
    return (ctxs, units, False, 0, (), None, None, frozenset())


def setf(t, i, v):
    return t[:i] + (v,) + t[i + 1:]


class St:
    """Mutable working copy of one state for building a successor."""

    def __init__(self, s):
        ctxs, units, self.flag, self.teff, watches, self.notif, self.resv, viol = s
        self.ctxs = [list(c) for c in ctxs]
        self.units = [[h, list(q), list(cl)] for h, q, cl in units]
        self.watches = list(watches)
        self.viol = set(viol)

    def freeze(self):
        return (
            tuple(tuple(c) for c in self.ctxs),
            tuple((h, tuple(q), tuple(cl)) for h, q, cl in self.units),
            self.flag, self.teff, tuple(self.watches), self.notif, self.resv,
            frozenset(self.viol),
        )


def refused(st, c, u):
    """A live reservation refuses a statement that holds nothing; a waiting
    claim refuses every statement that would take the unit by parking."""
    if st.ctxs[c][HELD] == 0 and u == 0 and st.resv is not None and st.resv != c:
        return True
    if st.units[u][2] and st.ctxs[c][HELD] == 0:
        return True
    return False


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


def unlock(st, cfg, c, u, handoff):
    """Release unit u held by c and give it on: claims first, then the parked
    queue (the reservation owner before the head), by wake or hand-off."""
    unit = st.units[u]
    assert unit[0] == c
    unit[0] = None
    if unit[2]:
        claimant = unit[2].pop(0)
        unit[0] = claimant
        st.ctxs[claimant][HELD] += 1
        st.ctxs[claimant][STATUS] = "run"
        return
    q = unit[1]
    if not q:
        return
    first = st.resv if st.resv in q else q[0]
    if handoff and refused_after(st, first, u):
        handoff = False
    q.remove(first)
    ctx = st.ctxs[first]
    ctx[STATUS] = "run"
    if handoff:
        unit[0] = first
        ctx[HELD] += 1
    else:
        ctx[WOKEN] = True


def refused_after(st, c, u):
    return st.ctxs[c][HELD] == 0 and u == 0 and st.resv is not None and st.resv != c


def withdraw(st, cfg, c, mutant, handoff):
    """The timed statement leaves before admission and takes the else branch."""
    ctx = st.ctxs[c]
    if mutant != "stale_links":
        for unit in st.units:
            if c in unit[1]:
                unit[1].remove(c)
            if c in unit[2]:
                unit[2].remove(c)
        if c in st.watches:
            st.watches.remove(c)
    if mutant != "keep_held":
        while ctx[HELD] > 0:
            u = cfg.units[c][ctx[HELD] - 1]
            ctx[HELD] -= 1
            if u == 0 and st.resv == c and mutant != "keep_reservation":
                st.resv = None
            unlock(st, cfg, c, u, handoff)
    if st.resv == c and mutant != "keep_reservation":
        st.resv = None
        q = st.units[0][1]
        if st.units[0][0] is None and q:
            head = q.pop(0)
            st.ctxs[head][STATUS] = "run"
            st.ctxs[head][WOKEN] = True
    if st.notif == c and mutant != "drop_notification":
        st.notif = None
        sweep(st, cfg, reserve=False)
    if ctx[WOKEN] and mutant != "drop_wake":
        q = st.units[0][1]
        if st.units[0][0] is None and q:
            head = q.pop(0)
            st.ctxs[head][STATUS] = "run"
            st.ctxs[head][WOKEN] = True
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
        # An expired timed statement withdraws at its next step.
        if timed and ctx[EXPIRED] and pc in ("acq", "guard", "park") and mutant != "late_admit":
            for handoff in (False, True):
                st = St(s)
                withdraw(st, cfg, c, mutant, handoff)
                out.append(st.freeze())
            continue
        if status == "claim":
            continue  # spinning until granted; the grant sets status run
        units = cfg.units[c]
        if pc == "acq":
            if held == len(units):
                st = St(s)
                st.ctxs[c][PC] = "guard"
                out.append(st.freeze())
                continue
            u = units[held]
            st = St(s)
            me = st.ctxs[c]
            if st.units[u][0] is None and not refused(st, c, u):
                st.units[u][0] = c
                me[HELD] += 1
                me[WOKEN] = False
            elif timed and me[EXPIRED]:
                withdraw(st, cfg, c, mutant, False)
            elif held == 0:
                # first unit: park; a woken context re-parks at the head
                if me[WOKEN]:
                    st.units[u][1].insert(0, c)
                else:
                    st.units[u][1].append(c)
                me[WOKEN] = False
                me[STATUS] = "parkq"
                me[WAITED] = True
            else:
                st.units[u][2].append(c)
                me[STATUS] = "claim"
                me[WAITED] = True
            out.append(st.freeze())
        elif pc == "guard":
            st = St(s)
            me = st.ctxs[c]
            if not cfg.guarded(c) or st.flag:
                if timed and me[EXPIRED]:
                    st.viol.add("admitted after expiry")
                for u in units:
                    if st.units[u][0] != c:
                        st.viol.add("admitted without holding every unit")
                if cfg.guarded(c) and not st.flag:
                    st.viol.add("admitted with false guard")
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
                    unlock(st, cfg, c, u, handoff)
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
        and all(h is None and not q and not cl for h, q, cl in units)
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
             "expired with notification": 0, "expired with reservation": 0,
             "expired after a wake": 0, "expired holding a unit after waiting": 0}
    t = cfg.kinds.index("T")
    for s, succ in edges.items():
        ctxs, units = s[0], s[1]
        me = ctxs[t]
        if me[EXPIRED] and me[OUTCOME] is None:
            if me[HELD] > 0:
                cover["expired holding a unit"] += 1
            if me[STATUS] == "claim":
                cover["expired while claiming"] += 1
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
