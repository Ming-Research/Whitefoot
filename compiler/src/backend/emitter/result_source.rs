//! A destination-form definition's `<symbol>.source` entry and the calls
//! that read through it (compiler/storage-placement and
//! compiler/result-registers, proposed; its facts follow
//! compiler/backend-facts).
//!
//! Every synchronous WF definition whose result uses a caller destination
//! supplies an ordinary-world entry with the same parameters that returns
//! the address holding its result; linked and budget entries are emitted
//! only when called. Waiting definitions have no entry, and clone and grain
//! worlds neither define nor call source entries. A body whose plan selected a
//! source on some return path is emitted as that entry, and its public
//! symbol keeps today's destination ABI by calling it and transferring from
//! the returned address unless that address is the destination. Any other
//! body keeps its public symbol, and its entry calls it and returns the
//! destination, as a linked definition's does. Whether the entry is
//! available therefore depends on the signature alone, and so does a
//! caller's choice to call it (`storage::result_sources`).
//!
//! The returned address is `nonnull` and aligned to the result type, never
//! `noalias`, because it aliases the source, and never `dereferenceable`,
//! because a caller may release the source after its last use. A reference
//! parameter whose own referent a return may address is captured through
//! the return; its exact host spelling is probed independently of the
//! no-capture spelling, and a host without the return-only form gets no
//! capture attribute on it.

use super::*;
use crate::backend::storage::ReturnSource;

/// The suffix of a definition's source entry. The fragment splitter keeps an
/// entry in its public symbol's group (`fragments`).
///
/// No other definition can hold the name, for the reason
/// [`result_body_symbol`] gives for `.body`.
pub(crate) const RESULT_SOURCE_SUFFIX: &str = ".source";

pub(super) fn result_source_symbol(symbol: &str) -> String {
    format!("{symbol}{RESULT_SOURCE_SUFFIX}")
}

/// The SSA name of the address a call's source entry returned.
pub(super) fn result_source_name(call: IrValueId) -> String {
    format!("{}.result.source", value_name(call))
}

/// The return-only capture spelling of a reference parameter whose referent
/// a source entry may return, or no attribute where the probed host does not
/// accept the exact return-only spelling.
fn returned_capture(facts: &str) -> String {
    let toolchain = crate::toolchain::facts();
    let returned = if toolchain.returned_capture_attribute.is_empty() {
        String::new()
    } else {
        format!(" {}", toolchain.returned_capture_attribute)
    };
    facts.replace(&format!(" {}", toolchain.no_capture_attribute), &returned)
}

/// The `nonnull`, aligned `ptr` a source entry returns.
fn source_result_type(
    target: TargetLayout,
    program: &IrProgram,
    ty: IrType,
) -> Result<String, BackendFailure> {
    let layout = validate_static_storage(
        target,
        program,
        &crate::target::TargetStorageType::source(ty),
    )
    .map_err(BackendFailure::TargetLayout)?;
    Ok(format!("nonnull align {} ptr", layout.align()))
}

/// The frame of a public entry's transfer: only the byte-interval scratch
/// its result type's bounded transfer needs, if any.
fn transfer_frame(
    target: TargetLayout,
    program: &IrProgram,
    ty: IrType,
) -> Result<FunctionFramePlan, BackendFailure> {
    let mut specifications = Vec::new();
    let mut ordered = Vec::new();
    let mut scratch_bytes = 0;
    if let Some(granules) = crate::target::bounded_transfer(target, program, ty)
        .map_err(BackendFailure::TargetLayout)?
    {
        for granule in granules {
            if granule.access == crate::target::TransferAccess::Bytes {
                scratch_bytes = scratch_bytes.max(granule.offset + granule.size);
            }
        }
    }
    if scratch_bytes != 0 {
        push_function_slot(
            &mut specifications,
            &mut ordered,
            FunctionSlot::TransferScratch,
            TargetStorageType::bytes(scratch_bytes),
            None,
        )?;
    }
    let target_plan = plan_target_frame(target, program, &specifications)
        .map_err(BackendFailure::TargetLayout)?;
    let slots = ordered
        .iter()
        .enumerate()
        .map(|(logical_index, key)| {
            (
                *key,
                PlannedFunctionSlot {
                    logical_index,
                    pointer: format!("%wf.slot.{logical_index}"),
                },
            )
        })
        .collect();
    Ok(FunctionFramePlan {
        target: target_plan,
        slots,
        ordered,
    })
}

/// The source entries the module's calls name.
pub(super) fn requested_entries(functions: &Module) -> BTreeSet<String> {
    functions
        .entities
        .iter()
        .flat_map(|entity| &entity.references.symbols)
        .filter(|symbol| symbol.ends_with(RESULT_SOURCE_SUFFIX))
        .cloned()
        .collect()
}

/// The body of a source entry that builds every result: call the public
/// symbol into the destination and return the destination.
fn built_body(symbol: &str, arguments: &str) -> Result<FunctionBody, BackendFailure> {
    let mut output = FunctionBody::default();
    output.open_block("entry".to_owned());
    output.symbol(symbol);
    write!(
        output,
        "  call void @{symbol}({arguments})\n  ret ptr {RESULT_POINTER}\n"
    )?;
    Ok(output)
}

impl FunctionEmitter<'_, '_> {
    /// Whether this emission defines the plain `<symbol>.source` entry: a
    /// synchronous destination-form definition in the ordinary world.
    fn defines_source_entry(&self, public: &FunctionAbi) -> bool {
        public.result().uses_destination()
            && !self.function.waits()
            && self.sequential_clones.is_none()
            && self.grain.is_none()
    }

    pub(super) fn source_result_type(&self, ty: IrType) -> Result<String, BackendFailure> {
        source_result_type(self.target, self.program, ty)
    }

    /// The parameters of a source entry, without its destination: the
    /// ordinary facts, with a parameter `returned` names captured only
    /// through the return.
    pub(super) fn source_parameters(
        &self,
        abi: &FunctionAbi,
        returned: impl Fn(usize) -> bool,
    ) -> Result<(Vec<Parameter>, References), BackendFailure> {
        let mut parameters = Vec::with_capacity(abi.parameters().len());
        let mut references = References::default();
        for (index, ((value, _), parameter)) in self
            .function
            .parameters()
            .iter()
            .zip(abi.parameters())
            .enumerate()
        {
            let mut facts = self.reference_parameter_facts(index, parameter.ty())?;
            if returned(index) {
                facts = returned_capture(&facts);
            }
            parameters.extend(incoming_parameters(
                self.program,
                *value,
                *parameter,
                &facts,
                &mut references,
            )?);
        }
        Ok((parameters, references))
    }

    /// What a caller's fragment declares for this definition's source entry,
    /// from the signature alone: any reference parameter may be captured
    /// through the return, whichever body stands behind the entry.
    pub(super) fn source_declaration(
        &self,
        symbol: &str,
        public: &FunctionAbi,
    ) -> Result<Signature, BackendFailure> {
        let (mut parameters, references) = self.source_parameters(public, |position| {
            self.function
                .source_signature()
                .and_then(|signature| signature.parameters().get(position))
                == Some(&crate::IrSourceMode::Reference)
        })?;
        parameters.insert(0, Parameter::named("ptr", RESULT_POINTER));
        let mut signature = Signature::new(
            result_source_symbol(symbol),
            self.source_result_type(public.result().ty())?,
            parameters,
        );
        signature.linkage = Linkage::Internal;
        signature.references = references;
        Ok(signature)
    }

    /// The source entry of a definition whose body builds every result, or
    /// nothing for an emission that defines no source entry.
    pub(super) fn built_source_entry(
        &self,
        symbol: &str,
        public: &FunctionAbi,
    ) -> Result<Module, BackendFailure> {
        let mut module = Module::default();
        if !self.defines_source_entry(public) {
            return Ok(module);
        }
        let (mut parameters, references) = self.source_parameters(public, |_| false)?;
        parameters.insert(0, Parameter::named("ptr", RESULT_POINTER));
        let mut signature = Signature::new(
            result_source_symbol(symbol),
            self.source_result_type(public.result().ty())?,
            parameters,
        );
        signature.linkage = Linkage::Internal;
        signature.references = references;
        let arguments = ordinary_call_arguments(self.program, self.function, public)?;
        let mut entry = signature.define(built_body(symbol, &arguments)?, "")?;
        entry.declare_as(&self.source_declaration(symbol, public)?);
        module.define(entry);
        module.text("\n");
        Ok(module)
    }

    /// A linked definition or budget family's source entry: its public body
    /// builds the result in the destination, which the entry returns. Both
    /// use the ordinary signature-only declaration, so entering or leaving
    /// a budget family cannot change a caller's fragment attributes.
    pub(super) fn destination_source_entry(&self) -> Result<Module, BackendFailure> {
        let public = FunctionAbi::build(self.program, self.function)?;
        self.built_source_entry(&source_symbol(self.function.name()), &public)
    }

    /// The public symbol of a definition emitted as its source entry: call
    /// the entry, and transfer from the returned address into the
    /// destination unless the entry built the result there. Every source
    /// granule is captured before any store, as the permitted result/input
    /// overlap already requires.
    pub(super) fn sourced_public_entry(
        &mut self,
        symbol: &str,
        source: &str,
        public: &FunctionAbi,
    ) -> Result<Module, BackendFailure> {
        let ty = public.result().ty();
        let (mut parameters, references) = self.signature_parameters(public)?;
        parameters.insert(0, Parameter::named("ptr", RESULT_POINTER));
        let mut signature = Signature::new(symbol, "void", parameters);
        signature.references = references;
        let arguments = ordinary_call_arguments(self.program, self.function, public)?;
        let frame = std::mem::replace(
            &mut self.frame,
            transfer_frame(self.target, self.program, ty)?,
        );
        let output = std::mem::take(&mut self.output);
        let transferred = self.emit_source_transfer(source, &arguments, ty);
        let frame = std::mem::replace(&mut self.frame, frame);
        let mut body = std::mem::replace(&mut self.output, output);
        transferred?;
        let prelude = frame.render(self.target, self.program, &mut body.references)?;
        let mut module = Module::default();
        module.define(signature.define(body, &prelude)?);
        module.text("\n");
        Ok(module)
    }

    fn emit_source_transfer(
        &mut self,
        source: &str,
        arguments: &str,
        ty: IrType,
    ) -> Result<(), BackendFailure> {
        self.output.open_block("entry".to_owned());
        self.output.symbol(source);
        writeln!(
            self.output,
            "  %wf.source = call ptr @{source}({arguments})\n  %wf.built = icmp eq ptr %wf.source, {RESULT_POINTER}\n  br i1 %wf.built, label %done, label %transfer"
        )?;
        self.output.open_block("transfer".to_owned());
        self.copy_storage(ty, "%wf.source", RESULT_POINTER)?;
        writeln!(self.output, "  br label %done")?;
        self.output.open_block("done".to_owned());
        writeln!(self.output, "  ret void")?;
        Ok(())
    }

    /// The slot of a Load snapshot a source return hands back by address,
    /// which takes no copy before that return.
    pub(super) fn source_returned_slot(
        &self,
        block: IrBlockId,
        terminator: &IrTerminator,
    ) -> Option<usize> {
        let IrTerminator::Return { value, .. } = terminator else {
            return None;
        };
        match self.storage.return_source(block.index())? {
            ReturnSource::Load(_) if !self.function.waits() => self.storage.slot(*value),
            _ => None,
        }
    }

    /// One `Return` of a body emitted as its source entry: the selected
    /// source's address, or the destination after building the result there.
    /// The caller reads the address after these drops; the storage plan
    /// proved that they leave the source unchanged.
    pub(super) fn emit_source_return(
        &mut self,
        block: IrBlockId,
        value: IrValueId,
        drops: &[IrDrop],
    ) -> Result<(), BackendFailure> {
        let address = match self.storage.return_source(block.index()) {
            Some(ReturnSource::Load(address)) => self.value_name(address),
            Some(ReturnSource::Call(call)) => result_source_name(call),
            None => {
                self.store_value_at(value, RESULT_POINTER)?;
                RESULT_POINTER.to_owned()
            }
        };
        self.emit_drops(drops)?;
        writeln!(self.output, "  ret ptr {address}").map_err(|_| BackendFailure::TextEmission)
    }

    /// A call whose result is read at its source: the callee's source entry
    /// when this call names the callee's plain symbol, otherwise the
    /// symbol it names, whose destination then holds the result. Either way
    /// the address the result's reads use is defined here.
    pub(super) fn emit_source_call(
        &mut self,
        result: IrValueId,
        callee: &str,
        name: &str,
        budgeted: bool,
        destination: &str,
        rendered: &str,
    ) -> Result<(), BackendFailure> {
        let address = result_source_name(result);
        if !budgeted && callee == source_symbol(name) && !self.is_overlap_join_site(result) {
            let entry = result_source_symbol(callee);
            self.output.symbol(entry.clone());
            writeln!(self.output, "  {address} = call ptr @{entry}({rendered})")?;
        } else {
            self.output.symbol(callee.to_owned());
            writeln!(
                self.output,
                "  call void @{callee}({rendered})\n  {address} = getelementptr i8, ptr {destination}, i64 0"
            )?;
        }
        Ok(())
    }
}
