//! Shared projected selector identities. Paths may contain earlier selector
//! identities through exact references, so clone, hash and comparison must
//! operate on the finite DAG rather than expand its tree.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use super::{CapturedTerm, CapturedValue, PlaceStep, ResolvedPlace};

#[derive(Clone)]
pub(crate) struct ProjectedSelector(Arc<SelectorData>);

struct SelectorData {
    place: ResolvedPlace,
    fingerprint: u64,
}

impl ProjectedSelector {
    pub(crate) fn new(place: ResolvedPlace) -> Self {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        place.hash(&mut hash);
        Self(Arc::new(SelectorData {
            place,
            fingerprint: hash.finish(),
        }))
    }

    pub(crate) fn place(&self) -> &ResolvedPlace {
        &self.0.place
    }
}

/// One bounded diagnostic rendering of shared selector paths. Labels are
/// traversal ordered descriptions, never Whitefoot expressions or identities.
#[derive(Default)]
pub(crate) struct SelectorRendering {
    ids: HashMap<ProjectedSelector, usize>,
    definitions: Vec<Option<String>>,
}

impl SelectorRendering {
    /// Direct scalar projections retain their ordinary source spelling.
    pub(crate) fn needs_label(selector: &ProjectedSelector) -> bool {
        selector.place().path.iter().any(|step| {
            let projected = |value: &CapturedValue| {
                matches!(
                    value.term,
                    CapturedTerm::Place(_) | CapturedTerm::SupersededPlace(_)
                )
            };
            match step {
                PlaceStep::Index(value) | PlaceStep::Page(value) => projected(value),
                PlaceStep::Range(range) => projected(&range.start) || projected(&range.end),
                _ => false,
            }
        })
    }

    pub(crate) fn reference(&mut self, selector: &ProjectedSelector) -> (usize, bool) {
        if let Some(id) = self.ids.get(selector) {
            return (*id, false);
        }
        let id = self.definitions.len();
        self.ids.insert(selector.clone(), id);
        self.definitions.push(None);
        (id, true)
    }

    pub(crate) fn label(id: usize) -> String {
        format!("⟪selection {id}⟫")
    }

    pub(crate) fn define(&mut self, id: usize, rendered: String) {
        self.definitions[id] = Some(rendered);
    }

    pub(crate) fn finish(self, rendered: String) -> String {
        if self.definitions.is_empty() {
            return rendered;
        }
        let definitions = self
            .definitions
            .into_iter()
            .enumerate()
            .map(|(id, definition)| {
                format!(
                    "{} = {}",
                    Self::label(id),
                    definition.expect("every reserved selector is rendered")
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        format!("{rendered} (selection DAG, diagnostic labels only: {definitions})")
    }

    pub(crate) fn is_diagnostic(rendered: &str) -> bool {
        rendered.contains("⟪selection ")
    }
}

impl std::fmt::Debug for ProjectedSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Receipt keys include Debug output: retain the complete DAG, not
        // just its hash, and assign traversal IDs without ordering pointers.
        let mut nodes = vec![self.clone()];
        let mut ids = HashMap::from([(Arc::as_ptr(&self.0), 0_usize)]);
        let mut cursor = 0;
        while cursor < nodes.len() {
            let path = nodes[cursor].place().path.clone();
            for step in &path {
                let values: &[&CapturedValue] = match step {
                    PlaceStep::Index(value) | PlaceStep::Page(value) => &[value],
                    PlaceStep::Range(range) => &[&range.start, &range.end],
                    _ => &[],
                };
                for value in values {
                    if let CapturedTerm::Place(selector) | CapturedTerm::SupersededPlace(selector) =
                        &value.term
                    {
                        let next = nodes.len();
                        if let std::collections::hash_map::Entry::Vacant(entry) =
                            ids.entry(Arc::as_ptr(&selector.0))
                        {
                            entry.insert(next);
                            nodes.push(selector.clone());
                        }
                    }
                }
            }
            cursor += 1;
        }
        fn captured(
            f: &mut std::fmt::Formatter<'_>,
            value: &CapturedValue,
            ids: &HashMap<*const SelectorData, usize>,
        ) -> std::fmt::Result {
            write!(f, "({:?},", value.capture)?;
            match &value.term {
                CapturedTerm::Place(selector) => {
                    write!(f, "Place@{}", ids[&Arc::as_ptr(&selector.0)])?
                }
                CapturedTerm::SupersededPlace(selector) => {
                    write!(f, "SupersededPlace@{}", ids[&Arc::as_ptr(&selector.0)])?
                }
                other => write!(f, "{other:?}")?,
            }
            write!(f, ")")
        }
        write!(f, "ProjectedSelector[")?;
        for (id, selector) in nodes.iter().enumerate() {
            let place = selector.place();
            write!(f, "{id}:({:?},{:?},[", place.root, place.atomic_aliases)?;
            for step in &place.path {
                match step {
                    PlaceStep::Index(value) | PlaceStep::Page(value) => {
                        write!(
                            f,
                            "{}",
                            if matches!(step, PlaceStep::Index(_)) {
                                "Index"
                            } else {
                                "Page"
                            }
                        )?;
                        captured(f, value, &ids)?;
                    }
                    PlaceStep::Range(range) => {
                        write!(f, "Range(")?;
                        captured(f, &range.start, &ids)?;
                        write!(f, ",")?;
                        captured(f, &range.end, &ids)?;
                        write!(f, ")")?;
                    }
                    other => write!(f, "{other:?}")?,
                }
                write!(f, ",")?;
            }
            write!(f, "]),")?;
        }
        write!(f, "]")
    }
}

impl Hash for ProjectedSelector {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.fingerprint.hash(state);
    }
}
impl PartialEq for ProjectedSelector {
    fn eq(&self, other: &Self) -> bool {
        self.0.fingerprint == other.0.fingerprint && self.cmp(other) == Ordering::Equal
    }
}
impl Eq for ProjectedSelector {}
impl PartialOrd for ProjectedSelector {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ProjectedSelector {
    fn cmp(&self, other: &Self) -> Ordering {
        compare(self, other, &mut HashMap::new())
    }
}

type Compared = HashMap<(*const SelectorData, *const SelectorData), Ordering>;

fn compare(left: &ProjectedSelector, right: &ProjectedSelector, memo: &mut Compared) -> Ordering {
    if Arc::ptr_eq(&left.0, &right.0) {
        return Ordering::Equal;
    }
    let key = (Arc::as_ptr(&left.0), Arc::as_ptr(&right.0));
    if let Some(order) = memo.get(&key) {
        return *order;
    }
    let a = left.place();
    let b = right.place();
    let mut order = a
        .root
        .cmp(&b.root)
        .then_with(|| a.atomic_aliases.cmp(&b.atomic_aliases));
    if order == Ordering::Equal {
        for (a, b) in a.path.iter().zip(&b.path) {
            order = step(a, b, memo);
            if order != Ordering::Equal {
                break;
            }
        }
        order = order.then_with(|| a.path.len().cmp(&b.path.len()));
    }
    memo.insert(key, order);
    order
}

fn term_rank(term: &CapturedTerm) -> u8 {
    match term {
        CapturedTerm::Literal(_) => 0,
        CapturedTerm::Binding(_) => 1,
        CapturedTerm::Place(_) => 2,
        CapturedTerm::SupersededPlace(_) => 3,
        CapturedTerm::Superseded(_) => 4,
        CapturedTerm::Const(_) => 5,
        CapturedTerm::Opaque => 6,
    }
}

fn value(a: &CapturedValue, b: &CapturedValue, memo: &mut Compared) -> Ordering {
    a.capture.cmp(&b.capture).then_with(|| {
        term_rank(&a.term)
            .cmp(&term_rank(&b.term))
            .then_with(|| match (&a.term, &b.term) {
                (CapturedTerm::Place(a), CapturedTerm::Place(b))
                | (CapturedTerm::SupersededPlace(a), CapturedTerm::SupersededPlace(b)) => {
                    compare(a, b, memo)
                }
                // These matching variants carry no recursive selectors.
                _ => a.term.cmp(&b.term),
            })
    })
}

fn rank(step: &PlaceStep) -> u8 {
    match step {
        PlaceStep::Descendant(_) => 0,
        PlaceStep::Field(_) => 1,
        PlaceStep::Deref => 2,
        PlaceStep::Payload { .. } => 3,
        PlaceStep::Index(_) => 4,
        PlaceStep::Page(_) => 5,
        PlaceStep::Range(_) => 6,
        PlaceStep::Part(_) => 7,
        PlaceStep::Measure(_) => 8,
    }
}

fn step(a: &PlaceStep, b: &PlaceStep, memo: &mut Compared) -> Ordering {
    rank(a).cmp(&rank(b)).then_with(|| match (a, b) {
        (PlaceStep::Index(a), PlaceStep::Index(b)) | (PlaceStep::Page(a), PlaceStep::Page(b)) => {
            value(a, b, memo)
        }
        (PlaceStep::Range(a), PlaceStep::Range(b)) => {
            value(&a.start, &b.start, memo).then_with(|| value(&a.end, &b.end, memo))
        }
        (PlaceStep::Field(a), PlaceStep::Field(b)) => a.cmp(b),
        (
            PlaceStep::Payload {
                variant: av,
                field: af,
            },
            PlaceStep::Payload {
                variant: bv,
                field: bf,
            },
        ) => av.cmp(bv).then_with(|| af.cmp(bf)),
        (PlaceStep::Part(a), PlaceStep::Part(b)) => a.cmp(b),
        (PlaceStep::Measure(a), PlaceStep::Measure(b)) => a.cmp(b),
        (PlaceStep::Descendant(a), PlaceStep::Descendant(b)) => a
            .loop_id
            .cmp(&b.loop_id)
            .then_with(|| a.holder.cmp(&b.holder))
            .then_with(|| a.ty.cmp(&b.ty))
            .then_with(|| a.range.cmp(&b.range))
            .then_with(|| a.readonly.cmp(&b.readonly))
            .then_with(|| a.frozen_content.cmp(&b.frozen_content)),
        _ => Ordering::Equal,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::model::BindingId;
    use crate::semantic::places::CaptureId;

    fn leaf(field: u32) -> ProjectedSelector {
        let mut place = ResolvedPlace::binding(BindingId(0));
        place.path.push(PlaceStep::Field(field));
        ProjectedSelector::new(place)
    }

    fn fork(left: ProjectedSelector, right: ProjectedSelector) -> ProjectedSelector {
        let mut place = ResolvedPlace::binding(BindingId(1));
        for selected in [left, right] {
            place.path.push(PlaceStep::Index(CapturedValue::new(
                CaptureId::SpellingDetermined,
                CapturedTerm::Place(selected),
            )));
        }
        ProjectedSelector::new(place)
    }

    #[test]
    fn sharing_and_hash_collisions_do_not_choose_selector_identity() {
        let a = leaf(0);
        assert_eq!(fork(a.clone(), a), fork(leaf(0), leaf(0)));
        let a = leaf(0);
        let b = ProjectedSelector(Arc::new(SelectorData {
            place: leaf(1).place().clone(),
            fingerprint: a.0.fingerprint,
        }));
        assert_ne!(a, b);
        assert_ne!(a.cmp(&b), Ordering::Equal);
        assert_ne!(format!("{a:?}"), format!("{b:?}"));
    }

    #[test]
    fn repeated_reference_selectors_keep_a_finite_rendered_dag() {
        let mut left = leaf(0);
        let mut right = leaf(0);
        for _ in 0..24 {
            left = fork(left.clone(), left);
            right = fork(right.clone(), right);
        }
        assert_eq!(left, right);
        let text = format!("{left:?}");
        assert!(text.len() < 20_000);
        assert_eq!(text.matches(":(Binding").count(), 25);
    }
}
