use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use inquire::Select;
use serde::Deserialize;
use sqlx::postgres::PgPool;
use tokio::fs;
use tracing::{error, info, warn};

use crate::{
    CONFIG, PUBLIC_UPLOADS, STORAGE,
    db::{
        fields::{AuthUserFields, ContentTypeFields, LocaleFields, Table},
        handles,
        models::{ContentEntry, ContentType, Locale},
        queries::QueryObj,
    },
    file::processing::save_image,
    utils::errors::ServiceError,
};

#[derive(Debug, Deserialize)]
struct Frontmatter {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    draft: Option<bool>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    author: Option<Vec<String>>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    thumbnail: Option<String>,
}

pub async fn import_markdown(
    pool: &PgPool,
    path: PathBuf,
    ignore: Vec<PathBuf>,
    media_path: Option<PathBuf>,
) -> Result<(), ServiceError> {
    if !path.exists() {
        return Err(ServiceError::BadRequest(format!(
            "Path does not exist: {path:?}"
        )));
    }

    let query: QueryObj<ContentTypeFields> = QueryObj::default();
    let content_types =
        handles::select_record::<ContentTypeFields, ContentType>(pool, &Table::ContentTypes, query)
            .await?;
    let query: QueryObj<AuthUserFields> = QueryObj::default();
    let auth_users = handles::select_auth_user(pool, query).await?;

    let type_list: Vec<String> = content_types
        .results
        .iter()
        .map(|t| t.name.clone())
        .collect();
    let type_name = Select::new("Content Type:", type_list).prompt()?;
    let content_type = content_types
        .results
        .iter()
        .find(|t| t.name == type_name)
        .ok_or(ServiceError::NoContent)?;

    let user_list: Vec<String> = auth_users
        .results
        .iter()
        .filter_map(|t| t.last_name.clone())
        .collect();
    let user_name = Select::new("User:", user_list).prompt()?;
    let auth_user = auth_users
        .results
        .iter()
        .find(|t| t.last_name.as_ref() == Some(&user_name))
        .ok_or(ServiceError::NoContent)?;

    let query: QueryObj<LocaleFields> = QueryObj::default();
    let locales =
        handles::select_record::<LocaleFields, Locale>(pool, &Table::Locales, query).await?;

    let locale_list: Vec<String> = locales.results.iter().map(|l| l.code.clone()).collect();
    let locale_code = Select::new("Locale:", locale_list).prompt()?;
    let locale = locales
        .results
        .iter()
        .find(|l| l.code == locale_code)
        .ok_or(ServiceError::NoContent)?;

    let files = if path.is_file() {
        vec![path]
    } else {
        collect_markdown_files(&path, &ignore).await?
    };

    let mut count = 0;
    for file in files {
        match import_file(
            pool,
            &file,
            media_path.clone(),
            content_type.id,
            locale.id,
            auth_user.id.unwrap_or(1),
        )
        .await
        {
            Ok(_) => {
                count += 1;
                info!("✓ Imported: {}", file.display());
            }
            Err(e) => {
                error!("✗ Error importing {}: {e}", file.display());
            }
        }
    }

    info!("✓ Successfully imported {count} file(s)", count = count);
    Ok(())
}

fn should_ignore(path: &Path, ignores: &[PathBuf]) -> bool {
    let file_name = path.file_name();

    for ig in ignores {
        // 1) Filename match (single path component)
        if ig.components().count() == 1
            && let Some(name) = file_name
            && name == ig.as_os_str()
        {
            return true;
        }

        // 2) Path prefix match (dir or full file path)
        if path.starts_with(ig) {
            return true;
        }

        // 3) Best-effort canonical comparison if available
        if let (Ok(p_can), Ok(ig_can)) = (path.canonicalize(), ig.canonicalize())
            && p_can.starts_with(&ig_can)
        {
            return true;
        }
    }

    false
}

async fn collect_markdown_files(
    dir: &Path,
    ignores: &[PathBuf],
) -> Result<Vec<PathBuf>, ServiceError> {
    let mut files = Vec::new();
    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if should_ignore(&path, ignores) {
            continue;
        }

        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
            files.push(path);
        } else if path.is_dir() {
            let mut subfiles = Box::pin(collect_markdown_files(&path, ignores)).await?;
            files.append(&mut subfiles);
        }
    }

    Ok(files)
}

async fn import_file(
    pool: &PgPool,
    path: &Path,
    media_path: Option<PathBuf>,
    type_id: i32,
    locale_id: i32,
    created_by: i32,
) -> Result<(), ServiceError> {
    let content = fs::read_to_string(path).await?;

    let custom = markdown::Constructs {
        frontmatter: true,
        ..markdown::Constructs::gfm()
    };

    let options = markdown::ParseOptions {
        constructs: custom,
        ..markdown::ParseOptions::default()
    };

    let ast = markdown::to_mdast(&content, &options)?;

    // Extract YAML frontmatter from ast
    let yaml_str = ast.children().as_ref().and_then(|children| {
        children.iter().find_map(|node| {
            if let markdown::mdast::Node::Yaml(yaml) = node {
                Some(yaml.value.clone())
            } else {
                None
            }
        })
    });

    let frontmatter: Option<Frontmatter> =
        yaml_str.as_ref().and_then(|y| serde_yaml::from_str(y).ok());

    // Extract body (everything after frontmatter)
    let body = extract_body(&content);

    let (title, slug, status, created_at) = if let Some(ref fm) = frontmatter {
        let title = fm.title.clone().unwrap_or_else(|| "Untitled".to_string());
        let slug = fm
            .url
            .as_ref()
            .map(|u| extract_slug(u))
            .unwrap_or_else(|| slugify(&title));
        let status = if fm.draft.unwrap_or(false) {
            "draft"
        } else {
            "published"
        }
        .to_string();
        let created_at = fm
            .date
            .as_ref()
            .and_then(|d| d.parse::<DateTime<Utc>>().ok());
        (title, slug, status, created_at)
    } else {
        let (title, _) = parse_markdown(&content);
        let slug = slugify(&title);
        (title, slug, "draft".to_string(), None)
    };

    let mut entry = ContentEntry {
        type_id,
        locale_id,
        slug,
        title,
        text: body,
        status,
        created_by,
        updated_by: created_by,
        created_at,
        ..Default::default()
    };

    // Lookup category if present
    if let Some(ref fm) = frontmatter
        && let Some(ref category_name) = fm.category
    {
        if let Ok(Some(cat_id)) = lookup_or_create_category(pool, category_name, locale_id).await {
            entry.category_id = Some(cat_id);
        }

        // Lookup/create thumbnail media if present
        if let Some(ref thumb) = fm.thumbnail {
            match lookup_or_create_media(pool, thumb, media_path, created_at.as_ref(), created_by)
                .await
            {
                Ok(Some(media_id)) => {
                    entry.media_id = Some(media_id);
                }
                Err(e) => {
                    warn!("Failed to process media {}: {}", thumb, e);
                }
                _ => {}
            }
        }
    }

    let entry_id =
        handles::insert_record::<ContentEntry, i32>(pool, &Table::ContentEntries, &entry).await?;

    // Insert authors if present
    if let Some(ref fm) = frontmatter {
        if let Some(ref authors) = fm.author {
            for author_name in authors {
                if let Ok(Some(author_id)) = lookup_or_create_author(pool, author_name).await {
                    let _ = insert_entry_author(pool, entry_id, author_id).await;
                }
            }
        }

        // Insert tags if present
        if let Some(ref tags) = fm.tags {
            for tag_name in tags {
                if let Ok(Some(tag_id)) = lookup_or_create_tag(pool, tag_name).await {
                    let _ = insert_entry_tag(pool, entry_id, tag_id).await;
                }
            }
        }
    }

    Ok(())
}

fn parse_markdown(content: &str) -> (String, String) {
    let lines = content.lines();
    let mut title = String::new();
    let mut body_lines = Vec::new();

    for line in lines {
        if line.starts_with("# ") && title.is_empty() {
            title = line.trim_start_matches("# ").trim().to_string();
        } else {
            body_lines.push(line);
        }
    }

    let body = body_lines.join("\n").trim().to_string();

    if title.is_empty() {
        title = "Untitled".to_string();
    }

    (title, body)
}

fn extract_body(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut in_frontmatter = false;
    let mut frontmatter_end = 0;

    for (i, line) in lines.iter().enumerate() {
        if line.trim() == "---" {
            if in_frontmatter {
                frontmatter_end = i + 1;
                break;
            }
            in_frontmatter = true;
        }
    }

    if frontmatter_end > 0 {
        lines[frontmatter_end..].join("\n").trim().to_string()
    } else {
        content.to_string()
    }
}

fn extract_slug(url: &str) -> String {
    url.trim_start_matches('/')
        .trim_end_matches('/')
        .rsplit_once('/')
        .map(|(_, slug)| slug.to_string())
        .unwrap_or_else(|| slugify(url))
}

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else if c.is_whitespace() {
                '-'
            } else {
                '_'
            }
        })
        .collect::<String>()
        .replace('_', "-")
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

async fn lookup_or_create_category(
    pool: &PgPool,
    name: &str,
    locale_id: i32,
) -> Result<Option<i32>, sqlx::Error> {
    let existing: Option<i32> = sqlx::query_scalar(
        "SELECT id FROM content_categories WHERE name = $1 AND locale_id = $2 LIMIT 1",
    )
    .bind(name)
    .bind(locale_id)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = existing {
        return Ok(Some(id));
    }

    let slug = slugify(name);
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO content_categories (name, slug, locale_id, status) VALUES ($1, $2, $3, 'published') RETURNING id",
    )
    .bind(name)
    .bind(&slug)
    .bind(locale_id)
    .fetch_one(pool)
    .await?;

    Ok(Some(id))
}

async fn lookup_or_create_author(pool: &PgPool, name: &str) -> Result<Option<i32>, sqlx::Error> {
    let parts: Vec<&str> = name.splitn(2, ' ').collect();
    let (first_name, last_name) = match parts.as_slice() {
        [first, last] => (*first, *last),
        [single] => (*single, ""),
        _ => (name, ""),
    };

    let existing: Option<i32> = sqlx::query_scalar(
        "SELECT id FROM content_authors WHERE first_name = $1 AND last_name = $2 LIMIT 1",
    )
    .bind(first_name)
    .bind(last_name)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = existing {
        return Ok(Some(id));
    }

    let slug = slugify(name);
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO content_authors (first_name, last_name, slug) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(first_name)
    .bind(last_name)
    .bind(&slug)
    .fetch_one(pool)
    .await?;

    Ok(Some(id))
}

async fn lookup_or_create_tag(pool: &PgPool, name: &str) -> Result<Option<i32>, sqlx::Error> {
    let slug = slugify(name);

    let existing: Option<i32> =
        sqlx::query_scalar("SELECT id FROM content_tags WHERE slug = $1 LIMIT 1")
            .bind(&slug)
            .fetch_optional(pool)
            .await?;

    if let Some(id) = existing {
        return Ok(Some(id));
    }

    let id: i32 =
        sqlx::query_scalar("INSERT INTO content_tags (name, slug) VALUES ($1, $2) RETURNING id")
            .bind(name)
            .bind(&slug)
            .fetch_one(pool)
            .await?;

    Ok(Some(id))
}

async fn lookup_or_create_media(
    pool: &PgPool,
    thumbnail_path: &str,
    media_path: Option<PathBuf>,
    date: Option<&DateTime<Utc>>,
    user_id: i32,
) -> Result<Option<i32>, Box<dyn std::error::Error>> {
    let thumb = thumbnail_path
        .trim_start_matches('/')
        .split('/')
        .skip(1)
        .collect::<Vec<_>>()
        .join("/");
    let source_path = media_path.unwrap_or(PathBuf::from("./")).join(thumb);

    let filename = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid filename")?;

    // Determine target directory from date: /YYYY/MM/
    let (year, month) = if let Some(d) = date {
        (d.format("%Y").to_string(), d.format("%m").to_string())
    } else {
        let now = Utc::now();
        (now.format("%Y").to_string(), now.format("%m").to_string())
    };

    let target_dir = format!("{PUBLIC_UPLOADS}/{year}/{month}");
    let target_path = PathBuf::from(STORAGE.as_str()).join(&year).join(&month);

    // Check if media already exists
    let existing: Option<i32> =
        sqlx::query_scalar("SELECT id FROM media WHERE path = $1 AND filename = $2 LIMIT 1")
            .bind(&target_dir)
            .bind(filename)
            .fetch_optional(pool)
            .await?;

    if let Some(id) = existing {
        return Ok(Some(id));
    }

    // Try to copy file from source to target
    // let source_full = PathBuf::from("./").join(&source_path);
    let target_file = target_path.join(filename);

    // Create target directory if it doesn't exist
    if !target_path.exists() {
        fs::create_dir_all(&target_path).await.ok();
    }

    // Copy file
    if source_path.exists() {
        match fs::copy(&source_path, &target_file).await {
            Ok(_) => {
                info!(
                    "Copied media: {} → {}",
                    source_path.display(),
                    target_file.display()
                );
            }
            Err(e) => {
                warn!("Failed to copy media {source_path:?}: {e}");
                return Ok(None);
            }
        }
    } else {
        warn!("Source media not found: {source_path:?}");
        return Ok(None);
    }

    // Get file metadata
    let metadata = fs::metadata(&target_file).await?;
    let size = metadata.len() as i64;

    // Get image dimensions if it's an image
    let (width, height) = if let Ok(img) = image::open(&target_file) {
        (Some(img.width() as i32), Some(img.height() as i32))
    } else {
        (None, None)
    };

    let mime_type = mime_guess::from_path(&target_file)
        .first_or_octet_stream()
        .to_string();

    // Insert media record
    let alt_text = target_file
        .file_stem()
        .and_then(|f| f.to_str())
        .unwrap_or(filename);

    let id: i32 = sqlx::query_scalar(
        "INSERT INTO media (alt, filename, path, type, width, height, size, created_at, uploaded_by) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
    )
    .bind(alt_text)
    .bind(filename)
    .bind(&target_dir)
    .bind(&mime_type)
    .bind(width)
    .bind(height)
    .bind(size)
    .bind(date)
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    // Generate variants for images
    if mime_type.starts_with("image") {
        let config = CONFIG.read().await;
        let resolutions = config.image_resolutions.clone().unwrap_or_default();
        let extensions = config.image_extensions.clone().unwrap_or_default();
        drop(config);

        if !resolutions.is_empty() && !extensions.is_empty() {
            match save_image(resolutions, &extensions, &target_file, None) {
                Ok(variants) => {
                    for (w, h, variant_filename) in variants {
                        let _ = sqlx::query(
                            "INSERT INTO media_variants (media_id, width, height, filename) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
                        )
                        .bind(id)
                        .bind(w)
                        .bind(h)
                        .bind(&variant_filename)
                        .execute(pool)
                        .await;

                        info!("Created variant: {}", variant_filename);
                    }
                }
                Err(e) => {
                    warn!("Failed to generate variants for media {}: {}", id, e);
                }
            }
        }
    }

    Ok(Some(id))
}

async fn insert_entry_author(
    pool: &PgPool,
    entry_id: i32,
    author_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO content_entry_authors (entry_id, author_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(entry_id)
        .bind(author_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn insert_entry_tag(pool: &PgPool, entry_id: i32, tag_id: i32) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO content_entry_tags (entry_id, tag_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(entry_id)
    .bind(tag_id)
    .execute(pool)
    .await?;
    Ok(())
}
