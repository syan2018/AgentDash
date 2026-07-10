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
  validateBackendServiceEntryRefs,
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
 * @typedef {{ key: string, wire_key: string, kind: string, title: string, description: string, permission_summary: string[] }} NormalizedCapability
 * @typedef {{ kind: "agentdash.app", id: string, name: string, version: string, description: string, panel: UnknownRecord, ui_components: UnknownRecord[], capabilities: NormalizedCapability[], agent_exposures: UnknownRecord[], dispatches: UnknownRecord[], artifacts: UnknownRecord[], operation_catalog: UnknownRecord[], permission_summary: UnknownRecord, package_name: string, extension_id: string, panel_entry: string, package_json: UnknownRecord, diagnostics: string[] }} NormalizedAppDefinition
 * @typedef {{ capability_key: string, kind: string, permissions: UnknownRecord[], runtime_permissions: string[], diagnostics: string[] }} PermissionSummaryItem
 * @typedef {{ manifest: UnknownRecord, package_json: UnknownRecord, host_entry: string, panel_client: string, permission_summary: PermissionSummaryItem[], diagnostics: string[], normalized: NormalizedAppDefinition, registered_surface: { runtime_actions: unknown[], protocols: unknown[] } }} GeneratedAppArtifacts
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
  const normalized = normalizeLoadedAppModel(app);
  const extensionId = normalizePackageKey(normalized.id);
  const packageName = stringField(packageJson, "name") ?? `@agentdash/${extensionId}`;
  const version = stringField(packageJson, "version") ?? normalized.version;
  return {
    ...normalized,
    extension_id: extensionId,
    package_name: packageName,
    version,
    panel_entry: stringField(normalized.panel, "entry") ?? "src/panel/index.ts",
    package_json: {
      name: packageName,
      version,
      type: "module",
    },
    diagnostics: [],
  };
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @returns {GeneratedAppArtifacts}
 */
export function generateAppArtifacts(normalized) {
  const permissions = normalizedPermissions(normalized);
  const runtimeActions = runtimeActionsFromDispatches(normalized);
  const protocols = protocolsFromDispatches(normalized);
  const backendServices = backendServicesFromArtifacts(normalized);
  const fetchRoutes = fetchRoutesFromBackendServices(backendServices);
  const operationCatalog = operationCatalogFromNormalized(normalized);
  const manifest = compactRecord({
    manifest_version: "2",
    extension_id: normalized.extension_id,
    package: {
      name: normalized.package_name,
      version: normalized.version,
    },
    asset_version: normalized.version,
    runtime_actions: runtimeActions,
    protocols: protocols,
    workspace_tabs: [
      {
        type_id: stringField(normalized.panel, "type_id") ?? `${normalized.extension_id}.panel`,
        label: stringField(normalized.panel, "title") ?? normalized.name,
        uri_scheme: stringField(normalized.panel, "uri_scheme") ?? normalizeUriScheme(normalized.extension_id),
        renderer: { kind: "webview", entry: "dist/panel/index.html" },
      },
    ],
    ui_components: normalized.ui_components.map((component) => ({
      ...component,
      renderer: {
        kind: "iframe",
        entry: `dist/components/${requireStringField(component, "component_key")}/index.html`,
      },
    })),
    permissions,
    bundles: [
      {
        kind: "extension_host",
        entry: "dist/extension.js",
        digest: zeroDigest,
      },
    ],
    fetch_routes: fetchRoutes,
    operation_catalog: operationCatalog,
    backend_services: backendServices,
  });
  const permissionSummary = permissionSummaryItems(normalized);
  return {
    manifest,
    package_json: normalized.package_json,
    host_entry: createHostEntrySource(manifest, normalized),
    panel_client: createPanelClientSource(normalized),
    permission_summary: permissionSummary,
    diagnostics: normalized.diagnostics,
    normalized,
    registered_surface: {
      runtime_actions: runtimeActions,
      protocols: protocols,
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
  await validateBackendServiceEntryRefs(projectRoot, generated.manifest, errors);
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
  await writeComponentStageEntries(root, stageRoot, generated.normalized.ui_components);
  await writeBackendServiceStageEntries(root, stageRoot, generated.manifest);
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
 * @param {UnknownRecord} app
 * @returns {NormalizedAppDefinition}
 */
function normalizeLoadedAppModel(app) {
  if (app.kind !== "agentdash.app") {
    throw new Error(`${APP_DEFINITION_FILE} 必须 default export defineApp(...) 产物`);
  }
  const panel = asRecord(app.panel);
  const permissionSummary = asRecord(app.permission_summary);
  if (!panel) throw new Error("defineApp normalized model 缺少 panel 对象");
  if (!permissionSummary) throw new Error("defineApp normalized model 缺少 permission_summary 对象");
  return {
    kind: "agentdash.app",
    id: requireStringField(app, "id"),
    name: requireStringField(app, "name"),
    version: requireStringField(app, "version"),
    description: stringField(app, "description") ?? "",
    panel,
    ui_components: arrayRecordField(app, "ui_components"),
    capabilities: arrayRecordField(app, "capabilities").map((capability) => ({
      key: requireStringField(capability, "key"),
      wire_key: requireStringField(capability, "wire_key"),
      kind: requireStringField(capability, "kind"),
      title: requireStringField(capability, "title"),
      description: stringField(capability, "description") ?? "",
      permission_summary: stringListField(capability, "permission_summary"),
    })),
    agent_exposures: arrayRecordField(app, "agent_exposures"),
    dispatches: arrayRecordField(app, "dispatches"),
    artifacts: arrayRecordField(app, "artifacts"),
    operation_catalog: arrayRecordField(app, "operation_catalog"),
    permission_summary: permissionSummary,
    extension_id: "",
    package_name: "",
    panel_entry: "",
    package_json: {},
    diagnostics: [],
  };
}

/**
 * @param {string} root
 * @param {string} stageRoot
 * @param {UnknownRecord[]} components
 * @returns {Promise<void>}
 */
async function writeComponentStageEntries(root, stageRoot, components) {
  for (const component of components) {
    const componentKey = requireStringField(component, "component_key");
    const renderer = asRecord(component.renderer);
    const sourceEntry = renderer ? stringField(renderer, "entry") : null;
    if (!sourceEntry) throw new Error(`ui_components[${componentKey}].renderer.entry 缺失`);
    const componentDir = path.join(stageRoot, "src", "components", componentKey);
    await mkdir(componentDir, { recursive: true });
    const resolved = path.resolve(root, sourceEntry);
    if (sourceEntry.endsWith(".html") && await isFile(resolved)) {
      await writeFile(path.join(componentDir, "index.html"), await readFile(resolved, "utf8"), "utf8");
      continue;
    }
    const mainFile = sourceEntry.endsWith(".tsx") ? "main.tsx" : "main.ts";
    const relativeImport = normalizeImportPath(path.relative(componentDir, resolved));
    await writeFile(
      path.join(componentDir, "index.html"),
      '<!doctype html>\n<html><head><meta charset="UTF-8"></head><body><div id="root"></div><script type="module" src="./main.js"></script></body></html>\n',
      "utf8",
    );
    await writeFile(path.join(componentDir, mainFile), `import ${JSON.stringify(relativeImport)};\n`, "utf8");
  }
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @returns {UnknownRecord[]}
 */
function normalizedPermissions(normalized) {
  return dedupeRecords(arrayRecordField(normalized.permission_summary, "declarations"));
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @returns {unknown[]}
 */
function runtimeActionsFromDispatches(normalized) {
  const result = [];
  for (const projection of normalized.dispatches) {
    const dispatch = asRecord(projection.dispatch);
    if (!dispatch || dispatch.kind !== "runtime_action") continue;
    const actionKey = requireStringField(dispatch, "action_key");
    const operation = firstOperationForRuntimeAction(normalized, actionKey);
    const capability = capabilityByKey(normalized, stringField(projection, "capability_key"));
    result.push({
      action_key: actionKey,
      kind: "session_runtime",
      description: stringField(operation ?? {}, "description")
        ?? capability?.description
        ?? capability?.title
        ?? actionKey,
      input_schema: schemaField(operation ?? {}, "input_schema"),
      output_schema: schemaField(operation ?? {}, "output_schema"),
      permissions: stringListField(projection, "runtime_permissions"),
    });
  }
  return result;
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @returns {unknown[]}
 */
function protocolsFromDispatches(normalized) {
  const result = [];
  for (const projection of normalized.dispatches) {
    const dispatch = asRecord(projection.dispatch);
    if (!dispatch || dispatch.kind !== "protocol_method") continue;
    result.push({
      protocol_key: requireStringField(dispatch, "protocol_key"),
      version: stringField(dispatch, "version") ?? "1.0.0",
      description: stringField(dispatch, "description") ?? requireStringField(dispatch, "protocol_key"),
      methods: arrayRecordField(dispatch, "methods").map((method) => ({
        name: requireStringField(method, "name"),
        description: requireStringField(method, "description"),
        input_schema: schemaField(method, "input_schema"),
        output_schema: schemaField(method, "output_schema"),
        permissions: stringListField(method, "permissions"),
      })),
    });
  }
  return result;
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @returns {UnknownRecord[]}
 */
function backendServicesFromArtifacts(normalized) {
  return normalized.artifacts
    .filter((artifact) => artifact.kind === "backend_service")
    .map((artifact) => compactRecord({
      service_key: requireStringField(artifact, "service_key"),
      runtime: stringField(artifact, "runtime") ?? "node",
      entry: requireStringField(artifact, "entry"),
      routes: stringListField(artifact, "routes"),
      health_path: stringField(artifact, "health_path"),
    }));
}

/**
 * @param {UnknownRecord[]} backendServices
 * @returns {UnknownRecord[]}
 */
function fetchRoutesFromBackendServices(backendServices) {
  return backendServices.flatMap((service) => stringListField(service, "routes").map((route, index) => ({
    route_key: routeKeyFromPattern(requireStringField(service, "service_key"), route, index),
    route,
    scope: "panel_only",
    target: {
      kind: "backend_service",
      service_key: requireStringField(service, "service_key"),
      route,
    },
  })));
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @returns {UnknownRecord[]}
 */
function operationCatalogFromNormalized(normalized) {
  return normalized.operation_catalog.map((operation) => {
    const dispatch = operationDispatchForManifest(normalized, asRecord(operation.dispatch) ?? {});
    const provenance = asRecord(operation.provenance) ?? {};
    return {
      operation_key: requireStringField(operation, "operation_key"),
      visibility: stringField(operation, "visibility") ?? "agent_and_panel",
      origin: stringField(operation, "origin") ?? "capability_exposure",
      description: requireStringField(operation, "description"),
      input_schema: schemaField(operation, "input_schema"),
      output_schema: schemaField(operation, "output_schema"),
      permission_summary: stringListField(operation, "permission_summary"),
      dispatch,
      readiness: asRecord(operation.readiness) ?? { kind: "ready" },
      provenance: {
        capability_key: requireStringField(provenance, "capability_key"),
        exposure_key: stringField(provenance, "exposure_key") ?? requireStringField(operation, "operation_key"),
        generated_from: stringField(provenance, "generated_from") ?? stringField(provenance, "source") ?? "capability_exposure",
      },
    };
  });
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @param {UnknownRecord} dispatch
 * @returns {UnknownRecord}
 */
function operationDispatchForManifest(normalized, dispatch) {
  const kind = stringField(dispatch, "kind");
  if (kind === "runtime_action") {
    return {
      kind,
      action_key: requireStringField(dispatch, "action_key"),
    };
  }
  if (kind === "protocol_method") {
    return {
      kind,
      protocol_key: requireStringField(dispatch, "protocol_key"),
      method: stringField(dispatch, "method") ?? requireStringField(dispatch, "method_name"),
    };
  }
  if (kind === "backend_service") {
    const serviceKey = requireStringField(dispatch, "service_key");
    return {
      kind,
      service_key: serviceKey,
      route: stringField(dispatch, "route") ?? firstBackendServiceRoute(normalized, serviceKey),
    };
  }
  throw new Error(`operation_catalog[].dispatch.kind 非法: ${kind ?? "<missing>"}`);
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @returns {PermissionSummaryItem[]}
 */
function permissionSummaryItems(normalized) {
  const byCapability = arrayRecordField(normalized.permission_summary, "by_capability");
  return byCapability.map((entry) => {
    const permissions = arrayRecordField(entry, "permissions");
    return {
      capability_key: requireStringField(entry, "capability_key"),
      kind: requireStringField(entry, "capability_kind"),
      permissions,
      runtime_permissions: permissions
        .map((permission) => stringField(permission, "runtime_permission"))
        .filter(isString),
      diagnostics: [],
    };
  });
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @param {string} actionKey
 * @returns {UnknownRecord | null}
 */
function firstOperationForRuntimeAction(normalized, actionKey) {
  return normalized.operation_catalog.find((operation) => {
    const dispatch = asRecord(operation.dispatch);
    return dispatch?.kind === "runtime_action" && stringField(dispatch, "action_key") === actionKey;
  }) ?? null;
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @param {string | null} capabilityKey
 * @returns {NormalizedCapability | null}
 */
function capabilityByKey(normalized, capabilityKey) {
  if (!capabilityKey) return null;
  return normalized.capabilities.find((capability) => capability.key === capabilityKey) ?? null;
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @param {string} serviceKey
 * @returns {string}
 */
function firstBackendServiceRoute(normalized, serviceKey) {
  const service = backendServicesFromArtifacts(normalized)
    .find((candidate) => stringField(candidate, "service_key") === serviceKey);
  const route = service ? stringListField(service, "routes")[0] : null;
  if (!route) throw new Error(`backend_service operation ${serviceKey} 缺少 route`);
  return route;
}

/**
 * @param {UnknownRecord} manifest
 * @param {NormalizedAppDefinition} normalized
 * @returns {string}
 */
function createHostEntrySource(manifest, normalized) {
  const runtimeActions = runtimeActionsFromDispatches(normalized);
  const protocols = protocolsFromDispatches(normalized);
  const actionRegistrations = runtimeActions.map((action) => {
    const dispatch = dispatchForRuntimeAction(normalized, requireStringField(asRecord(action) ?? {}, "action_key"));
    return [
      "    ctx.runtime.registerAction({",
      objectPropertiesSource(action, 6),
      "      async invoke(input) {",
      `        return invokeGeneratedRuntimeAction(ctx, ${indentJson(dispatch, 8)}, input);`,
      "      },",
      "    });",
    ].join("\n");
  });
  const protocolRegistrations = protocols.map((channel) => [
    "    ctx.protocols.register({",
    `      protocol_key: ${JSON.stringify(requireStringField(asRecord(channel) ?? {}, "protocol_key"))},`,
    `      version: ${JSON.stringify(stringField(asRecord(channel) ?? {}, "version") ?? "1.0.0")},`,
    `      description: ${JSON.stringify(stringField(asRecord(channel) ?? {}, "description") ?? "")},`,
    "      methods: {",
    ...arrayRecordField(asRecord(channel) ?? {}, "methods").map((method) => [
      `        ${JSON.stringify(requireStringField(method, "name"))}: {`,
      `          description: ${JSON.stringify(requireStringField(method, "description"))},`,
      `          input_schema: ${JSON.stringify(schemaField(method, "input_schema"))},`,
      `          output_schema: ${JSON.stringify(schemaField(method, "output_schema"))},`,
      `          permissions: ${JSON.stringify(stringListField(method, "permissions"))},`,
      "          async invoke(input) {",
      `            return invokeGeneratedProtocolMethod(${JSON.stringify(requireStringField(asRecord(channel) ?? {}, "protocol_key"))}, ${JSON.stringify(requireStringField(method, "name"))}, input);`,
      "          },",
      "        },",
    ].join("\n")),
    "      },",
    "    });",
  ].join("\n"));
  const body = [...actionRegistrations, ...protocolRegistrations].join("\n");
  return [
    "// Generated by agentdash-ext generate.",
    'import { defineExtension } from "@agentdash/extension/host";',
    "",
    generatedHostHelpersSource(),
    "",
    "export default defineExtension({",
    `  manifest: ${indentJson(manifest, 2)},`,
    "  activate(ctx) {",
    body || "    void ctx;",
    "  },",
    "});",
    "",
  ].join("\n");
}

/**
 * @param {NormalizedAppDefinition} normalized
 * @param {string} actionKey
 * @returns {UnknownRecord}
 */
function dispatchForRuntimeAction(normalized, actionKey) {
  for (const projection of normalized.dispatches) {
    const dispatch = asRecord(projection.dispatch);
    if (!dispatch || dispatch.kind !== "runtime_action") continue;
    if (stringField(dispatch, "action_key") === actionKey) return dispatch;
  }
  throw new Error(`runtime action dispatch 不存在: ${actionKey}`);
}

/**
 * @param {unknown} value
 * @param {number} spaces
 * @returns {string}
 */
function objectPropertiesSource(value, spaces) {
  const record = asRecord(value);
  if (!record) throw new Error("generated host registration must be an object");
  return Object.entries(record)
    .map(([key, item]) => `${" ".repeat(spaces)}${key}: ${JSON.stringify(item)},`)
    .join("\n");
}

/**
 * @returns {string}
 */
function generatedHostHelpersSource() {
  return [
    "function asObject(value) {",
    "  return value && typeof value === \"object\" && !Array.isArray(value) ? value : {};",
    "}",
    "",
    "function joinUrl(baseUrl, path) {",
    "  if (!path) return baseUrl;",
    "  return `${baseUrl.replace(/\\/$/, \"\")}/${String(path).replace(/^\\/+/, \"\")}`;",
    "}",
    "",
    "function processOptions(config) {",
    "  return { cwd: config.cwd, env: config.env, timeout_ms: config.timeout_ms };",
    "}",
    "",
    "function shellCommand(config) {",
    "  return [config.command, ...(config.args ?? [])].join(\" \");",
    "}",
    "",
    "async function invokeGeneratedRuntimeAction(ctx, dispatch, input) {",
    "  const payload = asObject(input);",
    "  if (dispatch.host_api === \"http.fetch\") {",
    "    const request = asObject(payload.options);",
    "    return ctx.api.http.fetch(joinUrl(dispatch.http.base_url, payload.path), request);",
    "  }",
    "  if (dispatch.host_api === \"process.exec\") {",
    "    return ctx.api.process.exec(dispatch.command.command, dispatch.command.args ?? [], processOptions(dispatch.command));",
    "  }",
    "  if (dispatch.host_api === \"process.shell\") {",
    "    return ctx.api.process.shell(shellCommand(dispatch.command), processOptions(dispatch.command));",
    "  }",
    "  if (dispatch.host_api === \"workspace.vfs\") {",
    "    const operation = payload.operation ?? payload.method ?? \"stat\";",
    "    const targetPath = typeof payload.path === \"string\" ? payload.path : \".\";",
    "    if (operation === \"read\" || operation === \"read_text\") return ctx.api.workspace.readText(targetPath);",
    "    if (operation === \"write\" || operation === \"write_text\") return ctx.api.workspace.writeText(targetPath, String(payload.content ?? \"\"));",
    "    if (operation === \"list\") return ctx.api.workspace.list(targetPath);",
    "    if (operation === \"stat\") return ctx.api.workspace.stat(targetPath);",
    "    throw new Error(`Unsupported workspace.vfs operation: ${operation}`);",
    "  }",
    "  throw new Error(`Unsupported generated runtime action host API: ${dispatch.host_api}`);",
    "}",
    "",
    "async function invokeGeneratedProtocolMethod(protocolKey, methodName) {",
    "  throw new Error(`Generated protocol ${protocolKey}.${methodName} requires host implementation`);",
    "}",
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
 * @param {string} root
 * @param {string} stageRoot
 * @param {UnknownRecord} manifest
 * @returns {Promise<void>}
 */
async function writeBackendServiceStageEntries(root, stageRoot, manifest) {
  for (const service of arrayRecordField(manifest, "backend_services")) {
    const entry = stringField(service, "entry");
    if (!entry) continue;
    const sourceEntry = resolvePackageRelativePath(root, entry, "backend_services[].entry");
    if (!await isFile(sourceEntry)) {
      throw new Error(`backend_services[].entry 文件不存在: ${entry}`);
    }
    const targetEntry = resolvePackageRelativePath(stageRoot, entry, "backend_services[].entry");
    await mkdir(path.dirname(targetEntry), { recursive: true });
    const relativeImport = normalizeImportPath(path.relative(path.dirname(targetEntry), sourceEntry));
    await writeFile(targetEntry, `import ${JSON.stringify(relativeImport)};\n`, "utf8");
  }
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
 * @param {string} root
 * @param {string} entry
 * @param {string} label
 * @returns {string}
 */
function resolvePackageRelativePath(root, entry, label) {
  if (
    entry.includes("\\")
    || entry.includes("\0")
    || entry.startsWith("/")
    || path.isAbsolute(entry)
    || entry.split("/").some((segment) => segment === "" || segment === "." || segment === "..")
  ) {
    throw new Error(`${label} 必须是 package 内相对文件路径: ${entry}`);
  }
  return path.join(root, ...entry.split("/"));
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
 * @param {UnknownRecord} record
 * @param {string} field
 * @returns {string}
 */
function requireStringField(record, field) {
  const value = stringField(record, field);
  if (!value) throw new Error(`${field} 不能为空`);
  return value;
}

/**
 * @param {UnknownRecord | undefined} record
 * @param {string} field
 * @returns {UnknownRecord[]}
 */
function arrayRecordField(record, field) {
  if (!record) return [];
  const values = record[field];
  if (!Array.isArray(values)) return [];
  return values.map((value, index) => {
    const item = asRecord(value);
    if (!item) throw new Error(`${field}[${index}] 必须是对象`);
    return item;
  });
}

/**
 * @param {UnknownRecord} record
 * @param {string} field
 * @returns {string[]}
 */
function stringListField(record, field) {
  const values = record[field];
  if (!Array.isArray(values)) return [];
  return values.map((value, index) => {
    if (typeof value !== "string" || value.trim() === "") {
      throw new Error(`${field}[${index}] 必须是非空字符串`);
    }
    return value.trim();
  });
}

/**
 * @param {UnknownRecord} record
 * @param {string} field
 * @returns {unknown}
 */
function schemaField(record, field) {
  const value = record[field];
  if (typeof value === "boolean" || asRecord(value)) return value;
  return true;
}

/**
 * @param {string | null} value
 * @returns {value is string}
 */
function isString(value) {
  return value != null;
}

/**
 * @param {string} serviceKey
 * @param {string} route
 * @param {number} index
 * @returns {string}
 */
function routeKeyFromPattern(serviceKey, route, index) {
  const suffix = normalizePackageKey(route.replaceAll("*", "all"));
  return `${serviceKey}.${suffix || `route-${index + 1}`}`;
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
