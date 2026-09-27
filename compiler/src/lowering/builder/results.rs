use crate::semantic::{
    BindingId, CheckedDrop, CheckedExpression, CheckedType, NominalId, PropagationContext,
};

use super::{
    IrBuilder, IrEnumType, IrMatchTarget, IrOperation, IrTerminator, IrType, LoweringFailure,
    lower_type,
};

impl IrBuilder<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_propagate(
        &mut self,
        binding: BindingId,
        scrutinee: &CheckedExpression,
        result_nominal: NominalId,
        return_nominal: NominalId,
        ok_type: CheckedType,
        error_type: CheckedType,
        error_drops: &[CheckedDrop],
        context: &PropagationContext,
    ) -> Result<(), LoweringFailure> {
        let result_nominal = self.erased(result_nominal)?;
        let return_nominal = self.erased(return_nominal)?;
        if self.result != IrType::Nominal(return_nominal)
            || context.function.is_empty()
            || context.node_path.components().is_empty()
        {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }

        let scrutinee = self.expression(scrutinee)?;
        if self.value_type(scrutinee)? != IrType::Nominal(result_nominal) {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let base_bindings = self.bindings.clone();
        let ok_block = self.new_block(&[])?.0;
        let error_block = self.new_block(&[])?.0;
        self.terminate(IrTerminator::Match {
            scrutinee,
            enum_type: IrEnumType::Nominal(result_nominal),
            targets: vec![
                IrMatchTarget {
                    tag: 0,
                    block: ok_block,
                },
                IrMatchTarget {
                    tag: 1,
                    block: error_block,
                },
            ],
        })?;

        self.current = Some(error_block);
        self.bindings = base_bindings.clone();
        let error = self.define(
            lower_type(self.erasure, error_type)?,
            IrOperation::ProjectVariant {
                aggregate: scrutinee,
                nominal: result_nominal,
                variant: 1,
                field: 0,
            },
        )?;
        let returned = self.define(
            IrType::Nominal(return_nominal),
            IrOperation::ConstructEnum {
                nominal: return_nominal,
                variant: 1,
                fields: vec![error],
            },
        )?;
        let drops = self.lower_drops(error_drops)?;
        self.terminate(IrTerminator::Return {
            value: returned,
            drops,
        })?;

        self.current = Some(ok_block);
        self.bindings = base_bindings;
        let value = self.define(
            lower_type(self.erasure, ok_type)?,
            IrOperation::ProjectVariant {
                aggregate: scrutinee,
                nominal: result_nominal,
                variant: 0,
                field: 0,
            },
        )?;
        if self.bindings.insert(binding, value).is_some() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        self.promote_binding_if_needed(binding)
    }
}
