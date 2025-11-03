use std::{collections::HashMap, sync::LazyLock};

use serde_json::{Map, Value, json};

use crate::db::serialize::MediaSerializer;

pub static STYLE_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("strong", "bold"),
        ("emphasis", "italic"),
        ("underline", "underline"),
        ("delete", "strikethrough"),
        ("inlineCode", "code"),
    ])
});

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

    let pos = media.iter().position(|m| m.ast_line == line)?;
    media[pos].ast_line = 0;
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
        "link" => {
            if let Some(href) = map.get("url").and_then(Value::as_str) {
                o.insert("url".into(), Value::String(href.into()));
            }
            o.insert("type".into(), Value::String("text".into()));
            true
        }
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
            if node_type == "image"
                && let Some(mut media_node) = pop_media(map, media)
                && let Some(obj) = media_node.as_object_mut()
            {
                obj.insert("type".into(), Value::String("image".into()));

                return media_node;
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

                    // If the converted child is an object, attempt to apply inline styles based on the current node type.
                    // If a style was applied, return the styled object directly.
                    if let Value::Object(ref mut o) = converted
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

            Value::Object(result)
        }
        _ => json!({}),
    }
}

fn merge_html_blocks(nodes: Vec<Value>) -> Vec<Value> {
    let mut merged = Vec::new();
    let mut buffer = String::new();

    for node in nodes {
        if let Some(t) = node.get("type").and_then(Value::as_str) {
            if t == "html" {
                if let Some(txt) = node.get("text").and_then(Value::as_str) {
                    buffer.push_str(txt);
                }
                continue;
            }
        }

        if !buffer.is_empty() {
            merged.push(json!({
                "type": "html",
                "text": buffer.clone()
            }));
            buffer.clear();
        }

        merged.push(node);
    }

    if !buffer.is_empty() {
        merged.push(json!({
            "type": "html",
            "text": buffer
        }));
    }

    merged
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
