use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt::{self, Write},
};

use comrak::{
    Arena, Options, create_formatter,
    html::{ChildRendering, Context, dangerous_url, format_node_default},
    nodes::{AstNode, Node, NodeLink, NodeValue, Sourcepos},
    parse_document,
};

use crate::{
    PUBLIC_UPLOADS,
    db::serialize::{MediaSerializer, MediaVariantSerializer, MediaVideoVariantSerializer},
    utils::errors::NurError,
};

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

pub(crate) fn media_location(raw_url: &str) -> Option<(String, String)> {
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

    Some((
        if dir.is_empty() {
            "/".to_string()
        } else {
            dir.to_string()
        },
        filename.to_string(),
    ))
}

#[derive(Default)]
struct HtmlMediaLookup {
    by_location: HashMap<(String, String), VecDeque<MediaSerializer>>,
    custom_nodes: HashSet<usize>,
    max_image_variant_width: Option<i32>,
}

impl HtmlMediaLookup {
    fn from_media(media: &[MediaSerializer], max_image_variant_width: Option<i32>) -> Self {
        let mut lookup = Self {
            max_image_variant_width,
            ..Self::default()
        };
        for item in media {
            if let (Some(path), Some(filename)) = (&item.path, &item.filename) {
                lookup
                    .by_location
                    .entry((path.clone(), filename.clone()))
                    .or_default()
                    .push_back(item.clone());
            }
        }
        lookup
    }

    fn pop_for_url(&mut self, url: &str) -> Option<MediaSerializer> {
        self.by_location.get_mut(&media_location(url)?)?.pop_front()
    }
}

fn node_key(node: Node<'_>) -> usize {
    node as *const AstNode<'_> as usize
}

fn media_url(media: &MediaSerializer, filename: &str) -> Option<String> {
    let path = media.path.as_deref()?;
    if path == "/" {
        return Some(format!("/{filename}"));
    }

    let path = path.trim_end_matches('/');
    (!path.is_empty()).then(|| format!("{path}/{filename}"))
}

fn image_mime_type(filename: &str) -> Option<String> {
    mime_guess::from_path(filename)
        .first_raw()
        .filter(|mime| mime.starts_with("image/"))
        .map(str::to_string)
}

fn video_mime_type(variant: &MediaVideoVariantSerializer) -> Option<String> {
    match variant.container.as_str() {
        "mp4" | "m4v" => Some("video/mp4".to_string()),
        "mov" => Some("video/quicktime".to_string()),
        "webm" => Some("video/webm".to_string()),
        "ogg" | "ogv" => Some("video/ogg".to_string()),
        _ => mime_guess::from_path(&variant.filename)
            .first_raw()
            .filter(|mime| mime.starts_with("video/"))
            .map(str::to_string),
    }
}

/// Maps FFmpeg/ffprobe codec names to codec identifiers understood by browser
/// media APIs. Profile, level and bit-depth suffixes are intentionally omitted:
/// those properties are not currently persisted for generated variants, and a
/// precise-looking but incorrect RFC 6381 string is worse than the valid base
/// identifier.
fn browser_video_codec(codec: &str) -> &str {
    match codec.trim().to_ascii_lowercase().as_str() {
        "h264" | "avc" | "libx264" => "avc1",
        "vp9" | "libvpx-vp9" => "vp09",
        "av1" | "libaom-av1" | "libsvtav1" | "svt-av1" => "av01",
        _ => codec,
    }
}

fn image_format_rank(mime: &str) -> usize {
    match mime {
        "image/avif" => 0,
        "image/webp" => 1,
        _ => 2,
    }
}

fn video_format_rank(mime: &str) -> usize {
    match mime {
        "video/mp4" => 0,
        "video/webm" => 1,
        _ => 2,
    }
}

fn preferred_poster_variant(
    variants: &[MediaVariantSerializer],
    preferred_width: Option<i32>,
) -> Option<&MediaVariantSerializer> {
    let valid_variants = variants
        .iter()
        .filter(|variant| variant.width > 0 && image_mime_type(&variant.filename).is_some())
        .collect::<Vec<_>>();
    let preferred_width = match preferred_width {
        Some(width) if valid_variants.iter().any(|variant| variant.width == width) => width,
        Some(width) => valid_variants
            .iter()
            .filter(|variant| variant.width > width)
            .map(|variant| variant.width)
            .min()
            .or_else(|| valid_variants.iter().map(|variant| variant.width).max())?,
        None => valid_variants.iter().map(|variant| variant.width).max()?,
    };

    valid_variants
        .into_iter()
        .filter(|variant| variant.width == preferred_width)
        .max_by_key(|variant| {
            usize::from(image_mime_type(&variant.filename).as_deref() == Some("image/webp"))
        })
}

fn image_sources(
    variants: &[MediaVariantSerializer],
    max_width: Option<i32>,
) -> Vec<(String, Vec<&MediaVariantSerializer>)> {
    let Some(max_width) = max_width else {
        return Vec::new();
    };
    let mut sources = Vec::<(String, Vec<&MediaVariantSerializer>)>::new();

    for variant in variants {
        if variant.width <= 0 || variant.width > max_width {
            continue;
        }
        let Some(mime) = image_mime_type(&variant.filename) else {
            continue;
        };
        if let Some((_, variants)) = sources.iter_mut().find(|(kind, _)| *kind == mime) {
            variants.push(variant);
        } else {
            sources.push((mime, vec![variant]));
        }
    }

    for (_, variants) in &mut sources {
        variants.sort_by_key(|variant| variant.width);
    }
    sources.sort_by_key(|(mime, _)| image_format_rank(mime));
    sources
}

fn write_safe_url<T>(context: &mut Context<T>, url: &str) -> fmt::Result {
    if context.options.render.r#unsafe || !dangerous_url(url) {
        if let Some(rewriter) = &context.options.extension.image_url_rewriter {
            context.escape_href(&rewriter.to_html(url))?;
        } else {
            context.escape_href(url)?;
        }
    }
    Ok(())
}

fn write_image_element<T>(
    context: &mut Context<T>,
    src: &str,
    image: &NodeLink,
    dimensions: Option<(i32, i32)>,
    alt: &str,
) -> fmt::Result {
    context.write_str("<img src=\"")?;
    write_safe_url(context, src)?;
    context.write_str("\" alt=\"")?;
    context.escape(alt)?;
    context.write_str("\"")?;
    if !image.title.is_empty() {
        context.write_str(" title=\"")?;
        context.escape(&image.title)?;
        context.write_str("\"")?;
    }
    if let Some((width, height)) = dimensions {
        if width > 0 {
            write!(context, " width=\"{width}\"")?;
        }
        if height > 0 {
            write!(context, " height=\"{height}\"")?;
        }
    }
    context.write_str(" />")
}

fn write_responsive_picture<T>(
    context: &mut Context<T>,
    image: &NodeLink,
    media: &MediaSerializer,
    alt: &str,
    max_image_variant_width: Option<i32>,
) -> Result<bool, fmt::Error> {
    let sources = image_sources(&media.variants, max_image_variant_width);
    if sources.is_empty() {
        return Ok(false);
    }

    let original_mime = image_mime_type(&image.url);
    let fallback_variant = sources
        .iter()
        .find(|(mime, _)| Some(mime.as_str()) == original_mime.as_deref())
        .or_else(|| {
            sources
                .iter()
                .find(|(mime, _)| matches!(mime.as_str(), "image/jpeg" | "image/png"))
        })
        .or_else(|| sources.first())
        .and_then(|(_, variants)| variants.last());
    let Some(fallback_variant) = fallback_variant else {
        return Ok(false);
    };
    let Some(fallback_url) = media_url(media, &fallback_variant.filename) else {
        return Ok(false);
    };
    let fallback_dimensions = (fallback_variant.width, fallback_variant.height);

    context.write_str("<picture>")?;
    for (mime, variants) in sources {
        context.write_str("<source type=\"")?;
        context.escape(&mime)?;
        context.write_str("\" srcset=\"")?;
        for (index, variant) in variants.iter().enumerate() {
            if index > 0 {
                context.write_str(", ")?;
            }
            if let Some(url) = media_url(media, &variant.filename) {
                write_safe_url(context, &url)?;
            }
            write!(context, " {}w", variant.width)?;
        }
        context.write_str("\" />")?;
    }
    write_image_element(
        context,
        &fallback_url,
        image,
        Some(fallback_dimensions),
        alt,
    )?;
    context.write_str("</picture>")?;
    Ok(true)
}

fn write_video<T>(
    context: &mut Context<T>,
    image: &NodeLink,
    media: Option<&MediaSerializer>,
    alt: &str,
    preferred_poster_width: Option<i32>,
) -> fmt::Result {
    let Some(media) = media else {
        context.write_str("<video controls src=\"")?;
        write_safe_url(context, &image.url)?;
        context.write_str("\"")?;
        if !image.title.is_empty() {
            context.write_str(" title=\"")?;
            context.escape(&image.title)?;
            context.write_str("\"")?;
        }
        context.write_str(">")?;
        context.escape(alt)?;
        return context.write_str("</video>");
    };

    let mut variants = media
        .video_variants
        .iter()
        .filter_map(|variant| {
            Some((
                variant,
                media_url(media, &variant.filename)?,
                video_mime_type(variant)?,
            ))
        })
        .collect::<Vec<_>>();
    if variants.is_empty() {
        return write_video(context, image, None, alt, preferred_poster_width);
    }
    variants.sort_by(|(left, _, left_mime), (right, _, right_mime)| {
        video_format_rank(left_mime)
            .cmp(&video_format_rank(right_mime))
            .then_with(|| left.width.cmp(&right.width))
            .then_with(|| left.height.cmp(&right.height))
    });

    let data_sources = serde_json::to_string(
        &variants
            .iter()
            .map(|(variant, url, mime)| {
                serde_json::json!({
                    "src": url,
                    "type": mime,
                    "codec": browser_video_codec(&variant.video_codec),
                    "width": variant.width,
                    "height": variant.height,
                })
            })
            .collect::<Vec<_>>(),
    )
    .map_err(|_| fmt::Error)?;
    let Some(fallback) = variants
        .iter()
        .filter(|(variant, _, _)| browser_video_codec(&variant.video_codec) == "avc1")
        .max_by_key(|(variant, _, _)| (variant.width, variant.height))
        .or_else(|| {
            variants
                .iter()
                .max_by_key(|(variant, _, _)| (variant.width, variant.height))
        })
    else {
        return write_video(context, image, None, alt, preferred_poster_width);
    };

    context.write_str("<video controls")?;
    if let Some(poster) = preferred_poster_variant(&media.variants, preferred_poster_width)
        && let Some(url) = media_url(media, &poster.filename)
    {
        context.write_str(" poster=\"")?;
        write_safe_url(context, &url)?;
        context.write_str("\"")?;
    }
    if !image.title.is_empty() {
        context.write_str(" title=\"")?;
        context.escape(&image.title)?;
        context.write_str("\"")?;
    }
    context.write_str(" data-sources=\"")?;
    context.escape(&data_sources)?;
    context.write_str("\">")?;
    context.write_str("<source src=\"")?;
    write_safe_url(context, &fallback.1)?;
    context.write_str("\" type=\"")?;
    context.escape(&fallback.2)?;
    context.write_str("\" />")?;
    context.escape(alt)?;
    context.write_str("</video>")
}

create_formatter!(ResponsiveMediaHtmlFormatter<HtmlMediaLookup>, {
    NodeValue::Image(ref image) => |context, node, entering| {
        let key = node_key(node);
        if !entering {
            if context.user.custom_nodes.remove(&key) {
                return Ok(ChildRendering::Skip);
            }
            return format_node_default(context, node, entering);
        }

        let media = context.user.pop_for_url(&image.url);
        let alt = node.collect_text();
        let max_image_variant_width = context.user.max_image_variant_width;
        if is_video_url(&image.url) {
            write_video(
                context,
                image,
                media.as_ref(),
                &alt,
                max_image_variant_width,
            )?;
            context.user.custom_nodes.insert(key);
            return Ok(ChildRendering::Skip);
        }
        if let Some(media) = media.as_ref()
            && write_responsive_picture(
                context,
                image,
                media,
                &alt,
                max_image_variant_width,
            )?
        {
            context.user.custom_nodes.insert(key);
            return Ok(ChildRendering::Skip);
        }

        return format_node_default(context, node, entering);
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

/// Renders GitHub-Flavored Markdown to safe HTML. Linked CMS media is emitted
/// as responsive image or video sources when processed variants are available.
/// Raw HTML remains escaped by Comrak unless its unsafe renderer option is
/// explicitly enabled (which this CMS never does).
pub fn render_gfm_html(
    markdown: &str,
    media: &[MediaSerializer],
    max_image_variant_width: Option<i32>,
) -> Result<String, NurError> {
    let arena = Arena::new();
    let options = gfm_options();
    let root = parse_document(&arena, markdown, &options);
    let mut html = String::with_capacity(markdown.len());
    ResponsiveMediaHtmlFormatter::format_document(
        root,
        &options,
        &mut html,
        HtmlMediaLookup::from_media(media, max_image_variant_width),
    )
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
    use crate::db::serialize::{
        MediaSerializer, MediaVariantSerializer, MediaVideoVariantSerializer,
    };

    use super::{is_video_url, media_references, render_gfm_html};

    #[test]
    fn renders_gfm_tables_and_keeps_raw_html_escaped() {
        let html =
            render_gfm_html("| Name | Value |\n| --- | --- |\n| One | 1 |\n\n<span>raw</span>\n\n<img src=\"https://example.test/image.jpg\" alt=\"Example\" />", &[], None)
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
            &[],
            None,
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
        let html = render_gfm_html("![Unsafe](javascript:alert(1).mp4)", &[], None)
            .expect("GFM rendering succeeds");

        assert!(html.contains("<video controls src=\"\">Unsafe</video>"));
        assert!(!html.contains("javascript:"));
    }

    #[test]
    fn renders_linked_images_as_responsive_pictures() {
        let media = MediaSerializer {
            path: Some("/uploads/2026/09".into()),
            filename: Some("cover.jpg".into()),
            width: Some(1600),
            height: Some(900),
            variants: vec![
                MediaVariantSerializer {
                    id: 1,
                    width: 640,
                    height: 360,
                    filename: "cover-640.webp".into(),
                },
                MediaVariantSerializer {
                    id: 2,
                    width: 1280,
                    height: 720,
                    filename: "cover-1280.webp".into(),
                },
                MediaVariantSerializer {
                    id: 3,
                    width: 640,
                    height: 360,
                    filename: "cover-640.avif".into(),
                },
            ],
            ..MediaSerializer::default()
        };

        let html = render_gfm_html("![Cover](/uploads/2026/09/cover.jpg)", &[media], Some(640))
            .expect("GFM rendering succeeds");

        assert!(html.contains("<picture>"));
        assert!(html.contains("type=\"image/avif\""));
        assert!(html.contains("cover-640.avif 640w"));
        assert!(html.contains("type=\"image/webp\""));
        assert!(html.contains("cover-640.webp 640w"));
        assert!(!html.contains("cover-1280.webp"));
        assert!(html.contains(
            "<img src=\"/uploads/2026/09/cover-640.avif\" alt=\"Cover\" width=\"640\" height=\"360\" />"
        ));
    }

    #[test]
    fn renders_linked_videos_with_all_available_transcodes() {
        let media = MediaSerializer {
            path: Some("/uploads/2026/09".into()),
            filename: Some("clip.mp4".into()),
            variants: vec![
                MediaVariantSerializer {
                    id: 1,
                    width: 640,
                    height: 360,
                    filename: "clip-poster-640.jpg".into(),
                },
                MediaVariantSerializer {
                    id: 2,
                    width: 1280,
                    height: 720,
                    filename: "clip-poster-1280.jpg".into(),
                },
                MediaVariantSerializer {
                    id: 3,
                    width: 1280,
                    height: 720,
                    filename: "clip-poster-1280.webp".into(),
                },
            ],
            video_variants: vec![
                MediaVideoVariantSerializer {
                    id: 1,
                    kind: "transcode".into(),
                    profile: "h264-720".into(),
                    width: 1280,
                    height: 720,
                    container: "mp4".into(),
                    video_codec: "h264".into(),
                    audio_codec: Some("aac".into()),
                    filename: "clip-h264-720.mp4".into(),
                    size: 1,
                    duration_ms: Some(5_000),
                },
                MediaVideoVariantSerializer {
                    id: 2,
                    kind: "transcode".into(),
                    profile: "av1-1440".into(),
                    width: 2560,
                    height: 1440,
                    container: "webm".into(),
                    video_codec: "av1".into(),
                    audio_codec: Some("opus".into()),
                    filename: "clip-av1-1440.webm".into(),
                    size: 1,
                    duration_ms: Some(5_000),
                },
                MediaVideoVariantSerializer {
                    id: 3,
                    kind: "transcode".into(),
                    profile: "h264-1080".into(),
                    width: 1920,
                    height: 1080,
                    container: "mp4".into(),
                    video_codec: "h264".into(),
                    audio_codec: Some("aac".into()),
                    filename: "clip-h264-1080.mp4".into(),
                    size: 1,
                    duration_ms: Some(5_000),
                },
            ],
            ..MediaSerializer::default()
        };

        let html = render_gfm_html(
            "![Short clip](/uploads/2026/09/clip.mp4)",
            &[media],
            Some(1280),
        )
        .expect("GFM rendering succeeds");

        assert!(html.contains(
            "<video controls poster=\"/uploads/2026/09/clip-poster-1280.webp\" data-sources=\""
        ));
        assert!(html.contains("&quot;src&quot;:&quot;/uploads/2026/09/clip-h264-720.mp4&quot;"));
        assert!(html.contains("&quot;codec&quot;:&quot;avc1&quot;"));
        assert!(html.contains("&quot;src&quot;:&quot;/uploads/2026/09/clip-av1-1440.webm&quot;"));
        assert!(html.contains("&quot;codec&quot;:&quot;av01&quot;"));
        assert!(
            html.contains(
                "<source src=\"/uploads/2026/09/clip-h264-1080.mp4\" type=\"video/mp4\" />"
            )
        );
        assert!(!html.contains("<source src=\"/uploads/2026/09/clip-h264-720.mp4\""));
        assert!(!html.contains("<source src=\"/uploads/2026/09/clip-av1-1440.webm\""));
        assert!(html.contains("Short clip</video>"));
    }

    #[test]
    fn maps_common_video_encoders_to_browser_codec_identifiers() {
        assert_eq!(super::browser_video_codec("h264"), "avc1");
        assert_eq!(super::browser_video_codec("libx264"), "avc1");
        assert_eq!(super::browser_video_codec("vp9"), "vp09");
        assert_eq!(super::browser_video_codec("libvpx-vp9"), "vp09");
        assert_eq!(super::browser_video_codec("av1"), "av01");
        assert_eq!(super::browser_video_codec("libsvtav1"), "av01");
        assert_eq!(super::browser_video_codec("theora"), "theora");
    }

    #[test]
    fn prefers_webp_at_the_configured_poster_width_or_the_next_larger_one() {
        let variants = vec![
            MediaVariantSerializer {
                id: 1,
                width: 640,
                height: 360,
                filename: "poster-640.jpg".into(),
            },
            MediaVariantSerializer {
                id: 2,
                width: 1280,
                height: 720,
                filename: "poster-1280.jpg".into(),
            },
            MediaVariantSerializer {
                id: 3,
                width: 1280,
                height: 720,
                filename: "poster-1280.webp".into(),
            },
        ];

        assert_eq!(
            super::preferred_poster_variant(&variants, Some(1280))
                .map(|variant| variant.filename.as_str()),
            Some("poster-1280.webp")
        );
        assert_eq!(
            super::preferred_poster_variant(&variants, Some(1024))
                .map(|variant| variant.filename.as_str()),
            Some("poster-1280.webp")
        );
    }
}
