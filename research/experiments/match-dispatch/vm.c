/* E0 of research/investigations/match-dispatch: one small register-machine
 * interpreter whose handler semantics are written once (the BODY_* macros)
 * and compiled under several dispatch shapes and operand-access forms.
 *
 * Build-time selection:
 *   -DDISPATCH=1  switch in a loop
 *   -DDISPATCH=2  computed goto
 *   -DDISPATCH=3  one function per opcode, musttail through a handler table
 *   -DDISPATCH=4  as 3, but each cell's opcode field holds its handler's
 *                 offset from a base handler (no table load)
 *   -DPRESERVE_NONE  handlers of DISPATCH 3/4 use preserve_none
 *   -DACCESS=1  checked: frame index base+a compared with the frame length,
 *               fetch index pc compared with the code length
 *   -DACCESS=2  u8 operands: base+(u8)a, no check, a call guarantees 256
 *               slots of headroom; fetch index still compared
 *   -DACCESS=3  raw: cell and frame pointers, no checks (not expressible in
 *               Whitefoot; the lower bound)
 *   -DCOUNT     count dispatches (a separate build; timing builds omit it)
 *   -DPAD=n     n bytes of unreachable code before the handlers (null
 *               comparison of layout)
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef DISPATCH
#define DISPATCH 1
#endif
#ifndef ACCESS
#define ACCESS 1
#endif

typedef struct {
    uint32_t op;           /* opcode, or handler offset under DISPATCH 4 */
    uint16_t a, b, c;      /* operand slots; b doubles as a small immediate */
    uint16_t pad;
    int32_t imm;           /* immediate, branch target or call target */
} Cell;

typedef struct {
    uintptr_t pc;
    uintptr_t base;
} Ret;

#define OPS(X) \
    X(MOVI) X(MOV) X(ADD) X(SUB) X(MUL) X(AND) X(XOR) X(SHL) X(SHRU) X(ADDI) \
    X(LT_BR) X(LTI_BR) X(NE_BR) X(EQZ_BR) X(JMP) \
    X(LOAD8) X(STORE8) X(LOAD32) X(STORE32) \
    X(FADD) X(FSUB) X(FMUL) X(FGT_BR) X(I2F) \
    X(CALL) X(RET)

enum {
#define ENUM_OP(n) OP_##n,
    OPS(ENUM_OP)
#undef ENUM_OP
    N_OPS
};

#define FRAME_HEADROOM 256
#define RS_CAP 100000
#define REGS_LEN (RS_CAP * 16 + FRAME_HEADROOM * 2)

#ifdef COUNT
static uint64_t dispatches;
#define COUNTED() (dispatches++)
#else
#define COUNTED() ((void)0)
#endif

/* A failed check stops the process without a call: a call would make every
 * handler a non-leaf function that saves the link register and frame
 * pointer on each dispatch, a cost a Whitefoot handler, whose failed check
 * is an early return, does not pay. */
#define trap(why) __builtin_trap()

#define LIKELY(x) __builtin_expect(!!(x), 1)
#define UNLIKELY(x) __builtin_expect(!!(x), 0)

static inline double as_f(uint64_t v) { double d; memcpy(&d, &v, 8); return d; }
static inline uint64_t as_u(double d) { uint64_t v; memcpy(&v, &d, 8); return v; }

/* ---- operand access ------------------------------------------------------ */

#if ACCESS == 1
#define SLOT(x) (*({ uint64_t i_ = base + (x); if (UNLIKELY(i_ >= len)) trap("frame index"); &regs[i_]; }))
#define FETCHCHK() do { if (UNLIKELY(pc >= ncode)) trap("fetch index"); } while (0)
#define OPND(f) (ip->f)
#elif ACCESS == 2
#define SLOT(x) (regs[base + (x)])
#define FETCHCHK() do { if (UNLIKELY(pc >= ncode)) trap("fetch index"); } while (0)
#define OPND(f) ((uint8_t)ip->f)
#else
#define SLOT(x) (fp[(x)])
#define FETCHCHK() ((void)0)
#define OPND(f) (ip->f)
#endif

#define RA SLOT(OPND(a))
#define RB SLOT(OPND(b))
#define WC(v) (SLOT(OPND(c)) = (v))
#define SB ((int64_t)(int16_t)ip->b)
#define IMM ((int64_t)ip->imm)

/* ---- control, per access form -------------------------------------------- */

#if ACCESS == 3
#define CUR_IP ip
#define ADVANCE() (ip += 1)
#define SETPC(t) (ip = code + (t))
#define CALL_ENTER(t, off) do { \
        if (UNLIKELY(rsp >= RS_CAP)) trap("call depth"); \
        rs[rsp].pc = (uintptr_t)(ip + 1); rs[rsp].base = (uintptr_t)fp; rsp++; \
        fp += (off); \
        if (UNLIKELY(fp + FRAME_HEADROOM > regs_end)) trap("frame space"); \
        SETPC(t); } while (0)
#define RET_POP() do { rsp--; ip = (const Cell *)rs[rsp].pc; fp = (uint64_t *)rs[rsp].base; } while (0)
#else
#define CUR_IP (&code[pc])
#define ADVANCE() (pc += 1)
#define SETPC(t) (pc = (uint32_t)(t))
#define CALL_ENTER(t, off) do { \
        if (UNLIKELY(rsp >= RS_CAP)) trap("call depth"); \
        rs[rsp].pc = pc + 1; rs[rsp].base = base; rsp++; \
        base += (off); \
        if (UNLIKELY(base + FRAME_HEADROOM > len)) trap("frame space"); \
        SETPC(t); } while (0)
#define RET_POP() do { rsp--; pc = (uint32_t)rs[rsp].pc; base = rs[rsp].base; } while (0)
#endif

/* The current cell: the raw form carries it as `ip` already; the index
 * forms name it from `pc` in each handler. */
#if ACCESS == 3
#define BIND_IP
#else
#define BIND_IP const Cell *ip = &code[pc];
#endif

#define NEXT() do { ADVANCE(); FETCHCHK(); DISPATCH_NEXT(); } while (0)
#define JUMP(t) do { SETPC(t); FETCHCHK(); DISPATCH_NEXT(); } while (0)

/* ---- handler semantics, written once ------------------------------------- */

#define BODY_MOVI   { WC((uint64_t)IMM); NEXT(); }
#define BODY_MOV    { WC(RA); NEXT(); }
#define BODY_ADD    { WC(RA + RB); NEXT(); }
#define BODY_SUB    { WC(RA - RB); NEXT(); }
#define BODY_MUL    { WC(RA * RB); NEXT(); }
#define BODY_AND    { WC(RA & RB); NEXT(); }
#define BODY_XOR    { WC(RA ^ RB); NEXT(); }
#define BODY_SHL    { WC(RA << (RB & 63)); NEXT(); }
#define BODY_SHRU   { WC(RA >> (RB & 63)); NEXT(); }
#define BODY_ADDI   { WC(RA + (uint64_t)IMM); NEXT(); }
#define BODY_LT_BR  { if ((int64_t)RA < (int64_t)RB) JUMP(IMM); NEXT(); }
#define BODY_LTI_BR { if ((int64_t)RA < SB) JUMP(IMM); NEXT(); }
#define BODY_NE_BR  { if (RA != RB) JUMP(IMM); NEXT(); }
#define BODY_EQZ_BR { if (RA == 0) JUMP(IMM); NEXT(); }
#define BODY_JMP    { JUMP(IMM); }
#define BODY_LOAD8  { uint64_t ad_ = RA + (uint32_t)IMM; \
                      if (UNLIKELY(ad_ >= memlen)) trap("memory"); \
                      WC(mem[ad_]); NEXT(); }
#define BODY_STORE8 { uint64_t ad_ = RA + (uint32_t)IMM; \
                      if (UNLIKELY(ad_ >= memlen)) trap("memory"); \
                      mem[ad_] = (uint8_t)RB; NEXT(); }
#define BODY_LOAD32 { uint64_t ad_ = RA + (uint32_t)IMM; uint32_t v_; \
                      if (UNLIKELY(ad_ + 4 > memlen)) trap("memory"); \
                      memcpy(&v_, mem + ad_, 4); WC(v_); NEXT(); }
#define BODY_STORE32 { uint64_t ad_ = RA + (uint32_t)IMM; uint32_t v_ = (uint32_t)RB; \
                      if (UNLIKELY(ad_ + 4 > memlen)) trap("memory"); \
                      memcpy(mem + ad_, &v_, 4); NEXT(); }
#define BODY_FADD   { WC(as_u(as_f(RA) + as_f(RB))); NEXT(); }
#define BODY_FSUB   { WC(as_u(as_f(RA) - as_f(RB))); NEXT(); }
#define BODY_FMUL   { WC(as_u(as_f(RA) * as_f(RB))); NEXT(); }
#define BODY_FGT_BR { if (as_f(RA) > as_f(RB)) JUMP(IMM); NEXT(); }
#define BODY_I2F    { WC(as_u((double)(int64_t)RA)); NEXT(); }
/* A call's callee frame starts at slot b of the caller's frame; the callee
 * returns its result in its slot 0, which is the caller's slot b. */
#define BODY_CALL   { uint64_t off_ = ip->b; CALL_ENTER(IMM, off_); FETCHCHK(); DISPATCH_NEXT(); }
#define BODY_RET    { uint64_t v_ = RA; SLOT(0) = v_; \
                      if (rsp == 0) return v_; \
                      RET_POP(); DISPATCH_NEXT(); }

/* ---- dispatch shapes ------------------------------------------------------ */

typedef struct {
    Ret *rs;
    uint64_t *regs_end;
} Spill;

#if DISPATCH == 1 || DISPATCH == 2

static uint64_t run(const Cell *code, uint32_t ncode, uint64_t *regs, uint64_t len,
                    uint8_t *mem, uint64_t memlen, Ret *rs) {
    (void)ncode; (void)len;
    uint64_t rsp = 0;
#if ACCESS == 3
    const Cell *ip = code;
    uint64_t *fp = regs;
    uint64_t *regs_end = regs + len;
#else
    uint32_t pc = 0;
    uint64_t base = 0;
#endif
#if DISPATCH == 1
#define DISPATCH_NEXT() goto top
    for (;;) {
    top:;
        COUNTED();
        switch (CUR_IP->op) {
#define CASE_OP(n) case OP_##n: { BIND_IP BODY_##n }
            OPS(CASE_OP)
#undef CASE_OP
        default:
            __builtin_unreachable();
        }
    }
#else
    static void *const labels[N_OPS] = {
#define LABEL_OP(n) &&L_##n,
        OPS(LABEL_OP)
#undef LABEL_OP
    };
#define DISPATCH_NEXT() do { COUNTED(); goto *labels[CUR_IP->op]; } while (0)
    DISPATCH_NEXT();
#define GOTO_OP(n) L_##n: { BIND_IP BODY_##n }
    OPS(GOTO_OP)
#undef GOTO_OP
#endif
}

#else /* DISPATCH 3 or 4 */

#ifdef PRESERVE_NONE
#define CC __attribute__((preserve_none))
#else
#define CC
#endif

#if ACCESS == 1
#define PARAMS const Cell *code, uint32_t pc, uint64_t *regs, uint64_t base, uint64_t len, \
               uint8_t *mem, uint64_t memlen, uint32_t ncode, uint64_t rsp, Spill *st
#define ARGS code, pc, regs, base, len, mem, memlen, ncode, rsp, st
#elif ACCESS == 2
#define PARAMS const Cell *code, uint32_t pc, uint64_t *regs, uint64_t base, \
               uint8_t *mem, uint64_t memlen, uint32_t ncode, uint64_t rsp, Spill *st
#define ARGS code, pc, regs, base, mem, memlen, ncode, rsp, st
#else
#define PARAMS const Cell *code, const Cell *ip, uint64_t *fp, \
               uint8_t *mem, uint64_t memlen, uint64_t rsp, Spill *st
#define ARGS code, ip, fp, mem, memlen, rsp, st
#endif

typedef uint64_t (CC *Handler)(PARAMS);

#define DECL_OP(n) CC static uint64_t h_##n(PARAMS);
OPS(DECL_OP)
#undef DECL_OP

#if DISPATCH == 3
static Handler const handlers[N_OPS] = {
#define TABLE_OP(n) h_##n,
    OPS(TABLE_OP)
#undef TABLE_OP
};
#define HANDLER_OF(op) (handlers[(op)])
#else
#define HANDLER_OF(op) ((Handler)((const char *)h_base + (int32_t)(op)))
CC __attribute__((noinline)) static uint64_t h_base(PARAMS);
#endif

#define DISPATCH_NEXT() do { COUNTED(); __attribute__((musttail)) return HANDLER_OF(CUR_IP->op)(ARGS); } while (0)

#if ACCESS == 2
#define LEN_FROM_SPILL uint64_t len = (uint64_t)(st->regs_end - regs); (void)len;
#else
#define LEN_FROM_SPILL
#endif
#if ACCESS == 3
#define SPILL_LOCALS Ret *rs = st->rs; uint64_t *regs_end = st->regs_end; (void)regs_end;
#else
#define SPILL_LOCALS Ret *rs = st->rs; LEN_FROM_SPILL
#endif

#ifdef PAD
__attribute__((used, noinline)) static void pad_before_handlers(void) {
#define STR_(x) #x
#define STR(x) STR_(x)
    __asm__ volatile(".space " STR(PAD));
}
#endif

#if DISPATCH == 4
CC __attribute__((noinline)) static uint64_t h_base(PARAMS) { (void)code; trap("base handler"); }
#endif

#define DEF_OP(n) CC static uint64_t h_##n(PARAMS) { SPILL_LOCALS (void)rs; BIND_IP BODY_##n }
OPS(DEF_OP)
#undef DEF_OP

static uint64_t run(const Cell *code, uint32_t ncode, uint64_t *regs, uint64_t len,
                    uint8_t *mem, uint64_t memlen, Ret *rs) {
    Spill st = { rs, regs + len };
    (void)ncode; (void)len;
    COUNTED();
#if ACCESS == 1
    return HANDLER_OF(code[0].op)(code, 0, regs, 0, len, mem, memlen, ncode, 0, &st);
#elif ACCESS == 2
    return HANDLER_OF(code[0].op)(code, 0, regs, 0, mem, memlen, ncode, 0, &st);
#else
    return HANDLER_OF(code[0].op)(code, code, regs, mem, memlen, 0, &st);
#endif
}

#endif

/* ---- program construction -------------------------------------------------- */

typedef struct {
    Cell *cells;
    uint32_t n, cap;
} Prog;

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

/* Every kernel returns its checksum through RET from the root frame. Frame
 * slots are numbered from the frame base; the host may preload slots of the
 * root frame (constants) before the run. */

static void k_loop(Prog *p, uint64_t *regs, int64_t n) {
    /* r0 sum, r1 i, r2 n, r3 tmp, r4 = 3 */
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
    /* main: r10 = n; call fib with frame at 10; return r10 */
    emit(p, OP_MOVI, 0, 0, 10, (int32_t)n);
    uint32_t call_main = emit(p, OP_CALL, 0, 10, 0, 0);
    emit(p, OP_RET, 10, 0, 0, 0);
    /* fib: r0 n; r2 saved fib(n-1); callee frame at 10 */
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
    /* r0 count total, r1 i, r2 j, r3 SIEVE_N, r4 one, r5 tmp, r6 rep, r7 reps, r8 zero */
    emit(p, OP_MOVI, 0, 0, 0, 0);
    emit(p, OP_MOVI, 0, 0, 3, SIEVE_N);
    emit(p, OP_MOVI, 0, 0, 4, 1);
    emit(p, OP_MOVI, 0, 0, 6, 0);
    emit(p, OP_MOVI, 0, 0, 7, (int32_t)reps);
    emit(p, OP_MOVI, 0, 0, 8, 0);
    uint32_t rep_top = here(p);
    /* fill */
    emit(p, OP_MOVI, 0, 0, 1, 0);
    uint32_t fill = here(p);
    emit(p, OP_STORE8, 1, 4, 0, 0);
    emit(p, OP_ADDI, 1, 0, 1, 1);
    emit(p, OP_LT_BR, 1, 3, 0, (int32_t)fill);
    /* sieve */
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
    /* constants preloaded: r20 4.0, r21 2.0, r22 x0, r23 y0, r24 dx, r25 dy
     * r0 total, r1 y, r2 x, r3 size, r4 maxiter, r5 it,
     * r6 cr, r7 ci, r8 zr, r9 zi, r10 zr2, r11 zi2, r12 t, r13 zero */
    regs[20] = as_u(4.0); regs[21] = as_u(2.0);
    regs[22] = as_u(-2.0); regs[23] = as_u(-1.5);
    regs[24] = as_u(3.0 / (double)size); regs[25] = as_u(3.0 / (double)size);
    emit(p, OP_MOVI, 0, 0, 0, 0);
    emit(p, OP_MOVI, 0, 0, 3, (int32_t)size);
    emit(p, OP_MOVI, 0, 0, 4, 100);
    emit(p, OP_MOVI, 0, 0, 13, 0);
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
    emit(p, OP_MOV, 13, 0, 8, 0);
    emit(p, OP_MOV, 13, 0, 9, 0);
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
    /* r0..r7 state, r8 rep, r9 reps; a shuffled straight-line body far
     * longer than the indirect predictor's memorisation period. */
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
    /* fold r0..r7 into r0 */
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
        fprintf(stderr, "usage: %s kernel [scale-percent]\n", argv[0]);
        return 2;
    }
    const Kernel *k = NULL;
    for (size_t i = 0; i < sizeof kernels / sizeof kernels[0]; i++)
        if (strcmp(argv[1], kernels[i].name) == 0) k = &kernels[i];
    if (!k) { fprintf(stderr, "unknown kernel %s\n", argv[1]); return 2; }
    int64_t param = k->param;
    if (argc > 2 && k->build != k_fib) param = param * atoll(argv[2]) / 100;
    if (argc > 2 && k->build == k_fib) param = atoll(argv[2]);

    uint64_t *regs = calloc(REGS_LEN, sizeof(uint64_t));
    uint8_t *mem = calloc(SIEVE_N, 1);
    Ret *rs = calloc(RS_CAP, sizeof(Ret));
    Prog p = { 0 };
    k->build(&p, regs, param);
#if DISPATCH == 4
    for (uint32_t i = 0; i < p.n; i++) {
        static Handler const table[N_OPS] = {
#define TABLE_OP4(n) h_##n,
            OPS(TABLE_OP4)
#undef TABLE_OP4
        };
        intptr_t off = (const char *)table[p.cells[i].op] - (const char *)h_base;
        if (off != (int32_t)off) trap("handler offset");
        p.cells[i].op = (uint32_t)(int32_t)off;
    }
#endif
    uint64_t result = run(p.cells, p.n, regs, REGS_LEN, mem, SIEVE_N, rs);
    printf("%s %llu\n", k->name, (unsigned long long)result);
#ifdef COUNT
    printf("dispatches %llu\n", (unsigned long long)dispatches);
#endif
    return 0;
}
