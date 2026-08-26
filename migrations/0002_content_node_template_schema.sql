ALTER TABLE content_node_templates
ADD COLUMN IF NOT EXISTS schema JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE content_nodes
ADD COLUMN IF NOT EXISTS template_id INT REFERENCES content_node_templates (id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_content_nodes_template_id ON content_nodes (template_id);

CREATE INDEX IF NOT EXISTS idx_content_nodes_data_gin ON content_nodes USING GIN (data jsonb_path_ops);
