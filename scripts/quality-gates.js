#!/usr/bin/env node

import { spawnSync } from "node:child_process";

import {
  QUALITY_GATES,
  gateCommand,
  gateNames,
  getStep,
  getGate,
  resolveGateSteps,
  validateQualityGateManifest,
} from "./lib/quality-gates.js";

const cliArgs = process.argv.slice(2);
const normalizedArgs = cliArgs.at(0) === "--" ? cliArgs.slice(1) : cliArgs;
const [command = "list", ...commandArgs] = normalizedArgs;

try {
  if (command === "list" || command === "--json") {
    printList(command === "--json" || hasFlag(commandArgs, "--json"));
  } else if (command === "show") {
    printGate(requireName(firstPositional(commandArgs), command), hasFlag(commandArgs, "--json"));
  } else if (command === "command") {
    console.log(gateCommand(requireName(firstPositional(commandArgs), command)));
  } else if (command === "run") {
    runGate(requireName(firstPositional(commandArgs), command), {
      dryRun: hasFlag(commandArgs, "--dry-run"),
    });
  } else if (command === "run-step") {
    runCommandStep(getStep(requireName(firstPositional(commandArgs), command)), {
      dryRun: hasFlag(commandArgs, "--dry-run"),
    });
  } else if (command === "check") {
    checkManifest(hasFlag(commandArgs, "--json"));
  } else if (command === "expect-failure") {
    expectFailure(commandArgs.at(0), commandArgs.slice(1));
  } else {
    printUsage();
    process.exit(1);
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}

function printList(asJson) {
  if (asJson) {
    console.log(JSON.stringify({ gates: gateNames() }, null, 2));
    return;
  }

  for (const name of gateNames()) {
    console.log(`${name} - ${QUALITY_GATES[name].description}`);
  }
}

function printGate(name, asJson) {
  const gate = getGate(name);
  const steps = resolveGateSteps(name);
  const payload = {
    name,
    description: gate.description,
    steps,
    command: gateCommand(name),
  };

  if (asJson) {
    console.log(JSON.stringify(payload, null, 2));
    return;
  }

  console.log(`${name} - ${gate.description}`);
  for (const [index, step] of steps.entries()) {
    console.log(`${index + 1}. ${step.id}: ${step.run}`);
  }
}

function checkManifest(asJson) {
  const result = validateQualityGateManifest();
  if (asJson) {
    console.log(JSON.stringify(result, null, 2));
  } else if (result.ok) {
    console.log(
      `quality gate manifest passed (${result.gate_count} gates, ${result.step_count} steps)`,
    );
  } else {
    for (const error of result.errors) {
      console.error(error);
    }
  }

  if (!result.ok) {
    process.exit(1);
  }
}

function runGate(name, options) {
  const steps = resolveGateSteps(name);
  for (const [index, step] of steps.entries()) {
    runCommandStep(step, { ...options, index, total: steps.length });
  }
}

function runCommandStep(step, options) {
  const prefix =
    typeof options.index === "number" && typeof options.total === "number"
      ? `[${options.index + 1}/${options.total}] `
      : "";
  console.log(`\n> ${prefix}${step.id}: ${step.run}`);
  if (options.dryRun) {
    return;
  }

  const result = spawnSync(step.run, {
    shell: true,
    stdio: "inherit",
  });

  if (result.error) {
    throw result.error;
  }
  if ((result.status ?? 1) !== 0) {
    process.exit(result.status ?? 1);
  }
}

function expectFailure(separator, args) {
  if (separator !== "--" || args.length === 0) {
    throw new Error("quality-gates expect-failure requires: -- <command> [args...]");
  }

  const [binary, ...binaryArgs] = args;
  const result = spawnSync(binary, binaryArgs, {
    stdio: "inherit",
  });

  if (result.error) {
    throw result.error;
  }
  if ((result.status ?? 1) === 0) {
    throw new Error("Expected command to fail, but it succeeded");
  }
}

function requireName(name, commandName) {
  if (!name) {
    throw new Error(`quality-gates ${commandName} requires a gate name`);
  }
  return name;
}

function firstPositional(args) {
  return args.find((arg) => !arg.startsWith("--"));
}

function hasFlag(args, flag) {
  return args.includes(flag);
}

function printUsage() {
  console.error(
    "Usage: node scripts/quality-gates.js <list|show|command|run|run-step|check> [gate-or-step] [--json] [--dry-run]",
  );
}
