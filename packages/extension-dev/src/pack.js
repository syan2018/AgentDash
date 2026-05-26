// @ts-check

import { build, context } from "esbuild";
import { createRequire } from "node:module";
import { mkdir, readdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import path from "node:path";

import { createTgz } from "./archive.js";
import {
  MANIFEST_FILE,
  PACKAGE_JSON_FILE,
  asRecord,
  readJsonFile,
  sha256Digest,
  validateProject,
} from "./manifest.js";

const require = createRequire(import.meta.url);
const AGENTDASH_SDK_PACKAGES = /^@agentdash\/extension-(sdk|ui)$/;
const PANEL_ENTRY_CANDIDATES = [
  "src/panel/main.tsx",
  "src/panel/main.ts",
  "src/panel/index.tsx",
  "src/panel/index.ts",
];

/**
 * @typedef {{ archive_path: string, archive_digest: string, manifest_digest: string, manifest: import("./manifest.js").UnknownRecord }} PackResult
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
  const manifest = await writePackedManifest(root);
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
 * @param {string} projectRoot
 * @returns {Promise<void>}
 */
export async function watchProject(projectRoot) {
  const root = path.resolve(projectRoot);
  const distDir = path.join(root, "dist");
  await mkdir(distDir, { recursive: true });
  await copyPanelAssets(root, distDir);
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
function agentdashSdkPackagesPlugin() {
  return {
    name: "agentdash-sdk-packages",
    setup(buildConfig) {
      buildConfig.onResolve({ filter: AGENTDASH_SDK_PACKAGES }, (args) => {
        return { path: require.resolve(args.path) };
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
 * @returns {Promise<import("./manifest.js").UnknownRecord>}
 */
async function writePackedManifest(root) {
  const manifest = asRecord(await readJsonFile(path.join(root, MANIFEST_FILE)));
  if (!manifest) throw new Error(`${MANIFEST_FILE} 必须是对象`);
  const bundlePath = path.join(root, "dist", "extension.js");
  const digest = sha256Digest(await readFile(bundlePath));
  manifest.bundles = [{ kind: "extension_host", entry: "dist/extension.js", digest }];
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
 * @param {string} raw
 * @returns {string}
 */
function safeFileName(raw) {
  return raw.replace(/[^a-zA-Z0-9._-]+/g, "-").replace(/^-+|-+$/g, "") || "extension";
}
