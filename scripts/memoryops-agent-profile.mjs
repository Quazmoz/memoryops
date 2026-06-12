#!/usr/bin/env node

import path from 'node:path';
import {
  asBoolean,
  fail,
  parseArgs,
  resolveMemoryOpsConfig,
  splitCsv,
  writeTextFile,
} from './memoryops-common.mjs';

const TARGETS = ['claude-code', 'vscode', 'cursor', 'openwebui', 'gemini', 'generic'];

const USAGE = `
MemoryOps agent install profile generator

Usage:
  node scripts/memoryops-agent-profile.mjs --target <target|all> [options]

Targets:
  ${TARGETS.join(', ')}, all

Options:
  --target <name>              Agent/client target. Use all to generate every profile.
  --agent-id <id>              Agent identifier to use in observations/retrieval. Default: target name.
  --user-id <id>               Optional user scope.
  --repo <owner/name>          Optional repository scope.
  --mcp-url <url>              MCP endpoint. Default: http://localhost:3003
  --token-budget <tokens>      Default retrieval token budget. Default: 4096
  --search-mode <mode>         hybrid, vector, or keyword. Default: hybrid
  --include-workspace-pool     Enable workspace-published memory inheritance.
  --include-master-memory      Enable master/global memory inheritance.
  --allowed-tools a,b,c        Optional allowed MemoryOps tool names.
  --write                      Write profiles to .memoryops/profiles instead of stdout.
  --out-dir <path>             Output directory when --write is set. Default: .memoryops/profiles
  --api-url <url>              Overrides MEMORYOPS_API_URL
  --workspace-id <uuid>        Overrides MEMORYOPS_WORKSPACE_ID

The generated files intentionally do not include API keys. Use environment variables, the VS Code extension secure key command, or your agent's secret store.
`;

const { options } = parseArgs();

if (options.help || options.h) {
  console.log(USAGE);
  process.exit(0);
}

const target = options.target || 'generic';
if (target !== 'all' && !TARGETS.includes(target)) {
  fail(`Unknown target "${target}". Valid targets: ${TARGETS.join(', ')}, all`);
}

let config;
try {
  config = resolveMemoryOpsConfig(options, { requireAuth: false });
} catch (error) {
  fail(error.message);
}

if (!config.workspaceId) {
  fail('Missing workspace ID. Set MEMORYOPS_WORKSPACE_ID, pass --workspace-id, or use .memoryops.local.json.');
}

const selectedTargets = target === 'all' ? TARGETS : [target];
const profiles = selectedTargets.map((item) => buildProfile(item, config, options));

if (asBoolean(options.write, false)) {
  const outDir = options.outDir || '.memoryops/profiles';
  for (const profile of profiles) {
    const filename = `${profile.target}.memoryops.md`;
    const written = writeTextFile(path.join(outDir, filename), profile.content);
    console.log(`Wrote ${written}`);
  }
} else {
  for (const profile of profiles) {
    console.log(profile.content);
    if (profiles.length > 1) console.log('\n---\n');
  }
}

function buildProfile(target, config, options) {
  const agentId = options.agentId || defaultAgentId(target);
  const policy = {
    apiUrl: config.apiUrl,
    workspaceId: config.workspaceId,
    mcpUrl: options.mcpUrl || 'http://localhost:3003',
    agentId,
    userId: options.userId || null,
    repo: options.repo || null,
    tokenBudget: options.tokenBudget || '4096',
    searchMode: options.searchMode || 'hybrid',
    includeWorkspacePool: asBoolean(options.includeWorkspacePool, false),
    includeMasterMemory: asBoolean(options.includeMasterMemory, false),
    allowedTools: splitCsv(options.allowedTools),
  };

  return {
    target,
    content: renderMarkdownProfile(target, policy),
  };
}

function renderMarkdownProfile(target, policy) {
  return `# MemoryOps Agent Install Profile: ${target}

Generated for workspace \`${policy.workspaceId}\`.

## Connection

| Field | Value |
|---|---|
| API URL | \`${policy.apiUrl}\` |
| MCP URL | \`${policy.mcpUrl}\` |
| Workspace ID | \`${policy.workspaceId}\` |
| Agent ID | \`${policy.agentId}\` |
| User ID | ${policy.userId ? `\`${policy.userId}\`` : '_not set_'} |
| Repo | ${policy.repo ? `\`${policy.repo}\`` : '_not set_'} |

API keys are intentionally omitted. Store them in environment variables, the editor secret store, or your agent runtime secret manager.

## Retrieval defaults

\`\`\`json
${JSON.stringify(buildRetrieveDefaults(policy), null, 2)}
\`\`\`

## Agent memory policy

Use MemoryOps as the durable memory control plane for this workspace.

- Retrieve context before non-trivial coding, DevOps, incident, architecture, or documentation work.
- Prefer \`search_mode=${policy.searchMode}\` unless the user asks for exact keyword matching.
- Stay within a default token budget of \`${policy.tokenBudget}\` unless the task explicitly needs broader history.
- Write observations only for durable project facts, architectural decisions, resolved bugs, operational runbooks, important user preferences, and tool behavior that should survive the current session.
- Do not store secrets, plaintext credentials, private keys, access tokens, or unrelated personal content.
- Include scope fields on both retrieval and observation writes: agent ID, user ID when known, and repo when repo-specific.
- Treat workspace-published and master memories as inherited context, not as private user memory.
- When retrieved memories conflict, prefer newer pinned semantic memory, then corroborated semantic memory, then recent episodic memory. Surface the conflict to the user when it affects the action.
${policy.allowedTools.length > 0 ? `- Only invoke these MemoryOps tools unless the user approves otherwise: ${policy.allowedTools.map((tool) => `\`${tool}\``).join(', ')}.\n` : ''}
## ${targetSpecificHeading(target)}

${renderTargetSpecificBlock(target, policy)}
`;
}

function buildRetrieveDefaults(policy) {
  const scope = {};
  if (policy.agentId) scope.agent_id = policy.agentId;
  if (policy.userId) scope.user_id = policy.userId;
  if (policy.repo) scope.repo = policy.repo;

  return {
    workspace_id: policy.workspaceId,
    token_budget: Number(policy.tokenBudget),
    search_mode: policy.searchMode,
    include_trace: true,
    include_workspace_pool: policy.includeWorkspacePool,
    include_master_memory: policy.includeMasterMemory,
    scope,
    agent_id: policy.agentId,
    user_id: policy.userId || undefined,
    repo: policy.repo || undefined,
  };
}

function renderTargetSpecificBlock(target, policy) {
  switch (target) {
    case 'vscode':
      return `Add these settings to VS Code, then store the API key with \`MemoryOps: Set API Key (Secure)\`:

\`\`\`json
${JSON.stringify({
  'memoryops.apiUrl': policy.apiUrl,
  'memoryops.workspaceId': policy.workspaceId,
  'memoryops.defaultAgentId': policy.agentId,
  'memoryops.defaultTokenBudget': Number(policy.tokenBudget),
  'memoryops.defaultSearchMode': policy.searchMode,
  'memoryops.includeWorkspacePool': policy.includeWorkspacePool,
}, null, 2)}
\`\`\``;
    case 'claude-code':
      return `Use MemoryOps through MCP when available and keep the REST settings as fallback environment variables:

\`\`\`bash
export MEMORYOPS_API_URL=${shellQuote(policy.apiUrl)}
export MEMORYOPS_WORKSPACE_ID=${shellQuote(policy.workspaceId)}
export MEMORYOPS_AGENT_ID=${shellQuote(policy.agentId)}
# export MEMORYOPS_API_KEY=... # store securely, do not commit
\`\`\`

MCP endpoint: \`${policy.mcpUrl}\``;
    case 'cursor':
      return `Add this profile text to your Cursor project instructions and expose MemoryOps through MCP or a terminal command:

\`\`\`bash
node scripts/memoryops-scope-audit.mjs "$QUERY" --agent-id ${shellQuote(policy.agentId)}${policy.repo ? ` --repo ${shellQuote(policy.repo)}` : ''}${policy.includeWorkspacePool ? ' --include-workspace-pool' : ''}${policy.includeMasterMemory ? ' --include-master-memory' : ''}
\`\`\``;
    case 'openwebui':
      return `Register the MemoryOps MCP endpoint in Open WebUI's tool/MCP configuration when using MCP-compatible tooling.

Recommended environment:

\`\`\`bash
MEMORYOPS_API_URL=${shellQuote(policy.apiUrl)}
MEMORYOPS_WORKSPACE_ID=${shellQuote(policy.workspaceId)}
MEMORYOPS_AGENT_ID=${shellQuote(policy.agentId)}
MEMORYOPS_MCP_URL=${shellQuote(policy.mcpUrl)}
\`\`\``;
    case 'gemini':
      return `Place this profile in your Gemini project instructions and use the MemoryOps CLI scripts for retrieval checks:

\`\`\`bash
node scripts/memoryops-scope-audit.mjs "<task context>" --agent-id ${shellQuote(policy.agentId)}${policy.repo ? ` --repo ${shellQuote(policy.repo)}` : ''}
node scripts/memoryops-eval.mjs --suite examples/evals/basic-memoryops.eval.json
\`\`\``;
    case 'generic':
    default:
      return `Use the REST API directly or connect via MCP.

Retrieve context:

\`\`\`bash
curl -X POST ${shellQuote(`${policy.apiUrl.replace(/\/$/, '')}/v1/retrieve`)} \\
  -H 'X-API-Key: <secure-key>' \\
  -H 'Content-Type: application/json' \\
  -d ${shellQuote(JSON.stringify(buildRetrieveDefaults(policy)))}
\`\`\``;
  }
}

function targetSpecificHeading(target) {
  switch (target) {
    case 'vscode': return 'VS Code setup';
    case 'claude-code': return 'Claude Code setup';
    case 'cursor': return 'Cursor setup';
    case 'openwebui': return 'Open WebUI setup';
    case 'gemini': return 'Gemini setup';
    default: return 'Generic setup';
  }
}

function defaultAgentId(target) {
  return target === 'all' ? 'memoryops-agent' : target;
}

function shellQuote(value) {
  return `'${String(value).replace(/'/g, `'\\''`)}'`;
}
