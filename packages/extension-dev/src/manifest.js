// @ts-check

import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";

export const MANIFEST_FILE = "agentdash.extension.json";
export const PACKAGE_JSON_FILE = "package.json";
export const LIFECYCLE_SCRIPTS = ["preinstall", "install", "postinstall", "prepare"];
const RUNTIME_DEPENDENCY_FIELDS = [
  "dependencies",
  "optionalDependencies",
  "peerDependencies",
  "bundleDependencies",
  "bundledDependencies",
];
const NATIVE_CONSTRAINT_FIELDS = ["gypfile", "binary", "os", "cpu", "libc"];

/**
 * @typedef {{ [key: string]: unknown }} UnknownRecord
 * @typedef {{ errors: string[], warnings: string[], manifest: UnknownRecord | null, package_json: UnknownRecord | null }} ValidationResult
 */

/**
 * @param {string} filePath
 * @returns {Promise<unknown>}
 */
export async function readJsonFile(filePath) {
  return JSON.parse(await readFile(filePath, "utf8"));
}

/**
 * @param {Buffer | Uint8Array | string} value
 * @returns {string}
 */
export function sha256Digest(value) {
  return `sha256:${createHash("sha256").update(value).digest("hex")}`;
}

/**
 * @param {string} projectRoot
 * @param {{ requireBundles?: boolean }} [options]
 * @returns {Promise<ValidationResult>}
 */
export async function validateProject(projectRoot, options = {}) {
  /** @type {string[]} */
  const errors = [];
  /** @type {string[]} */
  const warnings = [];
  const manifestPath = path.join(projectRoot, MANIFEST_FILE);
  const packagePath = path.join(projectRoot, PACKAGE_JSON_FILE);
  const manifest = asRecord(await readJsonFile(manifestPath));
  const packageJson = asRecord(await readJsonFile(packagePath));
  if (!manifest) {
    errors.push(`${MANIFEST_FILE} 必须是对象`);
  }
  if (!packageJson) {
    errors.push(`${PACKAGE_JSON_FILE} 必须是对象`);
  }
  if (manifest && packageJson) {
    validateManifest(manifest, errors);
    validatePackageJson(packageJson, manifest, errors);
    await validateBundleRefs(projectRoot, manifest, Boolean(options.requireBundles), errors, warnings);
  }
  return { errors, warnings, manifest, package_json: packageJson };
}

/**
 * @param {UnknownRecord} manifest
 * @param {string[]} errors
 */
export function validateManifest(manifest, errors) {
  requireString(manifest, "manifest_version", errors);
  requireString(manifest, "extension_id", errors);
  const packageInfo = asRecord(manifest.package);
  if (!packageInfo) {
    errors.push("manifest.package 必须是对象");
  } else {
    requireString(packageInfo, "name", errors, "manifest.package.name");
    requireString(packageInfo, "version", errors, "manifest.package.version");
  }
  requireString(manifest, "asset_version", errors);
  validateCommandDefs(arrayField(manifest, "commands"), errors);
  validateFlagDefs(arrayField(manifest, "flags"), errors);
  validateRuntimeActions(arrayField(manifest, "runtime_actions"), errors);
  validateProtocolChannels(arrayField(manifest, "protocol_channels"), errors);
  validateExtensionDependencies(arrayField(manifest, "extension_dependencies"), errors);
  validateWorkspaceTabs(arrayField(manifest, "workspace_tabs"), errors);
  validatePermissions(arrayField(manifest, "permissions"), errors);
  validateBundleDefs(arrayField(manifest, "bundles"), errors);
}

/**
 * @param {UnknownRecord} packageJson
 * @param {UnknownRecord} manifest
 * @param {string[]} errors
 */
export function validatePackageJson(packageJson, manifest, errors) {
  const manifestPackage = asRecord(manifest.package);
  const packageName = stringField(packageJson, "name");
  const packageVersion = stringField(packageJson, "version");
  if (!packageName) errors.push("package.json.name 不能为空");
  if (!packageVersion) errors.push("package.json.version 不能为空");
  if (manifestPackage) {
    const manifestName = stringField(manifestPackage, "name");
    const manifestVersion = stringField(manifestPackage, "version");
    if (packageName && manifestName && packageName !== manifestName) {
      errors.push(`package.json.name 与 manifest.package.name 不一致: ${packageName} != ${manifestName}`);
    }
    if (packageVersion && manifestVersion && packageVersion !== manifestVersion) {
      errors.push(`package.json.version 与 manifest.package.version 不一致: ${packageVersion} != ${manifestVersion}`);
    }
  }
  const scripts = asRecord(packageJson.scripts);
  if (scripts) {
    for (const key of LIFECYCLE_SCRIPTS) {
      if (Object.prototype.hasOwnProperty.call(scripts, key)) {
        errors.push(`package.json scripts.${key} 不允许出现在 extension package 中`);
      }
    }
  }
  validateNoRuntimeDependencies(packageJson, errors);
  validateNoNativeConstraints(packageJson, errors);
}

/**
 * @param {UnknownRecord} packageJson
 * @param {string[]} errors
 */
function validateNoRuntimeDependencies(packageJson, errors) {
  for (const field of RUNTIME_DEPENDENCY_FIELDS) {
    const value = packageJson[field];
    const hasRecordEntries = asRecord(value) ? Object.keys(asRecord(value) ?? {}).length > 0 : false;
    const hasArrayEntries = Array.isArray(value) && value.length > 0;
    if (hasRecordEntries || hasArrayEntries) {
      errors.push(`package.json.${field} 不允许出现在自包含 extension package 中`);
    }
  }
}

/**
 * @param {UnknownRecord} packageJson
 * @param {string[]} errors
 */
function validateNoNativeConstraints(packageJson, errors) {
  for (const field of NATIVE_CONSTRAINT_FIELDS) {
    if (Object.prototype.hasOwnProperty.call(packageJson, field)) {
      errors.push(`package.json.${field} 不允许出现在 extension package 中`);
    }
  }
}

/**
 * @param {string} projectRoot
 * @param {UnknownRecord} manifest
 * @param {boolean} requireBundles
 * @param {string[]} errors
 * @param {string[]} warnings
 */
export async function validateBundleRefs(projectRoot, manifest, requireBundles, errors, warnings) {
  for (const bundle of arrayField(manifest, "bundles")) {
    const record = asRecord(bundle);
    if (!record) continue;
    const entry = stringField(record, "entry");
    const digest = stringField(record, "digest");
    if (!entry || !digest) continue;
    const bundlePath = path.join(projectRoot, entry);
    try {
      const bytes = await readFile(bundlePath);
      const actual = sha256Digest(bytes);
      if (actual !== digest) {
        errors.push(`bundle ${entry} digest 不匹配: expected ${digest}, actual ${actual}`);
      }
    } catch (error) {
      if (requireBundles) {
        errors.push(`bundle ${entry} 文件不存在`);
      } else {
        warnings.push(`bundle ${entry} 尚未生成`);
      }
    }
  }
}

/**
 * @param {unknown[]} commands
 * @param {string[]} errors
 */
function validateCommandDefs(commands, errors) {
  for (const command of commands) {
    const record = asRecord(command);
    if (!record) {
      errors.push("commands[] 必须是对象");
      continue;
    }
    const name = stringField(record, "name");
    requireString(record, "description", errors, "commands[].description");
    if (!name) {
      errors.push("commands[].name 不能为空");
    } else if (name.startsWith("/") || name.includes("/")) {
      errors.push(`commands[].name 不应包含 /: ${name}`);
    }
    const handler = asRecord(record.handler);
    if (!handler || handler.kind !== "inject_message" || !stringField(handler, "content")) {
      errors.push("commands[].handler 必须是 inject_message 且包含 content");
    }
  }
}

/**
 * @param {unknown[]} flags
 * @param {string[]} errors
 */
function validateFlagDefs(flags, errors) {
  for (const flag of flags) {
    const record = asRecord(flag);
    if (!record) {
      errors.push("flags[] 必须是对象");
      continue;
    }
    requireString(record, "name", errors, "flags[].name");
    const type = stringField(record, "type");
    if (type !== "bool" && type !== "string") {
      errors.push("flags[].type 必须是 bool 或 string");
    } else if (type === "bool" && typeof record.default !== "boolean") {
      errors.push("flags[].default 必须匹配 bool");
    } else if (type === "string" && typeof record.default !== "string") {
      errors.push("flags[].default 必须匹配 string");
    }
  }
}

/**
 * @param {unknown[]} actions
 * @param {string[]} errors
 */
function validateRuntimeActions(actions, errors) {
  for (const action of actions) {
    const record = asRecord(action);
    if (!record) {
      errors.push("runtime_actions[] 必须是对象");
      continue;
    }
    validateQualifiedKey(record, "action_key", "runtime_actions[].action_key", errors);
    const kind = stringField(record, "kind");
    if (kind !== "session_runtime" && kind !== "setup") {
      errors.push("runtime_actions[].kind 必须是 session_runtime 或 setup");
    }
    validateJsonSchemaField(record, "input_schema", "runtime_actions[].input_schema", errors);
    validateJsonSchemaField(record, "output_schema", "runtime_actions[].output_schema", errors);
    validatePermissionStrings(arrayField(record, "permissions"), "runtime_actions[].permissions[]", errors);
  }
}

/**
 * @param {unknown[]} channels
 * @param {string[]} errors
 */
function validateProtocolChannels(channels, errors) {
  for (const channel of channels) {
    const record = asRecord(channel);
    if (!record) {
      errors.push("protocol_channels[] 必须是对象");
      continue;
    }
    validateNamespacedKey(record, "channel_key", "protocol_channels[].channel_key", errors);
    requireString(record, "version", errors, "protocol_channels[].version");
    requireString(record, "description", errors, "protocol_channels[].description");
    const methods = record.methods;
    if (!Array.isArray(methods) || methods.length === 0) {
      errors.push("protocol_channels[].methods 必须是非空数组");
      continue;
    }
    for (const method of methods) {
      const methodRecord = asRecord(method);
      if (!methodRecord) {
        errors.push("protocol_channels[].methods[] 必须是对象");
        continue;
      }
      validateMethodName(methodRecord, "name", "protocol_channels[].methods[].name", errors);
      requireString(methodRecord, "description", errors, "protocol_channels[].methods[].description");
      validateJsonSchemaField(
        methodRecord,
        "input_schema",
        "protocol_channels[].methods[].input_schema",
        errors,
      );
      validateJsonSchemaField(
        methodRecord,
        "output_schema",
        "protocol_channels[].methods[].output_schema",
        errors,
      );
      validatePermissionStrings(
        arrayField(methodRecord, "permissions"),
        "protocol_channels[].methods[].permissions[]",
        errors,
      );
    }
  }
}

/**
 * @param {unknown[]} dependencies
 * @param {string[]} errors
 */
function validateExtensionDependencies(dependencies, errors) {
  for (const dependency of dependencies) {
    const record = asRecord(dependency);
    if (!record) {
      errors.push("extension_dependencies[] 必须是对象");
      continue;
    }
    validateAlias(record, "alias", "extension_dependencies[].alias", errors);
    validatePackageKey(record, "extension_id", "extension_dependencies[].extension_id", errors);
    requireString(record, "version", errors, "extension_dependencies[].version");
    const channels = record.channels;
    if (!Array.isArray(channels) || channels.length === 0) {
      errors.push("extension_dependencies[].channels 必须是非空数组");
      continue;
    }
    for (const channel of channels) {
      validateNamespacedKeyValue(
        channel,
        "extension_dependencies[].channels[]",
        errors,
      );
    }
  }
}

/**
 * @param {unknown[]} tabs
 * @param {string[]} errors
 */
function validateWorkspaceTabs(tabs, errors) {
  for (const tab of tabs) {
    const record = asRecord(tab);
    if (!record) {
      errors.push("workspace_tabs[] 必须是对象");
      continue;
    }
    validateQualifiedKey(record, "type_id", "workspace_tabs[].type_id", errors);
    requireString(record, "label", errors, "workspace_tabs[].label");
    const scheme = stringField(record, "uri_scheme");
    if (!scheme || !/^[a-z][a-z0-9+.-]*$/.test(scheme)) {
      errors.push("workspace_tabs[].uri_scheme 必须是小写 URI scheme");
    }
    const renderer = asRecord(record.renderer);
    const rendererKind = stringField(renderer ?? {}, "kind");
    if (
      !renderer
      || (rendererKind !== "webview" && rendererKind !== "canvas_panel")
      || !stringField(renderer, "entry")
    ) {
      errors.push("workspace_tabs[].renderer 必须是 webview 或 canvas_panel 且包含 entry");
    }
  }
}

/**
 * @param {unknown[]} permissions
 * @param {string[]} errors
 */
function validatePermissions(permissions, errors) {
  for (const permission of permissions) {
    const record = asRecord(permission);
    if (!record) {
      errors.push("permissions[] 必须是对象");
      continue;
    }
    const kind = stringField(record, "kind");
    if (kind === "local_profile" || kind === "workspace") {
      const access = stringField(record, "access");
      if (access !== "read" && access !== "write" && access !== "read_write") {
        errors.push("permissions[].access 必须是 read、write 或 read_write");
      }
    } else if (kind === "http") {
      validateStringList(record, "hosts", "permissions[].hosts", errors);
      const access = stringField(record, "access");
      if (access !== "read" && access !== "write" && access !== "read_write") {
        errors.push("permissions[].access 必须是 read、write 或 read_write");
      }
    } else if (kind === "env") {
      validateStringList(record, "names", "permissions[].names", errors);
      const access = stringField(record, "access");
      if (access !== "read" && access !== "write" && access !== "read_write") {
        errors.push("permissions[].access 必须是 read、write 或 read_write");
      }
    } else if (kind === "process") {
      if (stringField(record, "access") !== "execute") {
        errors.push("permissions[].access 必须是 execute");
      }
    } else if (kind === "runtime_action") {
      validateQualifiedKey(record, "action_key", "permissions[].action_key", errors);
    } else if (kind === "extension_channel") {
      validateNamespacedKey(record, "channel_key", "permissions[].channel_key", errors);
      const methods = record.methods;
      if (!Array.isArray(methods) || methods.length === 0) {
        errors.push("permissions[].methods 必须是非空数组");
      } else {
        for (const method of methods) {
          validateMethodNameValue(method, "permissions[].methods[]", errors);
        }
      }
    } else {
      errors.push("permissions[].kind 非法");
    }
  }
}

/**
 * @param {unknown[]} bundles
 * @param {string[]} errors
 */
function validateBundleDefs(bundles, errors) {
  for (const bundle of bundles) {
    const record = asRecord(bundle);
    if (!record) {
      errors.push("bundles[] 必须是对象");
      continue;
    }
    if (record.kind !== "extension_host") {
      errors.push("bundles[].kind 必须是 extension_host");
    }
    requireString(record, "entry", errors, "bundles[].entry");
    const digest = stringField(record, "digest");
    if (!digest || !/^sha256:[0-9a-fA-F]{64}$/.test(digest)) {
      errors.push("bundles[].digest 必须是 sha256:<64 hex>");
    }
  }
}

/**
 * @param {UnknownRecord} record
 * @param {string} field
 * @param {string} label
 * @param {string[]} errors
 */
function validateQualifiedKey(record, field, label, errors) {
  const value = stringField(record, field);
  if (!value || !value.split(".").every((segment) => /^[a-z0-9_-]+$/.test(segment))) {
    errors.push(`${label} 必须由小写字母、数字、下划线、短横线和点分段组成`);
  }
}

/**
 * @param {UnknownRecord} record
 * @param {string} field
 * @param {string} label
 * @param {string[]} errors
 */
function validateNamespacedKey(record, field, label, errors) {
  validateNamespacedKeyValue(record[field], label, errors);
}

/**
 * @param {unknown} value
 * @param {string} label
 * @param {string[]} errors
 */
function validateNamespacedKeyValue(value, label, errors) {
  if (typeof value !== "string" || !value.includes(".")) {
    errors.push(`${label} 必须包含 provider namespace`);
    return;
  }
  if (!value.split(".").every((segment) => /^[a-z0-9_-]+$/.test(segment))) {
    errors.push(`${label} 必须由小写字母、数字、下划线、短横线和点分段组成`);
  }
}

/**
 * @param {UnknownRecord} record
 * @param {string} field
 * @param {string} label
 * @param {string[]} errors
 */
function validatePackageKey(record, field, label, errors) {
  const value = stringField(record, field);
  if (!value || !/^[a-z0-9_-]+$/.test(value)) {
    errors.push(`${label} 必须由小写字母、数字、下划线和短横线组成`);
  }
}

/**
 * @param {UnknownRecord} record
 * @param {string} field
 * @param {string} label
 * @param {string[]} errors
 */
function validateAlias(record, field, label, errors) {
  const value = stringField(record, field);
  if (!value || !/^[a-z][a-z0-9_-]*$/.test(value)) {
    errors.push(`${label} 必须以小写字母开头，并只包含小写字母、数字、下划线和短横线`);
  }
}

/**
 * @param {UnknownRecord} record
 * @param {string} field
 * @param {string} label
 * @param {string[]} errors
 */
function validateMethodName(record, field, label, errors) {
  validateMethodNameValue(record[field], label, errors);
}

/**
 * @param {unknown} value
 * @param {string} label
 * @param {string[]} errors
 */
function validateMethodNameValue(value, label, errors) {
  if (typeof value !== "string" || !/^[A-Za-z][A-Za-z0-9_]*$/.test(value)) {
    errors.push(`${label} 必须是合法 method 名称`);
  }
}

/**
 * @param {UnknownRecord} record
 * @param {string} field
 * @param {string} label
 * @param {string[]} errors
 */
function validateJsonSchemaField(record, field, label, errors) {
  const value = record[field];
  if (value == null || typeof value === "boolean" || asRecord(value)) return;
  errors.push(`${label} 必须是 JSON Schema 对象或布尔值`);
}

/**
 * @param {unknown[]} permissions
 * @param {string} label
 * @param {string[]} errors
 */
function validatePermissionStrings(permissions, label, errors) {
  for (const permission of permissions) {
    if (typeof permission !== "string" || !/^[A-Za-z0-9._:*=-]+$/.test(permission)) {
      errors.push(`${label} 必须是稳定 permission key`);
    }
  }
}

/**
 * @param {UnknownRecord} record
 * @param {string} field
 * @param {string} label
 * @param {string[]} errors
 */
function validateStringList(record, field, label, errors) {
  const values = record[field];
  if (!Array.isArray(values) || values.length === 0) {
    errors.push(`${label} 必须是非空字符串数组`);
    return;
  }
  for (const value of values) {
    if (typeof value !== "string" || value.trim() === "") {
      errors.push(`${label} 必须是非空字符串数组`);
      return;
    }
  }
}

/**
 * @param {UnknownRecord} record
 * @param {string} field
 * @param {string[]} errors
 * @param {string} [label]
 */
function requireString(record, field, errors, label = field) {
  if (!stringField(record, field)) {
    errors.push(`${label} 不能为空`);
  }
}

/**
 * @param {UnknownRecord} record
 * @param {string} field
 * @returns {string | null}
 */
function stringField(record, field) {
  const value = record[field];
  return typeof value === "string" && value.trim() !== "" ? value : null;
}

/**
 * @param {UnknownRecord} record
 * @param {string} field
 * @returns {unknown[]}
 */
function arrayField(record, field) {
  const value = record[field];
  return Array.isArray(value) ? value : [];
}

/**
 * @param {unknown} value
 * @returns {UnknownRecord | null}
 */
export function asRecord(value) {
  return value != null && typeof value === "object" && !Array.isArray(value)
    ? /** @type {UnknownRecord} */ (value)
    : null;
}
