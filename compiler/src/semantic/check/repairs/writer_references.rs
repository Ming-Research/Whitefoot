//! Effect roots and reference kinds explained at the writer's rejected use.

use crate::{DeclarationRole, SemanticCompilerFailure};

use super::super::{CheckContext, CheckStop, Checker, LocalBinding};

pub(in crate::semantic::check) fn by_value_effect_root(name: &str) -> String {
    format!(
        "`{name}` is a by-value parameter; an effect root is a reference parameter. Remove this effect entry: the call already records the argument's read or consumption."
    )
}

pub(in crate::semantic::check) fn array_range_argument(place: &str) -> String {
    format!(
        "pass a range reference `&{place}[0_u64..n]`, with `n <= {place}.len` proved, or `&{place}[0_u64..{place}.len]` for the whole array; extend the enclosing effect row to cover any reads through reference parameters [EFF-2]"
    )
}

impl Checker<'_, '_> {
    /// A payload bound by a reference match has the scrutinee's reference
    /// mode [OWN-13]. This formatter reads that existing classification and
    /// does not change the missing-dereference judgment.
    pub(in crate::semantic::check) fn reference_payload_repair(
        &self,
        check_context: &CheckContext<'_>,
        local: &LocalBinding,
        place: &str,
    ) -> Result<String, CheckStop> {
        let declaration = self
            .types
            .declarations
            .resolved
            .declaration(local.declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if declaration.role() != DeclarationRole::MatchBinder
            || !self.types.is_copy_type(check_context, local.ty)?
        {
            return Ok("write `p^`".to_owned());
        }
        let name = declaration.spelling();
        let suffix = place.strip_prefix(name).unwrap_or_default();
        let mut repair = format!(
            "`{name}` is a reference because the match scrutinee is a reference; read the copy payload with `{name}^`"
        );
        if !suffix.is_empty() {
            repair.push_str(&format!("; write `{name}^{suffix}` here"));
        }
        Ok(repair)
    }
}
