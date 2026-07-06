// @ts-check

import process from "node:process";

import { initProject } from "./init.js";

import {
  generateAppProject,
  installExtensionProject,
  packExtensionProject,
  startExtensionProject,
  validateExtensionProject,
} from "./app-pipeline.js";
import { runWrapWebappCli } from "./wrap-webapp.js";

/**
 * @typedef {{ log(message?: unknown): void, warn(message?: unknown): void, error(message?: unknown): void }} CliIo
 */

/**
 * @param {string[]} [args]
 * @param {CliIo} [io]
 * @returns {Promise<number>}
 */
export async function runAgentDashExtCli(args = process.argv.slice(2), io = console) {
  const command = args[0] ?? "help";
  try {
    if (command === "init") {
      const target = optionValue(args, "--cwd") ?? args[1] ?? process.cwd();
      await initProject(target, {
        packageName: optionValue(args, "--package") ?? undefined,
        extensionId: optionValue(args, "--extension-id") ?? undefined,
      });
      io.log(`Initialized AgentDash extension at ${target}`);
      return 0;
    }
    if (command === "wrap-webapp") {
      await runWrapWebappCli(args.slice(1));
      return 0;
    }
    if (command === "generate") {
      const cwd = optionValue(args, "--cwd") ?? process.cwd();
      await generateAppProject(cwd);
      io.log("Generated AgentDash Extension App artifacts");
      io.log("  manifest: .agentdash/generated/manifest.json");
      io.log("  host:     .agentdash/generated/extension.ts");
      io.log("  client:   .agentdash/generated/client.ts");
      return 0;
    }
    if (command === "validate") {
      const cwd = optionValue(args, "--cwd") ?? process.cwd();
      const result = await validateExtensionProject(cwd, { requireBundles: hasFlag(args, "--strict-bundles") });
      for (const warning of result.warnings) io.warn(warning);
      if (result.errors.length > 0) {
        throw new Error(result.errors.join("\n"));
      }
      io.log(result.mode === "app"
        ? "AgentDash Extension App generated surface is valid"
        : "AgentDash extension manifest is valid");
      return 0;
    }
    if (command === "pack") {
      const cwd = optionValue(args, "--cwd") ?? process.cwd();
      const packed = await packExtensionProject(cwd, { outDir: optionValue(args, "--out-dir") ?? undefined });
      io.log(JSON.stringify(packed, null, 2));
      return 0;
    }
    if (command === "dev") {
      const cwd = optionValue(args, "--cwd") ?? process.cwd();
      const dev = await startExtensionProject(cwd, {
        host: optionValue(args, "--host") ?? undefined,
        port: numberOption(args, "--port"),
      });
      const record = recordOf(dev);
      io.log("AgentDash extension dev ready");
      io.log(`  preview: ${String(record.previewUrl ?? "")}`);
      io.log(`  panel:   ${String(record.panelUrl ?? "")}`);
      io.log("  runtime: local extension host dispatcher");
      return 0;
    }
    if (command === "install") {
      const cwd = optionValue(args, "--cwd") ?? process.cwd();
      const apiUrl = requiredOption(args, "--api-url");
      const projectId = requiredOption(args, "--project");
      const token = requiredOption(args, "--token");
      const installed = await installExtensionProject(cwd, {
        apiUrl,
        projectId,
        token,
        archivePath: optionValue(args, "--archive") ?? undefined,
        extensionKey: optionValue(args, "--extension-key") ?? undefined,
        displayName: optionValue(args, "--display-name") ?? undefined,
        overwrite: hasFlag(args, "--overwrite"),
      });
      io.log(JSON.stringify(installed, null, 2));
      return 0;
    }
    printHelp(io);
    return 0;
  } catch (error) {
    io.error(error instanceof Error ? error.message : String(error));
    return 1;
  }
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
 * @returns {string}
 */
function requiredOption(values, name) {
  const value = optionValue(values, name);
  if (!value) {
    throw new Error(`Missing required option ${name}`);
  }
  return value;
}

/**
 * @param {string[]} values
 * @param {string} name
 * @returns {boolean}
 */
function hasFlag(values, name) {
  return values.includes(name);
}

/**
 * @param {string[]} values
 * @param {string} name
 * @returns {number | undefined}
 */
function numberOption(values, name) {
  const value = optionValue(values, name);
  if (!value) return undefined;
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`${name} 必须是正整数`);
  }
  return parsed;
}

/**
 * @param {unknown} value
 * @returns {Record<string, unknown>}
 */
function recordOf(value) {
  return value != null && typeof value === "object" && !Array.isArray(value)
    ? /** @type {Record<string, unknown>} */ (value)
    : {};
}

/**
 * @param {CliIo} io
 */
function printHelp(io) {
  io.log(`agentdash-ext <command>

Commands:
  init [target] [--package <name>] [--extension-id <id>]
  generate [--cwd <path>]
  validate [--cwd <path>] [--strict-bundles]
  pack [--cwd <path>] [--out-dir <path>]
  dev [--cwd <path>] [--host <host>] [--port <port>]
  install --api-url <url> --project <id> --token <token> [--archive <path>] [--overwrite]
`);
}
