mod bindings {
    wit_bindgen::generate!({
        path: "../../wit/nur-cms-plugin",
        world: "cms-plugin",
    });
}

use bindings::{
    exports::nur::cms::http_handler::{Guest, PluginError, Request, Response},
    nur::cms::{content, types::Header},
};
use maud::{DOCTYPE, Markup, PreEscaped, html};
use serde_json::Value;

struct CommunitySite;

impl Guest for CommunitySite {
    fn handle(request: Request) -> Result<Response, PluginError> {
        let html = match request.route_id.as_str() {
            "home" => page(
                "Home",
                render_entry("article", "first-article", content::OutputType::Html)?,
            ),
            "privacy" => page(
                "Privacy Policy",
                render_entry("page", "privacy-policy", content::OutputType::Html)?,
            ),
            "events" => page("Events", render_events()?),
            "event" => page("Event", render_event(path_param(&request, "slug")?)?),
            _ => return Err(PluginError::NotFound),
        };

        Ok(html_response(html))
    }
}

fn render_entry(
    content_type: &str,
    slug: &str,
    output: content::OutputType,
) -> Result<Markup, PluginError> {
    let query = format!("type={content_type}&slug={slug}&fields=title,node.text&limit=1");
    let entry = entries(&query, output)?.into_iter().next();

    let title = entry
        .as_ref()
        .and_then(|entry| entry.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Page not found");
    let content = entry
        .as_ref()
        .map(entry_html)
        .filter(|html| !html.is_empty());

    Ok(html! {
        article {
            h1 { (title) }
            @if let Some(content) = content {
                (PreEscaped(without_leading_h1(&content)))
            } @else {
                p { "The requested published CMS entry does not exist." }
            }
        }
    })
}

fn render_events() -> Result<Markup, PluginError> {
    let events = entries(
        "type=event&fields=title,slug,meta,node.text&ordering=start_time+ASC&limit=24",
        content::OutputType::Html,
    )?;
    if events.is_empty() {
        return Ok(html! {
            article {
                h1 { "Events" }
                p { "No events are currently scheduled." }
            }
        });
    }

    Ok(html! {
        article {
            h1 { "Events" }
            ul class="events" {
                @for event in &events {
                    (render_event_list_item(event))
                }
            }
        }
    })
}

fn render_event_list_item(event: &Value) -> Markup {
    let title = event
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled event");
    let start_time = event
        .get("meta")
        .and_then(|meta| meta.get("start_time"))
        .and_then(Value::as_str);
    let slug = event
        .get("slug")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let summary = without_leading_h1(&entry_html(event));

    html! {
        li {
            h2 {
                @if slug.is_empty() {
                    (title)
                } @else {
                    a href=(format!("/events/{slug}")) { (title) }
                }
            }
            @if let Some(start_time) = start_time {
                time { (start_time) }
            }
            (PreEscaped(summary))
        }
    }
}

fn path_param<'a>(request: &'a Request, name: &str) -> Result<&'a str, PluginError> {
    request
        .path_params
        .iter()
        .find(|param| param.name == name)
        .map(|param| param.value.as_str())
        .ok_or(PluginError::NotFound)
}

fn render_event(slug: &str) -> Result<Markup, PluginError> {
    if !valid_slug(slug) {
        return Err(PluginError::NotFound);
    }
    render_entry("event", slug, content::OutputType::Html)
}

fn entries(query: &str, output: content::OutputType) -> Result<Vec<Value>, PluginError> {
    let bytes = content::published_entries(query, output)?;
    let response: Value = serde_json::from_slice(&bytes)
        .map_err(|_| PluginError::Failed("CMS returned an invalid content response".into()))?;
    Ok(response
        .get("results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn entry_html(entry: &Value) -> String {
    entry
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(node_html)
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Entries commonly use their title as the first Markdown heading. List views
/// already render that title as a linked heading, so omit only this leading h1.
fn without_leading_h1(html: &str) -> String {
    let trimmed = html.trim_start();
    let Some(heading) = trimmed.strip_prefix("<h1>") else {
        return html.to_string();
    };
    let Some(end) = heading.find("</h1>") else {
        return html.to_string();
    };

    heading[end + "</h1>".len()..].trim_start().to_string()
}

fn node_html(node: &Value) -> Vec<String> {
    if let Some(blocks) = node.get("blocks").and_then(Value::as_array) {
        return blocks.iter().flat_map(node_html).collect();
    }
    node.get("html")
        .and_then(Value::as_str)
        .filter(|html| !html.trim().is_empty())
        .map(|html| vec![restore_allowed_raw_html(html)])
        .unwrap_or_default()
}

/// The CMS HTML output escapes raw Markdown HTML by design. This example
/// deliberately restores a small, attribute-restricted subset used by its
/// trusted demo content instead of accepting arbitrary tags.
fn restore_allowed_raw_html(html: &str) -> String {
    // Validate the value of a "class" attribute.
    fn valid_class(value: &str) -> bool {
        !value.is_empty()
            && value.len() <= 256
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b' ' | b'-'))
    }

    // Validate an image source URL.
    // Only HTTPS URLs and a limited set of safe characters are allowed.
    fn valid_src(value: &str) -> bool {
        value.len() <= 2048
            && value.starts_with("https://")
            && value.bytes().all(|b| {
                b.is_ascii_alphanumeric()
                    || matches!(b, b':' | b'/' | b'.' | b'?' | b'=' | b'_' | b'%' | b'-')
            })
    }

    // Validate the image alt text.
    fn valid_alt(value: &str) -> bool {
        value.len() <= 256
            && !value
                .bytes()
                .any(|b| matches!(b, b'"' | b'&' | b'<' | b'>'))
    }

    // Preallocate roughly enough space for the result.
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    'scan: while !rest.is_empty() {
        // Restore </div>.
        if let Some(next) = rest.strip_prefix("&lt;/div&gt;") {
            out.push_str("</div>");
            rest = next;
            continue;
        }

        // Restore <i>.
        if let Some(next) = rest.strip_prefix("&lt;i&gt;") {
            out.push_str("<i>");
            rest = next;
            continue;
        }

        // Restore </i>.
        if let Some(next) = rest.strip_prefix("&lt;/i&gt;") {
            out.push_str("</i>");
            rest = next;
            continue;
        }

        // Restore a plain <div>.
        if let Some(next) = rest.strip_prefix("&lt;div&gt;") {
            out.push_str("<div>");
            rest = next;
            continue;
        }

        // `markdown` escapes attribute quotes in raw HTML as `&quot;`. Accept
        // that actual renderer output as well as literal quotes for callers
        // that provide already-escaped HTML themselves.
        for (prefix, suffix) in [
            (r#"&lt;div class=&quot;"#, "&quot;&gt;"),
            (r#"&lt;div class=""#, r#""&gt;"#),
        ] {
            if let Some(after_prefix) = rest.strip_prefix(prefix)
                && let Some(end) = after_prefix.find(suffix)
            {
                let class = &after_prefix[..end];
                if valid_class(class) {
                    out.push_str(r#"<div class=""#);
                    out.push_str(class);
                    out.push_str(r#"">"#);
                    rest = &after_prefix[end + suffix.len()..];
                    continue 'scan;
                }
            }
        }

        // Restore an image only if both attributes match the restrictive
        // allowlist. Attribute delimiters can be either escaped `&quot;` values
        // (the CMS renderer output) or literal quotes.
        for (prefix, separator, quote) in [
            (r#"&lt;img src=&quot;"#, "&quot; alt=&quot;", "&quot;"),
            (r#"&lt;img src=""#, r#"" alt=""#, r#"""#),
        ] {
            let Some(after_prefix) = rest.strip_prefix(prefix) else {
                continue;
            };
            let Some(src_end) = after_prefix.find(separator) else {
                continue;
            };
            let src = &after_prefix[..src_end];
            let after_src = &after_prefix[src_end + separator.len()..];
            if !valid_src(src) {
                continue;
            }
            let Some(alt_end) = after_src.find(quote) else {
                continue;
            };
            let alt = &after_src[..alt_end];
            let after_alt = &after_src[alt_end + quote.len()..];
            if !valid_alt(alt) {
                continue;
            }

            // Allow whitespace before the optional self-closing slash.
            let whitespace_len = after_alt
                .bytes()
                .take_while(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
                .count();
            let ending = &after_alt[whitespace_len..];
            let consumed = if ending.starts_with("/&gt;") {
                Some(whitespace_len + "/&gt;".len())
            } else if ending.starts_with("&gt;") {
                Some(whitespace_len + "&gt;".len())
            } else {
                None
            };
            if let Some(consumed) = consumed {
                out.push_str(r#"<img src=""#);
                out.push_str(src);
                out.push_str(r#"" alt=""#);
                out.push_str(alt);
                out.push_str(r#"" />"#);
                rest = &after_alt[consumed..];
                continue 'scan;
            }
        }

        // No allowed HTML pattern matched.
        // Copy the next Unicode character unchanged.
        let ch = rest.chars().next().unwrap();
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }

    out
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 160
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn page(title: &str, content: Markup) -> String {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                link rel="stylesheet" href="/plugins/community-site/assets/site.css";
            }
            body {
                header {
                    a href="/" { "Community" }
                    nav {
                        a href="/events" { "Events" }
                        a href="/privacy" { "Privacy" }
                    }
                }
                main { (content) }
            }
        }
    }
    .into_string()
}

fn html_response(body: String) -> Response {
    Response {
        status: 200,
        headers: vec![Header {
            name: "content-type".into(),
            value: "text/html; charset=utf-8".into(),
        }],
        body: body.into_bytes(),
    }
}

bindings::export!(CommunitySite with_types_in bindings);

#[cfg(test)]
mod tests {
    use maud::html;

    use super::{page, render_event_list_item, restore_allowed_raw_html};
    use serde_json::json;

    #[test]
    fn restores_the_allowed_demo_html_only() {
        let html = r#"&lt;div class=&quot;flex justify-center&quot;&gt;&lt;div class=&quot;grid&quot;&gt;

&lt;img src=&quot;https://picsum.photos/id/237/200/300&quot; alt=&quot;image1&quot; /&gt;

&lt;img src=&quot;https://picsum.photos/id/29/200/300&quot; alt=&quot;image2&quot; /&gt;

&lt;img src=&quot;https://picsum.photos/id/19/200/300&quot; alt=&quot;image3&quot; /&gt;

&lt;/div&gt;&lt;/div&gt; Here is &lt;i&gt;inline&lt;/i&gt; HTML. &lt;script&gt;alert(1)&lt;/script&gt;"#;
        let restored = restore_allowed_raw_html(html);

        assert!(restored.contains(r#"<div class="flex justify-center">"#));
        assert!(restored.contains(r#"<div class="grid">"#));
        assert!(
            restored.contains(r#"<img src="https://picsum.photos/id/237/200/300" alt="image1" />"#)
        );
        assert!(
            restored.contains(r#"<img src="https://picsum.photos/id/29/200/300" alt="image2" />"#)
        );
        assert!(
            restored.contains(r#"<img src="https://picsum.photos/id/19/200/300" alt="image3" />"#)
        );
        assert!(restored.contains("Here is <i>inline</i> HTML."));
        assert!(restored.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    }

    #[test]
    fn page_escapes_text_and_keeps_structured_markup() {
        let rendered = page("Unsafe <title>", html! { p { "Safe content" } });

        assert!(rendered.starts_with("<!DOCTYPE html>"));
        assert!(rendered.contains("<title>Unsafe &lt;title&gt;</title>"));
        assert!(rendered.contains("<main><p>Safe content</p></main>"));
    }

    #[test]
    fn event_markup_escapes_database_values_but_keeps_rendered_node_html() {
        let rendered = render_event_list_item(&json!({
            "title": "Meeting <script>",
            "slug": "meeting\" onclick=\"alert(1)",
            "meta": { "start_time": "2026-09-02 <unsafe>" },
            "nodes": [{ "html": "<p>Rendered summary</p>" }]
        }))
        .into_string();

        assert!(rendered.contains("Meeting &lt;script&gt;"));
        assert!(rendered.contains("&quot; onclick=&quot;"));
        assert!(rendered.contains("2026-09-02 &lt;unsafe&gt;"));
        assert!(rendered.contains("<p>Rendered summary</p>"));
    }
}
