use std::{str::FromStr, sync::LazyLock};

use chrono::prelude::*;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{Postgres, QueryBuilder};

use crate::db::fields::{ColumnCounter, StrCompare, TypeSlag};

// Default response items limit
const DEFAULT_LIMIT: i64 = 50;

static RE_OFFSET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"offset=\d+").unwrap());

/// Response object:
/// - gives total amount of items
/// - if there is more then limit restricted, provide a link for the next request
/// - if possible, provide a previous link
/// - gives the actual result
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RespondObj<T> {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<T>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct QueryObj<T: FromStr + strum::IntoEnumIterator + StrCompare> {
    #[serde(default)]
    pub path: String,

    #[serde(default)]
    pub query: String,

    #[serde(default = "default_limit")]
    pub limit: i64,

    #[serde(default)]
    pub offset: i64,

    #[serde(default = "default_ordering", deserialize_with = "generic_ordering")]
    pub ordering: String,

    #[serde(default)]
    pub type_slag: Option<TypeSlag>,

    #[serde(default)]
    pub url_slag: Option<String>,

    #[serde(default)]
    pub search: Option<String>,

    #[serde(default, rename = "id")]
    pub search_id: Option<i32>,

    #[serde(default, rename = "locale")]
    pub search_locale: Option<String>,

    #[serde(default, rename = "slag")]
    pub search_slag: Option<String>,

    #[serde(default)]
    pub created_after: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_before: Option<DateTime<Utc>>,

    #[serde(
        default = "default_fields",
        deserialize_with = "split_string_to_fields"
    )]
    pub fields: Vec<T>,
}

impl<T: FromStr + strum::IntoEnumIterator + StrCompare> Default for QueryObj<T> {
    fn default() -> Self {
        Self {
            path: String::new(),
            query: String::new(),
            limit: default_limit(),
            offset: 0,
            ordering: default_ordering(),
            type_slag: None,
            url_slag: None,
            search: None,
            search_id: None,
            search_locale: None,
            search_slag: None,
            created_after: None,
            created_before: None,
            fields: default_fields(),
        }
    }
}

pub trait ResultObject: QueryResult {}

pub trait QueryResult {
    fn path(&self) -> String;
    fn query(&self) -> String;
    fn limit(&self) -> i64;
    fn offset(&self) -> i64;
}

impl<T: FromStr + strum::IntoEnumIterator + StrCompare> QueryResult for QueryObj<T> {
    fn path(&self) -> String {
        self.path.clone()
    }

    fn query(&self) -> String {
        self.query.clone()
    }

    fn limit(&self) -> i64 {
        self.limit
    }

    fn offset(&self) -> i64 {
        self.offset
    }
}

impl<T: FromStr + strum::IntoEnumIterator + StrCompare> ResultObject for QueryObj<T> {}

fn default_limit() -> i64 {
    DEFAULT_LIMIT
}

fn default_ordering() -> String {
    "created_at ASC".to_string()
}

/// When no fields are set, collect all fields from given object
fn default_fields<T>() -> Vec<T>
where
    T: strum::IntoEnumIterator + StrCompare,
{
    T::iter()
        .filter(|f| !f.is_equal_to_str("count"))
        .collect::<Vec<_>>()
}

/// Helper function, to transform strings to different arrays
pub fn split_string_to_fields<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr + strum::IntoEnumIterator + StrCompare,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    let mut l = s
        .split(',')
        .filter_map(|s| T::from_str(s.trim()).ok())
        .collect::<Vec<T>>();

    if l.is_empty() {
        l = T::iter()
            .filter(|f| !f.is_equal_to_str("count"))
            .collect::<Vec<_>>();
    }

    Ok(l)
}

pub fn generic_ordering<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let mut s: String = Deserialize::deserialize(deserializer)?;
    let re = Regex::new(r"[^\w,-_]+").unwrap();
    let mut order_clause = vec![];

    s = re.replace_all(&s, "").to_string();

    for mut field in s.split(',') {
        field = field.trim();

        if field.starts_with('-') {
            order_clause.push(format!("{} DESC", remove_first_char(field)));
        } else if !field.is_empty() {
            order_clause.push(format!("{field} ASC"));
        }
    }

    Ok(order_clause.join(", "))
}

fn remove_first_char(value: &str) -> &str {
    let mut chars = value.chars();
    chars.next();
    chars.as_str()
}

/// Create Response Object
///
/// add path for next and preview for pagination
impl<T> RespondObj<T>
where
    T: ColumnCounter,
{
    pub fn new<U>(query_obj: &U, data: Vec<T>) -> Self
    where
        U: ResultObject + std::fmt::Debug,
    {
        let mut next_string = None;
        let mut prev_string = None;
        let mut next_query = vec![];
        let mut prev_query = vec![];
        let url_string = query_obj.path();

        let count = if data.is_empty() {
            0
        } else {
            data[0].total_count()
        };

        if !query_obj.query().is_empty() {
            next_query.push(query_obj.query());
            prev_query.push(query_obj.query());
        }

        if query_obj.limit() > 0 && count > query_obj.offset() + query_obj.limit() {
            let next_offset = query_obj.offset() + query_obj.limit();

            if !query_obj.query().contains("limit=") {
                next_query.push(format!("limit={}", query_obj.limit()));
            }

            if query_obj.query().contains("offset=") {
                next_query[0] = RE_OFFSET
                    .replace(&query_obj.query(), format!("offset={next_offset}"))
                    .to_string();
            } else {
                next_query.push(format!("offset={next_offset}"));
            }

            next_string = Some(format!("{url_string}?{}", next_query.join("&")));
        }

        if count > 0 && query_obj.offset() > query_obj.limit() {
            let prev_offset = query_obj.offset() - query_obj.limit();

            if !query_obj.query().contains("limit=") {
                prev_query.push(format!("limit={}", query_obj.limit()));
            }

            if query_obj.query().contains("offset=") {
                prev_query[0] = RE_OFFSET
                    .replace(&query_obj.query(), format!("offset={prev_offset}"))
                    .to_string();
            } else {
                prev_query.push(format!("offset={prev_offset}"));
            }

            prev_string = Some(format!("{url_string}?{}", prev_query.join("&")));
        } else if query_obj.limit() > 0 && count > 0 && query_obj.offset() - query_obj.limit() == 0
        {
            if !query_obj.query().contains("limit=") {
                prev_query.push(format!("limit={}", query_obj.limit()));
            }

            if query_obj.query().contains("offset=") {
                prev_query[0] = RE_OFFSET
                    .replace(&query_obj.query(), "offset=0")
                    .to_string();
            } else {
                prev_query.push("offset=0".to_string());
            }

            prev_string = Some(format!("{url_string}?{}", prev_query.join("&")));
        }

        Self {
            count,
            next: next_string,
            previous: prev_string,
            results: data,
        }
    }
}

pub fn where_chain(builder: &mut QueryBuilder<Postgres>, operator: Option<&str>, condition: &str) {
    let s = operator.unwrap_or(" AND");

    if builder.sql().contains("WHERE") {
        builder.push(s);
    } else {
        builder.push(" WHERE");
    }

    if !condition.is_empty() {
        builder.push(format!(" {condition}"));
    }
}
