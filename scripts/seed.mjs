#!/usr/bin/env node

import fs from 'node:fs';
import { spawnSync } from 'node:child_process';

const API_BASE_URL = process.env.API_BASE_URL || 'http://localhost:8080';
const WORKSPACE_NAME = process.env.WORKSPACE_NAME || 'dev-workspace';
const HALF_LIFE_DAYS = 30;
const LOCAL_CREDENTIALS_PATH = '.memoryops.local.json';

const localCredentials = loadLocalCredentials();
const API_KEY = process.env.API_KEY || localCredentials.api_key || localCredentials.apiKey;
const WORKSPACE_ID = process.env.WORKSPACE_ID || process.env.MEMORYOPS_WORKSPACE_ID || localCredentials.workspace_id || localCredentials.workspaceId;

let MEMORIES_CREATED = 0;
let TOOLS_CREATED = 0;
let CONTRADICTION_FLAGS_CREATED = 0;
let CONTRADICTION_FLAGS_REOPENED = 0;

if (!API_KEY || !WORKSPACE_ID) {
  console.error('Error: API_KEY and WORKSPACE_ID are required.');
  console.error(`Run scripts/bootstrap.mjs first or set API_KEY and WORKSPACE_ID explicitly.`);
  console.error(`The seed script also reads ${LOCAL_CREDENTIALS_PATH} when present.`);
  process.exit(1);
}

function loadLocalCredentials() {
  if (!fs.existsSync(LOCAL_CREDENTIALS_PATH)) return {};
  try {
    return JSON.parse(fs.readFileSync(LOCAL_CREDENTIALS_PATH, 'utf8'));
  } catch {
    return {};
  }
}

function log(msg) {
  const time = new Date().toISOString().substring(11, 19);
  console.log(`[${time}] ${msg}`);
}

function die(msg) {
  console.error(`Error: ${msg}`);
  process.exit(1);
}

async function apiRequest(method, requestPath, body = null) {
  const url = `${API_BASE_URL}${requestPath}`;
  const options = {
    method,
    headers: {
      'X-API-Key': API_KEY,
      'Content-Type': 'application/json',
    },
  };
  if (body !== null && body !== undefined) {
    options.body = typeof body === 'string' ? body : JSON.stringify(body);
  }

  const response = await fetch(url, options);
  const text = await response.text();
  let json = null;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    // Non-JSON responses are preserved in body.
  }
  return { status: response.status, body: text, json };
}

function extractMemoryId(json) {
  if (!json) return null;
  return json.memory_id || json.id || (json.data && (json.data.memory_id || json.data.id)) || null;
}

function normalizeItems(json) {
  if (Array.isArray(json)) return json;
  if (json?.items && Array.isArray(json.items)) return json.items;
  if (json?.data && Array.isArray(json.data)) return json.data;
  if (json?.memories && Array.isArray(json.memories)) return json.memories;
  return [];
}

async function listMemories() {
  const res = await apiRequest('GET', `/v1/memory?workspace_id=${encodeURIComponent(WORKSPACE_ID)}&limit=500`);
  if (res.status >= 200 && res.status < 300) return res.json;
  return null;
}

function findMemoryIdByContent(content, listJson) {
  const item = normalizeItems(listJson).find((memory) => (memory.content || memory.text || '') === content);
  return item ? (item.memory_id || item.id) : null;
}

async function ensureMemory(memoryEndpoint, existingMemoriesJson, payload, label) {
  const existingId = findMemoryIdByContent(payload.content, existingMemoriesJson);
  if (existingId) {
    log(`${label} already exists (id=${existingId}); skipping.`);
    return { id: existingId, created: false, existingMemoriesJson };
  }

  let res = await apiRequest('POST', memoryEndpoint, payload);
  if (res.status >= 300 && memoryEndpoint === '/v1/ingest/raw') {
    log(`Primary memory endpoint failed with HTTP ${res.status}; falling back to /v1/memory`);
    res = await apiRequest('POST', '/v1/memory', payload);
  }
  if (res.status >= 300) {
    die(`Failed to create ${label}: HTTP ${res.status} - ${res.body}`);
  }

  const memoryId = extractMemoryId(res.json);
  if (!memoryId) die(`${label} created but memory id was not returned: ${res.body}`);
  MEMORIES_CREATED++;
  log(`Created ${label} (id=${memoryId})`);
  return { id: memoryId, created: true, existingMemoriesJson: await listMemories() || existingMemoriesJson };
}

async function detectMemoryEndpoint() {
  const rawRes = await apiRequest('GET', '/v1/ingest/raw');
  return rawRes.status !== 404 ? '/v1/ingest/raw' : '/v1/memory';
}

async function upsertTool(toolName, toolDescription) {
  const listRes = await apiRequest('GET', `/v1/workspaces/${WORKSPACE_ID}/tools`);
  if (listRes.status === 404 || listRes.status === 405) {
    log('Tools endpoint unavailable; skipping tools seeding.');
    return 2;
  }
  if (listRes.status < 200 || listRes.status >= 300) {
    die(`Failed to query tools endpoint: HTTP ${listRes.status} - ${listRes.body}`);
  }

  const existing = normalizeItems(listRes.json).find((item) => item.name === toolName);
  if (existing) {
    log(`Tool ${toolName} already exists; skipping.`);
    return 0;
  }

  const payload = {
    name: toolName,
    description: toolDescription,
    endpoint_url: `https://example.com/tools/${toolName}`,
    http_method: 'POST',
    input_schema: { type: 'object', properties: {} },
    output_schema: { type: 'object' },
    scope_visibility: 'workspace',
    enabled: true,
  };

  const createRes = await apiRequest('POST', `/v1/workspaces/${WORKSPACE_ID}/tools`, payload);
  if (createRes.status < 200 || createRes.status >= 300) {
    die(`Failed to create tool ${toolName}: HTTP ${createRes.status} - ${createRes.body}`);
  }

  TOOLS_CREATED++;
  log(`Created tool: ${toolName}`);
  return 0;
}

async function seedMemories(memoryEndpoint) {
  let existingMemoriesJson = await listMemories() || [];

  log('Step 2/7: Seeding 10 episodic memories');
  const episodicContents = [
    'Deployed memoryops v0.15.0 to production AKS cluster at 14:32 UTC. All health checks passed. Rollout took 4m22s.',
    'Qdrant vector DB cold start latency spiked to 2400ms during embedding of batch job at 09:15 UTC. Root cause: container memory limit hit.',
    "User user-001 queried 'recent deployment failures' - hybrid search returned 3 results, top score 0.91.",
    'Rotated API keys for retrieval service after CI detected leaked token pattern in logs. No unauthorized access observed.',
    'Canary deployment for ingestion-worker failed readiness probe due to missing REDIS_URL env var; rollback completed in 90 seconds.',
    'Enabled HNSW ef_search tuning from 64 to 96 for workspace dev-workspace; median recall improved from 0.82 to 0.89.',
    'Nightly summarization job consolidated 1,240 episodic memories into 86 semantic facts; processor queue stayed under 12 pending items.',
    'Alert fired for anomaly score 0.97 on Slack ingestion throughput drop; cause traced to revoked app-level token.',
    'Backfilled Jira issue events for the last 7 days; dedup logic skipped 14 duplicate updates from webhook retries.',
    'Index compaction finished on retrieval store at 03:48 UTC; query p95 dropped from 410ms to 290ms.',
  ];
  const episodicAgents = ['agent-alpha', 'agent-beta', 'agent-gamma', 'agent-alpha', 'agent-beta', 'agent-gamma', 'agent-alpha', 'agent-beta', 'agent-gamma', 'agent-alpha'];
  const episodicUsers = ['user-001', 'user-002', 'user-001', 'user-002', 'user-001', 'user-002', 'user-001', 'user-002', 'user-001', 'user-002'];
  const episodicScores = [0.95, 0.72, 0.88, 0.41, 0.67, 0.53, 0.83, 0.91, 0.36, 0.24];

  for (let i = 0; i < episodicContents.length; i += 1) {
    const result = await ensureMemory(memoryEndpoint, existingMemoriesJson, {
      workspace_id: WORKSPACE_ID,
      memory_type: 'episodic',
      user_id: episodicUsers[i],
      agent_id: episodicAgents[i],
      repo: 'Quazmoz/memoryops',
      importance_score: episodicScores[i],
      content: episodicContents[i],
      tags: ['demo', 'episodic'],
    }, `episodic memory ${i + 1}/10`);
    existingMemoriesJson = result.existingMemoriesJson;
  }

  log('Step 3/7: Seeding 5 semantic memories');
  const semanticContents = [
    'AKS node pool autoscaler triggers at 80% CPU utilization threshold.',
    'The slow path worker processes LLM consolidation jobs. Half-life for episodic memories defaults to 30 days.',
    'Redis Streams XREADGROUP is used for reliable job delivery with at-least-once semantics.',
    'Hybrid retrieval combines keyword filtering with vector similarity ranking to improve relevance under noisy queries.',
    'Pinned memories are excluded from half-life decay and remain in the fast retrieval lane.',
  ];

  for (let i = 0; i < semanticContents.length; i += 1) {
    const result = await ensureMemory(memoryEndpoint, existingMemoriesJson, {
      workspace_id: WORKSPACE_ID,
      memory_type: 'semantic',
      user_id: i % 2 === 0 ? 'user-001' : 'user-002',
      agent_id: ['agent-alpha', 'agent-beta', 'agent-gamma'][i % 3],
      repo: 'Quazmoz/memoryops',
      importance_score: [0.77, 0.63, 0.81, 0.58, 0.69][i],
      content: semanticContents[i],
      tags: ['demo', 'semantic'],
      scope_visibility: 'workspace',
    }, `semantic memory ${i + 1}/5`);
    existingMemoriesJson = result.existingMemoriesJson;
  }

  log('Step 4/7: Seeding and pinning 2 memories');
  const pinnedContents = [
    'Pinned runbook: If retrieval p95 exceeds 500ms for 5 minutes, trigger index warmup and scale query replicas by +2.',
    'Pinned escalation: During on-call incidents, route Sev-1 alerts to #memoryops-war-room and page incident-responder within 2 minutes.',
  ];

  for (let i = 0; i < pinnedContents.length; i += 1) {
    const result = await ensureMemory(memoryEndpoint, existingMemoriesJson, {
      workspace_id: WORKSPACE_ID,
      memory_type: 'episodic',
      user_id: 'user-001',
      agent_id: 'agent-gamma',
      repo: 'Quazmoz/memoryops',
      importance_score: 0.9,
      content: pinnedContents[i],
      tags: ['demo', 'pinned', 'runbook'],
    }, `pinned candidate memory ${i + 1}/2`);
    existingMemoriesJson = result.existingMemoriesJson;

    const patchRes = await apiRequest('PATCH', `/v1/memory/${result.id}`, { pinned: true });
    if (patchRes.status >= 300) die(`Failed to pin memory id=${result.id}: HTTP ${patchRes.status} - ${patchRes.body}`);
    log(`Pinned memory id=${result.id}`);
  }

  return existingMemoriesJson;
}

async function seedContradictions(memoryEndpoint, existingMemoriesJson) {
  log('Step 5/7: Seeding deterministic contradiction memories and flags');
  const pairs = [
    {
      left: 'Demo contradiction: Qdrant vector search is enabled for the MemoryOps workspace.',
      right: 'Demo contradiction: Qdrant vector search is disabled for the MemoryOps workspace.',
      similarity: 0.94,
      conflictScore: 0.91,
    },
    {
      left: 'Demo contradiction: Episodic memory retention is configured for 30 days.',
      right: 'Demo contradiction: Episodic memory retention is configured for 7 days.',
      similarity: 0.92,
      conflictScore: 0.86,
    },
    {
      left: 'Demo contradiction: Slack ingestion is active and processing workspace events.',
      right: 'Demo contradiction: Slack ingestion is inactive and not processing workspace events.',
      similarity: 0.90,
      conflictScore: 0.84,
    },
  ];

  const memoryPairs = [];
  for (let i = 0; i < pairs.length; i += 1) {
    const pair = pairs[i];
    const left = await ensureMemory(memoryEndpoint, existingMemoriesJson, contradictionMemoryPayload(pair.left), `contradiction memory ${i + 1}A`);
    existingMemoriesJson = left.existingMemoriesJson;
    const right = await ensureMemory(memoryEndpoint, existingMemoriesJson, contradictionMemoryPayload(pair.right), `contradiction memory ${i + 1}B`);
    existingMemoriesJson = right.existingMemoriesJson;
    memoryPairs.push({ ...pair, leftId: left.id, rightId: right.id });
  }

  provisionContradictionFlags(memoryPairs);
  return existingMemoriesJson;
}

function contradictionMemoryPayload(content) {
  return {
    workspace_id: WORKSPACE_ID,
    memory_type: 'semantic',
    user_id: 'demo-user',
    agent_id: 'demo-contradiction-seeder',
    repo: 'memoryops/demo',
    importance_score: 0.9,
    scope_visibility: 'workspace',
    content,
    tags: ['demo', 'contradiction'],
  };
}

function provisionContradictionFlags(pairs) {
  validateUuid(WORKSPACE_ID, 'WORKSPACE_ID');
  for (const pair of pairs) {
    validateUuid(pair.leftId, 'left contradiction memory id');
    validateUuid(pair.rightId, 'right contradiction memory id');
  }

  const sql = pairs.map((pair) => contradictionFlagSql(pair)).join('\n');
  const result = runSql(sql);
  if (!result.ok) {
    die(`Failed to seed contradiction flags. ${result.message}`);
  }
  log(`Provisioned ${pairs.length} open contradiction flags.`);
}

function contradictionFlagSql(pair) {
  return `
WITH existing AS (
  SELECT id, resolution::TEXT AS resolution
  FROM contradiction_flags
  WHERE workspace_id = '${WORKSPACE_ID}'::uuid
    AND ((memory_id_a = '${pair.leftId}'::uuid AND memory_id_b = '${pair.rightId}'::uuid)
      OR (memory_id_a = '${pair.rightId}'::uuid AND memory_id_b = '${pair.leftId}'::uuid))
  ORDER BY created_at DESC
  LIMIT 1
), reopened AS (
  UPDATE contradiction_flags
  SET resolution = 'open'::contradiction_resolution,
      resolved_by = NULL,
      resolved_at = NULL,
      notes = 'Reopened by demo seed data',
      kept_memory_id = NULL,
      discarded_memory_id = NULL
  WHERE id IN (SELECT id FROM existing WHERE resolution <> 'open')
  RETURNING id
), inserted AS (
  INSERT INTO contradiction_flags (workspace_id, memory_id_a, memory_id_b, similarity, conflict_score, resolution, notes)
  SELECT '${WORKSPACE_ID}'::uuid, '${pair.leftId}'::uuid, '${pair.rightId}'::uuid, ${pair.similarity}, ${pair.conflictScore}, 'open'::contradiction_resolution, 'Seeded deterministic demo contradiction'
  WHERE NOT EXISTS (SELECT 1 FROM existing)
  RETURNING id
)
SELECT COALESCE((SELECT id FROM inserted), (SELECT id FROM reopened), (SELECT id FROM existing));`;
}

function runSql(sql) {
  const candidates = [];
  if (process.env.DATABASE_URL) {
    candidates.push({ command: 'psql', args: [process.env.DATABASE_URL, '-v', 'ON_ERROR_STOP=1', '-f', '-'] });
  }
  candidates.push({ command: 'docker', args: ['compose', 'exec', '-T', 'postgres', 'psql', '-U', 'memoryops', '-d', 'memoryops', '-v', 'ON_ERROR_STOP=1', '-f', '-'] });

  const errors = [];
  for (const candidate of candidates) {
    const res = spawnSync(candidate.command, candidate.args, { input: sql, encoding: 'utf8' });
    if (res.status === 0) return { ok: true };
    errors.push(`${candidate.command} ${candidate.args.join(' ')}: ${res.error?.message || res.stderr || res.stdout || `exit ${res.status}`}`.trim());
  }

  return { ok: false, message: errors.join(' | ') };
}

function validateUuid(value, label) {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(String(value))) {
    die(`${label} is not a UUID: ${value}`);
  }
}

async function seedTools() {
  log('Step 6/7: Seeding 3 agent tools when endpoint exists');
  let toolsSupported = true;
  const rc = await upsertTool('incident_responder', 'Handles on-call triage workflows');
  if (rc === 2) toolsSupported = false;
  else if (rc !== 0) process.exit(rc);

  if (toolsSupported) {
    await upsertTool('code_reviewer', 'Reviews PRs and suggests improvements');
    await upsertTool('deploy_monitor', 'Monitors deployment pipelines and alerts');
  }
  return toolsSupported;
}

async function main() {
  log('Step 1/7: Validating workspace and API credentials');
  log(`Using workspace id=${WORKSPACE_ID}`);
  log(`Using API base URL=${API_BASE_URL}`);

  const workspaceRes = await apiRequest('GET', `/v1/workspaces/${WORKSPACE_ID}`);
  if (workspaceRes.status < 200 || workspaceRes.status >= 300) {
    die(`Workspace/API key validation failed: HTTP ${workspaceRes.status} - ${workspaceRes.body}`);
  }

  const memoryEndpoint = await detectMemoryEndpoint();
  log(`Using memory endpoint: ${memoryEndpoint}`);

  let existingMemoriesJson = await seedMemories(memoryEndpoint);
  existingMemoriesJson = await seedContradictions(memoryEndpoint, existingMemoriesJson);
  const toolsSupported = await seedTools();

  const contradictionCount = await apiRequest('GET', `/v1/workspaces/${WORKSPACE_ID}/contradictions/count`);
  const openContradictions = contradictionCount.json?.open ?? 'unknown';

  log('Step 7/7: Summary');
  console.log(`workspace_id=${WORKSPACE_ID}`);
  console.log(`memories_created=${MEMORIES_CREATED}`);
  console.log(`contradiction_flags_created_or_reopened=${openContradictions}`);
  console.log(`tools_created=${toolsSupported ? TOOLS_CREATED : 'skipped(endpoint unavailable)'}`);
  console.log(`verify_memories=curl -H "X-API-Key: <api-key>" "${API_BASE_URL}/v1/memory?workspace_id=${WORKSPACE_ID}"`);
  console.log(`verify_contradictions=curl -H "X-API-Key: <api-key>" "${API_BASE_URL}/v1/workspaces/${WORKSPACE_ID}/contradictions"`);
  console.log('Done.');
}

main().catch((error) => die(error.message));
