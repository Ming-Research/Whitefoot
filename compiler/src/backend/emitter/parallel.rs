//! Actualization of the permission judgment's overlap groups.
//!
//! For a group of sibling calls the checker permitted to overlap, every member
//! but the last is *handed out*: a lane is acquired, the call's arguments are
//! stored into that lane's frame, and the call — outlined into an internal
//! thunk over the frame — is published to the lane. The remaining member then
//! runs inline on the calling thread, and each handed-out member is joined
//! immediately after it — before the group's values are read and before any
//! exit edge.
//!
//! By default both edges call the same monomorphized function on the same
//! arguments. The opt-in sequential-refusal experiment instead calls its
//! existing sequential clone at the join, when one exists.
//! The clone has the same ordinary ABI and operations, and merely
//! declines descendant compute permissions. The successful task is unchanged.
//! Nothing here consults a fact, a source proof statement, or a row; it
//! consumes the group the checker already judged.
//!
//! **Lane acquisition comes before the frame.** The frame belongs to the lane, not to
//! the calling function, and nothing about it is built until a lane has been
//! granted. An activation that is refused a lane executes a null test and its
//! own call without reserving a task frame on the caller's stack. This removes
//! that additional frame from each recursive activation; it does not guarantee
//! identical spills or recursion depth in parallel and sequential machine code.
//! The earlier shape — a frame in the calling
//! function's entry block — put a slot and its stores in *every* activation of
//! an eligible recursive function, which cost about four times the stack per
//! frame on a small one and turned a recursion that ran into a bare SIGSEGV.
//!
//! Handing a call to another thread does still cost the caller what a
//! parallel schedule costs: the lane handle is live across the inline member,
//! and the thunk is a second caller of the handed-out function whose arguments
//! come out of memory, so an interprocedural fact about those arguments —
//! a constant, say — does not survive into the `--par` build.
//!
//! **Two worlds, selected once.** What the paragraph above describes is a cost
//! of *overlapping*, and a program that asked for lanes and did not get them
//! should not pay it. So a `--par` module carries the lowering above and, for
//! every function on a path from the entry to a handed-out call, a second copy
//! that actualizes nothing — the sequential lowering, byte for byte, so that
//! every transform the default build gets fires on it. The bootstrap asks the
//! runtime once whether this run was asked for a pool, and enters one world or
//! the other. In the default policy neither calls into the other, so nothing
//! below that branch tests anything again. The experimental refusal policy
//! also permits the already-required null branch to enter a sequential clone.
//! [`sequential_clone_set`] explains the second copy, the default selection, and
//! why the set is exactly that closure.
//!
//! **Symbol reservation.** A source function is emitted as `wf_` followed by
//! its own IDENT, and [FORM-3] spells IDENT `[a-z][a-z0-9_]*`, so no source
//! name can produce a symbol whose first character after `wf_` is an
//! underscore. Every symbol this module and the runtime introduce therefore
//! begins `wf__par_`: the prefix is unreachable from source, so a program that
//! declares `fn par_try_fork(...)` still compiles and links exactly as it did
//! before this module existed. That is a reserved namespace, not a name check
//! — nothing here inspects a source function's spelling.

use std::collections::HashSet;
use std::fmt::Write;

use super::{BackendFailure, FunctionEmitter, IntrinsicDeclaration, llvm_type, value_name};
use crate::backend::abi::{FunctionAbi, ResultAbi};
use crate::{
    IrAddressed, IrFunction, IrInstruction, IrNominalKind, IrOperation, IrProgram, IrSynthesis,
    IrType, IrValueId, IrWorkEstimate,
};

/// The counted loop's index type [FN-1], which fixes every width question the
/// split could otherwise have.
const U64: IrType = IrType::Integer {
    width: 64,
    signed: false,
};

/// One [`IrOperation::LoopSplit`] site, as the two renderings read it.
pub(super) struct LoopSplitSite<'ir> {
    pub(super) splitter: u32,
    pub(super) chunk: u32,
    pub(super) seed: IrValueId,
    pub(super) lower: IrValueId,
    pub(super) upper: IrValueId,
    pub(super) captures: &'ir [IrValueId],
    pub(super) weight: u64,
    pub(super) work: Option<&'ir IrWorkEstimate>,
}

/// The Windows parallel ABI as external obligations.
///
/// COFF modules do not carry the sequential weak definitions below.  A
/// Windows module that hands out work therefore cannot link unless the
/// runtime supplies the strong protocol; this is the compile-time half of the
/// fail-closed backend contract. All maintained targets use the protocol in
/// `sched/core.c`, configuration in `sched/entry.c`, and platform primitives.
/// Windows requires those external definitions at link time; pool resource
/// exhaustion still uses the ordinary-call fallback.
pub(crate) const PARALLEL_RUNTIME_DECLARATIONS: &str = "declare ptr @wf__par_acquire_lane(i64)\ndeclare void @wf__par_publish(ptr, ptr)\ndeclare void @wf__par_join(ptr)\ndeclare void @wf__par_release(ptr)\n";

/// The fail-closed Windows declaration of the once-per-process backend query.
pub(crate) const PARALLEL_POOL_QUERY_DECLARATION: &str = "declare i32 @wf__par_pool_active()\n";

/// The fail-closed Windows declaration of the loop split budget query.
pub(crate) const PARALLEL_SPLIT_BUDGET_DECLARATION: &str =
    "declare i64 @wf__par_split_budget(i64, i64)\n";

/// The fail-closed Windows declaration of the recursion budget query.
pub(crate) const PARALLEL_RECURSION_BUDGET_DECLARATION: &str =
    "declare i64 @wf__par_recursion_budget()\n";

/// A non-Windows module's own definition of the lane protocol: acquire no lane,
/// ever.
///
/// A non-Windows module that hands work out carries a *weak* sequential answer
/// to every runtime entry point, so it is a complete program on its own: with
/// no runtime linked, every acquisition is refused, so no frame is ever built, no
/// task is ever published, and every handed-out call runs on its own thread at
/// its own fallback edge — exactly today's schedule. Linking the runtime
/// replaces all four with its strong definitions, and only then can a lane be
/// granted. Windows deliberately takes the external declarations above and
/// cannot link without the scheduler core.
///
/// On the optional-runtime targets, the alternative — plain declarations —
/// would make the runtime a link obligation of every path that ever builds a
/// Whitefoot program rather than an option of the paths that want lanes. The
/// Windows production contract deliberately chooses that obligation.
pub(crate) const PARALLEL_RUNTIME_FALLBACK: &str = "define weak ptr @wf__par_acquire_lane(i64 %bytes) {\nentry:\n  ret ptr null\n}\n\ndefine weak void @wf__par_publish(ptr %frame, ptr %fn) {\nentry:\n  ret void\n}\n\ndefine weak void @wf__par_join(ptr %frame) {\nentry:\n  ret void\n}\n\ndefine weak void @wf__par_release(ptr %frame) {\nentry:\n  ret void\n}\n\n";

/// The first line of [`PARALLEL_RUNTIME_FALLBACK`], and so the marker a
/// non-Windows link path reads.
pub(crate) const PARALLEL_LANE_ACQUISITION_SYMBOL: &str =
    "define weak ptr @wf__par_acquire_lane(i64 %bytes)";

/// The first Windows declaration of the same ABI, used as that target's link
/// marker.
pub(crate) const PARALLEL_LANE_ACQUISITION_DECLARATION: &str =
    "declare ptr @wf__par_acquire_lane(i64)";

/// True when this emitted module hands work out, so linking the parallel
/// runtime would let it take lanes.
///
/// A module with no permitted overlap group names none of the runtime's
/// symbols, so nothing of the runtime — not one thread, not one atomic —
/// reaches a program that has no use for it. On the optional POSIX path, a
/// module that does hand work out still links and runs sequentially without
/// the runtime. On Windows, the same predicate recognizes external
/// declarations and is a hard link obligation.
pub fn module_requires_parallel_runtime(module: &str) -> bool {
    module.contains(PARALLEL_LANE_ACQUISITION_SYMBOL)
        || module.contains(PARALLEL_LANE_ACQUISITION_DECLARATION)
}

/// The runtime's answer to "was this run asked for a pool", put once per
/// process, and a non-Windows module's own weak answer of "no".
///
/// The optional-runtime path carries this for the same reason it carries the
/// four entry points: with no runtime linked, no pool can ever start, so the
/// honest answer is a constant zero and the program is complete on its own.
/// Windows emits the external declaration above instead. The query is not part
/// of the lane protocol — it takes no frame, moves no work, and starts nothing
/// — so it remains separate from the four protocol signatures.
pub(crate) const PARALLEL_POOL_QUERY_FALLBACK: &str =
    "define weak i32 @wf__par_pool_active() {\nentry:\n  ret i32 0\n}\n\n";

/// The runtime's answer to "how many times may a split of this span halve",
/// and a non-Windows module's own weak answer of "not at all".
///
/// Carried by the optional-runtime path for the same reason as the query above:
/// with no runtime linked there are no lanes, so the honest allowance is zero
/// and a splitter that gets it descends straight to its leaf — one call, then
/// the loop. Windows leaves the external query unresolved until native link.
///
/// It is a separate definition rather than a fifth lane-protocol entry point
/// because it takes no frame, publishes nothing, and moves no work; keeping it
/// apart also leaves the four protocol signatures' bytes exactly as they were.
pub(crate) const PARALLEL_SPLIT_BUDGET_FALLBACK: &str =
    "define weak i64 @wf__par_split_budget(i64 %span, i64 %weight) {\nentry:\n  ret i64 0\n}\n\n";

/// The runtime's answer to "how many levels of this recursive component may
/// still hand work out", and a non-Windows module's own weak answer of "none".
///
/// Carried by the optional-runtime path for the same reason as the allowance
/// above: with no runtime linked there are no lanes, so the honest answer is
/// zero and the component's ordinary entry descends straight into its
/// sequential clone — the world a pool-less run wants anyway. Windows leaves
/// the external query unresolved until native link.
pub(crate) const PARALLEL_RECURSION_BUDGET_FALLBACK: &str =
    "define weak i64 @wf__par_recursion_budget() {\nentry:\n  ret i64 0\n}\n\n";

/// The symbol one function's sequential clone is emitted under.
///
/// It lives in the same reserved `wf__par_` namespace as the runtime's own
/// symbols, which [FORM-3] puts out of reach of any source IDENT, so cloning a
/// function can never collide with a function the writer declared.
pub(crate) fn sequential_clone_symbol(name: &str) -> String {
    format!("wf__par_seq_{name}")
}

/// The functions that need a sequential clone: every function on some path
/// from the entry to a handed-out call.
///
/// **Why a second copy exists at all.** The hand-out rejoins its granted and
/// refused edges through a phi, so the callee's result flows into a phi rather
/// than into the caller's return. That is invisible on a heavy call and
/// decisive on a light one: it takes `fib`'s second recursion out of tail
/// position, and with it LLVM's accumulator tail-recursion elimination, which
/// turns the sequential build's second call into a loop. The measured price
/// with the pool off was 2.96x on `fib(38)` — a program paying for
/// parallelism it never activated. No rearrangement of one lowering can serve
/// both: the phi is what actualization *is*, and the transform requires its
/// absence. So the module carries both lowerings and selects between them.
///
/// **Why the selection is safe.** The default policy selects once per process,
/// from whether the run asked for a pool. Without one every acquisition is refused for
/// the whole process, so the two worlds compute on exactly the same schedule and
/// the choice between them is a choice of machine code, not of semantics.
/// On every maintained target, partial startup retains the available workers;
/// complete startup refusal makes every acquisition return null. The query
/// still reflects the requested pool, so startup refusal does not reselect
/// the process-level clone: the parallel body's ordinary-call fallback runs.
/// A *per-task* demand signal would be a different thing
/// entirely, and was measured killing the scheduler it was meant to help: the
/// shared word it needs costs two contended read-modify-writes per task, which
/// took the fine-grain oracle cell from 0.4905 s to 0.9254 s. Nothing here reads
/// a per-task signal. A recursive component's budget-carrying variant enters
/// its clone when the count it was handed reaches zero, and the opt-in refusal
/// policy reuses an existing null branch. The budget is one process-level
/// query at a component entry, not a signal read per task. Declining descendant compute permissions
/// preserves [PAR-1] operations, arguments and the same-ABI result; the call
/// still executes at the original join.
///
/// **Why this set and not another.** A function outside it has the same body
/// in both worlds — no hand-out is reachable from it, so nothing about its
/// lowering depends on which world called it — and both worlds call the one
/// copy. Cloning it would be bytes with no reader. The set is a property of
/// the call graph and the permission judgment, never of a name or a source
/// shape.
///
/// Empty when no hand-out is reachable from any definition, including every
/// default compilation: the default build carries no overlap group at all, so
/// there is one world and this changes nothing about it.
pub(crate) fn sequential_clone_set(program: &IrProgram) -> HashSet<u32> {
    let functions = program.functions();
    let mut callees: Vec<Vec<u32>> = vec![Vec::new(); functions.len()];
    let mut hands_out = Vec::with_capacity(functions.len());
    for (ordinal, function) in functions.iter().enumerate() {
        hands_out.push(
            function
                .overlaps()
                .iter()
                .any(|overlap| !overlap.handed_out().is_empty()),
        );
        for block in function.blocks() {
            for instruction in block.instructions() {
                let IrInstruction::Define { operation, .. } = instruction else {
                    continue;
                };
                match operation {
                    IrOperation::Call { function, .. } => callees[ordinal].push(*function),
                    // A split reaches both halves: the overlapped world calls
                    // the splitter and the sequential world calls the chunk, so
                    // each world's reachability has to hold the one it uses.
                    IrOperation::LoopSplit {
                        splitter, chunk, ..
                    } => {
                        callees[ordinal].push(*splitter);
                        callees[ordinal].push(*chunk);
                    }
                    _ => {}
                }
            }
        }
    }

    // Every ordinary definition can be selected by a linker or called by
    // another unit. The module has no privileged source entry function.
    let reachable: HashSet<_> = (0..functions.len())
        .filter_map(|ordinal| u32::try_from(ordinal).ok())
        .collect();

    // The other direction: a function needs a clone when a hand-out is
    // reachable *from* it, so the walk runs the call graph backwards from the
    // functions that hand work out.
    let mut callers: Vec<Vec<u32>> = vec![Vec::new(); functions.len()];
    for (ordinal, called) in callees.iter().enumerate() {
        let Ok(ordinal) = u32::try_from(ordinal) else {
            continue;
        };
        for callee in called {
            if let Some(entry) = callers.get_mut(*callee as usize) {
                entry.push(ordinal);
            }
        }
    }
    let mut reaches_hand_out = HashSet::new();
    let mut pending: Vec<u32> = hands_out
        .iter()
        .enumerate()
        .filter(|(_, hands_out)| **hands_out)
        .filter_map(|(ordinal, _)| u32::try_from(ordinal).ok())
        .collect();
    while let Some(ordinal) = pending.pop() {
        if !reaches_hand_out.insert(ordinal) {
            continue;
        }
        if let Some(calling) = callers.get(ordinal as usize) {
            pending.extend(calling.iter().copied());
        }
    }

    // A chunk reaches no hand-out of its own, and still needs a clone: it is
    // what the sequential world calls at a split site, and giving that world
    // its own copy leaves each copy with exactly one caller, which is what lets
    // the loop be inlined back into it and makes the sequential world free.
    //
    // A splitter is the other way round. It exists only in the overlapped
    // world, so its clone would be unreachable — and would be a second caller
    // of the chunk's clone, costing exactly the inlining above.
    reachable
        .iter()
        .copied()
        .filter(|ordinal| {
            match functions
                .get(*ordinal as usize)
                .and_then(IrFunction::synthesis)
            {
                Some(IrSynthesis::Splitter) => false,
                Some(IrSynthesis::Chunk) => true,
                None => reaches_hand_out.contains(ordinal),
            }
        })
        .collect()
}

/// The outlined thunks of one module, in emission order.
#[derive(Debug, Default)]
pub(crate) struct ParallelThunks {
    definitions: String,
    count: u32,
    /// How many thunks each function's emission has registered, which
    /// numbers the next one's symbol within that function alone.
    local: std::collections::HashMap<String, u32>,
    /// Whether any emitted function asked the runtime for a split allowance, so
    /// a module that splits no loop names that symbol nowhere.
    queries_split_budget: bool,
    /// The same for the recursion budget: a module with no budgeted component
    /// names that symbol nowhere either.
    pub(super) queries_recursion_budget: bool,
}

impl ParallelThunks {
    /// The thunk definitions this module needs, or empty when it hands out
    /// nothing.
    pub(crate) fn definitions(&self) -> &str {
        &self.definitions
    }

    pub(crate) const fn is_used(&self) -> bool {
        self.count != 0
    }

    pub(crate) const fn queries_split_budget(&self) -> bool {
        self.queries_split_budget
    }

    pub(crate) const fn queries_recursion_budget(&self) -> bool {
        self.queries_recursion_budget
    }

    /// Records one thunk body of `parent`'s emission and returns the symbol
    /// that names it: `parent`'s name and the thunk's number among its own,
    /// so an unchanged function's thunks keep their symbols when another
    /// function gains or loses a hand-out [MOD-8].
    fn register(
        &mut self,
        parent: &str,
        body: impl FnOnce(&str) -> String,
    ) -> Result<String, BackendFailure> {
        let local = self.local.entry(parent.to_owned()).or_insert(0);
        let symbol = format!("@wf__par_thunk_{parent}.{local}");
        *local = local
            .checked_add(1)
            .ok_or(BackendFailure::CounterOverflow)?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or(BackendFailure::CounterOverflow)?;
        self.definitions.push_str(&body(&symbol));
        Ok(symbol)
    }
}

/// One hand-out awaiting its join, in the order the group hands them out.
#[derive(Clone, Debug)]
pub(crate) struct ComputeHandedOut {
    /// The value the joined call defines: a granted lane's result read out of
    /// the frame, or a refused lane's own call, whichever edge ran.
    result: IrValueId,
    result_abi: ResultAbi,
    /// The frame's LLVM struct type, `{ arguments..., result }`.
    frame_type: String,
    /// The acquired lane's frame, or null when no lane was granted. It is both
    /// the storage the thunk reads and the handle the join names.
    frame: String,
    /// The frame field the result occupies: the argument count.
    result_field: usize,
    /// The call the refused edge makes with the original operands. Normally
    /// the thunk's symbol; the opt-in policy may select its same-ABI clone.
    callee: String,
    arguments: String,
}

/// One ordinary call awaiting the overlap join.
pub(crate) type HandedOut = ComputeHandedOut;

impl FunctionEmitter<'_, '_> {
    /// Hands one member of an overlap group to a worker lane.
    ///
    /// Acquires a lane first and builds the frame only inside the granted edge,
    /// so a refused hand-out leaves nothing behind but a null pointer. Defines
    /// nothing: the call's value comes into existence at the join, which is
    /// the only place it is known to have been computed.
    pub(super) fn emit_handed_out_call(
        &mut self,
        result: IrValueId,
        ty: IrType,
        function: u32,
        arguments: &[IrValueId],
    ) -> Result<(), BackendFailure> {
        let target = self
            .program
            .functions()
            .get(function as usize)
            .ok_or(BackendFailure::InvalidIr)?;
        let abi = FunctionAbi::build(self.program, target)?;
        if abi.result().ty() != ty || abi.parameters().len() != arguments.len() {
            return Err(BackendFailure::InvalidIr);
        }
        let mut field_types = Vec::with_capacity(arguments.len() + 1);
        let mut operands = Vec::with_capacity(arguments.len());
        let mut call_arguments = Vec::with_capacity(arguments.len());
        for (argument, parameter) in arguments.iter().zip(abi.parameters()) {
            if self.value_type(*argument) != Some(parameter.ty()) {
                return Err(BackendFailure::InvalidIr);
            }
            let parameter_type = llvm_type(self.program, parameter.ty())?;
            let operand = self.value_operand(*argument)?;
            operands.push(format!("{parameter_type} {operand}"));
            // The frame keeps a range reference's pair whole; the refused
            // edge's own call receives it split, like every other call.
            call_arguments.push(if parameter.is_indirect() {
                let address = self.value_place(*argument)?;
                format!("ptr {address}")
            } else {
                self.value_argument(*parameter, &operand)?
            });
            field_types.push(parameter_type);
        }
        let result_type = llvm_type(self.program, ty)?;
        let result_field = field_types.len();
        field_types.push(result_type.clone());
        let frame_layout = self
            .ordinary_lane_frames
            .get(&result)
            .copied()
            .ok_or(BackendFailure::InvalidIr)?;

        // A callback that lands inside a budgeted component enters the
        // callee's variant, so the budget its caller had left travels with the
        // arguments: one more frame field, stored at the offer and read by the
        // thunk. The field follows the result, which leaves every existing
        // field at the offset it had.
        let (callee, budget) = self.callee_target(function, target.name());
        let budget = budget.map(str::to_owned);
        let budget_field = budget.as_ref().map(|_| {
            field_types.push("i64".to_owned());
            field_types.len() - 1
        });
        let frame_type = format!("{{ {} }}", field_types.join(", "));
        let thunk = self.parallel.register(self.function.name(), |symbol| {
            thunk_definition(
                symbol,
                &ThunkFrame {
                    ty: &frame_type,
                    field_types: &field_types,
                    result: result_field,
                    budget: budget_field,
                },
                &abi,
                &callee,
                &result_type,
            )
        })?;

        // Target layout already computed the exact complete aggregate before
        // this function emitted any text. Passing that proved constant avoids
        // forming an address from `null` merely to ask LLVM for the same size.
        let frame = format!("%{}", self.next_temporary()?);
        let granted = format!("%{}", self.next_temporary()?);
        let offer = par_offer_label(result);
        let offered = par_offered_label(result);
        writeln!(
            self.output,
            "  {frame} = call ptr @wf__par_acquire_lane(i64 {})\n  {granted} = icmp ne ptr {frame}, null\n  br i1 {granted}, label %{offer}, label %{offered}\n{offer}:",
            frame_layout.size()
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        for (index, operand) in operands.iter().enumerate() {
            let field = format!("%{}", self.next_temporary()?);
            writeln!(
                self.output,
                "  {field} = getelementptr inbounds {frame_type}, ptr {frame}, i32 0, i32 {index}\n  store {operand}, ptr {field}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
        }
        if let (Some(field), Some(budget)) = (budget_field, budget.as_ref()) {
            let slot = format!("%{}", self.next_temporary()?);
            writeln!(
                self.output,
                "  {slot} = getelementptr inbounds {frame_type}, ptr {frame}, i32 0, i32 {field}\n  store i64 {budget}, ptr {slot}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
        }
        writeln!(
            self.output,
            "  call void @wf__par_publish(ptr {frame}, ptr {thunk})\n  br label %{offered}\n{offered}:"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        // The refused edge runs the same call on this thread. The opt-in
        // refusal control may send it to the clone instead, which is the
        // source ABI and carries no budget.
        let refused_to_clone = self.refusal_clones.contains(&function);
        if let (false, Some(budget)) = (refused_to_clone, budget.as_ref()) {
            call_arguments.push(format!("i64 {budget}"));
        }
        self.handed_out.push(ComputeHandedOut {
            result,
            result_abi: abi.result(),
            frame_type,
            frame,
            result_field,
            callee: if refused_to_clone {
                sequential_clone_symbol(target.name())
            } else {
                callee
            },
            arguments: call_arguments.join(", "),
        });
        Ok(())
    }

    /// Renders one permitted counted loop [PAR-2 candidate] into the world
    /// being emitted.
    ///
    /// The sequential world calls the chunk — the loop itself, seeded with the
    /// accumulator's incoming value — and is therefore the code the loop always
    /// had, behind one call its only caller inlines back. The overlapped world
    /// asks the runtime once, at loop entry, how far a split of this span may
    /// descend, and calls the splitter with that allowance. Nothing is asked
    /// per iteration, and neither world tests anything the other does.
    pub(super) fn emit_loop_split(
        &mut self,
        result: IrValueId,
        ty: IrType,
        split: &LoopSplitSite<'_>,
    ) -> Result<(), BackendFailure> {
        let target = if self.sequential_clones.is_some() {
            split.chunk
        } else {
            split.splitter
        };
        let function = self
            .program
            .functions()
            .get(target as usize)
            .ok_or(BackendFailure::InvalidIr)?;
        let abi = FunctionAbi::build(self.program, function)?;
        let declared = abi.parameters();
        // The splitter alone takes one trailing allowance. The chunk and
        // splitter otherwise share the typed internal parameter/result ABI.
        let expected = declared
            .len()
            .checked_sub(usize::from(self.sequential_clones.is_none()))
            .ok_or(BackendFailure::InvalidIr)?;
        if expected != split.captures.len() + 3
            || abi.result().ty() != ty
            || declared.first().map(|parameter| parameter.ty()) != Some(ty)
            || split
                .captures
                .iter()
                .zip(declared.get(3..).unwrap_or_default())
                .any(|(capture, parameter)| self.value_type(*capture) != Some(parameter.ty()))
        {
            return Err(BackendFailure::InvalidIr);
        }
        let result_type = llvm_type(self.program, ty)?;
        let mut arguments = Vec::with_capacity(split.captures.len() + 4);
        arguments.push(if declared[0].is_indirect() {
            let address = self.value_place(split.seed)?;
            format!("ptr {address}")
        } else {
            format!("{result_type} {}", self.value_name(split.seed))
        });
        for endpoint in [split.lower, split.upper] {
            if self.value_type(endpoint) != Some(U64) {
                return Err(BackendFailure::InvalidIr);
            }
            arguments.push(format!("i64 {}", self.value_name(endpoint)));
        }
        for (capture, parameter) in split.captures.iter().zip(&declared[3..]) {
            arguments.push(if parameter.is_indirect() {
                let address = self.value_place(*capture)?;
                format!("ptr {address}")
            } else {
                let operand = self.value_name(*capture);
                self.value_argument(*parameter, &operand)?
            });
        }

        let callee = self.callee_symbol(target, function.name());
        if self.sequential_clones.is_some() {
            return self.emit_split_call(result, abi.result(), &callee, arguments);
        }

        // The span, computed so an inverted range asks for nothing rather than
        // for a bogus 2^63 one. The splitter guards the same way; this keeps a
        // wrapped width out of the allowance as well as out of the descent.
        let width = format!("%{}", self.next_temporary()?);
        let ascending = format!("%{}", self.next_temporary()?);
        let span = format!("%{}", self.next_temporary()?);
        let budget = format!("%{}", self.next_temporary()?);
        let lower = self.value_name(split.lower);
        let upper = self.value_name(split.upper);
        let weight = if let Some(work) = split.work {
            self.emit_work_estimate(work, &mut std::collections::HashMap::new())?
        } else {
            split.weight.to_string()
        };
        writeln!(
            self.output,
            "  {width} = sub i64 {upper}, {lower}\n  \
             {ascending} = icmp ugt i64 {upper}, {lower}\n  \
             {span} = select i1 {ascending}, i64 {width}, i64 0\n  \
             {budget} = call i64 @wf__par_split_budget(i64 {span}, i64 {weight})"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.parallel.queries_split_budget = true;
        arguments.push(format!("i64 {budget}"));
        self.emit_split_call(result, abi.result(), &callee, arguments)
    }

    /// Scheduling arithmetic is total even when the priced source branch
    /// would never execute. Scalars and range descriptors are already
    /// captured; a Box-array header load additionally relies on its exhibited
    /// typed read and the retained no-write fact for the original formal.
    fn emit_work_estimate(
        &mut self,
        work: &IrWorkEstimate,
        memo: &mut std::collections::HashMap<IrWorkEstimate, String>,
    ) -> Result<String, BackendFailure> {
        if let Some(value) = memo.get(work) {
            return Ok(value.clone());
        }
        let value = match work {
            IrWorkEstimate::Constant(value) => value.to_string(),
            IrWorkEstimate::Value(value) => {
                if self.value_type(*value) != Some(U64) {
                    return Err(BackendFailure::InvalidIr);
                }
                self.value_name(*value)
            }
            IrWorkEstimate::BoxArrayLength(value) => {
                let Some(IrType::Address(IrAddressed::Nominal(nominal))) = self.value_type(*value)
                else {
                    return Err(BackendFailure::InvalidIr);
                };
                let IrNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind() else {
                    return Err(BackendFailure::InvalidIr);
                };
                let block @ IrType::Buffer { element } = *referent else {
                    return Err(BackendFailure::InvalidIr);
                };
                let pointer = self.projected_address_pointer(
                    IrType::Address(IrAddressed::Buffer { element }),
                    *value,
                    &crate::IrPlaceStep::BoxReferent { nominal },
                )?;
                let address = self.aggregate_field_pointer(block, &pointer, 0)?;
                let result = format!("%{}", self.next_temporary()?);
                writeln!(self.output, "  {result} = load i64, ptr {address}")
                    .map_err(|_| BackendFailure::TextEmission)?;
                result
            }
            IrWorkEstimate::Length(value) => {
                let ty = self.value_type(*value).ok_or(BackendFailure::InvalidIr)?;
                // A runtime-capacity `Array<T>` keeps its `len` in the block
                // the owning `Box` points at [TYPE-9]; a `&[T]` range
                // reference carries its count beside its pointer [REF-4].
                if let IrType::Address(IrAddressed::Buffer { element }) = ty {
                    let block = IrType::Buffer { element };
                    let address = self.value_name(*value);
                    let length_address = self.aggregate_field_pointer(block, &address, 0)?;
                    let result = format!("%{}", self.next_temporary()?);
                    writeln!(self.output, "  {result} = load i64, ptr {length_address}")
                        .map_err(|_| BackendFailure::TextEmission)?;
                    return Ok(result);
                }
                if !matches!(ty, IrType::Range { .. }) {
                    return Err(BackendFailure::InvalidIr);
                }
                let result = format!("%{}", self.next_temporary()?);
                writeln!(
                    self.output,
                    "  {result} = extractvalue {} {}, 1",
                    llvm_type(self.program, ty)?,
                    self.value_name(*value)
                )
                .map_err(|_| BackendFailure::TextEmission)?;
                result
            }
            IrWorkEstimate::Sum(parts) => {
                let mut result = "0".to_owned();
                for part in parts {
                    let value = self.emit_work_estimate(part, memo)?;
                    result = self.emit_work_binary("llvm.uadd.sat.i64", &result, &value)?;
                }
                result
            }
            IrWorkEstimate::Difference(left, right) => {
                let left = self.emit_work_estimate(left, memo)?;
                let right = self.emit_work_estimate(right, memo)?;
                self.emit_work_binary("llvm.usub.sat.i64", &left, &right)?
            }
            IrWorkEstimate::Product(left, right) => {
                let left = self.emit_work_estimate(left, memo)?;
                let right = self.emit_work_estimate(right, memo)?;
                self.intrinsics.insert(IntrinsicDeclaration::Overflow {
                    name: "llvm.umul.with.overflow.i64".to_owned(),
                    ty: "i64".to_owned(),
                });
                let pair = self.next_temporary()?;
                let product = self.next_temporary()?;
                let overflow = self.next_temporary()?;
                let result = format!("%{}", self.next_temporary()?);
                writeln!(self.output, "  %{pair} = call {{ i64, i1 }} @llvm.umul.with.overflow.i64(i64 {left}, i64 {right})\n  %{product} = extractvalue {{ i64, i1 }} %{pair}, 0\n  %{overflow} = extractvalue {{ i64, i1 }} %{pair}, 1\n  {result} = select i1 %{overflow}, i64 -1, i64 %{product}")
                    .map_err(|_| BackendFailure::TextEmission)?;
                result
            }
            IrWorkEstimate::Quotient(value, divisor) => {
                if *divisor == 0 {
                    return Err(BackendFailure::InvalidIr);
                }
                let value = self.emit_work_estimate(value, memo)?;
                let result = format!("%{}", self.next_temporary()?);
                writeln!(self.output, "  {result} = udiv i64 {value}, {divisor}")
                    .map_err(|_| BackendFailure::TextEmission)?;
                result
            }
        };
        memo.insert(work.clone(), value.clone());
        Ok(value)
    }

    fn emit_work_binary(
        &mut self,
        intrinsic: &str,
        left: &str,
        right: &str,
    ) -> Result<String, BackendFailure> {
        self.intrinsics.insert(IntrinsicDeclaration::Binary {
            name: intrinsic.to_owned(),
            ty: "i64".to_owned(),
        });
        let result = format!("%{}", self.next_temporary()?);
        writeln!(
            self.output,
            "  {result} = call i64 @{intrinsic}(i64 {left}, i64 {right})"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(result)
    }

    fn emit_split_call(
        &mut self,
        result: IrValueId,
        result_abi: ResultAbi,
        callee: &str,
        mut arguments: Vec<String>,
    ) -> Result<(), BackendFailure> {
        let result_type = llvm_type(self.program, result_abi.ty())?;
        if result_abi.uses_destination() {
            let destination = self.value_place(result)?;
            arguments.insert(0, format!("ptr {destination}"));
            // LoopSplit uses the value-definition bridge, whose ordinary
            // result save reads this snapshot of the completed destination.
            return writeln!(
                self.output,
                "  call void @{callee}({})\n  {} = load {result_type}, ptr {destination}",
                arguments.join(", "),
                value_name(result)
            )
            .map_err(|_| BackendFailure::TextEmission);
        }
        writeln!(
            self.output,
            "  {} = call {result_type} @{callee}({})",
            value_name(result),
            arguments.join(", ")
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Completes every hand-out of the group whose last member just ran.
    ///
    /// A granted lane is waited for, read, and given back; a refused one runs
    /// the same call on this thread. Either way the group's values exist from
    /// here on, and no exit edge of the block is reachable before this point.
    /// Hand-outs are joined in reverse publication order.
    pub(super) fn emit_overlap_joins(
        &mut self,
        join_site: IrValueId,
    ) -> Result<(), BackendFailure> {
        if !self.is_overlap_join_site(join_site) {
            return Ok(());
        }
        let queue = std::mem::take(&mut self.handed_out);
        for pending in queue.into_iter().rev() {
            let condition = format!("%{}", self.next_temporary()?);
            let refused = format!("%{}", self.next_temporary()?);
            let waited = format!("%{}", self.next_temporary()?);
            let field = format!("%{}", self.next_temporary()?);
            let inline = par_inline_label(pending.result);
            let wait = par_wait_label(pending.result);
            let done = par_done_label(pending.result);
            let result_type = llvm_type(self.program, pending.result_abi.ty())?;
            let ComputeHandedOut {
                frame,
                frame_type,
                callee,
                arguments,
                result_field,
                ..
            } = &pending;
            let (inline_call, save_waited) = if pending.result_abi.uses_destination() {
                let destination = self.value_place(pending.result)?;
                let arguments = if arguments.is_empty() {
                    format!("ptr {destination}")
                } else {
                    format!("ptr {destination}, {arguments}")
                };
                (
                    format!(
                        "call void @{callee}({arguments})\n  {refused} = load {result_type}, ptr {destination}"
                    ),
                    format!("store {result_type} {waited}, ptr {destination}\n  "),
                )
            } else {
                (
                    format!("{refused} = call {result_type} @{callee}({arguments})"),
                    String::new(),
                )
            };
            writeln!(
                self.output,
                "  {condition} = icmp eq ptr {frame}, null\n  \
                 br i1 {condition}, label %{inline}, label %{wait}\n\
                 {inline}:\n  {inline_call}\n  \
                 br label %{done}\n\
                 {wait}:\n  call void @wf__par_join(ptr {frame})\n  \
                 {field} = getelementptr inbounds {frame_type}, ptr {frame}, i32 0, i32 {result_field}\n  \
                 {waited} = load {result_type}, ptr {field}\n  \
                 {save_waited}call void @wf__par_release(ptr {frame})\n  br label %{done}\n\
                 {done}:\n  {} = phi {result_type} [ {refused}, %{inline} ], [ {waited}, %{wait} ]",
                value_name(pending.result),
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            self.save_value_result(pending.result)?;
        }
        Ok(())
    }
}

/// The lane frame one hand-out fills, as its thunk reads it back: the LLVM
/// struct type, its field types in order, and which fields are not arguments.
struct ThunkFrame<'site> {
    ty: &'site str,
    field_types: &'site [String],
    /// The field the result is left in: the argument count.
    result: usize,
    /// The field carrying the callee variant's budget, where the callback
    /// lands inside a budgeted component.
    budget: Option<usize>,
}

/// One outlined call over its frame.
fn thunk_definition(
    symbol: &str,
    frame: &ThunkFrame<'_>,
    abi: &FunctionAbi,
    callee: &str,
    result_type: &str,
) -> String {
    let ThunkFrame {
        ty: frame_type,
        field_types,
        result: result_field,
        budget: budget_field,
    } = *frame;
    let mut body = format!("define internal void {symbol}(ptr %frame) {{\nentry:\n");
    let mut rendered = Vec::with_capacity(field_types.len() - 1);
    for (index, (field_type, parameter)) in field_types.iter().zip(abi.parameters()).enumerate() {
        let _ = writeln!(
            body,
            "  %p{index} = getelementptr inbounds {frame_type}, ptr %frame, i32 0, i32 {index}"
        );
        if parameter.is_indirect() {
            // The field still owns the complete argument payload. The callee
            // snapshots this content into its own activation before mutation.
            rendered.push(format!("ptr %p{index}"));
        } else if parameter.is_range() {
            // A range reference's pair crosses the call as its element
            // pointer and count, the same split every call route passes.
            let _ = writeln!(
                body,
                "  %a{index} = load {field_type}, ptr %p{index}\n  \
                 %a{index}.data = extractvalue {field_type} %a{index}, 0\n  \
                 %a{index}.len = extractvalue {field_type} %a{index}, 1"
            );
            rendered.push(format!("ptr %a{index}.data, i64 %a{index}.len"));
        } else {
            let _ = writeln!(body, "  %a{index} = load {field_type}, ptr %p{index}");
            rendered.push(format!("{field_type} %a{index}"));
        }
    }
    // The budget the offering activation had left, where this callback lands
    // in a budget-carrying variant: an ordinary trailing argument, read out of
    // the frame like every other one.
    if let Some(index) = budget_field {
        let _ = writeln!(
            body,
            "  %pb = getelementptr inbounds {frame_type}, ptr %frame, i32 0, i32 {index}\n  %ab = load i64, ptr %pb"
        );
        rendered.push("i64 %ab".to_owned());
    }
    let field = result_field;
    if abi.result().uses_destination() {
        // Construct into the same result field the existing join path reads.
        // No pointer to worker-local or released storage becomes the result.
        let _ = write!(
            body,
            "  %slot = getelementptr inbounds {frame_type}, ptr %frame, i32 0, i32 {field}\n  call void @{callee}(ptr %slot{}{})\n  ret void\n}}\n\n",
            if rendered.is_empty() { "" } else { ", " },
            rendered.join(", ")
        );
        return body;
    }
    let _ = write!(
        body,
        "  %result = call {result_type} @{callee}({})\n  %slot = getelementptr inbounds {frame_type}, ptr %frame, i32 0, i32 {field}\n  store {result_type} %result, ptr %slot\n  ret void\n}}\n\n",
        rendered.join(", ")
    );
    body
}

/// The label a granted lane's frame is filled and published in.
fn par_offer_label(value: IrValueId) -> String {
    format!("par.offer.v{}", value.ordinal())
}

/// The label both edges of lane acquisition continue in, and so the block the inline
/// member of the group runs in.
fn par_offered_label(value: IrValueId) -> String {
    format!("par.offered.v{}", value.ordinal())
}

/// The label a refused lane runs the call in.
fn par_inline_label(value: IrValueId) -> String {
    format!("par.inline.v{}", value.ordinal())
}

/// The label a granted lane is waited for in.
fn par_wait_label(value: IrValueId) -> String {
    format!("par.wait.v{}", value.ordinal())
}

/// The label the joined value is read in. It is the block a later phi names as
/// its predecessor when the join is the last split of its block.
pub(super) fn par_done_label(value: IrValueId) -> String {
    format!("par.done.v{}", value.ordinal())
}

// The former mixed completion/worker join-order assertions are retired by
// v0.55's deletion of PAR-3 and direct completion handouts. Every ordinary
// worker group now joins in reverse publication order in emit_overlap_joins.
