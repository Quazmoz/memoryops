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
          workspaceId: content.workspace_id || content.workspaceId
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
if (!apiKey || !workspaceId) {
  const creds = loadLocalCredentials();
  if (creds) {
    apiKey = apiKey || creds.apiKey;
    workspaceId = workspaceId || creds.workspaceId;
  }
}

function printUsage() {
  console.log(`
MemoryOps Agent CLI Client

Usage:
  node memoryops-client.js <command> [arguments]

Commands:
  retrieve "<query>"       Retrieve relevant memories and skills matching query
  store "<content>" [tags] Directly persist an episodic memory
  observe "<content>" [tags] Submit a raw observation to the classification queue
  skills                   List all registered workspace skills
  help                     Show this help message

Environment Variables:
  MEMORYOPS_API_KEY        API key for authentication (e.g. mops_019...)
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
      case 'skills': {
        const endpoint = `/v1/workspaces/${workspaceId}/skills`;
        const res = await apiRequest('GET', endpoint);
        console.log(JSON.stringify(res, null, 2));
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
