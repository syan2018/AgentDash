// @ts-check

import { exec as execCommand, execFile } from "node:child_process";
import { mkdtemp, readFile, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { promisify } from "node:util";

import { build } from "esbuild";
import { createExtensionContext } from "./runtime-context.js";

import { MANIFEST_FILE, asRecord, validateRuntimeSurfaceParity } from "./manifest.js";
import { agentdashSdkPackagesPlugin } from "./pack.js";

const execAsync = promisify(execCommand);
const execFileAsync = promisify(execFile);
const DEFAULT_TIMEOUT_MS = 30_000;
const DEFAULT_MAX_OUTPUT_BYTES = 64 * 1024;

/**
 * @typedef {import("./runtime-context.js").JsonValue} JsonValue
 * @typedef {import("./runtime-context.js").JsonObject} JsonObject
 * @typedef {import("./runtime-context.js").ExtensionContext} ExtensionContext
 * @typedef {import("./runtime-context.js").ExtensionRuntimeActionDefinition} ExtensionRuntimeActionDefinition
 * @typedef {import("./runtime-context.js").ExtensionProtocolDefinition} ExtensionProtocolDefinition
 * @typedef {{ method: string, params?: Record<string, unknown> }} DevBridgeDispatchRequest
 * @typedef {{ project_id: string, execution_id: string, extension_id: string, extension_key: string, panel_type_id: string, uri: string }} DevPanelContext
 * @typedef {{ path: string, mtime_ms: number }} InputStamp
 * @typedef {{ extension_id?: unknown, package?: unknown, workspace_tabs?: unknown, extension_dependencies?: unknown }} ManifestRecord
 */

export class ExtensionDevRuntime {
  /**
   * @param {string} projectRoot
   * @param {{ tempRoot?: string }} [options]
   */
  constructor(projectRoot, options = {}) {
    this.projectRoot = path.resolve(projectRoot);
    this.tempRoot = options.tempRoot ?? null;
    /** @type {ManifestRecord | null} */
    this.manifest = null;
    /** @type {ExtensionContext | null} */
    this.context = null;
    /** @type {InputStamp[]} */
    this.inputStamps = [];
    this.loadVersion = 0;
    this.workspace = new MemoryWorkspace();
  }

  async load() {
    this.manifest = await readManifest(this.projectRoot);
    const bundlePath = await this.bundleExtension();
    const moduleUrl = `${pathToFileURL(bundlePath).href}?v=${Date.now()}-${this.loadVersion}`;
    const imported = await import(moduleUrl);
    const extension = imported.default ?? imported.extension;
    if (!extension || typeof extension !== "object") {
      throw new Error("extension dev runtime 需要 default export extension object");
    }
    const context = createExtensionContext(this.createApiOverrides());
    this.context = context;
    const activate = typeof extension.activate === "function" ? extension.activate : null;
    if (activate) {
      await activate(context);
    }
    /** @type {string[]} */
    const parityErrors = [];
    validateRuntimeSurfaceParity(this.manifest, context.contributions, parityErrors);
    if (parityErrors.length > 0) {
      throw new Error(parityErrors.join("\n"));
    }
    return this.status();
  }

  status() {
    const manifest = this.requireManifest();
    return {
      extension_id: extensionIdFromManifest(manifest),
      action_keys: this.actions().map((action) => action.action_key).sort(),
      protocol_keys: this.protocols()
        .map((channel) => canonicalProtocolKey(extensionIdFromManifest(manifest), channel.protocol_key))
        .sort(),
      input_count: this.inputStamps.length,
      load_version: this.loadVersion,
    };
  }

  /**
   * @param {DevBridgeDispatchRequest} request
   * @returns {Promise<JsonValue>}
   */
  async dispatch(request) {
    await this.reloadIfChanged();
    switch (request.method) {
      case "metadata.get_context":
        return this.panelContext();
      case "runtime.invoke_action":
        return await this.invokeAction(
          stringParam(request.params, "action_key"),
          toJsonValue(request.params?.input),
        );
      case "extension.invoke_protocol":
        return await this.invokeProtocol({
          protocol_key: stringParam(request.params, "protocol_key"),
          method: stringParam(request.params, "method"),
          dependency_alias: nullableStringParam(request.params, "dependency_alias"),
          input: toJsonValue(request.params?.input),
        });
      case "workspace.open_tab":
        return null;
      case "vfs.read":
      case "vfs.write":
        throw new Error(`${request.method} 在 extension dev preview 中暂未连接真实 WorkspacePanel VFS`);
      default:
        throw new Error(`未知 Extension dev bridge method: ${request.method}`);
    }
  }

  /**
   * @returns {DevPanelContext}
   */
  panelContext() {
    const manifest = this.requireManifest();
    const tab = firstWorkspaceTab(manifest);
    const extensionId = extensionIdFromManifest(manifest);
    const scheme = stringField(tab, "uri_scheme") ?? extensionId;
    return {
      project_id: "dev-project",
      execution_id: "dev-session",
      extension_id: extensionId,
      extension_key: extensionId,
      panel_type_id: stringField(tab, "type_id") ?? `${extensionId}.panel`,
      uri: `${scheme}://panel`,
    };
  }

  /**
   * @private
   * @returns {import("./runtime-context.js").ExtensionApiOverrides}
   */
  createApiOverrides() {
    /** @param {string} actionKey @param {JsonValue} input */
    const invokeRuntime = (actionKey, input) => this.invokeAction(actionKey, toJsonValue(input));
    /** @param {string} url @param {import("./runtime-context.js").ExtensionHttpRequestOptions} [options] */
    const fetchHttp = (url, options = {}) => devFetch(url, options);
    /** @param {string} url @param {import("./runtime-context.js").ExtensionHttpRequestOptions} [options] */
    const fetchJsonHttp = async (url, options = {}) => {
      const response = await devFetch(url, options);
      return toJsonValue(JSON.parse(response.body));
    };
    /** @param {string} filePath */
    const readText = (filePath) => this.workspace.readText(filePath);
    /** @param {string} filePath @param {string} content */
    const writeText = (filePath, content) => this.workspace.writeText(filePath, content);
    /** @param {string} filePath */
    const listWorkspace = (filePath) => this.workspace.list(filePath);
    /** @param {string} filePath */
    const statWorkspace = (filePath) => this.workspace.stat(filePath);
    /** @param {string} name */
    const getEnv = async (name) => process.env[name] ?? null;
    /** @param {string} command @param {string[]} [args] @param {import("./runtime-context.js").ExtensionProcessExecOptions} [options] */
    const execProcess = (command, args = [], options = {}) => runExecFile(command, args, options);
    /** @param {string} command @param {import("./runtime-context.js").ExtensionProcessExecOptions} [options] */
    const shellProcess = (command, options = {}) => runShell(command, options);
    /** @param {string} protocolKey @param {string} method @param {JsonValue} input */
    const invokeProtocol = (protocolKey, method, input) => this.invokeProtocol({
      protocol_key: protocolKey,
      method,
      input: toJsonValue(input),
    });
    /** @param {string} [protocolKey] */
    const selfProtocol = (protocolKey = "api") => ({
      /** @param {string} method @param {JsonValue} input */
      invoke: (method, input) => this.invokeProtocol({
        protocol_key: protocolKey,
        method,
        input: toJsonValue(input),
      }),
    });
    /** @param {string} alias @param {string | null} [protocolKey] */
    const dependencyProtocol = (alias, protocolKey = null) => ({
      /** @param {string} method @param {JsonValue} input */
      invoke: (method, input) => {
        const resolved = this.resolveDependencyProtocol(alias, protocolKey);
        return this.invokeProtocol({
          protocol_key: resolved,
          method,
          input: toJsonValue(input),
        });
      },
    });
    const overrides = {
      runtime: {
        invoke: invokeRuntime,
      },
      local: {
        getProfile: async () => ({
          username: "extension-toolchain",
          platform: process.platform,
          arch: process.arch,
          backend_id: "extension-toolchain",
          project_id: "dev-project",
          execution_id: "dev-session",
          workspace_roots: [{
            index: 0,
            name: path.basename(this.projectRoot),
            display_path: this.projectRoot,
          }],
        }),
      },
      http: {
        fetch: fetchHttp,
        fetchJson: fetchJsonHttp,
      },
      workspace: {
        readText,
        writeText,
        list: listWorkspace,
        stat: statWorkspace,
      },
      env: {
        get: getEnv,
      },
      process: {
        exec: execProcess,
        shell: shellProcess,
      },
      protocols: {
        invoke: invokeProtocol,
        self: selfProtocol,
        from: dependencyProtocol,
      },
    };
    return /** @type {import("./runtime-context.js").ExtensionApiOverrides} */ (overrides);
  }

  /**
   * @private
   * @param {string} actionKey
   * @param {JsonValue} input
   * @returns {Promise<JsonValue>}
   */
  async invokeAction(actionKey, input) {
    if (!actionKey) {
      throw new Error("runtime.invoke_action 缺少 action_key");
    }
    const action = this.actions().find((item) => item.action_key === actionKey);
    if (!action) {
      throw new Error(`Extension action 未注册: ${actionKey}`);
    }
    return toJsonValue(await action.invoke(input));
  }

  /**
   * @private
   * @param {{ protocol_key: string, method: string, input: JsonValue, dependency_alias?: string | null }} request
   * @returns {Promise<JsonValue>}
   */
  async invokeProtocol(request) {
    if (!request.protocol_key) {
      throw new Error("extension.invoke_protocol 缺少 protocol_key");
    }
    if (!request.method) {
      throw new Error("extension.invoke_protocol 缺少 method");
    }
    const manifest = this.requireManifest();
    const extensionId = extensionIdFromManifest(manifest);
    const protocolKey = request.dependency_alias
      ? this.resolveDependencyProtocol(request.dependency_alias, request.protocol_key)
      : canonicalProtocolKey(extensionId, request.protocol_key);
    const channel = this.protocols().find((item) =>
      canonicalProtocolKey(extensionId, item.protocol_key) === protocolKey
    );
    const method = channel?.methods.find((item) => item.name === request.method);
    if (!method) {
      throw new Error(`Extension protocol method 未注册: ${protocolKey}.${request.method}`);
    }
    return toJsonValue(await method.invoke(request.input));
  }

  /**
   * @private
   * @param {string} alias
   * @param {string | null} protocolKey
   * @returns {string}
   */
  resolveDependencyProtocol(alias, protocolKey) {
    const manifest = this.requireManifest();
    /** @type {Record<string, unknown>[]} */
    const dependencies = [];
    for (const rawDependency of arrayField(manifest, "extension_dependencies")) {
      const dependencyRecord = asRecord(rawDependency);
      if (dependencyRecord) dependencies.push(dependencyRecord);
    }
    const dependency = dependencies.find((item) => stringField(item, "alias") === alias);
    if (!dependency) {
      throw new Error(`Extension dependency alias 未声明: ${alias}`);
    }
    const protocols = arrayField(dependency, "protocols").filter((item) => typeof item === "string");
    if (!protocolKey) {
      const first = protocols[0];
      if (!first) {
        throw new Error(`Extension dependency 没有可用 channel: ${alias}`);
      }
      return first;
    }
    const matched = protocolKey.includes(".")
      ? protocols.find((item) => item === protocolKey)
      : protocols.find((item) => item.split(".").at(-1) === protocolKey);
    if (!matched) {
      throw new Error(`Extension dependency channel 未声明: ${alias}.${protocolKey}`);
    }
    return matched;
  }

  /**
   * @private
   * @returns {ExtensionRuntimeActionDefinition[]}
   */
  actions() {
    return this.context?.contributions.runtime_actions ?? [];
  }

  /**
   * @private
   * @returns {ExtensionProtocolDefinition[]}
   */
  protocols() {
    return this.context?.contributions.protocols ?? [];
  }

  /**
   * @private
   * @returns {ManifestRecord}
   */
  requireManifest() {
    if (!this.manifest) {
      throw new Error("extension dev runtime 尚未加载 manifest");
    }
    return this.manifest;
  }

  /**
   * @private
   * @returns {Promise<void>}
   */
  async reloadIfChanged() {
    if (this.inputStamps.length === 0) {
      await this.load();
      return;
    }
    for (const input of this.inputStamps) {
      const info = await stat(input.path).catch(() => null);
      if (!info || info.mtimeMs !== input.mtime_ms) {
        await this.load();
        return;
      }
    }
  }

  /**
   * @private
   * @returns {Promise<string>}
   */
  async bundleExtension() {
    const tempRoot = this.tempRoot ?? await mkdtemp(path.join(os.tmpdir(), "agentdash-extension-toolchain-"));
    this.tempRoot = tempRoot;
    const outfile = path.join(tempRoot, `extension-${Date.now()}-${this.loadVersion}.mjs`);
    const result = await build({
      entryPoints: [path.join(this.projectRoot, "src", "extension.ts")],
      outfile,
      bundle: true,
      platform: "neutral",
      format: "esm",
      target: "es2022",
      sourcemap: "inline",
      metafile: true,
      plugins: [agentdashSdkPackagesPlugin()],
    });
    /** @type {InputStamp[]} */
    const stamps = [];
    for (const input of Object.keys(result.metafile.inputs)) {
      const absolute = path.isAbsolute(input) ? input : path.resolve(this.projectRoot, input);
      const info = await stat(absolute).catch(() => null);
      if (info) {
        stamps.push({ path: absolute, mtime_ms: info.mtimeMs });
      }
    }
    this.inputStamps = stamps;
    this.loadVersion += 1;
    return outfile;
  }
}

/**
 * @param {string} projectRoot
 * @param {{ tempRoot?: string }} [options]
 * @returns {ExtensionDevRuntime}
 */
export function createDevRuntime(projectRoot, options = {}) {
  return new ExtensionDevRuntime(projectRoot, options);
}

class MemoryWorkspace {
  constructor() {
    /** @type {Map<string, string>} */
    this.files = new Map();
  }

  /**
   * @param {string} filePath
   * @returns {Promise<string>}
   */
  async readText(filePath) {
    const normalized = normalizeWorkspacePath(filePath);
    if (!this.files.has(normalized)) {
      throw new Error(`Extension dev workspace 文件不存在: ${normalized}`);
    }
    return this.files.get(normalized) ?? "";
  }

  /**
   * @param {string} filePath
   * @param {string} content
   * @returns {Promise<void>}
   */
  async writeText(filePath, content) {
    this.files.set(normalizeWorkspacePath(filePath), content);
  }

  /**
   * @param {string} directory
   * @returns {Promise<Array<{ path: string, kind: "file" | "directory" }>>}
   */
  async list(directory) {
    const prefix = normalizeWorkspacePath(directory);
    const base = prefix === "." ? "" : `${prefix}/`;
    const entries = new Map();
    for (const filePath of this.files.keys()) {
      if (!filePath.startsWith(base)) continue;
      const rest = filePath.slice(base.length);
      const [head, ...tail] = rest.split("/");
      if (!head) continue;
      entries.set(`${base}${head}`, tail.length > 0 ? "directory" : "file");
    }
    return [...entries.entries()]
      .map(([entryPath, kind]) => ({ path: entryPath, kind }))
      .sort((left, right) => left.path.localeCompare(right.path));
  }

  /**
   * @param {string} filePath
   * @returns {Promise<{ path: string, kind: "file" | "directory" | "missing", size?: number, modified_at?: string }>}
   */
  async stat(filePath) {
    const normalized = normalizeWorkspacePath(filePath);
    if (this.files.has(normalized)) {
      return {
        path: normalized,
        kind: "file",
        size: Buffer.byteLength(this.files.get(normalized) ?? "", "utf8"),
        modified_at: new Date().toISOString(),
      };
    }
    const children = await this.list(normalized);
    if (children.length > 0) {
      return { path: normalized, kind: "directory", modified_at: new Date().toISOString() };
    }
    return { path: normalized, kind: "missing" };
  }
}

/**
 * @param {string} root
 * @returns {Promise<ManifestRecord>}
 */
async function readManifest(root) {
  const parsed = JSON.parse(await readFile(path.join(root, MANIFEST_FILE), "utf8"));
  const record = asRecord(parsed);
  if (!record) {
    throw new Error(`${MANIFEST_FILE} 必须是对象`);
  }
  return record;
}

/**
 * @param {ManifestRecord} manifest
 * @returns {string}
 */
function extensionIdFromManifest(manifest) {
  const extensionId = typeof manifest.extension_id === "string" ? manifest.extension_id.trim() : "";
  if (!extensionId) {
    throw new Error("agentdash.extension.json 缺少 extension_id");
  }
  return extensionId;
}

/**
 * @param {ManifestRecord} manifest
 * @returns {Record<string, unknown>}
 */
function firstWorkspaceTab(manifest) {
  const tab = arrayField(manifest, "workspace_tabs").map(asRecord).find(Boolean);
  return tab ?? {};
}

/**
 * @param {string} extensionId
 * @param {string} protocolKey
 * @returns {string}
 */
function canonicalProtocolKey(extensionId, protocolKey) {
  return protocolKey.includes(".") ? protocolKey : `${extensionId}.${protocolKey}`;
}

/**
 * @param {Record<string, unknown> | undefined} params
 * @param {string} key
 * @returns {string}
 */
function stringParam(params, key) {
  const value = params?.[key];
  return typeof value === "string" ? value.trim() : "";
}

/**
 * @param {Record<string, unknown> | undefined} params
 * @param {string} key
 * @returns {string | null}
 */
function nullableStringParam(params, key) {
  const value = stringParam(params, key);
  return value || null;
}

/**
 * @param {Record<string, unknown>} record
 * @param {string} key
 * @returns {string | null}
 */
function stringField(record, key) {
  const value = record[key];
  return typeof value === "string" && value.trim() !== "" ? value.trim() : null;
}

/**
 * @param {Record<string, unknown>} record
 * @param {string} key
 * @returns {unknown[]}
 */
function arrayField(record, key) {
  return Array.isArray(record[key]) ? record[key] : [];
}

/**
 * @param {unknown} value
 * @returns {JsonValue}
 */
function toJsonValue(value) {
  if (value == null) return null;
  if (typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  if (Array.isArray(value)) return value.map(toJsonValue);
  const record = asRecord(value);
  if (!record) return null;
  /** @type {JsonObject} */
  const result = {};
  for (const [key, item] of Object.entries(record)) {
    result[key] = toJsonValue(item);
  }
  return result;
}

/**
 * @param {string} raw
 * @returns {string}
 */
function normalizeWorkspacePath(raw) {
  const normalized = raw.replaceAll("\\", "/").split("/").filter((part) => part && part !== ".").join("/");
  if (normalized.includes("..")) {
    throw new Error("Extension dev workspace path 不能包含 ..");
  }
  return normalized || ".";
}

/**
 * @param {string} url
 * @param {import("./runtime-context.js").ExtensionHttpRequestOptions} options
 * @returns {Promise<import("./runtime-context.js").ExtensionHttpResponse>}
 */
async function devFetch(url, options = {}) {
  const response = await fetch(url, {
    method: options.method ?? "GET",
    headers: options.headers,
    body: typeof options.body === "string" ? options.body : options.body == null ? undefined : JSON.stringify(options.body),
    signal: options.timeout_ms ? AbortSignal.timeout(options.timeout_ms) : undefined,
  });
  /** @type {Record<string, string>} */
  const headers = {};
  response.headers.forEach((value, key) => {
    headers[key] = value;
  });
  return {
    status: response.status,
    headers,
    body: await response.text(),
  };
}

/**
 * @param {string} command
 * @param {string[]} args
 * @param {import("./runtime-context.js").ExtensionProcessExecOptions} options
 * @returns {Promise<import("./runtime-context.js").ExtensionProcessResult>}
 */
async function runExecFile(command, args, options = {}) {
  try {
    const result = await execFileAsync(command, args, processOptions(options));
    return processResult(0, result.stdout, result.stderr, false, options);
  } catch (error) {
    return processErrorResult(error, options);
  }
}

/**
 * @param {string} command
 * @param {import("./runtime-context.js").ExtensionProcessExecOptions} options
 * @returns {Promise<import("./runtime-context.js").ExtensionProcessResult>}
 */
async function runShell(command, options = {}) {
  try {
    const result = await execAsync(command, processOptions(options));
    return processResult(0, result.stdout, result.stderr, false, options);
  } catch (error) {
    return processErrorResult(error, options);
  }
}

/**
 * @param {import("./runtime-context.js").ExtensionProcessExecOptions} options
 * @returns {import("node:child_process").ExecOptions}
 */
function processOptions(options) {
  return {
    cwd: options.cwd,
    env: { ...process.env, ...options.env },
    timeout: options.timeout_ms ?? DEFAULT_TIMEOUT_MS,
    maxBuffer: options.max_output_bytes ?? DEFAULT_MAX_OUTPUT_BYTES,
    windowsHide: true,
  };
}

/**
 * @param {number} exitCode
 * @param {unknown} stdout
 * @param {unknown} stderr
 * @param {boolean} timedOut
 * @param {import("./runtime-context.js").ExtensionProcessExecOptions} options
 * @returns {import("./runtime-context.js").ExtensionProcessResult}
 */
function processResult(exitCode, stdout, stderr, timedOut, options) {
  const maxBytes = options.max_output_bytes ?? DEFAULT_MAX_OUTPUT_BYTES;
  const out = truncateText(typeof stdout === "string" ? stdout : String(stdout ?? ""), maxBytes);
  const err = truncateText(typeof stderr === "string" ? stderr : String(stderr ?? ""), maxBytes);
  return {
    exit_code: exitCode,
    stdout: out.text,
    stderr: err.text,
    timed_out: timedOut,
    truncated: out.truncated || err.truncated,
  };
}

/**
 * @param {unknown} error
 * @param {import("./runtime-context.js").ExtensionProcessExecOptions} options
 * @returns {import("./runtime-context.js").ExtensionProcessResult}
 */
function processErrorResult(error, options) {
  const record = error != null && typeof error === "object" ? /** @type {Record<string, unknown>} */ (error) : {};
  const exitCode = typeof record.code === "number" ? record.code : 1;
  const timedOut = Boolean(record.killed) || Boolean(record.signal);
  return processResult(exitCode, record.stdout, record.stderr, timedOut, options);
}

/**
 * @param {string} text
 * @param {number} maxBytes
 * @returns {{ text: string, truncated: boolean }}
 */
function truncateText(text, maxBytes) {
  const bytes = Buffer.from(text);
  if (bytes.length <= maxBytes) {
    return { text, truncated: false };
  }
  return { text: bytes.subarray(0, maxBytes).toString("utf8"), truncated: true };
}
