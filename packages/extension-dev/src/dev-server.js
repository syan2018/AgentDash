// @ts-check

import { readFile } from "node:fs/promises";
import path from "node:path";

import react from "@vitejs/plugin-react";
import { createServer } from "vite";

import { MANIFEST_FILE, asRecord } from "./manifest.js";
import { createDevRuntime } from "./dev-runtime.js";
import { createPreviewHtml } from "./dev-preview.js";

const DEFAULT_HOST = "127.0.0.1";
const DEFAULT_PORT = 6200;
const BRIDGE_ENDPOINT = "/__agentdash_dev/bridge";
const STATUS_ENDPOINT = "/__agentdash_dev/status";
const PREVIEW_PATH = "/__agentdash_preview";

/**
 * @typedef {{ host?: string, port?: number, strictPort?: boolean, tempRoot?: string }} StartDevProjectOptions
 * @typedef {{ previewUrl: string, panelUrl: string, stop(): Promise<void>, runtime: import("./dev-runtime.js").ExtensionDevRuntime }} StartedDevProject
 */

/**
 * @param {string} projectRoot
 * @param {StartDevProjectOptions} [options]
 * @returns {Promise<StartedDevProject>}
 */
export async function startDevProject(projectRoot, options = {}) {
  const root = path.resolve(projectRoot);
  const runtime = createDevRuntime(root, { tempRoot: options.tempRoot });
  await runtime.load();
  const panel = await resolvePanel(root);
  const useReactPlugin = await hasPackageDependency(root, "react");
  const plugin = extensionDevPlugin({
    runtime,
    panelPath: panel.panelPath,
    extensionId: panel.extensionId,
    label: panel.label,
  });
  const server = await createServer({
    root,
    configFile: false,
    appType: "mpa",
    plugins: useReactPlugin ? [react(), plugin] : [plugin],
    server: {
      host: options.host ?? DEFAULT_HOST,
      port: options.port ?? DEFAULT_PORT,
      strictPort: options.strictPort ?? false,
    },
  });
  await server.listen();
  const baseUrl = firstLocalUrl(server.resolvedUrls?.local)
    ?? `http://${options.host ?? DEFAULT_HOST}:${options.port ?? DEFAULT_PORT}/`;
  return {
    previewUrl: new URL(PREVIEW_PATH, baseUrl).toString(),
    panelUrl: new URL(panel.panelPath, baseUrl).toString(),
    runtime,
    async stop() {
      await server.waitForRequestsIdle();
      await server.close();
    },
  };
}

/**
 * @param {{ runtime: import("./dev-runtime.js").ExtensionDevRuntime, panelPath: string, extensionId: string, label: string }} options
 * @returns {import("vite").Plugin}
 */
function extensionDevPlugin(options) {
  return {
    name: "agentdash-extension-dev",
    configureServer(server) {
      server.middlewares.use(async (req, res, next) => {
        const requestUrl = new URL(req.url ?? "/", "http://agentdash.local");
        if (req.method === "GET" && requestUrl.pathname === PREVIEW_PATH) {
          sendHtml(res, createPreviewHtml({
            extensionId: options.extensionId,
            label: options.label,
            panelPath: options.panelPath,
            bridgeEndpoint: BRIDGE_ENDPOINT,
          }));
          return;
        }
        if (req.method === "GET" && requestUrl.pathname === STATUS_ENDPOINT) {
          sendJson(res, 200, { result: options.runtime.status() });
          return;
        }
        if (req.method === "POST" && requestUrl.pathname === BRIDGE_ENDPOINT) {
          try {
            const body = asRecord(await readRequestJson(req)) ?? {};
            const method = typeof body.method === "string" ? body.method : "";
            const params = asRecord(body.params) ?? {};
            const result = await options.runtime.dispatch({ method, params });
            sendJson(res, 200, { result });
          } catch (error) {
            sendJson(res, 500, { error: error instanceof Error ? error.message : String(error) });
          }
          return;
        }
        next();
      });
    },
  };
}

/**
 * @param {string} root
 * @returns {Promise<{ panelPath: string, extensionId: string, label: string }>}
 */
async function resolvePanel(root) {
  const manifest = asRecord(JSON.parse(await readFile(path.join(root, MANIFEST_FILE), "utf8")));
  if (!manifest) {
    throw new Error(`${MANIFEST_FILE} 必须是对象`);
  }
  const extensionId = typeof manifest.extension_id === "string" ? manifest.extension_id : "extension";
  const tab = Array.isArray(manifest.workspace_tabs)
    ? manifest.workspace_tabs.map(asRecord).find(Boolean)
    : null;
  const label = tab && typeof tab.label === "string" ? tab.label : extensionId;
  return {
    extensionId,
    label,
    panelPath: "/src/panel/index.html",
  };
}

/**
 * @param {string} root
 * @param {string} dependencyName
 * @returns {Promise<boolean>}
 */
async function hasPackageDependency(root, dependencyName) {
  const packageJson = asRecord(JSON.parse(await readFile(path.join(root, "package.json"), "utf8")));
  const dependencies = asRecord(packageJson?.dependencies) ?? {};
  const devDependencies = asRecord(packageJson?.devDependencies) ?? {};
  return Object.prototype.hasOwnProperty.call(dependencies, dependencyName)
    || Object.prototype.hasOwnProperty.call(devDependencies, dependencyName);
}

/**
 * @param {string[] | undefined} values
 * @returns {string | null}
 */
function firstLocalUrl(values) {
  const value = values?.[0];
  return value ?? null;
}

/**
 * @param {import("node:http").IncomingMessage} req
 * @returns {Promise<unknown>}
 */
async function readRequestJson(req) {
  /** @type {Buffer[]} */
  const chunks = [];
  for await (const chunk of req) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  const raw = Buffer.concat(chunks).toString("utf8");
  return raw.trim() ? JSON.parse(raw) : {};
}

/**
 * @param {import("node:http").ServerResponse} res
 * @param {string} html
 */
function sendHtml(res, html) {
  res.statusCode = 200;
  res.setHeader("Content-Type", "text/html; charset=utf-8");
  res.end(html);
}

/**
 * @param {import("node:http").ServerResponse} res
 * @param {number} status
 * @param {unknown} body
 */
function sendJson(res, status, body) {
  res.statusCode = status;
  res.setHeader("Content-Type", "application/json; charset=utf-8");
  res.end(JSON.stringify(body));
}
