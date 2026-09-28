//! Actualization of a permitted counted loop [PAR-2 candidate]: the loop
//! becomes a recursive split of its own index range.
//!
//! # What is emitted
//!
//! For either a reduction
//! `let a = INIT; for @l (i in lo..hi) { … set a = a (+) e … }` or an
//! independently proved map `for @l (i in lo..hi) { set out[i] = e; }`, this
//! builds two synthesized functions and replaces the loop site with one
//! instruction:
//!
//! ```text
//! chunk(seed, lo, hi, captures…)   the loop itself; reduction seed/result is
//!                                  the accumulator, map seed/result is Unit
//! split(seed, lo, hi, captures…, budget)
//!                                  hi <= lo         -> seed
//!                                  budget 0 or thin -> chunk(seed, lo, hi, …)
//!                                  otherwise        -> reduction: split left
//!                                                      (+) split right
//!                                                   -> map: split both, join,
//!                                                      return Unit
//! ```
//!
//! and at the site, one call: `split(seed, lo, hi, captures…, allowance)` in
//! the overlapped world, `chunk(seed, lo, hi, captures…)` in the sequential
//! one. A reduction's incoming accumulator folds into the leftmost chunk
//! rather than being recombined afterwards, so the fold's leaf order is the
//! source's own. A map's Unit return is ignored; the join makes its captured
//! stores complete. In both forms the sequential world's call is the loop
//! exactly.
//!
//! # Why the leaf is the loop and never one iteration
//!
//! Splitting to width one is the shape a hand-written recursion has, and
//! emitting it from a loop is a pessimization no worker count recovers: it
//! costs the body the vectorization and unrolling the loop gave it, and pays
//! `2N-1` activations for `N` iterations. Measured against the plain loop on
//! this machine: 7.6x slower on a two-operation body, 3.6x on an
//! eight-operation one, at every worker count from zero to eight. Bottoming
//! out in the loop over a subrange keeps the body's own optimization inside
//! the chunk and measured 3.1x at ten lanes on the same probe.
//!
//! # Why the runtime chooses the grain
//!
//! Every admitted reduction combine is exactly associative on its type's
//! complete value set, so its value does not depend on the combination tree.
//! An independent map has no value to combine: its proof says distinct
//! iterations write disjoint elements, and the `Unit` result exists only to
//! carry the ordinary worker join. Those are the two reasons the *number* of
//! chunks may depend on the span, the lane count, and what this lane is already
//! doing — asked once per loop entry, never per iteration.
//!
//! [`assign_weights`] prices the emitted IR of the chunk and the functions it
//! calls. Available captured extents replace fixed inner-loop factors; unknown
//! work retains the static estimate. No program name or writer annotation
//! selects a price.
//!
//! # What declines, and loudly
//!
//! A lane frame is bounded ([`LANE_FRAME_BYTES`]), and a split whose frame exceeds
//! it would have every lane acquisition refused forever: the program would pay the
//! splitter's calls and never overlap anything. That is exactly the silent
//! sequentialization the ledger exists to prevent, so the frame is measured
//! here, at compile time, and a loop that does not fit declines with a line
//! naming the width. Every decline in [`Decline`] is reported the same way.

use std::{cell::RefCell, collections::HashMap};

use crate::semantic::{
    BindingId, CheckedDrop, CheckedLoopId, CheckedStatement, LoopActualization, LoopCombine,
    LoopPermission,
};
use crate::target::{LANE_FRAME_BYTES, parallel_lane_frame_layout};
use crate::{
    IrAddressed, IrBlock, IrBlockId, IrBooleanOperation, IrConstant, IrEnumType, IrFunction,
    IrInstruction, IrIntegerOperation, IrMatchTarget, IrNominalKind, IrOperation, IrOverlap,
    IrSynthesis, IrTerminator, IrType, IrValueId, LoweringFailure, NodePath,
};

use super::loops::U64;
use super::{BuildingBlock, IrBuilder};

/// The widest scalar the backend puts in a frame, and so the alignment every
/// frame field is conservatively charged.
const FRAME_FIELD_ALIGN: u64 = 8;

/// The type every shift amount carries in this IR.
const SHIFT_AMOUNT: IrType = IrType::Integer {
    width: 32,
    signed: false,
};

/// Why one permitted loop was not actualized.
///
/// Each variant is a property of the *emitted shape*, never of the judgment:
/// the permission holds in every one of them, and a later version that emits a
/// wider shape actualizes them without the rule moving.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Decline {
    /// The lane frame the splitter would need exceeds [`LANE_FRAME_BYTES`], so
    /// every lane acquisition would be refused and the split would never overlap.
    FrameTooWide { bytes: u64, captures: usize },
    /// The accumulator carries stable storage, so its value at the site is an
    /// address into this activation's frame rather than the value to fold.
    AccumulatorAddressed,
    /// The accumulator's type has no identity element under the combine, so
    /// there is nothing for an empty subrange to fold to. The admitted set has
    /// one for every integer width and for `Bool`; this closes the arm rather
    /// than assuming it is unreachable.
    NoIdentity,
}

/// How a synthesized chunk restores the binding representation used by the
/// ordinary statement lowering from the value carried in its task frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaptureReconstruction {
    Direct {
        readonly_reference: bool,
    },
    BoxSlot {
        referent: IrAddressed,
    },
    RuntimeBoxPayload {
        nominal: crate::IrNominalId,
        referent: IrAddressed,
    },
}

/// One binding's task ABI and the inverse used by the ordinary loop lowering.
/// Planning a capture does not load or project its source value.
struct Capture {
    binding: BindingId,
    ty: IrType,
    reconstruction: CaptureReconstruction,
}

impl Decline {
    fn reason(self) -> String {
        match self {
            Self::FrameTooWide { bytes, captures } => format!(
                "the lane frame does not fit its {LANE_FRAME_BYTES}-byte runtime slot; the conservative estimate is {bytes} bytes over {captures} captured bindings, so no lane could ever be granted"
            ),
            Self::AccumulatorAddressed => {
                "the accumulator is borrowed, so its site value is an address rather than the value to fold".to_owned()
            }
            Self::NoIdentity => {
                "the accumulator's type has no identity element under its combine".to_owned()
            }
        }
    }
}

/// The synthesized functions of one lowering, and the ledger of what it did
/// with each permitted loop.
///
/// Ordinals are handed out before a function is built, because the splitter
/// calls itself: a reservation returns the ordinal and the finished body is
/// filed under it afterwards. The cell is shared rather than threaded through
/// every builder method because a permitted loop can contain another one, and a
/// chunk is built by an inner builder while its outer builder is still live.
#[derive(Debug, Default)]
pub(crate) struct Synthesis {
    /// Where the synthesized functions start: the source function count, since
    /// they are appended after every source function.
    base: u32,
    functions: Vec<Option<IrFunction>>,
    /// How many functions each source function's splits have synthesized,
    /// which numbers the next one's symbol within that function alone.
    local: HashMap<String, u32>,
    ledger: Vec<String>,
    /// Observe construction, including work a later refusal used to discard.
    #[cfg(test)]
    pub(super) candidate_constructions: usize,
}

impl Synthesis {
    pub(crate) fn new(base: u32) -> Self {
        Self {
            base,
            functions: Vec::new(),
            local: HashMap::new(),
            ledger: Vec::new(),
            #[cfg(test)]
            candidate_constructions: 0,
        }
    }

    /// Reserves one synthesized function of `parent`'s splits: its ordinal,
    /// and the stable part of its symbol, which numbers it among `parent`'s
    /// own, so that an unchanged function's helpers keep their symbols when
    /// another function gains or loses a split [MOD-8].
    fn reserve(&mut self, parent: &str) -> Result<(u32, String), LoweringFailure> {
        let ordinal = u32::try_from(self.functions.len())
            .ok()
            .and_then(|offset| self.base.checked_add(offset))
            .ok_or(LoweringFailure::CounterOverflow)?;
        let local = self.local.entry(parent.to_owned()).or_insert(0);
        let name = format!("{parent}.{local}");
        *local = local
            .checked_add(1)
            .ok_or(LoweringFailure::CounterOverflow)?;
        self.functions.push(None);
        Ok((ordinal, name))
    }

    fn file(&mut self, ordinal: u32, function: IrFunction) -> Result<(), LoweringFailure> {
        let slot = ordinal
            .checked_sub(self.base)
            .and_then(|offset| self.functions.get_mut(offset as usize))
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        if slot.replace(function).is_some() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(())
    }

    /// The synthesized functions in ordinal order, and the ledger lines.
    pub(crate) fn finish(self) -> Result<(Vec<IrFunction>, Vec<String>), LoweringFailure> {
        let functions = self
            .functions
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        Ok((functions, self.ledger))
    }
}

/// A completed candidate can become either an outlined chunk or the ordinary
/// CFG at its source site. The extra identities are construction metadata;
/// they are consumed here and never become a second executable IR.
struct BuiltChunk {
    function: IrFunction,
    needed: Vec<bool>,
    binding_roots: HashMap<BindingId, IrValueId>,
    reconstructions: Vec<IrValueId>,
    call_results: HashMap<NodePath, (IrBlockId, IrValueId)>,
}

impl IrBuilder<'_> {
    /// Lowers one split candidate, or leaves the ordinary path to its caller.
    ///
    /// A candidate whose reduced frame is too wide reuses its completed CFG
    /// at the source site. Only a refusal before body construction returns
    /// `false`, leaving the caller to build the ordinary graph once.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn split_counted_range(
        &mut self,
        id: CheckedLoopId,
        node_path: &NodePath,
        binder: BindingId,
        body: &[CheckedStatement],
        backedge_drops: &[CheckedDrop],
        lower: IrValueId,
        upper: IrValueId,
    ) -> Result<bool, LoweringFailure> {
        let Some(actualization) = self.permitted_loop(node_path) else {
            return Ok(false);
        };
        let accumulator = match actualization {
            LoopActualization::IndependentMap => None,
            LoopActualization::Reduction { accumulator, .. } => Some(accumulator),
        };
        if accumulator.is_some_and(|binding| self.addressed_bindings.contains(&binding)) {
            self.note(
                node_path,
                &format!("declined: {}", Decline::AccumulatorAddressed.reason()),
            );
            return Ok(false);
        }
        let reduction_seed = accumulator
            .map(|binding| self.binding_value(binding))
            .transpose()?;
        let result_type = reduction_seed
            .map(|seed| self.value_type(seed))
            .transpose()?
            .unwrap_or(IrType::Unit);
        if let LoopActualization::Reduction { combine, .. } = actualization
            && identity(combine, result_type).is_none()
        {
            self.note(
                node_path,
                &format!("declined: {}", Decline::NoIdentity.reason()),
            );
            return Ok(false);
        }

        // Preserve the original capture interface when it already fits. A
        // wider scope gets one completed candidate whose runtime uses can
        // rescue its frame, including generated cleanup and nested splits.
        let mut bindings: Vec<BindingId> = self
            .bindings
            .keys()
            .copied()
            .filter(|binding| Some(*binding) != accumulator)
            .collect();
        bindings.sort_by_key(|binding| binding.0);
        let mut captures = Vec::with_capacity(bindings.len());
        for binding in bindings {
            let stored = self
                .bindings
                .get(&binding)
                .copied()
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
            let stored_type = self.value_type(stored)?;
            // A promoted Box is one pointer in a stable local slot. Snapshot
            // that pointer, encoding a runtime Array by its element base.
            // PAR-2 excludes whole-owner replacement or consumption by the
            // iterations, and the structured join preserves the owners they
            // use. The chunk reverses the Array projection before rebuilding
            // borrowed local owner storage; it acquires no cleanup authority.
            // Other Box kinds retain the block pointer, and inline affine
            // aggregates retain their address form. The rescue path removes
            // unused bindings before any parent load or projection is emitted.
            let local_slot = if self.addressed_bindings.contains(&binding)
                && let IrType::Address(referent @ IrAddressed::Nominal(nominal)) = stored_type
                && matches!(
                    &self.nominals[nominal.index()].kind,
                    IrNominalKind::Box { .. }
                ) {
                Some(referent)
            } else {
                None
            };
            let (ty, reconstruction) = match local_slot {
                Some(referent @ IrAddressed::Nominal(nominal)) => {
                    if matches!(
                        &self.nominals[nominal.index()].kind,
                        IrNominalKind::Box {
                            referent: IrType::Buffer { .. },
                            ..
                        }
                    ) {
                        (
                            IrType::RuntimeBoxPayload { nominal },
                            CaptureReconstruction::RuntimeBoxPayload { nominal, referent },
                        )
                    } else {
                        (referent.ty(), CaptureReconstruction::BoxSlot { referent })
                    }
                }
                Some(_) => return Err(LoweringFailure::InvalidCheckedProgram),
                None => (
                    stored_type,
                    CaptureReconstruction::Direct {
                        readonly_reference: self.readonly_reference_parameters.contains(&stored),
                    },
                ),
            };
            captures.push(Capture {
                binding,
                ty,
                reconstruction,
            });
        }

        let prune_captures = self.frame_decline(result_type, &captures)?.is_some();
        // Preserve the established preorder names and complete fitting ABI.
        // Only a still-undecided wide frame delays its helper reservation.
        let reserved = if prune_captures {
            None
        } else {
            let mut synthesis = self.synthesis.borrow_mut();
            Some((
                synthesis.reserve(self.function_name)?,
                synthesis.reserve(self.function_name)?,
            ))
        };
        let ledger_start = self.synthesis.borrow().ledger.len();
        let mut candidate = self.build_chunk(
            id,
            binder,
            body,
            backedge_drops,
            actualization,
            result_type,
            &captures,
            prune_captures,
        )?;
        captures = captures
            .into_iter()
            .zip(candidate.needed.iter().copied())
            .filter_map(|(capture, needed)| needed.then_some(capture))
            .collect();
        if let Some(decline) = self.frame_decline(result_type, &captures)?
            && parallel_lane_frame_layout(
                self.target,
                self.nominals,
                self.elements,
                [result_type, U64, U64]
                    .into_iter()
                    .chain(captures.iter().map(|capture| capture.ty))
                    .chain([U64]),
                result_type,
                // The splitter's allowance is already its final parameter.
                false,
            )?
            .is_none()
        {
            self.note(node_path, &format!("declined: {}", decline.reason()));
            // Keep the ordinary ledger order: this refusal precedes the
            // nested decisions that remain in its reused body.
            self.synthesis.borrow_mut().ledger[ledger_start..].rotate_right(1);
            self.splice_chunk(candidate, reduction_seed, lower, upper, accumulator)?;
            return Ok(true);
        }

        // A chunk does not call itself. Delay the enclosing pair's ordinals
        // until it fits, retaining every nested helper without reservation
        // holes or a function-ordinal relocation pass on refusal.
        let ((splitter, splitter_name), (chunk, chunk_name)) = match reserved {
            Some(pair) => pair,
            None => {
                let mut synthesis = self.synthesis.borrow_mut();
                (
                    synthesis.reserve(self.function_name)?,
                    synthesis.reserve(self.function_name)?,
                )
            }
        };
        candidate.function.name = chunk_symbol(&chunk_name);
        let capture_types = captures
            .iter()
            .map(|capture| capture.ty)
            .collect::<Vec<_>>();
        let mut capture_values = Vec::with_capacity(captures.len());
        for capture in &captures {
            let stored = self.bindings[&capture.binding];
            let value = match capture.reconstruction {
                CaptureReconstruction::Direct { .. } => stored,
                CaptureReconstruction::BoxSlot { .. } => self.load_storage_value(stored)?,
                CaptureReconstruction::RuntimeBoxPayload { nominal, .. } => {
                    let owner = self.load_storage_value(stored)?;
                    self.define(
                        capture.ty,
                        IrOperation::RuntimeBoxPayload { nominal, owner },
                    )?
                }
            };
            capture_values.push(value);
        }
        // The fitting path creates its map token here; reductions reuse seed.
        let seed = match reduction_seed {
            Some(seed) => seed,
            None => self.define(IrType::Unit, IrOperation::Constant(IrConstant::Unit))?,
        };
        let splitter_function = self.build_splitter(
            (splitter, &splitter_name),
            chunk,
            actualization,
            result_type,
            &capture_types,
        )?;
        {
            let mut synthesis = self.synthesis.borrow_mut();
            synthesis.file(chunk, candidate.function)?;
            synthesis.file(splitter, splitter_function)?;
        }

        let result = self.define(
            result_type,
            IrOperation::LoopSplit {
                splitter,
                chunk,
                seed,
                lower,
                upper,
                captures: capture_values,
                // Filled in once every function exists: the estimate reads the
                // chunk's own emitted IR and the IR of what it calls.
                weight: 0,
                work: None,
            },
        )?;
        if let LoopActualization::Reduction { accumulator, .. } = actualization {
            self.bindings.insert(accumulator, result);
        }
        let captured = if captures.len() == 1 {
            "binding"
        } else {
            "bindings"
        };
        let detail = match actualization {
            LoopActualization::IndependentMap => format!(
                "split independent map over {} captured {captured}",
                captures.len()
            ),
            LoopActualization::Reduction { combine, .. } => format!(
                "split under {} over {} captured {captured}",
                combine.spelling(),
                captures.len()
            ),
        };
        self.note(node_path, &detail);
        Ok(true)
    }

    /// The actualization payload of the permitted loop at this statement, when
    /// this compilation asked for overlap lowering at all.
    fn permitted_loop(&self, node_path: &NodePath) -> Option<LoopActualization> {
        if self.overlap != crate::OverlapLowering::On {
            return None;
        }
        self.permissions?
            .loops
            .iter()
            .find(|judged: &&LoopPermission| judged.statement == *node_path)
            .and_then(|judged| judged.actualization)
    }

    /// The emission conditions, all of them properties of the shape rather than
    /// of the permission.
    fn frame_decline(
        &self,
        result_type: IrType,
        captures: &[Capture],
    ) -> Result<Option<Decline>, LoweringFailure> {
        // `{ seed, lo, hi, captures…, budget, result }`, each field charged its
        // own size rounded up to the widest scalar the backend puts in a frame.
        // Keep this estimate as the established capture-pruning and helper-
        // reservation boundary. A final estimated refusal consults the shared
        // selected-target layout before declining the completed candidate.
        let mut bytes = 3 * FRAME_FIELD_ALIGN + 2 * frame_bytes(result_type);
        for capture in captures {
            let ty = self
                .bindings
                .get(&capture.binding)
                .copied()
                .ok_or(LoweringFailure::InvalidCheckedProgram)
                .and_then(|value| self.value_type(value))?;
            bytes = bytes.saturating_add(frame_bytes(ty));
        }
        Ok((bytes > LANE_FRAME_BYTES).then_some(Decline::FrameTooWide {
            bytes,
            captures: captures.len(),
        }))
    }

    /// One actualization ledger line.
    fn note(&self, node_path: &NodePath, detail: &str) {
        let path = node_path
            .components()
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".");
        self.synthesis.borrow_mut().ledger.push(format!(
            "PAR split       {}  loop at {path}  {detail}",
            self.function_name
        ));
    }

    /// The loop itself, outlined. A reduction chunk folds the half-open
    /// subrange starting from `seed`; an independent-map chunk performs its
    /// disjoint stores and returns the `Unit` seed as a synchronization token.
    ///
    /// This starts with the block graph [`IrBuilder::counted_range_graph`]
    /// builds at an unsplit site, using the same code and statements. Endpoints
    /// and the accumulator come from parameters. An originally wide frame
    /// removes unused capture forwarding without changing any body operation;
    /// a fitting frame retains its existing interface and graph.
    #[allow(clippy::too_many_arguments)]
    fn build_chunk(
        &self,
        id: CheckedLoopId,
        binder: BindingId,
        body: &[CheckedStatement],
        backedge_drops: &[CheckedDrop],
        actualization: LoopActualization,
        result_type: IrType,
        captures: &[Capture],
        prune_captures: bool,
    ) -> Result<BuiltChunk, LoweringFailure> {
        #[cfg(test)]
        {
            self.synthesis.borrow_mut().candidate_constructions += 1;
        }
        let mut builder = IrBuilder::new(
            self.context(),
            result_type,
            self.addressed_bindings.clone(),
            self.permissions,
            self.overlap,
            self.function_name,
        )?;
        let seed = builder.new_parameter(result_type)?;
        let lower = builder.new_parameter(U64)?;
        let upper = builder.new_parameter(U64)?;
        if let LoopActualization::Reduction { accumulator, .. } = actualization
            && builder.bindings.insert(accumulator, seed).is_some()
        {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        for capture in captures {
            let value = builder.new_parameter(capture.ty)?;
            let value = match &capture.reconstruction {
                CaptureReconstruction::Direct { readonly_reference } => {
                    if *readonly_reference {
                        builder.readonly_reference_parameters.push(value);
                    }
                    value
                }
                CaptureReconstruction::BoxSlot { referent } => {
                    if referent.ty() != capture.ty {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    }
                    builder.define(
                        IrType::Address(*referent),
                        IrOperation::AddressOf {
                            value,
                            referent: *referent,
                        },
                    )?
                }
                CaptureReconstruction::RuntimeBoxPayload { nominal, referent } => {
                    if capture.ty != (IrType::RuntimeBoxPayload { nominal: *nominal })
                        || referent.ty() != IrType::Nominal(*nominal)
                    {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    }
                    let owner = builder.define(
                        IrType::Nominal(*nominal),
                        IrOperation::RuntimeBoxOwner {
                            nominal: *nominal,
                            payload: value,
                        },
                    )?;
                    builder.define(
                        IrType::Address(*referent),
                        IrOperation::AddressOf {
                            value: owner,
                            referent: *referent,
                        },
                    )?
                }
            };
            if builder.bindings.insert(capture.binding, value).is_some() {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
        }
        let binding_roots = builder.bindings.clone();
        let reconstructions = builder.blocks[0]
            .instructions
            .iter()
            .map(|instruction| match instruction {
                IrInstruction::Define { result, .. } => Ok(*result),
                _ => Err(LoweringFailure::InvalidCheckedProgram),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let reconstruction_count = reconstructions.len();
        // No give target and no enclosing loop: condition 4 refused every edge
        // that could leave this loop, so a body reaching for one is a
        // malformed checked program rather than a shape this declines on.
        builder.counted_range_graph(id, binder, body, backedge_drops, None, lower, upper)?;
        let result = match actualization {
            LoopActualization::IndependentMap => seed,
            LoopActualization::Reduction { accumulator, .. } => {
                builder.binding_value(accumulator)?
            }
        };
        builder.terminate(IrTerminator::Return {
            value: result,
            drops: Vec::new(),
        })?;
        // A [PAR-1] pair inside the loop body is permitted exactly as it was
        // before the loop was split, and its sites are in this body. Reading
        // the same table here is what keeps splitting a loop from silently
        // costing the parallelism the window judgment already granted inside
        // it; a group whose members do not all land in this body resolves to
        // nothing, which is the narrowing `overlaps` already performs.
        let overlaps = builder.overlaps();
        let call_results = std::mem::take(&mut builder.call_results);
        let mut function = builder.finish(String::new(), overlaps, Some(IrSynthesis::Chunk))?;
        let needed = if prune_captures {
            prune_capture_parameters(&mut function, reconstruction_count)?
        } else {
            vec![true; captures.len()]
        };
        Ok(BuiltChunk {
            function,
            needed,
            binding_roots,
            reconstructions,
            call_results,
        })
    }

    /// Reuse a refused candidate in the current activation. Its source body
    /// has already been lowered, including nested candidates. PAR-2 excludes
    /// exits to the enclosing source context, so the chunk's return is the
    /// only interface that needs reconnecting to the parent continuation.
    fn splice_chunk(
        &mut self,
        candidate: BuiltChunk,
        seed: Option<IrValueId>,
        lower: IrValueId,
        upper: IrValueId,
        accumulator: Option<BindingId>,
    ) -> Result<(), LoweringFailure> {
        let BuiltChunk {
            function,
            binding_roots,
            reconstructions,
            call_results,
            ..
        } = candidate;
        let seed = match seed {
            Some(seed) => seed,
            None => self.define(IrType::Unit, IrOperation::Constant(IrConstant::Unit))?,
        };
        let mut values = function
            .values
            .iter()
            .map(|ty| self.new_value(*ty))
            .collect::<Result<Vec<_>, _>>()?;
        for ((parameter, _), actual) in function.parameters[..3].iter().zip([seed, lower, upper]) {
            values[parameter.index()] = actual;
        }
        for (binding, root) in binding_roots {
            values[root.index()] = self.bindings[&binding];
        }
        let value = |original: IrValueId| values[original.index()];
        let mut reconstruction = vec![false; values.len()];
        for original in reconstructions {
            reconstruction[original.index()] = true;
        }
        let (continuation, results) = self.new_block(&[function.result])?;
        let block_offset = self.blocks.len();
        let blocks = (0..function.blocks.len())
            .map(|index| {
                IrBlockId::from_index(block_offset + index).ok_or(LoweringFailure::CounterOverflow)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let block = |original: IrBlockId| blocks[original.index()];
        for mut original in function.blocks {
            // Bind a reconstructed capture's root directly to the original
            // parent slot. No Box snapshot, inverse projection or duplicate
            // local owner slot is needed by the ordinary fallback.
            original.instructions.retain(|instruction| {
                !matches!(instruction, IrInstruction::Define { result, .. }
                    if reconstruction[result.index()])
            });
            for (parameter, _) in &mut original.parameters {
                *parameter = value(*parameter);
            }
            for instruction in &mut original.instructions {
                if let IrInstruction::Define { result, .. } = instruction {
                    *result = value(*result);
                }
                instruction.remap_operands(value);
            }
            original.terminator.remap_operands(value);
            let terminator = match original.terminator {
                IrTerminator::Jump {
                    target,
                    arguments,
                    drops,
                } => IrTerminator::Jump {
                    target: block(target),
                    arguments,
                    drops,
                },
                IrTerminator::Match {
                    scrutinee,
                    enum_type,
                    mut targets,
                } => {
                    for target in &mut targets {
                        target.block = block(target.block);
                    }
                    IrTerminator::Match {
                        scrutinee,
                        enum_type,
                        targets,
                    }
                }
                IrTerminator::Return { value, drops } => IrTerminator::Jump {
                    target: continuation,
                    arguments: vec![value],
                    drops,
                },
                IrTerminator::Unreachable => IrTerminator::Unreachable,
            };
            self.blocks.push(BuildingBlock {
                parameters: original.parameters,
                instructions: original.instructions,
                terminator: Some(terminator),
            });
        }
        for mut source_call in function.source_calls {
            source_call.result = value(source_call.result);
            self.source_calls.push(source_call);
        }
        for (path, (original_block, result)) in call_results {
            self.call_results
                .insert(path, (block(original_block), value(result)));
        }
        for mut range in function.counted_ranges {
            range.blocks = (range.blocks.start + block_offset)..(range.blocks.end + block_offset);
            range.continuation = block(range.continuation);
            range.lower = value(range.lower);
            range.upper = value(range.upper);
            self.counted_ranges.push(range);
        }
        // The parent computes overlaps from the imported call sites. It also
        // retains only its own original readonly-reference formals; candidate
        // formals and new block parameters confer no additional marker.
        // Nested LoopSplit work is still unset: assign_weights runs only once
        // all functions and their counted-range metadata have been completed.
        self.terminate(IrTerminator::Jump {
            target: blocks[0],
            arguments: Vec::new(),
            drops: Vec::new(),
        })?;
        self.current = Some(continuation);
        if let Some(accumulator) = accumulator {
            self.bindings.insert(accumulator, results[0]);
        }
        Ok(())
    }

    /// The recursive range splitter, whose two halves are one ordinary overlap
    /// group.
    ///
    /// The group here is produced by this lowering, not judged: it actualizes
    /// the loop's own permission, and there is no pair of source statements for
    /// the window judgment to have looked at. Everything downstream — the
    /// lane acquisition, the frame, the thunk, the deque, and the join — is
    /// the machinery a permitted pair already uses, unchanged.
    fn build_splitter(
        &self,
        (ordinal, name): (u32, &str),
        chunk: u32,
        actualization: LoopActualization,
        result_type: IrType,
        capture_types: &[IrType],
    ) -> Result<IrFunction, LoweringFailure> {
        let mut builder = IrBuilder::new(
            self.context(),
            result_type,
            std::collections::HashSet::new(),
            None,
            self.overlap,
            self.function_name,
        )?;
        let seed = builder.new_parameter(result_type)?;
        let lower = builder.new_parameter(U64)?;
        let upper = builder.new_parameter(U64)?;
        let captures = capture_types
            .iter()
            .map(|ty| builder.new_parameter(*ty))
            .collect::<Result<Vec<_>, _>>()?;
        let budget = builder.new_parameter(U64)?;

        let (empty_block, _) = builder.new_block(&[])?;
        let (nonempty, _) = builder.new_block(&[])?;
        let (leaf, _) = builder.new_block(&[])?;
        let (halve, _) = builder.new_block(&[])?;

        // **`hi <= lo` before any width arithmetic.** An inverted range whose
        // width is computed first wraps to something near 2^64 and splits a
        // range that does not exist; the loop it stands for runs zero times.
        let nonempty_guard = builder.define(
            IrType::Bool,
            IrOperation::Integer {
                operation: IrIntegerOperation::Less,
                operand_type: U64,
                arguments: vec![lower, upper],
            },
        )?;
        builder.branch(nonempty_guard, nonempty, empty_block)?;

        // An empty range folds or writes nothing, so its incoming value — the
        // accumulator for a reduction, Unit for a map — arrives unchanged.
        builder.current = Some(empty_block);
        builder.terminate(IrTerminator::Return {
            value: seed,
            drops: Vec::new(),
        })?;

        // Two ways to reach the leaf: the allowance is spent, or the range is
        // too thin to halve into two nonempty halves.
        builder.current = Some(nonempty);
        let width = builder.define(
            U64,
            IrOperation::Integer {
                operation: IrIntegerOperation::SubtractWrap,
                operand_type: U64,
                arguments: vec![upper, lower],
            },
        )?;
        let one = builder.define(
            U64,
            IrOperation::Constant(IrConstant::Integer { ty: U64, bits: 1 }),
        )?;
        let zero = builder.define(
            U64,
            IrOperation::Constant(IrConstant::Integer { ty: U64, bits: 0 }),
        )?;
        let affordable = builder.define(
            IrType::Bool,
            IrOperation::Integer {
                operation: IrIntegerOperation::Less,
                operand_type: U64,
                arguments: vec![zero, budget],
            },
        )?;
        let divisible = builder.define(
            IrType::Bool,
            IrOperation::Integer {
                operation: IrIntegerOperation::Less,
                operand_type: U64,
                arguments: vec![one, width],
            },
        )?;
        let splittable = builder.define(
            IrType::Bool,
            IrOperation::Boolean {
                operation: crate::IrBooleanOperation::And,
                arguments: vec![affordable, divisible],
            },
        )?;
        builder.branch(splittable, halve, leaf)?;

        builder.current = Some(leaf);
        let mut leaf_arguments = vec![seed, lower, upper];
        leaf_arguments.extend(captures.iter().copied());
        let result = builder.define(
            result_type,
            IrOperation::Call {
                function: chunk,
                arguments: leaf_arguments,
            },
        )?;
        builder.terminate(IrTerminator::Return {
            value: result,
            drops: Vec::new(),
        })?;

        builder.current = Some(halve);
        // A shift amount is `u32` throughout the IR, so the halving carries its
        // own constant rather than reusing the `u64` one above.
        let one_bit = builder.define(
            SHIFT_AMOUNT,
            IrOperation::Constant(IrConstant::Integer {
                ty: SHIFT_AMOUNT,
                bits: 1,
            }),
        )?;
        let half = builder.define(
            U64,
            IrOperation::Integer {
                operation: IrIntegerOperation::ShiftRightWrap,
                operand_type: U64,
                arguments: vec![width, one_bit],
            },
        )?;
        let middle = builder.define(
            U64,
            IrOperation::Integer {
                operation: IrIntegerOperation::AddWrap,
                operand_type: U64,
                arguments: vec![lower, half],
            },
        )?;
        let remaining = builder.define(
            U64,
            IrOperation::Integer {
                operation: IrIntegerOperation::SubtractWrap,
                operand_type: U64,
                arguments: vec![budget, one],
            },
        )?;
        // A reduction sends the incoming accumulator left and the identity
        // right, preserving `seed, e0, e1, …` as the tree's leaf order. An
        // independent map sends the same Unit token both ways: it carries no
        // source value, but the two calls still form the ordinary overlap group
        // whose join completes every disjoint store before this helper returns.
        let right_seed = match actualization {
            LoopActualization::IndependentMap => seed,
            LoopActualization::Reduction { combine, .. } => {
                builder.identity_value(combine, result_type)?
            }
        };
        let mut left_arguments = vec![seed, lower, middle];
        left_arguments.extend(captures.iter().copied());
        left_arguments.push(remaining);
        let left = builder.define(
            result_type,
            IrOperation::Call {
                function: ordinal,
                arguments: left_arguments,
            },
        )?;
        let mut right_arguments = vec![right_seed, middle, upper];
        right_arguments.extend(captures.iter().copied());
        right_arguments.push(remaining);
        let right = builder.define(
            result_type,
            IrOperation::Call {
                function: ordinal,
                arguments: right_arguments,
            },
        )?;
        // Left before right, always. A reduction combines in source order; a
        // map only needs the join and may return either identical Unit token.
        let result = match actualization {
            LoopActualization::IndependentMap => right,
            LoopActualization::Reduction { combine, .. } => {
                builder.combine_values(combine, result_type, left, right)?
            }
        };
        builder.terminate(IrTerminator::Return {
            value: result,
            drops: Vec::new(),
        })?;

        builder.finish(
            splitter_symbol(name),
            vec![IrOverlap {
                members: vec![left, right],
            }],
            Some(IrSynthesis::Splitter),
        )
    }

    /// Materializes the combine's identity element.
    fn identity_value(
        &mut self,
        combine: LoopCombine,
        ty: IrType,
    ) -> Result<IrValueId, LoweringFailure> {
        let identity = identity(combine, ty).ok_or(LoweringFailure::InvalidCheckedProgram)?;
        self.define(ty, IrOperation::Constant(identity))
    }

    /// One application of the admitted combine.
    fn combine_values(
        &mut self,
        combine: LoopCombine,
        ty: IrType,
        left: IrValueId,
        right: IrValueId,
    ) -> Result<IrValueId, LoweringFailure> {
        let operation = match operation(combine) {
            Ok(operation) => IrOperation::Integer {
                operation,
                operand_type: ty,
                arguments: vec![left, right],
            },
            Err(operation) => IrOperation::Boolean {
                operation,
                arguments: vec![left, right],
            },
        };
        self.define(ty, operation)
    }

    /// A two-way branch on a `Bool`, the shape every guard in this module uses.
    fn branch(
        &mut self,
        condition: IrValueId,
        when_true: IrBlockId,
        when_false: IrBlockId,
    ) -> Result<(), LoweringFailure> {
        self.terminate(IrTerminator::Match {
            scrutinee: condition,
            enum_type: IrEnumType::Bool,
            targets: vec![
                IrMatchTarget {
                    tag: 1,
                    block: when_true,
                },
                IrMatchTarget {
                    tag: 0,
                    block: when_false,
                },
            ],
        })
    }
}

/// Removes capture-only forwarding from an outlined chunk. Every ordinary
/// instruction stays: its operands are runtime roots even when its result is
/// unused. Only the entry prefix generated to reconstruct captured Box slots
/// is removable. Cleanup subjects and returns are roots too; a jump argument
/// is needed exactly when its destination block parameter is needed.
///
/// The worklist visits each needed value once. Cyclic loop-carried forwarding
/// therefore terminates without mistaking an unobserved phi cycle for a read.
/// Value IDs stay stable for source-call, overlap and counted-range metadata.
fn prune_capture_parameters(
    function: &mut IrFunction,
    reconstruction_count: usize,
) -> Result<Vec<bool>, LoweringFailure> {
    let mut dependencies = vec![Vec::new(); function.values.len()];
    let mut pending = function.parameters[..3]
        .iter()
        .map(|(value, _)| *value)
        .collect::<Vec<_>>();
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            if block_index == 0 && instruction_index < reconstruction_count {
                let IrInstruction::Define {
                    result, operation, ..
                } = instruction
                else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                dependencies[result.index()] = operation.operands();
            } else {
                pending.extend(instruction.operands());
            }
        }
        if let IrTerminator::Jump {
            target,
            arguments,
            drops,
        } = &block.terminator
        {
            let target = function
                .blocks
                .get(target.index())
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
            if arguments.len() != target.parameters.len() {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
            for ((parameter, _), argument) in target.parameters.iter().zip(arguments) {
                dependencies[parameter.index()].push(*argument);
            }
            pending.extend(drops.iter().map(|drop| drop.operand()));
        } else {
            pending.extend(block.terminator.operands());
        }
    }
    let mut needed = vec![false; function.values.len()];
    while let Some(value) = pending.pop() {
        if !needed[value.index()] {
            needed[value.index()] = true;
            pending.extend(dependencies[value.index()].iter().copied());
        }
    }
    let captures = function.parameters[3..]
        .iter()
        .map(|(value, _)| needed[value.index()])
        .collect();
    let block_parameters = function
        .blocks
        .iter()
        .map(|block| {
            block
                .parameters
                .iter()
                .map(|(value, _)| needed[value.index()])
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    for (block_index, block) in function.blocks.iter_mut().enumerate() {
        block.parameters.retain(|(value, _)| needed[value.index()]);
        if let IrTerminator::Jump {
            target, arguments, ..
        } = &mut block.terminator
        {
            let mut keep = block_parameters[target.index()].iter();
            arguments.retain(|_| *keep.next().expect("checked jump arity"));
        }
        if block_index == 0 {
            let mut index = 0;
            block.instructions.retain(|instruction| {
                let reconstruction = index < reconstruction_count;
                index += 1;
                !reconstruction || matches!(instruction, IrInstruction::Define { result, .. } if needed[result.index()])
            });
        }
    }
    function
        .parameters
        .retain(|(value, _)| needed[value.index()]);
    function
        .readonly_reference_parameters
        .retain(|value| needed[value.index()]);
    Ok(captures)
}

/// The emitted operation of one admitted combine, as either half of the IR's
/// operation space.
const fn operation(combine: LoopCombine) -> Result<IrIntegerOperation, IrBooleanOperation> {
    match combine {
        LoopCombine::AddWrap => Ok(IrIntegerOperation::AddWrap),
        LoopCombine::MultiplyWrap => Ok(IrIntegerOperation::MultiplyWrap),
        LoopCombine::BitAnd => Ok(IrIntegerOperation::BitAnd),
        LoopCombine::BitOr => Ok(IrIntegerOperation::BitOr),
        LoopCombine::BitXor => Ok(IrIntegerOperation::BitXor),
        LoopCombine::Minimum => Ok(IrIntegerOperation::Minimum),
        LoopCombine::Maximum => Ok(IrIntegerOperation::Maximum),
        LoopCombine::And => Err(IrBooleanOperation::And),
        LoopCombine::Or => Err(IrBooleanOperation::Or),
        LoopCombine::ExclusiveOr => Err(IrBooleanOperation::ExclusiveOr),
    }
}

/// The two-sided identity of one combine at one accumulator type: the value the
/// right half of a split starts from.
///
/// `identity (+) x` and `x (+) identity` are both `x` for every entry, which is
/// what makes seeding a chunk legal at all. The table is total over the
/// admitted set, and every integer entry is that type's own extreme rather than
/// a width-independent constant, so widening the set has to extend it here.
const fn identity(combine: LoopCombine, ty: IrType) -> Option<IrConstant> {
    let (width, signed) = match ty {
        IrType::Integer { width, signed } => (width, signed),
        IrType::Bool => {
            return match combine {
                LoopCombine::And => Some(IrConstant::Bool(true)),
                LoopCombine::Or | LoopCombine::ExclusiveOr => Some(IrConstant::Bool(false)),
                _ => None,
            };
        }
        _ => return None,
    };
    if !matches!(width, 8 | 16 | 32 | 64) {
        return None;
    }
    let all_ones = if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    let sign_bit = 1_u64 << (width - 1);
    Some(IrConstant::Integer {
        ty,
        bits: match combine {
            LoopCombine::AddWrap | LoopCombine::BitOr | LoopCombine::BitXor => 0,
            LoopCombine::MultiplyWrap => 1,
            LoopCombine::BitAnd => all_ones,
            LoopCombine::Minimum if signed => sign_bit - 1,
            LoopCombine::Minimum => all_ones,
            LoopCombine::Maximum if signed => sign_bit,
            LoopCombine::Maximum => 0,
            LoopCombine::And | LoopCombine::Or | LoopCombine::ExclusiveOr => return None,
        },
    })
}

/// The symbols the two synthesized halves are emitted under, by the source
/// function they came from and their number among its helpers.
///
/// Both live in the `wf__par_` namespace [FORM-3] puts out of reach of every
/// source IDENT, so a synthesized function can never collide with a declared
/// one.
fn splitter_symbol(name: &str) -> String {
    format!("_par_split_{name}")
}

fn chunk_symbol(name: &str) -> String {
    format!("_par_chunk_{name}")
}

/// A conservative upper bound on what one value of this type costs in a lane
/// frame.
///
/// Every field is charged its own size rounded up to the widest scalar a frame
/// holds, which is what the emitted struct's own layout does for the scalar and
/// pointer types a call passes. Aggregates are charged field by field.
fn frame_bytes(ty: IrType) -> u64 {
    let raw = match ty {
        IrType::Unit | IrType::Bool => 1,
        IrType::Integer { width, .. } | IrType::Float { width } => u64::from(width).div_ceil(8),
        // A descriptor is a pointer and a length; a borrow and a box handle
        // are one pointer each.
        IrType::Buffer { .. } | IrType::Range { .. } => 2 * FRAME_FIELD_ALIGN,
        IrType::Address(_) | IrType::RuntimeBoxPayload { .. } => FRAME_FIELD_ALIGN,
        // Aggregates trigger capture selection and the final exact-layout
        // query; a conservative fit retains its established capture interface.
        IrType::Nominal(_) | IrType::Array { .. } | IrType::Window { .. } => LANE_FRAME_BYTES,
    };
    raw.div_ceil(FRAME_FIELD_ALIGN) * FRAME_FIELD_ALIGN
}

/// Fills in every [`IrOperation::LoopSplit`]'s static fallback and available
/// runtime extent estimate, once every function of the program exists.
///
/// The weight is a cost estimate over the emitted IR: the instructions of the
/// chunk, each charged more the deeper it sits inside a loop, plus the same
/// estimate for what the chunk calls, to a bounded depth. It reads no name, no
/// signature, and no source shape, and it feeds nothing but the runtime
/// allowance. The runtime estimate then substitutes available counted extents
/// through helper arguments, distinguishing a 17-element row from a
/// 1024-element row without changing either loop body. Unknown extents keep
/// this static price; no universal grain plateau is established.
pub(crate) fn assign_weights(functions: &mut [IrFunction]) {
    let costs: Vec<Cost> = functions.iter().map(cost).collect();
    let mut total: Vec<u64> = costs.iter().map(|cost| cost.instructions).collect();
    // Three rounds of substitution, so a chunk's weight sees its callees, their
    // callees, and theirs. Recursion is bounded by the same count rather than
    // by a cycle test: an estimate does not need a fixed point.
    for _ in 0..3 {
        let previous = total.clone();
        for (ordinal, weight) in total.iter_mut().enumerate() {
            let mut sum = costs[ordinal].instructions;
            for (callee, factor) in &costs[ordinal].calls {
                let called = previous.get(*callee as usize).copied().unwrap_or(0);
                sum = sum.saturating_add(called.saturating_mul(*factor));
            }
            *weight = sum;
        }
    }
    for function in functions.iter_mut() {
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                if let IrInstruction::Define {
                    operation: IrOperation::LoopSplit { chunk, weight, .. },
                    ..
                } = instruction
                {
                    // The chunk's own loop is charged [`LOOP_FACTOR`] like any
                    // other, so dividing it out turns the function's total back
                    // into what one iteration costs — which is the unit the
                    // runtime allowance multiplies the span by. A loop nested
                    // inside the body keeps its own factor, which is the whole
                    // point: its trip count is unknown and it does more work.
                    let chunk_weight = total.get(*chunk as usize).copied().unwrap_or(0);
                    *weight = (chunk_weight / LOOP_FACTOR).max(1);
                }
            }
        }
    }
    super::work::assign(functions, &total);
}

/// How much an instruction inside a loop is charged over one outside it.
///
/// A static count cannot know a trip count, and the whole point of the weight
/// is to tell a body that does real work from one that does two operations. A
/// fixed multiplier per nesting level is the smallest rule that does that; it
/// is an estimate and is stated as one.
pub(super) const LOOP_FACTOR: u64 = 16;

/// What one function costs before its callees are substituted in.
struct Cost {
    /// Its own instructions, each charged for the loops it sits in.
    instructions: u64,
    /// Every function it calls, with the charge its *call site* carries. A call
    /// inside a loop runs once per iteration, so the callee's whole cost is
    /// charged at that site's depth — which is the difference between reading a
    /// compute-heavy body as heavy and reading it as three instructions.
    calls: Vec<(u32, u64)>,
}

fn cost(function: &IrFunction) -> Cost {
    let depths = loop_depths(function.blocks());
    let mut instructions = 0_u64;
    let mut calls = Vec::new();
    for (index, block) in function.blocks().iter().enumerate() {
        let factor = LOOP_FACTOR.saturating_pow(u32::from(depths[index]).min(4));
        let count = u64::try_from(block.instructions().len()).unwrap_or(u64::MAX);
        instructions = instructions.saturating_add(count.saturating_mul(factor));
        for instruction in block.instructions() {
            let IrInstruction::Define { operation, .. } = instruction else {
                continue;
            };
            match operation {
                IrOperation::Call { function, .. } => calls.push((*function, factor)),
                // A split reaches its chunk in either world, and the splitter's
                // own descent is bounded by the allowance rather than by the
                // range, so the site is charged the chunk it will run.
                IrOperation::LoopSplit { chunk, .. } => calls.push((*chunk, factor)),
                _ => {}
            }
        }
    }
    Cost {
        instructions,
        calls,
    }
}

/// How many loops each block sits inside.
///
/// A back edge is a jump to a block that dominates the jumping block, and this
/// tests exactly that rather than approximating it: delete the target and ask
/// whether the jumping block is still reachable from the entry. Approximating
/// it by block order alone reads every `break` as a back edge — the builder
/// creates a loop's exit block before its body, so a break jumps *backwards* in
/// index — and a body with three breaks then weighs sixty-five thousand times
/// what it costs.
///
/// The loop's own extent is then approximated by the block interval, which
/// over-counts an exit block or two on a counted range. That is an estimate
/// inside an estimate and is stated as one; the back-edge test is not, because
/// getting it wrong is a factor of thousands rather than of a few instructions.
pub(super) fn loop_depths(blocks: &[IrBlock]) -> Vec<u8> {
    let mut depths = vec![0_u8; blocks.len()];
    for (index, block) in blocks.iter().enumerate() {
        let IrTerminator::Jump { target, .. } = block.terminator() else {
            continue;
        };
        if target.index() > index || reachable_without(blocks, target.index(), index) {
            continue;
        }
        for depth in &mut depths[target.index()..=index] {
            *depth = depth.saturating_add(1);
        }
    }
    depths
}

/// Whether `goal` is reachable from the entry block without passing through
/// `removed`.
fn reachable_without(blocks: &[IrBlock], removed: usize, goal: usize) -> bool {
    if removed == 0 || goal == removed {
        return false;
    }
    let mut seen = vec![false; blocks.len()];
    let mut pending = vec![0_usize];
    seen[0] = true;
    while let Some(index) = pending.pop() {
        if index == goal {
            return true;
        }
        let Some(block) = blocks.get(index) else {
            continue;
        };
        let successors: Vec<usize> = match block.terminator() {
            IrTerminator::Jump { target, .. } => vec![target.index()],
            IrTerminator::Match { targets, .. } => targets
                .iter()
                .map(|target| target.block().index())
                .collect(),
            IrTerminator::Return { .. } | IrTerminator::Unreachable => Vec::new(),
        };
        for successor in successors {
            if successor != removed
                && let Some(visited) = seen.get_mut(successor)
                && !*visited
            {
                *visited = true;
                pending.push(successor);
            }
        }
    }
    false
}

/// The shared synthesis cell one lowering threads through every builder it
/// creates.
pub(crate) type SynthesisCell = RefCell<Synthesis>;

#[cfg(test)]
mod tests {
    use super::{LoopCombine, U64, identity, operation, prune_capture_parameters};
    use crate::lowering::{IrBlockId, IrValueId};
    use crate::{
        IrAddressed, IrBlock, IrBooleanOperation, IrConstant, IrDrop, IrDropSubject, IrEnumType,
        IrFunction, IrInstruction, IrIntegerOperation, IrMatchTarget, IrOperation, IrSynthesis,
        IrTerminator, IrType,
    };

    #[test]
    fn capture_need_crosses_phi_cycles_for_calls_drops_returns_and_reconstruction() {
        let referent = IrAddressed::Integer {
            width: 64,
            signed: false,
        };
        let address = IrType::Address(referent);
        let value = IrValueId;
        let mut types = vec![U64; 20];
        for index in [3, 6, 9, 10, 11, 14, 15, 16] {
            types[index] = address;
        }
        types[19] = IrType::Bool;
        let parameters = |indices: std::ops::Range<u32>| {
            indices
                .map(|index| (value(index), types[index as usize]))
                .collect()
        };
        let mut function = IrFunction {
            name: "capture_need".into(),
            parameters: parameters(0..9),
            readonly_reference_parameters: vec![value(3), value(6)],
            source_signature: None,
            source_calls: Vec::new(),
            result: U64,
            values: types.clone(),
            counted_ranges: Vec::new(),
            overlaps: Vec::new(),
            synthesis: Some(IrSynthesis::Chunk),
            blocks: vec![
                IrBlock {
                    parameters: Vec::new(),
                    instructions: vec![
                        IrInstruction::Define {
                            result: value(9),
                            ty: address,
                            operation: IrOperation::AddressOf {
                                value: value(7),
                                referent,
                            },
                        },
                        IrInstruction::Define {
                            result: value(10),
                            ty: address,
                            operation: IrOperation::AddressOf {
                                value: value(8),
                                referent,
                            },
                        },
                    ],
                    terminator: IrTerminator::Jump {
                        target: IrBlockId(1),
                        arguments: [3, 4, 5, 6, 9, 10].map(value).to_vec(),
                        drops: Vec::new(),
                    },
                },
                IrBlock {
                    parameters: parameters(11..17),
                    instructions: vec![
                        IrInstruction::Define {
                            result: value(17),
                            ty: U64,
                            operation: IrOperation::Call {
                                function: 0,
                                arguments: vec![value(11)],
                            },
                        },
                        IrInstruction::Define {
                            result: value(18),
                            ty: U64,
                            operation: IrOperation::Load {
                                address: value(15),
                                referent,
                            },
                        },
                        IrInstruction::Define {
                            result: value(19),
                            ty: IrType::Bool,
                            operation: IrOperation::Constant(IrConstant::Bool(true)),
                        },
                    ],
                    terminator: IrTerminator::Match {
                        scrutinee: value(19),
                        enum_type: IrEnumType::Bool,
                        targets: vec![
                            IrMatchTarget {
                                tag: 1,
                                block: IrBlockId(2),
                            },
                            IrMatchTarget {
                                tag: 0,
                                block: IrBlockId(3),
                            },
                        ],
                    },
                },
                IrBlock {
                    parameters: Vec::new(),
                    instructions: Vec::new(),
                    terminator: IrTerminator::Jump {
                        target: IrBlockId(1),
                        arguments: (11..17).map(value).collect(),
                        drops: vec![IrDrop {
                            subject: IrDropSubject::Value(value(12)),
                            ty: U64,
                        }],
                    },
                },
                IrBlock {
                    parameters: Vec::new(),
                    instructions: Vec::new(),
                    terminator: IrTerminator::Return {
                        value: value(13),
                        drops: Vec::new(),
                    },
                },
            ],
        };
        let needed = prune_capture_parameters(&mut function, 2).expect("valid chunk graph");
        assert_eq!(needed, [true, true, true, false, true, false]);
        assert_eq!(function.readonly_reference_parameters, [value(3)]);
        assert_eq!(
            function
                .parameters
                .iter()
                .map(|(value, _)| value.0)
                .collect::<Vec<_>>(),
            [0, 1, 2, 3, 4, 5, 7]
        );
        assert_eq!(
            function.blocks[0].instructions.len(),
            1,
            "only unused reconstruction disappears"
        );
        assert_eq!(
            function.blocks[1].instructions.len(),
            3,
            "unused call/load results do not erase runtime uses"
        );
        assert_eq!(
            function.blocks[1]
                .parameters
                .iter()
                .map(|(value, _)| value.0)
                .collect::<Vec<_>>(),
            [11, 12, 13, 15]
        );
        for block in &function.blocks {
            if let IrTerminator::Jump {
                target, arguments, ..
            } = &block.terminator
            {
                assert_eq!(
                    arguments.len(),
                    function.blocks[target.index()].parameters.len()
                );
            }
        }
    }

    /// Every admitted combine, so a widening of the set has to come through
    /// here and answer the property below for its new entry.
    const ADMITTED: [LoopCombine; 10] = [
        LoopCombine::AddWrap,
        LoopCombine::MultiplyWrap,
        LoopCombine::BitAnd,
        LoopCombine::BitOr,
        LoopCombine::BitXor,
        LoopCombine::Minimum,
        LoopCombine::Maximum,
        LoopCombine::And,
        LoopCombine::Or,
        LoopCombine::ExclusiveOr,
    ];

    /// The identity element of every admitted combine is two-sided, at every
    /// width the combine can carry.
    ///
    /// This is the property the whole split rests on and the one nothing else
    /// checks. A chunk of an empty subrange returns the accumulator it was
    /// seeded with, and the right half of every split is seeded with this
    /// value; if it were not an identity, a program's answer would depend on
    /// how many chunks the runtime cut, which is exactly the observable the
    /// design promises does not exist. It is a property of a table rather than
    /// of a program, so a table test is what states it — and it is deliberately
    /// evaluated here rather than through the emitted code, so the check does
    /// not share a defect with the thing it checks.
    #[test]
    fn the_identity_of_every_admitted_combine_is_two_sided() {
        for combine in ADMITTED {
            let mut carried = 0;
            for width in [8_u8, 16, 32, 64] {
                for signed in [false, true] {
                    let ty = IrType::Integer { width, signed };
                    let Some(IrConstant::Integer { bits: unit, .. }) = identity(combine, ty) else {
                        continue;
                    };
                    carried += 1;
                    let Ok(operation) = operation(combine) else {
                        panic!("{combine:?} has an integer identity but is not an integer combine");
                    };
                    for sample in samples(width) {
                        assert_eq!(
                            apply(operation, unit, sample, width, signed),
                            mask(sample, width),
                            "{combine:?} at {}{width}: {unit} is not a left identity",
                            if signed { "i" } else { "u" }
                        );
                        assert_eq!(
                            apply(operation, sample, unit, width, signed),
                            mask(sample, width),
                            "{combine:?} at {}{width}: {unit} is not a right identity",
                            if signed { "i" } else { "u" }
                        );
                    }
                }
            }
            if let Some(IrConstant::Bool(unit)) = identity(combine, IrType::Bool) {
                carried += 1;
                let Err(operation) = operation(combine) else {
                    panic!("{combine:?} has a Bool identity but is not a boolean combine");
                };
                for sample in [false, true] {
                    assert_eq!(apply_boolean(operation, unit, sample), sample);
                    assert_eq!(apply_boolean(operation, sample, unit), sample);
                }
            }
            assert!(
                carried > 0,
                "{combine:?} is admitted and has an identity at no type at all"
            );
        }
    }

    /// Operand values that separate the entries: the extremes each ordering
    /// identity has to survive, and a value with mixed bits.
    fn samples(width: u8) -> Vec<u64> {
        let all_ones = mask(u64::MAX, width);
        vec![
            0,
            1,
            2,
            all_ones,
            all_ones >> 1,
            (all_ones >> 1) + 1,
            mask(0x0F0F_0F0F_0F0F_0F0F, width),
        ]
    }

    fn mask(bits: u64, width: u8) -> u64 {
        if width == 64 {
            bits
        } else {
            bits & ((1_u64 << width) - 1)
        }
    }

    /// One application of an admitted integer combine, at one width, evaluated
    /// independently of anything the compiler emits.
    fn apply(operation: IrIntegerOperation, left: u64, right: u64, width: u8, signed: bool) -> u64 {
        let (left, right) = (mask(left, width), mask(right, width));
        let ordered = |take_smaller: bool| {
            let take_left = if signed {
                (signed_value(left, width) < signed_value(right, width)) == take_smaller
            } else {
                (left < right) == take_smaller
            };
            if take_left { left } else { right }
        };
        mask(
            match operation {
                IrIntegerOperation::AddWrap => left.wrapping_add(right),
                IrIntegerOperation::MultiplyWrap => left.wrapping_mul(right),
                IrIntegerOperation::BitAnd => left & right,
                IrIntegerOperation::BitOr => left | right,
                IrIntegerOperation::BitXor => left ^ right,
                IrIntegerOperation::Minimum => ordered(true),
                IrIntegerOperation::Maximum => ordered(false),
                other => panic!("{other:?} is not an admitted combine"),
            },
            width,
        )
    }

    fn signed_value(bits: u64, width: u8) -> i128 {
        let bits = mask(bits, width);
        if bits & (1_u64 << (width - 1)) == 0 {
            i128::from(bits)
        } else {
            i128::from(bits) - (1_i128 << width)
        }
    }

    fn apply_boolean(operation: IrBooleanOperation, left: bool, right: bool) -> bool {
        match operation {
            IrBooleanOperation::And => left && right,
            IrBooleanOperation::Or => left || right,
            IrBooleanOperation::ExclusiveOr => left != right,
            IrBooleanOperation::Not => panic!("a unary operation is not a combine"),
        }
    }
}
