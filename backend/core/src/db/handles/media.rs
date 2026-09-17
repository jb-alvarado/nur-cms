use chrono::{DateTime, Utc};
use sqlx::{Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;

#[cfg(debug_assertions)]
use tracing::debug;

use crate::{
    db::{
        fields::MediaFields,
        queries::{QueryObj, RespondObj, WhereBuilder},
        serialize::{MediaSerializer, MediaVariantSerializer, MediaVideoVariantSerializer},
    },
    utils::errors::NurError,
};

#[cfg(debug_assertions)]
use crate::db::format_sql;

pub async fn select_media(
    pool: &PgPool,
    query_obj: &QueryObj<MediaFields>,
) -> Result<RespondObj<MediaSerializer>, NurError> {
    let ordering_with_alias = |alias: &str| {
        query_obj
            .ordering
            .split(',')
            .filter_map(|part| {
                let mut split = part.split_whitespace();
                let field = split.next()?.trim();
                let direction = split.next().unwrap_or("ASC").to_uppercase();

                if direction != "ASC" && direction != "DESC" {
                    return None;
                }

                if MediaFields::iter().any(|f| {
                    f.to_string() == field
                        && !matches!(f, MediaFields::MediaVariants | MediaFields::VideoVariants)
                }) {
                    Some(format!("{alias}.{field} {direction}"))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    let page_ordering = ordering_with_alias("f");
    let outer_ordering = ordering_with_alias("p");

    let mut query_builder: QueryBuilder<Postgres> =
        QueryBuilder::new("WITH filtered AS NOT MATERIALIZED ( SELECT m.* FROM media m ");

    let mut where_chain = WhereBuilder::new(query_builder);

    if let Some(id) = &query_obj.search_id {
        where_chain.push_and_bind(None, "m.id = ", id, None);
    }

    if let Some(search) = query_obj.search.clone() {
        where_chain.push_and_bind(
            None,
            "m.filename ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );
    }

    if let Some(media_type) = &query_obj.media_type {
        let array: Vec<String> = media_type.iter().map(|t| format!("{t}/%")).collect();
        where_chain.push_and_bind(None, "m.type LIKE ANY(", array, Some(")"));
    }

    query_builder = where_chain.into_inner();

    query_builder.push(" ), page AS ( SELECT f.* FROM filtered f");

    if !page_ordering.is_empty() {
        query_builder.push(format!(" ORDER BY {}", page_ordering));
    }

    query_builder.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    query_builder.push(" ), total AS ( SELECT COUNT(*) AS total_count FROM filtered ) SELECT ");

    let mut sep = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            MediaFields::MediaVariants => sep.push("COALESCE(variants.data, NULL) AS \"variants\""),
            MediaFields::VideoVariants => {
                sep.push("COALESCE(video_variants.data, NULL) AS \"video_variants\"")
            }
            _ => sep.push(format!("p.{f}")),
        };
    }

    sep.push("t.total_count");
    sep.push_unseparated(" ");
    query_builder.push("FROM page p CROSS JOIN total t ");

    if query_obj.fields.contains(&MediaFields::MediaVariants) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT json_agg(
                    json_build_object(
                        'id', mv.id,
                        'width', mv.width,
                        'height', mv.height,
                        'filename', mv.filename
                    )
                    ORDER BY mv.id
                ) AS data
                FROM media_variants mv
                WHERE mv.media_id = p.id
            ) AS variants ON TRUE "#,
        );
    }

    if query_obj.fields.contains(&MediaFields::VideoVariants) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT json_agg(
                    json_build_object(
                        'id', vv.id,
                        'kind', vv.kind,
                        'profile', vv.profile,
                        'width', vv.width,
                        'height', vv.height,
                        'container', vv.container,
                        'video_codec', vv.video_codec,
                        'audio_codec', vv.audio_codec,
                        'filename', vv.filename,
                        'size', vv.size,
                        'duration_ms', vv.duration_ms
                    ) ORDER BY vv.height, vv.id
                ) AS data
                FROM media_video_variants vv
                WHERE vv.media_id = p.id
            ) AS video_variants ON TRUE "#,
        );
    }

    if !outer_ordering.is_empty() {
        query_builder.push(format!(" ORDER BY {}", outer_ordering));
    }

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query_builder.sql()));

    let query = query_builder.build_query_as::<MediaSerializer>();

    let data: Vec<MediaSerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}

#[derive(Debug)]
pub struct ExistingUploadMedia {
    pub id: i32,
    pub upload_id: Option<String>,
    pub uploaded_by: Option<i32>,
}

#[derive(Debug)]
pub struct ExistingImportedMedia {
    pub id: i32,
    pub mime_type: Option<String>,
    pub processing_status: String,
}

pub struct NewUploadedMedia<'a> {
    pub alt: &'a str,
    pub filename: &'a str,
    pub path: &'a str,
    pub mime_type: &'a str,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub size: Option<i64>,
    pub uploaded_by: i32,
    pub upload_id: &'a str,
    pub processing_status: &'a str,
}

pub struct NewImportedMedia<'a> {
    pub alt: &'a str,
    pub filename: &'a str,
    pub path: &'a str,
    pub mime_type: &'a str,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub size: i64,
    pub uploaded_by: i32,
    pub created_at: &'a DateTime<Utc>,
}

pub async fn find_upload_media(
    pool: &PgPool,
    path: &str,
    filename: &str,
) -> Result<Option<ExistingUploadMedia>, sqlx::Error> {
    let row = sqlx::query_as::<_, (i32, Option<String>, Option<i32>)>(
        "SELECT id, upload_id, uploaded_by FROM media WHERE filename = $1 AND path = $2",
    )
    .bind(filename)
    .bind(path)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(id, upload_id, uploaded_by)| ExistingUploadMedia {
        id,
        upload_id,
        uploaded_by,
    }))
}

pub async fn delete_media(pool: &PgPool, media_id: i32) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM media WHERE id = $1")
        .bind(media_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn insert_uploaded_media(
    pool: &PgPool,
    media: &NewUploadedMedia<'_>,
) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar(
        r#"INSERT INTO media
               (alt, filename, path, type, width, height, size, uploaded_by, upload_id, processing_status)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
           ON CONFLICT (path, filename) DO NOTHING
           RETURNING id"#,
    )
    .bind(media.alt)
    .bind(media.filename)
    .bind(media.path)
    .bind(media.mime_type)
    .bind(media.width)
    .bind(media.height)
    .bind(media.size)
    .bind(media.uploaded_by)
    .bind(media.upload_id)
    .bind(media.processing_status)
    .fetch_optional(pool)
    .await
}

pub async fn find_owned_upload_media(
    pool: &PgPool,
    media: &NewUploadedMedia<'_>,
) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM media WHERE path = $1 AND filename = $2 AND upload_id = $3 AND uploaded_by = $4",
    )
    .bind(media.path)
    .bind(media.filename)
    .bind(media.upload_id)
    .bind(media.uploaded_by)
    .fetch_optional(pool)
    .await
}

pub async fn insert_imported_media(
    pool: &PgPool,
    media: &NewImportedMedia<'_>,
) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO media (alt, filename, path, type, width, height, size, created_at, uploaded_by) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
    )
    .bind(media.alt)
    .bind(media.filename)
    .bind(media.path)
    .bind(media.mime_type)
    .bind(media.width)
    .bind(media.height)
    .bind(media.size)
    .bind(media.created_at)
    .bind(media.uploaded_by)
    .fetch_one(pool)
    .await
}

pub async fn find_imported_media(
    pool: &PgPool,
    path: &str,
    filename: &str,
) -> Result<Option<ExistingImportedMedia>, sqlx::Error> {
    let row = sqlx::query_as::<_, (i32, Option<String>, String)>(
        "SELECT id, type, processing_status FROM media WHERE path = $1 AND filename = $2 LIMIT 1",
    )
    .bind(path)
    .bind(filename)
    .fetch_optional(pool)
    .await?;

    Ok(
        row.map(|(id, mime_type, processing_status)| ExistingImportedMedia {
            id,
            mime_type,
            processing_status,
        }),
    )
}

pub async fn insert_media_variants(
    pool: &PgPool,
    media_id: i32,
    variants: &[(i32, i32, String)],
) -> Result<(), sqlx::Error> {
    for (width, height, filename) in variants {
        insert_media_variant(pool, media_id, *width, *height, filename).await?;
    }

    Ok(())
}

pub async fn insert_media_variant(
    pool: &PgPool,
    media_id: i32,
    width: i32,
    height: i32,
    filename: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO media_variants (media_id, width, height, filename)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (media_id, width, height, filename) DO NOTHING"#,
    )
    .bind(media_id)
    .bind(width)
    .bind(height)
    .bind(filename)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn media_variant_count(pool: &PgPool, media_id: i32) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM media_variants WHERE media_id = $1")
        .bind(media_id)
        .fetch_one(pool)
        .await
}

pub async fn update_media(
    pool: &PgPool,
    media_id: i32,
    filename: Option<&str>,
    alt: Option<Option<&str>>,
    variants: &[MediaVariantSerializer],
    video_variants: &[MediaVideoVariantSerializer],
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;

    for variant in variants {
        sqlx::query("UPDATE media_variants SET filename = $1 WHERE id = $2")
            .bind(&variant.filename)
            .bind(variant.id)
            .execute(&mut *transaction)
            .await?;
    }

    for variant in video_variants {
        sqlx::query("UPDATE media_video_variants SET filename = $1 WHERE id = $2")
            .bind(&variant.filename)
            .bind(variant.id)
            .execute(&mut *transaction)
            .await?;
    }

    if let Some(filename) = filename {
        sqlx::query("UPDATE media SET filename = $1 WHERE id = $2")
            .bind(filename)
            .bind(media_id)
            .execute(&mut *transaction)
            .await?;
    }

    if let Some(alt) = alt {
        sqlx::query("UPDATE media SET alt = $1 WHERE id = $2")
            .bind(alt)
            .bind(media_id)
            .execute(&mut *transaction)
            .await?;
    }

    transaction.commit().await
}

pub async fn media_is_video(pool: &PgPool, media_id: i32) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM media WHERE id = $1 AND type LIKE 'video/%')")
        .bind(media_id)
        .fetch_one(pool)
        .await
}

pub async fn media_is_raster_image(pool: &PgPool, media_id: i32) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM media WHERE id = $1 AND type IN ('image/avif', 'image/gif', 'image/jpeg', 'image/jpg', 'image/png', 'image/webp'))",
    )
    .bind(media_id)
    .fetch_one(pool)
    .await
}

pub async fn media_type_and_processing_status(
    pool: &PgPool,
    media_id: i32,
) -> Result<Option<(Option<String>, String)>, sqlx::Error> {
    sqlx::query_as("SELECT type, processing_status FROM media WHERE id = $1")
        .bind(media_id)
        .fetch_optional(pool)
        .await
}
