use chrono::{DateTime, Local, LocalResult, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Deserializer, de};
use serde_json::Value;

#[cfg(debug_assertions)]
use colored::Colorize;

pub mod fields;
pub mod handles;
pub mod models;
pub mod queries;
pub mod serialize;

#[cfg(debug_assertions)]
const FM: sqlformat::FormatOptions = sqlformat::FormatOptions {
    indent: sqlformat::Indent::Spaces(4),
    uppercase: Some(true),
    ignore_case_convert: None,
    lines_between_queries: 1,
    inline: false,
    max_inline_block: 50,
    max_inline_arguments: None,
    max_inline_top_level: None,
    joins_as_top_level: false,
    dialect: sqlformat::Dialect::PostgreSql,
};

#[cfg(debug_assertions)]
pub fn format_sql(s: impl AsRef<str>) -> String {
    let sql = s.as_ref();

    sqlformat::format(sql, &sqlformat::QueryParams::default(), &FM)
        .bright_black()
        .to_string()
}

pub fn to_datetime_utc<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?.trim().to_string();

    // Attempt: RFC3339 / ISO8601 (e.g., "2024-01-01T12:00:00+02:00" or "2024-01-01T12:00:00Z")
    if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Attempt: e.g., "2024-01-01 12:00:00+02:00"
    if let Ok(dt) = DateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%:z") {
        return Ok(dt.with_timezone(&Utc));
    }

    // Attempt: naive formats → interpret as *local time*
    let naive = if let Ok(n) = NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f") {
        Some(n)
    } else if let Ok(n) = NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S") {
        Some(n)
    } else {
        NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").ok()
    };

    if let Some(naive) = naive {
        // local time → UTC
        match Local.from_local_datetime(&naive) {
            LocalResult::Single(local_dt) => Ok(local_dt.with_timezone(&Utc)),
            LocalResult::Ambiguous(_, _) => Err(de::Error::custom("Ambiguous local datetime")),
            LocalResult::None => Err(de::Error::custom("Invalid local datetime")),
        }
    } else {
        Err(de::Error::custom(format!("Invalid date format: {}", s)))
    }
}

pub fn to_datetime_utc_option<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = to_datetime_utc(deserializer)?;

    Ok(Some(s))
}

pub fn is_zero<T: PartialEq + Default>(val: &T) -> bool {
    *val == T::default()
}

pub fn is_null(v: &Value) -> bool {
    *v == Value::Null
}
