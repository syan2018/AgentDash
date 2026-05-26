// @ts-check

import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { installProject } from "./install.js";

test("installProject uploads archive and installs the returned artifact", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "agentdash-install-"));
  const archivePath = path.join(root, "extension.agentdash-extension.tgz");
  await writeFile(archivePath, Buffer.from("fake archive"));
  /** @type {Array<{ url: string, init: RequestInit }>} */
  const calls = [];
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (url, init = {}) => {
    calls.push({ url: String(url), init });
    if (calls.length === 1) {
      return new Response(JSON.stringify({ id: "artifact-1" }), { status: 200 });
    }
    return new Response(JSON.stringify({ installation_id: "install-1" }), { status: 200 });
  };
  try {
    const installed = await installProject(root, {
      apiUrl: "https://agentdash.test/",
      projectId: "project-1",
      token: "token-1",
      archivePath,
      extensionKey: "hello",
      displayName: "Hello",
      overwrite: true,
    });
    assert.deepEqual(installed, { installation_id: "install-1" });
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.equal(calls.length, 2);
  assert.equal(calls[0].url, "https://agentdash.test/api/projects/project-1/extension-artifacts");
  assert.equal(calls[0].init.method, "POST");
  assert.deepEqual(calls[0].init.headers, { Authorization: "Bearer token-1" });
  assert.equal(
    calls[1].url,
    "https://agentdash.test/api/projects/project-1/extension-artifacts/artifact-1/install",
  );
  assert.equal(calls[1].init.method, "POST");
  assert.deepEqual(calls[1].init.headers, {
    Authorization: "Bearer token-1",
    "Content-Type": "application/json",
  });
  const installBody = typeof calls[1].init.body === "string" ? calls[1].init.body : "{}";
  assert.deepEqual(JSON.parse(installBody), {
    extension_key: "hello",
    display_name: "Hello",
    overwrite: true,
  });
});
