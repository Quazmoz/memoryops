#!/usr/bin/env node

const API_KEY = process.env.API_KEY;
const API_BASE_URL = process.env.API_BASE_URL || 'http://localhost:8080';
let WORKSPACE_ID = process.env.WORKSPACE_ID;
const WORKSPACE_NAME = process.env.WORKSPACE_NAME || 'dev-workspace';
const HALF_LIFE_DAYS = 30;

if (!API_KEY) {
  console.error("Error: API_KEY is required.");
  console.error("Usage: API_KEY=your-key node scripts/seed.mjs");
  process.exit(1);
}

let MEMORIES_CREATED = 0;
let TOOLS_CREATED = 0;

function log(msg) {
  const time = new Date().toISOString().substring(11, 19);
  console.log(`[${time}] ${msg}`);
}

function die(msg) {
  console.error(`Error: ${msg}`);
  process.exit(1);
}

async function apiRequest(method, path, body = null) {
  const url = `${API_BASE_URL}${path}`;
  const options = {
    method,
    headers: {
      'X-API-Key': API_KEY,
      'Content-Type': 'application/json'
    }
  };
  if (body) {
    options.body = typeof body === 'string' ? body : JSON.stringify(body);
  }

  const response = await fetch(url, options);
  const text = await response.text();
  let json = null;
  try {
    json = JSON.parse(text);
  } catch (e) {
    // Ignore JSON parse error
  }
  return { status: response.status, body: text, json };
}

function extractWorkspaceId(json) {
  if (!json) return null;
  return json.workspace_id || json.id || (json.data && (json.data.workspace_id || json.data.id)) || null;
}

function extractMemoryId(json) {
  if (!json) return null;
  return json.memory_id || json.id || (json.data && (json.data.memory_id || json.data.id)) || null;
}

async function listMemories() {
  const res = await apiRequest('GET', `/v1/memory?workspace_id=${WORKSPACE_ID}`);
  if (res.status >= 200 && res.status < 300) {
    return res.json;
  }
  return null;
}

function findMemoryIdByContent(content, listJson) {
  if (!listJson) return null;
  let items = [];
  if (Array.isArray(listJson)) items = listJson;
  else if (listJson.items && Array.isArray(listJson.items)) items = listJson.items;
  else if (listJson.data && Array.isArray(listJson.data)) items = listJson.data;

  const item = items.find(i => (i.content || i.text || "") === content);
  return item ? (item.memory_id || item.id) : null;
}

async function upsertTool(toolName, toolDescription) {
  const listRes = await apiRequest('GET', `/v1/workspaces/${WORKSPACE_ID}/tools`);
  if (listRes.status === 404 || listRes.status === 405) {
    log(`Tools endpoint unavailable; skipping tools seeding.`);
    return 2;
  }
  if (listRes.status < 200 || listRes.status >= 300) {
    die(`Failed to query tools endpoint: HTTP ${listRes.status} - ${listRes.body}`);
  }

  let items = [];
  if (Array.isArray(listRes.json)) items = listRes.json;
  else if (listRes.json.items && Array.isArray(listRes.json.items)) items = listRes.json.items;
  else if (listRes.json.data && Array.isArray(listRes.json.data)) items = listRes.json.data;

  const existing = items.find(i => i.name === toolName);
  if (existing) {
    log(`Tool ${toolName} already exists; skipping.`);
    return 0;
  }

  const payload = {
    name: toolName,
    description: toolDescription,
    endpoint_url: `https://example.com/tools/${toolName}`
  };

  const createRes = await apiRequest('POST', `/v1/workspaces/${WORKSPACE_ID}/tools`, payload);
  if (createRes.status < 200 || createRes.status >= 300) {
    die(`Failed to create tool ${toolName}: HTTP ${createRes.status} - ${createRes.body}`);
  }

  TOOLS_CREATED++;
  log(`Created tool: ${toolName}`);
  return 0;
}

async function main() {
  log("Step 1/6: Ensuring workspace exists");
  
  if (!WORKSPACE_ID) {
    const wsPayload = {
      name: WORKSPACE_NAME,
      config: { half_life_days: HALF_LIFE_DAYS }
    };
    let wsRes = await apiRequest('POST', '/v1/workspaces', wsPayload);
    
    if (wsRes.status >= 200 && wsRes.status < 300) {
      WORKSPACE_ID = extractWorkspaceId(wsRes.json);
      if (!WORKSPACE_ID) die(`Workspace created but workspace_id not found in response: ${wsRes.body}`);
      log(`Created workspace '${WORKSPACE_NAME}' (id=${WORKSPACE_ID})`);
    } else if (wsRes.status === 409) {
      log(`Workspace '${WORKSPACE_NAME}' already exists; fetching existing workspace_id`);
      wsRes = await apiRequest('GET', '/v1/workspaces');
      if (wsRes.status < 200 || wsRes.status >= 300) {
        die(`Failed to fetch workspaces after 409: HTTP ${wsRes.status} - ${wsRes.body}`);
      }
      
      let items = [];
      if (Array.isArray(wsRes.json)) items = wsRes.json;
      else if (wsRes.json.items && Array.isArray(wsRes.json.items)) items = wsRes.json.items;
      else if (wsRes.json.data && Array.isArray(wsRes.json.data)) items = wsRes.json.data;
      
      const existing = items.find(i => i.name === WORKSPACE_NAME);
      WORKSPACE_ID = existing ? (existing.workspace_id || existing.id) : null;
      if (!WORKSPACE_ID) die(`Could not find existing workspace_id for '${WORKSPACE_NAME}'`);
      log(`Using existing workspace id=${WORKSPACE_ID}`);
    } else {
      die(`Failed to create workspace: HTTP ${wsRes.status} - ${wsRes.body}`);
    }
  } else {
    log(`Using provided workspace id=${WORKSPACE_ID}`);
  }

  log("Determining preferred memory write endpoint");
  let memoryEndpoint = "/v1/memory";
  const rawRes = await apiRequest('GET', '/v1/ingest/raw');
  if (rawRes.status !== 404) {
    memoryEndpoint = "/v1/ingest/raw";
  }
  log(`Using memory endpoint: ${memoryEndpoint}`);

  log("Step 2/6: Seeding 10 episodic memories");
  const episodicContents = [
    "Deployed memoryops v0.15.0 to production AKS cluster at 14:32 UTC. All health checks passed. Rollout took 4m22s.",
    "Qdrant vector DB cold start latency spiked to 2400ms during embedding of batch job at 09:15 UTC. Root cause: container memory limit hit.",
    "User user-001 queried 'recent deployment failures' - hybrid search returned 3 results, top score 0.91.",
    "Rotated API keys for retrieval service after CI detected leaked token pattern in logs. No unauthorized access observed.",
    "Canary deployment for ingestion-worker failed readiness probe due to missing REDIS_URL env var; rollback completed in 90 seconds.",
    "Enabled HNSW ef_search tuning from 64 to 96 for workspace dev-workspace; median recall improved from 0.82 to 0.89.",
    "Nightly summarization job consolidated 1,240 episodic memories into 86 semantic facts; processor queue stayed under 12 pending items.",
    "Alert fired for anomaly score 0.97 on Slack ingestion throughput drop; cause traced to revoked app-level token.",
    "Backfilled Jira issue events for the last 7 days; dedup logic skipped 14 duplicate updates from webhook retries.",
    "Index compaction finished on retrieval store at 03:48 UTC; query p95 dropped from 410ms to 290ms."
  ];
  const episodicAgents = ["agent-alpha", "agent-beta", "agent-gamma", "agent-alpha", "agent-beta", "agent-gamma", "agent-alpha", "agent-beta", "agent-gamma", "agent-alpha"];
  const episodicUsers = ["user-001", "user-002", "user-001", "user-002", "user-001", "user-002", "user-001", "user-002", "user-001", "user-002"];
  const episodicScores = [0.95, 0.72, 0.88, 0.41, 0.67, 0.53, 0.83, 0.91, 0.36, 0.24];
  const episodicDates = [
    "2026-04-10T14:32:00Z", "2026-04-11T09:15:00Z", "2026-04-12T11:04:00Z", "2026-04-13T08:22:00Z",
    "2026-04-14T16:48:00Z", "2026-04-15T07:31:00Z", "2026-04-16T01:10:00Z", "2026-04-17T19:27:00Z",
    "2026-04-18T13:05:00Z", "2026-04-19T03:48:00Z"
  ];

  let existingMemoriesJson = await listMemories() || [];

  for (let i = 0; i < episodicContents.length; i++) {
    const content = episodicContents[i];
    let existingId = findMemoryIdByContent(content, existingMemoriesJson);
    if (existingId) {
      log(`Episodic memory ${i + 1}/10 already exists (id=${existingId}); skipping.`);
      continue;
    }

    const payload = {
      workspace_id: WORKSPACE_ID,
      memory_type: "episodic",
      user_id: episodicUsers[i],
      agent_id: episodicAgents[i],
      importance_score: episodicScores[i],
      content: content,
      metadata: { occurred_at: episodicDates[i] }
    };

    let res = await apiRequest('POST', memoryEndpoint, payload);
    if (res.status >= 300) {
      if (memoryEndpoint === "/v1/ingest/raw") {
        log(`Primary memory endpoint failed with HTTP ${res.status}; falling back to /v1/memory`);
        res = await apiRequest('POST', '/v1/memory', payload);
      }
      if (res.status >= 300) {
        die(`Failed to create episodic memory ${i + 1}: HTTP ${res.status} - ${res.body}`);
      }
    }

    const memoryId = extractMemoryId(res.json);
    MEMORIES_CREATED++;
    log(`Created episodic memory ${i + 1}/10 (id=${memoryId || 'unknown'})`);
    
    existingMemoriesJson = await listMemories() || existingMemoriesJson;
  }

  log("Step 3/6: Seeding 5 semantic memories");
  const semanticContents = [
    "AKS node pool autoscaler triggers at 80% CPU utilization threshold.",
    "The slow path worker processes LLM consolidation jobs. Half-life for episodic memories defaults to 30 days.",
    "Redis Streams XREADGROUP is used for reliable job delivery with at-least-once semantics.",
    "Hybrid retrieval combines keyword filtering with vector similarity ranking to improve relevance under noisy queries.",
    "Pinned memories are excluded from half-life decay and remain in the fast retrieval lane."
  ];
  const semanticAgents = ["agent-alpha", "agent-beta", "agent-gamma", "agent-alpha", "agent-beta"];
  const semanticUsers = ["user-001", "user-002", "user-001", "user-002", "user-001"];
  const semanticScores = [0.77, 0.63, 0.81, 0.58, 0.69];
  const semanticDates = [
    "2026-04-20T10:00:00Z", "2026-04-21T10:00:00Z", "2026-04-22T10:00:00Z", "2026-04-23T10:00:00Z", "2026-04-24T10:00:00Z"
  ];

  for (let i = 0; i < semanticContents.length; i++) {
    const content = semanticContents[i];
    let existingId = findMemoryIdByContent(content, existingMemoriesJson);
    if (existingId) {
      log(`Semantic memory ${i + 1}/5 already exists (id=${existingId}); skipping.`);
      continue;
    }

    const payload = {
      workspace_id: WORKSPACE_ID,
      memory_type: "semantic",
      user_id: semanticUsers[i],
      agent_id: semanticAgents[i],
      importance_score: semanticScores[i],
      content: content,
      metadata: { occurred_at: semanticDates[i] }
    };

    let res = await apiRequest('POST', memoryEndpoint, payload);
    if (res.status >= 300) {
      if (memoryEndpoint === "/v1/ingest/raw") {
        res = await apiRequest('POST', '/v1/memory', payload);
      }
      if (res.status >= 300) {
        die(`Failed to create semantic memory ${i + 1}: HTTP ${res.status} - ${res.body}`);
      }
    }

    const memoryId = extractMemoryId(res.json);
    MEMORIES_CREATED++;
    log(`Created semantic memory ${i + 1}/5 (id=${memoryId || 'unknown'})`);
    
    existingMemoriesJson = await listMemories() || existingMemoriesJson;
  }

  log("Step 4/6: Seeding and pinning 2 memories");
  const pinnedContents = [
    "Pinned runbook: If retrieval p95 exceeds 500ms for 5 minutes, trigger index warmup and scale query replicas by +2.",
    "Pinned escalation: During on-call incidents, route Sev-1 alerts to #memoryops-war-room and page incident-responder within 2 minutes."
  ];

  for (let p = 0; p < pinnedContents.length; p++) {
    const content = pinnedContents[p];
    let memoryId = findMemoryIdByContent(content, existingMemoriesJson);

    if (!memoryId) {
      const payload = {
        workspace_id: WORKSPACE_ID,
        memory_type: "episodic",
        user_id: "user-001",
        agent_id: "agent-gamma",
        importance_score: 0.9,
        content: content,
        metadata: { occurred_at: "2026-04-25T12:00:00Z" }
      };

      let res = await apiRequest('POST', memoryEndpoint, payload);
      if (res.status >= 300) {
        if (memoryEndpoint === "/v1/ingest/raw") res = await apiRequest('POST', '/v1/memory', payload);
        if (res.status >= 300) die(`Failed to create pinned memory ${p + 1}: HTTP ${res.status} - ${res.body}`);
      }

      memoryId = extractMemoryId(res.json);
      if (!memoryId) die(`Pinned memory created but id not found in response: ${res.body}`);
      MEMORIES_CREATED++;
      log(`Created pinned candidate memory ${p + 1}/2 (id=${memoryId})`);
      existingMemoriesJson = await listMemories() || existingMemoriesJson;
    } else {
      log(`Pinned candidate memory ${p + 1}/2 already exists (id=${memoryId})`);
    }

    const patchRes = await apiRequest('PATCH', `/v1/memory/${memoryId}`, { pinned: true });
    if (patchRes.status >= 300) {
      die(`Failed to pin memory id=${memoryId}: HTTP ${patchRes.status} - ${patchRes.body}`);
    }
    log(`Pinned memory id=${memoryId}`);
  }

  log("Step 5/6: Seeding 3 agent tools when endpoint exists");
  let toolsSupported = true;
  let rc = await upsertTool("incident_responder", "Handles on-call triage workflows");
  if (rc === 2) {
    toolsSupported = false;
  } else if (rc !== 0) {
    process.exit(rc);
  }

  if (toolsSupported) {
    await upsertTool("code_reviewer", "Reviews PRs and suggests improvements");
    await upsertTool("deploy_monitor", "Monitors deployment pipelines and alerts");
  }

  log("Step 6/6: Summary");
  console.log(`workspace_id=${WORKSPACE_ID}`);
  console.log(`api_key=${API_KEY}`);
  console.log(`memories_created=${MEMORIES_CREATED}`);
  console.log(`tools_created=${toolsSupported ? TOOLS_CREATED : 'skipped(endpoint unavailable)'}`);
  console.log(`verify_command=curl -H "X-API-Key: ${API_KEY}" "${API_BASE_URL}/v1/memory?workspace_id=${WORKSPACE_ID}"`);
  console.log("Done.");
}

main();
