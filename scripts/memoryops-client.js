#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

// Resolve configuration from environment or local config file
let apiKey = process.env.MEMORYOPS_API_KEY;
let workspaceId = process.env.MEMORYOPS_WORKSPACE_ID;
let apiUrl = process.env.MEMORYOPS_API_URL || 'http://localhost:8080';

function loadLocalCredentials() {
  let dir = process.cwd();
  while (true) {
    const credPath = path.join(dir, '.memoryops.local.json');
    if (fs.existsSync(credPath)) {
      try {
        const content = JSON.parse(fs.readFileSync(credPath, 'utf8'));
        return {
          apiKey: content.api_key || content.apiKey,
          workspaceId: content.workspace_id || content.workspaceId,
          apiUrl: content.api_url || content.apiUrl
        };
      } catch (err) {
        // Ignore JSON parse errors and continue looking upward
      }
    }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

// Fallback to local configuration file if env vars are not set
if (!apiKey || !workspaceId || !process.env.MEMORYOPS_API_URL) {
  const creds = loadLocalCredentials();
  if (creds) {
    apiKey = apiKey || creds.apiKey;
    workspaceId = workspaceId || creds.workspaceId;
    apiUrl = process.env.MEMORYOPS_API_URL || creds.apiUrl || apiUrl;
  }
}

function printUsage() {
  console.log(`
MemoryOps Agent CLI Client

Usage:
  node memoryops-client.js <command> [arguments]

Commands:
  retrieve "<query>"       Retrieve relevant memories and tools matching query
  context "<query>"        Export token-packed memory context for non-MCP agents
  store "<content>" [tags] Directly persist an episodic memory
  observe "<content>" [tags] Submit a raw observation to the classification queue
  tools                   List all registered workspace tools (alias: skills)
  sync-skills             Sync remote agent skills to local directories (.gemini/skills, .claude/skills)
  help                    Show this help message

Context options:
  --out <file>             Write context to a file, for example .memoryops/context.md
  --format <markdown|json> Output format for context (default: markdown)
  --token-budget <tokens>  Retrieval packing budget (default: server config)
  --agent-id <id>          Scope retrieval to an agent id
  --user-id <id>           Scope retrieval to a user id
  --repo <owner/name>      Scope retrieval to a repository
  --workspace-pool         Include shared workspace pool memories
  --no-master-memory       Exclude master memory inheritance
  --include-trace          Include retrieval trace when using --format json

Environment Variables:
  MEMORYOPS_API_KEY        API key for authentication (for example, <memoryops-api-key>)
  MEMORYOPS_WORKSPACE_ID   Target Workspace UUID
  MEMORYOPS_API_URL        Endpoint URL of MemoryOps API (default: http://localhost:8080)
`);
}

const args = process.argv.slice(2);
const command = args[0];

if (!command || command === 'help' || command === '--help' || command === '-h') {
  printUsage();
  process.exit(0);
}

if (!apiKey) {
  console.error("Error: MEMORYOPS_API_KEY is not set.");
  console.error("Provide it in the environment or run the script in a directory containing '.memoryops.local.json'.");
  process.exit(1);
}

if (!workspaceId) {
  console.error("Error: MEMORYOPS_WORKSPACE_ID is not set.");
  console.error("Provide it in the environment or run the script in a directory containing '.memoryops.local.json'.");
  process.exit(1);
}

async function apiRequest(method, endpoint, body = null) {
  const url = `${apiUrl.replace(/\/$/, '')}${endpoint}`;
  const headers = {
    'X-API-Key': apiKey,
    'Content-Type': 'application/json'
  };
  const options = { method, headers };
  if (body) {
    options.body = JSON.stringify(body);
  }

  const response = await fetch(url, options);
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`HTTP ${response.status} - ${text}`);
  }

  if (response.status === 204) {
    return null;
  }
  return response.json();
}

function parseFlags(rawArgs) {
  const positional = [];
  const flags = {};

  for (let index = 0; index < rawArgs.length; index += 1) {
    const value = rawArgs[index];
    if (!value.startsWith('--')) {
      positional.push(value);
      continue;
    }

    const name = value.slice(2);
    if ([
      'workspace-pool',
      'no-master-memory',
      'include-trace'
    ].includes(name)) {
      flags[name] = true;
      continue;
    }

    const next = rawArgs[index + 1];
    if (!next || next.startsWith('--')) {
      throw new Error(`Flag --${name} requires a value.`);
    }
    flags[name] = next;
    index += 1;
  }

  return { positional, flags };
}

function formatContextMarkdown(response, query, options = {}) {
  const memories = Array.isArray(response.memories) ? response.memories : [];
  const lines = [
    '# MemoryOps Context',
    '',
    `Query: ${query}`,
    `Workspace: ${workspaceId}`,
    `Generated: ${new Date().toISOString()}`,
    `Total tokens: ${response.total_tokens ?? 'unknown'}`,
    ''
  ];

  if (options.agentId || options.userId || options.repo) {
    lines.push('## Scope', '');
    if (options.agentId) lines.push(`- Agent: ${options.agentId}`);
    if (options.userId) lines.push(`- User: ${options.userId}`);
    if (options.repo) lines.push(`- Repo: ${options.repo}`);
    lines.push('');
  }

  lines.push('## Retrieved Memories', '');
  if (memories.length === 0) {
    lines.push('No relevant memories were retrieved.', '');
  }

  memories.forEach((memory, index) => {
    const type = memory.memory_type || 'memory';
    const importance = memory.importance_score ?? 'n/a';
    const relevance = memory.relevance_score ?? 'n/a';
    lines.push(`### ${index + 1}. ${type}`);
    lines.push('');
    lines.push(`- Memory ID: ${memory.id || 'unknown'}`);
    lines.push(`- Importance: ${importance}`);
    lines.push(`- Relevance: ${relevance}`);
    if (Array.isArray(memory.entities) && memory.entities.length > 0) {
      const entities = memory.entities
        .map((entity) => entity.name || entity.value || entity.id)
        .filter(Boolean)
        .join(', ');
      if (entities) lines.push(`- Entities: ${entities}`);
    }
    lines.push('', '```text', memory.content || '', '```', '');
  });

  lines.push('## Agent Instructions', '');
  lines.push('- Treat this file as retrieved context, not as source code to edit.');
  lines.push('- Prefer current repository files over stale memories when they conflict.');
  lines.push('- Store only durable follow-up decisions back into MemoryOps; never store secrets.');
  lines.push('');

  return lines.join('\n');
}

function writeOutput(outputPath, content) {
  const fullPath = path.resolve(process.cwd(), outputPath);
  fs.mkdirSync(path.dirname(fullPath), { recursive: true });
  fs.writeFileSync(fullPath, content, 'utf8');
  console.log(`Wrote MemoryOps context to ${path.relative(process.cwd(), fullPath) || fullPath}`);
}

async function run() {
  try {
    switch (command) {
      case 'retrieve': {
        const query = args[1];
        if (!query) {
          console.error("Error: Please provide a search query.");
          process.exit(1);
        }
        const endpoint = `/v1/memory?workspace_id=${workspaceId}&query=${encodeURIComponent(query)}`;
        const res = await apiRequest('GET', endpoint);
        console.log(JSON.stringify(res, null, 2));
        break;
      }
      case 'context': {
        const { positional, flags } = parseFlags(args.slice(1));
        const query = positional[0];
        if (!query) {
          console.error("Error: Please provide a context query.");
          process.exit(1);
        }

        const tokenBudget = flags['token-budget'] ? Number.parseInt(flags['token-budget'], 10) : undefined;
        if (flags['token-budget'] && (!Number.isInteger(tokenBudget) || tokenBudget <= 0)) {
          console.error('Error: --token-budget must be a positive integer.');
          process.exit(1);
        }

        const payload = {
          query,
          workspace_id: workspaceId,
          token_budget: tokenBudget,
          agent_id: flags['agent-id'],
          user_id: flags['user-id'],
          repo: flags.repo,
          include_trace: Boolean(flags['include-trace']),
          include_workspace_pool: Boolean(flags['workspace-pool']),
          include_master_memory: !flags['no-master-memory']
        };

        Object.keys(payload).forEach((key) => {
          if (payload[key] === undefined) delete payload[key];
        });

        const res = await apiRequest('POST', '/v1/retrieve', payload);
        const format = flags.format || 'markdown';
        const output = format === 'json'
          ? `${JSON.stringify(res, null, 2)}\n`
          : formatContextMarkdown(res, query, {
              agentId: flags['agent-id'],
              userId: flags['user-id'],
              repo: flags.repo
            });

        if (!['markdown', 'json'].includes(format)) {
          console.error('Error: --format must be markdown or json.');
          process.exit(1);
        }

        if (flags.out) {
          writeOutput(flags.out, output);
        } else {
          console.log(output);
        }
        break;
      }
      case 'store': {
        const content = args[1];
        if (!content) {
          console.error("Error: Please provide the memory content.");
          process.exit(1);
        }
        const tags = args.slice(2);
        const payload = {
          workspace_id: workspaceId,
          memory_type: 'episodic',
          content: content,
          importance_score: 0.8,
          tags: tags,
          metadata: { occurred_at: new Date().toISOString() }
        };
        const res = await apiRequest('POST', '/v1/memory', payload);
        console.log(JSON.stringify(res, null, 2));
        break;
      }
      case 'observe': {
        const content = args[1];
        if (!content) {
          console.error("Error: Please provide the observation content.");
          process.exit(1);
        }
        const tags = args.slice(2);
        const payload = {
          content: content,
          agent_id: 'external-agent-cli',
          tags: tags.length > 0 ? tags : undefined
        };
        const res = await apiRequest('POST', '/v1/ingest/observation', payload);
        console.log(JSON.stringify(res, null, 2));
        break;
      }
      case 'tools':
      case 'skills': {
        const endpoint = `/v1/workspaces/${workspaceId}/tools`;
        const res = await apiRequest('GET', endpoint);
        console.log(JSON.stringify(res, null, 2));
        break;
      }
      case 'sync-skills': {
        console.log("Fetching agent skills from server...");
        const skills = await apiRequest('GET', '/v1/agent-skills');
        if (!Array.isArray(skills)) {
          console.error("Error: Server did not return an array of skills.");
          process.exit(1);
        }
        console.log(`Found ${skills.length} skills on server. Syncing to local folders...`);
        for (const skill of skills) {
          const { assistant, name, filename } = skill;
          console.log(`Downloading ${assistant} skill: ${name}...`);
          const detail = await apiRequest('GET', `/v1/agent-skills/${assistant}/${name}`);
          const dir = path.join(process.cwd(), `.${assistant}`, 'skills');
          fs.mkdirSync(dir, { recursive: true });
          fs.writeFileSync(path.join(dir, filename), detail.content);
          console.log(`Successfully synced ${assistant} skill: ${name} to ${filename}`);
        }
        console.log("Sync complete!");
        break;
      }
      default:
        console.error(`Error: Unknown command "${command}"`);
        printUsage();
        process.exit(1);
    }
  } catch (error) {
    console.error(`Request failed: ${error.message}`);
    process.exit(1);
  }
}

run();
