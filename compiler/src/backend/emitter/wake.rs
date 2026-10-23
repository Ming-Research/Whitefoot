//! [SHARE-3] Wake tests (compiler/waiting-contexts/state-locks): the bytes a
//! watch registers with the runtime, a `wf_wake_test` of `completion/bridge.h`
//! filled in the frame just before the registration call.
//!
//! The header's length and postfix codes and each atom's width, flags and
//! comparison are constants; an atom's offsets are the distances of its
//! scalars' addresses from the state's, and its operand is the captured
//! value extended to 64 bits as the scalar is. Bytes no comparison uses are
//! zero, so two watches of one condition register equal tests.
//! A full-guard callback uses the separate 24-byte tagged descriptor and a
//! frame-resident block of captured scalars; it is never bucket-coalesced.

use std::fmt::Write;

use super::*;
use crate::{IrWakeCode, IrWakeComparison, IrWakeOperand, IrWakeTest};

/// `WF_WAKE_PROGRAM` and `WF_WAKE_ATOMS` in `completion/bridge.h`.
const WAKE_PROGRAM: usize = 7;
const WAKE_ATOMS: usize = 4;
/// The codes and flags of `completion/bridge.h`.
const WAKE_AND: u8 = 8;
const WAKE_OR: u8 = 9;
const WAKE_SIGNED: u8 = 1;
const WAKE_BIT: u8 = 2;
const WAKE_FIELD: u8 = 4;
/// Byte positions in `wf_wake_test` and `wf_wake_atom`.
const ATOMS_AT: u64 = 8;
const ATOM_BYTES: u64 = 24;

/// The frame storage of the test the watch defining `result` registers.
pub(super) fn wake_test_name(result: IrValueId) -> String {
    format!("%wf.wake.v{}", result.ordinal())
}

/// The entry-block reservation of that storage.
pub(super) fn wake_test_storage(result: IrValueId, callback: bool) -> String {
    format!(
        "  {} = alloca [{} x i8], align 8\n",
        wake_test_name(result),
        if callback {
            24
        } else {
            crate::backend::WAKE_TEST_SIZE
        }
    )
}

fn comparison_code(comparison: IrWakeComparison) -> u8 {
    match comparison {
        IrWakeComparison::Equal => 0,
        IrWakeComparison::NotEqual => 1,
        IrWakeComparison::Less => 2,
        IrWakeComparison::LessEqual => 3,
        IrWakeComparison::Greater => 4,
        IrWakeComparison::GreaterEqual => 5,
    }
}

fn byte_array(bytes: &[u8; 8]) -> String {
    let items = bytes
        .iter()
        .map(|byte| format!("i8 {byte}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[8 x i8] [{items}]")
}

impl FunctionEmitter<'_, '_> {
    /// The scalar an atom compares: its LLVM type, width in bytes and flags.
    fn wake_scalar(&mut self, address: IrValueId) -> Result<(String, u8, u8), BackendFailure> {
        let Some(IrType::Address(referent)) = self.value_type(address) else {
            return Err(BackendFailure::InvalidIr);
        };
        let ty = referent.ty();
        let signed = match ty {
            IrType::Integer { signed, .. } => signed,
            IrType::Bool => false,
            IrType::Nominal(id)
                if self
                    .program
                    .nominal(id)
                    .is_some_and(crate::IrNominal::is_tag_only_enum) =>
            {
                false
            }
            _ => return Err(BackendFailure::InvalidIr),
        };
        let llvm = self.output.type_name(self.program, ty)?;
        let (width, bit) = match llvm.as_str() {
            "i1" => (1, true),
            "i8" => (1, false),
            "i16" => (2, false),
            "i32" => (4, false),
            "i64" => (8, false),
            _ => return Err(BackendFailure::InvalidIr),
        };
        let mut flags = 0;
        if signed {
            flags |= WAKE_SIGNED;
        }
        if bit {
            flags |= WAKE_BIT;
        }
        Ok((llvm, width, flags))
    }

    /// The distance of `address` from the state's, as the `i32` the atom
    /// stores.
    fn wake_offset(&mut self, state: &str, address: IrValueId) -> Result<String, BackendFailure> {
        let field = self.next_temporary()?;
        let base = self.next_temporary()?;
        let distance = self.next_temporary()?;
        let offset = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{field} = ptrtoint ptr {} to i64\n  %{base} = ptrtoint ptr {state} to i64\n  %{distance} = sub i64 %{field}, %{base}\n  %{offset} = trunc i64 %{distance} to i32",
            self.value_name(address)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{offset}"))
    }

    /// One store of `typed`, a type and its value, into the test at byte
    /// `at`.
    fn wake_store(
        &mut self,
        storage: &str,
        at: u64,
        typed: &str,
        align: u8,
    ) -> Result<(), BackendFailure> {
        let place = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{place} = getelementptr inbounds i8, ptr {storage}, i64 {at}\n  store {typed}, ptr %{place}, align {align}"
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Fills the test the watch defining `result` registers, when it has
    /// one, and answers the call's argument for it.
    pub(super) fn emit_wake_test(
        &mut self,
        result: IrValueId,
        test: Option<&IrWakeTest>,
    ) -> Result<String, BackendFailure> {
        let Some(test) = test else {
            return Ok("ptr null".to_owned());
        };
        if let Some(function) = test.callback {
            return self.emit_wake_callback(result, test, function);
        }
        if test.program.is_empty()
            || test.program.len() > WAKE_PROGRAM
            || test.atoms.is_empty()
            || test.atoms.len() > WAKE_ATOMS
            || test.values.is_empty()
        {
            return Err(BackendFailure::InvalidIr);
        }
        let storage = wake_test_name(result);
        let mut header = [0_u8; 8];
        header[0] = u8::try_from(test.program.len()).map_err(|_| BackendFailure::InvalidIr)?;
        for (slot, code) in header[1..].iter_mut().zip(&test.program) {
            *slot = match code {
                IrWakeCode::Atom(index) if usize::from(*index) < test.atoms.len() => *index + 1,
                IrWakeCode::Atom(_) => return Err(BackendFailure::InvalidIr),
                IrWakeCode::And => WAKE_AND,
                IrWakeCode::Or => WAKE_OR,
            };
        }
        writeln!(
            self.output,
            "  store {}, ptr {storage}, align 8",
            byte_array(&header)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let state = self.value_name(test.values[0]);
        for (index, atom) in test.atoms.iter().enumerate() {
            let at = ATOMS_AT
                + ATOM_BYTES * u64::try_from(index).map_err(|_| BackendFailure::InvalidIr)?;
            let field = *test.values.get(atom.field).ok_or(BackendFailure::InvalidIr)?;
            let (llvm, width, mut flags) = self.wake_scalar(field)?;
            let offset = self.wake_offset(&state, field)?;
            self.wake_store(&storage, at, &format!("i32 {offset}"), 8)?;
            let (other, value) = match atom.operand {
                IrWakeOperand::Field(position) => {
                    let other = *test.values.get(position).ok_or(BackendFailure::InvalidIr)?;
                    if self.wake_scalar(other)? != (llvm.clone(), width, flags) {
                        return Err(BackendFailure::InvalidIr);
                    }
                    flags |= WAKE_FIELD;
                    (self.wake_offset(&state, other)?, "0".to_owned())
                }
                IrWakeOperand::Value(position) => {
                    let operand = *test.values.get(position).ok_or(BackendFailure::InvalidIr)?;
                    let Some(IrType::Address(referent)) = self.value_type(field) else {
                        return Err(BackendFailure::InvalidIr);
                    };
                    if self.value_type(operand) != Some(referent.ty()) {
                        return Err(BackendFailure::InvalidIr);
                    }
                    let name = self.value_name(operand);
                    let value = if llvm == "i64" {
                        name
                    } else {
                        let extended = self.next_temporary()?;
                        let extension = if flags & WAKE_SIGNED != 0 { "sext" } else { "zext" };
                        writeln!(
                            self.output,
                            "  %{extended} = {extension} {llvm} {name} to i64"
                        )
                        .map_err(|_| BackendFailure::TextEmission)?;
                        format!("%{extended}")
                    };
                    ("0".to_owned(), value)
                }
            };
            self.wake_store(&storage, at + 4, &format!("i32 {other}"), 4)?;
            self.wake_store(&storage, at + 8, &format!("i64 {value}"), 8)?;
            let tail = [width, flags, comparison_code(atom.comparison), 0, 0, 0, 0, 0];
            self.wake_store(&storage, at + 16, &byte_array(&tail), 8)?;
        }
        Ok(format!("ptr {storage}"))
    }

    /// Each capture occupies one eight-byte slot. Its typed store/load keeps
    /// signedness and Bool representation unchanged; unused bytes are never
    /// read by the adapter or used to coalesce registrations.
    fn emit_wake_callback(
        &mut self,
        result: IrValueId,
        test: &IrWakeTest,
        function: u32,
    ) -> Result<String, BackendFailure> {
        let callee = self
            .program
            .functions()
            .get(function as usize)
            .ok_or(BackendFailure::InvalidIr)?;
        if !test.atoms.is_empty()
            || !test.program.is_empty()
            || callee.result() != IrType::Bool
            || callee.waits()
            || callee.parameters().len() != test.values.len()
            || test.values.is_empty()
        {
            return Err(BackendFailure::InvalidIr);
        }
        let storage = wake_test_name(result);
        let captures = format!("{storage}.captures");
        for (index, (value, (_, expected))) in
            test.values.iter().zip(callee.parameters()).enumerate()
        {
            if self.value_type(*value) != Some(*expected) {
                return Err(BackendFailure::InvalidIr);
            }
            if index == 0 {
                if !matches!(expected, IrType::Address(_)) {
                    return Err(BackendFailure::InvalidIr);
                }
                continue;
            }
            let llvm = self.output.type_name(self.program, *expected)?;
            if !matches!(llvm.as_str(), "i1" | "i8" | "i16" | "i32" | "i64") {
                return Err(BackendFailure::InvalidIr);
            }
            self.wake_store(
                &captures,
                (index as u64 - 1) * 8,
                &format!("{llvm} {}", self.value_name(*value)),
                8,
            )?;
        }
        let adapter = callback_symbol(callee.name());
        self.output.symbol(adapter.clone());
        self.wake_store(&storage, 0, &byte_array(&[255, 0, 0, 0, 0, 0, 0, 0]), 8)?;
        self.wake_store(&storage, 8, &format!("ptr @{adapter}"), 8)?;
        self.wake_store(&storage, 16, &format!("ptr {captures}"), 8)?;
        Ok(format!("ptr {storage}"))
    }
}

fn callback_symbol(name: &str) -> String {
    format!("{}.wake_filter", source_symbol(name))
}

/// The C runtime returns an int, whereas source Bool returns an i1. Keep the
/// adapter and its references in the normal emission graph so fragment
/// linkage sees both the address stored by the waiter and the thunk it calls.
pub(super) fn callbacks(
    program: &IrProgram,
    sequential: &HashSet<u32>,
) -> Result<Module, BackendFailure> {
    let callbacks = program
        .functions()
        .iter()
        .flat_map(|function| function.blocks())
        .flat_map(|block| block.instructions())
        .filter_map(|instruction| match instruction {
            IrInstruction::Define {
                operation:
                    IrOperation::SharedWatch {
                        test: Some(test), ..
                    }
                    | IrOperation::WatchObject {
                        test: Some(test), ..
                    },
                ..
            } => test.callback,
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut module = Module::default();
    for ordinal in callbacks {
        let function = program
            .functions()
            .get(ordinal as usize)
            .ok_or(BackendFailure::InvalidIr)?;
        if function.waits() || function.result() != IrType::Bool {
            return Err(BackendFailure::InvalidIr);
        }
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        let mut arguments = vec!["ptr %state".to_owned()];
        for (index, (_, ty)) in function.parameters().iter().enumerate().skip(1) {
            let llvm = body.type_name(program, *ty)?;
            if !matches!(llvm.as_str(), "i1" | "i8" | "i16" | "i32" | "i64") {
                return Err(BackendFailure::InvalidIr);
            }
            writeln!(
                body,
                "  %capture.at.{index} = getelementptr inbounds i8, ptr %captures, i64 {}\n  %capture.{index} = load {llvm}, ptr %capture.at.{index}, align 8",
                (index - 1) * 8
            )?;
            arguments.push(format!("{llvm} %capture.{index}"));
        }
        // An otherwise eligible source helper may have optional overlap
        // lowering. Its sequential closure removes all scheduler entry.
        let callee = if sequential.contains(&ordinal) {
            sequential_clone_symbol(function.name())
        } else {
            source_symbol(function.name())
        };
        body.symbol(callee.clone());
        writeln!(
            body,
            "  %ready = call i1 @{callee}({})\n  %result = zext i1 %ready to i32\n  ret i32 %result",
            arguments.join(", ")
        )?;
        let signature = Signature::new(
            callback_symbol(function.name()),
            "i32",
            vec![Parameter::named("ptr", "%state"), Parameter::named("ptr", "%captures")],
        );
        module.define(signature.define(body, "")?);
        module.text("\n");
    }
    Ok(module)
}
