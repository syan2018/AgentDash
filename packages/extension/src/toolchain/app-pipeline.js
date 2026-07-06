// @ts-check

import { mkdir, mkdtemp, readFile, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { build as esbuildBuild } from "esbuild";

import { startDevProject } from "./dev-server.js";
import { installProject } from "./install.js";
import {
  MANIFEST_FILE,
  asRecord,
  validateManifest,
  validatePackageJson,
  validateProject,
  validateRuntimeSurfaceParity,
} from "./manifest.js";
import { packProject } from "./pack.js";

export const APP_DEFINITION_FILE = "agentdash.app.ts";
export const GENERATED_DIR = ".agentdash/generated";
export const GENERATED_MANIFEST_FILE = `${GENERATED_DIR}/manifest.json`;
export const GENERATED_HOST_ENTRY_FILE = `${GENERATED_DIR}/extension.ts`;
export const GENERATED_PANEL_CLIENT_FILE = `${GENERATED_DIR}/client.ts`;
export const GENERATED_PERMISSION_SUMMARY_FILE = `${GENERATED_DIR}/permission-summary.json`;
export const GENERATED_PACKAGE_JSON_FILE = `${GENERATED_DIR}/package.json`;

const packageRoot = path.resolve(fileURLToPath(new URL("../..", import.meta.url)));
const zeroDigest = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
const appImportFilter = /^@agentdash\/extension(?:\/.*)?$/;

/**
 * @typedef {{ [key: string]: unknown }} UnknownRecord
 * @typedef {{ kind: "app", root: string, appPath: string } | { kind: "legacy", root: string, manifestPath: string } | { kind: "missing", root: string }} ExtensionProjectMode
 * @typedef {{ key: string, kind: string, title: string, description: string | null, config: UnknownRecord, permissions: UnknownRecord[], runtime_permissions: string[], diagnostics: string[] }} NormalizedCapability
 * @typedef {{ extension_id: string, package_name: string, version: string, display_name: string, panel_entry: string, capabilities: NormalizedCapability[], package_json: UnknownRecord, diagnostics: string[] }} NormalizedAppDefinition
 * @typedef {{ capability_key: string, kind: string, permissions: UnknownRecord[], runtime_permissions: string[], diagnostics: string[] }} PermissionSummaryItem
 * @typedef {{ manifest: UnknownRecord, package_json: UnknownRecord, host_entry: string, panel_client: string, permission_summary: PermissionSummaryItem[], diagnostics: string[], normalized: NormalizedAppDefinition, registered_surface: { runtime_actions: unknown[], protocol_channels: unknown[] } }} GeneratedAppArtifacts
 * @typedef {{ errors: string[], warnings: string[], manifest: UnknownRecord | null, package_json: UnknownRecord | null, generated?: GeneratedAppArtifacts, mode: "app" | "legacy" }} ExtensionValidationResult
 * @typedef {{ archive_path: string, archive_digest: string, manifest_digest: string, manifest: UnknownRecord, mode?: "app" | "legacy", generated?: GeneratedAppArtifacts, stage_root?: string }} ExtensionPackResult
 */

/**
 * @param {string} projectRoot
 * @returns {Promise<ExtensionProjectMode>}
 */
export async function resolveExtensionProjectMode(projectRoot) {
  const root = path.resolve(projectRoot);
  const appPath = path.join(root, APP_DEFINITION_FILE);
  if (await isFile(appPath)) {
    return { kind: "app", root, appPath };
  }
  const manifestPath = path.join(root, MANIFEST_FILE);
  if (await isFile(manifestPath)) {
    return { kind: "legacy", root, manifestPath };
  }
  return { kind: "missing", root };
}

/**
 * @param {string} projectRoot
 * @returns {Promise<boolean>}
 */
export async function hasAppDefinition(projectRoot) {
  return (await resolveExtensionProjectMode(projectRoot)).kind === "app";
}

/**
 * @param {string} projectRoot
 * @param {{ tempRoot?: string }} [options]
 * @returns {Promise<{ app: UnknownRecord, appPath: string }>}
 */
export async function loadAppDefinition(projectRoot, options = {}) {
  const mode = await resolveExtensionProjectMode(projectRoot);
  if (mode.kind !== "app") {
    throw new Error(`${APP_DEFINITION_FILE} 不存在，无法进入 Extension App 管线`);
  }
  const tempRoot = options.tempRoot ?? await mkdtemp(path.join(os.tmpdir(), "agentdash-app-definition-"));
  await mkdir(tempRoot, { recursive: true });
  const outfile = path.join(tempRoot, "agentdash-app.mjs");
  await esbuildBuild({
    entryPoints: [mode.appPath],
    outfile,
    bundle: true,
    platform: "node",
    format: "esm",
    target: "es2022",
    sourcemap: false,
    plugins: [agentdashExtensionSelfPlugin()],
  });
  const imported = await import(`${pathToFileURL(outfile).href}?v=${Date.now()}`);
  const app = asRecord(imported.default ?? imported.app);
  if (!app) {
    throw new Error(`${APP_DEFINITION_FILE} 需要 default export defineApp(...) 结果或 App definition 对象`);
  }
  return { app, appPath: mode.appPath };
}

/**
 * @param {string} projectRoot
 * @param {UnknownRecord} app
 * @returns {Promise<NormalizedAppDefinition>}
 */
export async function normalizeAppDefinition(projectRoot, app) {
  const root = path.resolve(projectRoot);
  const packageJson = await readPackageJson(root);
  /** @type {string[]} */
  const diagnostics = [];
  const extensionId = stringField(app, "id")
    ?? stringField(app, "extension_id")
    ?? packageExtensionId(packageJson);
  if (!extensionId) {
    diagnostics.push("App definition 缺少 id，已使用 extension-app 作为临时 extension_id");
  }
  const normalizedId = normalizePackageKey(extensionId ?? "extension-app");
  const version = stringField(app, "version") ?? stringField(packageJson, "version") ?? "0.0.0";
  const displayName = stringField(app, "name") ?? stringField(packageJson, "name") ?? normalizedId;
  const panel = asRecord(app.panel);
  const panelEntry = panel ? stringField(panel, "entry") : null;
  if (!panelEntry) {
    diagnostics.push("App definition 缺少 panel.entry，生成管线将使用 src/panel/index.ts 作为占位入口");
  }
  const capabilities = normalizeCapabilities(app.capabilities);
  return {
    extension_id: normalizedId,
    package_name: stringField(packageJson, "name") ?? `@agentdash/${normalizedId}`,
    version,
    display_name: displayName,
    panel_entry: panelEntry ?? "src/panel/index.ts",
    capabilities,
    package_json: {
      name: stringField(packageJson, "name") ?? `@agentdash/${normalizedId}`,
      version,
      type: "module",
    },
    diagnostics,
  };
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @returns {GeneratedAppArtifacts}
 */
export function generateAppArtifacts(normalized) {
  const permissions = dedupeRecords(normalized.capabilities.flatMap((capability) => capability.permissions));
  /** @type {unknown[]} */
  const runtimeActions = [];
  /** @type {unknown[]} */
  const protocolChannels = [];
  const manifest = compactRecord({
    manifest_version: "2",
    extension_id: normalized.extension_id,
    package: {
      name: normalized.package_name,
      version: normalized.version,
    },
    asset_version: normalized.version,
    runtime_actions: runtimeActions,
    protocol_channels: protocolChannels,
    workspace_tabs: [
      {
        type_id: `${normalized.extension_id}.panel`,
        label: normalized.display_name,
        uri_scheme: normalizeUriScheme(normalized.extension_id),
        renderer: { kind: "webview", entry: "dist/panel/index.html" },
      },
    ],
    permissions,
    bundles: [
      {
        kind: "extension_host",
        entry: "dist/extension.js",
        digest: zeroDigest,
      },
    ],
    fetch_routes: [],
    operation_catalog: [],
    backend_services: [],
  });
  const permissionSummary = normalized.capabilities.map((capability) => ({
    capability_key: capability.key,
    kind: capability.kind,
    permissions: capability.permissions,
    runtime_permissions: capability.runtime_permissions,
    diagnostics: capability.diagnostics,
  }));
  return {
    manifest,
    package_json: normalized.package_json,
    host_entry: createHostEntrySource(manifest),
    panel_client: createPanelClientSource(normalized),
    permission_summary: permissionSummary,
    diagnostics: normalized.diagnostics,
    normalized,
    registered_surface: {
      runtime_actions: [],
      protocol_channels: [],
    },
  };
}

/**
 * @param {string} projectRoot
 * @param {{ tempRoot?: string }} [options]
 * @returns {Promise<GeneratedAppArtifacts>}
 */
export async function generateAppProject(projectRoot, options = {}) {
  const loaded = await loadAppDefinition(projectRoot, options);
  const normalized = await normalizeAppDefinition(projectRoot, loaded.app);
  const generated = generateAppArtifacts(normalized);
  await writeGeneratedAppArtifacts(projectRoot, generated);
  return generated;
}

/**
 * @param {string} projectRoot
 * @param {GeneratedAppArtifacts} generated
 * @returns {Promise<void>}
 */
export async function writeGeneratedAppArtifacts(projectRoot, generated) {
  const root = path.resolve(projectRoot);
  const generatedRoot = path.join(root, GENERATED_DIR);
  await mkdir(generatedRoot, { recursive: true });
  await writeJson(path.join(root, GENERATED_MANIFEST_FILE), generated.manifest);
  await writeJson(path.join(root, GENERATED_PACKAGE_JSON_FILE), generated.package_json);
  await writeFile(path.join(root, GENERATED_HOST_ENTRY_FILE), generated.host_entry, "utf8");
  await writeFile(path.join(root, GENERATED_PANEL_CLIENT_FILE), generated.panel_client, "utf8");
  await writeJson(path.join(root, GENERATED_PERMISSION_SUMMARY_FILE), {
    generated_at: new Date().toISOString(),
    extension_id: generated.normalized.extension_id,
    capabilities: generated.permission_summary,
    diagnostics: generated.diagnostics,
  });
}

/**
 * @param {string} projectRoot
 * @param {{ requireBundles?: boolean, tempRoot?: string }} [options]
 * @returns {Promise<ExtensionValidationResult>}
 */
export async function validateExtensionProject(projectRoot, options = {}) {
  const mode = await resolveExtensionProjectMode(projectRoot);
  if (mode.kind === "app") {
    return await validateAppProject(projectRoot, options);
  }
  if (mode.kind === "legacy") {
    const legacy = await validateProject(projectRoot, { requireBundles: options.requireBundles });
    return { ...legacy, mode: "legacy" };
  }
  throw new Error(`${APP_DEFINITION_FILE} 或 ${MANIFEST_FILE} 不存在`);
}

/**
 * @param {string} projectRoot
 * @param {{ tempRoot?: string }} [options]
 * @returns {Promise<ExtensionValidationResult>}
 */
export async function validateAppProject(projectRoot, options = {}) {
  const generated = await generateAppProject(projectRoot, { tempRoot: options.tempRoot });
  /** @type {string[]} */
  const errors = [];
  validateManifest(generated.manifest, errors);
  validatePackageJson(generated.package_json, generated.manifest, errors);
  validateRuntimeSurfaceParity(generated.manifest, generated.registered_surface, errors);
  return {
    errors,
    warnings: [...generated.diagnostics],
    manifest: generated.manifest,
    package_json: generated.package_json,
    generated,
    mode: "app",
  };
}

/**
 * @param {string} projectRoot
 * @param {{ outDir?: string, tempRoot?: string }} [options]
 * @returns {Promise<ExtensionPackResult>}
 */
export async function packExtensionProject(projectRoot, options = {}) {
  const mode = await resolveExtensionProjectMode(projectRoot);
  if (mode.kind === "app") {
    return await packAppProject(projectRoot, options);
  }
  if (mode.kind === "legacy") {
    const packed = await packProject(projectRoot, { outDir: options.outDir });
    return { ...packed, mode: "legacy" };
  }
  throw new Error(`${APP_DEFINITION_FILE} 或 ${MANIFEST_FILE} 不存在`);
}

/**
 * @param {string} projectRoot
 * @param {{ outDir?: string, tempRoot?: string }} [options]
 * @returns {Promise<ExtensionPackResult>}
 */
export async function packAppProject(projectRoot, options = {}) {
  const root = path.resolve(projectRoot);
  const prepared = await prepareAppProjectForLegacyToolchain(root, { tempRoot: options.tempRoot });
  const outDir = path.resolve(root, options.outDir ?? "packed");
  const packed = await packProject(prepared.stageRoot, { outDir });
  const generated = {
    ...prepared.generated,
    manifest: packed.manifest,
  };
  await writeGeneratedAppArtifacts(root, generated);
  return {
    ...packed,
    mode: "app",
    generated,
    stage_root: prepared.stageRoot,
  };
}

/**
 * @param {string} projectRoot
 * @param {{ host?: string, port?: number, strictPort?: boolean, tempRoot?: string }} [options]
 * @returns {Promise<unknown>}
 */
export async function startExtensionProject(projectRoot, options = {}) {
  const mode = await resolveExtensionProjectMode(projectRoot);
  if (mode.kind === "app") {
    const prepared = await prepareAppProjectForLegacyToolchain(projectRoot, { tempRoot: options.tempRoot });
    return await startDevProject(prepared.stageRoot, {
      host: options.host,
      port: options.port,
      strictPort: options.strictPort,
      tempRoot: options.tempRoot,
    });
  }
  if (mode.kind === "legacy") {
    return await startDevProject(projectRoot, options);
  }
  throw new Error(`${APP_DEFINITION_FILE} 或 ${MANIFEST_FILE} 不存在`);
}

/**
 * @param {string} projectRoot
 * @param {{ apiUrl: string, projectId: string, token: string, archivePath?: string, extensionKey?: string, displayName?: string, overwrite?: boolean }} options
 * @returns {Promise<UnknownRecord>}
 */
export async function installExtensionProject(projectRoot, options) {
  const mode = await resolveExtensionProjectMode(projectRoot);
  if (mode.kind === "app" && !options.archivePath) {
    const packed = await packAppProject(projectRoot);
    return asRecord(await installProject(projectRoot, { ...options, archivePath: packed.archive_path })) ?? {};
  }
  if (mode.kind === "app" || mode.kind === "legacy") {
    return asRecord(await installProject(projectRoot, options)) ?? {};
  }
  throw new Error(`${APP_DEFINITION_FILE} 或 ${MANIFEST_FILE} 不存在`);
}

/**
 * @param {string} projectRoot
 * @param {{ tempRoot?: string }} [options]
 * @returns {Promise<{ stageRoot: string, generated: GeneratedAppArtifacts }>}
 */
export async function prepareAppProjectForLegacyToolchain(projectRoot, options = {}) {
  const root = path.resolve(projectRoot);
  const generated = await generateAppProject(root, { tempRoot: options.tempRoot });
  const stageRoot = options.tempRoot
    ? path.join(options.tempRoot, "legacy-toolchain-project")
    : await mkdtemp(path.join(os.tmpdir(), "agentdash-app-pack-"));
  await mkdir(path.join(stageRoot, "src"), { recursive: true });
  await writeJson(path.join(stageRoot, "agentdash.extension.json"), generated.manifest);
  await writeJson(path.join(stageRoot, "package.json"), generated.package_json);
  await writeFile(path.join(stageRoot, "src", "extension.ts"), generated.host_entry, "utf8");
  await writePanelStage(root, stageRoot, generated.normalized.panel_entry);
  return { stageRoot, generated };
}

/**
 * @param {string} root
 * @returns {Promise<UnknownRecord>}
 */
async function readPackageJson(root) {
  const packagePath = path.join(root, "package.json");
  if (!await isFile(packagePath)) return {};
  const parsed = JSON.parse(await readFile(packagePath, "utf8"));
  return asRecord(parsed) ?? {};
}

/**
 * @param {unknown} capabilities
 * @returns {NormalizedCapability[]}
 */
function normalizeCapabilities(capabilities) {
  const record = asRecord(capabilities) ?? {};
  return Object.entries(record).map(([key, value]) => normalizeCapability(key, value));
}

/**
 * @param {string} key
 * @param {unknown} value
 * @returns {NormalizedCapability}
 */
function normalizeCapability(key, value) {
  const config = asRecord(value) ?? {};
  const kind = normalizeCapabilityKind(
    stringField(config, "kind")
      ?? stringField(config, "type")
      ?? stringField(config, "recipe")
      ?? "unknown",
  );
  /** @type {string[]} */
  const diagnostics = [];
  const permissions = capabilityPermissions(kind, config, diagnostics);
  const runtimePermissions = capabilityRuntimePermissions(kind, config, diagnostics);
  return {
    key: normalizePropertyKey(key),
    kind,
    title: stringField(config, "name") ?? key,
    description: stringField(config, "description"),
    config,
    permissions,
    runtime_permissions: runtimePermissions,
    diagnostics,
  };
}

/**
 * @param {string} kind
 * @param {UnknownRecord} config
 * @param {string[]} diagnostics
 * @returns {UnknownRecord[]}
 */
function capabilityPermissions(kind, config, diagnostics) {
  if (kind === "http_proxy") {
    const host = hostFromBaseUrl(stringField(config, "baseUrl") ?? stringField(config, "base_url"));
    if (!host) {
      diagnostics.push("httpProxy capability 缺少可解析的 baseUrl");
      return [];
    }
    return [{ kind: "http", hosts: [host], access: normalizeAccess(stringField(config, "access")) }];
  }
  if (kind === "local_command") {
    return [{ kind: "process", access: "execute" }];
  }
  if (kind === "workspace_files") {
    return [{ kind: "workspace", access: normalizeAccess(stringField(config, "access")) }];
  }
  return [];
}

/**
 * @param {string} kind
 * @param {UnknownRecord} config
 * @param {string[]} diagnostics
 * @returns {string[]}
 */
function capabilityRuntimePermissions(kind, config, diagnostics) {
  if (kind === "http_proxy") {
    const host = hostFromBaseUrl(stringField(config, "baseUrl") ?? stringField(config, "base_url"));
    return host ? [`http.fetch:${host}`] : [];
  }
  if (kind === "local_command") {
    const mode = stringField(config, "mode");
    if (mode === "shell") return ["process.shell"];
    return ["process.exec"];
  }
  if (kind === "workspace_files") {
    const access = normalizeAccess(stringField(config, "access"));
    if (access === "read") return ["workspace.vfs.read", "workspace.vfs.list"];
    if (access === "write") return ["workspace.vfs.write"];
    return ["workspace.vfs.read", "workspace.vfs.write", "workspace.vfs.list"];
  }
  if (kind === "custom_channel" || kind === "backend_service") {
    diagnostics.push(`${kind} recipe 已进入 normalized model，manifest/runtime 生成将在后续切片补齐`);
  }
  return [];
}

/**
 * @param {string} value
 * @returns {string}
 */
function normalizeCapabilityKind(value) {
  const normalized = value.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`).replace(/[-\s]+/g, "_");
  if (normalized === "httpproxy") return "http_proxy";
  if (normalized === "localcommand") return "local_command";
  if (normalized === "workspacefiles") return "workspace_files";
  if (normalized === "customchannel") return "custom_channel";
  if (normalized === "backendservice") return "backend_service";
  return normalized;
}

/**
 * @param {string | null} raw
 * @returns {"read" | "write" | "read_write"}
 */
function normalizeAccess(raw) {
  if (raw === "read" || raw === "write" || raw === "read_write") return raw;
  return "read_write";
}

/**
 * @param {string | null} raw
 * @returns {string | null}
 */
function hostFromBaseUrl(raw) {
  if (!raw) return null;
  try {
    return new URL(raw).host;
  } catch {
    return null;
  }
}

/**
 * @param {UnknownRecord} packageJson
 * @returns {string | null}
 */
function packageExtensionId(packageJson) {
  const packageName = stringField(packageJson, "name");
  if (!packageName) return null;
  const last = packageName.split("/").pop();
  return last ? normalizePackageKey(last) : null;
}

/**
 * @param {UnknownRecord} manifest
 * @returns {string}
 */
function createHostEntrySource(manifest) {
  return [
    "// Generated by agentdash-ext generate.",
    'import { defineExtension } from "@agentdash/extension/host";',
    "",
    "export default defineExtension({",
    `  manifest: ${indentJson(manifest, 2)},`,
    "  activate() {",
    "  },",
    "});",
    "",
  ].join("\n");
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @returns {string}
 */
function createPanelClientSource(normalized) {
  return [
    "// Generated by agentdash-ext generate.",
    `export const extensionId = ${JSON.stringify(normalized.extension_id)};`,
    `export const capabilityKeys = ${JSON.stringify(normalized.capabilities.map((item) => item.key), null, 2)};`,
    "",
    "export function createAgentDashClient(bridge) {",
    "  return {",
    "    extensionId,",
    "    capabilityKeys,",
    "    invoke(actionKey, input) {",
    "      return bridge.runtime.invoke(actionKey, input);",
    "    },",
    "  };",
    "}",
    "",
  ].join("\n");
}

/**
 * @param {string} root
 * @param {string} stageRoot
 * @param {string} panelEntry
 * @returns {Promise<void>}
 */
async function writePanelStage(root, stageRoot, panelEntry) {
  const panelDir = path.join(stageRoot, "src", "panel");
  await mkdir(panelDir, { recursive: true });
  const sourceEntry = path.resolve(root, panelEntry);
  if (panelEntry.endsWith(".html") && await isFile(sourceEntry)) {
    await writeFile(path.join(panelDir, "index.html"), await readFile(sourceEntry, "utf8"), "utf8");
    return;
  }
  const mainFile = panelEntry.endsWith(".tsx") ? "main.tsx" : "main.ts";
  const relativeImport = normalizeImportPath(path.relative(panelDir, sourceEntry));
  await writeFile(
    path.join(panelDir, "index.html"),
    [
      '<!doctype html>',
      '<html lang="en">',
      "  <head><meta charset=\"UTF-8\"><title>AgentDash Extension</title></head>",
      '  <body><div id="root"></div><script type="module" src="./main.js"></script></body>',
      "</html>",
      "",
    ].join("\n"),
    "utf8",
  );
  await writeFile(path.join(panelDir, mainFile), `import ${JSON.stringify(relativeImport)};\n`, "utf8");
}

/**
 * @param {string} raw
 * @returns {string}
 */
function normalizeImportPath(raw) {
  const normalized = raw.replaceAll("\\", "/");
  return normalized.startsWith(".") ? normalized : `./${normalized}`;
}

/**
 * @returns {{ name: string, setup(build: { onResolve(options: { filter: RegExp }, callback: (args: { path: string }) => { path: string } | { external: boolean }) : void }): void }}
 */
function agentdashExtensionSelfPlugin() {
  return {
    name: "agentdash-extension-self",
    setup(build) {
      build.onResolve({ filter: appImportFilter }, (args) => {
        if (args.path === "@agentdash/extension") {
          return { path: path.join(packageRoot, "src", "app", "index.ts") };
        }
        if (args.path === "@agentdash/extension/react") {
          return { path: path.join(packageRoot, "src", "react", "index.ts") };
        }
        if (args.path === "@agentdash/extension/browser") {
          return { path: path.join(packageRoot, "src", "browser", "index.ts") };
        }
        if (args.path === "@agentdash/extension/host") {
          return { path: path.join(packageRoot, "src", "host", "index.ts") };
        }
        return { external: true };
      });
    },
  };
}

/**
 * @param {string} filePath
 * @returns {Promise<boolean>}
 */
async function isFile(filePath) {
  const info = await stat(filePath).catch(() => null);
  return Boolean(info?.isFile());
}

/**
 * @param {string} filePath
 * @param {UnknownRecord} value
 * @returns {Promise<void>}
 */
async function writeJson(filePath, value) {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

/**
 * @param {UnknownRecord} record
 * @returns {UnknownRecord}
 */
function compactRecord(record) {
  /** @type {UnknownRecord} */
  const result = {};
  for (const [key, value] of Object.entries(record)) {
    if (value === undefined) continue;
    if (Array.isArray(value) && value.length === 0) continue;
    result[key] = value;
  }
  return result;
}

/**
 * @param {UnknownRecord[]} records
 * @returns {UnknownRecord[]}
 */
function dedupeRecords(records) {
  const seen = new Set();
  const result = [];
  for (const record of records) {
    const key = JSON.stringify(record);
    if (seen.has(key)) continue;
    seen.add(key);
    result.push(record);
  }
  return result;
}

/**
 * @param {UnknownRecord} record
 * @param {string} field
 * @returns {string | null}
 */
function stringField(record, field) {
  const value = record[field];
  return typeof value === "string" && value.trim() !== "" ? value.trim() : null;
}

/**
 * @param {string} raw
 * @returns {string}
 */
function normalizePackageKey(raw) {
  return raw.toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "") || "extension-app";
}

/**
 * @param {string} raw
 * @returns {string}
 */
function normalizePropertyKey(raw) {
  return raw.replace(/[^A-Za-z0-9_$]+/g, "_") || "capability";
}

/**
 * @param {string} raw
 * @returns {string}
 */
function normalizeUriScheme(raw) {
  const normalized = raw.toLowerCase().replace(/[^a-z0-9+.-]+/g, "-").replace(/^[^a-z]+/, "");
  return normalized || "agentdash-extension";
}

/**
 * @param {unknown} value
 * @param {number} spaces
 * @returns {string}
 */
function indentJson(value, spaces) {
  return JSON.stringify(value, null, 2).split("\n").join(`\n${" ".repeat(spaces)}`);
}
