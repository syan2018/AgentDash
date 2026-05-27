// @ts-check

import { readFile } from "node:fs/promises";

import { packProject } from "./pack.js";
import { sha256Digest } from "./manifest.js";

/**
 * @param {string} projectRoot
 * @param {{ apiUrl: string, projectId: string, token: string, archivePath?: string, extensionKey?: string, displayName?: string, overwrite?: boolean }} options
 */
export async function installProject(projectRoot, options) {
  const archive = options.archivePath
    ? { archive_path: options.archivePath, archive_digest: sha256Digest(await readFile(options.archivePath)) }
    : await packProject(projectRoot);
  const upload = await uploadArchive(options.apiUrl, options.projectId, options.token, archive);
  const artifactId = typeof upload.id === "string" ? upload.id : null;
  if (!artifactId) {
    throw new Error("AgentDash API upload response must include string id");
  }
  return await installArtifact(options.apiUrl, options.projectId, options.token, artifactId, {
    extension_key: options.extensionKey ?? null,
    display_name: options.displayName ?? null,
    overwrite: Boolean(options.overwrite),
  });
}

/**
 * @param {string} apiUrl
 * @param {string} projectId
 * @param {string} token
 * @param {{ archive_path: string, archive_digest: string }} archive
 */
async function uploadArchive(apiUrl, projectId, token, archive) {
  const form = new FormData();
  form.append("archive_digest", archive.archive_digest);
  form.append(
    "archive",
    new Blob([await readFile(archive.archive_path)], { type: "application/vnd.agentdash.extension+gzip" }),
    archive.archive_path.split(/[\\/]/).pop() ?? "extension.agentdash-extension.tgz",
  );
  const response = await fetch(`${apiUrl.replace(/\/$/, "")}/api/projects/${projectId}/extension-artifacts`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}` },
    body: form,
  });
  return await readJsonResponse(response);
}

/**
 * @param {string} apiUrl
 * @param {string} projectId
 * @param {string} token
 * @param {string} artifactId
 * @param {{ extension_key: string | null, display_name: string | null, overwrite: boolean }} body
 */
async function installArtifact(apiUrl, projectId, token, artifactId, body) {
  const response = await fetch(
    `${apiUrl.replace(/\/$/, "")}/api/projects/${projectId}/extension-artifacts/${artifactId}/install`,
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(body),
    },
  );
  return await readJsonResponse(response);
}

/**
 * @param {Response} response
 * @returns {Promise<Record<string, unknown>>}
 */
async function readJsonResponse(response) {
  const body = await response.text();
  if (!response.ok) {
    throw new Error(`AgentDash API ${response.status}: ${body}`);
  }
  const parsed = JSON.parse(body);
  if (parsed == null || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("AgentDash API response must be an object");
  }
  return /** @type {Record<string, unknown>} */ (parsed);
}
