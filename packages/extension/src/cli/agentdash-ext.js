#!/usr/bin/env node

// @ts-check

import process from "node:process";

import { runAgentDashExtCli } from "../toolchain/cli.js";

process.exitCode = await runAgentDashExtCli();
