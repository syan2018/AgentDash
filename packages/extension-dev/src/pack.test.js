// @ts-check

import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile, mkdir } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { packProject } from "./pack.js";

test("packProject builds a self-contained agentdash extension archive", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "agentdash-pack-"));
  await mkdir(path.join(root, "src", "panel"), { recursive: true });
  await writeFile(
    path.join(root, "src", "extension.ts"),
    [
      'import { defineExtension } from "@agentdash/extension-sdk";',
      "",
      "export default defineExtension({",
      "  activate() {},",
      "});",
      "",
    ].join("\n"),
  );
  await writeFile(path.join(root, "src", "panel", "index.html"), "<main>Hello</main>\n");
  await writeFile(
    path.join(root, "src", "panel", "main.ts"),
    [
      'const target = document.querySelector("main");',
      'if (target) target.textContent = "Hello panel bundle";',
      "",
    ].join("\n"),
  );
  await writeFile(path.join(root, "package.json"), JSON.stringify({
    name: "@agentdash/local-hello",
    version: "0.1.0",
    type: "module",
  }));
  await writeFile(path.join(root, "agentdash.extension.json"), JSON.stringify({
    manifest_version: "2",
    extension_id: "local-hello",
    package: { name: "@agentdash/local-hello", version: "0.1.0" },
    asset_version: "0.1.0",
    bundles: [
      {
        kind: "extension_host",
        entry: "dist/extension.js",
        digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
      },
    ],
  }));

  const packed = await packProject(root);
  assert.match(packed.archive_digest, /^sha256:[0-9a-f]{64}$/);
  assert.match(packed.archive_path, /\.agentdash-extension\.tgz$/);
  const manifest = JSON.parse(await readFile(path.join(root, "agentdash.extension.json"), "utf8"));
  assert.match(manifest.bundles[0].digest, /^sha256:[0-9a-f]{64}$/);

  const panelBundle = await readFile(path.join(root, "dist", "panel", "main.js"), "utf8");
  assert.match(panelBundle, /Hello panel bundle/);
  await assert.rejects(readFile(path.join(root, "dist", "panel", "main.ts"), "utf8"));
});

test("packProject rejects TS runtime registrations missing from manifest", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "agentdash-pack-parity-"));
  await mkdir(path.join(root, "src"), { recursive: true });
  await writeFile(
    path.join(root, "src", "extension.ts"),
    [
      'import { defineExtension } from "@agentdash/extension-sdk";',
      "",
      "export default defineExtension({",
      "  manifest: { manifest_version: '2', extension_id: 'local-hello', package: { name: '@agentdash/local-hello', version: '0.1.0' }, asset_version: '0.1.0' },",
      "  activate(ctx) {",
      "    ctx.runtime.registerAction({",
      "      action_key: 'local-hello.profile',",
      "      kind: 'session_runtime',",
      "      description: 'Read profile',",
      "      input_schema: true,",
      "      output_schema: true,",
      "      invoke() { return {}; },",
      "    });",
      "  },",
      "});",
      "",
    ].join("\n"),
  );
  await writeFile(path.join(root, "package.json"), JSON.stringify({
    name: "@agentdash/local-hello",
    version: "0.1.0",
    type: "module",
  }));
  await writeFile(path.join(root, "agentdash.extension.json"), JSON.stringify({
    manifest_version: "2",
    extension_id: "local-hello",
    package: { name: "@agentdash/local-hello", version: "0.1.0" },
    asset_version: "0.1.0",
    bundles: [{
      kind: "extension_host",
      entry: "dist/extension.js",
      digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    }],
  }));

  await assert.rejects(
    packProject(root),
    /TS 注册了 manifest 未声明的 runtime action: local-hello\.profile/,
  );
});
