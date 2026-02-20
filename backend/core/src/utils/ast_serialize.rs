use std::collections::HashSet;

use markdown::mdast::Node;
use serde_json::{Map, Value, json};
use sqlx::postgres::PgPool;
use tracing::error;

use crate::{
    NurError, PUBLIC_UPLOADS,
    db::{fields::Table, handles, models::ContentNodeMedia, serialize::MediaSerializer},
};

#[derive(Debug)]
struct AstImageRef {
    url: String,
    ast_line: i32,
    start_offset: Option<i32>,
    end_offset: Option<i32>,
}

fn pop_media_mdast(node: &Node, media: &mut Vec<MediaSerializer>) -> Option<Value> {
    let line: i32 = node.position()?.start.line.try_into().ok()?;
    let pos = media.iter().position(|m| m.ast_line == Some(line))?;
    serde_json::to_value(media.remove(pos)).ok()
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

fn node_type_name(node: &Node) -> &'static str {
    match node {
        Node::Root(_) => "root",
        Node::Blockquote(_) => "blockquote",
        Node::FootnoteDefinition(_) => "footnoteDefinition",
        Node::Paragraph(_) => "paragraph",
        Node::Heading(_) => "heading",
        Node::List(_) => "list",
        Node::Toml(_) => "toml",
        Node::Yaml(_) => "yaml",
        Node::Break(_) => "break",
        Node::InlineMath(_) => "inlineMath",
        Node::FootnoteReference(_) => "footnoteReference",
        Node::Link(_) => "link",
        Node::LinkReference(_) => "linkReference",
        Node::Image(_) => "image",
        Node::ImageReference(_) => "imageReference",
        Node::Text(_) => "text",
        Node::Html(_) => "html",
        Node::Strong(_) => "strong",
        Node::Emphasis(_) => "emphasis",
        Node::Delete(_) => "delete",
        Node::InlineCode(_) => "inlineCode",
        Node::Code(_) => "code",
        Node::Math(_) => "math",
        Node::Table(_) => "table",
        Node::ThematicBreak(_) => "thematicBreak",
        Node::TableRow(_) => "tableRow",
        Node::TableCell(_) => "tableCell",
        Node::ListItem(_) => "listItem",
        Node::Definition(_) => "definition",
        _ => "unknown",
    }
}

fn to_structure_mdast(ast: &Node, media: &mut Vec<MediaSerializer>) -> Value {
    match ast {
        Node::Text(text) => {
            json!({
                "type": "text",
                "text": text.value,
            })
        }
        Node::Html(html) => {
            json!({
                "type": "html",
                "text": html.value,
            })
        }
        Node::InlineCode(code) => {
            json!({
                "type": "text",
                "text": code.value,
                "code": true,
            })
        }
        Node::Image(image) => {
            if let Some(mut media_node) = pop_media_mdast(ast, media)
                && let Some(obj) = media_node.as_object_mut()
            {
                obj.insert("type".into(), Value::String("image".into()));
                return media_node;
            }

            if image.url.starts_with("http://") || image.url.starts_with("https://") {
                return json!({
                    "type": "image",
                    "src": image.url,
                    "alt": image.alt,
                });
            }

            json!({ "type": "image" })
        }
        _ => {
            let node_type = node_type_name(ast);
            let is_paragraph = matches!(ast, Node::Paragraph(_));
            let is_link = matches!(ast, Node::Link(_));
            let style_key = match ast {
                Node::Strong(_) => Some("bold"),
                Node::Emphasis(_) => Some("italic"),
                Node::Delete(_) => Some("strikethrough"),
                _ => None,
            };

            let mut children: Vec<Value> = ast
                .children()
                .map(|nodes| Vec::with_capacity(nodes.len()))
                .unwrap_or_default();

            if let Some(nodes) = ast.children() {
                for child in nodes {
                    let mut converted = to_structure_mdast(child, media);

                    if is_paragraph
                        && converted.get("type").and_then(Value::as_str) == Some("image")
                    {
                        if !children.is_empty() {
                            children.push(converted);

                            return json!({
                                "type": "paragraph",
                                "children": children,
                            });
                        }

                        return converted;
                    }

                    if is_link
                        && let Node::Link(parent_link) = ast
                        && let Some(obj) = converted.as_object()
                        && obj.get("type").and_then(Value::as_str) == Some("link")
                        && obj.get("url").and_then(Value::as_str) == Some(parent_link.url.as_str())
                        && let Some(inner_children) = obj.get("children").and_then(Value::as_array)
                    {
                        children.extend(inner_children.iter().cloned());
                        continue;
                    }

                    if !is_link
                        && let Some(style_key) = style_key
                        && let Value::Object(ref mut out) = converted
                    {
                        out.insert(style_key.into(), Value::Bool(true));
                        return converted;
                    }

                    children.push(converted);
                }
            }

            let mut result = Map::new();
            result.insert("type".into(), Value::String(node_type.into()));

            if !children.is_empty() {
                let children = merge_html_blocks(children);
                result.insert("children".into(), Value::Array(children));
            }

            if let Node::Heading(heading) = ast {
                result.insert(
                    "level".into(),
                    Value::Number(serde_json::Number::from(heading.depth)),
                );
            }

            if let Node::Link(link) = ast {
                result.insert("url".into(), Value::String(link.url.clone()));
            }

            Value::Object(result)
        }
    }
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

pub fn to_structure_root_mdast(ast: &Node, media: &mut Vec<MediaSerializer>) -> Value {
    if let Node::Root(root) = ast {
        let mut converted: Vec<Value> = Vec::with_capacity(root.children.len());
        for child in &root.children {
            converted.push(to_structure_mdast(child, media));
        }

        let merged = merge_html_blocks(converted);
        Value::Array(merged)
    } else {
        Value::Array(vec![to_structure_mdast(ast, media)])
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
