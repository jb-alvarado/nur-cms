use std::{collections::HashSet, ops::ControlFlow, time::Duration};

use futures_util::TryStreamExt;
use sqlparser::{
    ast::{Expr, ObjectName, Select, Statement as AstStatement, TableFactor, Visit, Visitor},
    dialect::PostgreSqlDialect,
    parser::Parser,
};
use sqlx::{
    Column, PgConnection, Row, TypeInfo, ValueRef,
    postgres::{PgArguments, PgRow},
    query::Query,
};

use super::bindings::nur::cms::database::{NullType, QueryResult, Statement, Value};

const MAX_DATABASE_ROWS: usize = 10_000;
const MAX_DATABASE_STATEMENTS: usize = 32;
const MAX_DATABASE_PARAMS: usize = 128;
const MAX_DATABASE_SQL_BYTES: usize = 64 * 1024;
const RESULT_OVERHEAD: usize = 32;
const ROW_OVERHEAD: usize = 24;
const VALUE_OVERHEAD: usize = 16;

#[derive(Debug, thiserror::Error)]
pub(super) enum DatabaseHostError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("database parameter is invalid")]
    InvalidParameter,
    #[error("database result contains unsupported PostgreSQL type {0}")]
    UnsupportedType(String),
    #[error("database result exceeds the plugin response limit")]
    ResponseTooLarge,
    #[error("database result exceeds the plugin row limit")]
    TooManyRows,
    #[error("database transaction rollback failed: {rollback}; original error: {original}")]
    Rollback {
        original: Box<Self>,
        rollback: sqlx::Error,
    },
}

#[derive(Clone, Copy)]
pub(super) struct ValidatedStatement {
    returns_rows: bool,
}

pub(super) fn validate_statement(statement: &Statement) -> Result<ValidatedStatement, String> {
    let sql = statement.sql.trim();
    if sql.is_empty()
        || sql.len() > MAX_DATABASE_SQL_BYTES
        || statement.params.len() > MAX_DATABASE_PARAMS
    {
        return Err("invalid database statement".into());
    }
    if sql.contains('\0')
        || sql.contains(';')
        || sql.contains("--")
        || sql.contains("/*")
        || sql.contains("*/")
        || sql.contains('"')
        || sql.contains('\'')
        || sql.contains('.')
    {
        return Err("database statement contains unsupported syntax".into());
    }
    validate_placeholders(sql, statement.params.len())?;

    let mut parsed = Parser::parse_sql(&PostgreSqlDialect {}, sql)
        .map_err(|_| "database statement is not valid PostgreSQL".to_string())?;
    if parsed.len() != 1 {
        return Err("exactly one database statement is required".into());
    }
    let parsed = parsed.pop().expect("one parsed statement was checked");
    let returns_rows = match &parsed {
        AstStatement::Query(_) => true,
        AstStatement::Insert(insert) => insert.returning.is_some(),
        AstStatement::Update(update) => update.returning.is_some(),
        AstStatement::Delete(delete) => delete.returning.is_some(),
        _ => {
            return Err("only SELECT, INSERT, UPDATE, and DELETE statements are allowed".into());
        }
    };

    let mut validator = SqlSubsetValidator::default();
    if let ControlFlow::Break(reason) = parsed.visit(&mut validator) {
        return Err(reason.into());
    }

    Ok(ValidatedStatement { returns_rows })
}

pub(super) fn validate_transaction_size(statements: &[Statement]) -> Result<(), String> {
    if statements.is_empty() || statements.len() > MAX_DATABASE_STATEMENTS {
        return Err(format!(
            "transaction must contain between 1 and {MAX_DATABASE_STATEMENTS} statements"
        ));
    }
    Ok(())
}

pub(super) async fn execute_statements(
    pool: &sqlx::PgPool,
    schema: &str,
    statements: &[Statement],
    validated: &[ValidatedStatement],
    response_limit: usize,
    statement_timeout: Duration,
) -> Result<Vec<QueryResult>, DatabaseHostError> {
    debug_assert_eq!(statements.len(), validated.len());
    let mut transaction = pool.begin().await?;
    set_plugin_context(&mut transaction, schema, statement_timeout).await?;
    let mut results = Vec::with_capacity(statements.len());
    let mut response_size = 0usize;

    for (statement, validated) in statements.iter().zip(validated) {
        let remaining = response_limit
            .checked_sub(response_size)
            .ok_or(DatabaseHostError::ResponseTooLarge)?;
        let result = match execute_statement(
            &mut transaction,
            statement,
            validated.returns_rows,
            remaining,
        )
        .await
        {
            Ok(result) => result,
            Err(original) => {
                return match transaction.rollback().await {
                    Ok(()) => Err(original),
                    Err(rollback) => Err(DatabaseHostError::Rollback {
                        original: Box::new(original),
                        rollback,
                    }),
                };
            }
        };
        response_size = response_size
            .checked_add(result_size(&result))
            .ok_or(DatabaseHostError::ResponseTooLarge)?;
        if response_size > response_limit {
            let original = DatabaseHostError::ResponseTooLarge;
            return match transaction.rollback().await {
                Ok(()) => Err(original),
                Err(rollback) => Err(DatabaseHostError::Rollback {
                    original: Box::new(original),
                    rollback,
                }),
            };
        }
        results.push(result);
    }

    transaction.commit().await?;
    Ok(results)
}

async fn set_plugin_context(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    schema: &str,
    statement_timeout: Duration,
) -> Result<(), DatabaseHostError> {
    let timeout_ms = u64::try_from(statement_timeout.as_millis()).unwrap_or(u64::MAX);
    let statement = format!(
        "SET LOCAL search_path TO \"{schema}\", pg_temp; SET LOCAL statement_timeout = {timeout_ms}; SET LOCAL lock_timeout = {timeout_ms}"
    );
    // `schema` comes only from a manifest-validated plugin ID via `schema_name`.
    // `timeout_ms` comes from the bounded host configuration.
    sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn execute_statement(
    connection: &mut PgConnection,
    statement: &Statement,
    returns_rows: bool,
    response_limit: usize,
) -> Result<QueryResult, DatabaseHostError> {
    // The parsed statement has passed `SqlSubsetValidator`; request values are exclusively bound.
    let query = bind_values(
        sqlx::query(sqlx::AssertSqlSafe(statement.sql.as_str())),
        &statement.params,
    )?;
    if returns_rows {
        stream_rows(connection, query, response_limit).await
    } else {
        let result = query.execute(connection).await?;
        Ok(QueryResult {
            rows_affected: result.rows_affected(),
            columns: Vec::new(),
            rows: Vec::new(),
        })
    }
}

async fn stream_rows<'query>(
    connection: &mut PgConnection,
    query: Query<'query, sqlx::Postgres, PgArguments>,
    response_limit: usize,
) -> Result<QueryResult, DatabaseHostError> {
    let mut rows = query.fetch(connection);
    let mut columns = Vec::new();
    let mut result_rows = Vec::new();
    let mut size = RESULT_OVERHEAD;

    while let Some(row) = rows.try_next().await? {
        if result_rows.len() == MAX_DATABASE_ROWS {
            return Err(DatabaseHostError::TooManyRows);
        }
        if columns.is_empty() {
            columns = row
                .columns()
                .iter()
                .map(|column| column.name().to_owned())
                .collect();
            size = size
                .checked_add(
                    columns
                        .iter()
                        .map(|column| column.len() + VALUE_OVERHEAD)
                        .sum(),
                )
                .ok_or(DatabaseHostError::ResponseTooLarge)?;
        }

        size = size
            .checked_add(ROW_OVERHEAD)
            .ok_or(DatabaseHostError::ResponseTooLarge)?;
        let mut values = Vec::with_capacity(row.len());
        for index in 0..row.len() {
            let value = database_value(&row, index)?;
            size = size
                .checked_add(VALUE_OVERHEAD + value_size(&value))
                .ok_or(DatabaseHostError::ResponseTooLarge)?;
            if size > response_limit {
                return Err(DatabaseHostError::ResponseTooLarge);
            }
            values.push(value);
        }
        result_rows.push(values);
    }

    Ok(QueryResult {
        rows_affected: u64::try_from(result_rows.len())
            .map_err(|_| DatabaseHostError::TooManyRows)?,
        columns,
        rows: result_rows,
    })
}

fn bind_values<'query>(
    mut query: Query<'query, sqlx::Postgres, PgArguments>,
    values: &[Value],
) -> Result<Query<'query, sqlx::Postgres, PgArguments>, DatabaseHostError> {
    for value in values {
        query = match value {
            Value::Null(NullType::Boolean) => query.bind(Option::<bool>::None),
            Value::Null(NullType::Integer) => query.bind(Option::<i64>::None),
            Value::Null(NullType::Float) => query.bind(Option::<f64>::None),
            Value::Null(NullType::Text) => query.bind(Option::<String>::None),
            Value::Null(NullType::Bytes) => query.bind(Option::<Vec<u8>>::None),
            Value::Null(NullType::Json) => {
                query.bind(Option::<sqlx::types::Json<serde_json::Value>>::None)
            }
            Value::Boolean(value) => query.bind(*value),
            Value::Integer(value) => query.bind(*value),
            Value::Float(value) => query.bind(*value),
            Value::Text(value) => query.bind(value),
            Value::Bytes(value) => query.bind(value),
            Value::Json(value) => {
                let value: serde_json::Value =
                    serde_json::from_str(value).map_err(|_| DatabaseHostError::InvalidParameter)?;
                query.bind(sqlx::types::Json(value))
            }
        };
    }
    Ok(query)
}

fn database_value(row: &PgRow, index: usize) -> Result<Value, DatabaseHostError> {
    if row.try_get_raw(index)?.is_null() {
        return Ok(Value::Null(null_type(
            row.columns()[index].type_info().name(),
        )?));
    }
    let type_name = row.columns()[index].type_info().name();
    match type_name {
        "BOOL" => row.try_get(index).map(Value::Boolean).map_err(Into::into),
        "INT2" => row
            .try_get::<i16, _>(index)
            .map(|value| Value::Integer(i64::from(value)))
            .map_err(Into::into),
        "INT4" => row
            .try_get::<i32, _>(index)
            .map(|value| Value::Integer(i64::from(value)))
            .map_err(Into::into),
        "INT8" => row.try_get(index).map(Value::Integer).map_err(Into::into),
        "FLOAT4" => row
            .try_get::<f32, _>(index)
            .map(|value| Value::Float(f64::from(value)))
            .map_err(Into::into),
        "FLOAT8" => row.try_get(index).map(Value::Float).map_err(Into::into),
        "BYTEA" => row.try_get(index).map(Value::Bytes).map_err(Into::into),
        "JSON" | "JSONB" => row
            .try_get::<sqlx::types::Json<serde_json::Value>, _>(index)
            .map(|value| Value::Json(value.0.to_string()))
            .map_err(Into::into),
        "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "UNKNOWN" => {
            row.try_get(index).map(Value::Text).map_err(Into::into)
        }
        _ => Err(DatabaseHostError::UnsupportedType(type_name.into())),
    }
}

fn null_type(type_name: &str) -> Result<NullType, DatabaseHostError> {
    match type_name {
        "BOOL" => Ok(NullType::Boolean),
        "INT2" | "INT4" | "INT8" => Ok(NullType::Integer),
        "FLOAT4" | "FLOAT8" => Ok(NullType::Float),
        "BYTEA" => Ok(NullType::Bytes),
        "JSON" | "JSONB" => Ok(NullType::Json),
        "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "UNKNOWN" => Ok(NullType::Text),
        _ => Err(DatabaseHostError::UnsupportedType(type_name.into())),
    }
}

fn value_size(value: &Value) -> usize {
    match value {
        Value::Null(_) => 1,
        Value::Boolean(_) => 1,
        Value::Integer(_) | Value::Float(_) => 8,
        Value::Text(value) | Value::Json(value) => value.len(),
        Value::Bytes(value) => value.len(),
    }
}

fn result_size(result: &QueryResult) -> usize {
    RESULT_OVERHEAD
        + result
            .columns
            .iter()
            .map(|column| column.len() + VALUE_OVERHEAD)
            .sum::<usize>()
        + result
            .rows
            .iter()
            .map(|row| {
                ROW_OVERHEAD
                    + row
                        .iter()
                        .map(|value| VALUE_OVERHEAD + value_size(value))
                        .sum::<usize>()
            })
            .sum::<usize>()
}

fn validate_placeholders(sql: &str, param_count: usize) -> Result<(), String> {
    let bytes = sql.as_bytes();
    let mut placeholders = HashSet::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }
        let start = index + 1;
        let mut end = start;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end == start || bytes[start] == b'0' {
            return Err("database statement contains an invalid placeholder".into());
        }
        let placeholder = sql[start..end]
            .parse::<usize>()
            .map_err(|_| "database statement contains an invalid placeholder".to_string())?;
        placeholders.insert(placeholder);
        index = end;
    }
    if placeholders.len() != param_count
        || !(1..=param_count).all(|placeholder| placeholders.contains(&placeholder))
    {
        return Err("database statement parameters do not match its placeholders".into());
    }
    Ok(())
}

#[derive(Default)]
struct SqlSubsetValidator {
    statement_count: usize,
}

impl Visitor for SqlSubsetValidator {
    type Break = Box<str>;

    fn pre_visit_statement(&mut self, _statement: &AstStatement) -> ControlFlow<Self::Break> {
        self.statement_count += 1;
        if self.statement_count > 1 {
            return ControlFlow::Break("data-modifying subqueries are not allowed".into());
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_relation(&mut self, relation: &ObjectName) -> ControlFlow<Self::Break> {
        let Some(identifier) = relation
            .0
            .as_slice()
            .first()
            .and_then(|part| part.as_ident())
        else {
            return ControlFlow::Break("dynamic relation names are not allowed".into());
        };
        if relation.0.len() != 1 || identifier.quote_style.is_some() {
            return ControlFlow::Break("qualified or quoted relation names are not allowed".into());
        }
        if identifier.value.to_ascii_lowercase().starts_with("pg_") {
            return ControlFlow::Break("PostgreSQL system relations are not allowed".into());
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_select(&mut self, select: &Select) -> ControlFlow<Self::Break> {
        if select.into.is_some() {
            return ControlFlow::Break("SELECT INTO is not allowed".into());
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<Self::Break> {
        match factor {
            TableFactor::Table {
                args,
                with_hints,
                version,
                with_ordinality,
                partitions,
                json_path,
                sample,
                index_hints,
                ..
            } if args.is_none()
                && with_hints.is_empty()
                && version.is_none()
                && !with_ordinality
                && partitions.is_empty()
                && json_path.is_none()
                && sample.is_none()
                && index_hints.is_empty() =>
            {
                ControlFlow::Continue(())
            }
            TableFactor::Derived { sample: None, .. } | TableFactor::NestedJoin { .. } => {
                ControlFlow::Continue(())
            }
            _ => ControlFlow::Break(
                "table functions and advanced table sources are not allowed".into(),
            ),
        }
    }

    fn pre_visit_expr(&mut self, expression: &Expr) -> ControlFlow<Self::Break> {
        match expression {
            Expr::CompoundIdentifier(_) | Expr::QualifiedWildcard(_, _) => {
                ControlFlow::Break("qualified identifiers are not allowed".into())
            }
            Expr::Function(function) => {
                let Some(identifier) = function
                    .name
                    .0
                    .as_slice()
                    .first()
                    .and_then(|part| part.as_ident())
                else {
                    return ControlFlow::Break("dynamic function names are not allowed".into());
                };
                if function.name.0.len() != 1
                    || identifier.quote_style.is_some()
                    || !SAFE_FUNCTIONS.contains(&identifier.value.to_ascii_lowercase().as_str())
                {
                    return ControlFlow::Break("database function is not allowed".into());
                }
                ControlFlow::Continue(())
            }
            _ => ControlFlow::Continue(()),
        }
    }
}

const SAFE_FUNCTIONS: &[&str] = &[
    "abs",
    "avg",
    "ceil",
    "ceiling",
    "char_length",
    "coalesce",
    "concat",
    "count",
    "floor",
    "greatest",
    "json_array_length",
    "jsonb_array_length",
    "jsonb_typeof",
    "least",
    "length",
    "lower",
    "max",
    "min",
    "octet_length",
    "round",
    "substring",
    "sum",
    "trim",
    "upper",
];

#[cfg(test)]
mod tests {
    use std::{
        env,
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    };

    use sqlx::{PgPool, Row};

    use super::{
        DatabaseHostError, NullType, Statement, Value, execute_statements, validate_statement,
    };

    static TEST_SCHEMA_ID: AtomicU64 = AtomicU64::new(0);

    fn statement(sql: &str, params: Vec<Value>) -> Statement {
        Statement {
            sql: sql.into(),
            params,
        }
    }

    #[test]
    fn accepts_parameterized_crud_and_safe_aggregates() {
        assert!(validate_statement(&statement("SELECT id FROM messages", vec![])).is_ok());
        assert!(
            validate_statement(&statement(
                "WITH recent AS (SELECT id FROM messages) SELECT id FROM recent",
                vec![],
            ))
            .is_ok()
        );
        assert!(
            validate_statement(&statement(
                "INSERT INTO messages (message) VALUES ($1) RETURNING id",
                vec![Value::Text("message".into())],
            ))
            .is_ok()
        );
        assert!(
            validate_statement(&statement(
                "SELECT count(id) FROM messages WHERE id > $1",
                vec![Value::Integer(0)],
            ))
            .is_ok()
        );
        assert!(
            validate_statement(&statement(
                "UPDATE messages SET message = $1 WHERE id = $2",
                vec![
                    Value::Text("message".into()),
                    Value::Null(NullType::Integer)
                ],
            ))
            .is_ok()
        );
    }

    #[test]
    fn rejects_schema_escapes_runtime_ddl_and_executable_query_parameters() {
        for sql in [
            "CREATE TABLE messages (id INT)",
            "SELECT id FROM public.users",
            "SELECT id INTO copied_messages FROM messages",
            "SELECT set_config($1, $2, $3)",
            "SELECT query_to_xml($1, $2, $3, $4)",
            "SELECT id FROM pg_authid",
            "SELECT id FROM messages; DELETE FROM messages",
        ] {
            let params = match sql {
                value if value.contains("query_to_xml") => vec![
                    Value::Text("SELECT * FROM public.auth_users".into()),
                    Value::Boolean(true),
                    Value::Boolean(false),
                    Value::Text(String::new()),
                ],
                value if value.contains("set_config") => vec![
                    Value::Text("search_path".into()),
                    Value::Text("public".into()),
                    Value::Boolean(true),
                ],
                _ => Vec::new(),
            };
            assert!(
                validate_statement(&statement(sql, params)).is_err(),
                "{sql}"
            );
        }

        assert!(
            validate_statement(&statement(
                "WITH inserted AS (INSERT INTO messages (message) VALUES ($1) RETURNING id) SELECT id FROM inserted",
                vec![Value::Text("message".into())],
            ))
            .is_err()
        );
    }

    #[test]
    fn requires_contiguous_matching_placeholders() {
        assert!(
            validate_statement(&statement(
                "SELECT id FROM messages WHERE id = $2",
                vec![Value::Integer(1)],
            ))
            .is_err()
        );
        assert!(
            validate_statement(&statement(
                "SELECT id FROM messages WHERE id = $1",
                vec![Value::Integer(1), Value::Integer(2)],
            ))
            .is_err()
        );
    }

    #[test]
    fn deeply_nested_sql_is_rejected_without_exhausting_the_stack() {
        let nesting = 2_048;
        let sql = format!("SELECT {}1{}", "(".repeat(nesting), ")".repeat(nesting));

        assert!(validate_statement(&statement(&sql, Vec::new())).is_err());
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL via DATABASE_URL"]
    async fn database_host_binds_limits_and_rolls_back() {
        let pool = PgPool::connect(&env::var("DATABASE_URL").expect("DATABASE_URL is configured"))
            .await
            .expect("database is reachable");
        let schema = format!(
            "nur_plugin_runtime_test_{}_{}",
            std::process::id(),
            TEST_SCHEMA_ID.fetch_add(1, Ordering::Relaxed)
        );
        let create = format!(
            "CREATE SCHEMA \"{schema}\"; CREATE TABLE \"{schema}\".messages (id BIGSERIAL PRIMARY KEY, message TEXT NOT NULL, optional_number BIGINT)"
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(create))
            .execute(&pool)
            .await
            .expect("test schema and table are created");

        let outcome = run_database_host_test(&pool, &schema).await;
        let drop = format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE");
        sqlx::query(sqlx::AssertSqlSafe(drop))
            .execute(&pool)
            .await
            .expect("test schema is removed");
        outcome.expect("database host behavior is correct");
    }

    async fn run_database_host_test(pool: &PgPool, schema: &str) -> Result<(), String> {
        let injected = "safe'); DROP TABLE messages; --";
        let insert = statement(
            "INSERT INTO messages (message, optional_number) VALUES ($1, $2) RETURNING id, message, optional_number",
            vec![Value::Text(injected.into()), Value::Null(NullType::Integer)],
        );
        let insert_validated = validate_statement(&insert)?;
        let inserted = execute_statements(
            pool,
            schema,
            &[insert],
            &[insert_validated],
            4096,
            Duration::from_secs(5),
        )
        .await
        .map_err(|error| error.to_string())?;
        let values_match = matches!(
            inserted[0].rows.as_slice(),
            [row] if matches!(
                row.as_slice(),
                [Value::Integer(1), Value::Text(message), Value::Null(NullType::Integer)]
                    if message == injected
            )
        );
        if !values_match {
            return Err("bound or returned values differ".into());
        }

        let rollback = [
            statement(
                "INSERT INTO messages (message) VALUES ($1)",
                vec![Value::Text("rollback".into())],
            ),
            statement(
                "INSERT INTO missing_messages (message) VALUES ($1)",
                vec![Value::Text("rollback".into())],
            ),
        ];
        let validated = rollback
            .iter()
            .map(validate_statement)
            .collect::<Result<Vec<_>, _>>()?;
        if execute_statements(
            pool,
            schema,
            &rollback,
            &validated,
            4096,
            Duration::from_secs(5),
        )
        .await
        .is_ok()
        {
            return Err("failing transaction unexpectedly committed".into());
        }

        let verify = format!("SELECT count(*) FROM \"{schema}\".messages WHERE message = $1");
        let count: i64 = sqlx::query(sqlx::AssertSqlSafe(verify))
            .bind("rollback")
            .fetch_one(pool)
            .await
            .map_err(|error| error.to_string())?
            .try_get(0)
            .map_err(|error| error.to_string())?;
        if count != 0 {
            return Err("failing transaction was not rolled back".into());
        }

        let core_query = statement("SELECT plugin_id FROM plugin_registry LIMIT 1", Vec::new());
        let core_query_validated = validate_statement(&core_query)?;
        if execute_statements(
            pool,
            schema,
            &[core_query],
            &[core_query_validated],
            4096,
            Duration::from_secs(5),
        )
        .await
        .is_ok()
        {
            return Err("plugin query unexpectedly resolved a core table".into());
        }

        let oversized_insert = statement(
            "INSERT INTO messages (message) VALUES ($1)",
            vec![Value::Text("x".repeat(4096))],
        );
        let oversized_validated = validate_statement(&oversized_insert)?;
        execute_statements(
            pool,
            schema,
            &[oversized_insert],
            &[oversized_validated],
            8192,
            Duration::from_secs(5),
        )
        .await
        .map_err(|error| error.to_string())?;
        let select = statement(
            "SELECT message FROM messages ORDER BY id DESC LIMIT 1",
            vec![],
        );
        let select_validated = validate_statement(&select)?;
        if !matches!(
            execute_statements(
                pool,
                schema,
                &[select],
                &[select_validated],
                128,
                Duration::from_secs(5),
            )
            .await,
            Err(DatabaseHostError::ResponseTooLarge)
        ) {
            return Err("oversized result was accepted".into());
        }

        Ok(())
    }
}
