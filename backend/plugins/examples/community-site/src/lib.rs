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
    let query = format!("type={content_type}&slug={slug}&fields=title,node.html&limit=1");
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
                (PreEscaped(content))
            } @else {
                p { "The requested published CMS entry does not exist." }
            }
        }
    })
}

fn render_events() -> Result<Markup, PluginError> {
    let events = entries(
        "type=event&fields=title,slug,meta,node.html&ordering=start_time+ASC&limit=24",
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
    let summary = entry_html(event);

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

fn node_html(node: &Value) -> Vec<String> {
    if let Some(blocks) = node.get("blocks").and_then(Value::as_array) {
        return blocks.iter().flat_map(node_html).collect();
    }
    node.get("html")
        .and_then(Value::as_str)
        .filter(|html| !html.trim().is_empty())
        .map(|html| vec![html.to_string()])
        .unwrap_or_default()
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

    use super::{node_html, page, render_event_list_item};
    use serde_json::json;

    #[test]
    fn page_escapes_text_and_keeps_structured_markup() {
        let rendered = page("Unsafe <title>", html! { p { "Safe content" } });

        assert!(rendered.starts_with("<!DOCTYPE html>"));
        assert!(rendered.contains("<title>Unsafe &lt;title&gt;</title>"));
        assert!(rendered.contains("<main><p>Safe content</p></main>"));
    }

    #[test]
    fn keeps_raw_markdown_html_escaped() {
        let nodes = node_html(&json!({
            "html": "<p>&lt;i&gt;Raw HTML&lt;/i&gt;</p>"
        }));

        assert_eq!(nodes, vec!["<p>&lt;i&gt;Raw HTML&lt;/i&gt;</p>"]);
        assert!(!nodes[0].contains("<i>"));
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
