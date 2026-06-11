#!/usr/bin/env node

import {
  apiRequest,
  asBoolean,
  asNumber,
  fail,
  parseArgs,
  printJson,
  readJsonFile,
  resolveMemoryOpsConfig,
} from './memoryops-common.mjs';

const USAGE = `
MemoryOps retrieval evaluation harness

Usage:
  node scripts/memoryops-eval.mjs --suite examples/evals/basic-memoryops.eval.json [options]

Options:
  --suite <path>          JSON evaluation suite to run
  --fail-under <ratio>    Exit non-zero if pass ratio is below this value, e.g. 0.8
  --json                  Print machine-readable JSON only
  --api-url <url>         Overrides MEMORYOPS_API_URL
  --workspace-id <uuid>   Overrides MEMORYOPS_WORKSPACE_ID
  --api-key <key>         Overrides MEMORYOPS_API_KEY

Suite shape:
  {
    "name": "MemoryOps smoke eval",
    "defaults": {
      "limit": 5,
      "token_budget": 2048,
      "search_mode": "hybrid",
      "include_trace": true
    },
    "cases": [
      {
        "name": "Tool secret policy",
        "query": "How are tool secrets handled?",
        "expected_contains": ["secret"],
        "must_not_contain": ["BEGIN PRIVATE KEY"]
      }
    ]
  }
`;

const { options } = parseArgs();

if (options.help || options.h) {
  console.log(USAGE);
  process.exit(0);
}

if (!options.suite) {
  fail(USAGE.trim());
}

let suite;
let config;

try {
  suite = readJsonFile(options.suite);
  config = resolveMemoryOpsConfig(options);
} catch (error) {
  fail(error.message);
}

const failUnder = asNumber(options.failUnder, null);
const jsonOnly = asBoolean(options.json, false);

try {
  const result = await runSuite(config, suite);

  if (jsonOnly) {
    printJson(result);
  } else {
    printReport(result);
  }

  if (failUnder !== null && result.pass_ratio < failUnder) {
    fail(`Pass ratio ${formatPercent(result.pass_ratio)} is below --fail-under ${formatPercent(failUnder)}.`);
  }

  process.exit(result.failed === 0 ? 0 : 1);
} catch (error) {
  fail(`Evaluation failed: ${error.message}`);
}

async function runSuite(config, suite) {
  const defaults = suite.defaults || {};
  const cases = Array.isArray(suite.cases) ? suite.cases : [];

  if (cases.length === 0) {
    throw new Error('Suite must include at least one case.');
  }

  const startedAt = new Date();
  const caseResults = [];

  for (const testCase of cases) {
    caseResults.push(await runCase(config, defaults, testCase));
  }

  const passed = caseResults.filter((item) => item.passed).length;
  const failed = caseResults.length - passed;

  return {
    suite: suite.name || options.suite,
    started_at: startedAt.toISOString(),
    finished_at: new Date().toISOString(),
    total: caseResults.length,
    passed,
    failed,
    pass_ratio: caseResults.length === 0 ? 0 : passed / caseResults.length,
    cases: caseResults,
  };
}

async function runCase(config, defaults, testCase) {
  if (!testCase.query) {
    throw new Error(`Case "${testCase.name || '<unnamed>'}" is missing query.`);
  }

  const request = buildRetrieveRequest(config.workspaceId, defaults, testCase);
  const started = Date.now();
  const response = await apiRequest(config, 'POST', '/v1/retrieve', request);
  const elapsedMs = Date.now() - started;
  const memories = normalizeMemories(response);
  const checks = evaluateExpectations(testCase, memories, response);
  const passed = checks.every((check) => check.passed);

  return {
    name: testCase.name || testCase.query,
    query: testCase.query,
    passed,
    elapsed_ms: elapsedMs,
    returned: memories.length,
    query_id: response?.query_id || null,
    total_tokens: response?.total_tokens ?? response?.token_count ?? null,
    checks,
    top_results: memories.slice(0, 5).map((memory, index) => ({
      rank: index + 1,
      id: memory.id || memory.memory_id || null,
      memory_type: memory.memory_type || null,
      score: memory.rrf_score ?? memory.score ?? null,
      token_count: memory.token_count ?? null,
      snippet: snippet(memory.content || ''),
    })),
  };
}

function buildRetrieveRequest(workspaceId, defaults, testCase) {
  const reserved = new Set([
    'name',
    'expected_memory_ids',
    'expected_contains',
    'must_not_contain',
    'min_returned',
    'max_total_tokens',
  ]);

  const request = {
    workspace_id: workspaceId,
    include_trace: true,
    ...defaults,
    query: testCase.query,
  };

  for (const [key, value] of Object.entries(testCase)) {
    if (!reserved.has(key)) {
      request[key] = value;
    }
  }

  return request;
}

function normalizeMemories(response) {
  if (!response) return [];
  if (Array.isArray(response.memories)) return response.memories;
  if (Array.isArray(response.items)) return response.items;
  if (Array.isArray(response.results)) {
    return response.results.map((item) => item.memory || item);
  }
  return [];
}

function evaluateExpectations(testCase, memories, response) {
  const checks = [];
  const joinedContent = memories.map((memory) => memory.content || '').join('\n').toLowerCase();
  const ids = new Set(memories.map((memory) => memory.id || memory.memory_id).filter(Boolean));
  const totalTokens = response?.total_tokens ?? response?.token_count ?? 0;

  const expectedIds = arrayOf(testCase.expected_memory_ids);
  if (expectedIds.length > 0) {
    const missing = expectedIds.filter((id) => !ids.has(id));
    checks.push({
      name: 'expected_memory_ids',
      passed: missing.length === 0,
      detail: missing.length === 0 ? 'all expected memory IDs returned' : `missing IDs: ${missing.join(', ')}`,
    });
  }

  const expectedContains = arrayOf(testCase.expected_contains);
  if (expectedContains.length > 0) {
    const missing = expectedContains.filter((needle) => !joinedContent.includes(String(needle).toLowerCase()));
    checks.push({
      name: 'expected_contains',
      passed: missing.length === 0,
      detail: missing.length === 0 ? 'all expected text fragments found' : `missing fragments: ${missing.join(', ')}`,
    });
  }

  const forbidden = arrayOf(testCase.must_not_contain);
  if (forbidden.length > 0) {
    const found = forbidden.filter((needle) => joinedContent.includes(String(needle).toLowerCase()));
    checks.push({
      name: 'must_not_contain',
      passed: found.length === 0,
      detail: found.length === 0 ? 'no forbidden text fragments found' : `forbidden fragments found: ${found.join(', ')}`,
    });
  }

  if (testCase.min_returned !== undefined) {
    const minReturned = Number(testCase.min_returned);
    checks.push({
      name: 'min_returned',
      passed: memories.length >= minReturned,
      detail: `returned ${memories.length}, expected at least ${minReturned}`,
    });
  }

  if (testCase.max_total_tokens !== undefined) {
    const maxTotalTokens = Number(testCase.max_total_tokens);
    checks.push({
      name: 'max_total_tokens',
      passed: totalTokens <= maxTotalTokens,
      detail: `used ${totalTokens}, expected at most ${maxTotalTokens}`,
    });
  }

  if (checks.length === 0) {
    checks.push({
      name: 'non_empty_result',
      passed: memories.length > 0,
      detail: memories.length > 0 ? 'retrieval returned at least one memory' : 'retrieval returned no memories',
    });
  }

  return checks;
}

function printReport(result) {
  console.log(`MemoryOps eval: ${result.suite}`);
  console.log(`Result: ${result.passed}/${result.total} passed (${formatPercent(result.pass_ratio)})`);
  console.log('');

  for (const item of result.cases) {
    const status = item.passed ? 'PASS' : 'FAIL';
    console.log(`${status} ${item.name}`);
    console.log(`  query: ${item.query}`);
    console.log(`  returned: ${item.returned}, elapsed: ${item.elapsed_ms}ms, tokens: ${item.total_tokens ?? 'n/a'}`);
    for (const check of item.checks) {
      console.log(`  - ${check.passed ? 'ok' : 'x'} ${check.name}: ${check.detail}`);
    }
    if (item.top_results.length > 0) {
      console.log('  top results:');
      for (const top of item.top_results) {
        console.log(`    ${top.rank}. ${top.id || '<no-id>'} ${top.snippet}`);
      }
    }
    console.log('');
  }
}

function arrayOf(value) {
  if (value === undefined || value === null) return [];
  return Array.isArray(value) ? value : [value];
}

function snippet(value) {
  const compact = String(value).replace(/\s+/g, ' ').trim();
  return compact.length > 120 ? `${compact.slice(0, 117)}...` : compact;
}

function formatPercent(value) {
  return `${Math.round(value * 1000) / 10}%`;
}
