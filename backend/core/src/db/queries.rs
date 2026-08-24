use std::{str::FromStr, sync::LazyLock};

use chrono::prelude::*;
use regex::Regex;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{Postgres, QueryBuilder};
use strum::IntoEnumIterator;
use ts_rs::TS;

use crate::db::fields::*;

// Default response items limit
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;
const MAX_OFFSET: i64 = 1_000_000;

static RE_OFFSET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"offset=\d+").unwrap());

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "query.d.ts")]
pub struct QueryObj<T> {
    #[serde(default)]
    pub path: String,

    #[serde(default)]
    pub query: String,

    #[serde(default = "default_limit", deserialize_with = "bounded_limit")]
    pub limit: i64,

    #[serde(default, deserialize_with = "bounded_offset")]
    pub offset: i64,

    #[serde(default = "default_ordering", deserialize_with = "generic_ordering")]
    pub ordering: String,

    #[serde(default, alias = "type")]
    pub type_slug: Option<String>,

    #[serde(default, alias = "tag")]
    pub tag_slug: Option<String>,

    #[serde(default, alias = "author")]
    pub author_slug: Option<String>,

    #[serde(default, alias = "category")]
    pub category_slug: Option<String>,

    #[serde(default, alias = "locale")]
    pub locale_code: Option<String>,

    #[serde(default)]
    pub type_id: Option<i32>,

    #[serde(default, deserialize_with = "split_string_to_vec")]
    pub media_type: Option<Vec<String>>,

    #[serde(default)]
    pub entry_id: Option<i32>,

    #[serde(default, deserialize_with = "split_string_to_vec")]
    pub node_name: Option<Vec<String>>,

    #[serde(default, deserialize_with = "bounded_character_limit")]
    pub character_limit: Option<i32>,

    #[serde(default)]
    pub search: Option<String>,

    #[serde(default, rename = "id")]
    pub search_id: Option<i32>,

    #[serde(default, rename = "locale_id")]
    pub search_locale: Option<i32>,

    #[serde(default, rename = "slug")]
    pub search_slug: Option<String>,

    #[serde(default, rename = "status")]
    pub search_status: Option<String>,

    #[serde(default, deserialize_with = "split_string_to_vec")]
    pub exclude_types: Option<Vec<i32>>,

    #[serde(default)]
    pub output_type: Option<OutputType>,

    #[serde(default)]
    pub last_login: bool,

    #[ts(as = "Option<i32>")]
    #[serde(default)]
    pub group_id: Option<i64>,

    #[serde(default)]
    pub grouped: bool,

    #[serde(default)]
    pub start_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub end_time: Option<DateTime<Utc>>,

    #[serde(default)]
    pub created_after: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_before: Option<DateTime<Utc>>,

    #[serde(
        default = "default_fields",
        deserialize_with = "split_string_to_fields",
        bound(deserialize = "T: FromStr + DefaultFieldsProvider")
    )]
    pub fields: Vec<T>,

    #[serde(default, deserialize_with = "bounded_blocks_limit")]
    pub blocks_limit: Option<i32>,
    #[serde(default)]
    pub blocks_random: bool,
}

impl<T: FromStr + DefaultFieldsProvider> Default for QueryObj<T> {
    fn default() -> Self {
        Self {
            path: String::new(),
            query: String::new(),
            limit: default_limit(),
            offset: 0,
            ordering: default_ordering(),
            type_slug: None,
            tag_slug: None,
            author_slug: None,
            category_slug: None,
            locale_code: None,
            type_id: None,
            media_type: None,
            entry_id: None,
            node_name: None,
            character_limit: None,
            search: None,
            search_id: None,
            search_locale: None,
            search_slug: None,
            search_status: None,
            exclude_types: None,
            output_type: None,
            last_login: false,
            group_id: None,
            grouped: false,
            start_time: None,
            end_time: None,
            created_after: None,
            created_before: None,
            fields: default_fields(),
            blocks_limit: None,
            blocks_random: false,
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

impl<T> QueryResult for QueryObj<T> {
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

impl<T> ResultObject for QueryObj<T> {}

fn default_limit() -> i64 {
    DEFAULT_LIMIT
}

fn bounded_limit<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let limit = i64::deserialize(deserializer)?;
    if (1..=MAX_LIMIT).contains(&limit) {
        Ok(limit)
    } else {
        Err(D::Error::custom(format!(
            "limit must be between 1 and {MAX_LIMIT}"
        )))
    }
}

fn bounded_offset<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let offset = i64::deserialize(deserializer)?;
    if (0..=MAX_OFFSET).contains(&offset) {
        Ok(offset)
    } else {
        Err(D::Error::custom(format!(
            "offset must be between 0 and {MAX_OFFSET}"
        )))
    }
}

fn bounded_character_limit<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<i32>::deserialize(deserializer)?;
    if value.is_none_or(|limit| (1..=100_000).contains(&limit)) {
        Ok(value)
    } else {
        Err(D::Error::custom(
            "character_limit must be between 1 and 100000",
        ))
    }
}

fn bounded_blocks_limit<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<i32>::deserialize(deserializer)?;
    if value.is_none_or(|limit| (1..=1_000).contains(&limit)) {
        Ok(value)
    } else {
        Err(D::Error::custom("blocks_limit must be between 1 and 1000"))
    }
}

fn default_ordering() -> String {
    "created_at DESC".to_string()
}

/// Trait for providing default fields
pub trait DefaultFieldsProvider: Sized + strum::IntoEnumIterator + StrCompare {
    fn get_default_fields() -> Vec<Self>;
}

/// Specialized impl for ContentEntryFields to include nested Node fields
impl DefaultFieldsProvider for ContentEntryFields {
    fn get_default_fields() -> Vec<Self> {
        let mut fields = Self::iter()
            .filter(|f| !f.is_equal_to_str("count"))
            .collect::<Vec<_>>();

        // Add nested author fields
        for author_field in ContentAuthorFields::iter().filter(|f| !f.is_equal_to_str("count")) {
            fields.push(ContentEntryFields::Author(author_field));
        }

        // Add nested category fields
        for category_field in ContentCategoryFields::iter().filter(|f| !f.is_equal_to_str("count"))
        {
            fields.push(ContentEntryFields::Category(category_field));
        }

        // Add nested node fields
        for node_field in ContentNodeFields::iter()
            .filter(|f| !f.is_equal_to_str("count") && *f != ContentNodeFields::ID)
        {
            fields.push(ContentEntryFields::Node(node_field));
        }

        fields
    }
}

/// Macro to implement DefaultFieldsProvider for types without nested fields
macro_rules! impl_default_fields_provider {
    ($($t:ty),*) => {
        $(
            impl DefaultFieldsProvider for $t {
                fn get_default_fields() -> Vec<Self> {
                    Self::iter()
                        .filter(|f| !f.is_equal_to_str("count"))
                        .collect::<Vec<_>>()
                }
            }
        )*
    };
}

// Apply the macro to all field types except ContentEntryFields (which has special handling)
impl_default_fields_provider!(
    AuthRoleFields,
    AuthUserFields,
    CommentFields,
    ConfigurationFields,
    ContentAuthorFields,
    ContentCategoryFields,
    ContentNodeFields,
    ContentNodeTemplateFields,
    ContentTagFields,
    ContentTypeFields,
    LocaleFields,
    MailTargetFields,
    MediaFields,
    TSLanguage
);

/// When no fields are set, collect all fields from given object
fn default_fields<T: DefaultFieldsProvider>() -> Vec<T> {
    T::get_default_fields()
}

/// Helper function, to transform string to array
pub fn split_string_to_fields<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr + DefaultFieldsProvider,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    let mut l = s
        .split(',')
        .filter_map(|s| T::from_str(s.trim()).ok())
        .collect::<Vec<T>>();

    if l.is_empty() {
        l = T::get_default_fields();
    }

    Ok(l)
}

pub fn split_string_to_vec<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let s: Option<String> = Deserialize::deserialize(deserializer)?;

    let Some(s) = s else {
        return Ok(None);
    };

    let l = s
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            T::from_str(s).map_err(|err| D::Error::custom(format!("failed to parse '{s}': {err}")))
        })
        .collect::<Result<Vec<T>, D::Error>>()?;

    if l.is_empty() { Ok(None) } else { Ok(Some(l)) }
}

pub fn generic_ordering<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value: String = Deserialize::deserialize(deserializer)?;
    Ok(parse_ordering(&value))
}

fn parse_ordering(value: &str) -> String {
    value
        .split(',')
        .filter_map(|part| {
            let mut parts = part.split_whitespace();
            let raw_field = parts.next()?;
            let explicit_direction = parts.next();

            // More than a field and an optional direction is not valid ordering
            // syntax. Keeping this strict also prevents SQL fragments here.
            if parts.next().is_some() {
                return None;
            }

            let (field, direction) = if let Some(field) = raw_field.strip_prefix('-') {
                (field, "DESC")
            } else {
                let direction = match explicit_direction {
                    Some(direction) if direction.eq_ignore_ascii_case("asc") => "ASC",
                    Some(direction) if direction.eq_ignore_ascii_case("desc") => "DESC",
                    Some(_) => return None,
                    None => "ASC",
                };
                (raw_field, direction)
            };

            if field.is_empty()
                || !field.chars().all(|character| {
                    character.is_ascii_alphanumeric() || character == '_' || character == '.'
                })
            {
                return None;
            }

            Some(format!("{field} {direction}"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Response object:
/// - gives total amount of items
/// - if there is more then limit restricted, provide a link for the next request
/// - if possible, provide a previous link
/// - gives the actual result
#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "query.d.ts")]
pub struct RespondObj<T> {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<T>,
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

pub struct WhereBuilder {
    builder: QueryBuilder<Postgres>,
    where_set: bool,
}

impl WhereBuilder {
    pub fn new(builder: QueryBuilder<Postgres>) -> Self {
        Self {
            builder,
            where_set: false,
        }
    }

    pub fn push_and(&mut self, operator: Option<&str>, condition: &str) {
        if condition.is_empty() {
            return;
        }

        let op = operator.unwrap_or(" AND");

        if self.where_set {
            self.builder.push(op);
        } else {
            self.builder.push(" WHERE");
            self.where_set = true;
        }

        self.builder.push(format!(" {condition}"));
    }

    pub fn push_and_bind<'a, T>(
        &mut self,
        operator: Option<&str>,
        condition: &str,
        bind: T,
        suffix: Option<&str>,
    ) where
        T: sqlx::Encode<'a, Postgres> + sqlx::Type<Postgres> + 'a,
    {
        if condition.is_empty() {
            return;
        }

        let op = operator.unwrap_or(" AND");

        if self.where_set {
            self.builder.push(op);
        } else {
            self.builder.push(" WHERE");
            self.where_set = true;
        }

        self.builder.push(format!(" {condition}"));
        self.builder.push_bind(bind);

        if let Some(s) = suffix {
            self.builder.push(s);
        }
    }

    pub fn into_inner(self) -> QueryBuilder<Postgres> {
        self.builder
    }
}

#[cfg(test)]
mod tests {
    use axum::{extract::Query, http::Uri};

    use super::{QueryObj, parse_ordering};
    use crate::db::fields::{ContentEntryFields, MediaFields};

    #[test]
    fn accepts_all_supported_ordering_forms() {
        for (input, expected) in [
            ("created_at", "created_at ASC"),
            ("-created_at", "created_at DESC"),
            ("created_at ASC", "created_at ASC"),
            ("created_at DESC", "created_at DESC"),
            ("created_at desc", "created_at DESC"),
            ("author.first_name DESC", "author.first_name DESC"),
            (
                "-created_at,title ASC,author.last_name DESC",
                "created_at DESC, title ASC, author.last_name DESC",
            ),
        ] {
            assert_eq!(parse_ordering(input), expected, "{input}");
        }
    }

    #[test]
    fn ignores_invalid_ordering_fragments() {
        for input in [
            "created_at DESC DROP TABLE content_entries",
            "created_at sideways",
            "created_at;DROP",
            "-",
        ] {
            assert!(parse_ordering(input).is_empty(), "{input}");
        }
    }

    #[test]
    fn parses_reported_entry_request_with_explicit_desc_ordering() {
        let uri: Uri = "/api/content/entries?type=note&locale=de&fields=id%2Ctitle%2Cslug%2Ccreated_at%2Cupdated_at%2Cmedia%2Ccategory.name%2Ccategory.slug%2Ctags%2Cauthor.first_name%2Cauthor.last_name%2Cauthor.slug%2Cnode.text%2Cnode.ast&ordering=created_at+DESC&character_limit=420&blocks_limit=1&limit=18&offset=0"
            .parse()
            .expect("request URI");
        let Query(query): Query<QueryObj<ContentEntryFields>> =
            Query::try_from_uri(&uri).expect("request query");

        assert_eq!(query.ordering, "created_at DESC");
        assert_eq!(query.limit, 18);
        assert_eq!(query.offset, 0);
        assert_eq!(query.character_limit, Some(420));
        assert_eq!(query.blocks_limit, Some(1));
    }

    #[test]
    fn parses_comma_separated_node_name_filter() {
        let uri: Uri = "/api/content/entries/article/example?fields=id%2Cnode.name%2Cnode.text&node_name=description%2Csummary"
            .parse()
            .expect("request URI");
        let Query(query): Query<QueryObj<ContentEntryFields>> =
            Query::try_from_uri(&uri).expect("request query");

        assert_eq!(
            query.node_name,
            Some(vec!["description".to_string(), "summary".to_string()])
        );
    }

    #[test]
    fn parses_single_node_name_filter() {
        let uri: Uri = "/api/content/entries/article/example?node_name=description"
            .parse()
            .expect("request URI");
        let Query(query): Query<QueryObj<ContentEntryFields>> =
            Query::try_from_uri(&uri).expect("request query");

        assert_eq!(query.node_name, Some(vec!["description".to_string()]));
    }

    #[test]
    fn node_name_filter_is_optional() {
        let query = QueryObj::<ContentEntryFields>::default();

        assert!(query.node_name.is_none());
    }

    #[test]
    fn rejects_unbounded_query_work() {
        for value in [
            serde_json::json!({"limit": 201}),
            serde_json::json!({"offset": 1_000_001}),
            serde_json::json!({"character_limit": -1}),
            serde_json::json!({"blocks_limit": 1_001}),
        ] {
            assert!(serde_json::from_value::<QueryObj<MediaFields>>(value).is_err());
        }
    }

    #[test]
    fn accepts_query_values_within_bounds() {
        let query = serde_json::from_value::<QueryObj<MediaFields>>(serde_json::json!({
            "limit": 100,
            "offset": 1_000_000,
            "character_limit": 100_000,
            "blocks_limit": 1_000
        }))
        .expect("bounded query");

        assert_eq!(query.limit, 100);
        assert_eq!(query.offset, 1_000_000);
        assert_eq!(query.character_limit, Some(100_000));
        assert_eq!(query.blocks_limit, Some(1_000));
    }
}
