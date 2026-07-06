// @ts-check

import test from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { parseFetchRouteBinding } from "../browser/fetch-route.js";
import { wrapWebapp, WrapWebappDiagnosticError } from "./wrap-webapp.js";

test("wrapWebapp packages a static dist with normalized definition and no-op host", async () => {
  const root = await fixtureDist({
    "index.html": "<main>Static app</main><script type=\"module\" src=\"/app.js\"></script>",
    "app.js": "document.querySelector('main').textContent = 'wrapped';",
  });
  const outDir = await mkdtemp(path.join(os.tmpdir(), "agentdash-wrap-out-"));

  const result = await wrapWebapp({
    dist: root,
    extensionId: "static-demo",
    name: "Static Demo",
    outDir,
  });

  assert.match(result.archive_digest, /^sha256:[0-9a-f]{64}$/);
  assert.match(result.archive_path, /static-demo-0\.1\.0\.agentdash-extension\.tgz$/);
  assert.equal((await stat(result.archive_path)).isFile(), true);
  assert.equal(result.normalized_definition.kind, "web_app_wrapper");
  assert.equal(result.normalized_definition.panel.entry, "dist/panel/index.html");
  assert.equal(result.normalized_definition.host.kind, "noop");
  assert.deepEqual(result.normalized_definition.fetch_routes, []);
  assert.equal(result.manifest.extension_id, "static-demo");
  assert.deepEqual(result.manifest.fetch_routes, []);
  assert.deepEqual(result.diagnostics, []);

  const normalizedOnDisk = JSON.parse(await readFile(result.normalized_definition_path, "utf8"));
  assert.equal(normalizedOnDisk.app.id, "static-demo");
});

test("wrapWebapp rejects /api fetch when no explicit route is declared", async () => {
  const root = await fixtureDist({
    "index.html": "<script type=\"module\" src=\"/app.js\"></script>",
    "app.js": "await fetch('/api/users');",
  });

  await assert.rejects(
    wrapWebapp({
      dist: root,
      extensionId: "api-demo",
      name: "API Demo",
      outDir: await mkdtemp(path.join(os.tmpdir(), "agentdash-wrap-out-")),
    }),
    (error) => {
      assert.equal(error instanceof WrapWebappDiagnosticError, true);
      assert.match(error instanceof Error ? error.message : String(error), /api_fetch_route_required/);
      return true;
    },
  );
});

test("wrapWebapp accepts /api fetch only when route target kind is explicit", async () => {
  const root = await fixtureDist({
    "index.html": "<script type=\"module\" src=\"/app.js\"></script>",
    "app.js": "await fetch('/api/users');",
  });
  const route = parseFetchRouteBinding("/api/**=httpProxy:https://api.example.com");

  const result = await wrapWebapp({
    dist: root,
    extensionId: "api-demo",
    name: "API Demo",
    outDir: await mkdtemp(path.join(os.tmpdir(), "agentdash-wrap-out-")),
    fetchRoutes: [route],
  });

  assert.equal(result.normalized_definition.fetch_routes[0]?.target.kind, "http_proxy");
  assert.deepEqual(result.manifest.permissions, [
    { kind: "http", hosts: ["api.example.com"], access: "read_write" },
  ]);
});

test("wrapWebapp rejects absolute localhost fetch without treating it as backendService", async () => {
  const root = await fixtureDist({
    "index.html": "<script type=\"module\" src=\"/app.js\"></script>",
    "app.js": "await fetch('http://localhost:4510/api/users');",
  });

  await assert.rejects(
    wrapWebapp({
      dist: root,
      extensionId: "local-api-demo",
      name: "Local API Demo",
      outDir: await mkdtemp(path.join(os.tmpdir(), "agentdash-wrap-out-")),
    }),
    /localhost_requires_explicit_route/,
  );
});

test("wrapWebapp does not cover absolute localhost with a relative api route", async () => {
  const root = await fixtureDist({
    "index.html": "<script type=\"module\" src=\"/app.js\"></script>",
    "app.js": "await fetch('http://localhost:4510/api/users');",
  });

  await assert.rejects(
    wrapWebapp({
      dist: root,
      extensionId: "local-api-demo",
      name: "Local API Demo",
      outDir: await mkdtemp(path.join(os.tmpdir(), "agentdash-wrap-out-")),
      fetchRoutes: [
        parseFetchRouteBinding("/api/**=httpProxy:https://api.example.com"),
      ],
    }),
    /localhost_requires_explicit_route/,
  );
});

test("wrapWebapp allows localhost only through an explicit matching route", async () => {
  const root = await fixtureDist({
    "index.html": "<script type=\"module\" src=\"/app.js\"></script>",
    "app.js": "await fetch('http://localhost:4510/api/users');",
  });

  const result = await wrapWebapp({
    dist: root,
    extensionId: "local-api-demo",
    name: "Local API Demo",
    outDir: await mkdtemp(path.join(os.tmpdir(), "agentdash-wrap-out-")),
    fetchRoutes: [
      parseFetchRouteBinding("http://localhost:4510/api/**=httpProxy:http://localhost:4510"),
    ],
  });

  assert.equal(result.normalized_definition.fetch_routes[0]?.target.kind, "http_proxy");
  assert.deepEqual(result.diagnostics, []);
});

/**
 * @param {Record<string, string>} files
 * @returns {Promise<string>}
 */
async function fixtureDist(files) {
  const root = await mkdtemp(path.join(os.tmpdir(), "agentdash-webapp-dist-"));
  for (const [relativePath, contents] of Object.entries(files)) {
    const filePath = path.join(root, relativePath);
    await mkdir(path.dirname(filePath), { recursive: true });
    await writeFile(filePath, contents);
  }
  return root;
}
