/* E1 of research/investigations/match-dispatch: vm.c's register machine with
 * Silverfir-nano's two register-residency mechanisms, to test whether
 * LLVM-compiled handlers reach Silverfir-nano's dispatch cost once values
 * stop passing through frame memory.
 *
 * - Accumulator: every value producer also leaves its result in `acc`
 *   (`facc` for floats); an operand read by the instruction right after its
 *   producer, in the same control region and value domain, reads it there.
 * - Pinned locals: per function, the two most-referenced frame slots whose
 *   reads and writes share one domain live in registers (`l0`/`l1`, or
 *   `fl0`/`fl1` for floats). Writes go to the register and the slot
 *   (write-through), so the slot stays authoritative; a call loads the
 *   callee's pinned registers from its frame and a return reloads the
 *   caller's.
 *
 * Each handler is specialised by the residency class of its operands (a and
 * b: slot, acc, pin0, pin1; destination: slot, pin0, pin1), 48 variants per
 * opcode, as Silverfir-nano's generated handlers are. A link pass assigns
 * the classes; `vm1 KERNEL MODE` with MODE none, acc, accpin or full (accpin
 * plus no slot store where liveness shows the slot dead, as Silverfir-nano's
 * accumulator leaves a stack temporary unstored) selects which mechanisms it
 * uses, in one binary.
 *
 *   -DDISPATCH=1 switch, 2 computed goto, 3 musttail + table,
 *              4 musttail + handler offset in the cell
 *   -DACCESS=2 u8 operands + fetch comparison, 4 u8 without it, 3 raw
 *   -DCOUNT    count dispatches
 * Handlers of DISPATCH 3 and 4 use preserve_none.
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef DISPATCH
#define DISPATCH 3
#endif
#ifndef ACCESS
#define ACCESS 2
#endif

typedef struct {
    uint32_t op;           /* variant index, or handler offset under DISPATCH 4 */
    uint16_t a, b, c;
    uint16_t pad;          /* CALL: the caller's pinned slots */
    int32_t imm;
} Cell;

typedef struct {
    uintptr_t pc;
    uintptr_t base;
    uint64_t pins;
} Ret;

#define OPS(X) \
    X(MOVI) X(MOV) X(ADD) X(SUB) X(MUL) X(AND) X(XOR) X(SHL) X(SHRU) X(ADDI) \
    X(LT_BR) X(LTI_BR) X(NE_BR) X(EQZ_BR) X(JMP) \
    X(LOAD8) X(STORE8) X(LOAD32) X(STORE32) \
    X(FADD) X(FSUB) X(FMUL) X(FGT_BR) X(I2F) X(FMOV) \
    X(CALL) X(RET)

enum {
#define ENUM_OP(n) OP_##n,
    OPS(ENUM_OP)
#undef ENUM_OP
    N_OPS
};

/* Variant index: op * 64 + a * 16 + b * 4 + d, classes in the order below;
 * destination N writes only the accumulator (the slot is dead afterwards). */
#define N_VAR 64
enum { CL_S, CL_A, CL_P0, CL_P1 };
enum { DL_S, DL_P0, DL_P1, DL_N };

#define FRAME_HEADROOM 256
#define RS_CAP 100000
#define REGS_LEN (RS_CAP * 16 + FRAME_HEADROOM * 2)
#define NO_PIN 0x7f

#ifdef COUNT
static uint64_t dispatches;
#define COUNTED() (dispatches++)
#else
#define COUNTED() ((void)0)
#endif

#define trap(why) __builtin_trap()
#define UNLIKELY(x) __builtin_expect(!!(x), 0)

static inline double as_f(uint64_t v) { double d; memcpy(&d, &v, 8); return d; }
static inline uint64_t as_u(double d) { uint64_t v; memcpy(&v, &d, 8); return v; }

/* ---- operand access ------------------------------------------------------ */

#if ACCESS == 2
#define SLOT(x) (regs[base + (x)])
#define FETCHCHK() do { if (UNLIKELY(pc >= ncode)) trap("fetch index"); } while (0)
#define OPND(f) ((uint8_t)ip->f)
#elif ACCESS == 4
#define SLOT(x) (regs[base + (x)])
#define FETCHCHK() ((void)ncode)
#define OPND(f) ((uint8_t)ip->f)
#else
#define SLOT(x) (fp[(x)])
#define FETCHCHK() ((void)0)
#define OPND(f) (ip->f)
#endif

/* Readers by class, integer and float domain. */
#define RI_S(f) SLOT(OPND(f))
#define RI_A(f) acc
#define RI_P0(f) l0
#define RI_P1(f) l1
#define RF_S(f) as_f(SLOT(OPND(f)))
#define RF_A(f) facc
#define RF_P0(f) fl0
#define RF_P1(f) fl1
/* Writers by destination class; every producer also sets the accumulator. */
#define WI_S(v) (acc = (v), SLOT(OPND(c)) = acc)
#define WI_P0(v) (acc = l0 = (v), SLOT(OPND(c)) = l0)
#define WI_P1(v) (acc = l1 = (v), SLOT(OPND(c)) = l1)
#define WF_S(v) (facc = (v), SLOT(OPND(c)) = as_u(facc))
#define WF_P0(v) (facc = fl0 = (v), SLOT(OPND(c)) = as_u(fl0))
#define WF_P1(v) (facc = fl1 = (v), SLOT(OPND(c)) = as_u(fl1))
#define WI_N(v) (acc = (v))
#define WF_N(v) (facc = (v))

#define SB ((int64_t)(int16_t)ip->b)
#define IMM ((int64_t)ip->imm)

/* Load the pinned registers named by a 16-bit pin word: two bytes, each a
 * slot in its low seven bits (NO_PIN for none) and the float flag in bit 7. */
#define LOAD_PINS(w) do { \
        unsigned p0_ = (unsigned)(w) & 0xff, p1_ = ((unsigned)(w) >> 8) & 0xff; \
        if ((p0_ & 0x7f) != NO_PIN) { \
            if (p0_ & 0x80) fl0 = as_f(SLOT(p0_ & 0x7f)); else l0 = SLOT(p0_ & 0x7f); } \
        if ((p1_ & 0x7f) != NO_PIN) { \
            if (p1_ & 0x80) fl1 = as_f(SLOT(p1_ & 0x7f)); else l1 = SLOT(p1_ & 0x7f); } \
    } while (0)

#if ACCESS == 3
#define CUR_IP ip
#define BIND_IP
#define ADVANCE() (ip += 1)
#define SETPC(t) (ip = code + (t))
#define CALL_ENTER(t, off, pw) do { \
        if (UNLIKELY(rsp >= RS_CAP)) trap("call depth"); \
        rs[rsp].pc = (uintptr_t)(ip + 1); rs[rsp].base = (uintptr_t)fp; rs[rsp].pins = (pw); rsp++; \
        fp += (off); \
        if (UNLIKELY(fp + FRAME_HEADROOM > regs_end)) trap("frame space"); \
        SETPC(t); } while (0)
#define RET_POP(pw) do { rsp--; ip = (const Cell *)rs[rsp].pc; fp = (uint64_t *)rs[rsp].base; \
        (pw) = rs[rsp].pins; } while (0)
#else
#define CUR_IP (&code[pc])
#define BIND_IP const Cell *ip = &code[pc]; (void)ip;
#define ADVANCE() (pc += 1)
#define SETPC(t) (pc = (uint32_t)(t))
#define CALL_ENTER(t, off, pw) do { \
        if (UNLIKELY(rsp >= RS_CAP)) trap("call depth"); \
        rs[rsp].pc = pc + 1; rs[rsp].base = base; rs[rsp].pins = (pw); rsp++; \
        base += (off); \
        if (UNLIKELY(base + FRAME_HEADROOM > len)) trap("frame space"); \
        SETPC(t); } while (0)
#define RET_POP(pw) do { rsp--; pc = (uint32_t)rs[rsp].pc; base = rs[rsp].base; \
        (pw) = rs[rsp].pins; } while (0)
#endif

#define NEXT() do { ADVANCE(); FETCHCHK(); DISPATCH_NEXT(); } while (0)
#define JUMP(t) do { SETPC(t); FETCHCHK(); DISPATCH_NEXT(); } while (0)

/* ---- handler semantics: integer readers A B, float readers FA FB, integer
 * writer W, float writer FW ------------------------------------------------- */

#define BODY_MOVI(A, B, FA, FB, W, FW)   { W((uint64_t)IMM); NEXT(); }
#define BODY_MOV(A, B, FA, FB, W, FW)    { W(A); NEXT(); }
#define BODY_ADD(A, B, FA, FB, W, FW)    { W(A + B); NEXT(); }
#define BODY_SUB(A, B, FA, FB, W, FW)    { W(A - B); NEXT(); }
#define BODY_MUL(A, B, FA, FB, W, FW)    { W(A * B); NEXT(); }
#define BODY_AND(A, B, FA, FB, W, FW)    { W(A & B); NEXT(); }
#define BODY_XOR(A, B, FA, FB, W, FW)    { W(A ^ B); NEXT(); }
#define BODY_SHL(A, B, FA, FB, W, FW)    { W(A << (B & 63)); NEXT(); }
#define BODY_SHRU(A, B, FA, FB, W, FW)   { W(A >> (B & 63)); NEXT(); }
#define BODY_ADDI(A, B, FA, FB, W, FW)   { W(A + (uint64_t)IMM); NEXT(); }
#define BODY_LT_BR(A, B, FA, FB, W, FW)  { if ((int64_t)A < (int64_t)B) JUMP(IMM); NEXT(); }
#define BODY_LTI_BR(A, B, FA, FB, W, FW) { if ((int64_t)A < SB) JUMP(IMM); NEXT(); }
#define BODY_NE_BR(A, B, FA, FB, W, FW)  { if (A != B) JUMP(IMM); NEXT(); }
#define BODY_EQZ_BR(A, B, FA, FB, W, FW) { if (A == 0) JUMP(IMM); NEXT(); }
#define BODY_JMP(A, B, FA, FB, W, FW)    { JUMP(IMM); }
#define BODY_LOAD8(A, B, FA, FB, W, FW)  { uint64_t ad_ = A + (uint32_t)IMM; \
        if (UNLIKELY(ad_ >= memlen)) trap("memory"); W((uint64_t)mem[ad_]); NEXT(); }
#define BODY_STORE8(A, B, FA, FB, W, FW) { uint64_t ad_ = A + (uint32_t)IMM; \
        if (UNLIKELY(ad_ >= memlen)) trap("memory"); mem[ad_] = (uint8_t)B; NEXT(); }
#define BODY_LOAD32(A, B, FA, FB, W, FW) { uint64_t ad_ = A + (uint32_t)IMM; uint32_t v_; \
        if (UNLIKELY(ad_ + 4 > memlen)) trap("memory"); memcpy(&v_, mem + ad_, 4); W((uint64_t)v_); NEXT(); }
#define BODY_STORE32(A, B, FA, FB, W, FW) { uint64_t ad_ = A + (uint32_t)IMM; uint32_t v_ = (uint32_t)B; \
        if (UNLIKELY(ad_ + 4 > memlen)) trap("memory"); memcpy(mem + ad_, &v_, 4); NEXT(); }
#define BODY_FADD(A, B, FA, FB, W, FW)   { FW(FA + FB); NEXT(); }
#define BODY_FSUB(A, B, FA, FB, W, FW)   { FW(FA - FB); NEXT(); }
#define BODY_FMUL(A, B, FA, FB, W, FW)   { FW(FA * FB); NEXT(); }
#define BODY_FGT_BR(A, B, FA, FB, W, FW) { if (FA > FB) JUMP(IMM); NEXT(); }
#define BODY_I2F(A, B, FA, FB, W, FW)    { FW((double)(int64_t)A); NEXT(); }
#define BODY_FMOV(A, B, FA, FB, W, FW)   { FW(FA); NEXT(); }
/* The callee frame starts at slot b; its slot 0 returns the result into the
 * caller's slot b, which the return also leaves in the accumulator. */
#define BODY_CALL(A, B, FA, FB, W, FW)   { uint64_t off_ = ip->b; unsigned callee_ = ip->c; \
        CALL_ENTER(IMM, off_, ip->pad); LOAD_PINS(callee_); FETCHCHK(); DISPATCH_NEXT(); }
#define BODY_RET(A, B, FA, FB, W, FW)    { uint64_t v_ = A; uint64_t pins_; SLOT(0) = v_; acc = v_; \
        if (rsp == 0) return v_; \
        RET_POP(pins_); LOAD_PINS(pins_); DISPATCH_NEXT(); }

/* ---- variant enumeration -------------------------------------------------- */

#define FOR_D(M, op, a, b) M(op, a, b, S) M(op, a, b, P0) M(op, a, b, P1) M(op, a, b, N)
#define FOR_B(M, op, a) FOR_D(M, op, a, S) FOR_D(M, op, a, A) FOR_D(M, op, a, P0) FOR_D(M, op, a, P1)
#define FOR_A(M, op) FOR_B(M, op, S) FOR_B(M, op, A) FOR_B(M, op, P0) FOR_B(M, op, P1)

#define HANDLER_BODY(op, ca, cb, cd) \
    BODY_##op(RI_##ca(a), RI_##cb(b), RF_##ca(a), RF_##cb(b), WI_##cd, WF_##cd)

typedef struct {
    Ret *rs;
    uint64_t *regs_end;
} Spill;

#if DISPATCH == 1 || DISPATCH == 2

static uint64_t run(const Cell *code, uint32_t ncode, uint64_t *regs, uint64_t len,
                    uint8_t *mem, uint64_t memlen, Ret *rs, unsigned root_pins) {
    (void)ncode; (void)len;
    uint64_t rsp = 0, acc = 0, l0 = 0, l1 = 0;
    double facc = 0, fl0 = 0, fl1 = 0;
#if ACCESS == 3
    const Cell *ip = code;
    uint64_t *fp = regs;
    uint64_t *regs_end = regs + len;
#else
    uint32_t pc = 0;
    uint64_t base = 0;
#endif
    LOAD_PINS(root_pins);
#if DISPATCH == 1
#define DISPATCH_NEXT() goto top
    for (;;) {
    top:;
        COUNTED();
        switch (CUR_IP->op) {
#define VARIANT_M(op, a, b, d) case (OP_##op * N_VAR + CL_##a * 16 + CL_##b * 4 + DL_##d): \
            { BIND_IP HANDLER_BODY(op, a, b, d) }
#define CASES(op) FOR_A(VARIANT_M, op)
            OPS(CASES)
#undef CASES
#undef VARIANT_M
        default:
            __builtin_unreachable();
        }
    }
#else
    static void *const labels[N_OPS * N_VAR] = {
#define VARIANT_M(op, a, b, d) [OP_##op * N_VAR + CL_##a * 16 + CL_##b * 4 + DL_##d] = &&L_##op##_##a##_##b##_##d,
#define LABELS(op) FOR_A(VARIANT_M, op)
        OPS(LABELS)
#undef LABELS
#undef VARIANT_M
    };
#define DISPATCH_NEXT() do { COUNTED(); goto *labels[CUR_IP->op]; } while (0)
    DISPATCH_NEXT();
#define VARIANT_M(op, a, b, d) L_##op##_##a##_##b##_##d: { BIND_IP HANDLER_BODY(op, a, b, d) }
#define BLOCKS(op) FOR_A(VARIANT_M, op)
    OPS(BLOCKS)
#undef BLOCKS
#undef VARIANT_M
#endif
}

#else /* DISPATCH 3 or 4 */

#define CC __attribute__((preserve_none))

/* -DHANDLER_BASE_PARAM passes the handler table's address (DISPATCH 3) or
 * the base handler's address (DISPATCH 4) along the chain as a parameter,
 * instead of letting each handler rematerialise it. */
#ifdef HANDLER_BASE_PARAM
#define BASE_PARAM , const void *hbase
#define BASE_ARG , hbase
#else
#define BASE_PARAM
#define BASE_ARG
#endif

#if ACCESS == 3
#define PARAMS const Cell *code, const Cell *ip, uint64_t *fp, uint8_t *mem, uint64_t memlen, \
               uint64_t rsp, Spill *st, uint64_t acc, uint64_t l0, uint64_t l1, \
               double facc, double fl0, double fl1 BASE_PARAM
#define ARGS code, ip, fp, mem, memlen, rsp, st, acc, l0, l1, facc, fl0, fl1 BASE_ARG
#define SPILL_LOCALS Ret *rs = st->rs; uint64_t *regs_end = st->regs_end; (void)rs; (void)regs_end;
#else
#define PARAMS const Cell *code, uint32_t pc, uint64_t *regs, uint64_t base, uint8_t *mem, \
               uint64_t memlen, uint32_t ncode, uint64_t rsp, Spill *st, uint64_t acc, \
               uint64_t l0, uint64_t l1, double facc, double fl0, double fl1 BASE_PARAM
#define ARGS code, pc, regs, base, mem, memlen, ncode, rsp, st, acc, l0, l1, facc, fl0, fl1 BASE_ARG
#define SPILL_LOCALS Ret *rs = st->rs; uint64_t len = (uint64_t)(st->regs_end - regs); (void)rs; (void)len;
#endif

typedef uint64_t (CC *Handler)(PARAMS);

#define VARIANT_M(op, a, b, d) CC static uint64_t h_##op##_##a##_##b##_##d(PARAMS);
#define DECLS(op) FOR_A(VARIANT_M, op)
OPS(DECLS)
#undef DECLS
#undef VARIANT_M

static Handler const handlers[N_OPS * N_VAR] = {
#define VARIANT_M(op, a, b, d) [OP_##op * N_VAR + CL_##a * 16 + CL_##b * 4 + DL_##d] = h_##op##_##a##_##b##_##d,
#define TABLE(op) FOR_A(VARIANT_M, op)
    OPS(TABLE)
#undef TABLE
#undef VARIANT_M
};

#if DISPATCH == 3 && defined(HANDLER_BASE_PARAM)
#define HANDLER_OF(op) (((Handler const *)hbase)[(op)])
#define HBASE_INIT (const void *)handlers
#elif DISPATCH == 3
#define HANDLER_OF(op) (handlers[(op)])
#elif defined(HANDLER_BASE_PARAM)
CC __attribute__((noinline)) static uint64_t h_base(PARAMS) { trap("base handler"); }
#define HANDLER_OF(op) ((Handler)((const char *)hbase + (int32_t)(op)))
#define HBASE_INIT (const void *)h_base
#else
CC __attribute__((noinline)) static uint64_t h_base(PARAMS) { trap("base handler"); }
#define HANDLER_OF(op) ((Handler)((const char *)h_base + (int32_t)(op)))
#endif

#define DISPATCH_NEXT() do { COUNTED(); __attribute__((musttail)) return HANDLER_OF(CUR_IP->op)(ARGS); } while (0)

#define VARIANT_M(op, a, b, d) CC static uint64_t h_##op##_##a##_##b##_##d(PARAMS) { \
        SPILL_LOCALS BIND_IP HANDLER_BODY(op, a, b, d) }
#define DEFS(op) FOR_A(VARIANT_M, op)
OPS(DEFS)
#undef DEFS
#undef VARIANT_M

static uint64_t run(const Cell *code, uint32_t ncode, uint64_t *regs, uint64_t len,
                    uint8_t *mem, uint64_t memlen, Ret *rs, unsigned root_pins) {
    Spill st = { rs, regs + len };
    uint64_t acc = 0, l0 = 0, l1 = 0, rsp = 0;
    double facc = 0, fl0 = 0, fl1 = 0;
    (void)len;
#if ACCESS == 3
    uint64_t *fp = regs;
    const Cell *ip = code;
#else
    uint64_t base = 0;
    uint32_t pc = 0;
#endif
    LOAD_PINS(root_pins);
    COUNTED();
#if ACCESS == 3
#ifdef HANDLER_BASE_PARAM
    const void *hbase = HBASE_INIT;
#endif
    return HANDLER_OF(code[0].op)(code, ip, fp, mem, memlen, rsp, &st, acc, l0, l1, facc, fl0, fl1 BASE_ARG);
#else
#ifdef HANDLER_BASE_PARAM
    const void *hbase = HBASE_INIT;
#endif
    return HANDLER_OF(code[0].op)(code, pc, regs, base, mem, memlen, ncode, rsp, &st,
                                  acc, l0, l1, facc, fl0, fl1 BASE_ARG);
#endif
}

#endif

/* ---- link pass: residency classes ----------------------------------------- */

enum { D_NONE, D_I, D_F };
typedef struct { uint8_t reads_a, reads_b, writes, dom_a, dom_b, dom_w; } OpInfo;
static const OpInfo info[N_OPS] = {
    [OP_MOVI] = { 0, 0, 1, 0, 0, D_I },     [OP_MOV] = { 1, 0, 1, D_I, 0, D_I },
    [OP_ADD] = { 1, 1, 1, D_I, D_I, D_I },  [OP_SUB] = { 1, 1, 1, D_I, D_I, D_I },
    [OP_MUL] = { 1, 1, 1, D_I, D_I, D_I },  [OP_AND] = { 1, 1, 1, D_I, D_I, D_I },
    [OP_XOR] = { 1, 1, 1, D_I, D_I, D_I },  [OP_SHL] = { 1, 1, 1, D_I, D_I, D_I },
    [OP_SHRU] = { 1, 1, 1, D_I, D_I, D_I }, [OP_ADDI] = { 1, 0, 1, D_I, 0, D_I },
    [OP_LT_BR] = { 1, 1, 0, D_I, D_I, 0 },  [OP_LTI_BR] = { 1, 0, 0, D_I, 0, 0 },
    [OP_NE_BR] = { 1, 1, 0, D_I, D_I, 0 },  [OP_EQZ_BR] = { 1, 0, 0, D_I, 0, 0 },
    [OP_JMP] = { 0, 0, 0, 0, 0, 0 },
    [OP_LOAD8] = { 1, 0, 1, D_I, 0, D_I },  [OP_STORE8] = { 1, 1, 0, D_I, D_I, 0 },
    [OP_LOAD32] = { 1, 0, 1, D_I, 0, D_I }, [OP_STORE32] = { 1, 1, 0, D_I, D_I, 0 },
    [OP_FADD] = { 1, 1, 1, D_F, D_F, D_F }, [OP_FSUB] = { 1, 1, 1, D_F, D_F, D_F },
    [OP_FMUL] = { 1, 1, 1, D_F, D_F, D_F }, [OP_FGT_BR] = { 1, 1, 0, D_F, D_F, 0 },
    [OP_I2F] = { 1, 0, 1, D_I, 0, D_F },    [OP_FMOV] = { 1, 0, 1, D_F, 0, D_F },
    [OP_CALL] = { 0, 0, 0, 0, 0, 0 },       [OP_RET] = { 1, 0, 0, D_I, 0, 0 },
};

static int is_branch(int op) {
    return op == OP_LT_BR || op == OP_LTI_BR || op == OP_NE_BR || op == OP_EQZ_BR || op == OP_JMP;
}

/* The slot an instruction writes and its domain, if it produces a value:
 * a CALL produces the caller's slot b, through the return. */
static int produced(const Cell *c, int *slot) {
    if (c->op == OP_CALL) { *slot = c->b; return D_I; }
    if (info[c->op].writes) { *slot = c->c; return info[c->op].dom_w; }
    return D_NONE;
}

typedef struct { int slot[2]; int dom[2]; } Pins;

static unsigned pin_word(const Pins *p) {
    unsigned w = 0;
    for (int k = 0; k < 2; k++) {
        unsigned byte = p->slot[k] < 0 ? NO_PIN : (unsigned)p->slot[k] | (p->dom[k] == D_F ? 0x80 : 0);
        w |= byte << (8 * k);
    }
    return w;
}

/* Backward liveness of frame slots. live_out[i] is the set of slots some
 * path from after instruction i reads before writing; slot_read[i] says
 * which of its operands (bit 0 a, bit 1 b) read the slot rather than the
 * accumulator or a pinned register. A CALL reads the
 * argument slots b..b+15 of its callee's frame and writes slot b; a RET
 * ends its function, after which nothing of the frame is read. */
typedef struct { uint64_t w[4]; } Slots;
static void slots_set(Slots *s, unsigned k) { s->w[k >> 6] |= 1ull << (k & 63); }
static void slots_clear(Slots *s, unsigned k) { s->w[k >> 6] &= ~(1ull << (k & 63)); }
static int slots_has(const Slots *s, unsigned k) { return (s->w[k >> 6] >> (k & 63)) & 1; }

static Slots *liveness(const Cell *code, uint32_t n, const uint8_t *slot_read) {
    Slots *in = calloc(n + 1, sizeof(Slots)), *out = calloc(n, sizeof(Slots));
    for (int changed = 1; changed;) {
        changed = 0;
        for (uint32_t i = n; i-- > 0;) {
            const Cell *c = &code[i];
            Slots o = { { 0 } };
            int falls = c->op != OP_JMP && c->op != OP_RET;
            if (falls && i + 1 < n)
                for (int k = 0; k < 4; k++) o.w[k] |= in[i + 1].w[k];
            if (is_branch((int)c->op))
                for (int k = 0; k < 4; k++) o.w[k] |= in[c->imm].w[k];
            Slots l = o;
            if (c->op == OP_CALL) {
                slots_clear(&l, c->b);
                for (unsigned s = c->b; s < c->b + 16u && s < 256; s++) slots_set(&l, s);
            } else {
                if (info[c->op].writes) slots_clear(&l, c->c);
                if (slot_read[i] & 1) slots_set(&l, c->a);
                if (slot_read[i] & 2) slots_set(&l, c->b);
            }
            if (memcmp(&l, &in[i], sizeof l) || memcmp(&o, &out[i], sizeof o)) {
                in[i] = l; out[i] = o; changed = 1;
            }
        }
    }
    free(in);
    return out;
}

/* Rewrites the bytecode's opcodes into variant indices. Returns the root
 * function's pin word. */
static unsigned link_classes(Cell *code, uint32_t n, int use_acc, int use_pins, int use_dead) {
    uint8_t *target = calloc(n + 1, 1), *entry = calloc(n + 1, 1);
    entry[0] = 1;
    for (uint32_t i = 0; i < n; i++) {
        if (is_branch((int)code[i].op)) target[code[i].imm] = 1;
        if (code[i].op == OP_CALL) { target[code[i].imm] = 1; entry[code[i].imm] = 1; }
    }
    /* Each function runs from its entry to the next entry. */
    Pins *fpins = calloc(n, sizeof(Pins));
    uint32_t start = 0;
    unsigned root = 0;
    for (uint32_t i = 1; i <= n; i++) {
        if (i < n && !entry[i]) continue;
        int refs[256] = { 0 }, doms[256] = { 0 };
        for (uint32_t j = start; j < i; j++) {
            const Cell *c = &code[j];
            const OpInfo *o = &info[c->op];
            if (o->reads_a) { refs[c->a]++; doms[c->a] |= 1 << o->dom_a; }
            if (o->reads_b) { refs[c->b]++; doms[c->b] |= 1 << o->dom_b; }
            int s, d = produced(c, &s);
            if (d) { refs[s]++; doms[s] |= 1 << d; }
        }
        Pins p = { { -1, -1 }, { 0, 0 } };
        for (int k = 0; k < 2 && use_pins; k++) {
            int best = -1;
            for (int s = 0; s < NO_PIN; s++) {
                int dm = doms[s];
                if (!refs[s] || (dm != (1 << D_I) && dm != (1 << D_F))) continue;
                if (s == p.slot[0]) continue;
                if (best < 0 || refs[s] > refs[best]) best = s;
            }
            if (best >= 0) { p.slot[k] = best; p.dom[k] = doms[best] == (1 << D_I) ? D_I : D_F; }
        }
        for (uint32_t j = start; j < i; j++) fpins[j] = p;
        if (start == 0) root = pin_word(&p);
        start = i;
    }
    uint32_t *out = malloc(n * sizeof(uint32_t));
    uint8_t (*cls)[2] = calloc(n, sizeof *cls), *slot_read = calloc(n, 1);
    for (uint32_t i = 0; i < n; i++) {
        const Cell *c = &code[i];
        const OpInfo *o = &info[c->op];
        const Pins *p = &fpins[i];
        for (int k = 0; k < 2; k++) {
            int reads = k ? o->reads_b : o->reads_a, dom = k ? o->dom_b : o->dom_a;
            int slot = k ? c->b : c->a;
            if (!reads) continue;
            if (slot == p->slot[0] && p->dom[0] == dom) cls[i][k] = CL_P0;
            else if (slot == p->slot[1] && p->dom[1] == dom) cls[i][k] = CL_P1;
            else if (use_acc && i > 0 && !target[i]) {
                int s, d = produced(&code[i - 1], &s);
                if (d == dom && s == slot) cls[i][k] = CL_A;
            }
            if (cls[i][k] == CL_S) slot_read[i] |= (uint8_t)(1 << k);
        }
    }
    Slots *live_out = liveness(code, n, slot_read);
    for (uint32_t i = 0; i < n; i++) {
        Cell *c = &code[i];
        const OpInfo *o = &info[c->op];
        const Pins *p = &fpins[i];
        int dcls = DL_S;
        if (o->writes) {
            if (c->c == p->slot[0]) dcls = DL_P0;
            else if (c->c == p->slot[1]) dcls = DL_P1;
            else if (use_dead && !slots_has(&live_out[i], c->c)) dcls = DL_N;
        }
        if (c->op == OP_CALL) {
            c->c = (uint16_t)pin_word(&fpins[c->imm]);
            c->pad = (uint16_t)pin_word(p);
        }
        out[i] = c->op * N_VAR + (uint32_t)cls[i][0] * 16 + (uint32_t)cls[i][1] * 4 + (uint32_t)dcls;
    }
    for (uint32_t i = 0; i < n; i++) code[i].op = out[i];
    free(out); free(fpins); free(target); free(entry); free(live_out); free(cls); free(slot_read);
    return root;
}

/* ---- programs: vm.c's kernels, mandel writing zr and zi with FMOV --------- */

typedef struct { Cell *cells; uint32_t n, cap; } Prog;

static uint32_t emit(Prog *p, int op, int a, int b, int c, int32_t imm) {
    if (p->n == p->cap) {
        p->cap = p->cap ? p->cap * 2 : 256;
        p->cells = realloc(p->cells, p->cap * sizeof(Cell));
    }
    p->cells[p->n] = (Cell){ (uint32_t)op, (uint16_t)a, (uint16_t)b, (uint16_t)c, 0, imm };
    return p->n++;
}
static uint32_t here(const Prog *p) { return p->n; }
static void patch(Prog *p, uint32_t at, uint32_t target) { p->cells[at].imm = (int32_t)target; }

static void k_loop(Prog *p, uint64_t *regs, int64_t n) {
    emit(p, OP_MOVI, 0, 0, 0, 0);
    emit(p, OP_MOVI, 0, 0, 1, 0);
    emit(p, OP_MOVI, 0, 0, 2, (int32_t)n);
    emit(p, OP_MOVI, 0, 0, 4, 3);
    uint32_t top = here(p);
    emit(p, OP_SHRU, 0, 4, 3, 0);
    emit(p, OP_XOR, 3, 1, 3, 0);
    emit(p, OP_ADD, 0, 3, 0, 0);
    emit(p, OP_ADDI, 1, 0, 1, 1);
    emit(p, OP_LT_BR, 1, 2, 0, (int32_t)top);
    emit(p, OP_RET, 0, 0, 0, 0);
    (void)regs;
}

static void k_fib(Prog *p, uint64_t *regs, int64_t n) {
    emit(p, OP_MOVI, 0, 0, 10, (int32_t)n);
    uint32_t call_main = emit(p, OP_CALL, 0, 10, 0, 0);
    emit(p, OP_RET, 10, 0, 0, 0);
    uint32_t fib = here(p);
    patch(p, call_main, fib);
    uint32_t base_case = emit(p, OP_LTI_BR, 0, 2, 0, 0);
    emit(p, OP_ADDI, 0, 0, 10, -1);
    emit(p, OP_CALL, 0, 10, 0, (int32_t)fib);
    emit(p, OP_MOV, 10, 0, 2, 0);
    emit(p, OP_ADDI, 0, 0, 10, -2);
    emit(p, OP_CALL, 0, 10, 0, (int32_t)fib);
    emit(p, OP_ADD, 2, 10, 0, 0);
    emit(p, OP_RET, 0, 0, 0, 0);
    patch(p, base_case, here(p));
    emit(p, OP_RET, 0, 0, 0, 0);
    (void)regs;
}

#define SIEVE_N 8192
static void k_sieve(Prog *p, uint64_t *regs, int64_t reps) {
    emit(p, OP_MOVI, 0, 0, 0, 0);
    emit(p, OP_MOVI, 0, 0, 3, SIEVE_N);
    emit(p, OP_MOVI, 0, 0, 4, 1);
    emit(p, OP_MOVI, 0, 0, 6, 0);
    emit(p, OP_MOVI, 0, 0, 7, (int32_t)reps);
    emit(p, OP_MOVI, 0, 0, 8, 0);
    uint32_t rep_top = here(p);
    emit(p, OP_MOVI, 0, 0, 1, 0);
    uint32_t fill = here(p);
    emit(p, OP_STORE8, 1, 4, 0, 0);
    emit(p, OP_ADDI, 1, 0, 1, 1);
    emit(p, OP_LT_BR, 1, 3, 0, (int32_t)fill);
    emit(p, OP_MOVI, 0, 0, 1, 2);
    uint32_t outer = here(p);
    emit(p, OP_LOAD8, 1, 0, 5, 0);
    uint32_t skip = emit(p, OP_EQZ_BR, 5, 0, 0, 0);
    emit(p, OP_ADDI, 0, 0, 0, 1);
    emit(p, OP_ADD, 1, 1, 2, 0);
    uint32_t inner_test = emit(p, OP_JMP, 0, 0, 0, 0);
    uint32_t inner = here(p);
    emit(p, OP_STORE8, 2, 8, 0, 0);
    emit(p, OP_ADD, 2, 1, 2, 0);
    patch(p, inner_test, here(p));
    emit(p, OP_LT_BR, 2, 3, 0, (int32_t)inner);
    patch(p, skip, here(p));
    emit(p, OP_ADDI, 1, 0, 1, 1);
    emit(p, OP_LT_BR, 1, 3, 0, (int32_t)outer);
    emit(p, OP_ADDI, 6, 0, 6, 1);
    emit(p, OP_LT_BR, 6, 7, 0, (int32_t)rep_top);
    emit(p, OP_RET, 0, 0, 0, 0);
    (void)regs;
}

static void k_mandel(Prog *p, uint64_t *regs, int64_t size) {
    regs[20] = as_u(4.0); regs[21] = as_u(2.0);
    regs[22] = as_u(-2.0); regs[23] = as_u(-1.5);
    regs[24] = as_u(3.0 / (double)size); regs[25] = as_u(3.0 / (double)size);
    emit(p, OP_MOVI, 0, 0, 0, 0);
    emit(p, OP_MOVI, 0, 0, 3, (int32_t)size);
    emit(p, OP_MOVI, 0, 0, 4, 100);
    emit(p, OP_MOVI, 0, 0, 1, 0);
    uint32_t row = here(p);
    emit(p, OP_I2F, 1, 0, 12, 0);
    emit(p, OP_FMUL, 12, 25, 12, 0);
    emit(p, OP_FADD, 12, 23, 7, 0);
    emit(p, OP_MOVI, 0, 0, 2, 0);
    uint32_t col = here(p);
    emit(p, OP_I2F, 2, 0, 12, 0);
    emit(p, OP_FMUL, 12, 24, 12, 0);
    emit(p, OP_FADD, 12, 22, 6, 0);
    emit(p, OP_FMOV, 13, 0, 8, 0);      /* r13 is never written: 0.0 */
    emit(p, OP_FMOV, 13, 0, 9, 0);
    emit(p, OP_MOVI, 0, 0, 5, 0);
    uint32_t iter = here(p);
    emit(p, OP_FMUL, 8, 8, 10, 0);
    emit(p, OP_FMUL, 9, 9, 11, 0);
    emit(p, OP_FADD, 10, 11, 12, 0);
    uint32_t escape = emit(p, OP_FGT_BR, 12, 20, 0, 0);
    emit(p, OP_FMUL, 8, 9, 12, 0);
    emit(p, OP_FMUL, 12, 21, 12, 0);
    emit(p, OP_FADD, 12, 7, 9, 0);
    emit(p, OP_FSUB, 10, 11, 12, 0);
    emit(p, OP_FADD, 12, 6, 8, 0);
    emit(p, OP_ADDI, 5, 0, 5, 1);
    emit(p, OP_LT_BR, 5, 4, 0, (int32_t)iter);
    patch(p, escape, here(p));
    emit(p, OP_ADD, 0, 5, 0, 0);
    emit(p, OP_ADDI, 2, 0, 2, 1);
    emit(p, OP_LT_BR, 2, 3, 0, (int32_t)col);
    emit(p, OP_ADDI, 1, 0, 1, 1);
    emit(p, OP_LT_BR, 1, 3, 0, (int32_t)row);
    emit(p, OP_RET, 0, 0, 0, 0);
}

#define POLY_BODY 1536
static void k_poly(Prog *p, uint64_t *regs, int64_t reps) {
    static const int ops[] = { OP_ADD, OP_SUB, OP_XOR, OP_AND, OP_SHL, OP_SHRU,
                               OP_MUL, OP_MOV, OP_ADDI };
    uint64_t seed = 0x9e3779b97f4a7c15ull;
    for (int r = 0; r < 8; r++)
        emit(p, OP_MOVI, 0, 0, r, r * 7919 + 13);
    emit(p, OP_MOVI, 0, 0, 8, 0);
    emit(p, OP_MOVI, 0, 0, 9, (int32_t)reps);
    uint32_t top = here(p);
    for (int k = 0; k < POLY_BODY; k++) {
        seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
        int op = ops[seed % 9];
        int a = (int)((seed >> 8) & 7), b = (int)((seed >> 11) & 7), c = (int)((seed >> 14) & 7);
        emit(p, op, a, b, c, (int32_t)((seed >> 20) & 0xff) + 1);
    }
    emit(p, OP_ADDI, 8, 0, 8, 1);
    emit(p, OP_LT_BR, 8, 9, 0, (int32_t)top);
    for (int r = 1; r < 8; r++)
        emit(p, OP_XOR, 0, r, 0, 0);
    emit(p, OP_RET, 0, 0, 0, 0);
    (void)regs;
}

typedef struct {
    const char *name;
    void (*build)(Prog *, uint64_t *, int64_t);
    int64_t param;
} Kernel;

static const Kernel kernels[] = {
    { "loop", k_loop, 200000000 },
    { "fib", k_fib, 35 },
    { "sieve", k_sieve, 4000 },
    { "mandel", k_mandel, 600 },
    { "poly", k_poly, 100000 },
};

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: %s kernel [none|acc|accpin|full]\n", argv[0]);
        return 2;
    }
    const Kernel *k = NULL;
    for (size_t i = 0; i < sizeof kernels / sizeof kernels[0]; i++)
        if (strcmp(argv[1], kernels[i].name) == 0) k = &kernels[i];
    if (!k) { fprintf(stderr, "unknown kernel %s\n", argv[1]); return 2; }
    const char *mode = argc > 2 ? argv[2] : "full";
    int use_dead = strcmp(mode, "full") == 0;
    int use_pins = use_dead || strcmp(mode, "accpin") == 0;
    int use_acc = use_pins || strcmp(mode, "acc") == 0;
    if (!use_acc && strcmp(mode, "none") != 0) { fprintf(stderr, "unknown mode %s\n", mode); return 2; }

    uint64_t *regs = calloc(REGS_LEN, sizeof(uint64_t));
    uint8_t *mem = calloc(SIEVE_N, 1);
    Ret *rs = calloc(RS_CAP, sizeof(Ret));
    Prog p = { 0 };
    k->build(&p, regs, k->param);
    unsigned root_pins = link_classes(p.cells, p.n, use_acc, use_pins, use_dead);
#if DISPATCH == 4
    for (uint32_t i = 0; i < p.n; i++) {
        intptr_t off = (const char *)handlers[p.cells[i].op] - (const char *)h_base;
        if (off != (int32_t)off) trap("handler offset");
        p.cells[i].op = (uint32_t)(int32_t)off;
    }
#endif
    uint64_t result = run(p.cells, p.n, regs, REGS_LEN, mem, SIEVE_N, rs, root_pins);
    printf("%s %llu\n", k->name, (unsigned long long)result);
#ifdef COUNT
    printf("dispatches %llu\n", (unsigned long long)dispatches);
#endif
    return 0;
}
