use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

use serde_json::{Map, Value, json};
use sqlx::postgres::PgPool;
use tracing::error;

use crate::{
    NurError, PUBLIC_UPLOADS,
    db::{fields::Table, handles, models::ContentNodeMedia, serialize::MediaSerializer},
};

pub static STYLE_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("strong", "bold"),
        ("emphasis", "italic"),
        ("underline", "underline"),
        ("delete", "strikethrough"),
        ("inlineCode", "code"),
    ])
});

#[derive(Debug)]
struct AstImageRef {
    url: String,
    ast_line: i32,
    start_offset: Option<i32>,
    end_offset: Option<i32>,
}

// Try to find and remove a matching media node from `media` based on the AST node's start line.
// Returns the serde_json::Value representation of the removed media if found.
fn pop_media(map: &Map<String, Value>, media: &mut Vec<MediaSerializer>) -> Option<Value> {
    let line: i32 = map
        .get("position")?
        .get("start")?
        .as_object()?
        .get("line")?
        .as_i64()?
        .try_into()
        .ok()?;

    let pos = media.iter().position(|m| m.ast_line == Some(line))?;
    serde_json::to_value(media.remove(pos)).ok()
}

// Apply inline styles based on the node type onto `o` (an object representing converted node).
// Returns true if a style was applied.
fn apply_styles(node_type: &str, map: &Map<String, Value>, o: &mut Map<String, Value>) -> bool {
    if let Some(&style_key) = STYLE_MAP.get(node_type) {
        o.insert(style_key.into(), Value::Bool(true));
        return true;
    }

    match node_type {
        "html" => {
            if let Some(value) = map.get("value") {
                o.insert("text".into(), value.clone());
            }
            true
        }
        _ => false,
    }
}

// Convert a single AST node into the target structure.
// `media` is mutated to pick up external media nodes when an "image" node is encountered.
pub fn to_structure(ast: &Value, media: &mut Vec<MediaSerializer>) -> Value {
    match ast {
        Value::Object(map) => {
            let node_type = map
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            // Text nodes are terminal and directly map to a simple object.
            if node_type == "text" {
                return json!({
                    "type": "text",
                    "text": map.get("value").and_then(|v| v.as_str()).unwrap_or(""),
                });
            }

            if node_type == "html" {
                return json!({
                    "type": "html",
                    "text": map.get("value").and_then(|v| v.as_str()).unwrap_or(""),
                });
            }

            // If this AST node represents an image, try to pop the corresponding media entry.
            if node_type == "image" {
                if let Some(mut media_node) = pop_media(map, media)
                    && let Some(obj) = media_node.as_object_mut()
                {
                    obj.insert("type".into(), Value::String("image".into()));

                    return media_node;
                }

                // If no media entry found (e.g., external image), check if URL is external
                if let Some(url) = map.get("url").and_then(Value::as_str)
                    && (url.starts_with("http://") || url.starts_with("https://"))
                {
                    // Return external image node with src (not url) to avoid conflicts with link urls
                    let alt = map.get("alt").and_then(Value::as_str).unwrap_or("");
                    return json!({
                        "type": "image",
                        "src": url,
                        "alt": alt,
                    });
                }
            }

            let mut children: Vec<Value> = vec![];

            // Convert children recursively if present.
            if let Some(arr) = map.get("children").and_then(|v| v.as_array()) {
                for child in arr {
                    let mut converted = to_structure(child, media);

                    // Special case: if a paragraph contains an image child, we either
                    // - append the image if we already accumulated other children, then return the paragraph,
                    // - or if the image is the only/first child, return the image directly (no wrapping paragraph).
                    if node_type == "paragraph"
                        && converted.get("type").and_then(Value::as_str) == Some("image")
                    {
                        if !children.is_empty() {
                            children.push(converted.clone());

                            return json!({
                                "type": "paragraph",
                                "children": children
                            });
                        };

                        return converted;
                    }

                    if node_type == "link"
                        && let Some(parent_url) = map.get("url").and_then(Value::as_str)
                        && let Some(obj) = converted.as_object()
                        && obj.get("type").and_then(Value::as_str) == Some("link")
                        && obj.get("url").and_then(Value::as_str) == Some(parent_url)
                        && let Some(inner_children) = obj.get("children").and_then(Value::as_array)
                    {
                        children.extend(inner_children.clone());
                        continue;
                    }

                    // If the converted child is an object, attempt to apply inline styles based on the current node type.
                    // If a style was applied, return the styled object directly.
                    // Skip this for links - they should keep their children structure.
                    if node_type != "link"
                        && let Value::Object(ref mut o) = converted
                        && apply_styles(node_type, map, o)
                    {
                        return converted;
                    }

                    children.push(converted);
                }
            }

            // Build the resulting object: always include the node type and children if any.
            let mut result = Map::new();
            result.insert("type".into(), Value::String(node_type.into()));

            if !children.is_empty() {
                let children = merge_html_blocks(children);
                result.insert("children".into(), Value::Array(children));
            }

            // Heading nodes may carry a numeric depth; if present, attach it as "level".
            if node_type.starts_with("heading")
                && let Some(Value::Number(level)) = map.get("depth")
            {
                // Clone the Number since we only have an immutable borrow of `map`.
                result.insert("level".into(), Value::Number(level.clone()));
            }

            // Link nodes should include their URL.
            if node_type == "link"
                && let Some(url) = map.get("url").and_then(Value::as_str)
            {
                result.insert("url".into(), Value::String(url.into()));
            }

            Value::Object(result)
        }
        _ => json!({}),
    }
}

fn merge_html_blocks(nodes: Vec<Value>) -> Vec<Value> {
    let mut merged = Vec::new();
    let mut buffer = String::new();
    let mut tag_stack: Vec<String> = Vec::new();

    let iter = nodes.into_iter().peekable();

    for node in iter {
        let node_type = node.get("type").and_then(Value::as_str);
        let text = node.get("text").and_then(Value::as_str).unwrap_or("");

        match node_type {
            Some("html") => {
                if text.starts_with("</") {
                    buffer.push_str(text);

                    if let Some(close_tag_name) = extract_tag_name(text)
                        && let Some(pos) = tag_stack.iter().rposition(|t| *t == close_tag_name)
                    {
                        tag_stack.truncate(pos);
                    }

                    if tag_stack.is_empty() {
                        merged.push(json!({ "type": "html", "text": buffer.clone() }));
                        buffer.clear();
                    }
                } else if text.starts_with('<') {
                    if tag_stack.is_empty() && !buffer.is_empty() {
                        merged.push(json!({ "type": "html", "text": buffer.clone() }));
                        buffer.clear();
                    }

                    if let Some(open_tag_name) = extract_tag_name(text) {
                        tag_stack.push(open_tag_name);
                    }
                    buffer.push_str(text);
                } else if !tag_stack.is_empty() {
                    buffer.push_str(text);
                } else {
                    merged.push(node);
                }
            }

            Some("text") => {
                if tag_stack.is_empty() {
                    if !buffer.is_empty() {
                        merged.push(json!({ "type": "html", "text": buffer.clone() }));
                        buffer.clear();
                    }
                    merged.push(node);
                } else {
                    buffer.push_str(text);
                }
            }

            _ => {
                if !buffer.is_empty() {
                    merged.push(json!({ "type": "html", "text": buffer.clone() }));
                    buffer.clear();
                    tag_stack.clear();
                }
                merged.push(node);
            }
        }
    }

    if !buffer.is_empty() {
        merged.push(json!({ "type": "html", "text": buffer }));
    }

    merged
}

fn extract_tag_name(tag: &str) -> Option<String> {
    let tag = tag.trim_matches(|c| c == '<' || c == '>');
    let tag = tag.trim_start_matches('/');
    let name: String = tag
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '>')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

// Convert the AST root: if it has children, map them, otherwise wrap the single converted node in an array.
pub fn to_structure_root(ast: &Value, media: &mut Vec<MediaSerializer>) -> Value {
    if let Some(children) = ast.get("children").and_then(|v| v.as_array()) {
        let converted: Vec<Value> = children
            .iter()
            .map(|child| to_structure(child, media))
            .collect();

        let merged = merge_html_blocks(converted);
        Value::Array(merged)
    } else {
        Value::Array(vec![to_structure(ast, media)])
    }
}

fn truncate_text_at_word(text: &str, remaining: usize) -> String {
    let mut out: String = text.chars().take(remaining).collect();

    if let Some(pos) = out.rfind(|c: char| c.is_whitespace()) {
        out.truncate(pos);
    }

    let out = out.trim_end();

    if out.is_empty() {
        return " ...".to_string();
    }

    let mut result = out.to_string();
    result.push_str(" ...");
    result
}

fn truncate_structure_node(node: &mut Value, remaining: &mut usize) -> bool {
    match node {
        Value::Object(map) => {
            let node_type = map
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();

            if ![
                "text",
                "paragraph",
                "heading",
                "list",
                "listItem",
                "link",
                "quote",
            ]
            .contains(&node_type.as_str())
            {
                return false;
            }

            if node_type == "text" {
                let text = map.get("text").and_then(Value::as_str).unwrap_or("");

                if text.is_empty() {
                    return false;
                }

                if *remaining == 0 {
                    map.insert("text".into(), Value::String(String::new()));
                    return false;
                }

                let len = text.chars().count();

                if len <= *remaining {
                    *remaining -= len;
                    return true;
                }

                let truncated_text = truncate_text_at_word(text, *remaining);
                *remaining = 0;
                map.insert("text".into(), Value::String(truncated_text));
                return true;
            }

            if let Some(Value::Array(children)) = map.get_mut("children") {
                let mut next_children = Vec::new();

                for mut child in std::mem::take(children) {
                    if truncate_structure_node(&mut child, remaining) {
                        next_children.push(child);
                    }
                }

                *children = next_children;
            }

            let is_empty_children = map
                .get("children")
                .and_then(Value::as_array)
                .map(Vec::is_empty)
                .unwrap_or(false);

            if is_empty_children {
                return false;
            }

            true
        }
        Value::Array(arr) => {
            let mut next_children = Vec::new();

            for mut child in std::mem::take(arr) {
                if truncate_structure_node(&mut child, remaining) {
                    next_children.push(child);
                }
            }

            *arr = next_children;
            !arr.is_empty()
        }
        _ => true,
    }
}

pub fn truncate_structure_root(root: &mut Value, limit: usize) {
    if limit == 0 {
        if let Value::Array(arr) = root {
            arr.clear();
        }
        return;
    }

    let mut remaining = limit;

    match root {
        Value::Array(arr) => {
            let mut next_children = Vec::new();

            for mut child in std::mem::take(arr) {
                if truncate_structure_node(&mut child, &mut remaining) {
                    next_children.push(child);
                }
            }

            *arr = next_children;
        }
        _ => {
            let _ = truncate_structure_node(root, &mut remaining);
        }
    }
}

fn collect_image_refs(node: &Value, acc: &mut Vec<AstImageRef>) {
    match node {
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("image") {
                let url = map
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();

                let position = map.get("position");
                let ast_line = position
                    .and_then(|pos| pos.get("start"))
                    .and_then(|start| start.get("line"))
                    .and_then(Value::as_i64)
                    .and_then(|v| i32::try_from(v).ok())
                    .unwrap_or_default();

                let start_offset = position
                    .and_then(|pos| pos.get("start"))
                    .and_then(|start| start.get("offset"))
                    .and_then(Value::as_i64)
                    .and_then(|v| i32::try_from(v).ok());

                let end_offset = position
                    .and_then(|pos| pos.get("end"))
                    .and_then(|end| end.get("offset"))
                    .and_then(Value::as_i64)
                    .and_then(|v| i32::try_from(v).ok());

                acc.push(AstImageRef {
                    url,
                    ast_line,
                    start_offset,
                    end_offset,
                });
            }

            if let Some(children) = map.get("children").and_then(Value::as_array) {
                for child in children {
                    collect_image_refs(child, acc);
                }
            }
        }
        Value::Array(arr) => {
            for child in arr {
                collect_image_refs(child, acc);
            }
        }
        _ => {}
    }
}

fn normalize_media_path(raw_url: &str) -> Option<(String, String)> {
    let mut path = raw_url.trim().to_string();
    if path.is_empty() {
        return None;
    }

    if let Some(pos) = path.find("://") {
        if let Some(slash_pos) = path[pos + 3..].find('/') {
            path = path[pos + 3 + slash_pos..].to_string();
        } else {
            return None;
        }
    }

    if let Some(pos) = path.find('#') {
        path.truncate(pos);
    }

    if let Some(pos) = path.find('?') {
        path.truncate(pos);
    }

    if !path.starts_with(PUBLIC_UPLOADS) {
        return None;
    }

    let (dir, filename) = path.rsplit_once('/')?;
    if filename.is_empty() {
        return None;
    }

    let dir = if dir.is_empty() {
        "/".to_string()
    } else {
        dir.to_string()
    };

    Some((dir, filename.to_string()))
}

pub async fn persist_content_media(
    pool: &PgPool,
    node_id: i64,
    ast: &Value,
) -> Result<(), NurError> {
    let mut images = Vec::new();
    collect_image_refs(ast, &mut images);

    if images.is_empty() {
        return Ok(());
    }

    let mut seen = HashSet::new();

    for image in images {
        let Some((path, filename)) = normalize_media_path(&image.url) else {
            continue;
        };

        if let Some(media_id) = handles::select_media_id_by_path(pool, &path, &filename).await? {
            if !seen.insert((media_id, image.ast_line)) {
                continue;
            }

            let link = ContentNodeMedia {
                node_id,
                media_id,
                ast_line: image.ast_line,
                start_offset: image.start_offset,
                end_offset: image.end_offset,
            };

            if let Err(e) = handles::insert_record::<ContentNodeMedia, i64>(
                pool,
                &Table::ContentNodeMedia,
                &link,
            )
            .await
            {
                error!("content_media insert error: {e}");
            }
        }
    }

    Ok(())
}
