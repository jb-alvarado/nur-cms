use markdown::{Options, to_html_with_options};

use crate::utils::errors::NurError;

/// Renders GitHub-Flavored Markdown to safe HTML.
///
/// Raw HTML in the Markdown input remains escaped by the renderer.
pub fn render_gfm_html(markdown: &str) -> Result<String, NurError> {
    Ok(to_html_with_options(markdown, &Options::gfm())?)
}

#[cfg(test)]
mod tests {
    use super::render_gfm_html;

    #[test]
    fn renders_gfm_tables_and_keeps_raw_html_escaped() {
        let html =
            render_gfm_html("| Name | Value |\n| --- | --- |\n| One | 1 |\n\n<span>raw</span>")
                .expect("GFM rendering succeeds");

        assert!(html.contains("<table>"));
        assert!(html.contains("<th>Name</th>"));
        assert!(html.contains("&lt;span&gt;raw&lt;/span&gt;"));
    }
}
