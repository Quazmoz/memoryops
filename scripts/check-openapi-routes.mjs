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
const specOperations = extractSpecOperations(readFileSync(resolve(repoRoot, "docs/openapi.yaml"), "utf8"));
const routerOperations = extractRouterOperations(routerFiles);
const routerPaths = new Set([...routerOperations].map((operation) => operation.split(" ", 2)[1]));

const missingFromSpec = [...routerPaths].filter((route) => !specPaths.has(route)).sort();
const missingFromBackend = [...specPaths].filter((route) => !routerPaths.has(route)).sort();
const missingOperationsFromSpec = [...routerOperations]
  .filter((operation) => !specOperations.has(operation))
  .sort();
const missingOperationsFromBackend = [...specOperations]
  .filter((operation) => !routerOperations.has(operation))
  .sort();

if (
  missingFromSpec.length > 0
  || missingFromBackend.length > 0
  || missingOperationsFromSpec.length > 0
  || missingOperationsFromBackend.length > 0
) {
  if (missingFromSpec.length > 0) {
    console.error("Backend routes missing from docs/openapi.yaml:");
    for (const route of missingFromSpec) console.error(`  ${route}`);
  }
  if (missingFromBackend.length > 0) {
    console.error("OpenAPI paths missing from backend route registrations:");
    for (const route of missingFromBackend) console.error(`  ${route}`);
  }
  if (missingOperationsFromSpec.length > 0) {
    console.error("Backend route methods missing from docs/openapi.yaml:");
    for (const operation of missingOperationsFromSpec) console.error(`  ${operation}`);
  }
  if (missingOperationsFromBackend.length > 0) {
    console.error("OpenAPI methods missing from backend route registrations:");
    for (const operation of missingOperationsFromBackend) console.error(`  ${operation}`);
  }
  process.exit(1);
}

console.log(`OpenAPI route parity OK (${routerOperations.size} operations across ${routerPaths.size} paths).`);

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

function extractSpecOperations(source) {
  const operations = new Set();
  let inPaths = false;
  let currentPath = null;
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

    const pathMatch = line.match(/^  (\/[^:]+):\s*$/);
    if (pathMatch) {
      currentPath = pathMatch[1];
      continue;
    }

    const methodMatch = line.match(/^    (get|post|put|patch|delete|head|options|trace):\s*$/);
    if (currentPath && methodMatch) {
      operations.add(`${methodMatch[1].toUpperCase()} ${currentPath}`);
    }
  }
  return operations;
}

function extractRouterOperations(files) {
  const operations = new Set();
  const routePattern = /\.route\(\s*"([^"]+)"/g;
  for (const file of files) {
    const source = readFileSync(resolve(repoRoot, file), "utf8");
    let match;
    while ((match = routePattern.exec(source)) !== null) {
      const routePath = match[1];
      const routeCall = readRouteCall(source, match.index);
      for (const method of extractRouteMethods(routeCall)) {
        operations.add(`${method} ${routePath}`);
      }
    }
  }
  return operations;
}

function readRouteCall(source, startIndex) {
  let depth = 0;
  let inString = false;
  let escaped = false;

  for (let index = startIndex; index < source.length; index += 1) {
    const char = source[index];
    if (inString) {
      if (escaped) {
        escaped = false;
      } else if (char === "\\") {
        escaped = true;
      } else if (char === "\"") {
        inString = false;
      }
      continue;
    }

    if (char === "\"") {
      inString = true;
      continue;
    }
    if (char === "(") {
      depth += 1;
      continue;
    }
    if (char === ")") {
      depth -= 1;
      if (depth === 0) {
        return source.slice(startIndex, index + 1);
      }
    }
  }

  return source.slice(startIndex);
}

function extractRouteMethods(routeCall) {
  const methods = new Set();
  const methodPattern = /(?:^|[^\w])(get|post|put|patch|delete|head|options|trace)\s*\(/g;
  let match;
  while ((match = methodPattern.exec(routeCall)) !== null) {
    methods.add(match[1].toUpperCase());
  }
  return methods;
}
