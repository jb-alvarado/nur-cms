use std::fmt::Write;

use comrak::{
    Arena, Options, create_formatter,
    html::{ChildRendering, dangerous_url, format_node_default},
    nodes::{AstNode, Node, NodeValue, Sourcepos},
    parse_document,
};

use crate::utils::errors::NurError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkdownImageRef {
    pub url: String,
    pub document_index: i32,
}

/// Whether an image-style Markdown URL denotes a browser-playable video.
///
/// Markdown has no native video syntax, so videos deliberately use the same
/// `![description](url)` form as images. The list is limited to the video
/// containers accepted by the CMS upload pipeline and broadly supported by
/// browsers.
pub(crate) fn is_video_url(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let Some((_, extension)) = path.rsplit_once('.') else {
        return false;
    };

    ["mp4", "m4v", "mov", "webm", "ogv", "ogg"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

create_formatter!(VideoHtmlFormatter, {
    NodeValue::Image(ref image) => |context, node, entering| {
        if !is_video_url(&image.url) {
            return format_node_default(context, node, entering);
        }

        if entering {
            context.write_str("<video controls src=\"")?;
            if context.options.render.r#unsafe || !dangerous_url(&image.url) {
                if let Some(rewriter) = &context.options.extension.image_url_rewriter {
                    context.escape_href(&rewriter.to_html(&image.url))?;
                } else {
                    context.escape_href(&image.url)?;
                }
            }
            context.write_str("\"")?;
            if !image.title.is_empty() {
                context.write_str(" title=\"")?;
                context.escape(&image.title)?;
                context.write_str("\"")?;
            }
            context.write_str(">")?;
            return Ok(ChildRendering::Plain);
        }

        context.write_str("</video>")?;
        return Ok(ChildRendering::Skip);
    },
});

pub(crate) struct MarkdownSource<'a> {
    markdown: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> MarkdownSource<'a> {
    pub fn new(markdown: &'a str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            markdown
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
        );
        Self {
            markdown,
            line_starts,
        }
    }

    pub fn slice(&self, sourcepos: Sourcepos) -> Option<&'a str> {
        let start = self
            .line_starts
            .get(sourcepos.start.line.checked_sub(1)?)?
            .checked_add(sourcepos.start.column.checked_sub(1)?)?;
        let end = self
            .line_starts
            .get(sourcepos.end.line.checked_sub(1)?)?
            .checked_add(sourcepos.end.column)?;
        self.markdown.get(start..end)
    }
}

fn closing_label_offset(source: &str, label_start: usize) -> Option<usize> {
    let mut depth = 1_u32;
    let mut escaped = false;

    for (offset, ch) in source[label_start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '[' => depth = depth.saturating_add(1),
            ']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(label_start + offset + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn uses_reference_syntax(source: &str, image: bool) -> bool {
    let label_start = if image {
        if !source.starts_with("![") {
            return false;
        }
        2
    } else {
        if !source.starts_with('[') {
            return false;
        }
        1
    };

    closing_label_offset(source, label_start)
        .and_then(|offset| source.get(offset..))
        .is_some_and(|remainder| !remainder.trim_start().starts_with('('))
}

pub(crate) fn is_reference_definition(line: &str) -> bool {
    let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
    if indentation > 3 {
        return false;
    }
    let Some(source) = line.get(indentation..) else {
        return false;
    };
    if !source.starts_with('[') || source.starts_with("[^") {
        return false;
    }

    closing_label_offset(source, 1)
        .and_then(|offset| source.get(offset..))
        .is_some_and(|remainder| remainder.starts_with(':'))
}

/// Options shared by HTML, AST, and media-reference parsing.
pub fn gfm_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.autolink = true;
    options.extension.footnotes = true;
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.tasklist = true;
    options.render.escape = true;
    options
}

/// Parses Markdown with the CMS' common GFM configuration.
pub fn parse_gfm<'a>(arena: &'a Arena<'a>, markdown: &str) -> Node<'a> {
    parse_document(arena, markdown, &gfm_options())
}

/// Renders GitHub-Flavored Markdown to safe HTML. Raw HTML remains escaped by
/// Comrak unless its unsafe renderer option is explicitly enabled (which this
/// CMS never does).
pub fn render_gfm_html(markdown: &str) -> Result<String, NurError> {
    let arena = Arena::new();
    let options = gfm_options();
    let root = parse_document(&arena, markdown, &options);
    let mut html = String::with_capacity(markdown.len());
    VideoHtmlFormatter::format_document(root, &options, &mut html)
        .map_err(|_| NurError::InternalServerError)?;
    Ok(html)
}

fn collect_image_references<'a>(
    node: &'a AstNode<'a>,
    source: &MarkdownSource<'_>,
    next_position: &mut usize,
    references: &mut Vec<MarkdownImageRef>,
) {
    let data = node.data.borrow();
    if let NodeValue::Image(link) = &data.value
        && !source
            .slice(data.sourcepos)
            .is_some_and(|value| uses_reference_syntax(value, true))
    {
        let position = *next_position;
        *next_position = next_position.saturating_add(1);

        if let Ok(document_index) = i32::try_from(position) {
            references.push(MarkdownImageRef {
                url: link.url.clone(),
                document_index,
            });
        }
    }
    drop(data);

    for child in node.children() {
        collect_image_references(child, source, next_position, references);
    }
}

/// Returns direct Markdown image and video references in document order.
/// Reference-style images remain reference nodes in the compatibility AST and
/// are not persisted.
pub(crate) fn media_references(markdown: &str) -> Vec<MarkdownImageRef> {
    let arena = Arena::new();
    let root = parse_gfm(&arena, markdown);
    let source = MarkdownSource::new(markdown);
    let mut references = Vec::new();
    let mut next_position = 0;
    collect_image_references(root, &source, &mut next_position, &mut references);
    references
}

#[cfg(test)]
mod tests {
    use super::{is_video_url, media_references, render_gfm_html};

    #[test]
    fn renders_gfm_tables_and_keeps_raw_html_escaped() {
        let html =
            render_gfm_html("| Name | Value |\n| --- | --- |\n| One | 1 |\n\n<span>raw</span>\n\n<img src=\"https://example.test/image.jpg\" alt=\"Example\" />")
                .expect("GFM rendering succeeds");

        assert!(html.contains("<table>"));
        assert!(html.contains("<th>Name</th>"));
        assert!(html.contains("&lt;span&gt;raw&lt;/span&gt;"));
        assert!(html.contains(
            "&lt;img src=&quot;https://example.test/image.jpg&quot; alt=&quot;Example&quot; /&gt;"
        ));
    }

    #[test]
    fn media_references_count_external_and_local_images_in_document_order() {
        let references = media_references(
            "![External](https://example.test/image.jpg)\n\n![Local](/uploads/image.jpg)\n\n![Reference][image]\n\n[image]: /uploads/reference.jpg",
        );

        assert_eq!(references.len(), 2);
        assert_eq!(references[0].document_index, 0);
        assert_eq!(references[1].document_index, 1);
        assert_eq!(references[1].url, "/uploads/image.jpg");
    }

    #[test]
    fn renders_image_style_video_urls_as_safe_video_elements() {
        let html = render_gfm_html(
            "![A short clip](/uploads/clip.WEBM?download=1 \"Clip title\")\n\n![Image](/uploads/image.jpg)",
        )
        .expect("GFM rendering succeeds");

        assert!(html.contains(
            "<video controls src=\"/uploads/clip.WEBM?download=1\" title=\"Clip title\">A short clip</video>"
        ));
        assert!(html.contains("<img src=\"/uploads/image.jpg\" alt=\"Image\" />"));
    }

    #[test]
    fn video_urls_are_detected_by_extension_without_accepting_other_files() {
        assert!(is_video_url("/uploads/clip.mp4"));
        assert!(is_video_url("/uploads/clip.ogv#preview"));
        assert!(is_video_url("https://example.test/clip.MOV?download=1"));
        assert!(!is_video_url("/uploads/clip.jpg"));
        assert!(!is_video_url("/uploads/clip.mp4.txt"));
    }

    #[test]
    fn does_not_render_dangerous_video_urls() {
        let html =
            render_gfm_html("![Unsafe](javascript:alert(1).mp4)").expect("GFM rendering succeeds");

        assert!(html.contains("<video controls src=\"\">Unsafe</video>"));
        assert!(!html.contains("javascript:"));
    }
}
