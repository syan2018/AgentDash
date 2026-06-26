#!/usr/bin/env node

import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { runDesktopBuild } from './lib/desktop-build.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, '..');

runDesktopBuild({
  root,
  tauriConfigPath: 'crates/agentdash-local-tauri/tauri.conf.json',
  defaultApiMode: 'builtin',
  defaultApiOrigin: 'http://127.0.0.1:17301',
});
