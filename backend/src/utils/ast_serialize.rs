use serde_json::{Value, json};

pub fn to_structure(ast: &Value) -> Value {
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

            let mut children: Vec<Value> = vec![];
            if let Some(arr) = map.get("children").and_then(|v| v.as_array()) {
                for child in arr {
                    let mut converted = to_structure(child);

                    if node_type == "strong"
                        && let Value::Object(ref mut o) = converted
                    {
                        o.insert("bold".into(), Value::Bool(true));
                    }
                    if node_type == "emphasis"
                        && let Value::Object(ref mut o) = converted
                    {
                        o.insert("italic".into(), Value::Bool(true));
                    }

                    children.push(converted);
                }
            }

            let mut result = json!({
                "type": node_type,
                "children": children
            });

            if node_type.starts_with("heading")
                && let Some(Value::Number(level)) = map.get("depth")
            {
                result
                    .as_object_mut()
                    .unwrap()
                    .insert("level".into(), Value::Number(level.clone()));
            }
            result
        }
        _ => json!({}),
    }
}
