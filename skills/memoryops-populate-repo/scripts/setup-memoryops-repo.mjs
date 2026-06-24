#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import readline from 'node:readline/promises';
import { stdin as input, stdout as output } from 'node:process';
import { Writable } from 'node:stream';

const TARGETS = ['claude-code', 'vscode', 'cursor', 'gemini', 'openai', 'generic'];
const SECRET_IGNORES = [
  '.memoryops.local.json',
  '.memoryops/',
  '.mcp.json',
  '.vscode/mcp.json',
  'mcp.json',
  'mcp.*.json',
];

const repoRoot = process.cwd();
const repoName = path.basename(repoRoot);
const pipedAnswers = process.stdin.isTTY ? null : fs.readFileSync(0, 'utf8').split(/\r?\n/);
const rl = pipedAnswers ? null : readline.createInterface({ input, output });

try {
  const answers = await collectAnswers();
  const written = [];

  ensureGitignore(written);
  writeLocalConfig(answers, written);
  writeProfiles(answers, written);
  writeRulesAndAgentAssets(answers, written);
  writeClientConfigs(answers, written);

  if (answers.syncFromServer && answers.apiKey) {
    await syncAgentResources(answers, written);
  } else {
    writeFallbackSkills(answers, written);
  }

  console.log('\nMemoryOps repo setup complete.\n');
  for (const file of written) {
    console.log(`Wrote ${file}`);
  }
  console.log('\nNext steps: restart any MCP-capable clients, then run a small MemoryOps retrieval to verify authentication.');
} finally {
  if (rl) rl.close();
}

async function collectAnswers() {
  const defaultApiUrl = process.env.MEMORYOPS_API_URL || 'http://localhost:8080';
  const apiUrl = normalizeUrl(await ask('MemoryOps API URL', defaultApiUrl));
  const mcpUrl = normalizeMcpUrl(await ask('MemoryOps MCP URL', defaultMcpUrl(apiUrl)));
  const workspaceId = await askRequired('Workspace ID', process.env.MEMORYOPS_WORKSPACE_ID || '');
  const apiKey = await askApiKey();
  const storeApiKey = apiKey ? await askYesNo('Store API key in .memoryops.local.json? This file is gitignored but still plaintext.', false) : false;
  const agentId = normalizeName(await ask('Agent ID', process.env.MEMORYOPS_AGENT_ID || normalizeName(repoName)));
  const userId = await ask('Optional user ID', process.env.MEMORYOPS_USER_ID || '');
  const repo = await ask('Repository scope, for example owner/repo', detectRepoScope() || repoName);
  const targetAnswer = await ask(`Target clients (${TARGETS.join(', ')}, all)`, 'all');
  const targets = parseTargets(targetAnswer);
  const includeWorkspacePool = await askYesNo('Include workspace-published memory by default?', true);
  const includeMasterMemory = await askYesNo('Include master/global memory by default?', false);
  const writeMcpConfigs = await askYesNo('Create local MCP config files for selected clients?', true);
  const embedApiKeyInMcpConfig = writeMcpConfigs && apiKey
    ? await askYesNo('Embed API key in generated MCP config files? These files are gitignored but still plaintext.', false)
    : false;
  const syncFromServer = apiKey ? await askYesNo('Sync skills, prompts, agents, and instructions from MemoryOps Agent Library?', true) : false;

  return {
    apiUrl,
    mcpUrl,
    workspaceId,
    apiKey,
    storeApiKey,
    agentId,
    userId,
    repo,
    targets,
    includeWorkspacePool,
    includeMasterMemory,
    writeMcpConfigs,
    embedApiKeyInMcpConfig,
    syncFromServer,
  };
}

async function askApiKey() {
  if (process.env.MEMORYOPS_API_KEY) {
    console.log('Using MEMORYOPS_API_KEY from the environment for verification and sync. It will not be written unless explicitly approved.');
    return process.env.MEMORYOPS_API_KEY;
  }
  return askSecret('API key (blank to skip verification and server sync)');
}

async function ask(label, defaultValue = '') {
  const suffix = defaultValue ? ` [${defaultValue}]` : '';
  if (pipedAnswers) {
    const value = (pipedAnswers.shift() || '').trim();
    console.log(`${label}${suffix}: ${value}`);
    return value || defaultValue;
  }
  const value = await rl.question(`${label}${suffix}: `);
  return value.trim() || defaultValue;
}

async function askRequired(label, defaultValue = '') {
  while (true) {
    const value = await ask(label, defaultValue);
    if (value) return value;
    console.log(`${label} is required.`);
  }
}

async function askSecret(label) {
  if (pipedAnswers) {
    const value = (pipedAnswers.shift() || '').trim();
    console.log(`${label}: ${value ? '<redacted>' : ''}`);
    return value;
  }

  const mutedOutput = new Writable({
    write(chunk, encoding, callback) {
      if (!mutedOutput.muted) {
        output.write(chunk, encoding);
      }
      callback();
    },
  });
  mutedOutput.muted = false;

  const secretRl = readline.createInterface({ input, output: mutedOutput, terminal: true });
  const question = secretRl.question(`${label}: `);
  mutedOutput.muted = true;
  const value = await question;
  secretRl.close();
  output.write('\n');
  return value.trim();
}

async function askYesNo(label, defaultValue) {
  const defaultText = defaultValue ? 'Y/n' : 'y/N';
  if (pipedAnswers) {
    const answer = (pipedAnswers.shift() || '').trim().toLowerCase();
    console.log(`${label} [${defaultText}]: ${answer}`);
    if (!answer) return defaultValue;
    return ['y', 'yes', 'true', '1'].includes(answer);
  }
  const answer = (await rl.question(`${label} [${defaultText}]: `)).trim().toLowerCase();
  if (!answer) return defaultValue;
  return ['y', 'yes', 'true', '1'].includes(answer);
}

function normalizeUrl(value) {
  const trimmed = String(value).trim().replace(/\/$/, '');
  return /^https?:\/\//i.test(trimmed) ? trimmed : `http://${trimmed}`;
}

function normalizeMcpUrl(value) {
  const url = normalizeUrl(value);
  return url.endsWith('/mcp') ? url : `${url.replace(/\/$/, '')}/mcp`;
}

function defaultMcpUrl(apiUrl) {
  try {
    const parsed = new URL(apiUrl);
    parsed.port = '3003';
    parsed.pathname = '/mcp';
    parsed.search = '';
    parsed.hash = '';
    return parsed.toString().replace(/\/$/, '');
  } catch {
    return 'http://localhost:3003/mcp';
  }
}

function normalizeName(value) {
  return String(value || 'memoryops-agent')
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 64) || 'memoryops-agent';
}

function parseTargets(value) {
  const items = String(value)
    .split(',')
    .map((item) => item.trim().toLowerCase())
    .filter(Boolean);
  if (items.length === 0 || items.includes('all')) return TARGETS;
  const invalid = items.filter((item) => !TARGETS.includes(item));
  if (invalid.length > 0) {
    throw new Error(`Unknown target(s): ${invalid.join(', ')}. Valid targets: ${TARGETS.join(', ')}, all`);
  }
  return items;
}

function detectRepoScope() {
  try {
    const remote = execFileSync('git', ['config', '--get', 'remote.origin.url'], {
      cwd: repoRoot,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
    const ssh = remote.match(/[:/]([^/:]+\/[^/]+?)(?:\.git)?$/);
    return ssh ? ssh[1] : '';
  } catch {
    return '';
  }
}

function ensureGitignore(written) {
  const gitignorePath = path.join(repoRoot, '.gitignore');
  const existing = fs.existsSync(gitignorePath) ? fs.readFileSync(gitignorePath, 'utf8') : '';
  const lines = new Set(existing.split(/\r?\n/).map((line) => line.trim()));
  const missing = SECRET_IGNORES.filter((entry) => !lines.has(entry) && !lines.has(`/${entry}`));
  if (missing.length === 0) return;
  const prefix = existing.endsWith('\n') || existing.length === 0 ? '' : '\n';
  fs.writeFileSync(gitignorePath, `${existing}${prefix}\n# MemoryOps local credentials and private generated config\n${missing.join('\n')}\n`);
  written.push(relative(gitignorePath));
}

function writeLocalConfig(answers, written) {
  const config = {
    api_url: answers.apiUrl,
    workspace_id: answers.workspaceId,
    mcp_url: answers.mcpUrl,
    agent_id: answers.agentId,
    user_id: answers.userId || undefined,
    repo: answers.repo || undefined,
  };
  if (answers.storeApiKey && answers.apiKey) {
    config.api_key = answers.apiKey;
  }
  writeJson('.memoryops.local.json', config, written, { privateFile: true });
}

function writeProfiles(answers, written) {
  for (const target of answers.targets) {
    writeText(`.memoryops/profiles/${target}.memoryops.md`, renderProfile(target, answers), written);
  }
}

function writeRulesAndAgentAssets(answers, written) {
  const rules = renderMemoryRules(answers);
  if (answers.targets.includes('generic')) {
    writeText('agent-library/generic/instructions/memoryops-rules.md', rules, written);
    writeText('agent-library/generic/agents/memoryops-coding-agent.md', renderAgentProfile('generic', answers), written);
  }
  if (answers.targets.includes('openai')) {
    writeText('agent-library/openai/instructions/memoryops-rules.md', rules, written);
    writeText('agent-library/openai/agents/memoryops-coding-agent.md', renderAgentProfile('openai', answers), written);
  }
  if (answers.targets.includes('cursor')) {
    writeText('.cursor/rules/memoryops.mdc', renderCursorRule(answers), written);
  }
}

function writeClientConfigs(answers, written) {
  if (!answers.writeMcpConfigs) return;
  const authHeader = answers.embedApiKeyInMcpConfig ? `Bearer ${answers.apiKey}` : 'Bearer YOUR_MEMORYOPS_API_KEY';
  if (answers.targets.includes('claude-code')) {
    writeJson('.mcp.json', {
      mcpServers: {
        memoryops: {
          type: 'http',
          url: answers.mcpUrl,
          headers: {
            Authorization: authHeader,
          },
        },
      },
    }, written, { privateFile: true });
  }
  if (answers.targets.includes('vscode')) {
    writeJson('.vscode/mcp.json', {
      servers: {
        memoryops: {
          type: 'http',
          url: answers.mcpUrl,
          headers: {
            Authorization: authHeader,
          },
        },
      },
    }, written, { privateFile: true });
  }
}

async function syncAgentResources(answers, written) {
  try {
    const resources = await requestJson(answers, '/v1/agent-resources');
    if (!Array.isArray(resources)) {
      throw new Error('Agent resources response was not an array.');
    }

    let synced = 0;
    for (const resource of resources) {
      if (!resource || !resource.kind || !resource.assistant || !resource.name) continue;
      const detail = await requestJson(
        answers,
        `/v1/agent-resources/${encodeURIComponent(resource.kind)}/${encodeURIComponent(resource.assistant)}/${encodeURIComponent(resource.name)}`,
      );
      const content = detail.content || detail.body || resource.content || resource.body;
      if (!content) continue;
      const outputPath = pathForResource(resource);
      if (!outputPath) continue;
      writeText(outputPath, content, written);
      synced += 1;
    }

    if (synced === 0) {
      console.log('No server-managed agent resources were written; creating fallback local skills.');
      writeFallbackSkills(answers, written);
    }
  } catch (error) {
    console.log(`Agent Library sync failed: ${error.message}`);
    console.log('Creating fallback local skills instead.');
    writeFallbackSkills(answers, written);
  }
}

async function requestJson(answers, endpoint) {
  const response = await fetch(`${answers.apiUrl}${endpoint}`, {
    headers: {
      Accept: 'application/json',
      'X-API-Key': answers.apiKey,
    },
  });
  const text = await response.text();
  if (!response.ok) {
    throw new Error(`GET ${endpoint} returned HTTP ${response.status}: ${text}`);
  }
  return text ? JSON.parse(text) : null;
}

function pathForResource(resource) {
  const safeName = normalizeName(resource.name);
  const assistant = normalizeName(resource.assistant);
  switch (resource.kind) {
    case 'skill':
      if (assistant === 'claude') return `.claude/skills/${safeName}.md`;
      if (assistant === 'gemini') return `.gemini/skills/${safeName}.md`;
      return null;
    case 'prompt':
      return `agent-library/${assistant}/prompts/${safeName}.md`;
    case 'agent':
      return `agent-library/${assistant}/agents/${safeName}.md`;
    case 'instruction':
      return `agent-library/${assistant}/instructions/${safeName}.md`;
    default:
      return null;
  }
}

function writeFallbackSkills(answers, written) {
  if (answers.targets.includes('claude-code')) {
    writeText('.claude/skills/use_memoryops.md', renderUseMemoryOpsSkill('Claude Code', answers), written);
  }
  if (answers.targets.includes('gemini')) {
    writeText('.gemini/skills/use_memoryops.md', renderUseMemoryOpsSkill('Gemini', answers), written);
  }
}

function renderProfile(target, answers) {
  return `# MemoryOps Agent Install Profile: ${target}

Generated for workspace \`${answers.workspaceId}\`.

## Connection

| Field | Value |
|---|---|
| API URL | \`${answers.apiUrl}\` |
| MCP URL | \`${answers.mcpUrl}\` |
| Workspace ID | \`${answers.workspaceId}\` |
| Agent ID | \`${answers.agentId}\` |
| User ID | ${answers.userId ? `\`${answers.userId}\`` : '_not set_'} |
| Repo | ${answers.repo ? `\`${answers.repo}\`` : '_not set_'} |

API keys are intentionally local. Use \`MEMORYOPS_API_KEY\`, a secret store, or the generated gitignored MCP config.

## Retrieval Defaults

\`\`\`json
${JSON.stringify(retrieveDefaults(answers), null, 2)}
\`\`\`

${renderMemoryRules(answers)}
`;
}

function retrieveDefaults(answers) {
  return {
    workspace_id: answers.workspaceId,
    token_budget: 4096,
    search_mode: 'hybrid',
    include_trace: true,
    include_workspace_pool: answers.includeWorkspacePool,
    include_master_memory: answers.includeMasterMemory,
    agent_id: answers.agentId,
    user_id: answers.userId || undefined,
    repo: answers.repo || undefined,
  };
}

function renderMemoryRules(answers) {
  return `# MemoryOps Rules

- Retrieve MemoryOps context before non-trivial coding, DevOps, incident, architecture, migration, or release work.
- Use MCP tools when available; use REST against \`${answers.apiUrl}\` as a fallback.
- Use \`${answers.agentId}\` as the default \`agent_id\`${answers.repo ? ` and \`${answers.repo}\` as the default repo scope` : ''}.
- Store durable outcomes: architectural decisions, root causes, migration notes, stable preferences, and reusable workflow rules.
- Use observations for raw logs, partial evidence, or notes that should be classified asynchronously.
- Never store secrets, plaintext credentials, private keys, access tokens, unrelated personal content, or private reasoning.
- Treat retrieved memory as evidence. When memories conflict, surface the conflict before acting.
- Submit feedback on retrieved memories when the result was clearly useful or clearly irrelevant.
`;
}

function renderAgentProfile(target, answers) {
  return `# MemoryOps Coding Agent (${target})

## Role

Act as a repo-aware coding agent that uses MemoryOps as durable project memory.

## Connection

- API URL: \`${answers.apiUrl}\`
- MCP URL: \`${answers.mcpUrl}\`
- Workspace ID: \`${answers.workspaceId}\`
- Agent ID: \`${answers.agentId}\`

## Operating Rules

${renderMemoryRules(answers)}
`;
}

function renderCursorRule(answers) {
  return `---
description: Use MemoryOps for durable repo context
alwaysApply: true
---

${renderMemoryRules(answers)}

MemoryOps connection:

- API URL: ${answers.apiUrl}
- MCP URL: ${answers.mcpUrl}
- Workspace ID: ${answers.workspaceId}
- Agent ID: ${answers.agentId}
`;
}

function renderUseMemoryOpsSkill(clientName, answers) {
  return `# Use MemoryOps

Use this skill when working in this repository and durable project memory would help.

## Connection

- API URL: \`${answers.apiUrl}\`
- MCP URL: \`${answers.mcpUrl}\`
- Workspace ID: \`${answers.workspaceId}\`
- Agent ID: \`${answers.agentId}\`

## Workflow

1. Retrieve MemoryOps context before substantial code, DevOps, incident, architecture, migration, or release work.
2. Prefer the MemoryOps MCP server when ${clientName} exposes it. Fall back to REST with \`X-API-Key\` when MCP is unavailable.
3. Store only durable project facts and outcomes. Use observations for raw evidence.
4. Never store secrets, credentials, private keys, tokens, unrelated personal content, or private reasoning.
5. Surface conflicting memories before acting on them.

## Retrieval Defaults

\`\`\`json
${JSON.stringify(retrieveDefaults(answers), null, 2)}
\`\`\`
`;
}

function writeJson(filePath, value, written, options = {}) {
  writeText(filePath, `${JSON.stringify(dropUndefined(value), null, 2)}\n`, written, options);
}

function writeText(filePath, content, written, options = {}) {
  const absolute = path.join(repoRoot, filePath);
  fs.mkdirSync(path.dirname(absolute), { recursive: true });
  fs.writeFileSync(absolute, content.endsWith('\n') ? content : `${content}\n`, {
    encoding: 'utf8',
    mode: options.privateFile ? 0o600 : 0o644,
  });
  if (options.privateFile) {
    try {
      fs.chmodSync(absolute, 0o600);
    } catch {
      // Best effort: Windows ACLs may not map cleanly to POSIX-style modes.
    }
  }
  written.push(relative(absolute));
}

function dropUndefined(value) {
  return JSON.parse(JSON.stringify(value));
}

function relative(absolute) {
  return path.relative(repoRoot, absolute).replace(/\\/g, '/');
}
