import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const routerFiles = [
  "crates/api/src/main.rs",
  "crates/api/src/handlers/mod.rs",
  "crates/ingestion/src/router.rs",
  "crates/retrieval/src/lib.rs",
];

const specPaths = extractSpecPaths(readFileSync(resolve(repoRoot, "docs/openapi.yaml"), "utf8"));
const routerPaths = extractRouterPaths(routerFiles);

const missingFromSpec = [...routerPaths].filter((route) => !specPaths.has(route)).sort();
const missingFromBackend = [...specPaths].filter((route) => !routerPaths.has(route)).sort();

if (missingFromSpec.length > 0 || missingFromBackend.length > 0) {
  if (missingFromSpec.length > 0) {
    console.error("Backend routes missing from docs/openapi.yaml:");
    for (const route of missingFromSpec) console.error(`  ${route}`);
  }
  if (missingFromBackend.length > 0) {
    console.error("OpenAPI paths missing from backend route registrations:");
    for (const route of missingFromBackend) console.error(`  ${route}`);
  }
  process.exit(1);
}

console.log(`OpenAPI route parity OK (${routerPaths.size} paths).`);

function extractSpecPaths(source) {
  const paths = new Set();
  let inPaths = false;
  for (const line of source.split(/\r?\n/)) {
    if (line === "paths:") {
      inPaths = true;
      continue;
    }
    if (inPaths && line === "components:") {
      break;
    }
    if (!inPaths) {
      continue;
    }
    const match = line.match(/^  (\/[^:]+):\s*$/);
    if (match) {
      paths.add(match[1]);
    }
  }
  return paths;
}

function extractRouterPaths(files) {
  const paths = new Set();
  const routePattern = /\.route\(\s*"([^"]+)"/g;
  for (const file of files) {
    const source = readFileSync(resolve(repoRoot, file), "utf8");
    let match;
    while ((match = routePattern.exec(source)) !== null) {
      paths.add(match[1]);
    }
  }
  return paths;
}