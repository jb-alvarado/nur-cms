use jiff::{civil::DateTime, tz::TimeZone};
use serde::{Deserialize, Deserializer, de};
use serde_json::Value;

#[cfg(debug_assertions)]
use colored::Colorize;

mod timestamp;
pub use timestamp::Timestamp;

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

pub fn to_datetime_utc<'de, D>(deserializer: D) -> Result<Timestamp, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_datetime(&s, &TimeZone::system()).map_err(de::Error::custom)
}

fn parse_datetime(value: &str, zone: &TimeZone) -> Result<Timestamp, jiff::Error> {
    let value = value.trim();
    if let Ok(timestamp) = value.parse::<Timestamp>() {
        return Ok(timestamp);
    }

    // Naive input denotes local wall time. Reject DST gaps and folds rather
    // than silently selecting a different instant.
    let datetime = value.parse::<DateTime>()?;
    zone.to_ambiguous_timestamp(datetime)
        .unambiguous()
        .map(Timestamp::from)
}

pub fn to_datetime_utc_option<'de, D>(deserializer: D) -> Result<Option<Timestamp>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)?
        .map(|value| parse_datetime(&value, &TimeZone::system()).map_err(de::Error::custom))
        .transpose()
}

pub fn is_zero<T: PartialEq + Default>(val: &T) -> bool {
    *val == T::default()
}

pub fn is_null(v: &Value) -> bool {
    *v == Value::Null
}

#[cfg(test)]
mod datetime_tests {
    use serde::Deserialize;

    use super::*;

    #[test]
    fn accepts_existing_timestamp_formats_and_rejects_dst_gaps_and_folds() {
        let utc: Timestamp = "2024-01-01T10:00:00Z".parse().expect("UTC");
        let zone = TimeZone::get("Europe/Berlin").expect("zone");
        for value in [
            "2024-01-01T12:00:00+02:00",
            "2024-01-01 12:00:00+02:00",
            " 2024-01-01 11:00:00 ",
            "2024-01-01T11:00:00",
        ] {
            assert_eq!(parse_datetime(value, &zone).expect(value), utc);
        }
        assert!(parse_datetime("2024-03-31T02:30:00", &zone).is_err());
        assert!(parse_datetime("2024-10-27T02:30:00", &zone).is_err());
        assert!(parse_datetime("2024-02-30T12:00:00Z", &zone).is_err());
        let fractional = parse_datetime("2024-01-01 11:00:00.123456", &zone).expect("fractional");
        assert_eq!(fractional.to_string(), "2024-01-01T10:00:00.123456Z");
    }

    #[test]
    fn nullable_api_timestamps_accept_null_and_offsets() {
        #[derive(Deserialize)]
        struct Input {
            #[serde(default, deserialize_with = "to_datetime_utc_option")]
            timestamp: Option<Timestamp>,
        }
        assert!(
            serde_json::from_str::<Input>("{}")
                .expect("missing")
                .timestamp
                .is_none()
        );
        assert!(
            serde_json::from_str::<Input>(r#"{"timestamp":null}"#)
                .expect("null")
                .timestamp
                .is_none()
        );
        let input = serde_json::from_str::<Input>(r#"{"timestamp":"2024-01-01T12:00:00+02:00"}"#)
            .expect("offset");
        assert_eq!(
            input.timestamp.expect("present").to_string(),
            "2024-01-01T10:00:00Z"
        );
    }
}
