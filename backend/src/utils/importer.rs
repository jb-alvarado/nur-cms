/// Markdown importer module for bulk importing content from markdown files with YAML frontmatter.
///
/// This module provides functionality to:
/// - Import markdown files with optional YAML frontmatter containing metadata
/// - Extract and process frontmatter fields (title, date, draft status, author, category, tags, etc.)
/// - Handle inline image syntax with custom DOM parsing for `{{ <img> }}` elements
/// - Manage media files (copy, resize, and track in database)
/// - Create or link associated database records (categories, authors, tags, media entries)
///
/// # Import Workflow
///
/// 1. Collects markdown files from a directory (recursively or single file)
/// 2. Extracts YAML frontmatter and markdown body from each file
/// 3. Parses custom image syntax `{{ <img src="..." alt="..." caption="..." align="..." /> }}`
/// 4. Resolves and copies media files to dated storage directories (YYYY/MM)
/// 5. Converts markdown to an AST and links embedded media
/// 6. Creates ContentEntry record with title, slug, and content
/// 7. Inserts associated metadata: category, thumbnail, authors, tags, event dates
///
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use colored::Colorize;
use html_parser::Dom;
use inquire::Select;
use serde::Deserialize;
use serde_json::Value;
use sqlx::postgres::PgPool;
use tokio::fs;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    CONFIG, PUBLIC_UPLOADS, STORAGE,
    db::{
        fields::{AuthUserFields, ContentTypeFields, LocaleFields, Table},
        handles,
        models::{ContentEntry, ContentMeta, ContentType, Locale},
        queries::QueryObj,
    },
    file::processing::save_image,
    utils::{ast_serialize::persist_content_media, errors::NurError},
};

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub content_type_id: Option<i32>,
    pub locale_id: Option<i32>,
    pub created_by: Option<i32>,
    pub media_root: Option<PathBuf>,
    pub ignores: Vec<PathBuf>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum AuthorField {
    Single(String),
    List(Vec<String>),
}

#[derive(Clone, Debug, Default, Deserialize)]
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
    author: Option<AuthorField>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(default)]
    event_start: Option<String>,
    #[serde(default)]
    event_end: Option<String>,
}

pub async fn import_markdown(
    pool: &PgPool,
    path: PathBuf,
    ignore: Vec<PathBuf>,
    media_path: Option<PathBuf>,
) -> Result<(), NurError> {
    // Prepare options, will prompt for missing values.
    let mut opts = ImportOptions {
        content_type_id: None,
        locale_id: None,
        created_by: None,
        media_root: media_path,
        ignores: ignore,
    };

    if !path.exists() {
        return Err(NurError::BadRequest(format!(
            "Path does not exist: {path:?}"
        )));
    }

    prompt_missing_options(pool, &mut opts).await?;

    let mut files = if path.is_file() {
        vec![path]
    } else {
        collect_markdown_files(&path, &opts.ignores).await?
    };

    files.sort();

    let mut count = 0;
    let total_count = files.len();
    for file in files {
        match import_file(pool, &file, &opts).await {
            Ok(_) => {
                count += 1;
                info!("✓ Imported {count} of {total_count}: {file:?}");
            }
            Err(e) => {
                error!("✗ Error importing {file:?}: {e}");
            }
        }
    }

    info!("✓ Successfully imported {count} file(s)");
    Ok(())
}

async fn prompt_missing_options(pool: &PgPool, opts: &mut ImportOptions) -> Result<(), NurError> {
    if opts.content_type_id.is_none() {
        let query: QueryObj<ContentTypeFields> = QueryObj::default();
        let content_types = handles::select_record::<ContentTypeFields, ContentType>(
            pool,
            &Table::ContentTypes,
            query,
        )
        .await?;

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
            .ok_or(NurError::NoContent)?;
        opts.content_type_id = Some(content_type.id);
    }

    if opts.created_by.is_none() {
        let query: QueryObj<AuthUserFields> = QueryObj::default();
        let auth_users = handles::select_auth_user(pool, query).await?;
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
            .ok_or(NurError::NoContent)?;
        opts.created_by = auth_user.id;
    }

    if opts.locale_id.is_none() {
        let query: QueryObj<LocaleFields> = QueryObj::default();
        let locales =
            handles::select_record::<LocaleFields, Locale>(pool, &Table::Locales, query).await?;
        let locale_list: Vec<String> = locales.results.iter().map(|l| l.code.clone()).collect();
        let locale_code = Select::new("Locale:", locale_list).prompt()?;
        let locale = locales
            .results
            .iter()
            .find(|l| l.code == locale_code)
            .ok_or(NurError::NoContent)?;
        opts.locale_id = Some(locale.id);
    }

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

async fn collect_markdown_files(dir: &Path, ignores: &[PathBuf]) -> Result<Vec<PathBuf>, NurError> {
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

async fn insert_meta(pool: &PgPool, type_id: i32, fm: &Frontmatter) -> Result<(), NurError> {
    let start_time = fm
        .event_start
        .as_ref()
        .and_then(|s| s.parse::<DateTime<Utc>>().ok());
    let end_time = fm
        .event_end
        .as_ref()
        .and_then(|s| s.parse::<DateTime<Utc>>().ok());
    let meta = ContentMeta {
        id: 0,
        entry_id: type_id,
        data: None,
        start_time,
        end_time,
        total_count: None,
    };

    handles::insert_record::<ContentMeta, i32>(pool, &Table::ContentMeta, &meta).await?;

    Ok(())
}

async fn import_file(pool: &PgPool, path: &Path, opts: &ImportOptions) -> Result<(), NurError> {
    let content = fs::read_to_string(path).await?;

    // Extract body and fallback title early, ensuring media for inline images
    let (frontmatter, created_at, body, fallback_title) = extract_body_and_title(
        &content,
        pool,
        opts.media_root.as_deref(),
        path.parent().unwrap_or(Path::new(".")),
        opts.created_by
            .ok_or(NurError::BadRequest("Missing created_by".into()))?,
    )
    .await?;

    let ast = markdown::to_mdast(&body, &markdown::ParseOptions::default())?;

    let (title, slug, status) = if let Some(ref fm) = frontmatter {
        let title = fm.title.clone().unwrap_or_else(|| fallback_title.clone());
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
        (title, slug, status)
    } else {
        let slug = slugify(&fallback_title);
        (fallback_title, slug, "draft".to_string())
    };

    let mut entry = ContentEntry {
        type_id: opts
            .content_type_id
            .ok_or(NurError::BadRequest("Missing content_type_id".into()))?,
        locale_id: opts
            .locale_id
            .ok_or(NurError::BadRequest("Missing locale_id".into()))?,
        slug,
        title,
        text: body,
        status,
        created_by: opts
            .created_by
            .ok_or(NurError::BadRequest("Missing created_by".into()))?,
        updated_by: opts
            .created_by
            .ok_or(NurError::BadRequest("Missing created_by".into()))?,
        created_at: Some(created_at),
        ..Default::default()
    };

    // Lookup category if present
    if let Some(ref fm) = frontmatter
        && let Some(ref category_name) = fm.category
    {
        if let Ok(Some(cat_id)) =
            lookup_or_create_category(pool, category_name, entry.locale_id).await
        {
            entry.category_id = Some(cat_id);
        }

        // Lookup/create thumbnail media if present
        if let Some(ref thumb) = fm.thumbnail {
            match ensure_media(
                pool,
                thumb,
                opts.media_root.as_deref(),
                path.parent().unwrap_or(Path::new(".")),
                &created_at,
                entry.created_by,
            )
            .await
            {
                Ok(Some(media_id)) => {
                    entry.media_id = Some(media_id);
                }
                Err(e) => {
                    warn!("Failed to process media {thumb}: {e}");
                }
                _ => {}
            }
        }
    }

    let entry_id =
        handles::insert_record::<ContentEntry, i32>(pool, &Table::ContentEntries, &entry).await?;

    // Build AST from body content and persist content_media links (with positions)
    let tree: Value = serde_json::to_value(ast).unwrap_or_default();
    persist_content_media(pool, entry_id, &tree).await?;

    // Insert authors/meta/tags if present
    if let Some(ref fm) = frontmatter {
        if entry.type_id == 3 {
            insert_meta(pool, entry_id, fm).await?;
        }

        if let Some(ref authors) = fm.author {
            match authors {
                AuthorField::List(list) => {
                    for author_name in list {
                        if let Ok(Some(author_id)) =
                            lookup_or_create_author(pool, author_name, created_at).await
                        {
                            let _ = insert_entry_author(pool, entry_id, author_id).await;
                        }
                    }
                }
                AuthorField::Single(name) => {
                    if let Ok(Some(author_id)) =
                        lookup_or_create_author(pool, name, created_at).await
                    {
                        let _ = insert_entry_author(pool, entry_id, author_id).await;
                    }
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

fn build_picture_tag(fallback_src: &str, alt: &str, variants: Vec<(i32, i32, String)>) -> String {
    let mut sources = String::new();

    // Group variants by breakpoint (width)
    let mut breakpoints: Vec<i32> = variants.iter().map(|(w, _, _)| *w).collect();
    breakpoints.sort_by(|a, b| b.cmp(a)); // Descending order
    breakpoints.dedup();

    for bp in breakpoints {
        let bp_variants: Vec<_> = variants.iter().filter(|(w, _, _)| *w == bp).collect();

        for (_, _, filename) in bp_variants {
            let media_query = format!("(min-width: {}px)", bp + 100);

            // Build path consistent with ensure_target_paths output
            let src_path = if let Some(dir_end) = fallback_src.rfind('/') {
                format!("{}/{}", &fallback_src[..dir_end], filename)
            } else {
                filename.to_string()
            };

            sources.push_str(&format!(
                r#"<source media="{media_query}" srcset="{src_path}" width="100%">"#
            ));
        }
    }

    format!(r#"<picture>{sources}<img src="{fallback_src}" alt="{alt}" width="100%"></picture>"#)
}

async fn extract_body_and_title(
    content: &str,
    pool: &PgPool,
    media_root: Option<&Path>,
    file_dir: &Path,
    user_id: i32,
) -> Result<(Option<Frontmatter>, DateTime<Utc>, String, String), NurError> {
    let mut lines = content.lines();
    let mut title = String::new();

    // Skip YAML frontmatter
    let mut in_frontmatter = false;
    let mut frontmatter_raw = Vec::new();
    let mut frontmatter: Option<Frontmatter> = None;
    let mut created_at = Utc::now();
    let mut body_lines: Vec<String> = Vec::new();

    for line in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if in_frontmatter {
                let front = frontmatter_raw.join("\n");
                frontmatter = serde_yaml::from_str(&front).ok();

                created_at = frontmatter
                    .as_ref()
                    .and_then(|f| f.date.as_ref())
                    .and_then(|d| d.parse::<DateTime<Utc>>().ok())
                    .unwrap_or(Utc::now());
            }

            in_frontmatter = !in_frontmatter;
            continue;
        }
        if in_frontmatter {
            frontmatter_raw.push(trimmed);
            continue;
        }

        // Capture first H1 as title
        if trimmed.starts_with("# ") && title.is_empty() {
            title = trimmed.trim_start_matches("# ").trim().to_string();
            continue;
        }

        // Handle custom DOM/image syntax
        if trimmed.starts_with("{{") && trimmed.ends_with("}}") {
            let mut ln = trimmed
                .replace("{{", "")
                .replace("}}", "")
                .replace(r#" render="markdown""#, "")
                .to_string();

            if let Ok(dom_parsed) = Dom::parse(&ln)
                && let Some(element) = dom_parsed.children.first().and_then(|c| c.element())
                && element.name == "img"
            {
                let get_attr = |name: &str| {
                    element
                        .attributes
                        .get(name)
                        .and_then(|c| c.as_ref())
                        .map_or("", |v| v)
                };
                let align = get_attr("align");
                let alt = get_attr("alt");
                let caption = get_attr("caption");
                let src = get_attr("src");

                // Try to ensure/copy media and use new public path when successful
                let ensured =
                    ensure_media(pool, src, media_root, file_dir, &created_at, user_id).await;
                let new_src = match ensured {
                    Ok(Some(_media_id)) => {
                        // Compute public path consistent with ensure_media
                        let source = resolve_source_path(src, media_root, file_dir);
                        let filename = source.file_name().and_then(|n| n.to_str()).unwrap_or(src);
                        let (target_dir, _target_fs) = ensure_target_paths(&created_at);
                        format!("{}/{}", target_dir, filename)
                    }
                    _ => src.to_string(),
                };

                if align.is_empty() && caption.is_empty() {
                    ln = format!("![{alt}]({new_src})");
                } else {
                    let al = match align {
                        "right" => " class=\"float-right\"",
                        "left" => " class=\"float-left\"",
                        _ => "",
                    };

                    let img = // Fetch media variants if media_id exists
                        if let Ok(Some(media_id)) = ensured {
                            match fetch_media_variants(pool, media_id).await {
                                Ok(variants) if !variants.is_empty() => {
                                    build_picture_tag(&new_src, alt, variants)
                                }
                                _ => {
                                    format!("<img src=\"{new_src}\" alt=\"{alt}\" />")
                                }
                            }
                        } else {
                            format!("<img src=\"{new_src}\" alt=\"{alt}\" />")
                        };

                    ln = if caption.is_empty() {
                        img.replace(" />", &format!("{al} />"))
                    } else {
                        format!("<figure{al}>{img}<figcaption>{caption}</figcaption></figure>")
                    }
                }
            }

            body_lines.push(ln);
        } else {
            body_lines.push(line.to_string());
        }
    }

    let body = body_lines.join("\n").trim().to_string();
    let fallback_title = if title.is_empty() {
        let random_suffix = Uuid::new_v4().to_string()[0..8].to_string();
        format!("Untitled-{random_suffix}")
    } else {
        title
    };

    Ok((frontmatter, created_at, body, fallback_title))
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

async fn fetch_media_variants(
    pool: &PgPool,
    media_id: i32,
) -> Result<Vec<(i32, i32, String)>, sqlx::Error> {
    sqlx::query_as::<_, (i32, i32, String)>(
        "SELECT width, height, filename FROM media_variants WHERE media_id = $1 ORDER BY width DESC",
    )
    .bind(media_id)
    .fetch_all(pool)
    .await
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

async fn lookup_or_create_author(
    pool: &PgPool,
    name: &str,
    created_at: DateTime<Utc>,
) -> Result<Option<i32>, sqlx::Error> {
    let parts: Vec<&str> = name.rsplitn(2, ' ').collect();
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
        "INSERT INTO content_authors (first_name, last_name, slug, created_at, updated_at) VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(first_name)
    .bind(last_name)
    .bind(&slug)
    .bind(created_at)
    .bind(created_at)
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

fn resolve_source_path(
    thumbnail_path: &str,
    media_root: Option<&Path>,
    file_dir: &Path,
) -> PathBuf {
    let t = thumbnail_path.trim();
    // If it looks like a URL, we can't copy locally.
    if t.starts_with("http://") || t.starts_with("https://") {
        return PathBuf::from(t);
    }

    // If it's under PUBLIC_UPLOADS, interpret relative to STORAGE
    if t.starts_with(PUBLIC_UPLOADS) {
        let rel = t.trim_start_matches(PUBLIC_UPLOADS).trim_start_matches('/');
        return PathBuf::from(STORAGE.as_str()).join(rel);
    }

    let thumb = thumbnail_path
        .trim_start_matches('/')
        .split('/')
        .skip(1)
        .collect::<Vec<_>>()
        .join("/");

    if let Some(root) = media_root {
        root.join(thumb)
    } else {
        file_dir.join(thumb)
    }
}

fn ensure_target_paths(date: &DateTime<Utc>) -> (String, PathBuf) {
    let year = date.format("%Y").to_string();
    let month = date.format("%m").to_string();
    let target_dir = format!("{PUBLIC_UPLOADS}/{year}/{month}");
    let target_fs = PathBuf::from(STORAGE.as_str()).join(&year).join(&month);
    (target_dir, target_fs)
}

async fn ensure_media(
    pool: &PgPool,
    thumbnail_path: &str,
    media_root: Option<&Path>,
    file_dir: &Path,
    date: &DateTime<Utc>,
    user_id: i32,
) -> Result<Option<i32>, NurError> {
    let source_path = resolve_source_path(thumbnail_path, media_root, file_dir);
    let filename = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| NurError::BadRequest("Invalid filename".into()))?;

    let (target_dir, target_path) = ensure_target_paths(date);

    // Check if media record already exists
    if let Some(id) = sqlx::query_scalar::<_, i32>(
        "SELECT id FROM media WHERE path = $1 AND filename = $2 LIMIT 1",
    )
    .bind(&target_dir)
    .bind(filename)
    .fetch_optional(pool)
    .await?
    {
        return Ok(Some(id));
    }

    // Ensure target directory exists
    if !target_path.exists() {
        fs::create_dir_all(&target_path).await?;
    }

    let target_file = target_path.join(filename);

    // Skip copy if already in place
    if source_path != target_file && !target_file.is_file() {
        if source_path.exists() {
            if let Err(e) = fs::copy(&source_path, &target_file).await {
                warn!("Failed to copy media {source_path:?} → {target_file:?}: {e}");
                return Ok(None);
            }
            info!("Copied media: {source_path:?} → {target_file:?}");
        } else {
            warn!("Source media not found: {source_path:?}");
            return Ok(None);
        }
    }

    // Gather metadata
    let metadata = fs::metadata(&target_file).await?;
    let size = metadata.len() as i64;
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

    // Generate variants for images (only if they don't already exist)
    if mime_type.starts_with("image") {
        let existing_variants: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM media_variants WHERE media_id = $1")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap_or(0);

        if existing_variants == 0 {
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
                            info!("Insert variant: {}", variant_filename.bright_magenta());
                        }
                    }
                    Err(e) => {
                        warn!("Failed to generate variants for media {id}: {e}");
                    }
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

// content_media linking is handled via utils::ast_serialize::persist_content_media
