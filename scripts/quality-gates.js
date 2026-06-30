#!/usr/bin/env node

import { spawnSync } from "node:child_process";

import {
  QUALITY_GATES,
  gateCommand,
  gateNames,
  getGate,
  resolveGateSteps,
  validateQualityGateManifest,
} from "./lib/quality-gates.js";

const [command = "list", gateName, ...rest] = process.argv.slice(2);
const json = rest.includes("--json") || gateName === "--json";

try {
  if (command === "list") {
    printList(json);
  } else if (command === "show") {
    printGate(requireGateName(gateName, command), json);
  } else if (command === "command") {
    console.log(gateCommand(requireGateName(gateName, command)));
  } else if (command === "run") {
    runGate(requireGateName(gateName, command));
  } else if (command === "check") {
    checkManifest(json);
  } else if (command === "expect-failure") {
    expectFailure(gateName, rest);
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

function runGate(name) {
  for (const step of resolveGateSteps(name)) {
    console.log(`\n> ${step.id}: ${step.run}`);
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

function requireGateName(name, commandName) {
  if (!name || name === "--json") {
    throw new Error(`quality-gates ${commandName} requires a gate name`);
  }
  return name;
}

function printUsage() {
  console.error(
    "Usage: node scripts/quality-gates.js <list|show|command|run|check> [gate] [--json]",
  );
}
