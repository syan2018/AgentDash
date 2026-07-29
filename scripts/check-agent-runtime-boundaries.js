import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const ABSENT_PATHS = [
  "crates/agentdash-agent-service-api/Cargo.toml",
  "crates/agentdash-agent-service-api/src/lib.rs",
  "crates/agentdash-agent-runtime-contract/src/agent_runtime_validators.ts",
  "crates/agentdash-agent-runtime-contract/src/complete_agent_codecs.ts",
  "crates/agentdash-agent-runtime-wire/src/runtime_wire_codecs.ts",
  "packages/app-web/src/generated/agent-service-api.ts",
  "packages/app-web/src/generated/agent-runtime-service-codecs.ts",
  "packages/app-web/src/generated/agent-runtime-validators.ts",
];

const SOURCE_DTO_OWNERS = [
  "crates/agentdash-application-agentrun/src",
  "crates/agentdash-api/src",
  "packages/app-web/src",
];

const GENERATED_CODEC_PROOFS = [
  {
    generator: "crates/agentdash-agent-runtime-contract/src/generate.rs",
    output: "packages/app-web/src/generated/agent-runtime-codecs.ts",
    manifest: "schemas/agent-runtime-contract.manifest.json",
  },
  {
    generator: "crates/agentdash-agent-runtime-wire/src/generate.rs",
    output: "packages/app-web/src/generated/agent-runtime-wire-codecs.ts",
    manifest: "schemas/agent-runtime-wire.manifest.json",
  },
];

export function collectAgentRuntimeBoundaryFailures(root = REPO_ROOT) {
  const failures = [];

  for (const path of ABSENT_PATHS) {
    if (existsSync(resolve(root, path))) {
      failures.push(`${path}: retired Agent Runtime path exists`);
    }
  }

  for (const owner of SOURCE_DTO_OWNERS) {
    const absoluteOwner = resolve(root, owner);
    if (!existsSync(absoluteOwner)) continue;
    for (const file of sourceFiles(absoluteOwner)) {
      const path = normalizePath(relative(root, file));
      if (path.includes("/generated/") || isTestFile(path)) continue;
      const source = readFileSync(file, "utf8");
      if (/\bAgentContext(?:Query|Snapshot)\b/.test(source)) {
        failures.push(`${path}: source context DTO leaked beyond Runtime contract/wire`);
      }
    }
  }

  const sessionUi = resolve(root, "packages/app-web/src/features/session/ui");
  if (existsSync(sessionUi)) {
    for (const file of sourceFiles(sessionUi)) {
      const path = normalizePath(relative(root, file));
      if (isTestFile(path)) continue;
      const source = readFileSync(file, "utf8");
      for (const pattern of [/\bisTerminalThreadItem\b/, /["']status["']\s+in\s+item/]) {
        if (pattern.test(source)) {
          failures.push(`${path}: renderer infers terminal state instead of consuming lifecycle`);
        }
      }
    }
  }

  checkTerminalProjection(root, failures);
  checkGeneratedEncoding(root, failures);

  return failures;
}

function checkTerminalProjection(root, failures) {
  const protocolPath = "crates/agentdash-agent-protocol/src/backbone/item.rs";
  const protocol = readRequired(root, protocolPath, failures);
  if (protocol && !/struct ItemCompletedNotification\s*\{[\s\S]*?pub terminal: AgentDashItemTerminal,/.test(protocol)) {
    failures.push(`${protocolPath}: ItemCompletedNotification lacks typed terminal evidence`);
  }

  const projectionPath =
    "crates/agentdash-integration-native-agent/src/canonical_projection.rs";
  const projection = readRequired(root, projectionPath, failures);
  if (!projection) return;

  for (const variant of ["CompactionCompleted", "CompactionFailed", "CompactionCancelled"]) {
    const arm = historyArm(projection, variant);
    if (!arm) {
      failures.push(`${projectionPath}: missing ${variant} projection arm`);
      continue;
    }
    if (!arm.includes("BackboneEvent::ItemCompleted")) {
      failures.push(`${projectionPath}: ${variant} does not close the canonical item lifecycle`);
    }
    if (arm.includes("BackboneEvent::ItemUpdated")) {
      failures.push(`${projectionPath}: ${variant} uses progress event as terminal evidence`);
    }
  }

  const codexPath =
    "crates/agentdash-integration-codex/src/canonical_projection.rs";
  const codex = readRequired(root, codexPath, failures);
  if (!codex) return;
  const liveTerminal = codex.slice(
    codex.indexOf("if let Source::TurnCompleted"),
    codex.indexOf("let mapped = match notification"),
  );
  if (
    !liveTerminal.includes("BackboneEvent::ItemCompleted(terminal)")
    || !liveTerminal.includes("BackboneEvent::TurnCompleted")
  ) {
    failures.push(`${codexPath}: Codex turn terminal does not close compaction before the turn`);
  }
  if (
    !/Source::ItemCompleted\(value\)[\s\S]*?ContextCompaction[\s\S]*?\bNone\b/.test(codex)
  ) {
    failures.push(`${codexPath}: vendor compaction terminal is not normalized by the canonical owner`);
  }
}

function checkGeneratedEncoding(root, failures) {
  for (const proof of GENERATED_CODEC_PROOFS) {
    const generator = readRequired(root, proof.generator, failures);
    const output = readRequired(root, proof.output, failures);
    const manifest = readRequired(root, proof.manifest, failures);

    if (generator && !generator.includes("generate_codec_module")) {
      failures.push(`${proof.generator}: codec output is not owned by shared schema traversal`);
    }
    if (output && !output.includes("const ENCODING_PLANS")) {
      failures.push(`${proof.output}: generated recursive encoding plan is missing`);
    }
    if (manifest) {
      const parsed = JSON.parse(manifest);
      if (!parsed.root || !parsed.input || !parsed.outputs || !parsed.digest) {
        failures.push(`${proof.manifest}: generation identity is incomplete`);
      }
    }
  }
}

function historyArm(source, variant) {
  const start = source.indexOf(`HistoryPayload::${variant}`);
  if (start < 0) return null;
  const next = source.indexOf("\n        HistoryPayload::", start + 1);
  return source.slice(start, next < 0 ? source.length : next);
}

function readRequired(root, path, failures) {
  const absolutePath = resolve(root, path);
  if (!existsSync(absolutePath)) {
    failures.push(`${path}: required architecture proof is missing`);
    return null;
  }
  return readFileSync(absolutePath, "utf8");
}

function* sourceFiles(dir) {
  for (const entry of readdirSync(dir)) {
    const path = resolve(dir, entry);
    const stats = statSync(path);
    if (stats.isDirectory()) {
      yield* sourceFiles(path);
    } else if (/\.(?:rs|ts|tsx)$/.test(entry)) {
      yield path;
    }
  }
}

function isTestFile(path) {
  return /\.(?:test|spec)\.(?:rs|ts|tsx)$/.test(path) || path.includes("/tests/");
}

function normalizePath(path) {
  return path.replaceAll("\\", "/");
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const failures = collectAgentRuntimeBoundaryFailures();
  if (failures.length > 0) {
    console.error("Agent Runtime architecture boundaries are not closed:");
    for (const failure of failures) console.error(`- ${failure}`);
    process.exit(1);
  }
  console.log("agent runtime boundaries ok");
}
