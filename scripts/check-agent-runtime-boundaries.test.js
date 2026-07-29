import assert from "node:assert/strict";
import {
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { collectAgentRuntimeBoundaryFailures } from "./check-agent-runtime-boundaries.js";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

test("current Agent Runtime architecture satisfies the blocking boundaries", () => {
  assert.deepEqual(collectAgentRuntimeBoundaryFailures(REPO_ROOT), []);
});

test("source context DTO leak is rejected", () => {
  withRepositoryCopy((root) => {
    const target = resolve(
      root,
      "packages/app-web/src/features/session/ui/LeakedContext.ts",
    );
    writeFileSync(target, "export type Leaked = AgentContextSnapshot;\n");

    assert.ok(
      collectAgentRuntimeBoundaryFailures(root).some((failure) =>
        failure.includes("source context DTO leaked"),
      ),
    );
  });
});

test("terminal compaction projected as progress is rejected", () => {
  withRepositoryCopy((root) => {
    const target = resolve(
      root,
      "crates/agentdash-integration-native-agent/src/canonical_projection.rs",
    );
    const source = readFileSync(target, "utf8").replace(
      "HistoryPayload::CompactionCompleted",
      "HistoryPayload::CompactionCompleted /* BackboneEvent::ItemUpdated */",
    );
    writeFileSync(target, source);

    assert.ok(
      collectAgentRuntimeBoundaryFailures(root).some((failure) =>
        failure.includes("uses progress event as terminal evidence"),
      ),
    );
  });
});

function withRepositoryCopy(run) {
  const root = mkdtempSync(resolve(tmpdir(), "agentdash-runtime-guard-"));
  try {
    for (const path of [
      "crates/agentdash-agent-protocol/src/backbone/item.rs",
      "crates/agentdash-integration-native-agent/src/canonical_projection.rs",
      "crates/agentdash-integration-codex/src/canonical_projection.rs",
      "crates/agentdash-agent-runtime-contract/src/generate.rs",
      "crates/agentdash-agent-runtime-wire/src/generate.rs",
      "packages/app-web/src",
      "packages/app-web/src/generated/agent-runtime-codecs.ts",
      "packages/app-web/src/generated/agent-runtime-wire-codecs.ts",
      "schemas/agent-runtime-contract.manifest.json",
      "schemas/agent-runtime-wire.manifest.json",
    ]) {
      const target = resolve(root, path);
      mkdirSync(dirname(target), { recursive: true });
      cpSync(resolve(REPO_ROOT, path), target, { recursive: true });
    }
    run(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}
