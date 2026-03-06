#!/usr/bin/env node

/**
 * CURD CLI Wrapper for NPM distributions.
 * In a full production release (Phase C), this script will either execute
 * an embedded WASM/N-API port of the CLI, or download and spawn the 
 * pre-compiled native OS binary for maximum performance.
 */

const { spawnSync } = require('child_process');
const path = require('path');
const fs = require('fs');

// Attempt to find the native binary if compiled locally in the monorepo
const localBinary = path.resolve(__dirname, '../../target/release/curd');

let command = 'curd';
if (fs.existsSync(localBinary)) {
    command = localBinary;
}

const args = process.argv.slice(2);

const result = spawnSync(command, args, { stdio: 'inherit' });

if (result.error) {
    if (result.error.code === 'ENOENT') {
        console.error('\x1b[31mError: Native `curd` binary not found in PATH.\x1b[0m');
        console.error('Please ensure CURD is installed correctly or built via `make release`.');
        process.exit(1);
    }
    console.error(result.error);
    process.exit(1);
}

process.exit(result.status ?? 0);
