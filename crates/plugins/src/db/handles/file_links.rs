use sqlx::{PgPool, Row, query, query_scalar};

pub(crate) struct DownloadLink {
    pub id: i64,
    pub directory_id: String,
    pub filename: String,
}

pub(crate) struct ReservedUploadLink {
    pub id: i64,
    pub filename: String,
    pub maximum_size: i64,
    pub storage_path: String,
    pub finalizing: bool,
}

pub(crate) struct NewFileLink<'a> {
    pub plugin_id: &'a str,
    pub directory_id: &'a str,
    pub purpose: &'a str,
    pub token_hash: Vec<u8>,
    pub filename: String,
    pub maximum_size: Option<i64>,
    pub expires_seconds: i32,
}

pub(crate) async fn cleanup(pool: &PgPool, upload_session_seconds: i32) -> Result<(), sqlx::Error> {
    query(
        "DELETE FROM public.plugin_file_links \
         WHERE (upload_id IS NULL AND expires_at <= now()) \
            OR consumed_at <= now() - interval '1 day' \
            OR (upload_id IS NOT NULL AND consumed_at IS NULL \
                AND claimed_at <= now() - make_interval(secs => $1))",
    )
    .bind(upload_session_seconds)
    .execute(pool)
    .await?;

    Ok(())
}

pub(crate) async fn find_download(
    pool: &PgPool,
    plugin_id: &str,
    token_hash: Vec<u8>,
) -> Result<Option<DownloadLink>, sqlx::Error> {
    let row = query(
        "SELECT id, directory_id, filename FROM public.plugin_file_links \
         WHERE plugin_id = $1 AND purpose = 'download' AND token_hash = $2 \
           AND consumed_at IS NULL AND expires_at > now()",
    )
    .bind(plugin_id)
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        Ok(DownloadLink {
            id: row.try_get("id")?,
            directory_id: row.try_get("directory_id")?,
            filename: row.try_get("filename")?,
        })
    })
    .transpose()
}

pub(crate) async fn consume_download(pool: &PgPool, link_id: i64) -> Result<bool, sqlx::Error> {
    let result = query(
        "UPDATE public.plugin_file_links SET consumed_at = now() \
         WHERE id = $1 AND purpose = 'download' AND consumed_at IS NULL AND expires_at > now()",
    )
    .bind(link_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

pub(crate) async fn upload_directory_id(
    pool: &PgPool,
    plugin_id: &str,
    token_hash: Vec<u8>,
) -> Result<Option<String>, sqlx::Error> {
    query_scalar(
        "SELECT directory_id FROM public.plugin_file_links \
         WHERE plugin_id = $1 AND purpose = 'upload' AND token_hash = $2",
    )
    .bind(plugin_id)
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

pub(crate) async fn reserve_upload(
    pool: &PgPool,
    plugin_id: &str,
    token_hash: Vec<u8>,
    total_size: i64,
    upload_id: &str,
    upload_session_seconds: i32,
    storage_path: &str,
) -> Result<Option<ReservedUploadLink>, sqlx::Error> {
    let row = query(
        "UPDATE public.plugin_file_links \
         SET upload_id = COALESCE(upload_id, $4), claimed_at = COALESCE(claimed_at, now()), \
             storage_path = COALESCE(storage_path, $6) \
         WHERE plugin_id = $1 AND purpose = 'upload' AND token_hash = $2 \
           AND consumed_at IS NULL \
           AND ((upload_id IS NULL AND expires_at > now()) \
                OR (upload_id = $4 AND claimed_at > now() - make_interval(secs => $5))) \
           AND (upload_id IS NULL OR upload_id = $4) \
           AND max_size IS NOT NULL AND max_size >= $3 \
         RETURNING id, filename, max_size, storage_path, finalizing_at IS NOT NULL AS finalizing",
    )
    .bind(plugin_id)
    .bind(token_hash)
    .bind(total_size)
    .bind(upload_id)
    .bind(upload_session_seconds)
    .bind(storage_path)
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        Ok(ReservedUploadLink {
            id: row.try_get("id")?,
            filename: row.try_get("filename")?,
            maximum_size: row.try_get("max_size")?,
            storage_path: row.try_get("storage_path")?,
            finalizing: row.try_get("finalizing")?,
        })
    })
    .transpose()
}

pub(crate) async fn begin_upload_finalization(
    pool: &PgPool,
    link_id: i64,
    upload_id: &str,
) -> Result<bool, sqlx::Error> {
    let result = query(
        "UPDATE public.plugin_file_links SET finalizing_at = COALESCE(finalizing_at, now()) \
         WHERE id = $1 AND purpose = 'upload' AND upload_id = $2 AND consumed_at IS NULL",
    )
    .bind(link_id)
    .bind(upload_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

pub(crate) async fn complete_upload(
    pool: &PgPool,
    link_id: i64,
    upload_id: &str,
) -> Result<bool, sqlx::Error> {
    let result = query(
        "UPDATE public.plugin_file_links SET consumed_at = now() \
         WHERE id = $1 AND purpose = 'upload' AND upload_id = $2 \
           AND finalizing_at IS NOT NULL AND consumed_at IS NULL",
    )
    .bind(link_id)
    .bind(upload_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

pub(crate) async fn create(pool: &PgPool, link: NewFileLink<'_>) -> Result<(), sqlx::Error> {
    query(
        "INSERT INTO public.plugin_file_links \
         (plugin_id, directory_id, purpose, token_hash, filename, max_size, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now() + make_interval(secs => $7))",
    )
    .bind(link.plugin_id)
    .bind(link.directory_id)
    .bind(link.purpose)
    .bind(link.token_hash)
    .bind(link.filename)
    .bind(link.maximum_size)
    .bind(link.expires_seconds)
    .execute(pool)
    .await?;

    Ok(())
}
