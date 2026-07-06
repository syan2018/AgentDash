import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const CRATES_DIR = resolve(REPO_ROOT, "crates");

const STATEFUL_PREFIX = /^(?:Memory|InMemory|Fake|Mock|Test)[A-Z0-9_]/;
const ADAPTER_NAME = /(?:Repository|Repo|Store)$/;
const STRUCT_PATTERN = /^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+([A-Za-z][A-Za-z0-9_]*)\b/;
const IMPL_PATTERN =
  /^\s*impl(?:<[^>]+>)?\s+(?<trait>[A-Za-z0-9_:<>,\s&'[\]]*?(?:Repository|Store))\s+for\s+(?<type>[A-Za-z][A-Za-z0-9_]*)\b/;

const ALLOWED_PATH_PREFIXES = [
  `crates${sep}agentdash-test-support${sep}`,
];

const failures = [];

for (const file of rustFiles(CRATES_DIR)) {
  const relativePath = relative(REPO_ROOT, file);
  if (isAllowedPath(relativePath)) {
    continue;
  }
  scanFile(file, relativePath);
}

if (failures.length > 0) {
  console.error("Test repository adapters must live in crates/agentdash-test-support.");
  console.error("Move reusable stateful fakes there, or give local one-off fixtures explicit fixture/recording/capturing/static/failing names.");
  console.error("");
  for (const failure of failures) {
    console.error(`${failure.path}:${failure.line}: ${failure.message}`);
    console.error(`  ${failure.source.trim()}`);
  }
  process.exit(1);
}

console.log("test-support boundaries ok");

function scanFile(file, relativePath) {
  const lines = readFileSync(file, "utf8").split(/\r?\n/);

  lines.forEach((line, index) => {
    const structMatch = line.match(STRUCT_PATTERN);
    if (structMatch) {
      const name = structMatch[1];
      if (isStatefulAdapterName(name)) {
        failures.push({
          path: relativePath,
          line: index + 1,
          message: `${name} is a stateful test adapter outside agentdash-test-support`,
          source: line,
        });
      }
    }

    const implMatch = line.match(IMPL_PATTERN);
    if (implMatch?.groups) {
      const typeName = implMatch.groups.type;
      if (STATEFUL_PREFIX.test(typeName)) {
        failures.push({
          path: relativePath,
          line: index + 1,
          message: `${typeName} implements ${implMatch.groups.trait.trim()} outside agentdash-test-support`,
          source: line,
        });
      }
    }
  });
}

function isStatefulAdapterName(name) {
  return STATEFUL_PREFIX.test(name) && ADAPTER_NAME.test(name);
}

function isAllowedPath(relativePath) {
  return ALLOWED_PATH_PREFIXES.some((prefix) => relativePath.startsWith(prefix));
}

function* rustFiles(dir) {
  for (const entry of readdirSync(dir)) {
    const fullPath = resolve(dir, entry);
    const stats = statSync(fullPath);
    if (stats.isDirectory()) {
      yield* rustFiles(fullPath);
      continue;
    }
    if (entry.endsWith(".rs")) {
      yield fullPath;
    }
  }
}
