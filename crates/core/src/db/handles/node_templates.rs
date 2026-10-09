use sqlx::postgres::PgPool;

use crate::{db::models::ContentNodeTemplate, utils::errors::NurError};

fn template_write_error(error: sqlx::Error) -> NurError {
    if error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "23505")
    {
        NurError::Conflict("A node template with this name already exists".into())
    } else {
        error.into()
    }
}

/// Inserts a node template while binding its schema as one JSONB document.
///
/// A schema is represented as a Rust vector, but PostgreSQL stores it in a
/// `jsonb` column. Binding the serialized JSON value explicitly prevents SQLx
/// from interpreting the vector as a PostgreSQL `jsonb[]` array.
pub async fn insert_node_template(
    pool: &PgPool,
    template: &ContentNodeTemplate,
) -> Result<i32, NurError> {
    let schema = serde_json::to_value(&template.schema)?;

    sqlx::query_scalar(
        "INSERT INTO content_node_templates (name, data, schema) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(&template.name)
    .bind(&template.data)
    .bind(schema)
    .fetch_one(pool)
    .await
    .map_err(template_write_error)
}

/// Updates a node template while binding its schema as one JSONB document.
pub async fn update_node_template(
    pool: &PgPool,
    id: i32,
    template: &ContentNodeTemplate,
) -> Result<(), NurError> {
    let schema = serde_json::to_value(&template.schema)?;

    let result = sqlx::query(
        "UPDATE content_node_templates SET name = $1, data = $2, schema = $3 WHERE id = $4",
    )
    .bind(&template.name)
    .bind(&template.data)
    .bind(schema)
    .bind(id)
    .execute(pool)
    .await
    .map_err(template_write_error)?;

    if result.rows_affected() == 0 {
        return Err(NurError::NotFound);
    }

    Ok(())
}
