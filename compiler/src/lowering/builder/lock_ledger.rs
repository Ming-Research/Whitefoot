//! Developer observations of the lock requests actually emitted by lowering.
//! `on every path` means every path in the emitted atomic-body control-flow
//! graph reaches a request before leaving or cycling; no constant or proof
//! feasibility is inferred. `on some paths` requires a conditional bypass.
//! Requests are recorded before the taken-word test: that internal test skips
//! only a unit already held, and runtime suspension resumes acquisition.
//! `never` means no request is reachable in this graph. A unit the guard and
//! block reach only through effects or facts is requested before the guard
//! [SHARE-3], so `never` marks only a unit whose every use lies where the
//! emitted graph cannot reach.

use super::BuildingBlock;
use crate::{IrBlockId, IrTerminator, SourceDescription};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub(super) struct AtomicLedger {
    site: SourceDescription,
    units: Vec<UnitLedger>,
    pub(super) body_entry: Option<IrBlockId>,
    pub(super) exits: HashSet<IrBlockId>,
}

struct UnitLedger {
    name: String,
    state: String,
    requests: Vec<TakeSite>,
}

struct TakeSite {
    block: IrBlockId,
    source: SourceDescription,
    before_guard: bool,
    taken_with: Option<usize>,
}

impl AtomicLedger {
    pub(super) fn new(site: SourceDescription) -> Self {
        Self {
            site,
            units: Vec::new(),
            body_entry: None,
            exits: HashSet::new(),
        }
    }

    pub(super) fn push_unit(&mut self, name: String, state: String) {
        self.units.push(UnitLedger {
            name,
            state,
            requests: Vec::new(),
        });
    }

    pub(super) fn request(
        &mut self,
        index: usize,
        block: IrBlockId,
        direct: &BTreeMap<usize, SourceDescription>,
        before_guard: bool,
    ) {
        // This is the same ordered prefix that take_units_through emits.
        let trigger = direct.range(index..).next();
        let source = trigger
            .map(|(_, source)| source.clone())
            .unwrap_or_else(|| self.site.clone());
        let taken_with = trigger
            .map(|(unit, _)| *unit)
            .filter(|unit| *unit != index);
        self.units[index].requests.push(TakeSite {
            block,
            source,
            before_guard,
            taken_with,
        });
    }

    pub(super) fn render(&self, function: &str, blocks: &[BuildingBlock]) -> Vec<SourceDescription> {
        self.units
            .iter()
            .map(|unit| {
                let eager = unit.requests.iter().position(|site| site.before_guard);
                let (classification, first) = if let Some(index) = eager {
                    ("before the guard", vec![index])
                } else {
                    let paths = self.first_requests(unit, blocks);
                    let classification = if paths.first.is_empty() {
                        "never"
                    } else if paths.bypass && paths.conditional {
                        "on some paths"
                    } else {
                        "on every path"
                    };
                    (classification, paths.first)
                };
                let mut line = SourceDescription::literal(format!("LOCK {function}  "));
                line.append(&self.site);
                line.push_text(format!(
                    "  unit({}): {}  {classification}",
                    unit.name, unit.state,
                ));
                if !first.is_empty() {
                    let uses = first
                        .iter()
                        .map(|index| {
                            let source = &unit.requests[*index].source;
                            (source.identity(), source)
                        })
                        .collect::<BTreeMap<_, _>>();
                    line.push_text("; first use ");
                    for (index, source) in uses.into_values().enumerate() {
                        if index != 0 {
                            line.push_text(" | ");
                        }
                        line.append(source);
                    }
                    let causes = first
                        .iter()
                        .filter_map(|index| {
                            let site = &unit.requests[*index];
                            site.taken_with.map(|later| {
                                let mut cause = SourceDescription::literal(format!(
                                    "unit({}) at ",
                                    self.units[later].name,
                                ));
                                cause.append(&site.source);
                                (cause.identity(), cause)
                            })
                        })
                        .collect::<BTreeMap<_, _>>();
                    if !causes.is_empty() {
                        line.push_text("; taken with ");
                        for (index, cause) in causes.into_values().enumerate() {
                            if index != 0 {
                                line.push_text(" | ");
                            }
                            line.append(&cause);
                        }
                    }
                }
                line
            })
            .collect()
    }

    fn first_requests(&self, unit: &UnitLedger, blocks: &[BuildingBlock]) -> Paths {
        let mut paths = Paths::default();
        let Some(entry) = self.body_entry else {
            return paths;
        };
        // Iterative depth-first traversal detects a path that can cycle
        // forever before its first request, as well as an early leaving edge.
        let mut requests_by_block: HashMap<IrBlockId, Vec<usize>> = HashMap::new();
        for (index, site) in unit.requests.iter().enumerate() {
            if !site.before_guard {
                requests_by_block.entry(site.block).or_default().push(index);
            }
        }
        let mut colors = vec![0u8; blocks.len()];
        let mut pending = vec![(entry, false)];
        let mut first = BTreeSet::new();
        while let Some((block, leaving)) = pending.pop() {
            let index = block.index();
            if leaving {
                colors[index] = 2;
                continue;
            }
            if colors[index] == 1 {
                paths.bypass = true;
                continue;
            }
            if colors[index] == 2 {
                continue;
            }
            if let Some(requests) = requests_by_block.get(&block) {
                first.extend(requests.iter().copied());
                colors[index] = 2;
                continue;
            }
            if self.exits.contains(&block) {
                paths.bypass = true;
                colors[index] = 2;
                continue;
            }
            colors[index] = 1;
            pending.push((block, true));
            let mut successors = match &blocks[index].terminator {
                Some(IrTerminator::Jump { target, .. }) => {
                    pending.push((*target, false));
                    continue;
                }
                Some(IrTerminator::Match { targets, .. }) => targets
                    .iter()
                    .map(|target| target.block())
                    .collect::<Vec<_>>(),
                Some(IrTerminator::IntegerMatch {
                    targets, default, ..
                }) => targets
                    .iter()
                    .map(|target| target.block())
                    .chain(default.iter().copied())
                    .collect::<Vec<_>>(),
                Some(IrTerminator::Return { .. } | IrTerminator::Unreachable) | None => {
                    paths.bypass = true;
                    continue;
                }
            };
            successors.sort_by_key(|target| target.index());
            successors.dedup();
            paths.conditional |= successors.len() > 1;
            paths.bypass |= successors.is_empty();
            pending.extend(successors.into_iter().rev().map(|target| (target, false)));
        }
        paths.first = first.into_iter().collect();
        paths
    }
}

#[derive(Default)]
struct Paths {
    first: Vec<usize>,
    bypass: bool,
    conditional: bool,
}
