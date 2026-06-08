import {
  FeedbackResponse,
  MemoryUnit,
  MemoryVersion,
  ProvenanceGraph,
  RetrievalResult,
  Skill,
  SkillTestResult,
  SkillVersion,
} from "./client";

export function formatMemoryMarkdown(memory: MemoryUnit): string {
  return [
    `# MemoryOps Memory${memory.id ? ` \`${memory.id}\`` : ""}`,
    memory.score !== undefined ? `Score: ${formatNumber(memory.score)}` : undefined,
    memory.memory_type ? `Type: ${memory.memory_type}` : undefined,
    memory.scope_visibility ? `Visibility: ${memory.scope_visibility}` : undefined,
    memory.version !== undefined ? `Version: ${memory.version}` : undefined,
    memory.pinned !== undefined ? `Pinned: ${memory.pinned ? "yes" : "no"}` : undefined,
    memory.importance_score !== undefined ? `Importance: ${formatNumber(memory.importance_score)}` : undefined,
    memory.decay_score !== undefined ? `Decay: ${formatNumber(memory.decay_score)}` : undefined,
    memory.relevance_score !== undefined ? `Relevance: ${formatNumber(memory.relevance_score)}` : undefined,
    Array.isArray(memory.tags) && memory.tags.length > 0 ? `Tags: ${memory.tags.join(", ")}` : undefined,
    memory.created_at ? `Created: ${memory.created_at}` : undefined,
    memory.updated_at ? `Updated: ${memory.updated_at}` : undefined,
    memory.deleted_at ? `Deleted: ${memory.deleted_at}` : undefined,
    "",
    memory.content ?? formatJsonBlock(memory),
  ]
    .filter(Boolean)
    .join("\n");
}

export function formatRetrievalMarkdown(result: RetrievalResult, title: string): string {
  const memories = Array.isArray(result.memories) ? result.memories : [];
  return [
    `# ${title}`,
    result.query_id ? `Query ID: \`${result.query_id}\`` : undefined,
    typeof result.total_tokens === "number" ? `Total tokens: ${result.total_tokens}` : undefined,
    "",
    result.packed_context ?? result.context ?? "No context returned.",
    ...(memories.length > 0
      ? [
          "",
          "## Memories",
          ...memories.map((memory, index) => [
            `### ${index + 1}. ${memory.id ?? "Memory"}`,
            memory.score !== undefined ? `Score: ${formatNumber(memory.score)}` : undefined,
            memory.importance_score !== undefined ? `Importance: ${formatNumber(memory.importance_score)}` : undefined,
            memory.memory_type ? `Type: ${memory.memory_type}` : undefined,
            "",
            memory.content ?? formatJsonBlock(memory),
          ].filter(Boolean).join("\n")),
        ]
      : []),
  ]
    .filter((part) => part !== undefined && part !== "")
    .join("\n");
}

export function formatMemoryHistoryMarkdown(memory: MemoryUnit, versions: MemoryVersion[]): string {
  return [
    `# MemoryOps Memory History${memory.id ? ` \`${memory.id}\`` : ""}`,
    `Versions: ${versions.length}`,
    "",
    ...(versions.length > 0
      ? versions.map((version, index) => [
          `## ${index + 1}. Version ${version.version ?? index + 1}`,
          version.edited_by ? `Edited by: ${version.edited_by}` : undefined,
          version.created_at ? `Created: ${version.created_at}` : undefined,
          version.importance_score !== undefined ? `Importance: ${formatNumber(version.importance_score)}` : undefined,
          Array.isArray(version.tags) && version.tags.length > 0 ? `Tags: ${version.tags.join(", ")}` : undefined,
          "",
          version.content ?? formatJsonBlock(version),
        ].filter(Boolean).join("\n"))
      : ["No prior versions were returned for this memory."]),
  ].join("\n");
}

export function formatMemoryProvenanceMarkdown(memory: MemoryUnit, graph: ProvenanceGraph): string {
  return [
    `# MemoryOps Provenance${memory.id ? ` \`${memory.id}\`` : ""}`,
    graph.root_id ? `Root ID: \`${graph.root_id}\`` : undefined,
    `Nodes: ${graph.nodes.length}`,
    `Edges: ${graph.edges.length}`,
    "",
    ...(graph.nodes.length > 0
      ? [
          "## Nodes",
          ...graph.nodes.map((node, index) => [
            `### ${index + 1}. ${node.title ?? node.id ?? "Node"}`,
            node.id ? `ID: \`${node.id}\`` : undefined,
            node.node_type ? `Type: ${node.node_type}` : undefined,
            node.subtitle ? `Subtitle: ${node.subtitle}` : undefined,
            node.timestamp ? `Timestamp: ${node.timestamp}` : undefined,
            node.metadata && Object.keys(node.metadata).length > 0 ? `Metadata:\n${formatJsonBlock(node.metadata)}` : undefined,
          ].filter(Boolean).join("\n")),
        ]
      : ["No provenance nodes were returned for this memory."]),
    "",
    ...(graph.edges.length > 0
      ? [
          "## Edges",
          ...graph.edges.map((edge) => `- \`${edge.from ?? "unknown"}\` --${edge.edge_type ?? "related_to"}--> \`${edge.to ?? "unknown"}\``),
        ]
      : []),
  ]
    .filter(Boolean)
    .join("\n");
}

export function formatMemoryFeedbackMarkdown(memory: MemoryUnit, feedback: FeedbackResponse): string {
  return [
    `# MemoryOps Feedback${memory.id ? ` \`${memory.id}\`` : ""}`,
    `Entries: ${feedback.total}`,
    feedback.avg_rating !== undefined ? `Average rating: ${formatNumber(feedback.avg_rating)}` : undefined,
    feedback.relevance_score !== undefined ? `Relevance score: ${formatNumber(feedback.relevance_score)}` : undefined,
    "",
    ...(feedback.items.length > 0
      ? [
          "## Entries",
          ...feedback.items.map((entry, index) => [
            `### ${index + 1}. ${formatRating(entry.rating)}`,
            entry.occurred_at ? `Occurred: ${entry.occurred_at}` : undefined,
            entry.query_id ? `Query ID: \`${entry.query_id}\`` : undefined,
            entry.agent_id ? `Agent: ${entry.agent_id}` : undefined,
            entry.user_id ? `User: ${entry.user_id}` : undefined,
            entry.comment ? `Comment: ${entry.comment}` : undefined,
          ].filter(Boolean).join("\n")),
        ]
      : ["No retrieval feedback has been recorded for this memory."]),
  ]
    .filter(Boolean)
    .join("\n");
}

export function formatSkillMarkdown(skill: Skill): string {
  return [
    `# MemoryOps Skill \`${skill.name}\``,
    `Version: ${skill.version}`,
    `Enabled: ${skill.enabled ? "yes" : "no"}`,
    `Method: ${skill.http_method}`,
    `URL: ${skill.endpoint_url}`,
    skill.auth_header ? `Auth header: ${skill.auth_header}` : undefined,
    skill.created_at ? `Created: ${skill.created_at}` : undefined,
    skill.updated_at ? `Updated: ${skill.updated_at}` : undefined,
    "",
    "## Description",
    skill.description || "_(no description)_",
    "",
    "## Input schema",
    formatJsonBlock(skill.input_schema ?? {}),
    "",
    "## Output schema",
    formatJsonBlock(skill.output_schema ?? {}),
  ]
    .filter((part) => part !== undefined)
    .join("\n");
}

export function formatSkillVersionsMarkdown(skill: Skill, versions: SkillVersion[]): string {
  return [
    `# Skill version history \`${skill.name}\``,
    `Current version: ${skill.version}`,
    `Total versions: ${versions.length}`,
    "",
    ...(versions.length > 0
      ? versions.map((v) => [
          `## v${v.version}${v.version === skill.version ? " (current)" : ""}`,
          v.created_at ? `Created: ${v.created_at}` : undefined,
          v.created_by ? `By: ${v.created_by}` : undefined,
          `Method: ${v.http_method}`,
          `URL: ${v.endpoint_url}`,
          `Enabled: ${v.enabled ? "yes" : "no"}`,
          v.change_note ? `Change note: ${v.change_note}` : undefined,
          v.description ? `Description: ${v.description}` : undefined,
        ].filter(Boolean).join("\n"))
      : ["No version history recorded yet."]),
  ].join("\n");
}

export function formatSkillTestMarkdown(skill: Skill, result: SkillTestResult): string {
  return [
    `# Skill test \`${skill.name}\``,
    `Status: ${result.status}`,
    `Latency: ${result.latency_ms} ms`,
    "",
    "## Response body",
    formatJsonBlock(result.body),
  ].join("\n");
}

export function firstLine(value: string): string {
  const lines = value.split(/\r?\n/);
  const firstNonEmpty = lines.find(line => line.trim().length > 0);
  return firstNonEmpty !== undefined ? firstNonEmpty : (lines[0] ?? value);
}

export function truncate(value: string, maxLength: number): string {
  return value.length <= maxLength ? value : `${value.slice(0, maxLength - 3)}...`;
}

export function scoreLabel(score: unknown): string | undefined {
  return typeof score === "number" ? `score ${score.toFixed(3)}` : undefined;
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function formatJsonBlock(value: unknown): string {
  return `\`\`\`json\n${JSON.stringify(value, null, 2)}\n\`\`\``;
}

function formatNumber(value: number): string {
  return value.toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
}

function formatRating(value: number): string {
  if (value > 0) {
    return `Helpful (${value})`;
  }
  if (value < 0) {
    return `Not helpful (${value})`;
  }
  return `Neutral (${value})`;
}