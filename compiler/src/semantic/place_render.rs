//! One source-spelling walk for checker and proof diagnostic places.

use super::model::{CheckedType, IntegerType};
use super::places::{CapturedValue, PlaceStep, WindowPart};

#[derive(Clone, Copy)]
pub(super) enum PlaceProjection {
    Place(PlaceStep),
    FormalSubscript(u32),
    FormalPage(u32),
}

/// The names and selected types retained by the stage rendering a place.
/// Lookup failure remains the checker's error or the proof renderer's `?`.
pub(super) trait PlaceNames {
    type Error;

    fn field(
        &self,
        ty: Option<CheckedType>,
        field: u32,
    ) -> Result<Option<(String, CheckedType)>, Self::Error>;
    fn payload(
        &self,
        ty: Option<CheckedType>,
        variant: u32,
        field: u32,
    ) -> Result<Option<(String, String, CheckedType)>, Self::Error>;
    fn deref(
        &self,
        ty: Option<CheckedType>,
    ) -> Result<(bool, Option<CheckedType>), Self::Error>;
    fn element(
        &self,
        ty: Option<CheckedType>,
        range: bool,
    ) -> Result<Option<CheckedType>, Self::Error>;
    fn table_entry(
        &self,
        ty: Option<CheckedType>,
    ) -> Result<Option<CheckedType>, Self::Error>;
    fn offset(&self, value: CapturedValue) -> Result<String, Self::Error>;
}

/// Root spelling is supplied by the caller: body bindings, entry images and
/// counted binders keep their distinct source roles while sharing selectors.
pub(super) fn render_place_projections<N: PlaceNames>(
    names: &N,
    mut rendered: String,
    mut ty: Option<CheckedType>,
    mut range: bool,
    index_prefix: usize,
    projections: impl IntoIterator<Item = PlaceProjection>,
) -> Result<String, N::Error> {
    let path = projections.into_iter().collect::<Vec<_>>();
    for (position, projection) in path.iter().enumerate() {
        let step = match *projection {
            PlaceProjection::Place(step) => step,
            PlaceProjection::FormalSubscript(ordinal) => {
                rendered.push_str(&format!("[parameter #{ordinal}]"));
                ty = names.element(ty, range)?;
                range = false;
                continue;
            }
            PlaceProjection::FormalPage(ordinal) => {
                rendered.push_str(&format!(".pages[parameter #{ordinal}]"));
                ty = names.element(ty, range)?;
                range = true;
                continue;
            }
        };
        match step {
            PlaceStep::Descendant(target) => {
                rendered.push_str(".**");
                ty = Some(target.ty);
                range = target.range;
            }
            PlaceStep::Field(field) => match names.field(ty, field)? {
                Some((name, selected)) => {
                    rendered.push('.');
                    rendered.push_str(&name);
                    ty = Some(selected);
                }
                None => {
                    rendered.push_str(".?");
                    ty = None;
                }
            },
            PlaceStep::Payload { variant, field } => match names.payload(ty, variant, field)? {
                Some((variant, field, selected)) => {
                    rendered.push_str(&format!(".{variant}.{field}"));
                    ty = Some(selected);
                }
                None => {
                    rendered.push_str(".?.?");
                    ty = None;
                }
            },
            PlaceStep::Deref => {
                let (boxed, selected) = names.deref(ty)?;
                rendered.push_str(if boxed { ".inner" } else { "^" });
                ty = selected;
            }
            PlaceStep::Page(offset) => {
                rendered.push_str(&format!(".pages[{}]", names.offset(offset)?));
                ty = names.element(ty, range)?;
                range = true;
            }
            PlaceStep::Index(offset) => {
                // An indexed atomic binder already names its selected entry.
                if position < index_prefix {
                    continue;
                }
                rendered.push_str(&format!("[{}]", names.offset(offset)?));
                let segment = !range && matches!(ty, Some(CheckedType::Segments { .. }));
                match names.table_entry(ty)? {
                    Some(entry) if !range => {
                        ty = Some(entry);
                        range = matches!(
                            path.get(position + 1),
                            Some(PlaceProjection::Place(PlaceStep::Index(_)))
                        );
                    }
                    _ => {
                        ty = names.element(ty, range)?;
                        range = segment;
                    }
                }
            }
            PlaceStep::Range(_)
                if !range && matches!(ty, Some(CheckedType::Segments { .. })) =>
            {
                rendered.push_str(".all");
                ty = names.element(ty, range)?;
                range = true;
            }
            PlaceStep::Range(span) => {
                rendered.push_str(&format!(
                    "[{}..{}]",
                    names.offset(span.start)?,
                    names.offset(span.end)?
                ));
                ty = names.element(ty, range)?;
                range = true;
            }
            PlaceStep::Part(part) => {
                rendered.push('.');
                rendered.push_str(part.spelling());
                ty = names.element(ty, range)?;
                range = matches!(part, WindowPart::Filled | WindowPart::Free);
            }
            PlaceStep::Measure(measure) => {
                rendered.push('.');
                rendered.push_str(measure.spelling());
                ty = Some(CheckedType::Integer(IntegerType::U64));
            }
        }
    }
    Ok(rendered)
}
