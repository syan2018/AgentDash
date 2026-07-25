// @ts-check

import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { gzipSync } from "node:zlib";
import {
  describeFetchRouteTarget,
  hasFetchRoute,
  isLocalhostUrl,
  normalizeFetchRouteBinding,
  parseFetchRouteBinding,
} from "../browser/fetch-route.js";

const DEFAULT_VERSION = "0.1.0";
const NORMALIZED_DEFINITION_FILE = "agentdash.app.normalized.json";
const MANIFEST_FILE = "agentdash.extension.json";
const PACKAGE_JSON_FILE = "package.json";
const TEXT_EXTENSIONS = new Set([
  ".css",
  ".html",
  ".js",
  ".json",
  ".map",
  ".mjs",
  ".svg",
  ".txt",
  ".wasm.map",
  ".xml",
]);
const FETCH_LITERAL_PATTERN = /\bfetch\s*\(\s*(['"`])([^'"`]+)\1/g;
const XHR_LITERAL_PATTERN = /\.open\s*\(\s*(['"`])(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\1\s*,\s*(['"`])([^'"`]+)\2/g;
const LOCALHOST_URL_PATTERN = /\bhttps?:\/\/(?:localhost|127\.0\.0\.1|\[::1\])(?::\d+)?[^\s'"`<>)\\]*/g;
const SERVICE_WORKER_PATTERN = /\bnavigator\.serviceWorker\.register\s*\(/;

/**
 * @typedef {import("../browser/fetch-route.js").FetchRouteBinding} FetchRouteBinding
 * @typedef {"error" | "warning" | "info"} WebappWrapDiagnosticSeverity
 * @typedef {{ severity: WebappWrapDiagnosticSeverity, code: string, message: string, file?: string, value?: string }} WebappWrapDiagnostic
 * @typedef {{ dist: string, extensionId: string, name: string, version?: string, outDir?: string, entry?: string, fetchRoutes?: Array<FetchRouteBinding | unknown>, failOnDiagnostics?: boolean }} WrapWebappOptions
 * @typedef {{ kind: "web_app_wrapper", schema_version: 1, app: { id: string, name: string, version: string }, panel: { entry: string }, host: { kind: "noop", entry: string }, fetch_routes: FetchRouteBinding[] }} NormalizedWebappDefinition
 * @typedef {{ archive_path: string, archive_digest: string, manifest_digest: string, normalized_definition_path: string, normalized_definition: NormalizedWebappDefinition, manifest: Record<string, unknown>, package_json: Record<string, unknown>, diagnostics: WebappWrapDiagnostic[] }} WrapWebappResult
 */

export class WrapWebappDiagnosticError extends Error {
  /**
   * @param {WebappWrapDiagnostic[]} diagnostics
   */
  constructor(diagnostics) {
    super(formatDiagnostics(diagnostics));
    this.name = "WrapWebappDiagnosticError";
    this.diagnostics = diagnostics;
  }
}

/**
 * Wraps a static web app dist directory into a minimal AgentDash extension
 * archive with a no-op host bundle and a Project workspace webview tab.
 *
 * @param {WrapWebappOptions} options
 * @returns {Promise<WrapWebappResult>}
 */
export async function wrapWebapp(options) {
  const distDir = path.resolve(options.dist);
  const distStat = await stat(distDir);
  if (!distStat.isDirectory()) {
    throw new Error(`--dist 必须指向目录: ${distDir}`);
  }
  const extensionId = normalizeExtensionId(options.extensionId);
  const version = options.version ?? DEFAULT_VERSION;
  validateVersion(version);
  const entry = normalizeEntry(options.entry ?? "index.html");
  await assertDistEntry(distDir, entry);
  const fetchRoutes = (options.fetchRoutes ?? []).map((route) => normalizeFetchRouteBinding(route));
  const diagnostics = await analyzeWebappDist(distDir, fetchRoutes);
  const blockingDiagnostics = diagnostics.filter((diagnostic) => diagnostic.severity === "error");
  if (blockingDiagnostics.length > 0 && options.failOnDiagnostics !== false) {
    throw new WrapWebappDiagnosticError(diagnostics);
  }

  const packageName = `@agentdash/wrapped-${extensionId}`;
  const normalizedDefinition = createNormalizedDefinition({
    extensionId,
    name: options.name,
    version,
    entry,
    fetchRoutes,
  });
  const noOpHost = createNoopHostBundle();
  const manifest = createWrappedManifest({
    extensionId,
    name: options.name,
    version,
    packageName,
    entry,
    fetchRoutes,
    hostDigest: sha256Digest(Buffer.from(noOpHost)),
  });
  const packageJson = {
    name: packageName,
    version,
    private: true,
    type: "module",
  };

  const outDir = path.resolve(options.outDir ?? path.join(process.cwd(), "packed"));
  await mkdir(outDir, { recursive: true });
  const normalizedDefinitionPath = path.join(
    outDir,
    `${safeFileName(extensionId)}.${NORMALIZED_DEFINITION_FILE}`,
  );
  await writeFile(normalizedDefinitionPath, `${JSON.stringify(normalizedDefinition, null, 2)}\n`);

  const archiveFiles = [
    { path: MANIFEST_FILE, data: Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`) },
    { path: PACKAGE_JSON_FILE, data: Buffer.from(`${JSON.stringify(packageJson, null, 2)}\n`) },
    { path: NORMALIZED_DEFINITION_FILE, data: Buffer.from(`${JSON.stringify(normalizedDefinition, null, 2)}\n`) },
    { path: "dist/extension.js", data: Buffer.from(noOpHost) },
    ...await collectDistFiles(distDir, "dist/panel"),
  ];
  const archiveBytes = createTgz(archiveFiles);
  const archiveDigest = sha256Digest(archiveBytes);
  const archivePath = path.join(
    outDir,
    `${safeFileName(extensionId)}-${version}.agentdash-extension.tgz`,
  );
  await writeFile(archivePath, archiveBytes);

  return {
    archive_path: archivePath,
    archive_digest: archiveDigest,
    manifest_digest: sha256Digest(Buffer.from(JSON.stringify(manifest))),
    normalized_definition_path: normalizedDefinitionPath,
    normalized_definition: normalizedDefinition,
    manifest,
    package_json: packageJson,
    diagnostics,
  };
}

/**
 * @param {string[]} args
 * @returns {Promise<void>}
 */
export async function runWrapWebappCli(args) {
  if (args.includes("--help") || args.includes("-h")) {
    printWrapWebappHelp();
    return;
  }
  const dist = requiredOption(args, "--dist");
  const extensionId = requiredOption(args, "--extension-id");
  const name = requiredOption(args, "--name");
  const fetchRoutes = optionValues(args, "--fetch-route").map((value) => parseFetchRouteBinding(value));
  const result = await wrapWebapp({
    dist,
    extensionId,
    name,
    version: optionValue(args, "--version") ?? undefined,
    outDir: optionValue(args, "--out-dir") ?? undefined,
    entry: optionValue(args, "--entry") ?? undefined,
    fetchRoutes,
  });
  for (const diagnostic of result.diagnostics) {
    if (diagnostic.severity === "warning") {
      console.warn(formatDiagnostic(diagnostic));
    }
  }
  console.log(JSON.stringify(result, null, 2));
}

/**
 * @param {string} distDir
 * @param {FetchRouteBinding[]} fetchRoutes
 * @returns {Promise<WebappWrapDiagnostic[]>}
 */
export async function analyzeWebappDist(distDir, fetchRoutes) {
  /** @type {WebappWrapDiagnostic[]} */
  const diagnostics = [];
  const files = await collectTextFiles(distDir);
  for (const file of files) {
    const text = await readFile(file.absolute, "utf8");
    if (SERVICE_WORKER_PATTERN.test(text)) {
      diagnostics.push({
        severity: "error",
        code: "service_worker_unsupported",
        file: file.relative,
        message: "wrap-webapp 暂不支持 Service Worker 注册；请改为显式 capability 或移除注册后再包装",
      });
    }
    for (const fetchUrl of findFetchLiteralUrls(text)) {
      diagnostics.push(...diagnoseFetchUrl(fetchUrl, file.relative, fetchRoutes));
    }
    for (const localhostUrl of findLocalhostUrls(text)) {
      if (!hasExplicitAbsoluteRoute(localhostUrl, fetchRoutes)) {
        diagnostics.push({
          severity: "error",
          code: "localhost_requires_explicit_route",
          file: file.relative,
          value: localhostUrl,
          message:
            `发现绝对 localhost 请求 ${localhostUrl}；wrap-webapp 不会自动代理 localhost，也不会把它解释为 backendService，请声明匹配的 httpProxy/customProtocol/backendService fetch route`,
        });
      }
    }
  }
  return dedupeDiagnostics(diagnostics);
}

/**
 * @param {string} value
 * @returns {FetchRouteBinding}
 */
export function parseWrapWebappFetchRoute(value) {
  return parseFetchRouteBinding(value);
}

/**
 * @param {{ extensionId: string, name: string, version: string, entry: string, fetchRoutes: FetchRouteBinding[] }} input
 * @returns {NormalizedWebappDefinition}
 */
function createNormalizedDefinition(input) {
  return {
    kind: "web_app_wrapper",
    schema_version: 1,
    app: {
      id: input.extensionId,
      name: input.name,
      version: input.version,
    },
    panel: {
      entry: `dist/panel/${input.entry}`,
    },
    host: {
      kind: "noop",
      entry: "dist/extension.js",
    },
    fetch_routes: input.fetchRoutes,
  };
}

/**
 * @param {{ extensionId: string, name: string, version: string, packageName: string, entry: string, fetchRoutes: FetchRouteBinding[], hostDigest: string }} input
 * @returns {Record<string, unknown>}
 */
function createWrappedManifest(input) {
  const permissions = collectFetchRoutePermissions(input.fetchRoutes);
  return pruneUndefined({
    manifest_version: "2",
    extension_id: input.extensionId,
    package: {
      name: input.packageName,
      version: input.version,
    },
    asset_version: input.version,
    workspace_tabs: [
      {
        type_id: `${input.extensionId}.panel`,
        label: input.name,
        uri_scheme: uriSchemeFromExtensionId(input.extensionId),
        renderer: {
          kind: "webview",
          entry: `dist/panel/${input.entry}`,
        },
      },
    ],
    fetch_routes: input.fetchRoutes,
    permissions: permissions.length > 0 ? permissions : undefined,
    bundles: [
      {
        kind: "extension_host",
        entry: "dist/extension.js",
        digest: input.hostDigest,
      },
    ],
  });
}

/**
 * @param {FetchRouteBinding[]} fetchRoutes
 * @returns {Record<string, unknown>[]}
 */
function collectFetchRoutePermissions(fetchRoutes) {
  const hosts = new Set();
  for (const route of fetchRoutes) {
    if (route.target.kind !== "http_proxy") continue;
    hosts.add(new URL(route.target.base_url).host);
  }
  return hosts.size > 0
    ? [{ kind: "http", hosts: [...hosts].sort(), access: "read_write" }]
    : [];
}

/**
 * @returns {string}
 */
function createNoopHostBundle() {
  return [
    "export default {",
    "  async activate() {",
    "    // Static web app wrapper: no host-side runtime surface is registered.",
    "  },",
    "};",
    "",
  ].join("\n");
}

/**
 * @param {string} url
 * @param {string} file
 * @param {FetchRouteBinding[]} fetchRoutes
 * @returns {WebappWrapDiagnostic[]}
 */
function diagnoseFetchUrl(url, file, fetchRoutes) {
  if (isRelativeApiUrl(url) && !hasFetchRoute(url, fetchRoutes, { baseUrl: "http://agentdash.local/" })) {
    return [
      {
        severity: "error",
        code: "api_fetch_route_required",
        file,
        value: url,
        message: `发现 ${url} 请求；/api/** 类 route 必须通过 --fetch-route 显式绑定到 httpProxy/customProtocol/backendService`,
      },
    ];
  }
  if (/^https?:\/\//i.test(url) && isLocalhostUrl(url) && !hasExplicitAbsoluteRoute(url, fetchRoutes)) {
    return [
      {
        severity: "error",
        code: "localhost_requires_explicit_route",
        file,
        value: url,
        message:
          `发现绝对 localhost 请求 ${url}；wrap-webapp 不会自动代理 localhost，也不会把它解释为 backendService，请声明匹配的 httpProxy/customProtocol/backendService fetch route`,
      },
    ];
  }
  return [];
}

/**
 * @param {string} url
 * @param {FetchRouteBinding[]} fetchRoutes
 * @returns {boolean}
 */
function hasExplicitAbsoluteRoute(url, fetchRoutes) {
  return fetchRoutes.some((route) => /^https?:\/\//i.test(route.route) && hasFetchRoute(url, [route]));
}

/**
 * @param {string} url
 * @returns {boolean}
 */
function isRelativeApiUrl(url) {
  if (/^https?:\/\//i.test(url)) return false;
  return url === "/api" || url.startsWith("/api/") || url === "api" || url.startsWith("api/");
}

/**
 * @param {string} text
 * @returns {string[]}
 */
function findFetchLiteralUrls(text) {
  /** @type {string[]} */
  const result = [];
  for (const match of text.matchAll(FETCH_LITERAL_PATTERN)) {
    const value = match[2];
    if (value) result.push(value);
  }
  for (const match of text.matchAll(XHR_LITERAL_PATTERN)) {
    const value = match[3];
    if (value) result.push(value);
  }
  return result;
}

/**
 * @param {string} text
 * @returns {string[]}
 */
function findLocalhostUrls(text) {
  return [...text.matchAll(LOCALHOST_URL_PATTERN)].map((match) => match[0]);
}

/**
 * @param {WebappWrapDiagnostic[]} diagnostics
 * @returns {WebappWrapDiagnostic[]}
 */
function dedupeDiagnostics(diagnostics) {
  const seen = new Set();
  /** @type {WebappWrapDiagnostic[]} */
  const result = [];
  for (const diagnostic of diagnostics) {
    const key = `${diagnostic.code}\n${diagnostic.file ?? ""}\n${diagnostic.value ?? ""}`;
    if (seen.has(key)) continue;
    seen.add(key);
    result.push(diagnostic);
  }
  return result;
}

/**
 * @param {string} distDir
 * @param {string} archiveRoot
 * @returns {Promise<Array<{ path: string, data: Buffer }>>}
 */
async function collectDistFiles(distDir, archiveRoot) {
  /** @type {Array<{ path: string, data: Buffer }>} */
  const files = [];
  for (const file of await collectFiles(distDir)) {
    files.push({
      path: `${archiveRoot}/${file.relative}`.replaceAll("\\", "/"),
      data: await readFile(file.absolute),
    });
  }
  return files.sort((left, right) => left.path.localeCompare(right.path));
}

/**
 * @param {string} root
 * @returns {Promise<Array<{ absolute: string, relative: string }>>}
 */
async function collectTextFiles(root) {
  const files = await collectFiles(root);
  return files.filter((file) => isTextFile(file.relative));
}

/**
 * @param {string} root
 * @returns {Promise<Array<{ absolute: string, relative: string }>>}
 */
async function collectFiles(root) {
  /** @type {Array<{ absolute: string, relative: string }>} */
  const files = [];
  await collectFilesInto(root, root, files);
  return files.sort((left, right) => left.relative.localeCompare(right.relative));
}

/**
 * @param {string} root
 * @param {string} directory
 * @param {Array<{ absolute: string, relative: string }>} files
 * @returns {Promise<void>}
 */
async function collectFilesInto(root, directory, files) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      await collectFilesInto(root, absolute, files);
    } else if (entry.isFile()) {
      files.push({
        absolute,
        relative: path.relative(root, absolute).replaceAll("\\", "/"),
      });
    }
  }
}

/**
 * @param {string} filePath
 * @returns {boolean}
 */
function isTextFile(filePath) {
  const lower = filePath.toLowerCase();
  if (lower.endsWith(".wasm.map")) return true;
  return TEXT_EXTENSIONS.has(path.extname(lower));
}

/**
 * @param {string} distDir
 * @param {string} entry
 * @returns {Promise<void>}
 */
async function assertDistEntry(distDir, entry) {
  const entryPath = path.join(distDir, entry);
  const entryStat = await stat(entryPath);
  if (!entryStat.isFile()) {
    throw new Error(`Web App dist entry 必须是文件: ${entry}`);
  }
}

/**
 * @param {string} value
 * @returns {string}
 */
function normalizeEntry(value) {
  const normalized = value.replaceAll("\\", "/").replace(/^\/+/, "");
  if (normalized === "" || normalized.includes("..")) {
    throw new Error("--entry 必须是 dist 内部的相对文件路径");
  }
  return normalized;
}

/**
 * @param {string} value
 * @returns {string}
 */
function normalizeExtensionId(value) {
  const normalized = value.trim();
  if (!/^[a-z0-9][a-z0-9_-]*$/.test(normalized)) {
    throw new Error("--extension-id 必须由小写字母、数字、下划线或短横线组成，并以字母或数字开头");
  }
  return normalized;
}

/**
 * @param {string} value
 * @returns {void}
 */
function validateVersion(value) {
  if (!/^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$/.test(value)) {
    throw new Error("--version 必须是 semver 版本号");
  }
}

/**
 * @param {string} extensionId
 * @returns {string}
 */
function uriSchemeFromExtensionId(extensionId) {
  const scheme = extensionId.replaceAll("_", "-");
  return /^[a-z]/.test(scheme) ? scheme : `x-${scheme}`;
}

/**
 * @param {string} raw
 * @returns {string}
 */
function safeFileName(raw) {
  return raw.replace(/[^a-zA-Z0-9._-]+/g, "-").replace(/^-+|-+$/g, "") || "extension";
}

/**
 * @param {Record<string, unknown>} record
 * @returns {Record<string, unknown>}
 */
function pruneUndefined(record) {
  return Object.fromEntries(Object.entries(record).filter(([, value]) => value !== undefined));
}

/**
 * @param {WebappWrapDiagnostic[]} diagnostics
 * @returns {string}
 */
function formatDiagnostics(diagnostics) {
  const errors = diagnostics.filter((diagnostic) => diagnostic.severity === "error");
  return errors.length > 0
    ? errors.map((diagnostic) => formatDiagnostic(diagnostic)).join("\n")
    : diagnostics.map((diagnostic) => formatDiagnostic(diagnostic)).join("\n");
}

/**
 * @param {WebappWrapDiagnostic} diagnostic
 * @returns {string}
 */
function formatDiagnostic(diagnostic) {
  const location = diagnostic.file ? `${diagnostic.file}: ` : "";
  return `[${diagnostic.severity}] ${diagnostic.code}: ${location}${diagnostic.message}`;
}

/**
 * @param {string[]} values
 * @param {string} name
 * @returns {string | null}
 */
function optionValue(values, name) {
  const index = values.indexOf(name);
  if (index < 0) return null;
  return values[index + 1] ?? null;
}

/**
 * @param {string[]} values
 * @param {string} name
 * @returns {string[]}
 */
function optionValues(values, name) {
  /** @type {string[]} */
  const result = [];
  for (let index = 0; index < values.length; index += 1) {
    if (values[index] === name && values[index + 1]) {
      result.push(values[index + 1]);
      index += 1;
    }
  }
  return result;
}

/**
 * @param {string[]} values
 * @param {string} name
 * @returns {string}
 */
function requiredOption(values, name) {
  const value = optionValue(values, name);
  if (!value) throw new Error(`Missing required option ${name}`);
  return value;
}

function printWrapWebappHelp() {
  console.log(`agentdash-ext wrap-webapp --dist <dir> --extension-id <id> --name <name>

Options:
  --version <semver>       Wrapped extension version, default ${DEFAULT_VERSION}
  --entry <file>           Web app entry inside dist, default index.html
  --out-dir <dir>          Output directory, default ./packed
  --fetch-route <binding>  Explicit route binding. Repeatable.

Fetch route binding examples:
  /api/**=httpProxy:https://api.example.com
  /api/**=customProtocol:demo.api#fetch
  /api/**=backendService:api
  http://localhost:5174/api/**=httpProxy:http://localhost:5174
`);
}

/**
 * @returns {Promise<string>}
 */
export async function createTemporaryWrapOutputDir() {
  const root = await mkdtemp(path.join(os.tmpdir(), "agentdash-wrap-webapp-"));
  return root;
}

/**
 * @param {string} directory
 * @returns {Promise<void>}
 */
export async function removeTemporaryWrapOutputDir(directory) {
  await rm(directory, { recursive: true, force: true });
}

/**
 * @param {string} value
 * @returns {string}
 */
export function digestForTest(value) {
  return sha256Digest(value);
}

export { describeFetchRouteTarget };

/**
 * @param {Buffer | Uint8Array | string} value
 * @returns {string}
 */
function sha256Digest(value) {
  return `sha256:${createHash("sha256").update(value).digest("hex")}`;
}

/**
 * @param {Array<{ path: string, data: Buffer }>} files
 * @returns {Buffer}
 */
function createTgz(files) {
  const chunks = [];
  for (const file of files) {
    const data = Buffer.from(file.data);
    chunks.push(createTarHeader(file.path, data.length));
    chunks.push(data);
    chunks.push(Buffer.alloc(tarPaddingSize(data.length)));
  }
  chunks.push(Buffer.alloc(1024));
  return gzipSync(Buffer.concat(chunks));
}

/**
 * @param {string} filePath
 * @param {number} size
 * @returns {Buffer}
 */
function createTarHeader(filePath, size) {
  const normalized = filePath.replaceAll("\\", "/");
  if (Buffer.byteLength(normalized) > 100) {
    throw new Error(`archive path 过长: ${normalized}`);
  }
  const header = Buffer.alloc(512);
  writeTarString(header, normalized, 0, 100);
  writeTarOctal(header, 0o644, 100, 8);
  writeTarOctal(header, 0, 108, 8);
  writeTarOctal(header, 0, 116, 8);
  writeTarOctal(header, size, 124, 12);
  writeTarOctal(header, Math.floor(Date.now() / 1000), 136, 12);
  header.fill(0x20, 148, 156);
  header[156] = "0".charCodeAt(0);
  writeTarString(header, "ustar", 257, 6);
  writeTarString(header, "00", 263, 2);
  const checksum = header.reduce((sum, byte) => sum + byte, 0);
  writeTarOctal(header, checksum, 148, 8);
  return header;
}

/**
 * @param {Buffer} buffer
 * @param {string} value
 * @param {number} offset
 * @param {number} length
 * @returns {void}
 */
function writeTarString(buffer, value, offset, length) {
  buffer.write(value, offset, Math.min(Buffer.byteLength(value), length), "utf8");
}

/**
 * @param {Buffer} buffer
 * @param {number} value
 * @param {number} offset
 * @param {number} length
 * @returns {void}
 */
function writeTarOctal(buffer, value, offset, length) {
  const octal = value.toString(8).padStart(length - 1, "0");
  buffer.write(octal.slice(0, length - 1), offset, length - 1, "ascii");
  buffer[offset + length - 1] = 0;
}

/**
 * @param {number} size
 * @returns {number}
 */
function tarPaddingSize(size) {
  const remainder = size % 512;
  return remainder === 0 ? 0 : 512 - remainder;
}
