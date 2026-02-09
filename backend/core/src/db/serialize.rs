use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, Row, postgres::PgRow};
use ts_rs::TS;

use crate::db::{
    fields::ColumnCounter,
    models::{AuthRole, Role},
};

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct AuthUserSerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(default, skip_serializing)]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<AuthRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_login: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for AuthUserSerializer {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let mut role = None;

        if let Ok((id, name)) = row.try_get::<(i32, String), &str>("auth_role") {
            role = Some(AuthRole {
                id,
                name: Role::set_role(&name),
                total_count: None,
            });
        };

        Ok(Self {
            id: row.try_get("id").ok(),
            email: row.try_get("email").ok(),
            username: row.try_get("username").ok(),
            first_name: row.try_get("first_name").ok(),
            last_name: row.try_get("last_name").ok(),
            password: row.try_get("password").ok(),
            role_id: row.try_get("role_id").ok(),
            role,
            created_at: None,
            updated_at: None,
            last_login: row.try_get("last_login").ok(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for AuthUserSerializer {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct AuthorSerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaSerializer>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for AuthorSerializer {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let media = row
            .try_get::<Option<serde_json::Value>, _>("media")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<MediaSerializer>(v).unwrap_or_default());

        Ok(Self {
            id: row.try_get("id").ok(),
            first_name: row.try_get("first_name").ok(),
            last_name: row.try_get("last_name").ok(),
            slug: row.try_get("slug").ok(),
            bio: row.try_get("bio").ok(),
            media_id: row.try_get("media_id").ok(),
            media,
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for AuthorSerializer {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "serialized.d.ts")]
pub enum NodeSerializer {
    Blocks(Vec<ContentNodeSerializer>),
    #[serde(untagged)]
    Single(Box<ContentNodeSerializer>),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct ContentNodeSerializer {
    #[ts(as = "Option<i32>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_index: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[ts(type = "any")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ast: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<i32>,
    #[serde(default, skip_serializing)]
    pub parent_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embeds: Vec<MediaSerializer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaSerializer>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct ContentEntrySerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[ts(as = "Option<i32>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>, // draft, published, archived
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<AuthorSerializer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<ContentMetaSerializer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<ContentCategorySerializer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<ContentTagSerializer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodeSerializer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaSerializer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_members: Vec<GroupMemberSerializer>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentEntrySerializer {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let mut tags = vec![];

        let authors = row
            .try_get::<Option<serde_json::Value>, _>("authors")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<Vec<AuthorSerializer>>(v).unwrap_or_default())
            .unwrap_or_default();

        let meta = row
            .try_get::<(Option<DateTime<Utc>>, Option<DateTime<Utc>>), &str>("meta")
            .ok()
            .filter(|(start_time, end_time)| start_time.is_some() || end_time.is_some())
            .map(|(start_time, end_time)| ContentMetaSerializer {
                start_time,
                end_time,
            });

        let category = row
            .try_get::<Option<serde_json::Value>, _>("category")
            .unwrap_or_default()
            .and_then(|v| serde_json::from_value::<ContentCategorySerializer>(v).ok());

        for (id, name, slug) in row
            .try_get::<Vec<Option<(i32, String, String)>>, &str>("tags")
            .unwrap_or_default()
            .into_iter()
            .flatten()
        {
            tags.push(ContentTagSerializer {
                id: Some(id),
                name: Some(name),
                slug: Some(slug),
                total_count: None,
            });
        }

        let nodes = row
            .try_get::<Option<serde_json::Value>, _>("nodes")
            .unwrap_or_default()
            .map(|v| {
                let nodes_vec =
                    serde_json::from_value::<Vec<ContentNodeSerializer>>(v).unwrap_or_default();

                // Build a map of node_id -> children
                let mut children_map: HashMap<i64, Vec<ContentNodeSerializer>> = HashMap::new();
                let mut parent_nodes: HashMap<i64, ContentNodeSerializer> = HashMap::new();
                let mut single_nodes: Vec<ContentNodeSerializer> = Vec::new();

                for node in nodes_vec {
                    if let Some(parent_id) = node.parent_id {
                        // This node has a parent - add it to children_map
                        children_map.entry(parent_id as i64).or_default().push(node);
                    } else if let Some(node_id) = node.id {
                        // This node has no parent - it might be a parent itself or a single
                        parent_nodes.insert(node_id, node);
                    } else {
                        // Node without id and without parent - treat as single
                        single_nodes.push(node);
                    }
                }

                // Convert to NodeSerializer
                let mut result = Vec::new();

                // Process parent nodes
                for (node_id, parent_node) in parent_nodes {
                    if let Some(mut children) = children_map.remove(&node_id) {
                        // Sort children by order_index
                        children.sort_by_key(|n| n.order_index.unwrap_or(0));

                        // This node has children - create a Block with parent + children
                        let mut block = vec![parent_node];
                        block.extend(children);
                        result.push(NodeSerializer::Blocks(block));
                    } else {
                        // This node has no children - it's a Single
                        result.push(NodeSerializer::Single(Box::new(parent_node)));
                    }
                }

                // Add any single nodes without id
                for node in single_nodes {
                    result.push(NodeSerializer::Single(Box::new(node)));
                }

                // Handle orphaned children (nodes with parent_id but no parent found)
                if !children_map.is_empty() {
                    for (_, children) in children_map {
                        for child in children {
                            result.push(NodeSerializer::Single(Box::new(child)));
                        }
                    }
                }

                // Sort result by order_index of the first node
                result.sort_by_key(|n| match n {
                    NodeSerializer::Single(node) => node.order_index.unwrap_or(0),
                    NodeSerializer::Blocks(nodes) => {
                        nodes.first().and_then(|n| n.order_index).unwrap_or(0)
                    }
                });

                result
            })
            .unwrap_or_default();

        let group_members = row
            .try_get::<Option<serde_json::Value>, _>("group_members")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<Vec<GroupMemberSerializer>>(v).unwrap_or_default())
            .unwrap_or_default();

        let media = row
            .try_get::<Option<serde_json::Value>, _>("media")
            .unwrap_or_default()
            .and_then(|v| serde_json::from_value::<MediaSerializer>(v).ok());

        // let embeds = row
        //     .try_get::<Option<serde_json::Value>, _>("embeds")
        //     .unwrap_or_default()
        //     .map(|v| serde_json::from_value::<Vec<MediaSerializer>>(v).unwrap_or_default())
        //     .unwrap_or_default();

        Ok(Self {
            id: row.try_get("id").ok(),
            group_id: row.try_get("group_id").ok(),
            category_id: row.try_get("category_id").ok(),
            locale_id: row.try_get("locale_id").ok(),
            media_id: row.try_get("media_id").ok(),
            slug: row.try_get("slug").ok(),
            status: row.try_get("status").ok(),
            authors,
            meta,
            category,
            tags,
            title: row.try_get("title").ok(),
            nodes,
            media,
            created_at: row.try_get("created_at").ok(),
            updated_at: row.try_get("updated_at").ok(),
            group_members,
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentEntrySerializer {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct ContentCategorySerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[ts(as = "Option<i32>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaSerializer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_members: Vec<GroupMemberSerializer>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentCategorySerializer {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let media = row
            .try_get::<Option<serde_json::Value>, _>("media")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<MediaSerializer>(v).unwrap_or_default());

        let group_members = row
            .try_get::<Option<serde_json::Value>, _>("group_members")
            .unwrap_or_default()
            .and_then(|v| serde_json::from_value::<Vec<GroupMemberSerializer>>(v).ok())
            .unwrap_or_default();

        Ok(Self {
            id: row.try_get("id").ok(),
            group_id: row.try_get("group_id").ok(),
            locale_id: row.try_get("locale_id").ok(),
            name: row.try_get("name").ok(),
            slug: row.try_get("slug").ok(),
            status: row.try_get("status").ok(),
            media_id: row.try_get("media_id").ok(),
            media,
            group_members,
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentCategorySerializer {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct ContentTagSerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentTagSerializer {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").ok(),
            name: row.try_get("name").ok(),
            slug: row.try_get("slug").ok(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentTagSerializer {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct ContentMetaSerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_time: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct GroupMemberSerializer {
    pub id: i32,
    pub locale_id: i32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct MediaSerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[ts(as = "i32")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ast_line: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<MediaVariantSerializer>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for MediaSerializer {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let variants = row
            .try_get::<Option<serde_json::Value>, _>("variants")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<Vec<MediaVariantSerializer>>(v).unwrap_or_default())
            .unwrap_or_default();

        Ok(Self {
            id: row.try_get("id").ok(),
            alt: row.try_get("alt").ok(),
            filename: row.try_get("filename").ok(),
            path: row.try_get("path").ok(),
            r#type: row.try_get("type").ok(),
            width: row.try_get("width").ok(),
            height: row.try_get("height").ok(),
            size: row.try_get("size").ok(),
            ast_line: row.try_get("ast_line").ok(),
            start_offset: row.try_get("start_offset").ok(),
            end_offset: row.try_get("end_offset").ok(),
            created_at: row.try_get("created_at").ok(),
            variants,
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for MediaSerializer {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct MediaVariantSerializer {
    #[ts(as = "i32")]
    pub id: i64,
    pub width: i32,
    pub height: i32,
    pub filename: String,
}
