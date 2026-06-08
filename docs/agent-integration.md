# External Agent Integration Guide

This guide explains how to connect and configure AI agents (e.g. Claude Code, Cursor, VS Code, or custom scripts) running in **other repositories** to interact with your running MemoryOps instance.

---

## 1. Zero-Dependency REST CLI Client

If you want a lightweight integration without configuring MCP, you can copy the helper client script from this repository into the other repo:

```bash
# From the root of your other repository:
cp /path/to/memoryops/scripts/memoryops-client.js ./scripts/
chmod +x ./scripts/memoryops-client.js
```

### Configuration (Environment Variables)
In the other repository, set the following environment variables (usually in a local `.env` or in your terminal session) to authenticate against the local MemoryOps instance:

```bash
export MEMORYOPS_API_KEY="mops_019e8d27_HoKjZ2mh1nyeTeRnMGRVEvZZ2bJaK6dG7xkb99RJvmtA"
export MEMORYOPS_WORKSPACE_ID="019e8d27-2242-7310-a62d-5124996ad146"
export MEMORYOPS_API_URL="http://localhost:8080" # Default is http://localhost:8080
```

### Command Reference

#### Retrieve Context (Query Vector DB)
```bash
node scripts/memoryops-client.js retrieve "Qdrant configuration details"
```

#### Directly Store a Decision (Immediate)
```bash
node scripts/memoryops-client.js store "Configured loopback interface restrictions to prevent SSRF in skill endpoint URLs" security dns config
```

#### Submit a Raw Observation (Async Queue)
```bash
node scripts/memoryops-client.js observe "Spotted connection timeout when calling Ollama from inside Docker container api-1" ollama networking
```

#### List Workspace Skills
```bash
node scripts/memoryops-client.js skills
```

---

## 2. Setting Up Agent Skill definitions

For agents that support custom skill directories (like `.claude/skills` or `.gemini/skills`), you need to populate those skill files into your repository. This teaches the agent when and how to query MemoryOps for context, or when to save memories.

### Option A — Copying from Local Clone
If you have the MemoryOps repository cloned locally on the same machine, run the following from the root of your target repository:

```bash
# For Claude Code:
mkdir -p .claude/skills
cp /path/to/memoryops/.claude/skills/*.md .claude/skills/

# For Gemini / Antigravity:
mkdir -p .gemini/skills
cp /path/to/memoryops/.gemini/skills/*.md .gemini/skills/
```

### Option B — Downloading Directly via MemoryOps API
If you don't have the source repository locally, you can pull the skills directly from the running MemoryOps instance.

First, list all available agent skills:
```bash
curl -H "X-API-Key: YOUR_API_KEY" http://localhost:8080/v1/agent-skills
```

Then, download the skills using `jq` or `Node.js` directly into your folders:

#### Using `jq`:
```bash
# Download Gemini skills
mkdir -p .gemini/skills
curl -s -H "X-API-Key: YOUR_API_KEY" http://localhost:8080/v1/agent-skills/gemini/use_memoryops | jq -r .content > .gemini/skills/use_memoryops.md
curl -s -H "X-API-Key: YOUR_API_KEY" http://localhost:8080/v1/agent-skills/gemini/manage_contradictions | jq -r .content > .gemini/skills/manage_contradictions.md

# Download Claude skills
mkdir -p .claude/skills
curl -s -H "X-API-Key: YOUR_API_KEY" http://localhost:8080/v1/agent-skills/claude/use_memoryops | jq -r .content > .claude/skills/use_memoryops.md
```

#### Using `Node.js` (No external dependencies):
```bash
node -e "
const fs = require('fs');
const apiKey = 'YOUR_API_KEY';
const baseUrl = 'http://localhost:8080/v1/agent-skills';

async function download(assistant, skillName) {
  const res = await fetch(\`\${baseUrl}/\${assistant}/\${skillName}\`, { headers: { 'X-API-Key': apiKey } });
  const data = await res.json();
  const dir = \`.\${assistant}/skills\`;
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(\`\${dir}/\${data.filename}\`, data.content);
  console.log(\`Downloaded \${assistant} skill: \${skillName}\`);
}

download('gemini', 'use_memoryops');
download('gemini', 'manage_contradictions');
download('claude', 'use_memoryops');
"
```

Once populated, the agent in your repository will automatically detect these instructions and use the `memoryops-client.js` script to pull workspace context at the start of a task or to save key decisions.

---

## 3. Direct MCP Integration (HTTP Transport)

If the agent runner supports Model Context Protocol (MCP) directly, it can interact with MemoryOps over the HTTP streamable transport without needing any local scripts.

The local MCP server is running on:
- **Endpoint**: `http://localhost:3003/mcp`
- **Authentication**: `Authorization: Bearer <YOUR_API_KEY>`

### A. Claude Code Setup (Project-level)
Add a `.mcp.json` file in the root of the other repository:

```json
{
  "mcpServers": {
    "memoryops": {
      "type": "http",
      "url": "http://localhost:3003/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_API_KEY"
      }
    }
  }
}
```

*Be sure to add `.mcp.json` to your `.gitignore` to prevent leaking your key.*

### B. Cursor / VS Code Setup (via Continue.dev extension)
Add the MCP server provider to your global `~/.continue/config.json`:

```json
{
  "contextProviders": [
    {
      "name": "mcp",
      "options": {
        "url": "http://localhost:3003/mcp",
        "headers": {
          "Authorization": "Bearer YOUR_API_KEY"
        }
      }
    }
  ]
}
```

### C. Claude Desktop Setup
Configure the global `claude_desktop_config.json` (located at `~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "memoryops": {
      "command": "curl",
      "args": [
        "-sS",
        "-X", "POST",
        "http://localhost:3003/mcp",
        "-H", "Authorization: Bearer YOUR_API_KEY",
        "-H", "Content-Type: application/json",
        "-d", "{\"jsonrpc\":\"2.0\",\"method\":\"initialize\",\"params\":{}}"
      ]
    }
  }
}
```
