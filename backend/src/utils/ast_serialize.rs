use serde_json::{Map, Value, json};

use crate::db::serialize::MediaSerializer;

fn pop_media(line: i32, media: &mut Vec<MediaSerializer>) -> Option<MediaSerializer> {
    media
        .iter()
        .position(|m| m.node_index == line)
        .map(|pos| media.remove(pos))
}

pub fn to_structure(ast: &Value, media: &mut Vec<MediaSerializer>) -> Value {
    match ast {
        Value::Object(map) => {
            let node_type = map
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            if node_type == "text" {
                return json!({
                    "type": "text",
                    "text": map.get("value").and_then(|v| v.as_str()).unwrap_or(""),
                });
            }

            if node_type == "image" {
                let line: i32 = map
                    .get("position")
                    .and_then(|p| p.get("start"))
                    .and_then(|s| s.get("line"))
                    .and_then(Value::as_i64)
                    .unwrap_or_default()
                    .try_into()
                    .unwrap_or_default();

                return json!({
                    "type": "image",
                    "children": pop_media(line, media),
                });
            }

            let mut children: Vec<Value> = vec![];
            if let Some(arr) = map.get("children").and_then(|v| v.as_array()) {
                for child in arr {
                    let mut converted = to_structure(child, media);

                    if let Value::Object(ref mut o) = converted {
                        match node_type {
                            "strong" => {
                                o.insert("bold".into(), Value::Bool(true));
                            }
                            "emphasis" => {
                                o.insert("italic".into(), Value::Bool(true));
                            }
                            "underline" => {
                                o.insert("underline".into(), Value::Bool(true));
                            }
                            "delete" => {
                                o.insert("strikethrough".into(), Value::Bool(true));
                            }
                            "inlineCode" => {
                                o.insert("code".into(), Value::Bool(true));
                            }
                            "link" => {
                                if let Some(href) = map.get("url").and_then(|v| v.as_str()) {
                                    o.insert("href".into(), Value::String(href.to_string()));
                                }
                            }
                            _ => {}
                        }
                    }

                    children.push(converted);
                }
            }

            let mut result = Map::new();
            result.insert("type".into(), Value::String(node_type.to_string()));

            if !children.is_empty() {
                result.insert("children".into(), Value::Array(children));
            }

            if node_type.starts_with("heading")
                && let Some(Value::Number(level)) = map.get("depth")
            {
                result.insert("level".into(), Value::Number(level.clone()));
            }

            Value::Object(result)
        }
        _ => json!({}),
    }
}

pub fn to_structure_root(ast: &Value, media: &mut Vec<MediaSerializer>) -> Value {
    if let Some(children) = ast.get("children").and_then(|v| v.as_array()) {
        let converted: Vec<Value> = children
            .iter()
            .map(|child| to_structure(child, media))
            .collect();
        Value::Array(converted)
    } else {
        Value::Array(vec![to_structure(ast, media)])
    }
}
