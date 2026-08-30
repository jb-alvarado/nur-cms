use markdown::{ParseOptions, to_mdast};

use crate::{
    db::{
        fields::OutputType,
        serialize::{ContentEntrySerializer, ContentNodeSerializer, NodeSerializer},
    },
    utils::{
        ast_serialize::{to_structure_root_mdast, truncate_structure_root},
        errors::NurError,
        markdown::render_gfm_html,
    },
};

/// Converts selected content-node Markdown to the requested public output format.
pub fn render_entry_nodes(
    entries: &mut [ContentEntrySerializer],
    output: &OutputType,
    character_limit: Option<i32>,
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
                    continue;
                }

                match output {
                    OutputType::AST => {
                        let ast = to_mdast(&text, &ParseOptions::gfm())?;
                        let mut body = to_structure_root_mdast(&ast, &mut node.embeds);
                        if let Some(limit) = character_limit {
                            truncate_structure_root(&mut body, limit as usize);
                        }
                        node.ast = Some(body);
                    }
                    OutputType::HTML => node.html = Some(render_gfm_html(&text)?),
                    OutputType::Markdown => {}
                }
            }
        }
    }

    Ok(())
}
