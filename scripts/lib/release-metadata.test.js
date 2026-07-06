import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  discoverWindowsDesktopArtifacts,
  generateDesktopReleaseDirectory,
  parseArgs,
} from './release-metadata.js';

test('release metadata parser keeps existing --out and accepts desktop release options', () => {
  assert.deepEqual(parseArgs([
    '--out',
    'dist/release/agentdash-release.json',
    '--desktop-release-dir=dist/release/desktop',
    '--desktop-artifacts-dir',
    'target/release/bundle',
    '--object-key-prefix',
    'desktop',
  ]), {
    out: 'dist/release/agentdash-release.json',
    desktopReleaseDir: 'dist/release/desktop',
    desktopArtifactsDir: 'target/release/bundle',
    channel: 'stable',
    platform: 'windows',
    arch: 'x86_64',
    objectKeyPrefix: 'desktop',
  });
});

test('desktop release directory contains manifests, sha256 files, signature reference, and upload plan', () => {
  const tempRoot = makeTempRoot();
  const artifactsDir = path.join(tempRoot, 'target/release/bundle/nsis');
  fs.mkdirSync(artifactsDir, { recursive: true });
  fs.writeFileSync(path.join(artifactsDir, 'AgentDash_0.1.0_x64-setup.exe'), 'installer fixture');
  fs.writeFileSync(path.join(artifactsDir, 'AgentDash_0.1.0_x64.nsis.zip'), 'updater fixture');
  fs.writeFileSync(path.join(artifactsDir, 'AgentDash_0.1.0_x64.nsis.zip.sig'), 'fixture-signature');

  const summary = generateDesktopReleaseDirectory({
    root: tempRoot,
    releaseDir: 'dist/release/desktop',
    artifactsDir: 'target/release/bundle',
    metadata: {
      product: 'AgentDash',
      version: '0.1.0',
      git_sha: 'abc123def456',
      build_time: '2026-07-06T00:00:00.000Z',
      published_at: '2026-07-06T00:00:00.000Z',
      release_notes: 'Desktop updater fixture release',
    },
    channel: 'stable',
    platform: 'windows',
    arch: 'x86_64',
    objectKeyPrefix: 'desktop',
    productSlug: 'agentdash',
  });

  assert.deepEqual(summary, {
    release_dir: 'dist/release/desktop',
    release_manifest: 'releases/agentdash/0.1.0/release.json',
    stable_latest_manifest: 'channels/stable/latest.json',
    upload_plan: 'upload-plan.json',
    platforms: ['windows-x86_64'],
  });

  const releaseDir = path.join(tempRoot, 'dist/release/desktop');
  const releaseManifest = readJson(path.join(releaseDir, 'releases/agentdash/0.1.0/release.json'));
  const latestManifest = readJson(path.join(releaseDir, 'channels/stable/latest.json'));
  const uploadPlan = readJson(path.join(releaseDir, 'upload-plan.json'));

  const platform = releaseManifest.platforms['windows-x86_64'];
  assert.equal(releaseManifest.schema_version, 1);
  assert.equal(releaseManifest.channel, 'stable');
  assert.equal(releaseManifest.published_at, '2026-07-06T00:00:00.000Z');
  assert.equal(releaseManifest.release_notes, 'Desktop updater fixture release');
  assert.equal(platform.installer.object_key, 'desktop/releases/agentdash/0.1.0/windows/x86_64/AgentDash_0.1.0_x64-setup.exe');
  assert.match(platform.installer.sha256, /^[a-f0-9]{64}$/);
  assert.equal(platform.updater.signature, 'fixture-signature');
  assert.equal(platform.updater.signature_file, 'releases/agentdash/0.1.0/windows/x86_64/AgentDash_0.1.0_x64.nsis.zip.sig');
  assert.match(platform.updater.sha256, /^[a-f0-9]{64}$/);
  assert.equal(latestManifest.release_manifest.object_key, 'desktop/releases/agentdash/0.1.0/release.json');
  assert.equal(latestManifest.published_at, '2026-07-06T00:00:00.000Z');
  assert.equal(latestManifest.release_notes, 'Desktop updater fixture release');
  assert.equal(latestManifest.platforms['windows-x86_64'].updater.object_key, platform.updater.object_key);

  assert.ok(fs.existsSync(path.join(releaseDir, platform.installer.file)));
  assert.ok(fs.existsSync(path.join(releaseDir, platform.installer.sha256_file)));
  assert.ok(fs.existsSync(path.join(releaseDir, platform.updater.file)));
  assert.ok(fs.existsSync(path.join(releaseDir, platform.updater.sha256_file)));
  assert.ok(fs.existsSync(path.join(releaseDir, platform.updater.signature_file)));

  assert.equal(uploadPlan.object_storage.contract, 's3-compatible');
  assert.equal(uploadPlan.object_storage.public_base_url_env, 'AGENTDASH_DESKTOP_RELEASE_PUBLIC_BASE_URL');
  assert.equal(uploadPlan.object_storage.private_mapping_owner, 'private-deployment');
  assert.equal(uploadPlan.uploads.at(-1).local_path, 'channels/stable/latest.json');
  assert.equal(uploadPlan.uploads.at(-1).immutable, false);
  assert.ok(uploadPlan.uploads.every((entry) => entry.object_key.startsWith('desktop/')));
  assertNoPrivateObjectStorageFacts(uploadPlan);
});

test('desktop artifact discovery fails clearly when updater artifact is missing', () => {
  const tempRoot = makeTempRoot();
  const artifactsDir = path.join(tempRoot, 'bundle/nsis');
  fs.mkdirSync(artifactsDir, { recursive: true });
  fs.writeFileSync(path.join(artifactsDir, 'AgentDash_0.1.0_x64-setup.exe'), 'installer fixture');

  assert.throws(
    () => discoverWindowsDesktopArtifacts(path.join(tempRoot, 'bundle')),
    /未发现 Tauri updater artifact: 期望在 .* 下唯一匹配 \*\.nsis\.zip 或 \*\.msi\.zip/,
  );
});

test('desktop artifact discovery fails clearly when updater signature is missing', () => {
  const tempRoot = makeTempRoot();
  const artifactsDir = path.join(tempRoot, 'bundle/nsis');
  fs.mkdirSync(artifactsDir, { recursive: true });
  fs.writeFileSync(path.join(artifactsDir, 'AgentDash_0.1.0_x64-setup.exe'), 'installer fixture');
  fs.writeFileSync(path.join(artifactsDir, 'AgentDash_0.1.0_x64.nsis.zip'), 'updater fixture');

  assert.throws(
    () => discoverWindowsDesktopArtifacts(path.join(tempRoot, 'bundle')),
    /未发现 Tauri updater signature: .*\.nsis\.zip\.sig/,
  );
});

function makeTempRoot() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'agentdash-release-metadata-'));
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function assertNoPrivateObjectStorageFacts(value) {
  const serialized = JSON.stringify(value);
  assert.doesNotMatch(serialized, /AKIA|secret_access_key|access_key|bucket|endpoint|\.internal|\.corp/i);
}
