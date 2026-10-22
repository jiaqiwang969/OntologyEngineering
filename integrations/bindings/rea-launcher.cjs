#!/usr/bin/env node
// Portable form of the locally maintained REA release launcher.
// Select a Node executable explicitly when invoking this file. No installation or discovery occurs.
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');

function javaMajor(home) {
  if (!home || !path.isAbsolute(home)) return 0;
  try {
    const release = fs.readFileSync(path.join(home, 'release'), 'utf8');
    return Number(release.match(/^JAVA_VERSION="(\d+)/m)?.[1] || 0);
  } catch { return 0; }
}

function configuration(env, nodeExecutable) {
  const packageRoot = env.ONTOLOGY_REA_PACKAGE_ROOT;
  if (!packageRoot || !path.isAbsolute(packageRoot)) {
    throw new Error('Set ONTOLOGY_REA_PACKAGE_ROOT to the absolute installed rea-agents 6.5.0 package directory');
  }
  const metadata = JSON.parse(fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8'));
  if (metadata.name !== 'rea-agents' || metadata.version !== '6.5.0') {
    throw new Error('REA launcher requires the selected rea-agents 6.5.0 release');
  }
  if (env.GHIDRA_INSTALL_DIR && !path.isAbsolute(env.GHIDRA_INSTALL_DIR)) {
    throw new Error('GHIDRA_INSTALL_DIR must be absolute');
  }
  let javaHome = env.JAVA_HOME;
  if (javaMajor(javaHome) < 21) {
    javaHome = env.ONTOLOGY_REA_JAVA_HOME;
    if (!javaHome || !path.isAbsolute(javaHome) || javaMajor(javaHome) < 21) {
      throw new Error('Select a JDK >= 21 with JAVA_HOME or ONTOLOGY_REA_JAVA_HOME');
    }
  }
  return {
    module: pathToFileURL(path.join(packageRoot, 'scripts/rea.mjs')).href,
    javaHome,
    searchPath: path.dirname(nodeExecutable) + path.delimiter + (env.PATH || ''),
  };
}

async function main() {
  const selected = configuration(process.env, process.execPath);
  process.env.JAVA_HOME = selected.javaHome;
  process.env.PATH = selected.searchPath;
  await import(selected.module);
}

module.exports = { javaMajor, configuration };
if (require.main === module) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
