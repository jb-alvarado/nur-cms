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
use regex::{Captures, Regex};
use serde_json::Value;
use std::sync::OnceLock;

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
) -> Result<String, PluginError> {
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
        .filter(|html| !html.is_empty())
        .unwrap_or_else(|| "<p>The requested published CMS entry does not exist.</p>".into());

    Ok(format!(
        "<article><h1>{}</h1>{}</article>",
        escape(title),
        without_leading_h1(&content)
    ))
}

fn render_events() -> Result<String, PluginError> {
    let events = entries(
        "type=event&fields=title,slug,meta,node.text&ordering=start_time+ASC&limit=24",
        content::OutputType::Html,
    )?;
    if events.is_empty() {
        return Ok(
            "<article><h1>Events</h1><p>No events are currently scheduled.</p></article>".into(),
        );
    }

    let items = events
        .iter()
        .map(|event| {
            let title = event
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Untitled event");
            let start_time = event
                .get("meta")
                .and_then(|meta| meta.get("start_time"))
                .and_then(Value::as_str)
                .map(|time| format!("<time>{}</time>", escape(time)))
                .unwrap_or_default();
            let slug = event
                .get("slug")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let link = if slug.is_empty() {
                escape(title)
            } else {
                format!(
                    "<a href=\"/events/{}\">{}</a>",
                    escape_attribute(slug),
                    escape(title)
                )
            };
            let summary = without_leading_h1(&entry_html(event));
            format!("<li><h2>{link}</h2>{start_time}{summary}</li>")
        })
        .collect::<String>();

    Ok(format!(
        "<article><h1>Events</h1><ul class=\"events\">{items}</ul></article>"
    ))
}

fn path_param<'a>(request: &'a Request, name: &str) -> Result<&'a str, PluginError> {
    request
        .path_params
        .iter()
        .find(|param| param.name == name)
        .map(|param| param.value.as_str())
        .ok_or(PluginError::NotFound)
}

fn render_event(slug: &str) -> Result<String, PluginError> {
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
    static DIV: OnceLock<Regex> = OnceLock::new();
    static IMAGE: OnceLock<Regex> = OnceLock::new();
    let div = DIV.get_or_init(|| {
        Regex::new(r#"&lt;div(?: class="([A-Za-z0-9_: -]{1,256})")?&gt;"#)
            .expect("the allowed div regex is valid")
    });
    let image = IMAGE.get_or_init(|| {
        Regex::new(
            r#"&lt;img src="(https://[A-Za-z0-9./?=_%-]{1,2048})" alt="([^"&<>]{0,256})"[ \t\r\n]*/?&gt;"#,
        )
        .expect("the allowed image regex is valid")
    });

    let html = div.replace_all(html, |captures: &Captures<'_>| {
        captures.get(1).map_or_else(
            || "<div>".to_string(),
            |class| format!(r#"<div class="{}">"#, class.as_str()),
        )
    });
    let html = image.replace_all(&html, |captures: &Captures<'_>| {
        format!(r#"<img src="{}" alt="{}" />"#, &captures[1], &captures[2])
    });

    html.replace("&lt;/div&gt;", "</div>")
        .replace("&lt;i&gt;", "<i>")
        .replace("&lt;/i&gt;", "</i>")
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 160
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn escape_attribute(value: &str) -> String {
    escape(value)
}

fn page(title: &str, content: String) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{}</title><link rel=\"stylesheet\" href=\"/plugins/community-site/assets/site.css\"></head><body><header><a href=\"/\">Community</a><nav><a href=\"/events\">Events</a><a href=\"/privacy\">Privacy</a></nav></header><main>{content}</main></body></html>",
        escape(title),
    )
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
    use super::restore_allowed_raw_html;

    #[test]
    fn restores_the_allowed_demo_html_only() {
        let html = r#"&lt;div class="flex justify-center"&gt;&lt;img src="https://picsum.photos/id/237/200/300" alt="image1" /&gt;&lt;/div&gt; Here is &lt;i&gt;inline&lt;/i&gt; HTML. &lt;script&gt;alert(1)&lt;/script&gt;"#;
        let restored = restore_allowed_raw_html(html);

        assert!(restored.contains(r#"<div class="flex justify-center">"#));
        assert!(
            restored.contains(r#"<img src="https://picsum.photos/id/237/200/300" alt="image1" />"#)
        );
        assert!(restored.contains("Here is <i>inline</i> HTML."));
        assert!(restored.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    }
}
