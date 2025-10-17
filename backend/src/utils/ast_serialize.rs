use serde_json::{Map, Value, json};

use crate::db::serialize::MediaSerializer;

fn pop_media(map: &Map<String, Value>, media: &mut Vec<MediaSerializer>) -> Option<Value> {
    let line: i32 = map
        .get("position")
        .and_then(|p| p.get("start"))
        .and_then(Value::as_object)
        .and_then(|s| s.get("line"))
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .try_into()
        .unwrap_or_default();

    media
        .iter_mut()
        .position(|m| {
            if m.node_index == line {
                m.node_index = 0;
                true
            } else {
                false
            }
        })
        .map(|pos| media.remove(pos))
        .and_then(|m| serde_json::to_value(m).ok())
}

fn apply_styles(node_type: &str, map: &Map<String, Value>, o: &mut Map<String, Value>) -> bool {
    match node_type {
        "strong" => {
            o.insert("bold".into(), Value::Bool(true));
            true
        }
        "emphasis" => {
            o.insert("italic".into(), Value::Bool(true));
            true
        }
        "underline" => {
            o.insert("underline".into(), Value::Bool(true));
            true
        }
        "delete" => {
            o.insert("strikethrough".into(), Value::Bool(true));
            true
        }
        "inlineCode" => {
            o.insert("code".into(), Value::Bool(true));
            true
        }
        "link" => {
            if let Some(href) = map.get("url").and_then(Value::as_str) {
                o.insert("url".into(), Value::String(href.to_string()));
            }
            o.insert("type".into(), Value::String("text".to_string()));
            true
        }
        _ => false,
    }
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

            if node_type == "image"
                && let Some(mut media_node) = pop_media(map, media)
                && let Some(obj) = media_node.as_object_mut()
            {
                obj.insert("type".into(), Value::String("image".into()));

                return media_node;
            }

            let mut children: Vec<Value> = vec![];

            if let Some(arr) = map.get("children").and_then(|v| v.as_array()) {
                for child in arr {
                    let mut converted = to_structure(child, media);

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

                    if let Value::Object(ref mut o) = converted
                        && apply_styles(node_type, map, o)
                    {
                        return Value::Object(o.clone());
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
