//! One all-cheap choice per function entry between the parallel body and the
//! canonical ordinary closure (compiler/parallel-lowering/two-worlds).

use super::*;

/// The block that evaluates the entry choice, and the block that enters the
/// ordinary closure when every reachable split is below its grain.
pub(crate) const ENTRY_VERSION_LABEL: &str = "par.region.entry";
pub(crate) const ENTRY_SEQUENTIAL_LABEL: &str = "par.region.sequential";

impl FunctionEmitter<'_, '_> {
    /// Selects the ordinary closure before any source operation runs or any
    /// capture escapes into scheduling. There are exactly two versions, never
    /// one per combination of sites. A clone, a budget variant, a waiting
    /// frame, a dispatch part and a recursive environment member keep their
    /// own entries, and lowering plans no recursive function.
    ///
    /// Opening this first block selects it for the frame prelude, as the
    /// budget entry does; every leaf the choice reads is an incoming formal.
    pub(super) fn emit_entry_version(
        &mut self,
        public: &FunctionAbi,
    ) -> Result<(), BackendFailure> {
        if self.sequential_clones.is_some()
            || self.grain.is_some()
            || self.function.waits()
            || self.dispatch.is_some()
            || self.environment.is_some()
        {
            return Ok(());
        }
        let program = self.program;
        let Some(predicates) = program.entry_versions.get(self.function.name()) else {
            return Ok(());
        };
        let ordinal = program
            .functions()
            .iter()
            .position(|function| std::ptr::eq(function, self.function))
            .ok_or(BackendFailure::InvalidIr)?;
        let ordinal = u32::try_from(ordinal).map_err(|_| BackendFailure::CounterOverflow)?;
        if !sequential_clone_set(program).contains(&ordinal) {
            return Ok(());
        }
        // Lowering plans no function whose entry block is a jump target.
        if self.incoming.first().is_some_and(|edges| !edges.is_empty()) {
            return Err(BackendFailure::InvalidIr);
        }
        self.parallel.selects_entry_versions = true;
        self.output.open_block(ENTRY_VERSION_LABEL.to_owned());
        let mut memo = HashMap::new();
        let mut cheap = "true".to_owned();
        for predicate in predicates {
            let span = self.emit_work_estimate(&predicate.span, &mut memo)?;
            let price = self.emit_work_estimate(&predicate.price, &mut memo)?;
            // The driver's own arithmetic: a positive price, the minimum span
            // ceil(150000 / price), and at least two iterations to divide.
            let price = self.emit_work_binary("llvm.umax.i64", &price, "1")?;
            let quotient = self.next_temporary()?;
            let minimum = self.next_temporary()?;
            writeln!(
                self.output,
                "  %{quotient} = udiv i64 149999, {price}\n  %{minimum} = add i64 %{quotient}, 1"
            )?;
            let minimum = self.emit_work_binary("llvm.umax.i64", &format!("%{minimum}"), "2")?;
            let tiny = self.next_temporary()?;
            writeln!(self.output, "  %{tiny} = icmp ult i64 {span}, {minimum}")?;
            let mut site = format!("%{tiny}");
            if let Some(cells) = &predicate.cells {
                // A far half no larger than three traversals of the families'
                // cells, in iterations of this price, never pays for them.
                let cells = self.emit_work_estimate(cells, &mut memo)?;
                let twice = self.emit_work_binary("llvm.uadd.sat.i64", &cells, &cells)?;
                let charge = self.emit_work_binary("llvm.uadd.sat.i64", &twice, &cells)?;
                let floor = self.next_temporary()?;
                let near = self.next_temporary()?;
                let far = self.next_temporary()?;
                let unpaid = self.next_temporary()?;
                let either = self.next_temporary()?;
                writeln!(
                    self.output,
                    "  %{floor} = udiv i64 {charge}, {price}\n  %{near} = lshr i64 {span}, 1\n  %{far} = sub i64 {span}, %{near}\n  %{unpaid} = icmp ule i64 %{far}, %{floor}\n  %{either} = or i1 %{tiny}, %{unpaid}"
                )?;
                site = format!("%{either}");
            }
            let both = self.next_temporary()?;
            writeln!(self.output, "  %{both} = and i1 {cheap}, {site}")?;
            cheap = format!("%{both}");
        }
        let body = block_label(IrBlockId::from_index(0).ok_or(BackendFailure::InvalidIr)?);
        writeln!(
            self.output,
            "  br i1 {cheap}, label %{ENTRY_SEQUENTIAL_LABEL}, label %{body}"
        )?;
        self.output.open_block(ENTRY_SEQUENTIAL_LABEL.to_owned());
        let clone = sequential_clone_symbol(self.function.name());
        let arguments = ordinary_call_arguments(program, self.function, public)?;
        self.output.symbol(clone.clone());
        match public.result() {
            ResultAbi::Destination(_) => {
                writeln!(self.output, "  call void @{clone}({arguments})\n  ret void")?;
            }
            ResultAbi::StoredValue(ty) => {
                let ty = self.output.type_name(program, ty)?;
                writeln!(
                    self.output,
                    "  %wf.region = call {ty} @{clone}({arguments})\n  store {ty} %wf.region, ptr {RESULT_POINTER}\n  ret void"
                )?;
            }
            ResultAbi::Value(ty) => {
                let ty = self.output.type_name(program, ty)?;
                writeln!(
                    self.output,
                    "  %wf.region = call {ty} @{clone}({arguments})\n  ret {ty} %wf.region"
                )?;
            }
        }
        Ok(())
    }
}
