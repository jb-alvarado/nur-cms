use crate::{
    db::{
        fields::OutputType,
        serialize::{ContentEntrySerializer, ContentNodeSerializer, NodeSerializer},
    },
    utils::{
        ast_serialize::{to_structure_root, truncate_structure_root},
        errors::NurError,
        markdown::render_gfm_html,
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

    for entry in entries {
        for node_wrapper in &mut entry.nodes {
            let nodes: Vec<&mut ContentNodeSerializer> = match node_wrapper {
                NodeSerializer::Single(node) => vec![node.as_mut()],
                NodeSerializer::Blocks(nodes) => nodes.iter_mut().collect(),
            };

            for node in nodes {
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
                        node.html = Some(render_gfm_html(
                            &text,
                            &node.embeds,
                            max_image_variant_width,
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
}
