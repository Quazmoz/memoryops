#!/usr/bin/env bash
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "Error: jq is required but not installed." >&2
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "Error: curl is required but not installed." >&2
  exit 1
fi

BASE_URL="${BASE_URL:-http://localhost:8080}"
API_KEY="${API_KEY:-}"
WORKSPACE_NAME="dev-workspace"
HALF_LIFE_DAYS=30

if [[ -z "$API_KEY" ]]; then
  echo "Error: API_KEY is required." >&2
  echo "Usage: API_KEY=your-key bash scripts/seed.sh" >&2
  exit 1
fi

MEMORIES_CREATED=0
SKILLS_CREATED=0

RESP_CODE=""
RESP_BODY=""

log() {
  echo "[$(date +"%H:%M:%S")] $*"
}

die() {
  echo "Error: $*" >&2
  exit 1
}

api_request() {
  local method="$1"
  local path="$2"
  local body="${3:-}"

  local url="${BASE_URL}${path}"
  local response

  if [[ -n "$body" ]]; then
    response=$(curl -sS -X "$method" "$url" \
      -H "X-API-Key: $API_KEY" \
      -H "Content-Type: application/json" \
      --data "$body" \
      -w $'\n%{http_code}')
  else
    response=$(curl -sS -X "$method" "$url" \
      -H "X-API-Key: $API_KEY" \
      -H "Content-Type: application/json" \
      -w $'\n%{http_code}')
  fi

  RESP_CODE="$(printf '%s' "$response" | tail -n1)"
  RESP_BODY="$(printf '%s' "$response" | sed '$d')"
}

is_success() {
  [[ "$1" =~ ^2[0-9][0-9]$ ]]
}

extract_workspace_id_from_create() {
  printf '%s' "$1" | jq -r '.workspace_id // .id // .data.workspace_id // .data.id // empty'
}

extract_workspace_id_from_list() {
  local json="$1"
  printf '%s' "$json" | jq -r --arg name "$WORKSPACE_NAME" '
    [
      (if type == "object" and (.name? == $name) then . else empty end),
      (if type == "array" then .[] else empty end),
      (.items? | arrays | .[]),
      (.data? | arrays | .[])
    ]
    | map(select(.name? == $name))
    | .[0]
    | (.workspace_id // .id // empty)
  '
}

extract_memory_id() {
  printf '%s' "$1" | jq -r '.memory_id // .id // .data.memory_id // .data.id // empty'
}

list_memories() {
  api_request GET "/v1/memory?workspace_id=${WORKSPACE_ID}"
  if ! is_success "$RESP_CODE"; then
    return 1
  fi

  printf '%s' "$RESP_BODY"
}

find_memory_id_by_content() {
  local content="$1"
  local list_json="$2"

  printf '%s' "$list_json" | jq -r --arg content "$content" '
    [
      (if type == "array" then .[] else empty end),
      (.items? | arrays | .[]),
      (.data? | arrays | .[])
    ]
    | map(select((.content? // .text? // "") == $content))
    | .[0]
    | (.memory_id // .id // empty)
  '
}

create_memory() {
  local payload="$1"
  local endpoint="$2"

  api_request POST "$endpoint" "$payload"

  if is_success "$RESP_CODE"; then
    return 0
  fi

  if [[ "$endpoint" == "/v1/ingest/raw" ]]; then
    log "Primary memory endpoint /v1/ingest/raw failed with HTTP ${RESP_CODE}; falling back to /v1/memory"
    api_request POST "/v1/memory" "$payload"
    if is_success "$RESP_CODE"; then
      return 0
    fi
  fi

  return 1
}

upsert_skill() {
  local skill_name="$1"
  local skill_description="$2"

  local list_code list_body existing_id payload

  api_request GET "/v1/workspaces/${WORKSPACE_ID}/skills"
  list_code="$RESP_CODE"
  list_body="$RESP_BODY"

  if [[ "$list_code" == "404" || "$list_code" == "405" ]]; then
    log "Skills endpoint unavailable; skipping skills seeding."
    return 2
  fi

  if ! is_success "$list_code"; then
    die "Failed to query skills endpoint: HTTP ${list_code} - ${list_body}"
  fi

  existing_id=$(printf '%s' "$list_body" | jq -r --arg name "$skill_name" '
    [
      (if type == "array" then .[] else empty end),
      (.items? | arrays | .[]),
      (.data? | arrays | .[])
    ]
    | map(select(.name? == $name))
    | .[0]
    | (.id // .skill_id // empty)
  ')

  if [[ -n "$existing_id" ]]; then
    log "Skill ${skill_name} already exists; skipping."
    return 0
  fi

  payload=$(jq -n \
    --arg name "$skill_name" \
    --arg description "$skill_description" \
    '{name: $name, description: $description, endpoint_url: "https://example.com/skills/\($name)"}')

  api_request POST "/v1/workspaces/${WORKSPACE_ID}/skills" "$payload"
  if ! is_success "$RESP_CODE"; then
    die "Failed to create skill ${skill_name}: HTTP ${RESP_CODE} - ${RESP_BODY}"
  fi

  SKILLS_CREATED=$((SKILLS_CREATED + 1))
  log "Created skill: ${skill_name}"
}

log "Step 1/6: Ensuring workspace exists"
workspace_payload=$(jq -n \
  --arg name "$WORKSPACE_NAME" \
  --argjson half_life_days "$HALF_LIFE_DAYS" \
  '{name: $name, config: {half_life_days: $half_life_days}}')

api_request POST "/v1/workspaces" "$workspace_payload"

if is_success "$RESP_CODE"; then
  WORKSPACE_ID="$(extract_workspace_id_from_create "$RESP_BODY")"
  [[ -n "$WORKSPACE_ID" ]] || die "Workspace created but workspace_id not found in response: ${RESP_BODY}"
  log "Created workspace '${WORKSPACE_NAME}' (id=${WORKSPACE_ID})"
elif [[ "$RESP_CODE" == "409" ]]; then
  log "Workspace '${WORKSPACE_NAME}' already exists; fetching existing workspace_id"

  api_request GET "/v1/workspaces"
  if ! is_success "$RESP_CODE"; then
    die "Failed to fetch workspaces after 409: HTTP ${RESP_CODE} - ${RESP_BODY}"
  fi

  WORKSPACE_ID="$(extract_workspace_id_from_list "$RESP_BODY")"
  [[ -n "$WORKSPACE_ID" ]] || die "Could not find existing workspace_id for '${WORKSPACE_NAME}'"
  log "Using existing workspace id=${WORKSPACE_ID}"
else
  die "Failed to create workspace: HTTP ${RESP_CODE} - ${RESP_BODY}"
fi

log "Determining preferred memory write endpoint"
MEMORY_ENDPOINT="/v1/memory"
api_request GET "/v1/ingest/raw"
if [[ "$RESP_CODE" != "404" ]]; then
  MEMORY_ENDPOINT="/v1/ingest/raw"
fi
log "Using memory endpoint: ${MEMORY_ENDPOINT}"

log "Step 2/6: Seeding 10 episodic memories"

episodic_contents=(
  "Deployed memoryops v0.15.0 to production AKS cluster at 14:32 UTC. All health checks passed. Rollout took 4m22s."
  "Qdrant vector DB cold start latency spiked to 2400ms during embedding of batch job at 09:15 UTC. Root cause: container memory limit hit."
  "User user-001 queried 'recent deployment failures' - hybrid search returned 3 results, top score 0.91."
  "Rotated API keys for retrieval service after CI detected leaked token pattern in logs. No unauthorized access observed."
  "Canary deployment for ingestion-worker failed readiness probe due to missing REDIS_URL env var; rollback completed in 90 seconds."
  "Enabled HNSW ef_search tuning from 64 to 96 for workspace dev-workspace; median recall improved from 0.82 to 0.89."
  "Nightly summarization job consolidated 1,240 episodic memories into 86 semantic facts; processor queue stayed under 12 pending items."
  "Alert fired for anomaly score 0.97 on Slack ingestion throughput drop; cause traced to revoked app-level token."
  "Backfilled Jira issue events for the last 7 days; dedup logic skipped 14 duplicate updates from webhook retries."
  "Index compaction finished on retrieval store at 03:48 UTC; query p95 dropped from 410ms to 290ms."
)

episodic_agents=(
  "agent-alpha"
  "agent-beta"
  "agent-gamma"
  "agent-alpha"
  "agent-beta"
  "agent-gamma"
  "agent-alpha"
  "agent-beta"
  "agent-gamma"
  "agent-alpha"
)

episodic_users=(
  "user-001"
  "user-002"
  "user-001"
  "user-002"
  "user-001"
  "user-002"
  "user-001"
  "user-002"
  "user-001"
  "user-002"
)

episodic_scores=(0.95 0.72 0.88 0.41 0.67 0.53 0.83 0.91 0.36 0.24)
episodic_dates=(
  "2026-04-10T14:32:00Z"
  "2026-04-11T09:15:00Z"
  "2026-04-12T11:04:00Z"
  "2026-04-13T08:22:00Z"
  "2026-04-14T16:48:00Z"
  "2026-04-15T07:31:00Z"
  "2026-04-16T01:10:00Z"
  "2026-04-17T19:27:00Z"
  "2026-04-18T13:05:00Z"
  "2026-04-19T03:48:00Z"
)

existing_memories_json=""
if existing_memories_json="$(list_memories 2>/dev/null)"; then
  :
else
  existing_memories_json="[]"
fi

i=0
while [[ $i -lt ${#episodic_contents[@]} ]]; do
  content="${episodic_contents[$i]}"
  existing_id="$(find_memory_id_by_content "$content" "$existing_memories_json")"

  if [[ -n "$existing_id" ]]; then
    log "Episodic memory $((i + 1))/10 already exists (id=${existing_id}); skipping."
    i=$((i + 1))
    continue
  fi

  payload=$(jq -n \
    --arg workspace_id "$WORKSPACE_ID" \
    --arg memory_type "episodic" \
    --arg user_id "${episodic_users[$i]}" \
    --arg agent_id "${episodic_agents[$i]}" \
    --arg content "$content" \
    --argjson importance_score "${episodic_scores[$i]}" \
    --arg occurred_at "${episodic_dates[$i]}" \
    '{
      workspace_id: $workspace_id,
      memory_type: $memory_type,
      user_id: $user_id,
      agent_id: $agent_id,
      importance_score: $importance_score,
      content: $content,
      metadata: {occurred_at: $occurred_at}
    }')

  if ! create_memory "$payload" "$MEMORY_ENDPOINT"; then
    die "Failed to create episodic memory $((i + 1)): HTTP ${RESP_CODE} - ${RESP_BODY}"
  fi

  memory_id="$(extract_memory_id "$RESP_BODY")"
  MEMORIES_CREATED=$((MEMORIES_CREATED + 1))
  log "Created episodic memory $((i + 1))/10 (id=${memory_id:-unknown})"

  if refreshed="$(list_memories 2>/dev/null)"; then
    existing_memories_json="$refreshed"
  fi

  i=$((i + 1))
done

log "Step 3/6: Seeding 5 semantic memories"

semantic_contents=(
  "AKS node pool autoscaler triggers at 80% CPU utilization threshold."
  "The slow path worker processes LLM consolidation jobs. Half-life for episodic memories defaults to 30 days."
  "Redis Streams XREADGROUP is used for reliable job delivery with at-least-once semantics."
  "Hybrid retrieval combines keyword filtering with vector similarity ranking to improve relevance under noisy queries."
  "Pinned memories are excluded from half-life decay and remain in the fast retrieval lane."
)

semantic_agents=("agent-alpha" "agent-beta" "agent-gamma" "agent-alpha" "agent-beta")
semantic_users=("user-001" "user-002" "user-001" "user-002" "user-001")
semantic_scores=(0.77 0.63 0.81 0.58 0.69)
semantic_dates=(
  "2026-04-20T10:00:00Z"
  "2026-04-21T10:00:00Z"
  "2026-04-22T10:00:00Z"
  "2026-04-23T10:00:00Z"
  "2026-04-24T10:00:00Z"
)

i=0
while [[ $i -lt ${#semantic_contents[@]} ]]; do
  content="${semantic_contents[$i]}"
  existing_id="$(find_memory_id_by_content "$content" "$existing_memories_json")"

  if [[ -n "$existing_id" ]]; then
    log "Semantic memory $((i + 1))/5 already exists (id=${existing_id}); skipping."
    i=$((i + 1))
    continue
  fi

  payload=$(jq -n \
    --arg workspace_id "$WORKSPACE_ID" \
    --arg memory_type "semantic" \
    --arg user_id "${semantic_users[$i]}" \
    --arg agent_id "${semantic_agents[$i]}" \
    --arg content "$content" \
    --argjson importance_score "${semantic_scores[$i]}" \
    --arg occurred_at "${semantic_dates[$i]}" \
    '{
      workspace_id: $workspace_id,
      memory_type: $memory_type,
      user_id: $user_id,
      agent_id: $agent_id,
      importance_score: $importance_score,
      content: $content,
      metadata: {occurred_at: $occurred_at}
    }')

  if ! create_memory "$payload" "$MEMORY_ENDPOINT"; then
    die "Failed to create semantic memory $((i + 1)): HTTP ${RESP_CODE} - ${RESP_BODY}"
  fi

  memory_id="$(extract_memory_id "$RESP_BODY")"
  MEMORIES_CREATED=$((MEMORIES_CREATED + 1))
  log "Created semantic memory $((i + 1))/5 (id=${memory_id:-unknown})"

  if refreshed="$(list_memories 2>/dev/null)"; then
    existing_memories_json="$refreshed"
  fi

  i=$((i + 1))
done

log "Step 4/6: Seeding and pinning 2 memories"

pinned_contents=(
  "Pinned runbook: If retrieval p95 exceeds 500ms for 5 minutes, trigger index warmup and scale query replicas by +2."
  "Pinned escalation: During on-call incidents, route Sev-1 alerts to #memoryops-war-room and page incident-responder within 2 minutes."
)

p=0
while [[ $p -lt ${#pinned_contents[@]} ]]; do
  content="${pinned_contents[$p]}"
  memory_id="$(find_memory_id_by_content "$content" "$existing_memories_json")"

  if [[ -z "$memory_id" ]]; then
    payload=$(jq -n \
      --arg workspace_id "$WORKSPACE_ID" \
      --arg memory_type "episodic" \
      --arg user_id "user-001" \
      --arg agent_id "agent-gamma" \
      --arg content "$content" \
      --argjson importance_score "0.9" \
      --arg occurred_at "2026-04-25T12:00:00Z" \
      '{
        workspace_id: $workspace_id,
        memory_type: $memory_type,
        user_id: $user_id,
        agent_id: $agent_id,
        importance_score: $importance_score,
        content: $content,
        metadata: {occurred_at: $occurred_at}
      }')

    if ! create_memory "$payload" "$MEMORY_ENDPOINT"; then
      die "Failed to create pinned memory $((p + 1)): HTTP ${RESP_CODE} - ${RESP_BODY}"
    fi

    memory_id="$(extract_memory_id "$RESP_BODY")"
    [[ -n "$memory_id" ]] || die "Pinned memory created but id not found in response: ${RESP_BODY}"
    MEMORIES_CREATED=$((MEMORIES_CREATED + 1))
    log "Created pinned candidate memory $((p + 1))/2 (id=${memory_id})"

    if refreshed="$(list_memories 2>/dev/null)"; then
      existing_memories_json="$refreshed"
    fi
  else
    log "Pinned candidate memory $((p + 1))/2 already exists (id=${memory_id})"
  fi

  patch_payload='{"pinned": true}'
  api_request PATCH "/v1/memory/${memory_id}" "$patch_payload"
  if ! is_success "$RESP_CODE"; then
    die "Failed to pin memory id=${memory_id}: HTTP ${RESP_CODE} - ${RESP_BODY}"
  fi

  log "Pinned memory id=${memory_id}"
  p=$((p + 1))
done

log "Step 5/6: Seeding 3 agent skills when endpoint exists"
skills_supported=true

if ! upsert_skill "incident_responder" "Handles on-call triage workflows"; then
  rc=$?
  if [[ $rc -eq 2 ]]; then
    skills_supported=false
  else
    exit $rc
  fi
fi

if [[ "$skills_supported" == true ]]; then
  upsert_skill "code_reviewer" "Reviews PRs and suggests improvements"
  upsert_skill "deploy_monitor" "Monitors deployment pipelines and alerts"
fi

log "Step 6/6: Summary"
echo "workspace_id=${WORKSPACE_ID}"
echo "api_key=${API_KEY}"
echo "memories_created=${MEMORIES_CREATED}"
if [[ "$skills_supported" == true ]]; then
  echo "skills_created=${SKILLS_CREATED}"
else
  echo "skills_created=skipped(endpoint unavailable)"
fi
echo "verify_command=curl -H \"X-API-Key: ${API_KEY}\" \"${BASE_URL}/v1/memory?workspace_id=${WORKSPACE_ID}\""

echo "Done."
