use std::collections::HashSet;

use crate::utils::errors::NurError;

pub const ENTRY_EDITOR_FIELDS: &[&str] = &[
    "title",
    "slug",
    "author",
    "tags",
    "category",
    "start_time",
    "end_time",
    "status",
    "delete",
];
pub const ENTRY_STATUSES: &[&str] = &["draft", "published", "archived"];

pub fn valid_entry_status(status: &str) -> bool {
    ENTRY_STATUSES.contains(&status)
}

pub fn validate_hidden_entry_fields(fields: &[String]) -> Result<(), NurError> {
    if fields.len() > ENTRY_EDITOR_FIELDS.len() {
        return Err(NurError::InvalidInput);
    }

    let mut unique = HashSet::with_capacity(fields.len());
    if fields
        .iter()
        .any(|field| !ENTRY_EDITOR_FIELDS.contains(&field.as_str()) || !unique.insert(field))
    {
        return Err(NurError::InvalidInput);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{valid_entry_status, validate_hidden_entry_fields};

    #[test]
    fn accepts_known_editor_values() {
        assert!(valid_entry_status("published"));
        assert!(validate_hidden_entry_fields(&["author".into(), "end_time".into()]).is_ok());
    }

    #[test]
    fn rejects_invalid_or_duplicate_editor_fields() {
        assert!(!valid_entry_status("pending"));
        assert!(validate_hidden_entry_fields(&["unknown".into()]).is_err());
        assert!(validate_hidden_entry_fields(&["author".into(), "author".into()]).is_err());
    }
}
