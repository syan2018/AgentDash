#!/usr/bin/env node

import { main } from './lib/release-metadata.js';

main(process.argv.slice(2)).catch((error) => {
  console.error(`[release-metadata] ${error.message}`);
  process.exit(1);
});
