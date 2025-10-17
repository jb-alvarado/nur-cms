use serde_json::{Map, Value, json};

use crate::db::serialize::MediaSerializer;

// Try to find and remove a matching media node from `media` based on the AST node's start line.
// Returns the serde_json::Value representation of the removed media if found.
fn pop_media(map: &Map<String, Value>, media: &mut Vec<MediaSerializer>) -> Option<Value> {
    // Extract the start line from a nested "position.start.line" structure.
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
                // set to 0 to skip serialization
                m.node_index = 0;
                true
            } else {
                false
            }
        })
        .map(|pos| media.remove(pos))
        .and_then(|m| serde_json::to_value(m).ok())
}

// Apply inline styles based on the node type onto `o` (an object representing converted node).
// Returns true if a style was applied.
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
            // If link has a URL, add it. Also mark type as "text" for links.
            if let Some(href) = map.get("url").and_then(Value::as_str) {
                o.insert("url".into(), Value::String(href.to_string()));
            }
            o.insert("type".into(), Value::String("text".to_string()));
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

            // If this AST node represents an image, try to pop the corresponding media entry.
            // This uses the new `if ... && let ... && let ...` pattern chaining to ensure all conditions match.
            // `as_object_mut()` gets a mutable view into the Value so we can insert the "type" field before returning.
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
                        // Clone the object to return an owned Value without borrowing `converted`.
                        return Value::Object(o.clone());
                    }

                    children.push(converted);
                }
            }

            // Build the resulting object: always include the node type and children if any.
            let mut result = Map::new();
            result.insert("type".into(), Value::String(node_type.to_string()));

            if !children.is_empty() {
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

// Convert the AST root: if it has children, map them, otherwise wrap the single converted node in an array.
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
