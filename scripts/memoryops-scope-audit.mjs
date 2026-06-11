#!/usr/bin/env node

import {
  apiRequest,
  asBoolean,
  asNumber,
  fail,
  parseArgs,
  printJson,
  resolveMemoryOpsConfig,
  splitCsv,
} from './memoryops-common.mjs';

const USAGE = `
MemoryOps scope-aware retrieval audit

Usage:
  node scripts/memoryops-scope-audit.mjs "query" [options]

Options:
  --agent-id <id>               Agent scope to retrieve as
  --user-id <id>                User scope to retrieve as
  --repo <owner/name>           Repo scope to retrieve as
  --source-ref <ref>            Source reference filter when supported by the backend
  --include-workspace-pool      Include workspace-published memories
  --include-master-memory       Include master/global memory
  --as-of <timestamp>           Point-in-time retrieval timestamp
  --token-budget <tokens>       Token budget, default 4096
  --limit <n>                   Max memories, default 10
  --search-mode <mode>          hybrid, vector, or keyword. Default hybrid
  --tags a,b,c                  Optional tag filter
  --memory-types episodic,semantic
  --json                        Print machine-readable JSON only
  --api-url <url>               Overrides MEMORYOPS_API_URL
  --workspace-id <uuid>         Overrides MEMORYOPS_WORKSPACE_ID
  --api-key <key>               Overrides MEMORYOPS_API_KEY

Examples:
  node scripts/memoryops-scope-audit.mjs "tool secret handling" --agent-id vscode --repo Quazmoz/memoryops --include-workspace-pool --include-master-memory
  node scripts/memoryops-scope-audit.mjs "auth decisions" --user-id quinn --repo Quazmoz/memoryops --as-of 2026-04-15T00:00:00Z
`;

const { options, positional } = parseArgs();

if (options.help || options.h) {
  console.log(USAGE);
  process.exit(0);
}

const query = positional.join(' ').trim();
if (!query) {
  fail(USAGE.trim());
}

let config;
try {
  config = resolveMemoryOpsConfig(options);
} catch (error) {
  fail(error.message);
}

const request = buildRequest(config.workspaceId, query, options);

try {
  const response = await apiRequest(config, 'POST', '/v1/retrieve', request);
  const audit = buildAudit(request, response);

  if (asBoolean(options.json, false)) {
    printJson(audit);
  } else {
    printAudit(audit);
  }
} catch (error) {
  fail(`Scope audit failed: ${error.message}`);
}

function buildRequest(workspaceId, query, options) {
  const scope = cleanObject({
    agent_id: options.agentId,
    user_id: options.userId,
    repo: options.repo,
  });

  const filters = cleanObject({
    source_ref: options.sourceRef,
    tags: splitCsv(options.tags),
  });

  const memoryTypes = splitCsv(options.memoryTypes);

  return cleanObject({
    workspace_id: workspaceId,
    query,
    limit: asNumber(options.limit, 10),
    token_budget: asNumber(options.tokenBudget, 4096),
    search_mode: options.searchMode || 'hybrid',
    include_trace: true,
    include_workspace_pool: asBoolean(options.includeWorkspacePool, false),
    include_master_memory: asBoolean(options.includeMasterMemory, false),
    as_of: options.asOf,
    scope: Object.keys(scope).length > 0 ? scope : undefined,
    agent_id: options.agentId,
    user_id: options.userId,
    repo: options.repo,
    memory_types: memoryTypes.length > 0 ? memoryTypes : undefined,
    filters: Object.keys(filters).length > 0 ? filters : undefined,
  });
}

function buildAudit(request, response) {
  const memories = normalizeMemories(response);
  const included = memories.map((memory, index) => describeMemory(memory, index + 1));
  const traceCandidates = normalizeTraceCandidates(response);
  const excluded = traceCandidates
    .filter((candidate) => candidate.included === false)
    .map((candidate) => ({
      memory_id: candidate.memory_id || candidate.id || null,
      reason: candidate.exclusion_reason || inferExclusionReason(candidate),
      memory_type: candidate.memory_type || null,
      final_score: candidate.final_score ?? candidate.score ?? null,
      token_count: candidate.token_count ?? null,
      snippet: snippet(candidate.content_snippet || candidate.content || ''),
    }));

  return {
    query: request.query,
    request_scope: {
      agent_id: request.agent_id || request.scope?.agent_id || null,
      user_id: request.user_id || request.scope?.user_id || null,
      repo: request.repo || request.scope?.repo || null,
      include_workspace_pool: Boolean(request.include_workspace_pool),
      include_master_memory: Boolean(request.include_master_memory),
      as_of: request.as_of || null,
    },
    query_id: response?.query_id || null,
    elapsed_ms: response?.elapsed_ms ?? null,
    total_tokens: response?.total_tokens ?? response?.token_count ?? null,
    total_candidates: response?.total_candidates ?? response?.trace?.total_candidates ?? null,
    included,
    excluded,
    warnings: buildWarnings(request, included, excluded, response),
  };
}

function describeMemory(memory, rank) {
  const scope = normalizeScope(memory.scope);
  return {
    rank,
    id: memory.id || memory.memory_id || null,
    memory_type: memory.memory_type || null,
    scope_visibility: memory.scope_visibility || null,
    scope_class: classifyScope(memory, scope),
    scope,
    tags: Array.isArray(memory.tags) ? memory.tags : [],
    importance_score: memory.importance_score ?? null,
    decay_score: memory.decay_score ?? null,
    relevance_score: memory.relevance_score ?? null,
    rrf_score: memory.rrf_score ?? memory.score ?? null,
    token_count: memory.token_count ?? null,
    snippet: snippet(memory.content || ''),
  };
}

function classifyScope(memory, scope) {
  if (memory.scope_visibility === 'workspace') return 'workspace-published';
  if (memory.scope_visibility === 'private') {
    if (scope.user_id) return 'user-private';
    if (scope.agent_id) return 'agent-private';
    if (scope.repo) return 'repo-private';
    return 'private';
  }
  if (scope.user_id) return 'user-scoped';
  if (scope.agent_id) return 'agent-scoped';
  if (scope.repo) return 'repo-scoped';
  return 'workspace-local';
}

function buildWarnings(request, included, excluded, response) {
  const warnings = [];
  if (request.include_master_memory && included.every((item) => item.scope_class !== 'master')) {
    warnings.push('include_master_memory was enabled, but no included result was classified as master memory. This may be expected if no master memory matched.');
  }
  if (!request.include_workspace_pool && included.some((item) => item.scope_class === 'workspace-published')) {
    warnings.push('A workspace-published memory was included even though include_workspace_pool was false. Verify backend scope enforcement.');
  }
  if (excluded.length === 0 && !response?.trace) {
    warnings.push('No trace candidates were returned. Exclusion reasons require backend trace support.');
  }
  return warnings;
}

function normalizeMemories(response) {
  if (!response) return [];
  if (Array.isArray(response.memories)) return response.memories;
  if (Array.isArray(response.items)) return response.items;
  if (Array.isArray(response.results)) return response.results.map((item) => item.memory || item);
  return [];
}

function normalizeTraceCandidates(response) {
  const trace = response?.trace;
  if (!trace) return [];
  if (Array.isArray(trace.candidates)) return trace.candidates;
  if (Array.isArray(trace.entries)) return trace.entries;
  return [];
}

function normalizeScope(scope) {
  if (!scope || typeof scope !== 'object' || Array.isArray(scope)) return {};
  return scope;
}

function inferExclusionReason(candidate) {
  if (candidate.token_count && candidate.token_count > 0) return 'not_included';
  return 'excluded_by_retrieval_pipeline';
}

function printAudit(audit) {
  console.log(`MemoryOps scope audit: ${audit.query}`);
  console.log(`Query ID: ${audit.query_id || 'n/a'}`);
  console.log(`Scope: agent=${audit.request_scope.agent_id || '-'} user=${audit.request_scope.user_id || '-'} repo=${audit.request_scope.repo || '-'}`);
  console.log(`Pools: workspace=${audit.request_scope.include_workspace_pool ? 'on' : 'off'} master=${audit.request_scope.include_master_memory ? 'on' : 'off'} as_of=${audit.request_scope.as_of || '-'}`);
  console.log(`Tokens: ${audit.total_tokens ?? 'n/a'} | Candidates: ${audit.total_candidates ?? 'n/a'} | Elapsed: ${audit.elapsed_ms ?? 'n/a'}ms`);
  console.log('');

  console.log(`Included memories (${audit.included.length})`);
  if (audit.included.length === 0) {
    console.log('  none');
  }
  for (const item of audit.included) {
    console.log(`  ${item.rank}. ${item.id || '<no-id>'} [${item.scope_class}] ${item.memory_type || ''}`.trim());
    console.log(`     scope=${formatScope(item.scope)} tokens=${item.token_count ?? 'n/a'} score=${item.rrf_score ?? 'n/a'}`);
    console.log(`     ${item.snippet}`);
  }

  console.log('');
  console.log(`Excluded trace candidates (${audit.excluded.length})`);
  if (audit.excluded.length === 0) {
    console.log('  none reported');
  }
  for (const item of audit.excluded.slice(0, 20)) {
    console.log(`  - ${item.memory_id || '<no-id>'} reason=${item.reason} score=${item.final_score ?? 'n/a'} tokens=${item.token_count ?? 'n/a'}`);
    if (item.snippet) console.log(`    ${item.snippet}`);
  }

  if (audit.warnings.length > 0) {
    console.log('');
    console.log('Warnings');
    for (const warning of audit.warnings) {
      console.log(`  - ${warning}`);
    }
  }
}

function cleanObject(value) {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => {
      if (entry === undefined || entry === null || entry === '') return false;
      if (Array.isArray(entry) && entry.length === 0) return false;
      return true;
    }),
  );
}

function snippet(value) {
  const compact = String(value).replace(/\s+/g, ' ').trim();
  return compact.length > 140 ? `${compact.slice(0, 137)}...` : compact;
}

function formatScope(scope) {
  const entries = Object.entries(scope || {}).filter(([, value]) => value !== null && value !== undefined && value !== '');
  if (entries.length === 0) return '{}';
  return `{${entries.map(([key, value]) => `${key}:${value}`).join(', ')}}`;
}
