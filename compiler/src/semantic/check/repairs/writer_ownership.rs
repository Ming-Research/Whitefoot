//! Ownership repairs name the selected place rather than a placeholder.

pub(in crate::semantic::check) fn copy(place: &str) -> String {
    format!("write `{place}` without `move`")
}

pub(in crate::semantic::check) fn consume(place: &str) -> String {
    format!("write `move {place}`")
}

pub(in crate::semantic::check) const fn constant() -> &'static str {
    "read the const through a field, subscript or measure supported by its type, or take a `&` reference"
}

pub(in crate::semantic::check) fn join(binding: Option<(&str, &str)>) -> String {
    let general = "every predecessor of a join agrees on a binding's live-or-dead status: consume it on every predecessor, on none, or commit a value back into it before the predecessor that consumed it reaches the join";
    match binding {
        Some((binding, ty)) => format!(
            "{general}; alternatively, if this owner is no longer needed, consume it unconditionally outside the `if`: write `release::<{ty}>(value: move {binding});` before the code that needs it released and remove the conditional consume"
        ),
        None => general.to_owned(),
    }
}
