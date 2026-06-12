#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import path from 'node:path';
import {
  asBoolean,
  fail,
  parseArgs,
  printJson,
} from './memoryops-common.mjs';

const USAGE = `
MemoryOps release gate runner

Usage:
  node scripts/memoryops-release-gate.mjs [options]

Options:
  --eval-suite <path>       Eval suite path. Default: examples/evals/basic-memoryops.eval.json
  --health-fail-under <n>   Health score threshold. Default: 80
  --eval-fail-under <n>     Eval pass ratio threshold. Default: 0.8
  --scope-query <query>     Optional scope audit query
  --agent-id <id>           Optional scope audit agent ID
  --user-id <id>            Optional scope audit user ID
  --repo <owner/name>       Optional scope audit repo
  --tool-pack <path>        Optional tool pack to validate
  --skip-health             Skip workspace health report
  --skip-eval               Skip retrieval eval suite
  --skip-snapshot           Skip workspace snapshot
  --json                    Print machine-readable summary
  --api-url <url>           Forwarded to child utilities
  --workspace-id <uuid>     Forwarded to child utilities
  --api-key <key>           Forwarded to child utilities through environment, never command args

Examples:
  node scripts/memoryops-release-gate.mjs
  node scripts/memoryops-release-gate.mjs --scope-query "tool secret handling" --agent-id vscode --repo Quazmoz/memoryops
  node scripts/memoryops-release-gate.mjs --tool-pack examples/tool-packs/http-smoke.toolpack.json --json

This utility orchestrates existing MemoryOps utilities and returns non-zero when any enabled gate fails.
`;

const { options } = parseArgs();

if (options.help || options.h) {
  console.log(USAGE);
  process.exit(0);
}

const gates = [];

if (!asBoolean(options.skipHealth, false)) {
  gates.push({
    name: 'health',
    command: 'memoryops-health-report.mjs',
    args: ['--fail-under', String(options.healthFailUnder || '80')],
  });
}

if (!asBoolean(options.skipEval, false)) {
  gates.push({
    name: 'eval',
    command: 'memoryops-eval.mjs',
    args: [
      '--suite',
      options.evalSuite || 'examples/evals/basic-memoryops.eval.json',
      '--fail-under',
      String(options.evalFailUnder || '0.8'),
    ],
  });
}

if (options.scopeQuery) {
  const scopeArgs = [options.scopeQuery];
  appendOption(scopeArgs, '--agent-id', options.agentId);
  appendOption(scopeArgs, '--user-id', options.userId);
  appendOption(scopeArgs, '--repo', options.repo);
  scopeArgs.push('--include-workspace-pool', '--include-master-memory');
  gates.push({
    name: 'scope-audit',
    command: 'memoryops-scope-audit.mjs',
    args: scopeArgs,
  });
}

if (options.toolPack) {
  gates.push({
    name: 'tool-pack-validate',
    command: 'memoryops-tool-pack.mjs',
    args: ['validate', '--file', options.toolPack],
  });
}

if (!asBoolean(options.skipSnapshot, false)) {
  gates.push({
    name: 'snapshot',
    command: 'memoryops-snapshot.mjs',
    args: [],
  });
}

if (gates.length === 0) {
  fail('No gates selected. Remove skip flags or provide scope/tool-pack options.');
}

const results = gates.map((gate) => runGate(gate, options));
const summary = {
  generated_at: new Date().toISOString(),
  passed: results.filter((result) => result.ok).length,
  failed: results.filter((result) => !result.ok).length,
  gates: results,
};

if (asBoolean(options.json, false)) {
  printJson(summary);
} else {
  printSummary(summary);
}

process.exit(summary.failed === 0 ? 0 : 1);

function runGate(gate, options) {
  const scriptPath = path.join('scripts', gate.command);
  const args = [scriptPath, ...gate.args, ...forwardedOptions(options)];
  const started = Date.now();
  const result = spawnSync(process.execPath, args, {
    cwd: process.cwd(),
    encoding: 'utf8',
    env: childEnv(options),
  });

  return {
    name: gate.name,
    ok: result.status === 0,
    exit_code: result.status,
    elapsed_ms: Date.now() - started,
    command: `node ${args.map(shellDisplay).join(' ')}`,
    stdout: trimOutput(result.stdout),
    stderr: trimOutput(result.stderr),
    error: result.error ? result.error.message : null,
  };
}

function forwardedOptions(options) {
  const args = [];
  appendOption(args, '--api-url', options.apiUrl);
  appendOption(args, '--workspace-id', options.workspaceId);
  return args;
}

function childEnv(options) {
  const env = { ...process.env };
  if (options.apiKey) env.MEMORYOPS_API_KEY = String(options.apiKey);
  if (options.apiUrl) env.MEMORYOPS_API_URL = String(options.apiUrl);
  if (options.workspaceId) env.MEMORYOPS_WORKSPACE_ID = String(options.workspaceId);
  return env;
}

function appendOption(args, flag, value) {
  if (value !== undefined && value !== null && value !== '') {
    args.push(flag, String(value));
  }
}

function printSummary(summary) {
  console.log(`MemoryOps release gate: ${summary.passed}/${summary.gates.length} passed`);
  console.log(`Generated: ${summary.generated_at}`);
  console.log('');

  for (const gate of summary.gates) {
    console.log(`${gate.ok ? 'PASS' : 'FAIL'} ${gate.name} (${gate.elapsed_ms}ms)`);
    console.log(`  ${gate.command}`);
    if (!gate.ok) {
      if (gate.stderr) console.log(indent('stderr', gate.stderr));
      if (gate.stdout) console.log(indent('stdout', gate.stdout));
      if (gate.error) console.log(`  error: ${gate.error}`);
    }
  }
}

function trimOutput(value) {
  const text = String(value || '').trim();
  if (text.length <= 2000) return text;
  return `${text.slice(0, 2000)}\n... <truncated>`;
}

function indent(label, value) {
  return `  ${label}:\n${String(value).split('\n').map((line) => `    ${line}`).join('\n')}`;
}

function shellDisplay(value) {
  const text = String(value);
  return /\s/.test(text) ? JSON.stringify(text) : text;
}
