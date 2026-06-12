#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import {
  apiRequest,
  asBoolean,
  fail,
  parseArgs,
  printJson,
  readJsonFile,
  resolveMemoryOpsConfig,
  writeTextFile,
} from './memoryops-common.mjs';

const VALID_METHODS = new Set(['GET', 'POST', 'PUT', 'PATCH', 'DELETE']);
const VALID_VISIBILITY = new Set(['private', 'workspace', 'published']);

const USAGE = `
MemoryOps tool-pack utility

Usage:
  node scripts/memoryops-tool-pack.mjs <command> [options]

Commands:
  validate --file <path>        Validate a tool pack file without contacting MemoryOps
  import --file <path>          Validate and import a tool pack into the workspace
  export --out <path>           Export workspace tools into a tool pack file
  list                          List registered workspace tools

Options:
  --file <path>                 Tool pack JSON file for validate/import
  --out <path>                  Output path for export
  --overwrite                   Update existing tools during import
  --json                        Print machine-readable output
  --api-url <url>               Overrides MEMORYOPS_API_URL
  --workspace-id <uuid>         Overrides MEMORYOPS_WORKSPACE_ID
  --api-key <key>               Overrides MEMORYOPS_API_KEY

Tool pack shape:
  {
    "name": "memoryops-devops-pack",
    "version": "0.1.0",
    "tools": [
      {
        "name": "example_tool",
        "description": "Short explanation of what this tool does.",
        "endpoint_url": "https://example.com/api/tool",
        "http_method": "POST",
        "input_schema": { "type": "object", "properties": {} },
        "output_schema": { "type": "object" },
        "scope_visibility": "workspace",
        "enabled": true
      }
    ]
  }

Secrets are intentionally not exported. Add auth_secret manually only through a secure local workflow.
`;

const { options, positional } = parseArgs();
const command = positional[0];

if (options.help || options.h || !command) {
  console.log(USAGE);
  process.exit(command ? 0 : 1);
}

try {
  switch (command) {
    case 'validate':
      await validateCommand(options);
      break;
    case 'import':
      await importCommand(options);
      break;
    case 'export':
      await exportCommand(options);
      break;
    case 'list':
      await listCommand(options);
      break;
    default:
      fail(`Unknown command "${command}".\n\n${USAGE.trim()}`);
  }
} catch (error) {
  fail(`Tool pack command failed: ${error.message}`);
}

async function validateCommand(options) {
  const pack = loadPack(options.file);
  const validation = validatePack(pack);
  output(validation, options);
  process.exit(validation.valid ? 0 : 1);
}

async function importCommand(options) {
  const config = resolveMemoryOpsConfig(options);
  const pack = loadPack(options.file);
  const validation = validatePack(pack);

  if (!validation.valid) {
    output(validation, options);
    process.exit(1);
  }

  const response = await apiRequest(
    config,
    'POST',
    `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/tools/import`,
    {
      tools: pack.tools.map(toImportPayload),
      overwrite: asBoolean(options.overwrite, false),
    },
  );

  output({
    valid: true,
    action: 'import',
    pack: pack.name || null,
    version: pack.version || null,
    requested: pack.tools.length,
    warnings: validation.issues.filter((item) => item.severity === 'warning'),
    response,
  }, options);
}

async function exportCommand(options) {
  const config = resolveMemoryOpsConfig(options);
  const out = options.out || `memoryops-tools-${config.workspaceId}.json`;
  const response = await apiRequest(config, 'GET', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/tools/export`);
  const tools = normalizeToolArray(response);
  const pack = {
    name: `memoryops-workspace-${config.workspaceId}-tools`,
    version: new Date().toISOString(),
    exported_at: new Date().toISOString(),
    workspace_id: config.workspaceId,
    notes: 'Secrets are intentionally omitted from exported tool packs.',
    tools: tools.map(toExportPayload),
  };
  const written = writeTextFile(out, JSON.stringify(pack, null, 2));
  output({ action: 'export', written, tools: pack.tools.length }, options);
}

async function listCommand(options) {
  const config = resolveMemoryOpsConfig(options);
  const response = await apiRequest(config, 'GET', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/tools`);
  const normalized = normalizeToolArray(response).map((tool) => ({
    name: tool.name,
    enabled: tool.enabled,
    version: tool.version,
    method: tool.http_method,
    endpoint_url: tool.endpoint_url,
    scope_visibility: tool.scope_visibility,
    rate_limit_per_minute: tool.rate_limit_per_minute,
  }));
  output({ action: 'list', tools: normalized, count: normalized.length }, options);
}

function loadPack(filePath) {
  if (!filePath) {
    throw new Error('--file is required.');
  }
  const resolved = path.resolve(filePath);
  if (!fs.existsSync(resolved)) {
    throw new Error(`Tool pack file does not exist: ${resolved}`);
  }
  const payload = readJsonFile(resolved);
  const tools = normalizeToolArray(payload);
  if (tools.length === 0) {
    throw new Error('Tool pack must be an array or an object with a non-empty tools array.');
  }
  return Array.isArray(payload) ? { name: path.basename(resolved), tools } : { ...payload, tools };
}

function validatePack(pack) {
  const issues = [];
  const names = new Set();

  if (pack.name !== undefined && typeof pack.name !== 'string') {
    issues.push(issue('pack.name', 'Pack name must be a string when provided.'));
  }

  pack.tools.forEach((tool, index) => {
    validateTool(tool, index, issues, names);
  });

  const errorCount = issues.filter((item) => item.severity === 'error').length;
  const warningCount = issues.filter((item) => item.severity === 'warning').length;

  return {
    valid: errorCount === 0,
    pack: pack.name || null,
    version: pack.version || null,
    tools: pack.tools.length,
    error_count: errorCount,
    warning_count: warningCount,
    issues,
  };
}

function validateTool(tool, index, issues, names) {
  const prefix = `tools[${index}]`;
  if (!tool || typeof tool !== 'object' || Array.isArray(tool)) {
    issues.push(issue(prefix, 'Tool must be an object.'));
    return;
  }

  requireString(tool, 'name', prefix, issues);
  requireString(tool, 'description', prefix, issues);
  requireString(tool, 'endpoint_url', prefix, issues);

  if (typeof tool.name === 'string') {
    if (!/^[a-zA-Z0-9_.-]+$/.test(tool.name)) {
      issues.push(issue(`${prefix}.name`, 'Tool name should contain only letters, numbers, underscore, dot, or dash.'));
    }
    if (names.has(tool.name)) {
      issues.push(issue(`${prefix}.name`, `Duplicate tool name: ${tool.name}`));
    }
    names.add(tool.name);
  }

  const method = String(tool.http_method || 'POST').toUpperCase();
  if (!VALID_METHODS.has(method)) {
    issues.push(issue(`${prefix}.http_method`, `Invalid method ${method}.`));
  }

  if (tool.endpoint_url && typeof tool.endpoint_url === 'string') {
    try {
      const url = new URL(tool.endpoint_url);
      if (!['http:', 'https:'].includes(url.protocol)) {
        issues.push(issue(`${prefix}.endpoint_url`, 'Endpoint URL must use http or https.'));
      }
    } catch {
      issues.push(issue(`${prefix}.endpoint_url`, 'Endpoint URL must be a valid URL.'));
    }
  }

  validateSchema(tool.input_schema, `${prefix}.input_schema`, issues);
  validateSchema(tool.output_schema, `${prefix}.output_schema`, issues);

  if (tool.scope_visibility !== undefined && !VALID_VISIBILITY.has(tool.scope_visibility)) {
    issues.push(issue(`${prefix}.scope_visibility`, `Invalid scope visibility ${tool.scope_visibility}.`));
  }

  validatePositiveNumber(tool.rate_limit_per_minute, `${prefix}.rate_limit_per_minute`, issues);
  validatePositiveNumber(tool.circuit_breaker_threshold, `${prefix}.circuit_breaker_threshold`, issues);
  validatePositiveNumber(tool.circuit_breaker_cooldown_seconds, `${prefix}.circuit_breaker_cooldown_seconds`, issues);

  if (tool.auth_secret) {
    issues.push(issue(`${prefix}.auth_secret`, 'Tool pack contains auth_secret. Avoid committing packs with plaintext secrets.', 'warning'));
  }
}

function requireString(tool, key, prefix, issues) {
  if (typeof tool[key] !== 'string' || tool[key].trim().length === 0) {
    issues.push(issue(`${prefix}.${key}`, `${key} is required and must be a non-empty string.`));
  }
}

function validateSchema(schema, path, issues) {
  if (schema === undefined || schema === null) return;
  if (typeof schema !== 'object' || Array.isArray(schema)) {
    issues.push(issue(path, 'Schema must be a JSON object when provided.'));
    return;
  }
  if (schema.type !== undefined && typeof schema.type !== 'string') {
    issues.push(issue(`${path}.type`, 'Schema type must be a string when provided.'));
  }
}

function validatePositiveNumber(value, path, issues) {
  if (value === undefined || value === null) return;
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
    issues.push(issue(path, 'Value must be a positive number when provided.'));
  }
}

function issue(path, message, severity = 'error') {
  return { severity, path, message };
}

function toImportPayload(tool) {
  return stripUndefined({
    name: tool.name,
    description: tool.description,
    endpoint_url: tool.endpoint_url,
    http_method: String(tool.http_method || 'POST').toUpperCase(),
    input_schema: tool.input_schema || { type: 'object', properties: {} },
    output_schema: tool.output_schema || { type: 'object' },
    auth_header: tool.auth_header,
    auth_secret: tool.auth_secret,
    enabled: tool.enabled ?? true,
    change_note: tool.change_note || 'Imported from MemoryOps tool pack',
    scope_visibility: tool.scope_visibility || 'workspace',
    rate_limit_per_minute: tool.rate_limit_per_minute,
    circuit_breaker_threshold: tool.circuit_breaker_threshold,
    circuit_breaker_cooldown_seconds: tool.circuit_breaker_cooldown_seconds,
  });
}

function toExportPayload(tool) {
  return stripUndefined({
    name: tool.name,
    description: tool.description,
    endpoint_url: tool.endpoint_url,
    http_method: tool.http_method,
    input_schema: tool.input_schema,
    output_schema: tool.output_schema,
    auth_header: tool.auth_header,
    enabled: tool.enabled,
    scope_visibility: tool.scope_visibility,
    rate_limit_per_minute: tool.rate_limit_per_minute,
    circuit_breaker_threshold: tool.circuit_breaker_threshold,
    circuit_breaker_cooldown_seconds: tool.circuit_breaker_cooldown_seconds,
    version: tool.version,
  });
}

function normalizeToolArray(payload) {
  if (Array.isArray(payload)) return payload;
  if (!payload || typeof payload !== 'object') return [];
  for (const key of ['tools', 'items', 'data', 'results']) {
    if (Array.isArray(payload[key])) return payload[key];
  }
  return [];
}

function stripUndefined(value) {
  return Object.fromEntries(Object.entries(value).filter(([, entry]) => entry !== undefined));
}

function output(payload, options) {
  if (asBoolean(options.json, false)) {
    printJson(payload);
    return;
  }

  if (payload.action === 'list') {
    console.log(`Workspace tools: ${payload.count}`);
    for (const tool of payload.tools) {
      console.log(`  - ${tool.name} v${tool.version} ${tool.enabled ? 'enabled' : 'disabled'} ${tool.method} ${tool.endpoint_url}`);
    }
    return;
  }

  if (payload.action === 'export') {
    console.log(`Exported ${payload.tools} tools to ${payload.written}`);
    return;
  }

  if (payload.action === 'import') {
    console.log(`Imported tool pack ${payload.pack || '<unnamed>'}: ${payload.requested} requested`);
    if (payload.warnings.length > 0) {
      console.log('Warnings:');
      for (const warning of payload.warnings) {
        console.log(`  - ${warning.path}: ${warning.message}`);
      }
    }
    console.log(JSON.stringify(payload.response, null, 2));
    return;
  }

  console.log(`Tool pack validation: ${payload.valid ? 'valid' : 'invalid'} (${payload.tools} tools, ${payload.error_count} errors, ${payload.warning_count} warnings)`);
  if (payload.issues.length > 0) {
    for (const item of payload.issues) {
      console.log(`  - [${item.severity}] ${item.path}: ${item.message}`);
    }
  }
}
