//! Developer observations of the lock requests actually emitted by lowering.
//! `on every path` means every path in the emitted atomic-body control-flow
//! graph reaches a request before leaving or cycling; no constant or proof
//! feasibility is inferred. `on some paths` requires a conditional bypass.
//! Requests are recorded before the taken-word test: that internal test skips
//! only a group already held, and runtime suspension resumes acquisition.
//! `never` means no request is reachable in this graph. SHARE-2 requires the
//! guard or block to name every target, and proof-only names do not satisfy
//! that rule, so this is not a way to admit unused targets.

use super::BuildingBlock;
use crate::{IrBlockId, IrTerminator};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub(super) struct AtomicLedger {
    site: String,
    groups: Vec<GroupLedger>,
    pub(super) body_entry: Option<IrBlockId>,
    pub(super) exits: HashSet<IrBlockId>,
}

struct GroupLedger {
    names: String,
    state: String,
    requests: Vec<TakeSite>,
}

struct TakeSite {
    block: IrBlockId,
    source: String,
    before_guard: bool,
    taken_with: Option<usize>,
}

impl AtomicLedger {
    pub(super) fn new(site: String) -> Self {
        Self {
            site,
            groups: Vec::new(),
            body_entry: None,
            exits: HashSet::new(),
        }
    }

    pub(super) fn push_group(&mut self, names: String, state: String) {
        self.groups.push(GroupLedger {
            names,
            state,
            requests: Vec::new(),
        });
    }

    pub(super) fn request(
        &mut self,
        index: usize,
        block: IrBlockId,
        direct: &BTreeMap<usize, String>,
        before_guard: bool,
    ) {
        // This is the same ordered prefix that take_groups_through emits.
        let trigger = direct.range(index..).next();
        let source = trigger
            .map(|(_, source)| source.clone())
            .unwrap_or_else(|| self.site.clone());
        let taken_with = trigger
            .map(|(group, _)| *group)
            .filter(|group| *group != index);
        self.groups[index].requests.push(TakeSite {
            block,
            source,
            before_guard,
            taken_with,
        });
    }

    pub(super) fn render(&self, function: &str, blocks: &[BuildingBlock]) -> Vec<String> {
        self.groups
            .iter()
            .map(|group| {
                let eager = group.requests.iter().position(|site| site.before_guard);
                let (classification, first) = if let Some(index) = eager {
                    ("before the guard", vec![index])
                } else {
                    let paths = self.first_requests(group, blocks);
                    let classification = if paths.first.is_empty() {
                        "never"
                    } else if paths.bypass && paths.conditional {
                        "on some paths"
                    } else {
                        "on every path"
                    };
                    (classification, paths.first)
                };
                let mut line = format!(
                    "LOCK {function}  {}  targets({}): {}  {classification}",
                    self.site, group.names, group.state,
                );
                if !first.is_empty() {
                    let uses = first
                        .iter()
                        .map(|index| group.requests[*index].source.as_str())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>()
                        .join(" | ");
                    line.push_str(&format!("; first use {uses}"));
                    let causes = first
                        .iter()
                        .filter_map(|index| {
                            let site = &group.requests[*index];
                            site.taken_with.map(|later| {
                                format!(
                                    "targets({}) at {}",
                                    self.groups[later].names, site.source,
                                )
                            })
                        })
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>();
                    if !causes.is_empty() {
                        line.push_str(&format!("; taken with {}", causes.join(" | ")));
                    }
                }
                line
            })
            .collect()
    }

    fn first_requests(&self, group: &GroupLedger, blocks: &[BuildingBlock]) -> Paths {
        let mut paths = Paths::default();
        let Some(entry) = self.body_entry else {
            return paths;
        };
        // Iterative depth-first traversal detects a path that can cycle
        // forever before its first request, as well as an early leaving edge.
        let mut requests_by_block: HashMap<IrBlockId, Vec<usize>> = HashMap::new();
        for (index, site) in group.requests.iter().enumerate() {
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
            match &blocks[index].terminator {
                Some(IrTerminator::Jump { target, .. }) => pending.push((*target, false)),
                Some(IrTerminator::Match { targets, .. }) => {
                    let mut successors = targets
                        .iter()
                        .map(|target| target.block())
                        .collect::<Vec<_>>();
                    successors.sort_by_key(|target| target.index());
                    successors.dedup();
                    paths.conditional |= successors.len() > 1;
                    paths.bypass |= successors.is_empty();
                    pending.extend(successors.into_iter().rev().map(|target| (target, false)));
                }
                Some(IrTerminator::Return { .. } | IrTerminator::Unreachable) | None => {
                    paths.bypass = true;
                }
            }
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
