#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';

export function parseArgs(argv = process.argv.slice(2)) {
  const options = {};
  const positional = [];
  let positionalOnly = false;

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];

    if (positionalOnly) {
      positional.push(arg);
      continue;
    }

    if (arg === '--') {
      positionalOnly = true;
      continue;
    }

    if (!arg.startsWith('--')) {
      positional.push(arg);
      continue;
    }

    const withoutPrefix = arg.slice(2);
    if (withoutPrefix.startsWith('no-') && !withoutPrefix.includes('=')) {
      options[toCamelCase(withoutPrefix.slice(3))] = false;
      continue;
    }

    const equalsIndex = withoutPrefix.indexOf('=');
    if (equalsIndex !== -1) {
      const key = toCamelCase(withoutPrefix.slice(0, equalsIndex));
      options[key] = withoutPrefix.slice(equalsIndex + 1);
      continue;
    }

    const key = toCamelCase(withoutPrefix);
    const next = argv[i + 1];
    if (!next || next.startsWith('--')) {
      options[key] = true;
      continue;
    }

    options[key] = next;
    i += 1;
  }

  return { options, positional };
}

export function loadLocalCredentials(startDir = process.cwd()) {
  let dir = startDir;
  while (true) {
    const credPath = path.join(dir, '.memoryops.local.json');
    if (fs.existsSync(credPath)) {
      try {
        const content = readJsonFile(credPath);
        return {
          apiKey: content.api_key || content.apiKey,
          workspaceId: content.workspace_id || content.workspaceId,
          apiUrl: content.api_url || content.apiUrl,
        };
      } catch {
        // Keep walking upward. A malformed local file should not prevent env-based use.
      }
    }

    const parent = path.dirname(dir);
    if (parent === dir) {
      return null;
    }
    dir = parent;
  }
}

export function resolveMemoryOpsConfig(options = {}, { requireAuth = true } = {}) {
  const local = loadLocalCredentials() || {};
  const config = {
    apiUrl: options.apiUrl || process.env.MEMORYOPS_API_URL || local.apiUrl || 'http://localhost:8080',
    apiKey: options.apiKey || process.env.MEMORYOPS_API_KEY || local.apiKey,
    workspaceId: options.workspaceId || process.env.MEMORYOPS_WORKSPACE_ID || local.workspaceId,
  };

  if (requireAuth && !config.apiKey) {
    throw new Error('Missing MemoryOps API key. Set MEMORYOPS_API_KEY, pass --api-key, or use .memoryops.local.json.');
  }

  if (requireAuth && !config.workspaceId) {
    throw new Error('Missing MemoryOps workspace ID. Set MEMORYOPS_WORKSPACE_ID, pass --workspace-id, or use .memoryops.local.json.');
  }

  return config;
}

export async function apiRequest(config, method, endpoint, body = null) {
  const url = `${String(config.apiUrl).replace(/\/$/, '')}${endpoint}`;
  const headers = {
    Accept: 'application/json, application/x-ndjson, text/plain, */*',
    'Content-Type': 'application/json',
  };

  if (config.apiKey) {
    headers['X-API-Key'] = config.apiKey;
  }

  const response = await fetch(url, {
    method,
    headers,
    body: body === null ? undefined : JSON.stringify(body),
  });

  const text = await response.text();
  const data = text ? safeJsonParse(text) : null;

  if (!response.ok) {
    const detail = typeof data === 'object' && data !== null ? JSON.stringify(data) : text;
    throw new Error(`HTTP ${response.status} ${response.statusText}: ${detail}`);
  }

  return data;
}

export function readJsonFile(filePath) {
  const resolved = path.resolve(filePath);
  try {
    return JSON.parse(fs.readFileSync(resolved, 'utf8'));
  } catch (error) {
    throw new Error(`Failed to read JSON file ${resolved}: ${error.message}`);
  }
}

export function writeTextFile(filePath, content) {
  const resolved = path.resolve(filePath);
  const text = String(content);
  fs.mkdirSync(path.dirname(resolved), { recursive: true });
  fs.writeFileSync(resolved, text.endsWith('\n') ? text : `${text}\n`, 'utf8');
  return resolved;
}

export function asBoolean(value, defaultValue = false) {
  if (value === undefined) return defaultValue;
  if (typeof value === 'boolean') return value;
  return ['1', 'true', 'yes', 'y', 'on'].includes(String(value).toLowerCase());
}

export function asNumber(value, defaultValue) {
  if (value === undefined || value === null || value === '') return defaultValue;
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    throw new Error(`Expected a number but received: ${value}`);
  }
  return parsed;
}

export function splitCsv(value) {
  if (!value) return [];
  return String(value)
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
}

export function printJson(value) {
  console.log(JSON.stringify(value, null, 2));
}

export function fail(message, exitCode = 1) {
  console.error(message);
  process.exit(exitCode);
}

function safeJsonParse(text) {
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

function toCamelCase(value) {
  return value.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
}
