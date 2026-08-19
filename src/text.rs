//! Small shared text normalization helpers.

/// Borrow a non-empty, trimmed string without allocating.
pub(crate) fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
