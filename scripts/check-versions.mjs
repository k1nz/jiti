#!/usr/bin/env node
/**
 * Align package.json / Cargo.toml / tauri.conf.json versions and identifier.
 * Used as an RC / CI gate so a stale 0.1.0 cannot ship.
 */
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

function read(rel) {
  return readFileSync(join(root, rel), 'utf8');
}

function jsonVersion(rel) {
  return JSON.parse(read(rel)).version;
}

const expected = jsonVersion('package.json');
const identifier = JSON.parse(read('packages/native/tauri.conf.json')).identifier;
const cargo = read('packages/native/Cargo.toml').match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const vite = read('packages/ui/vite.config.ts');

const versions = {
  'package.json': expected,
  'packages/ui/package.json': jsonVersion('packages/ui/package.json'),
  'packages/native/package.json': jsonVersion('packages/native/package.json'),
  'packages/native/tauri.conf.json': JSON.parse(read('packages/native/tauri.conf.json')).version,
  'packages/native/Cargo.toml': cargo,
};

const mismatch = Object.entries(versions).filter(([, v]) => v !== expected);
if (mismatch.length) {
  console.error(`version mismatch (expected ${expected}):`);
  for (const [file, v] of mismatch) console.error(`  ${file}: ${v}`);
  process.exit(1);
}

if (identifier !== 'com.jiti.app') {
  console.error(`identifier must be com.jiti.app, got ${identifier}`);
  process.exit(1);
}

if (!/sourcemap:\s*false/.test(vite)) {
  console.error('packages/ui/vite.config.ts must set build.sourcemap = false');
  process.exit(1);
}

const tauri = JSON.parse(read('packages/native/tauri.conf.json'));
if (tauri.bundle?.macOS?.signingIdentity !== '-') {
  console.error('RC macOS signingIdentity must be "-" (ad-hoc). Override in M5.1 via APPLE_SIGNING_IDENTITY.');
  process.exit(1);
}

console.log(`ok ${expected} identifier=${identifier}`);
