use std::collections::HashSet;

use axum::Json;
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use serde::{Deserialize, Serialize};

use crate::{
    CONFIG,
    db::{models::Role, serialize::MediaSerializer},
    utils::{errors::NurError, markdown::render_gfm_html_scoped},
};

const MAX_PREVIEW_NODES: usize = 128;
const MAX_PREVIEW_MARKDOWN_BYTES: usize = 512 * 1024;
const MAX_PREVIEW_KEY_BYTES: usize = 128;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkdownPreviewRequest {
    nodes: Vec<MarkdownPreviewInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkdownPreviewInput {
    key: String,
    markdown: String,
    #[serde(default)]
    media: Vec<MediaSerializer>,
}

#[derive(Debug, Serialize)]
pub struct MarkdownPreviewResponse {
    nodes: Vec<MarkdownPreviewOutput>,
}

#[derive(Debug, Serialize)]
struct MarkdownPreviewOutput {
    key: String,
    html: String,
}

fn render_preview_nodes(
    request: MarkdownPreviewRequest,
    max_image_variant_width: Option<i32>,
) -> Result<MarkdownPreviewResponse, NurError> {
    if request.nodes.len() > MAX_PREVIEW_NODES {
        return Err(NurError::BadRequest(
            "Too many Markdown preview nodes.".into(),
        ));
    }

    let total_markdown_bytes = request
        .nodes
        .iter()
        .try_fold(0usize, |total, node| total.checked_add(node.markdown.len()))
        .ok_or_else(|| NurError::BadRequest("Markdown preview is too large.".into()))?;
    let mut keys = HashSet::with_capacity(request.nodes.len());
    if total_markdown_bytes > MAX_PREVIEW_MARKDOWN_BYTES
        || request.nodes.iter().any(|node| {
            node.key.is_empty()
                || node.key.len() > MAX_PREVIEW_KEY_BYTES
                || !keys.insert(node.key.as_str())
        })
    {
        return Err(NurError::BadRequest(
            "Invalid Markdown preview request.".into(),
        ));
    }

    let nodes = request
        .nodes
        .into_iter()
        .map(|node| {
            let footnote_scope = format!("preview-{}", node.key);
            let html = render_gfm_html_scoped(
                &node.markdown,
                &node.media,
                max_image_variant_width,
                Some(&footnote_scope),
                None,
            )?;
            Ok(MarkdownPreviewOutput {
                key: node.key,
                html,
            })
        })
        .collect::<Result<Vec<_>, NurError>>()?;

    Ok(MarkdownPreviewResponse { nodes })
}

pub async fn markdown_preview(
    details: AuthDetails<Role>,
    Json(request): Json<MarkdownPreviewRequest>,
) -> Result<Json<MarkdownPreviewResponse>, NurError> {
    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return Err(NurError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ));
    }

    let max_image_variant_width = CONFIG.read().await.max_image_resolution();
    let response =
        tokio::task::spawn_blocking(move || render_preview_nodes(request, max_image_variant_width))
            .await
            .map_err(|_| NurError::InternalServerError)??;

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::{MarkdownPreviewInput, MarkdownPreviewRequest, render_preview_nodes};

    #[test]
    fn renders_preview_nodes_with_the_shared_comrak_configuration() {
        let response = render_preview_nodes(
            MarkdownPreviewRequest {
                nodes: vec![MarkdownPreviewInput {
                    key: "article-0".into(),
                    markdown: "==Marked== and Inline^[note]".into(),
                    media: Vec::new(),
                }],
            },
            None,
        )
        .expect("preview renders");

        assert_eq!(response.nodes[0].key, "article-0");
        assert!(response.nodes[0].html.contains("<mark>Marked</mark>"));
        assert!(
            response.nodes[0]
                .html
                .contains("id=\"fn-preview-article-0-__inline_1\"")
        );
    }

    #[test]
    fn rejects_oversized_preview_batches() {
        let request = MarkdownPreviewRequest {
            nodes: vec![MarkdownPreviewInput {
                key: "article-0".into(),
                markdown: "x".repeat(super::MAX_PREVIEW_MARKDOWN_BYTES + 1),
                media: Vec::new(),
            }],
        };

        assert!(render_preview_nodes(request, None).is_err());
    }
}
