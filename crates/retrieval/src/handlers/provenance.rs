use std::collections::{BTreeSet, HashMap, HashSet};

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{types::Json as SqlJson, PgPool};
use uuid::Uuid;

use super::{resolve_workspace_id, workspace_id_param};

const MAX_LINEAGE_MEMORIES: usize = 200;
const MAX_LINEAGE_DEPTH: usize = 8;
const PREVIEW_CHARS: usize = 140;

#[derive(Debug, Serialize)]
pub struct ProvenanceGraph {
    pub root_id: String,
    pub nodes: Vec<ProvenanceNode>,
    pub edges: Vec<ProvenanceEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvenanceNode {
    pub id: String,
    pub node_type: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProvenanceEdge {
    pub from: String,
    pub to: String,
    pub edge_type: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ProvenanceMemoryRow {
    id: Uuid,
    workspace_id: Uuid,
    memory_type: String,
    content: String,
    source_events: Vec<Uuid>,
    source_episode_ids: Vec<Uuid>,
    corroboration_count: i32,
    promoted_at: Option<DateTime<Utc>>,
    access_count: i32,
    last_accessed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ProvenanceRawEventRow {
    id: Uuid,
    source: String,
    event_type: String,
    actor: String,
    occurred_at: DateTime<Utc>,
    ingested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MergeAuditRow {
    id: Uuid,
    actor: String,
    target_id: Uuid,
    diff: Option<SqlJson<Value>>,
    occurred_at: DateTime<Utc>,
}

impl MergeAuditRow {
    fn source_id(&self) -> Option<Uuid> {
        let value = self.diff.as_ref()?.0.pointer("/source/id")?.as_str()?;
        Uuid::parse_str(value).ok()
    }
}

#[axum::debug_handler]
pub async fn handle_provenance(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<ProvenanceGraph>> {
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, workspace_id_param(&params)?)?;
    let (memories, events, merges) = collect_lineage(&state.db, workspace_id, id).await?;

    Ok(Json(build_graph(id, &memories, &events, &merges)))
}

async fn collect_lineage(
    db: &PgPool,
    workspace_id: Uuid,
    root_id: Uuid,
) -> AppResult<(
    HashMap<Uuid, ProvenanceMemoryRow>,
    HashMap<Uuid, ProvenanceRawEventRow>,
    Vec<MergeAuditRow>,
)> {
    let mut memory_ids = BTreeSet::from([root_id]);
    let mut memories = HashMap::new();
    let mut merges = HashMap::<Uuid, MergeAuditRow>::new();

    for _ in 0..MAX_LINEAGE_DEPTH {
        let missing_ids = memory_ids
            .iter()
            .copied()
            .filter(|id| !memories.contains_key(id))
            .collect::<Vec<_>>();
        if !missing_ids.is_empty() {
            for row in load_memory_rows(db, workspace_id, &missing_ids).await? {
                memories.insert(row.id, row);
            }
        }

        let mut changed = false;
        for memory in memories.values() {
            for source_id in &memory.source_episode_ids {
                if memory_ids.len() < MAX_LINEAGE_MEMORIES && memory_ids.insert(*source_id) {
                    changed = true;
                }
            }
        }

        let ids = memory_ids.iter().copied().collect::<Vec<_>>();
        for merge in load_merge_rows(db, workspace_id, &ids).await? {
            if memory_ids.len() < MAX_LINEAGE_MEMORIES && memory_ids.insert(merge.target_id) {
                changed = true;
            }
            if let Some(source_id) = merge.source_id() {
                if memory_ids.len() < MAX_LINEAGE_MEMORIES && memory_ids.insert(source_id) {
                    changed = true;
                }
            }
            merges.entry(merge.id).or_insert(merge);
        }

        if !changed {
            break;
        }
    }

    if !memories.contains_key(&root_id) {
        return Err(AppError::NotFound {
            resource: format!("memory:{root_id}"),
        });
    }

    let event_ids = memories
        .values()
        .flat_map(|memory| memory.source_events.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let events = load_raw_event_rows(db, workspace_id, &event_ids)
        .await?
        .into_iter()
        .map(|event| (event.id, event))
        .collect::<HashMap<_, _>>();

    Ok((memories, events, merges.into_values().collect()))
}

async fn load_memory_rows(
    db: &PgPool,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> AppResult<Vec<ProvenanceMemoryRow>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    sqlx::query_as::<_, ProvenanceMemoryRow>(
        r#"
        SELECT id,
               workspace_id,
               memory_type::TEXT AS memory_type,
               content,
               source_events,
               source_episode_ids,
               corroboration_count,
               promoted_at,
               access_count,
               last_accessed_at,
               created_at,
               updated_at,
               deleted_at
        FROM memory_units
        WHERE workspace_id = $1 AND id = ANY($2)
        "#,
    )
    .bind(workspace_id)
    .bind(ids.to_vec())
    .fetch_all(db)
    .await
    .map_err(AppError::Database)
}

async fn load_raw_event_rows(
    db: &PgPool,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> AppResult<Vec<ProvenanceRawEventRow>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    sqlx::query_as::<_, ProvenanceRawEventRow>(
        r#"
        SELECT id,
               source::TEXT AS source,
               event_type::TEXT AS event_type,
               actor,
               occurred_at,
               ingested_at
        FROM raw_events
        WHERE workspace_id = $1 AND id = ANY($2)
        "#,
    )
    .bind(workspace_id)
    .bind(ids.to_vec())
    .fetch_all(db)
    .await
    .map_err(AppError::Database)
}

async fn load_merge_rows(
    db: &PgPool,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> AppResult<Vec<MergeAuditRow>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let id_strings = ids.iter().map(Uuid::to_string).collect::<Vec<_>>();
    sqlx::query_as::<_, MergeAuditRow>(
        r#"
        SELECT id, actor, target_id, diff, occurred_at
        FROM audit_log
        WHERE workspace_id = $1
          AND action::TEXT = 'memory_merged'
          AND target_type = 'memory'
          AND (target_id = ANY($2) OR diff #>> '{source,id}' = ANY($3))
        ORDER BY occurred_at ASC, id ASC
        "#,
    )
    .bind(workspace_id)
    .bind(ids.to_vec())
    .bind(id_strings)
    .fetch_all(db)
    .await
    .map_err(AppError::Database)
}

fn build_graph(
    root_id: Uuid,
    memories: &HashMap<Uuid, ProvenanceMemoryRow>,
    events: &HashMap<Uuid, ProvenanceRawEventRow>,
    merges: &[MergeAuditRow],
) -> ProvenanceGraph {
    let mut builder = GraphBuilder::default();

    for memory in sorted_memories(memories) {
        builder.push_node(memory_node(memory));
        for event_id in &memory.source_events {
            if let Some(event) = events.get(event_id) {
                builder.push_node(raw_event_node(event));
            } else {
                builder.push_node(missing_event_node(*event_id));
            }
            builder.push_edge(
                event_node_id(*event_id),
                memory_node_id(memory.id),
                "created_from",
            );
        }
        for source_id in &memory.source_episode_ids {
            if !memories.contains_key(source_id) {
                builder.push_node(missing_memory_node(*source_id));
            }
            builder.push_edge(
                memory_node_id(*source_id),
                memory_node_id(memory.id),
                "promoted_to",
            );
        }
        if memory.access_count > 0 || memory.last_accessed_at.is_some() {
            builder.push_node(access_node(memory));
            builder.push_edge(
                memory_node_id(memory.id),
                access_node_id(memory.id),
                "accessed_as",
            );
        }
    }

    for merge in merges {
        let Some(source_id) = merge.source_id() else {
            continue;
        };
        if !memories.contains_key(&source_id) {
            builder.push_node(missing_memory_node(source_id));
        }
        if !memories.contains_key(&merge.target_id) {
            builder.push_node(missing_memory_node(merge.target_id));
        }
        builder.push_node(merge_node(merge, source_id));
        builder.push_edge(
            memory_node_id(source_id),
            merge_node_id(merge.id),
            "merged_into",
        );
        builder.push_edge(
            merge_node_id(merge.id),
            memory_node_id(merge.target_id),
            "merged_into",
        );
    }

    ProvenanceGraph {
        root_id: memory_node_id(root_id),
        nodes: builder.nodes,
        edges: builder.edges,
    }
}

#[derive(Default)]
struct GraphBuilder {
    node_ids: HashSet<String>,
    edge_ids: HashSet<(String, String, String)>,
    nodes: Vec<ProvenanceNode>,
    edges: Vec<ProvenanceEdge>,
}

impl GraphBuilder {
    fn push_node(&mut self, node: ProvenanceNode) {
        if self.node_ids.insert(node.id.clone()) {
            self.nodes.push(node);
        }
    }

    fn push_edge(&mut self, from: String, to: String, edge_type: &'static str) {
        let key = (from.clone(), to.clone(), edge_type.to_owned());
        if self.edge_ids.insert(key) {
            self.edges.push(ProvenanceEdge {
                from,
                to,
                edge_type: edge_type.to_owned(),
            });
        }
    }
}

fn sorted_memories(memories: &HashMap<Uuid, ProvenanceMemoryRow>) -> Vec<&ProvenanceMemoryRow> {
    let mut rows = memories.values().collect::<Vec<_>>();
    rows.sort_by_key(|memory| (memory.created_at, memory.id));
    rows
}

fn memory_node(memory: &ProvenanceMemoryRow) -> ProvenanceNode {
    ProvenanceNode {
        id: memory_node_id(memory.id),
        node_type: "memory".to_owned(),
        title: format!("{} memory", title_case(&memory.memory_type)),
        subtitle: Some(preview_text(&memory.content)),
        timestamp: Some(memory.created_at),
        metadata: json!({
            "memory_id": memory.id,
            "workspace_id": memory.workspace_id,
            "memory_type": memory.memory_type,
            "corroboration_count": memory.corroboration_count,
            "promoted_at": memory.promoted_at,
            "updated_at": memory.updated_at,
            "deleted": memory.deleted_at.is_some(),
        }),
    }
}

fn raw_event_node(event: &ProvenanceRawEventRow) -> ProvenanceNode {
    ProvenanceNode {
        id: event_node_id(event.id),
        node_type: "raw_event".to_owned(),
        title: format!(
            "{} {}",
            title_case(&event.source),
            event.event_type.replace('_', " ")
        ),
        subtitle: Some(format!("Actor: {}", event.actor)),
        timestamp: Some(event.occurred_at),
        metadata: json!({
            "event_id": event.id,
            "source": event.source,
            "event_type": event.event_type,
            "actor": event.actor,
            "ingested_at": event.ingested_at,
        }),
    }
}

fn merge_node(merge: &MergeAuditRow, source_id: Uuid) -> ProvenanceNode {
    ProvenanceNode {
        id: merge_node_id(merge.id),
        node_type: "merge".to_owned(),
        title: "Merge".to_owned(),
        subtitle: Some(format!("{} merged into {}", source_id, merge.target_id)),
        timestamp: Some(merge.occurred_at),
        metadata: json!({
            "audit_id": merge.id,
            "actor": merge.actor,
            "source_id": source_id,
            "target_id": merge.target_id,
        }),
    }
}

fn access_node(memory: &ProvenanceMemoryRow) -> ProvenanceNode {
    ProvenanceNode {
        id: access_node_id(memory.id),
        node_type: "access".to_owned(),
        title: "Access".to_owned(),
        subtitle: Some(format!("{} reads", memory.access_count.max(0))),
        timestamp: memory.last_accessed_at,
        metadata: json!({
            "memory_id": memory.id,
            "access_count": memory.access_count.max(0),
            "last_accessed_at": memory.last_accessed_at,
        }),
    }
}

fn missing_event_node(id: Uuid) -> ProvenanceNode {
    ProvenanceNode {
        id: event_node_id(id),
        node_type: "raw_event".to_owned(),
        title: "Source event".to_owned(),
        subtitle: Some("Event record unavailable".to_owned()),
        timestamp: None,
        metadata: json!({ "event_id": id, "missing": true }),
    }
}

fn missing_memory_node(id: Uuid) -> ProvenanceNode {
    ProvenanceNode {
        id: memory_node_id(id),
        node_type: "memory".to_owned(),
        title: "Memory".to_owned(),
        subtitle: Some("Memory record unavailable".to_owned()),
        timestamp: None,
        metadata: json!({ "memory_id": id, "missing": true }),
    }
}

fn memory_node_id(id: Uuid) -> String {
    format!("memory:{id}")
}

fn event_node_id(id: Uuid) -> String {
    format!("event:{id}")
}

fn merge_node_id(id: Uuid) -> String {
    format!("merge:{id}")
}

fn access_node_id(id: Uuid) -> String {
    format!("access:{id}")
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

fn preview_text(value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= PREVIEW_CHARS {
        return normalized;
    }

    normalized
        .chars()
        .take(PREVIEW_CHARS.saturating_sub(3))
        .collect::<String>()
        + "..."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_graph_links_events_episodes_semantic_and_access() {
        let workspace_id = Uuid::now_v7();
        let event_id = Uuid::now_v7();
        let episode_id = Uuid::now_v7();
        let semantic_id = Uuid::now_v7();
        let now = Utc::now();
        let memories = HashMap::from([
            (
                episode_id,
                memory_row(
                    workspace_id,
                    episode_id,
                    "episodic",
                    vec![event_id],
                    Vec::new(),
                    0,
                ),
            ),
            (
                semantic_id,
                memory_row(
                    workspace_id,
                    semantic_id,
                    "semantic",
                    Vec::new(),
                    vec![episode_id],
                    3,
                ),
            ),
        ]);
        let events = HashMap::from([(
            event_id,
            ProvenanceRawEventRow {
                id: event_id,
                source: "github".to_owned(),
                event_type: "push".to_owned(),
                actor: "octo".to_owned(),
                occurred_at: now,
                ingested_at: now,
            },
        )]);

        let graph = build_graph(semantic_id, &memories, &events, &[]);

        assert_eq!(graph.root_id, memory_node_id(semantic_id));
        assert!(graph.edges.contains(&ProvenanceEdge {
            from: event_node_id(event_id),
            to: memory_node_id(episode_id),
            edge_type: "created_from".to_owned(),
        }));
        assert!(graph.edges.contains(&ProvenanceEdge {
            from: memory_node_id(episode_id),
            to: memory_node_id(semantic_id),
            edge_type: "promoted_to".to_owned(),
        }));
        assert!(graph.edges.contains(&ProvenanceEdge {
            from: memory_node_id(semantic_id),
            to: access_node_id(semantic_id),
            edge_type: "accessed_as".to_owned(),
        }));
    }

    fn memory_row(
        workspace_id: Uuid,
        id: Uuid,
        memory_type: &str,
        source_events: Vec<Uuid>,
        source_episode_ids: Vec<Uuid>,
        access_count: i32,
    ) -> ProvenanceMemoryRow {
        let now = Utc::now();
        ProvenanceMemoryRow {
            id,
            workspace_id,
            memory_type: memory_type.to_owned(),
            content: "Memory content with enough detail for a preview".to_owned(),
            source_events,
            source_episode_ids,
            corroboration_count: 1,
            promoted_at: None,
            access_count,
            last_accessed_at: (access_count > 0).then_some(now),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}
