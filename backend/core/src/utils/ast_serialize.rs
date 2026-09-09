use std::collections::{HashMap, VecDeque};

use comrak::{
    Arena,
    nodes::{AstNode, NodeValue},
};
use serde_json::{Map, Value, json};
use sqlx::postgres::{PgConnection, PgPool};

use crate::{
    NurError, PUBLIC_UPLOADS,
    db::serialize::MediaSerializer,
    utils::markdown::{
        MarkdownImageRef, MarkdownSource, is_reference_definition, is_video_url,
        uses_reference_syntax,
    },
};

#[derive(Default)]
struct MediaLookup {
    by_location: HashMap<(String, String), VecDeque<MediaSerializer>>,
    unmatched: Vec<MediaSerializer>,
}

impl MediaLookup {
    fn take_from(media: &mut Vec<MediaSerializer>) -> Self {
        let mut lookup = Self::default();
        for item in media.drain(..) {
            match (item.path.clone(), item.filename.clone()) {
                (Some(path), Some(filename)) => lookup
                    .by_location
                    .entry((path, filename))
                    .or_default()
                    .push_back(item),
                _ => lookup.unmatched.push(item),
            }
        }
        lookup
    }

    fn pop_for_url(&mut self, url: &str) -> Option<MediaSerializer> {
        let location = normalize_media_path(url)?;
        self.by_location.get_mut(&location)?.pop_front()
    }

    fn restore_unmatched(mut self, media: &mut Vec<MediaSerializer>) {
        self.unmatched
            .extend(self.by_location.into_values().flat_map(VecDeque::into_iter));
        self.unmatched
            .sort_by_key(|item| item.position_index.unwrap_or(i32::MAX));
        *media = self.unmatched;
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

fn merge_adjacent_text_nodes(nodes: Vec<Value>) -> Vec<Value> {
    let mut merged: Vec<Value> = Vec::with_capacity(nodes.len());

    for node in nodes {
        let Some(current) = node.as_object() else {
            merged.push(node);
            continue;
        };
        let Some(previous) = merged.last_mut().and_then(Value::as_object_mut) else {
            merged.push(node);
            continue;
        };

        let same_attributes = previous.len() == current.len()
            && current.iter().all(|(key, value)| {
                key == "text" || previous.get(key).is_some_and(|previous| previous == value)
            });

        if same_attributes
            && current.get("type").and_then(Value::as_str) == Some("text")
            && previous.get("type").and_then(Value::as_str) == Some("text")
        {
            let suffix = current.get("text").and_then(Value::as_str).unwrap_or("");
            if let Some(Value::String(text)) = previous.get_mut("text") {
                text.push_str(suffix);
                continue;
            }
        }

        merged.push(node);
    }

    merged
}

fn node_type_name(node: &NodeValue) -> &'static str {
    match node {
        NodeValue::Document => "root",
        NodeValue::BlockQuote => "blockquote",
        NodeValue::FootnoteDefinition(_) => "footnoteDefinition",
        NodeValue::Paragraph => "paragraph",
        NodeValue::Heading(_) => "heading",
        NodeValue::List(_) => "list",
        NodeValue::FrontMatter(_) => "yaml",
        NodeValue::LineBreak => "break",
        NodeValue::Math(math) if !math.display_math => "inlineMath",
        NodeValue::Math(_) => "math",
        NodeValue::FootnoteReference(_) => "footnoteReference",
        NodeValue::Link(_) => "link",
        NodeValue::Image(_) => "image",
        NodeValue::Text(_) => "text",
        NodeValue::HtmlBlock(_) | NodeValue::HtmlInline(_) => "html",
        NodeValue::Strong => "strong",
        NodeValue::Emph => "emphasis",
        NodeValue::Strikethrough => "delete",
        NodeValue::Code(_) => "inlineCode",
        NodeValue::CodeBlock(_) => "code",
        NodeValue::Table(_) => "table",
        NodeValue::ThematicBreak => "thematicBreak",
        NodeValue::TableRow(_) => "tableRow",
        NodeValue::TableCell => "tableCell",
        NodeValue::Item(_) | NodeValue::TaskItem(_) => "listItem",
        _ => "unknown",
    }
}

fn apply_style_to_text_descendants(node: &mut Value, style_key: &str) {
    match node {
        Value::Object(map) => {
            let node_type = map.get("type").and_then(Value::as_str).unwrap_or("");

            if node_type == "text" {
                map.insert(style_key.into(), Value::Bool(true));
                return;
            }

            if let Some(Value::Array(children)) = map.get_mut("children") {
                for child in children {
                    apply_style_to_text_descendants(child, style_key);
                }
            }
        }
        Value::Array(children) => {
            for child in children {
                apply_style_to_text_descendants(child, style_key);
            }
        }
        _ => {}
    }
}

fn append_plain_text<'a>(node: &'a AstNode<'a>, text: &mut String) {
    match &node.data.borrow().value {
        NodeValue::Text(value) => text.push_str(value),
        NodeValue::HtmlInline(value) => text.push_str(value),
        NodeValue::Code(code) => text.push_str(&code.literal),
        NodeValue::SoftBreak | NodeValue::LineBreak => text.push('\n'),
        _ => {}
    }

    for child in node.children() {
        append_plain_text(child, text);
    }
}

fn plain_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut text = String::new();
    append_plain_text(node, &mut text);
    text
}

fn to_structure_ast<'a>(
    ast: &'a AstNode<'a>,
    source: &MarkdownSource<'_>,
    media: &mut MediaLookup,
) -> Value {
    let data = ast.data.borrow();
    match &data.value {
        NodeValue::Text(value) => {
            json!({
                "type": "text",
                "text": value,
            })
        }
        NodeValue::SoftBreak => {
            json!({
                "type": "text",
                "text": "\n",
            })
        }
        NodeValue::LineBreak => json!({ "type": "break" }),
        NodeValue::HtmlInline(value) => {
            json!({
                "type": "html",
                "text": value,
            })
        }
        NodeValue::HtmlBlock(html) => {
            json!({
                "type": "html",
                "text": html.literal,
            })
        }
        NodeValue::Code(code) => {
            json!({
                "type": "text",
                "text": code.literal,
                "code": true,
            })
        }
        NodeValue::Image(image) => {
            if source
                .slice(data.sourcepos)
                .is_some_and(|value| uses_reference_syntax(value, true))
            {
                let media_type = if is_video_url(&image.url) {
                    "videoReference"
                } else {
                    "imageReference"
                };
                return json!({ "type": media_type });
            }

            let media_type = if is_video_url(&image.url) {
                "video"
            } else {
                "image"
            };
            let mut node = json!({ "type": media_type });

            if !image.title.trim().is_empty() {
                node["title"] = image.title.clone().into();
            }

            if let Some(media_node) = media.pop_for_url(&image.url) {
                let image_alt = plain_text(ast);
                let alt = if image_alt.trim().is_empty() {
                    media_node.alt.unwrap_or_default()
                } else {
                    image_alt
                };

                node["alt"] = alt.into();
                node["filename"] = media_node.filename.into();
                node["path"] = media_node.path.into();

                if let Ok(variants) = serde_json::to_value(media_node.variants) {
                    node["variants"] = variants;
                }
                if media_type == "video"
                    && let Ok(video_variants) = serde_json::to_value(media_node.video_variants)
                {
                    node["video_variants"] = video_variants;
                }

                return node;
            }

            node["alt"] = plain_text(ast).into();
            node["src"] = image.url.clone().into();

            node
        }
        NodeValue::FootnoteReference(reference) => {
            json!({
                "type": "footnote_reference",
                "identifier": reference.name.clone(),
                "label": reference.name.clone(),
            })
        }
        NodeValue::FootnoteDefinition(definition) => {
            let name = definition.name.clone();
            drop(data);
            json!({
                "type": "footnote_definition",
                "children": ast.children().map(|child| to_structure_ast(child, source, media)).collect::<Vec<_>>(),
                "identifier": name.clone(),
                "label": name,
            })
        }
        _ => {
            let is_link_reference = matches!(data.value, NodeValue::Link(_))
                && source
                    .slice(data.sourcepos)
                    .is_some_and(|value| uses_reference_syntax(value, false));
            let node_type = if is_link_reference {
                "linkReference"
            } else {
                node_type_name(&data.value)
            };
            let is_paragraph = matches!(data.value, NodeValue::Paragraph);
            let heading_level = match &data.value {
                NodeValue::Heading(heading) => Some(heading.level),
                _ => None,
            };
            let link_url = match &data.value {
                NodeValue::Link(link) if !is_link_reference => Some(link.url.clone()),
                _ => None,
            };
            let style_key = match data.value {
                NodeValue::Strong => Some("bold"),
                NodeValue::Emph => Some("italic"),
                NodeValue::Strikethrough => Some("strikethrough"),
                _ => None,
            };
            drop(data);

            let mut children = Vec::new();
            for child in ast.children() {
                let mut converted = to_structure_ast(child, source, media);

                if let Some(style_key) = style_key {
                    apply_style_to_text_descendants(&mut converted, style_key);
                }

                if let Some(parent_link) = &link_url
                    && let Some(obj) = converted.as_object()
                    && obj.get("type").and_then(Value::as_str) == Some("link")
                    && obj.get("url").and_then(Value::as_str) == Some(parent_link.as_str())
                    && let Some(inner_children) = obj.get("children").and_then(Value::as_array)
                {
                    children.extend(inner_children.iter().cloned());
                    continue;
                }

                if let Some(obj) = converted.as_object()
                    && matches!(
                        obj.get("type").and_then(Value::as_str),
                        Some("strong" | "emphasis" | "delete")
                    )
                    && let Some(inner_children) = obj.get("children").and_then(Value::as_array)
                {
                    children.extend(inner_children.iter().cloned());
                    continue;
                }

                children.push(converted);
            }

            // Handle paragraphs containing one standalone media element.
            if is_paragraph
                && children.len() == 1
                && let Some(first) = children.first()
                && matches!(
                    first.get("type").and_then(Value::as_str),
                    Some("image" | "video")
                )
            {
                return first.clone();
            }

            let mut result = Map::new();
            result.insert("type".into(), Value::String(node_type.into()));

            if !children.is_empty() {
                let children = merge_adjacent_text_nodes(children);
                let children = merge_html_blocks(children);
                result.insert("children".into(), Value::Array(children));
            }

            if let Some(level) = heading_level {
                result.insert(
                    "level".into(),
                    Value::Number(serde_json::Number::from(level)),
                );
            }

            if let Some(url) = link_url {
                result.insert("url".into(), Value::String(url));
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

pub fn to_structure_root(markdown: &str, media: &mut Vec<MediaSerializer>) -> Value {
    let arena = Arena::new();
    let ast = crate::utils::markdown::parse_gfm(&arena, markdown);
    let source = MarkdownSource::new(markdown);
    let mut media_lookup = MediaLookup::take_from(media);
    let children = ast.children().collect::<Vec<_>>();
    let line_count = markdown.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let mut covered_lines = vec![false; line_count + 1];
    for child in &children {
        let sourcepos = child.data.borrow().sourcepos;
        let end = sourcepos.end.line.min(line_count);
        if sourcepos.start.line <= end {
            covered_lines[sourcepos.start.line..=end].fill(true);
        }
    }
    let mut converted = children
        .into_iter()
        .enumerate()
        .map(|(order, child)| {
            let line = child.data.borrow().sourcepos.start.line;
            (
                line,
                order,
                to_structure_ast(child, &source, &mut media_lookup),
            )
        })
        .collect::<Vec<_>>();

    converted.extend(
        markdown
            .lines()
            .enumerate()
            .filter(|(index, line)| {
                let line_number = index + 1;
                is_reference_definition(line)
                    && !covered_lines.get(line_number).copied().unwrap_or(true)
            })
            .map(|(index, _)| (index + 1, usize::MAX, json!({ "type": "definition" }))),
    );
    converted.sort_by_key(|(line, order, _)| (*line, *order));

    media_lookup.restore_unmatched(media);
    Value::Array(merge_html_blocks(
        converted.into_iter().map(|(_, _, node)| node).collect(),
    ))
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

// Truncates a single structure node in-place while consuming the remaining
// character budget.
//
// Returns `true` when the node should be kept in the final structure and
// `false` when it should be removed.
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

// Applies truncation to the root structure with a global character limit.
//
// The function mutates `root` in-place and removes nodes that become empty
// after truncation.
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

fn normalize_media_path(raw_url: &str) -> Option<(String, String)> {
    let mut path = raw_url.trim().to_string();
    if path.is_empty() {
        return None;
    }

    if let Some(pos) = path.find("://") {
        let slash_pos = path[pos + 3..].find('/')?;
        path = path[pos + 3 + slash_pos..].to_string();
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

pub(crate) async fn persist_content_media(
    pool: &PgPool,
    node_id: i64,
    images: &[MarkdownImageRef],
) -> Result<(), NurError> {
    let mut connection = pool.acquire().await?;
    persist_content_media_on(&mut connection, node_id, images).await
}

pub(crate) async fn persist_content_media_on(
    connection: &mut PgConnection,
    node_id: i64,
    images: &[MarkdownImageRef],
) -> Result<(), NurError> {
    let mut paths = Vec::new();
    let mut filenames = Vec::new();
    let mut positions = Vec::new();

    for image in images {
        if let Some((path, filename)) = normalize_media_path(&image.url) {
            paths.push(path);
            filenames.push(filename);
            positions.push(image.document_index);
        }
    }

    if paths.is_empty() {
        return Ok(());
    }

    sqlx::query(
        r#"
        WITH matched_images AS (
            SELECT
                m.id AS media_id,
                (row_number() OVER (ORDER BY image.document_index) - 1)::int AS position_index
            FROM UNNEST($2::text[], $3::text[], $4::int[])
                AS image(path, filename, document_index)
            JOIN media m
              ON m.path = image.path
             AND m.filename = image.filename
        )
        INSERT INTO content_node_media (node_id, media_id, position_index)
        SELECT $1, media_id, position_index
        FROM matched_images
        ON CONFLICT (node_id, position_index) DO UPDATE
        SET media_id = EXCLUDED.media_id,
            updated_at = now()
        "#,
    )
    .bind(node_id)
    .bind(&paths)
    .bind(&filenames)
    .bind(&positions)
    .execute(&mut *connection)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::db::serialize::{MediaSerializer, MediaVideoVariantSerializer};

    use super::to_structure_root;

    #[test]
    fn preserves_the_public_structure_ast_shape_for_gfm_content() {
        let mut media = Vec::new();
        let ast = to_structure_root(
            "# Heading\n\nA **bold** and *italic* with [a link](https://example.test).\n\n| Name | Value |\n| --- | --- |\n| One | 1 |",
            &mut media,
        );

        assert_eq!(
            ast[0],
            json!({
                "type": "heading",
                "level": 1,
                "children": [{ "type": "text", "text": "Heading" }],
            })
        );
        assert_eq!(ast[1]["type"], "paragraph");
        assert_eq!(
            ast[1]["children"][1],
            json!({ "type": "text", "text": "bold", "bold": true })
        );
        assert_eq!(
            ast[1]["children"][3],
            json!({ "type": "text", "text": "italic", "italic": true })
        );
        assert_eq!(
            ast[1]["children"][5],
            json!({
                "type": "link",
                "url": "https://example.test",
                "children": [{ "type": "text", "text": "a link" }],
            })
        );
        assert_eq!(ast[2]["type"], "table");
        assert_eq!(ast[2]["children"][0]["type"], "tableRow");
        assert_eq!(ast[2]["children"][0]["children"][0]["type"], "tableCell");
    }

    #[test]
    fn preserves_soft_and_hard_break_shapes() {
        let mut media = Vec::new();
        let ast = to_structure_root("soft\nbreak\n\nhard  \nbreak", &mut media);

        assert_eq!(
            ast[0]["children"],
            json!([{ "type": "text", "text": "soft\nbreak" }])
        );
        assert_eq!(ast[1]["children"][0]["text"], "hard");
        assert_eq!(ast[1]["children"][1], json!({ "type": "break" }));
        assert_eq!(ast[1]["children"][2]["text"], "break");
    }

    #[test]
    fn preserves_reference_node_types() {
        let mut media = Vec::new();
        let ast = to_structure_root(
            "[Docs][docs] and ![Logo][logo]\n\n[docs]: https://example.test\n[logo]: /uploads/logo.png",
            &mut media,
        );

        assert_eq!(ast[0]["children"][0]["type"], "linkReference");
        assert_eq!(ast[0]["children"][2]["type"], "imageReference");
        assert_eq!(ast[1]["type"], "definition");
        assert_eq!(ast[2]["type"], "definition");

        let ast = to_structure_root("Ä [Docs][docs]\n\n[docs]: https://example.test", &mut media);
        assert_eq!(ast[0]["children"][1]["type"], "linkReference");

        let ast = to_structure_root("![Clip][clip]\n\n[clip]: /uploads/clip.webm", &mut media);
        assert_eq!(ast[0]["children"][0]["type"], "videoReference");
    }

    #[test]
    fn matches_embedded_media_by_url_when_external_images_come_first() {
        let mut media = vec![MediaSerializer {
            filename: Some("local.jpg".into()),
            path: Some("/uploads".into()),
            position_index: Some(0),
            ..MediaSerializer::default()
        }];
        let ast = to_structure_root(
            "![External](https://example.test/image.jpg) ![Local](/uploads/local.jpg)",
            &mut media,
        );

        assert_eq!(
            ast[0]["children"][0]["src"],
            "https://example.test/image.jpg"
        );
        assert_eq!(ast[0]["children"][2]["path"], "/uploads");
        assert_eq!(ast[0]["children"][2]["filename"], "local.jpg");
        assert!(media.is_empty());
    }

    #[test]
    fn matches_repeated_occurrences_of_the_same_media() {
        let linked_media = || MediaSerializer {
            filename: Some("same.jpg".into()),
            path: Some("/uploads".into()),
            ..MediaSerializer::default()
        };
        let mut media = vec![linked_media(), linked_media()];
        let ast = to_structure_root(
            "![First](/uploads/same.jpg) ![Second](/uploads/same.jpg)",
            &mut media,
        );

        assert_eq!(ast[0]["children"][0]["filename"], "same.jpg");
        assert_eq!(ast[0]["children"][2]["filename"], "same.jpg");
        assert!(media.is_empty());
    }

    #[test]
    fn represents_image_style_video_urls_as_video_nodes() {
        let mut media = vec![MediaSerializer {
            filename: Some("clip.mp4".into()),
            path: Some("/uploads".into()),
            r#type: Some("video/mp4".into()),
            position_index: Some(0),
            video_variants: vec![MediaVideoVariantSerializer {
                id: 7,
                kind: "transcode".into(),
                profile: "h264-720".into(),
                width: 1280,
                height: 720,
                container: "mp4".into(),
                video_codec: "h264".into(),
                audio_codec: Some("aac".into()),
                filename: "clip--h264-720.mp4".into(),
                size: 12_345,
                duration_ms: Some(5_000),
            }],
            ..MediaSerializer::default()
        }];
        let ast = to_structure_root(
            "![A short clip](/uploads/clip.mp4 \"Clip title\")",
            &mut media,
        );

        assert_eq!(ast[0]["type"], "video");
        assert_eq!(ast[0]["alt"], "A short clip");
        assert_eq!(ast[0]["title"], "Clip title");
        assert_eq!(ast[0]["path"], "/uploads");
        assert_eq!(ast[0]["filename"], "clip.mp4");
        assert_eq!(
            ast[0]["video_variants"][0]["filename"],
            "clip--h264-720.mp4"
        );
        assert!(media.is_empty());

        let mut external_media = Vec::new();
        let external = to_structure_root(
            "![External](https://example.test/clip.webm)",
            &mut external_media,
        );
        assert_eq!(external[0]["type"], "video");
        assert_eq!(external[0]["src"], "https://example.test/clip.webm");
    }
}
