#!/usr/bin/env node

import path from 'node:path';
import {
  apiRequest,
  asBoolean,
  fail,
  parseArgs,
  printJson,
  resolveMemoryOpsConfig,
  writeTextFile,
} from './memoryops-common.mjs';

const USAGE = `
MemoryOps workspace snapshot exporter

Usage:
  node scripts/memoryops-snapshot.mjs [options]

Options:
  --out-dir <path>        Output directory. Default: .memoryops/snapshots/<timestamp>
  --include-memory        Include workspace memory export. Default: true
  --include-tools         Include workspace tool export. Default: true
  --include-health        Include health/readiness/system snapshots. Default: true
  --include-audit         Include audit log first page. Default: false
  --include-dlq           Include DLQ jobs. Default: true
  --include-integrations  Include integration status. Default: true
  --include-tags          Include top workspace tags. Default: true
  --include-contradictions Include contradiction count and first page. Default: true
  --json                  Print machine-readable manifest
  --api-url <url>         Overrides MEMORYOPS_API_URL
  --workspace-id <uuid>   Overrides MEMORYOPS_WORKSPACE_ID
  --api-key <key>         Overrides MEMORYOPS_API_KEY

Examples:
  node scripts/memoryops-snapshot.mjs
  node scripts/memoryops-snapshot.mjs --out-dir backups/memoryops-$(date +%Y%m%d)
  node scripts/memoryops-snapshot.mjs --include-audit --json

The snapshot is a portable operator bundle. It intentionally writes API responses to local files but does not decrypt secrets.
`;

const { options } = parseArgs();

if (options.help || options.h) {
  console.log(USAGE);
  process.exit(0);
}

let config;
try {
  config = resolveMemoryOpsConfig(options);
} catch (error) {
  fail(error.message);
}

try {
  const snapshotOptions = normalizeOptions(options);
  const manifest = await createSnapshot(config, snapshotOptions);

  if (snapshotOptions.json) {
    printJson(manifest);
  } else {
    printManifest(manifest);
  }

  process.exit(manifest.failed.length === 0 ? 0 : 1);
} catch (error) {
  fail(`Snapshot failed: ${error.message}`);
}

function normalizeOptions(options) {
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
  return {
    outDir: options.outDir || `.memoryops/snapshots/${timestamp}`,
    includeMemory: optionDefaultTrue(options.includeMemory),
    includeTools: optionDefaultTrue(options.includeTools),
    includeHealth: optionDefaultTrue(options.includeHealth),
    includeAudit: asBoolean(options.includeAudit, false),
    includeDlq: optionDefaultTrue(options.includeDlq),
    includeIntegrations: optionDefaultTrue(options.includeIntegrations),
    includeTags: optionDefaultTrue(options.includeTags),
    includeContradictions: optionDefaultTrue(options.includeContradictions),
    json: asBoolean(options.json, false),
  };
}

async function createSnapshot(config, options) {
  const outDir = path.resolve(options.outDir);
  const startedAt = new Date().toISOString();
  const manifest = {
    workspace_id: config.workspaceId,
    api_url: config.apiUrl,
    started_at: startedAt,
    finished_at: null,
    out_dir: outDir,
    files: [],
    failed: [],
  };

  await captureJson(config, manifest, 'workspace.json', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}`);
  await captureJson(config, manifest, 'stats.json', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/stats`);
  await captureJson(config, manifest, 'stats-history.json', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/stats/history?days=30`);

  if (options.includeHealth) {
    await captureJson(config, manifest, 'health-ready.json', '/health/ready');
    await captureJson(config, manifest, 'health-system.json', '/health/system');
  }

  if (options.includeMemory) {
    await captureRaw(config, manifest, 'memory-export.ndjson', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/export`);
  }

  if (options.includeTools) {
    await captureJson(config, manifest, 'tools-export.json', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/tools/export`);
  }

  if (options.includeIntegrations) {
    await captureJson(config, manifest, 'integrations.json', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/integrations`);
  }

  if (options.includeDlq) {
    await captureJson(config, manifest, 'dlq.json', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/dlq`);
  }

  if (options.includeTags) {
    await captureJson(config, manifest, 'tags.json', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/tags?limit=100`);
  }

  if (options.includeContradictions) {
    await captureJson(config, manifest, 'contradiction-count.json', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/contradictions/count`);
    await captureJson(config, manifest, 'contradictions.json', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/contradictions`);
  }

  if (options.includeAudit) {
    await captureJson(config, manifest, 'audit.json', `/v1/workspaces/${encodeURIComponent(config.workspaceId)}/audit?limit=100`);
  }

  manifest.finished_at = new Date().toISOString();
  const manifestPath = writeTextFile(path.join(outDir, 'manifest.json'), JSON.stringify(manifest, null, 2));
  manifest.files.push({ path: manifestPath, kind: 'manifest' });
  return manifest;
}

async function captureJson(config, manifest, filename, endpoint) {
  try {
    const data = await apiRequest(config, 'GET', endpoint);
    const written = writeTextFile(path.join(manifest.out_dir, filename), JSON.stringify(data, null, 2));
    manifest.files.push({ path: written, endpoint, kind: 'json' });
  } catch (error) {
    manifest.failed.push({ filename, endpoint, error: error.message });
  }
}

async function captureRaw(config, manifest, filename, endpoint) {
  try {
    const data = await apiRequest(config, 'GET', endpoint);
    const body = typeof data === 'string' ? data : JSON.stringify(data, null, 2);
    const written = writeTextFile(path.join(manifest.out_dir, filename), body);
    manifest.files.push({ path: written, endpoint, kind: 'raw' });
  } catch (error) {
    manifest.failed.push({ filename, endpoint, error: error.message });
  }
}

function printManifest(manifest) {
  console.log(`MemoryOps snapshot for ${manifest.workspace_id}`);
  console.log(`Output: ${manifest.out_dir}`);
  console.log(`Files: ${manifest.files.length} | Failed: ${manifest.failed.length}`);
  for (const file of manifest.files) {
    console.log(`  - ${file.path}`);
  }
  if (manifest.failed.length > 0) {
    console.log('Failures:');
    for (const failure of manifest.failed) {
      console.log(`  - ${failure.filename}: ${failure.error}`);
    }
  }
}

function optionDefaultTrue(value) {
  if (value === undefined) return true;
  return asBoolean(value, true);
}
