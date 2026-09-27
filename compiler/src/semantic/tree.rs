//! Semantic-stage error mapping for the shared syntax views.

use super::{CheckStop, SemanticCompilerFailure};
use crate::syntax::views::SyntaxViewFailure;
pub(super) use crate::syntax::views::{ConditionalAlternative, SyntaxView as TreeView};

impl From<SyntaxViewFailure> for SemanticCompilerFailure {
    fn from(failure: SyntaxViewFailure) -> Self {
        match failure {
            SyntaxViewFailure::InvalidCanonicalTree => Self::InvalidCanonicalTree,
            SyntaxViewFailure::CounterOverflow => Self::CounterOverflow,
        }
    }
}

impl From<SyntaxViewFailure> for CheckStop {
    fn from(failure: SyntaxViewFailure) -> Self {
        SemanticCompilerFailure::from(failure).into()
    }
}
