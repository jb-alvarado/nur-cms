use crate::{
    db::{
        fields::OutputType,
        serialize::{ContentEntrySerializer, ContentNodeSerializer, NodeSerializer},
    },
    utils::{
        ast_serialize::{to_structure_root, truncate_structure_root},
        errors::NurError,
        markdown::render_gfm_html_scoped,
    },
};

/// Converts selected content-node Markdown to the requested public output format.
pub fn render_entry_nodes(
    entries: &mut [ContentEntrySerializer],
    output: &OutputType,
    character_limit: Option<i32>,
    keep_embeds: bool,
    max_image_variant_width: Option<i32>,
) -> Result<(), NurError> {
    if *output == OutputType::Markdown {
        return Ok(());
    }

    for (entry_index, entry) in entries.iter_mut().enumerate() {
        let mut node_index = 0usize;
        for node_wrapper in &mut entry.nodes {
            let nodes: Vec<&mut ContentNodeSerializer> = match node_wrapper {
                NodeSerializer::Single(node) => vec![node.as_mut()],
                NodeSerializer::Blocks(nodes) => nodes.iter_mut().collect(),
            };

            for node in nodes {
                let footnote_scope = node
                    .id
                    .map(|id| format!("node-{id}"))
                    .unwrap_or_else(|| format!("entry-{entry_index}-node-{node_index}"));
                node_index += 1;
                let text = node.text.take().unwrap_or_default();
                node.text = None;
                if text.is_empty() {
                    if !keep_embeds {
                        node.embeds.clear();
                    }
                    continue;
                }

                match output {
                    OutputType::AST => {
                        let mut body = to_structure_root(&text, &mut node.embeds);
                        if let Some(limit) = character_limit {
                            truncate_structure_root(&mut body, limit as usize);
                        }
                        node.ast = Some(body);
                    }
                    OutputType::HTML => {
                        node.html = Some(render_gfm_html_scoped(
                            &text,
                            &node.embeds,
                            max_image_variant_width,
                            Some(&footnote_scope),
                            character_limit.map(|limit| limit as usize),
                        )?);
                    }
                    OutputType::Markdown => {}
                }

                if !keep_embeds {
                    node.embeds.clear();
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::db::{
        fields::OutputType,
        serialize::{
            ContentEntrySerializer, ContentNodeSerializer, MediaSerializer, MediaVariantSerializer,
            NodeSerializer,
        },
    };

    use super::render_entry_nodes;

    #[test]
    fn renders_html_with_internal_media_variants_without_returning_embeds() {
        let mut entries = vec![ContentEntrySerializer {
            nodes: vec![NodeSerializer::Single(Box::new(ContentNodeSerializer {
                text: Some("![Cover](/uploads/cover.jpg)".into()),
                embeds: vec![MediaSerializer {
                    path: Some("/uploads".into()),
                    filename: Some("cover.jpg".into()),
                    variants: vec![MediaVariantSerializer {
                        id: 1,
                        width: 640,
                        height: 360,
                        filename: "cover-640.webp".into(),
                    }],
                    ..MediaSerializer::default()
                }],
                ..ContentNodeSerializer::default()
            }))],
            ..ContentEntrySerializer::default()
        }];

        render_entry_nodes(&mut entries, &OutputType::HTML, None, false, Some(640))
            .expect("HTML rendering succeeds");

        let NodeSerializer::Single(node) = &entries[0].nodes[0] else {
            panic!("single node expected");
        };
        assert!(
            node.html
                .as_deref()
                .is_some_and(|html| html.contains("<picture>"))
        );
        assert!(node.embeds.is_empty());
    }

    #[test]
    fn removes_internally_loaded_embeds_from_empty_html_nodes() {
        let mut entries = vec![ContentEntrySerializer {
            nodes: vec![NodeSerializer::Single(Box::new(ContentNodeSerializer {
                text: Some(String::new()),
                embeds: vec![MediaSerializer {
                    id: Some(1),
                    ..MediaSerializer::default()
                }],
                ..ContentNodeSerializer::default()
            }))],
            ..ContentEntrySerializer::default()
        }];

        render_entry_nodes(&mut entries, &OutputType::HTML, None, false, Some(640))
            .expect("HTML rendering succeeds");

        let NodeSerializer::Single(node) = &entries[0].nodes[0] else {
            panic!("single node expected");
        };
        assert!(node.embeds.is_empty());
    }

    #[test]
    fn namespaces_footnote_ids_for_separately_rendered_nodes() {
        let mut entries = vec![ContentEntrySerializer {
            nodes: vec![
                NodeSerializer::Single(Box::new(ContentNodeSerializer {
                    id: Some(41),
                    text: Some("First^[Note]".into()),
                    ..ContentNodeSerializer::default()
                })),
                NodeSerializer::Single(Box::new(ContentNodeSerializer {
                    id: Some(42),
                    text: Some("Second^[Note]".into()),
                    ..ContentNodeSerializer::default()
                })),
            ],
            ..ContentEntrySerializer::default()
        }];

        render_entry_nodes(&mut entries, &OutputType::HTML, None, false, None)
            .expect("HTML rendering succeeds");

        let NodeSerializer::Single(first) = &entries[0].nodes[0] else {
            panic!("single node expected");
        };
        let NodeSerializer::Single(second) = &entries[0].nodes[1] else {
            panic!("single node expected");
        };
        assert!(
            first
                .html
                .as_deref()
                .is_some_and(|html| html.contains("id=\"fn-node-41-__inline_1\""))
        );
        assert!(
            second
                .html
                .as_deref()
                .is_some_and(|html| html.contains("id=\"fn-node-42-__inline_1\""))
        );
    }

    #[test]
    fn applies_character_limit_to_html_output() {
        let mut entries = vec![ContentEntrySerializer {
            nodes: vec![NodeSerializer::Single(Box::new(ContentNodeSerializer {
                text: Some("A short **formatted passage** followed by hidden text.".into()),
                ..ContentNodeSerializer::default()
            }))],
            ..ContentEntrySerializer::default()
        }];

        render_entry_nodes(&mut entries, &OutputType::HTML, Some(24), false, None)
            .expect("HTML rendering succeeds");

        let NodeSerializer::Single(node) = &entries[0].nodes[0] else {
            panic!("single node expected");
        };
        let html = node.html.as_deref().expect("HTML output exists");
        assert!(html.contains("<strong>formatted …</strong>"));
        assert!(!html.contains("hidden text"));
    }
}
