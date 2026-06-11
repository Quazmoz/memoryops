#!/usr/bin/env node

import {
  apiRequest,
  asBoolean,
  asNumber,
  fail,
  parseArgs,
  printJson,
  resolveMemoryOpsConfig,
} from './memoryops-common.mjs';

const USAGE = `
MemoryOps workspace health report

Usage:
  node scripts/memoryops-health-report.mjs [options]

Options:
  --days <n>              Stats history window. Default: 30
  --json                  Print machine-readable JSON
  --fail-under <score>    Exit non-zero if health score is below this value, e.g. 80
  --api-url <url>         Overrides MEMORYOPS_API_URL
  --workspace-id <uuid>   Overrides MEMORYOPS_WORKSPACE_ID
  --api-key <key>         Overrides MEMORYOPS_API_KEY

The report checks API readiness, system health, workspace stats, stats history,
integrations, DLQ jobs, contradiction counts, tags, and basic retrieval smoke.
`;

const { options } = parseArgs();

if (options.help || options.h) {
  console.log(USAGE);
  process.exit(0);
}

let config;
try {
  config = resolveMemoryOpsConfig(options);
} catch (error) {
  fail(error.message);
}

try {
  const days = asNumber(options.days, 30);
  const report = await buildReport(config, days);

  if (asBoolean(options.json, false)) {
    printJson(report);
  } else {
    printReport(report);
  }

  const failUnder = asNumber(options.failUnder, null);
  if (failUnder !== null && report.score < failUnder) {
    fail(`Health score ${report.score} is below --fail-under ${failUnder}.`);
  }

  process.exit(report.status === 'critical' ? 1 : 0);
} catch (error) {
  fail(`Health report failed: ${error.message}`);
}

async function buildReport(config, days) {
  const checks = await Promise.all([
    safeRequest(config, 'readiness', 'GET', '/health/ready'),
    safeRequest(config, 'system_health', 'GET', '/health/system'),
    safeRequest(config, 'workspace', 'GET', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}`),
    safeRequest(config, 'stats', 'GET', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/stats`),
    safeRequest(config, 'stats_history', 'GET', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/stats/history?days=${encodeURIComponent(days)}`),
    safeRequest(config, 'integrations', 'GET', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/integrations`),
    safeRequest(config, 'dlq', 'GET', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/dlq`),
    safeRequest(config, 'contradiction_count', 'GET', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/contradictions/count`),
    safeRequest(config, 'tags', 'GET', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/tags?limit=20`),
  ]);

  const byName = Object.fromEntries(checks.map((check) => [check.name, check]));
  const smoke = await retrievalSmoke(config);
  const findings = evaluateFindings(byName, smoke);
  const score = calculateScore(findings);

  return {
    workspace_id: config.workspaceId,
    generated_at: new Date().toISOString(),
    status: statusFromScore(score, findings),
    score,
    summary: buildSummary(byName, smoke),
    findings,
    checks: byName,
    retrieval_smoke: smoke,
  };
}

async function safeRequest(config, name, method, endpoint) {
  try {
    const data = await apiRequest(config, method, endpoint);
    return { name, ok: true, data };
  } catch (error) {
    return { name, ok: false, error: error.message };
  }
}

async function retrievalSmoke(config) {
  try {
    const data = await apiRequest(config, 'POST', '/v1/retrieve', {
      workspace_id: config.workspaceId,
      query: 'MemoryOps workspace health retrieval smoke test',
      limit: 3,
      token_budget: 1024,
      search_mode: 'hybrid',
      include_trace: true,
      include_workspace_pool: true,
      include_master_memory: true,
    });
    const count = Array.isArray(data?.memories) ? data.memories.length : Array.isArray(data?.items) ? data.items.length : 0;
    return { ok: true, returned: count, query_id: data?.query_id || null, elapsed_ms: data?.elapsed_ms ?? null, token_count: data?.total_tokens ?? data?.token_count ?? null };
  } catch (error) {
    return { ok: false, error: error.message };
  }
}

function evaluateFindings(checks, smoke) {
  const findings = [];

  if (!checks.readiness.ok) {
    findings.push(critical('api_readiness_failed', 'API readiness endpoint failed.', checks.readiness.error));
  }

  if (!checks.system_health.ok) {
    findings.push(warning('system_health_unavailable', 'System health endpoint was unavailable.', checks.system_health.error));
  }

  const stats = checks.stats.data || {};
  if (!checks.stats.ok) {
    findings.push(critical('workspace_stats_failed', 'Workspace stats could not be loaded.', checks.stats.error));
  } else {
    const total = Number(stats.total_memories || 0);
    const semantic = Number(stats.semantic_count || 0);
    const deleted = Number(stats.deleted_count || 0);
    const pinned = Number(stats.pinned_count || 0);

    if (total === 0) {
      findings.push(warning('no_memories', 'Workspace has no memories yet.', 'Seed or import memory before connecting agents.'));
    }

    if (total > 20 && semantic / total < 0.1) {
      findings.push(warning('low_semantic_ratio', 'Semantic memory ratio is low.', `${semantic}/${total} memories are semantic.`));
    }

    if (total > 0 && deleted / Math.max(total + deleted, 1) > 0.25) {
      findings.push(info('high_deleted_ratio', 'Deleted memory ratio is elevated.', `${deleted} deleted memories reported.`));
    }

    if (total > 20 && pinned === 0) {
      findings.push(info('no_pinned_memories', 'No pinned memories found.', 'Pin stable project rules, runbooks, and architectural decisions.'));
    }
  }

  const integrations = Array.isArray(checks.integrations.data) ? checks.integrations.data : [];
  if (!checks.integrations.ok) {
    findings.push(warning('integrations_unavailable', 'Integration status could not be loaded.', checks.integrations.error));
  } else {
    for (const integration of integrations) {
      if (integration.status && !['active', 'ok', 'healthy'].includes(String(integration.status).toLowerCase())) {
        findings.push(warning('integration_degraded', `Integration ${integration.source} is ${integration.status}.`, `${integration.errors_24h || 0} errors in 24h.`));
      }
    }
  }

  const dlqItems = normalizeArrayPayload(checks.dlq.data, ['items', 'jobs', 'dlq']);
  if (!checks.dlq.ok) {
    findings.push(warning('dlq_unavailable', 'DLQ could not be loaded.', checks.dlq.error));
  } else if (dlqItems.length > 0) {
    findings.push(warning('dlq_not_empty', 'Dead-letter queue is not empty.', `${dlqItems.length} failed jobs require review.`));
  }

  const contradictionCount = extractCount(checks.contradiction_count.data);
  if (!checks.contradiction_count.ok) {
    findings.push(info('contradiction_count_unavailable', 'Contradiction count could not be loaded.', checks.contradiction_count.error));
  } else if (contradictionCount > 0) {
    findings.push(warning('open_contradictions', 'Open contradictions require review.', `${contradictionCount} unresolved contradiction flags.`));
  }

  if (!smoke.ok) {
    findings.push(critical('retrieval_smoke_failed', 'Retrieval smoke test failed.', smoke.error));
  }

  return findings;
}

function buildSummary(checks, smoke) {
  const stats = checks.stats.data || {};
  const integrations = Array.isArray(checks.integrations.data) ? checks.integrations.data : [];
  const dlqItems = normalizeArrayPayload(checks.dlq.data, ['items', 'jobs', 'dlq']);
  const tags = normalizeArrayPayload(checks.tags.data, ['tags', 'items']);

  return {
    total_memories: stats.total_memories ?? null,
    semantic_count: stats.semantic_count ?? null,
    episodic_count: stats.episodic_count ?? null,
    pinned_count: stats.pinned_count ?? null,
    memories_created_7d: stats.memories_created_7d ?? null,
    memories_created_30d: stats.memories_created_30d ?? null,
    integrations: integrations.length,
    dlq_jobs: dlqItems.length,
    contradiction_count: extractCount(checks.contradiction_count.data),
    top_tags: tags.slice(0, 10),
    retrieval_smoke_returned: smoke.returned ?? null,
  };
}

function calculateScore(findings) {
  const penalty = findings.reduce((sum, finding) => {
    if (finding.severity === 'critical') return sum + 35;
    if (finding.severity === 'warning') return sum + 12;
    return sum + 4;
  }, 0);
  return Math.max(0, 100 - penalty);
}

function statusFromScore(score, findings) {
  if (findings.some((finding) => finding.severity === 'critical')) return 'critical';
  if (score >= 90) return 'healthy';
  if (score >= 70) return 'degraded';
  return 'attention_required';
}

function printReport(report) {
  console.log(`MemoryOps health report for ${report.workspace_id}`);
  console.log(`Status: ${report.status} | Score: ${report.score}/100 | Generated: ${report.generated_at}`);
  console.log('');
  console.log('Summary');
  for (const [key, value] of Object.entries(report.summary)) {
    if (key === 'top_tags') continue;
    console.log(`  ${key}: ${formatValue(value)}`);
  }
  if (Array.isArray(report.summary.top_tags) && report.summary.top_tags.length > 0) {
    console.log(`  top_tags: ${report.summary.top_tags.map((tag) => tag.name || tag.tag || String(tag)).join(', ')}`);
  }
  console.log('');
  console.log('Findings');
  if (report.findings.length === 0) {
    console.log('  none');
  }
  for (const finding of report.findings) {
    console.log(`  - [${finding.severity}] ${finding.code}: ${finding.message}`);
    if (finding.detail) console.log(`    ${finding.detail}`);
  }
}

function critical(code, message, detail) {
  return { severity: 'critical', code, message, detail };
}

function warning(code, message, detail) {
  return { severity: 'warning', code, message, detail };
}

function info(code, message, detail) {
  return { severity: 'info', code, message, detail };
}

function normalizeArrayPayload(payload, keys) {
  if (Array.isArray(payload)) return payload;
  if (!payload || typeof payload !== 'object') return [];
  for (const key of keys) {
    if (Array.isArray(payload[key])) return payload[key];
  }
  return [];
}

function extractCount(payload) {
  if (typeof payload === 'number') return payload;
  if (!payload || typeof payload !== 'object') return null;
  for (const key of ['count', 'total', 'open', 'unresolved']) {
    if (typeof payload[key] === 'number') return payload[key];
  }
  return null;
}

function formatValue(value) {
  if (value === null || value === undefined) return 'n/a';
  if (Array.isArray(value)) return String(value.length);
  return String(value);
}
