#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import {
  apiRequest,
  asBoolean,
  asNumber,
  fail,
  parseArgs,
  printJson,
  readJsonFile,
  resolveMemoryOpsConfig,
  splitCsv,
} from './memoryops-common.mjs';

const TEXT_EXTENSIONS = new Set(['.md', '.mdx', '.txt']);

const USAGE = `
MemoryOps memory importer

Usage:
  node scripts/memoryops-import.mjs --path <file-or-directory> [options]

Options:
  --path <path>               Markdown/text directory, markdown/text file, JSON file, or JSONL file
  --format <auto|markdown|json|jsonl>  Input format. Default: auto
  --mode <observation|memory> Import through observation queue or direct memory write. Default: observation
  --agent-id <id>             Agent ID for imported observations. Default: memoryops-import
  --tags a,b,c                Tags to append to every imported item
  --repo <owner/name>         Optional repo scope metadata
  --user-id <id>              Optional user scope metadata
  --memory-type <type>        Direct memory mode only. Default: episodic
  --scope-visibility <value>  Direct memory mode only. private, workspace, or published
  --importance <score>        Direct memory mode only. Default: 0.6
  --max-chars <n>             Chunk text files above this size. Default: 6000
  --dry-run                   Parse and report without writing to MemoryOps
  --limit <n>                 Import at most n items
  --json                      Print machine-readable result
  --api-url <url>             Overrides MEMORYOPS_API_URL
  --workspace-id <uuid>       Overrides MEMORYOPS_WORKSPACE_ID
  --api-key <key>             Overrides MEMORYOPS_API_KEY

Examples:
  node scripts/memoryops-import.mjs --path docs --tags docs,bootstrap
  node scripts/memoryops-import.mjs --path notes.jsonl --mode memory --tags migrated
  node scripts/memoryops-import.mjs --path README.md --dry-run --json

Supported JSON/JSONL item fields:
  content, tags, metadata, source_ref, agent_id, user_id, repo, memory_type, scope_visibility, importance_score
`;

const { options } = parseArgs();

if (options.help || options.h) {
  console.log(USAGE);
  process.exit(0);
}

if (!options.path) {
  fail(USAGE.trim());
}

try {
  const importOptions = normalizeOptions(options);
  const config = resolveMemoryOpsConfig(options, { requireAuth: !importOptions.dryRun });
  const items = loadImportItems(importOptions);
  const limitedItems = importOptions.limit === null ? items : items.slice(0, importOptions.limit);

  if (importOptions.dryRun) {
    const result = summarize('dry_run', limitedItems, []);
    outputResult(result, importOptions.json);
    process.exit(0);
  }

  const writes = [];
  for (const item of limitedItems) {
    writes.push(await writeImportItem(config, importOptions, item));
  }

  const result = summarize('imported', limitedItems, writes);
  outputResult(result, importOptions.json);
  process.exit(result.failed === 0 ? 0 : 1);
} catch (error) {
  fail(`Import failed: ${error.message}`);
}

function normalizeOptions(options) {
  const inputPath = path.resolve(options.path);
  const format = options.format || 'auto';
  if (!['auto', 'markdown', 'json', 'jsonl'].includes(format)) {
    throw new Error(`Unsupported format "${format}".`);
  }

  const mode = options.mode || 'observation';
  if (!['observation', 'memory'].includes(mode)) {
    throw new Error(`Unsupported import mode "${mode}".`);
  }

  const importance = asNumber(options.importance, 0.6);
  if (importance < 0 || importance > 1) {
    throw new Error('--importance must be between 0 and 1.');
  }

  const maxChars = asNumber(options.maxChars, 6000);
  if (!Number.isInteger(maxChars) || maxChars < 500) {
    throw new Error('--max-chars must be an integer of at least 500.');
  }

  const limit = options.limit === undefined ? null : asNumber(options.limit, null);
  if (limit !== null && (!Number.isInteger(limit) || limit < 1)) {
    throw new Error('--limit must be a positive integer.');
  }

  return {
    inputPath,
    format,
    mode,
    agentId: options.agentId || 'memoryops-import',
    tags: splitCsv(options.tags),
    repo: options.repo || null,
    userId: options.userId || null,
    memoryType: options.memoryType || 'episodic',
    scopeVisibility: options.scopeVisibility || null,
    importance,
    maxChars,
    dryRun: asBoolean(options.dryRun, false),
    json: asBoolean(options.json, false),
    limit,
  };
}

function loadImportItems(importOptions) {
  if (!fs.existsSync(importOptions.inputPath)) {
    throw new Error(`Path does not exist: ${importOptions.inputPath}`);
  }

  const stats = fs.statSync(importOptions.inputPath);
  if (stats.isDirectory()) {
    return loadDirectory(importOptions);
  }

  const format = resolveFormat(importOptions.inputPath, importOptions.format);
  if (format === 'json') return loadJsonItems(importOptions.inputPath, importOptions);
  if (format === 'jsonl') return loadJsonlItems(importOptions.inputPath, importOptions);
  return loadTextFile(importOptions.inputPath, importOptions);
}

function loadDirectory(importOptions) {
  const files = walk(importOptions.inputPath)
    .filter((file) => TEXT_EXTENSIONS.has(path.extname(file).toLowerCase()))
    .sort();

  return files.flatMap((file) => loadTextFile(file, importOptions));
}

function loadTextFile(filePath, importOptions) {
  const content = fs.readFileSync(filePath, 'utf8').trim();
  if (!content) return [];

  const rel = path.relative(process.cwd(), filePath);
  const chunks = chunkText(content, importOptions.maxChars);
  return chunks.map((chunk, index) => ({
    content: chunks.length === 1 ? chunk : `${chunk}\n\n[chunk ${index + 1}/${chunks.length}]`,
    tags: inferTags(filePath, importOptions.tags),
    source_ref: rel,
    agent_id: importOptions.agentId,
    user_id: importOptions.userId,
    repo: importOptions.repo,
    metadata: {
      importer: 'memoryops-import',
      source_path: rel,
      chunk_index: index,
      chunk_count: chunks.length,
      imported_at: new Date().toISOString(),
    },
  }));
}

function loadJsonItems(filePath, importOptions) {
  const payload = readJsonFile(filePath);
  const items = Array.isArray(payload) ? payload : payload.items || payload.memories || payload.observations;
  if (!Array.isArray(items)) {
    throw new Error('JSON import file must be an array or contain items, memories, or observations array.');
  }
  return items.map((item, index) => normalizeJsonItem(item, filePath, index, importOptions));
}

function loadJsonlItems(filePath, importOptions) {
  return fs.readFileSync(filePath, 'utf8')
    .split(/\r?\n/)
    .map((line, index) => ({ line: line.trim(), index }))
    .filter(({ line }) => Boolean(line))
    .map(({ line, index }) => {
      try {
        return normalizeJsonItem(JSON.parse(line), filePath, index, importOptions);
      } catch (error) {
        throw new Error(`Invalid JSONL at ${filePath}:${index + 1}: ${error.message}`);
      }
    });
}

function normalizeJsonItem(item, filePath, index, importOptions) {
  if (!item || typeof item !== 'object' || Array.isArray(item)) {
    throw new Error(`Import item ${index + 1} in ${filePath} is not an object.`);
  }
  if (!item.content || typeof item.content !== 'string') {
    throw new Error(`Import item ${index + 1} in ${filePath} is missing string content.`);
  }

  return {
    ...item,
    tags: mergeTags(item.tags, importOptions.tags),
    source_ref: item.source_ref || item.sourceRef || path.relative(process.cwd(), filePath),
    agent_id: item.agent_id || item.agentId || importOptions.agentId,
    user_id: item.user_id || item.userId || importOptions.userId,
    repo: item.repo || importOptions.repo,
    scope_visibility: item.scope_visibility || item.scopeVisibility || importOptions.scopeVisibility,
    metadata: {
      ...(item.metadata && typeof item.metadata === 'object' && !Array.isArray(item.metadata) ? item.metadata : {}),
      importer: 'memoryops-import',
      imported_at: new Date().toISOString(),
      source_path: path.relative(process.cwd(), filePath),
      source_index: index,
    },
  };
}

async function writeImportItem(config, importOptions, item) {
  try {
    if (importOptions.mode === 'memory') {
      const payload = stripUndefined({
        workspace_id: config.workspaceId,
        memory_type: item.memory_type || item.memoryType || importOptions.memoryType,
        content: item.content,
        importance_score: item.importance_score ?? item.importanceScore ?? importOptions.importance,
        tags: mergeTags(item.tags, importOptions.tags),
        agent_id: item.agent_id || item.agentId || importOptions.agentId,
        user_id: item.user_id || item.userId || importOptions.userId,
        repo: item.repo || importOptions.repo,
        scope_visibility: item.scope_visibility || item.scopeVisibility || importOptions.scopeVisibility || undefined,
        metadata: buildMetadata(importOptions, item),
      });
      const response = await apiRequest(config, 'POST', '/v1/memory', payload);
      return { ok: true, source_ref: item.source_ref || null, id: response?.id || response?.memory_id || null };
    }

    const payload = stripUndefined({
      content: item.content,
      agent_id: item.agent_id || item.agentId || importOptions.agentId,
      user_id: item.user_id || item.userId || importOptions.userId,
      repo: item.repo || importOptions.repo,
      tags: mergeTags(item.tags, importOptions.tags),
      importance: item.importance ?? item.importance_score ?? item.importanceScore,
      source_ref: item.source_ref || item.sourceRef,
    });
    const response = await apiRequest(config, 'POST', '/v1/ingest/observation', payload);
    return { ok: true, source_ref: item.source_ref || null, id: response?.id || response?.event_id || null };
  } catch (error) {
    return { ok: false, source_ref: item.source_ref || null, error: error.message };
  }
}

function buildMetadata(importOptions, item) {
  return stripUndefined({
    ...(item.metadata && typeof item.metadata === 'object' && !Array.isArray(item.metadata) ? item.metadata : {}),
    source_ref: item.source_ref || item.sourceRef,
    repo: item.repo || importOptions.repo,
    user_id: item.user_id || item.userId || importOptions.userId,
  });
}

function summarize(status, items, writes) {
  const failedWrites = writes.filter((write) => !write.ok);
  return {
    status,
    total_items: items.length,
    imported: writes.length === 0 ? 0 : writes.length - failedWrites.length,
    failed: failedWrites.length,
    sources: [...new Set(items.map((item) => item.source_ref).filter(Boolean))],
    failures: failedWrites,
    preview: items.slice(0, 10).map((item) => ({
      source_ref: item.source_ref || null,
      tags: item.tags || [],
      chars: item.content.length,
      snippet: snippet(item.content),
    })),
  };
}

function outputResult(result, json) {
  if (json) {
    printJson(result);
    return;
  }

  console.log(`MemoryOps import ${result.status}`);
  console.log(`Items: ${result.total_items} | Imported: ${result.imported} | Failed: ${result.failed}`);
  console.log(`Sources: ${result.sources.length}`);
  if (result.failures.length > 0) {
    console.log('Failures:');
    for (const failure of result.failures) {
      console.log(`  - ${failure.source_ref || '<unknown>'}: ${failure.error}`);
    }
  }
  console.log('Preview:');
  for (const item of result.preview) {
    console.log(`  - ${item.source_ref || '<inline>'} (${item.chars} chars) ${item.snippet}`);
  }
}

function resolveFormat(filePath, format) {
  if (format !== 'auto') return format;
  const ext = path.extname(filePath).toLowerCase();
  if (ext === '.json') return 'json';
  if (ext === '.jsonl' || ext === '.ndjson') return 'jsonl';
  return 'markdown';
}

function walk(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === 'node_modules' || entry.name === '.git' || entry.name === 'target' || entry.name === 'dist') {
        return [];
      }
      return walk(full);
    }
    return entry.isFile() ? [full] : [];
  });
}

function chunkText(content, maxChars) {
  if (content.length <= maxChars) return [content];
  const chunks = [];
  let remaining = content;
  while (remaining.length > maxChars) {
    const preferredBreak = Math.max(
      remaining.lastIndexOf('\n## ', maxChars),
      remaining.lastIndexOf('\n\n', maxChars),
      remaining.lastIndexOf('\n', maxChars),
    );
    const splitAt = preferredBreak > maxChars * 0.4 ? preferredBreak : maxChars;
    chunks.push(remaining.slice(0, splitAt).trim());
    remaining = remaining.slice(splitAt).trim();
  }
  if (remaining) chunks.push(remaining);
  return chunks.filter(Boolean);
}

function inferTags(filePath, baseTags) {
  const extTag = path.extname(filePath).replace(/^\./, '') || 'text';
  return mergeTags([extTag, 'imported'], baseTags);
}

function mergeTags(...tagSets) {
  const merged = tagSets.flatMap((tags) => {
    if (!tags) return [];
    if (Array.isArray(tags)) return tags;
    return String(tags).split(',');
  });
  return [...new Set(merged.map((tag) => String(tag).trim()).filter(Boolean))];
}

function stripUndefined(value) {
  return Object.fromEntries(Object.entries(value).filter(([, entry]) => entry !== undefined && entry !== null));
}

function snippet(value) {
  const compact = String(value).replace(/\s+/g, ' ').trim();
  return compact.length > 120 ? `${compact.slice(0, 117)}...` : compact;
}
