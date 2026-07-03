#!/usr/bin/env node
/**
 * Execute SQL against the current AgentDash development PostgreSQL.
 *
 * The script only manages connection discovery. It does not contain
 * business-specific diagnostics.
 *
 * Usage:
 *   node scripts/db-query.js --sql "select 1"
 *   node scripts/db-query.js --file ./tmp/query.sql
 *   Get-Content ./tmp/query.sql | node scripts/db-query.js
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, "..");
const isWindows = process.platform === "win32";

const args = parseArgs(process.argv.slice(2));

try {
  if (args.help) {
    printHelp();
    process.exit(0);
  }

  const sql = readSql(args);
  const connection = resolveConnection(args);
  runPsql(connection, sql);
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}

function printHelp() {
  console.log(`Usage:
  node scripts/db-query.js --sql "select 1"
  node scripts/db-query.js --file ./tmp/query.sql
  <command producing sql> | node scripts/db-query.js

Optional connection overrides:
  --url <postgres-url>
  --service <embedded-service-name>   default: agentdash_api when present
  --database <database-name>
  --port <port>
  --psql <path-to-psql>
`);
}

function parseArgs(argv) {
  const parsed = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--help" || arg === "-h") {
      parsed.help = true;
      continue;
    }
    if (!arg.startsWith("--")) {
      (parsed._ ??= []).push(arg);
      continue;
    }
    const eq = arg.indexOf("=");
    const key = arg.slice(2, eq === -1 ? undefined : eq);
    if (eq !== -1) {
      parsed[key] = arg.slice(eq + 1);
      continue;
    }
    const next = argv[i + 1];
    if (next == null || next.startsWith("--")) {
      parsed[key] = true;
    } else {
      parsed[key] = next;
      i += 1;
    }
  }
  return parsed;
}

function readSql(parsed) {
  if (typeof parsed.sql === "string" && parsed.sql.trim().length > 0) {
    return parsed.sql;
  }
  if (typeof parsed.file === "string" && parsed.file.trim().length > 0) {
    return fs.readFileSync(path.resolve(root, parsed.file), "utf8");
  }
  if (!process.stdin.isTTY) {
    const input = fs.readFileSync(0, "utf8");
    if (input.trim().length > 0) return input;
  }
  const positional = Array.isArray(parsed._) ? parsed._.join(" ") : "";
  if (positional.trim().length > 0) return positional;
  throw new Error("需要通过 --sql、--file、stdin 或位置参数传入 SQL");
}

function resolveConnection(parsed) {
  const psql = resolvePsql(parsed);
  if (typeof parsed.url === "string" && parsed.url.trim().length > 0) {
    return { kind: "url", psql, url: parsed.url.trim() };
  }

  const service = selectEmbeddedService(parsed, psql);
  const database = String(parsed.database ?? service.name);
  const password = fs.existsSync(service.pgpass)
    ? fs.readFileSync(service.pgpass, "utf8").trim()
    : "";
  return {
    kind: "embedded",
    psql,
    host: "127.0.0.1",
    port: service.port,
    user: "postgres",
    database,
    password,
  };
}

function selectEmbeddedService(parsed, psql) {
  const services = discoverEmbeddedServices();
  if (services.length === 0) {
    throw new Error("未发现 embedded Postgres；请先启动 dev runtime，或使用 --url");
  }

  if (typeof parsed.service === "string") {
    const found = services.find((service) => service.name === parsed.service);
    if (!found) {
      throw new Error(`未发现 embedded service: ${parsed.service}`);
    }
    if (!isServiceReachable(found, parsed, psql)) {
      throw new Error(`embedded service 不可连接: ${parsed.service} (${found.port})`);
    }
    return found;
  }

  if (typeof parsed.port === "string") {
    const port = Number.parseInt(parsed.port, 10);
    const found = services.find((service) => service.port === port);
    if (!found) {
      throw new Error(`未发现 embedded Postgres port: ${parsed.port}`);
    }
    if (!isServiceReachable(found, parsed, psql)) {
      throw new Error(`embedded Postgres port 不可连接: ${parsed.port}`);
    }
    return found;
  }

  const reachable = services.filter((service) => isServiceReachable(service, parsed, psql));
  if (reachable.length === 0) {
    const candidates = services.map((service) => `${service.name}:${service.port}`).join(", ");
    throw new Error(`未发现可连接的 embedded Postgres；候选: ${candidates}`);
  }
  return reachable.find((service) => service.name === "agentdash_api") ?? reachable[0];
}

function discoverEmbeddedServices() {
  const roots = new Set([
    path.join(root, ".agentdash", "embedded-postgres"),
  ]);
  if (process.env.AGENTDASH_DATA_ROOT) {
    roots.add(path.join(path.resolve(process.env.AGENTDASH_DATA_ROOT), ".agentdash", "embedded-postgres"));
  }
  if (isWindows && process.env.APPDATA) {
    roots.add(path.join(process.env.APPDATA, "AgentDash", "local-runtime", ".agentdash", "embedded-postgres"));
  }

  const services = [];
  for (const base of roots) {
    if (!fs.existsSync(base)) continue;
    for (const entry of fs.readdirSync(base, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const serviceDir = path.join(base, entry.name);
      const dataDir = path.join(serviceDir, "data");
      const pid = readPostmasterPid(dataDir);
      if (!pid) continue;
      services.push({
        name: entry.name,
        port: pid.port,
        pid: pid.pid,
        dataDir,
        pgpass: path.join(serviceDir, "pgpass"),
      });
    }
  }
  services.sort((left, right) => {
    if (left.name === "agentdash_api") return -1;
    if (right.name === "agentdash_api") return 1;
    return left.name.localeCompare(right.name);
  });
  return services;
}

function readPostmasterPid(dataDir) {
  const file = path.join(dataDir, "postmaster.pid");
  if (!fs.existsSync(file)) return null;
  const lines = fs.readFileSync(file, "utf8").split(/\r?\n/);
  const pid = Number.parseInt(lines[0] ?? "", 10);
  const port = Number.parseInt(lines[3] ?? "", 10);
  if (!Number.isFinite(pid) || !Number.isFinite(port)) return null;
  return { pid, port };
}

function isServiceReachable(service, parsed, psql) {
  const database = String(parsed.database ?? service.name);
  const password = fs.existsSync(service.pgpass)
    ? fs.readFileSync(service.pgpass, "utf8").trim()
    : "";
  const result = spawnSync(
    psql,
    [
      "-X",
      "-q",
      "-t",
      "-A",
      "-v",
      "ON_ERROR_STOP=1",
      "-h",
      "127.0.0.1",
      "-p",
      String(service.port),
      "-U",
      "postgres",
      "-d",
      database,
      "-c",
      "select 1;",
    ],
    {
      cwd: root,
      env: { ...process.env, PGPASSWORD: password },
      stdio: "ignore",
      timeout: 1500,
    },
  );
  return result.status === 0;
}

function resolvePsql(parsed) {
  if (typeof parsed.psql === "string" && parsed.psql.trim().length > 0) {
    return path.resolve(parsed.psql);
  }
  if (process.env.PSQL_BIN) {
    return process.env.PSQL_BIN;
  }
  const fromPath = findOnPath(isWindows ? "psql.exe" : "psql");
  if (fromPath) return fromPath;

  const theseus = path.join(os.homedir(), ".theseus", "postgresql");
  if (fs.existsSync(theseus)) {
    const found = fs.readdirSync(theseus, { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => path.join(theseus, entry.name, "bin", isWindows ? "psql.exe" : "psql"))
      .filter((candidate) => fs.existsSync(candidate))
      .sort((a, b) => b.localeCompare(a, undefined, { numeric: true }))[0];
    if (found) return found;
  }

  throw new Error("找不到 psql；请设置 PSQL_BIN 或传 --psql");
}

function findOnPath(name) {
  for (const dir of (process.env.PATH ?? "").split(path.delimiter)) {
    if (!dir) continue;
    const candidate = path.join(dir, name);
    if (fs.existsSync(candidate)) return candidate;
  }
  return null;
}

function runPsql(connection, sql) {
  const baseArgs = ["-X", "-v", "ON_ERROR_STOP=1"];
  const args = connection.kind === "url"
    ? [...baseArgs, connection.url, "-c", sql]
    : [
        ...baseArgs,
        "-h",
        connection.host,
        "-p",
        String(connection.port),
        "-U",
        connection.user,
        "-d",
        connection.database,
        "-c",
        sql,
      ];
  const env = connection.kind === "embedded"
    ? { ...process.env, PGPASSWORD: connection.password }
    : { ...process.env };
  const result = spawnSync(connection.psql, args, {
    cwd: root,
    env,
    stdio: "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
