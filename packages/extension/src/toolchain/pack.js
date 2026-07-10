// @ts-check

import { build, context } from "esbuild";
import { mkdir, readdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { createTgz } from "./archive.js";
import { createExtensionContext } from "./runtime-context.js";
import {
  MANIFEST_FILE,
  PACKAGE_JSON_FILE,
  asRecord,
  readJsonFile,
  sha256Digest,
  validateProject,
  validateRuntimeSurfaceParity,
} from "./manifest.js";

const packageRoot = path.resolve(fileURLToPath(new URL("../..", import.meta.url)));
const AGENTDASH_EXTENSION_PACKAGES = /^@agentdash\/extension(?:\/.*)?$/;
const PANEL_ENTRY_CANDIDATES = [
  "src/panel/main.tsx",
  "src/panel/main.ts",
  "src/panel/index.tsx",
  "src/panel/index.ts",
];
const BACKEND_SERVICE_OUTPUT_DIR = "dist/backend-services";
const UI_COMPONENT_OUTPUT_DIR = "dist/components";

/**
 * @typedef {{ archive_path: string, archive_digest: string, manifest_digest: string, manifest: import("./manifest.js").UnknownRecord }} PackResult
 * @typedef {{ service_key: string, entry: string, digest: string }} BackendServiceBundle
 */

/**
 * @param {string} projectRoot
 * @param {{ outDir?: string }} [options]
 * @returns {Promise<PackResult>}
 */
export async function packProject(projectRoot, options = {}) {
  const root = path.resolve(projectRoot);
  const distDir = path.join(root, "dist");
  const extensionEntry = path.join(root, "src", "extension.ts");
  await rm(distDir, { recursive: true, force: true });
  await mkdir(distDir, { recursive: true });

  const preflight = await validateProject(root, { requireBundles: false });
  if (preflight.errors.length > 0) {
    throw new Error(preflight.errors.join("\n"));
  }

  await build({
    entryPoints: [extensionEntry],
    outfile: path.join(distDir, "extension.js"),
    bundle: true,
    platform: "neutral",
    format: "esm",
    target: "es2022",
    sourcemap: false,
    metafile: true,
    plugins: [agentdashSdkPackagesPlugin()],
  });

  await copyPanelAssets(root, distDir);
  await buildPanelBundle(root, distDir);
  await buildUiComponentBundles(root);
  const backendServiceBundles = await buildBackendServiceBundles(root);
  await validatePackedRuntimeSurface(root);
  const manifest = await writePackedManifest(root, backendServiceBundles);
  const validation = await validateProject(root, { requireBundles: true });
  if (validation.errors.length > 0) {
    throw new Error(validation.errors.join("\n"));
  }

  const files = await collectArchiveFiles(root, [MANIFEST_FILE, PACKAGE_JSON_FILE, "dist"]);
  const archiveBytes = createTgz(files);
  const archiveDigest = sha256Digest(archiveBytes);
  const packageInfo = asRecord(manifest.package);
  const packageName = stringValue(packageInfo?.name, "extension");
  const packageVersion = stringValue(packageInfo?.version, "0.0.0");
  const outDir = path.resolve(options.outDir ?? path.join(root, "packed"));
  await mkdir(outDir, { recursive: true });
  const archivePath = path.join(
    outDir,
    `${safeFileName(packageName)}-${packageVersion}.agentdash-extension.tgz`,
  );
  await writeFile(archivePath, archiveBytes);

  return {
    archive_path: archivePath,
    archive_digest: archiveDigest,
    manifest_digest: sha256Digest(Buffer.from(JSON.stringify(manifest))),
    manifest,
  };
}

/**
 * @param {string} root
 * @returns {Promise<BackendServiceBundle[]>}
 */
async function buildBackendServiceBundles(root) {
  const manifest = asRecord(await readJsonFile(path.join(root, MANIFEST_FILE)));
  if (!manifest) throw new Error(`${MANIFEST_FILE} 必须是对象`);
  const bundles = [];
  for (const service of backendServiceRecords(manifest)) {
    const serviceKey = stringValue(service.service_key, "");
    const entry = stringValue(service.entry, "");
    if (!serviceKey || !entry) continue;
    const outfile = `${BACKEND_SERVICE_OUTPUT_DIR}/${safeFileName(serviceKey)}/index.js`;
    const outfilePath = path.join(root, outfile);
    await mkdir(path.dirname(outfilePath), { recursive: true });
    await build({
      entryPoints: [path.join(root, ...entry.split("/"))],
      outfile: outfilePath,
      bundle: true,
      platform: "node",
      format: "esm",
      target: "es2022",
      sourcemap: false,
      plugins: [agentdashSdkPackagesPlugin()],
    });
    bundles.push({
      service_key: serviceKey,
      entry: outfile,
      digest: sha256Digest(await readFile(outfilePath)),
    });
  }
  return bundles;
}

/**
 * @param {string} root
 * @returns {Promise<void>}
 */
async function validatePackedRuntimeSurface(root) {
  const manifest = asRecord(await readJsonFile(path.join(root, MANIFEST_FILE)));
  if (!manifest) throw new Error(`${MANIFEST_FILE} 必须是对象`);
  const bundlePath = path.join(root, "dist", "extension.js");
  const moduleUrl = `${pathToFileURL(bundlePath).href}?pack-validate=${Date.now()}`;
  const imported = await import(moduleUrl);
  const extension = imported.default ?? imported.extension;
  if (!extension || typeof extension !== "object") {
    throw new Error("extension package 需要 default export extension object");
  }
  const context = createExtensionContext();
  if (typeof extension.activate === "function") {
    await extension.activate(context);
  }
  /** @type {string[]} */
  const errors = [];
  validateRuntimeSurfaceParity(manifest, context.contributions, errors);
  if (errors.length > 0) {
    throw new Error(errors.join("\n"));
  }
}

/**
 * @param {string} projectRoot
 * @returns {Promise<void>}
 */
export async function watchProject(projectRoot) {
  const root = path.resolve(projectRoot);
  const distDir = path.join(root, "dist");
  await mkdir(distDir, { recursive: true });
  await copyPanelAssets(root, distDir);
  await buildUiComponentBundles(root);
  const contexts = [];
  contexts.push(await context({
    entryPoints: [path.join(root, "src", "extension.ts")],
    outfile: path.join(root, "dist", "extension.js"),
    bundle: true,
    platform: "neutral",
    format: "esm",
    target: "es2022",
    plugins: [agentdashSdkPackagesPlugin()],
  }));
  const panelEntry = await findPanelEntry(root);
  if (panelEntry) {
    contexts.push(await context(panelBuildOptions(panelEntry, path.join(distDir, "panel", "main.js"))));
  }
  await Promise.all(contexts.map((ctx) => ctx.watch()));
}

/**
 * @returns {import("esbuild").Plugin}
 */
export function agentdashSdkPackagesPlugin() {
  return {
    name: "agentdash-sdk-packages",
    setup(buildConfig) {
      buildConfig.onResolve({ filter: AGENTDASH_EXTENSION_PACKAGES }, (args) => {
        if (
          args.path === "@agentdash/extension"
          || args.path === "@agentdash/extension/host"
        ) {
          return { path: path.join(packageRoot, "src", "toolchain", "authoring-runtime.js") };
        }
        if (args.path === "@agentdash/extension/browser") {
          return { path: path.join(packageRoot, "src", "browser", "index.ts") };
        }
        if (args.path === "@agentdash/extension/react") {
          return { path: path.join(packageRoot, "src", "react", "index.ts") };
        }
        return { external: true };
      });
    },
  };
}

/**
 * @param {string} root
 * @param {string} distDir
 */
async function copyPanelAssets(root, distDir) {
  const panelSource = path.join(root, "src", "panel");
  try {
    const info = await stat(panelSource);
    if (!info.isDirectory()) return;
  } catch {
    return;
  }
  await copyDirectory(panelSource, path.join(distDir, "panel"));
}

/**
 * @param {string} root
 * @param {string} distDir
 * @returns {Promise<void>}
 */
async function buildPanelBundle(root, distDir) {
  const panelEntry = await findPanelEntry(root);
  if (!panelEntry) return;
  await build(panelBuildOptions(panelEntry, path.join(distDir, "panel", "main.js")));
}

/**
 * @param {string} root
 * @returns {Promise<void>}
 */
async function buildUiComponentBundles(root) {
  const manifest = asRecord(await readJsonFile(path.join(root, MANIFEST_FILE)));
  if (!manifest) throw new Error(`${MANIFEST_FILE} 必须是对象`);
  for (const component of uiComponentRecords(manifest)) {
    const componentKey = stringValue(component.component_key, "");
    if (!componentKey) continue;
    const sourceDir = path.join(root, "src", "components", componentKey);
    const outputDir = path.join(root, UI_COMPONENT_OUTPUT_DIR, componentKey);
    await copyDirectory(sourceDir, outputDir);
    const entry = await findWebEntry(sourceDir);
    if (entry) {
      await build(panelBuildOptions(entry, path.join(outputDir, "main.js")));
    }
  }
}

/**
 * @param {string} root
 * @returns {Promise<string | null>}
 */
async function findWebEntry(root) {
  for (const file of ["main.tsx", "main.ts", "index.tsx", "index.ts"]) {
    const candidate = path.join(root, file);
    try {
      if ((await stat(candidate)).isFile()) return candidate;
    } catch {
      // try next candidate
    }
  }
  return null;
}

/**
 * @param {string} root
 * @returns {Promise<string | null>}
 */
async function findPanelEntry(root) {
  for (const candidate of PANEL_ENTRY_CANDIDATES) {
    const absolute = path.join(root, candidate);
    try {
      const info = await stat(absolute);
      if (info.isFile()) return absolute;
    } catch {
      // try next candidate
    }
  }
  return null;
}

/**
 * @param {string} entry
 * @param {string} outfile
 * @returns {import("esbuild").BuildOptions}
 */
function panelBuildOptions(entry, outfile) {
  return {
    entryPoints: [entry],
    outfile,
    bundle: true,
    platform: "browser",
    format: "esm",
    target: "es2022",
    sourcemap: false,
    plugins: [agentdashSdkPackagesPlugin()],
  };
}

/**
 * @param {string} source
 * @param {string} target
 */
async function copyDirectory(source, target) {
  await mkdir(target, { recursive: true });
  for (const entry of await readdir(source, { withFileTypes: true })) {
    const sourcePath = path.join(source, entry.name);
    const targetPath = path.join(target, entry.name);
    if (entry.isDirectory()) {
      await copyDirectory(sourcePath, targetPath);
    } else if (entry.isFile() && !isPanelSourceFile(sourcePath)) {
      await writeFile(targetPath, await readFile(sourcePath));
    }
  }
}

/**
 * @param {string} filePath
 * @returns {boolean}
 */
function isPanelSourceFile(filePath) {
  return /\.(ts|tsx)$/.test(filePath);
}

/**
 * @param {string} root
 * @param {BackendServiceBundle[]} backendServiceBundles
 * @returns {Promise<import("./manifest.js").UnknownRecord>}
 */
async function writePackedManifest(root, backendServiceBundles) {
  const manifest = asRecord(await readJsonFile(path.join(root, MANIFEST_FILE)));
  if (!manifest) throw new Error(`${MANIFEST_FILE} 必须是对象`);
  const bundlePath = path.join(root, "dist", "extension.js");
  const digest = sha256Digest(await readFile(bundlePath));
  const backendBundleByServiceKey = new Map(
    backendServiceBundles.map((bundle) => [bundle.service_key, bundle]),
  );
  if (Array.isArray(manifest.backend_services)) {
    manifest.backend_services = manifest.backend_services.map((service) => {
      const record = asRecord(service);
      if (!record) return service;
      const bundle = backendBundleByServiceKey.get(stringValue(record.service_key, ""));
      return bundle ? { ...record, entry: bundle.entry } : service;
    });
  }
  manifest.bundles = [
    { kind: "extension_host", entry: "dist/extension.js", digest },
    ...backendServiceBundles.map((bundle) => ({
      kind: "backend_service",
      entry: bundle.entry,
      digest: bundle.digest,
    })),
  ];
  await writeFile(path.join(root, MANIFEST_FILE), `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

/**
 * @param {string} root
 * @param {string[]} entries
 * @returns {Promise<Array<{ path: string, data: Buffer }>>}
 */
async function collectArchiveFiles(root, entries) {
  const files = [];
  for (const entry of entries) {
    const absolute = path.join(root, entry);
    const info = await stat(absolute);
    if (info.isDirectory()) {
      files.push(...await collectDirectory(root, absolute));
    } else if (info.isFile()) {
      files.push({ path: entry.replaceAll("\\", "/"), data: await readFile(absolute) });
    }
  }
  return files.sort((left, right) => left.path.localeCompare(right.path));
}

/**
 * @param {string} root
 * @param {string} directory
 * @returns {Promise<Array<{ path: string, data: Buffer }>>}
 */
async function collectDirectory(root, directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...await collectDirectory(root, absolute));
    } else if (entry.isFile()) {
      files.push({
        path: path.relative(root, absolute).replaceAll("\\", "/"),
        data: await readFile(absolute),
      });
    }
  }
  return files;
}

/**
 * @param {unknown} value
 * @param {string} fallback
 * @returns {string}
 */
function stringValue(value, fallback) {
  return typeof value === "string" && value.trim() !== "" ? value : fallback;
}

/**
 * @param {import("./manifest.js").UnknownRecord} manifest
 * @returns {import("./manifest.js").UnknownRecord[]}
 */
function backendServiceRecords(manifest) {
  const services = manifest.backend_services;
  if (!Array.isArray(services)) return [];
  return services
    .map((service) => asRecord(service))
    .filter((service) => service != null);
}

/**
 * @param {import("./manifest.js").UnknownRecord} manifest
 * @returns {import("./manifest.js").UnknownRecord[]}
 */
function uiComponentRecords(manifest) {
  const components = manifest.ui_components;
  if (!Array.isArray(components)) return [];
  return components
    .map((component) => asRecord(component))
    .filter((component) => component != null);
}

/**
 * @param {string} raw
 * @returns {string}
 */
function safeFileName(raw) {
  return raw.replace(/[^a-zA-Z0-9._-]+/g, "-").replace(/^-+|-+$/g, "") || "extension";
}
