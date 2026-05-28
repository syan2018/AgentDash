#!/usr/bin/env node
// @ts-check

import process from "node:process";

import { initProject } from "./init.js";
import { installProject } from "./install.js";
import { validateProject } from "./manifest.js";
import { packProject } from "./pack.js";
import { startDevProject } from "./dev-server.js";

const args = process.argv.slice(2);
const command = args[0] ?? "help";

try {
  if (command === "init") {
    const target = optionValue(args, "--cwd") ?? args[1] ?? process.cwd();
    await initProject(target, {
      packageName: optionValue(args, "--package") ?? undefined,
      extensionId: optionValue(args, "--extension-id") ?? undefined,
    });
    console.log(`Initialized AgentDash extension at ${target}`);
  } else if (command === "validate") {
    const cwd = optionValue(args, "--cwd") ?? process.cwd();
    const result = await validateProject(cwd, { requireBundles: hasFlag(args, "--strict-bundles") });
    for (const warning of result.warnings) console.warn(warning);
    if (result.errors.length > 0) {
      throw new Error(result.errors.join("\n"));
    }
    console.log("AgentDash extension manifest is valid");
  } else if (command === "pack") {
    const cwd = optionValue(args, "--cwd") ?? process.cwd();
    const packed = await packProject(cwd, { outDir: optionValue(args, "--out-dir") ?? undefined });
    console.log(JSON.stringify(packed, null, 2));
  } else if (command === "dev") {
    const cwd = optionValue(args, "--cwd") ?? process.cwd();
    const dev = await startDevProject(cwd, {
      host: optionValue(args, "--host") ?? undefined,
      port: numberOption(args, "--port"),
    });
    console.log("AgentDash extension dev ready");
    console.log(`  preview: ${dev.previewUrl}`);
    console.log(`  panel:   ${dev.panelUrl}`);
    console.log("  runtime: local extension host dispatcher");
  } else if (command === "install") {
    const cwd = optionValue(args, "--cwd") ?? process.cwd();
    const apiUrl = requiredOption(args, "--api-url");
    const projectId = requiredOption(args, "--project");
    const token = requiredOption(args, "--token");
    const installed = await installProject(cwd, {
      apiUrl,
      projectId,
      token,
      archivePath: optionValue(args, "--archive") ?? undefined,
      extensionKey: optionValue(args, "--extension-key") ?? undefined,
      displayName: optionValue(args, "--display-name") ?? undefined,
      overwrite: hasFlag(args, "--overwrite"),
    });
    console.log(JSON.stringify(installed, null, 2));
  } else {
    printHelp();
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
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

function printHelp() {
  console.log(`agentdash-ext <command>

Commands:
  init [target] [--package <name>] [--extension-id <id>]
  validate [--cwd <path>] [--strict-bundles]
  pack [--cwd <path>] [--out-dir <path>]
  dev [--cwd <path>] [--host <host>] [--port <port>]
  install --api-url <url> --project <id> --token <token> [--archive <path>] [--overwrite]
`);
}
