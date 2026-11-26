//! Program-wide DIAG-1 admission before independent MOD-8 module verdicts.

use std::collections::BTreeMap;

use super::{
    Acceptance, BuildCache, CheckOutcome, CompilationFailure, CompilationFailureKind,
    CompilationStage, CompilerLimits, MODULE_VERDICTS, ModuleCheck, content_digest, decode_outcome,
    finish_syntax, interface_record, module_name, segment_syntax,
};
use crate::{CanonicalSyntaxUnit, ModuleGraph, SourceBundle, SourceId, SourceInput, SyntaxCoordinate};

/// A complete accepted cache input certifies syntax without running any
/// dependency revalidation, which may resolve or check changed interfaces.
fn already_admitted(graph: &ModuleGraph, check: &ModuleCheck<'_>, cache: &BuildCache) -> bool {
    let reading = content_digest(&check.reading);
    if cache
        .load(MODULE_VERDICTS, &check.material)
        .and_then(|payload| decode_outcome(&payload, &reading))
        .is_some_and(|outcome| matches!(outcome, CheckOutcome::Accepted { .. }))
    {
        return true;
    }
    let Some(recorded) = cache
        .load(MODULE_VERDICTS, &check.own)
        .and_then(|payload| Acceptance::decode(&payload))
    else {
        return false;
    };
    let closure = graph.dependency_closure(check.target);
    recorded.interfaces.len() == closure.len()
        && closure.iter().all(|module| {
            let name = module_name(graph, *module);
            let digest = interface_record(&check.selected, *module).map(content_digest);
            recorded
                .interfaces
                .iter()
                .any(|(recorded, value)| *recorded == name && Some(*value) == digest)
        })
}

/// All source failures use the program's record ordinals, including a shared
/// dependency interface that several module checks read. Its single segment
/// rejection is retained once, independent of its number of consumers.
fn collect(
    mut failure: CompilationFailure,
    local: &SourceBundle,
    inputs: &[SourceInput<'_>],
    rejected: &mut BTreeMap<(u32, u64, u64), CompilationFailure>,
) -> Result<(), CompilationFailure> {
    if failure.kind() != CompilationFailureKind::Source {
        return Err(failure);
    }
    let additional = std::mem::take(&mut failure.additional);
    for mut failure in std::iter::once(failure).chain(additional) {
        if let Some(coordinate) = failure.source_coordinate()
            && let Some(file) = local.file(coordinate.source())
            && let Some(ordinal) = inputs
                .iter()
                .position(|candidate| candidate.logical_path() == file.logical_path().as_str())
        {
            let ordinal = u32::try_from(ordinal).map_err(|_| {
                CompilationFailure::new(
                    CompilationStage::SourceEnvelope,
                    CompilationFailureKind::Resource,
                    "program source ordinal exceeds its representation",
                )
            })?;
            let source = SourceId::from_ordinal(ordinal);
            let coordinate = SyntaxCoordinate::new(source, coordinate.start(), coordinate.end());
            failure.coordinate = Some(coordinate);
            // The rendered place already uses the complete source's path and
            // byte offsets. Keep its FORM-2 gap anchor as well as its text.
            rejected
                .entry((
                    source.ordinal(),
                    coordinate.start().value(),
                    coordinate.end().value(),
                ))
                .or_insert(failure);
        } else {
            return Err(failure);
        }
    }
    Ok(())
}

fn rejection(
    rejected: BTreeMap<(u32, u64, u64), CompilationFailure>,
    aggregate: bool,
) -> Option<CompilationFailure> {
    let mut failures = rejected.into_values();
    let mut first = failures.next()?;
    if aggregate {
        first.additional.extend(failures);
    }
    Some(first)
}

/// Completes the program's segment, complete-derivation and canonical gates
/// in that order. Successful derivations are consumed by the module verdicts;
/// an exact cached acceptance stands in for its already admitted records.
pub(super) fn prepare(
    graph: &ModuleGraph,
    inputs: &[SourceInput<'_>],
    checks: &[ModuleCheck<'_>],
    limits: CompilerLimits,
    cache: Option<&BuildCache>,
) -> Result<Vec<Option<CanonicalSyntaxUnit>>, CompilationFailure> {
    let formed = crate::in_parallel(checks, |check| -> Result<_, CompilationFailure> {
        if cache.is_some_and(|cache| already_admitted(graph, check, cache)) {
            return Ok(None);
        }
        let bundle = SourceBundle::with_prelude_and_modules(
            &check.selected,
            graph.modules().to_vec(),
            limits.source,
        )
        .map_err(CompilationFailure::source_envelope)?;
        let result = segment_syntax(&bundle, limits, false);
        Ok(Some((bundle, result)))
    });
    let mut rejected = BTreeMap::new();
    let mut prepared = Vec::with_capacity(checks.len());
    for formed in formed {
        match formed? {
            Some((bundle, Ok(parsed))) => prepared.push(Some((bundle, parsed))),
            Some((bundle, Err(failure))) => {
                collect(failure, &bundle, inputs, &mut rejected)?;
                prepared.push(None);
            }
            None => prepared.push(None),
        }
    }
    if let Some(failure) = rejection(rejected, true) {
        return Err(failure);
    }

    // Complete-source grammar failures cannot outrank any segment failure.
    let mut rejected = BTreeMap::new();
    let mut parsed = Vec::with_capacity(prepared.len());
    for prepared in prepared {
        match prepared {
            Some((bundle, Ok(complete))) => parsed.push(Some((bundle, complete))),
            Some((bundle, Err(failure))) => {
                collect(failure, &bundle, inputs, &mut rejected)?;
                parsed.push(None);
            }
            None => parsed.push(None),
        }
    }
    if let Some(failure) = rejection(rejected, false) {
        return Err(failure);
    }

    // Every canonical source is admitted before resolution or cache
    // revalidation can inspect the meaning of any module's records.
    let mut rejected = BTreeMap::new();
    let mut canonical = Vec::with_capacity(parsed.len());
    for parsed in parsed {
        match parsed {
            Some((bundle, parsed)) => match finish_syntax(&bundle, parsed, limits) {
                Ok(complete) => canonical.push(Some(complete)),
                Err(failure) => {
                    collect(failure, &bundle, inputs, &mut rejected)?;
                    canonical.push(None);
                }
            },
            None => canonical.push(None),
        }
    }
    match rejection(rejected, false) {
        Some(failure) => Err(failure),
        None => Ok(canonical),
    }
}
