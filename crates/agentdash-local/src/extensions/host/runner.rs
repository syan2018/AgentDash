pub(super) const EXTENSION_HOST_RUNNER: &str = r#"
import fs from "node:fs/promises";
import path from "node:path";
import readline from "node:readline";
import vm from "node:vm";
import { pathToFileURL } from "node:url";

let active = null;
let currentActionKey = null;
let nextHostApiId = 1;
const pendingHostApi = new Map();

const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });

function send(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function log(level, message) {
  send({ kind: "log", level, message: String(message) });
}

function toJsonValue(value) {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  if (Array.isArray(value)) return value.map(toJsonValue);
  if (typeof value === "object") {
    const result = {};
    for (const [key, item] of Object.entries(value)) {
      if (typeof item !== "function" && typeof item !== "symbol" && typeof item !== "undefined") {
        result[key] = toJsonValue(item);
      }
    }
    return result;
  }
  return null;
}

function createExtensionContext() {
  const actions = new Map();
  const contributions = {
    commands: [],
    flags: [],
    runtime_actions: [],
    workspace_panels: [],
    permissions: [],
  };
  const ctx = {
    api: {
      runtime: {
        async invoke(actionKey, input) {
          return await requestHostApi("runtime.invoke", { action_key: actionKey, input: toJsonValue(input) });
        },
      },
      local: {
        async getProfile() {
          return await requestHostApi("local.get_profile", { action_key: currentActionKey });
        },
      },
    },
    commands: {
      registerCommand(definition) {
        contributions.commands.push(toJsonValue(definition));
      },
    },
    flags: {
      registerFlag(definition) {
        contributions.flags.push(toJsonValue(definition));
      },
    },
    runtime: {
      registerAction(definition) {
        if (!definition || typeof definition.action_key !== "string" || typeof definition.invoke !== "function") {
          throw new Error("runtime action must include action_key and invoke");
        }
        actions.set(definition.action_key, definition);
        const { invoke, ...serializable } = definition;
        contributions.runtime_actions.push(toJsonValue(serializable));
      },
    },
    workspace: {
      registerPanel(definition) {
        contributions.workspace_panels.push(toJsonValue(definition));
      },
    },
    permissions: {
      require(permission) {
        contributions.permissions.push(toJsonValue(permission));
      },
    },
    contributions,
  };
  return { ctx, actions, contributions };
}

async function requestHostApi(method, params) {
  const id = `host-api-${nextHostApiId++}`;
  send({ kind: "host_api_request", id, method, params: toJsonValue(params) });
  return await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      pendingHostApi.delete(id);
      reject(new Error(`host api timeout: ${method}`));
    }, 30000);
    pendingHostApi.set(id, {
      resolve(value) {
        clearTimeout(timeout);
        resolve(value);
      },
      reject(error) {
        clearTimeout(timeout);
        reject(error);
      },
    });
  });
}

async function loadExtension(bundlePath) {
  const source = await fs.readFile(bundlePath, "utf8");
  const moduleUrl = pathToFileURL(path.resolve(bundlePath)).href;
  const context = vm.createContext({
    console: {
      log: (...args) => log("info", args.join(" ")),
      warn: (...args) => log("warn", args.join(" ")),
      error: (...args) => log("error", args.join(" ")),
    },
    setTimeout,
    clearTimeout,
    structuredClone,
    TextDecoder,
    TextEncoder,
  });
  const module = new vm.SourceTextModule(source, {
    context,
    identifier: `${moduleUrl}?t=${Date.now()}`,
    initializeImportMeta(meta) {
      meta.url = moduleUrl;
    },
    importModuleDynamically(specifier) {
      throw new Error(`extension bundle must be self-contained; dynamic import blocked: ${specifier}`);
    },
  });
  await module.link((specifier) => {
    throw new Error(`extension bundle must be self-contained; import blocked: ${specifier}`);
  });
  await module.evaluate();
  const exported = module.namespace.default ?? module.namespace.extension;
  if (!exported || typeof exported !== "object") {
    throw new Error("extension bundle must export a default extension object");
  }
  return exported;
}

async function activate(params) {
  const extension = await loadExtension(params.bundle_path);
  const { ctx, actions, contributions } = createExtensionContext();
  active = {
    extension,
    manifest: params.manifest,
    extensionKey: params.extension_key,
    actions,
    contributions,
  };
  if (typeof extension.activate === "function") {
    await extension.activate(ctx);
  }
  return healthPayload();
}

async function deactivate() {
  if (active?.extension && typeof active.extension.deactivate === "function") {
    await active.extension.deactivate();
  }
  active = null;
  return healthPayload();
}

async function invokeAction(params) {
  if (!active) throw new Error("extension is not active");
  const actionKey = params.action_key;
  const action = active.actions.get(actionKey);
  if (!action) throw new Error(`extension action is not registered: ${actionKey}`);
  const previous = currentActionKey;
  currentActionKey = actionKey;
  try {
    return toJsonValue(await action.invoke(toJsonValue(params.input)));
  } finally {
    currentActionKey = previous;
  }
}

function healthPayload() {
  return {
    active: Boolean(active),
    extension_id: active?.manifest?.extension_id ?? null,
    action_keys: active ? [...active.actions.keys()].sort() : [],
    pid: process.pid,
  };
}

async function handleRequest(message) {
  switch (message.method) {
    case "activate":
      return await activate(message.params ?? {});
    case "reload":
      await deactivate();
      return await activate(message.params ?? {});
    case "deactivate":
      return await deactivate();
    case "invoke_action":
      return await invokeAction(message.params ?? {});
    case "health":
      return healthPayload();
    default:
      throw new Error(`unknown extension host method: ${message.method}`);
  }
}

rl.on("line", (line) => {
  void (async () => {
    const message = JSON.parse(line);
    if (message.kind === "host_api_response") {
      const pending = pendingHostApi.get(message.id);
      if (!pending) return;
      pendingHostApi.delete(message.id);
      if (message.error) pending.reject(new Error(message.error));
      else pending.resolve(message.result ?? null);
      return;
    }
    if (message.kind !== "request") return;
    try {
      const result = await handleRequest(message);
      send({ kind: "response", id: message.id, result: toJsonValue(result) });
    } catch (error) {
      send({ kind: "response", id: message.id, error: error instanceof Error ? error.message : String(error) });
    }
  })().catch((error) => {
    send({ kind: "log", level: "error", message: error instanceof Error ? error.message : String(error) });
  });
});
"#;
