#!/usr/bin/env node

import fs from 'node:fs';
import { spawnSync } from 'node:child_process';

const CREDENTIALS_FILE = '.memoryops.local.json';
const API_BASE_URL = process.env.API_BASE_URL || 'http://localhost:8080';
const appSecretKeyName = ['APP', 'SECRET', 'KEY'].join('_');
const workspaceCreationSecretName = ['WORKSPACE', 'CREATION', 'SECRET'].join('_');

const env = {
  ...process.env,
  API_BASE_URL,
  APP_ENV: process.env.APP_ENV || 'development',
  [appSecretKeyName]: process.env[appSecretKeyName] || 'dev-placeholder',
  [workspaceCreationSecretName]: process.env[workspaceCreationSecretName] || 'dev-placeholder',
};

function run(command, args, options = {}) {
  console.log(`\n$ ${command} ${args.join(' ')}`);
  const result = spawnSync(command, args, {
    stdio: 'inherit',
    env,
    ...options,
  });
  if (result.status !== 0) {
    process.exit(result.status || 1);
  }
}

function readCredentials() {
  if (!fs.existsSync(CREDENTIALS_FILE)) return null;
  try {
    const parsed = JSON.parse(fs.readFileSync(CREDENTIALS_FILE, 'utf8'));
    if (parsed.workspace_id && parsed.api_key) return parsed;
  } catch {
    // Re-bootstrap below.
  }
  return null;
}

async function waitForApi() {
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${API_BASE_URL}/health/ready`);
      if (res.ok) return;
    } catch {
      // API is not ready yet.
    }
    await sleep(2000);
  }
  console.error(`API did not become ready at ${API_BASE_URL}/health/ready within 120 seconds.`);
  process.exit(1);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

console.log('Starting MemoryOps local stack...');
run('docker', ['compose', 'up', '-d', '--build', 'postgres', 'redis', 'qdrant', 'api']);
await waitForApi();

let credentials = readCredentials();
if (!credentials) {
  console.log('\nNo local credentials found; bootstrapping workspace...');
  run('node', ['scripts/bootstrap.mjs']);
  credentials = readCredentials();
}

if (!credentials) {
  console.error(`Could not read ${CREDENTIALS_FILE} after bootstrap.`);
  process.exit(1);
}

env.MEMORYOPS_WORKSPACE_ID = credentials.workspace_id;
env.API_KEY = credentials.api_key;
env.WORKSPACE_ID = credentials.workspace_id;

console.log('\nSeeding demo data, including deterministic contradiction flags...');
run('node', ['scripts/seed.mjs']);

console.log('\nStarting frontend with workspace ID injected...');
run('docker', ['compose', 'up', '-d', '--build', 'frontend']);

console.log('\nMemoryOps local launch complete.');
console.log(`Frontend: http://localhost:5173`);
console.log(`API:      ${API_BASE_URL}`);
console.log(`Workspace: ${credentials.workspace_id}`);
console.log('Paste the API key from .memoryops.local.json into the frontend Settings modal.');
