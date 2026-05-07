#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';

const API_BASE_URL = process.env.API_BASE_URL || 'http://localhost:8080';
const WORKSPACE_CREATION_SECRET = process.env.WORKSPACE_CREATION_SECRET;

if (!WORKSPACE_CREATION_SECRET) {
  console.error("Error: WORKSPACE_CREATION_SECRET environment variable is required.");
  console.error("Usage: WORKSPACE_CREATION_SECRET=your_secret node scripts/bootstrap.mjs");
  process.exit(1);
}

const workspaceName = `dev-workspace-${Date.now()}`;
console.log(`Bootstrapping workspace: ${workspaceName}...`);

try {
  const response = await fetch(`${API_BASE_URL}/v1/workspaces`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'x-admin-token': WORKSPACE_CREATION_SECRET
    },
    body: JSON.stringify({ name: workspaceName })
  });

  if (!response.ok) {
    const errorText = await response.text();
    console.error(`Failed to create workspace: HTTP ${response.status} - ${errorText}`);
    process.exit(1);
  }

  const data = await response.json();
  const workspaceId = data.workspace_id || data.id;
  const apiKey = data.api_key;

  if (!workspaceId || !apiKey) {
    console.error(`Invalid response from API: ${JSON.stringify(data)}`);
    process.exit(1);
  }

  const outFile = path.join(process.cwd(), '.memoryops.local.json');
  fs.writeFileSync(outFile, JSON.stringify({
    workspace_id: workspaceId,
    api_key: apiKey
  }, null, 2));

  console.log(`\n✅ Workspace bootstrapped successfully!`);
  console.log(`Saved credentials to: .memoryops.local.json\n`);
  
  console.log(`Next steps:`);
  console.log(`1. Start the frontend:`);
  let isWin = process.platform === "win32";
  if (isWin) {
    console.log(`   $env:MEMORYOPS_WORKSPACE_ID="${workspaceId}"; docker compose up -d --build frontend\n`);
  } else {
    console.log(`   MEMORYOPS_WORKSPACE_ID=${workspaceId} docker compose up -d --build frontend\n`);
  }
  
  console.log(`2. (Optional) Seed development data:`);
  if (isWin) {
    console.log(`   $env:API_KEY="${apiKey}"; node scripts/seed.mjs\n`);
  } else {
    console.log(`   API_KEY=${apiKey} node scripts/seed.mjs\n`);
  }
  
  console.log(`3. (Optional) Start MCP Server:`);
  console.log(`   docker compose up -d mcp`);
  
} catch (error) {
  console.error(`Error bootstrapping workspace: ${error.message}`);
  process.exit(1);
}
