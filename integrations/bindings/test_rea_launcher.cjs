const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { configuration, javaMajor } = require('./rea-launcher.cjs');
const originalCwd = process.cwd();
const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ontology-rea-launcher-'));
try {
  const pkg = path.join(root, 'package');
  fs.mkdirSync(pkg);
  fs.writeFileSync(path.join(pkg, 'package.json'), JSON.stringify({ name: 'rea-agents', version: '6.5.0' }));
  const java = path.join(root, 'jdk21');
  const old = path.join(root, 'jdk17');
  for (const [dir, major] of [[java, 21], [old, 17]]) {
    fs.mkdirSync(dir); fs.writeFileSync(path.join(dir, 'release'), `JAVA_VERSION="${major}.0.1"\n`);
  }
  const env = { ONTOLOGY_REA_PACKAGE_ROOT: pkg, JAVA_HOME: java, PATH: '/bin' };
  assert.equal(configuration(env, '/selected-node/bin/node').javaHome, java);
  assert.equal(configuration({ ...env, JAVA_HOME: old, ONTOLOGY_REA_JAVA_HOME: java }, '/selected-node/bin/node').javaHome, java);
  assert.equal(configuration(env, '/selected-node/bin/node').searchPath, '/selected-node/bin:/bin');
  assert.throws(() => configuration({ ...env, JAVA_HOME: old }, '/node'), /JDK/);
  assert.throws(() => configuration({ ...env, ONTOLOGY_REA_PACKAGE_ROOT: 'relative' }, '/node'), /absolute/);
  fs.writeFileSync(path.join(root, 'release'), 'JAVA_VERSION="21.0.1"\n');
  process.chdir(root);
  assert.equal(javaMajor(undefined), 0);
  assert.throws(() => configuration({ ...env, JAVA_HOME: undefined }, '/node'), /JDK/);
  assert.throws(() => configuration({ ...env, JAVA_HOME: 'jdk21' }, '/node'), /JDK/);
  process.chdir(originalCwd);
  fs.writeFileSync(path.join(pkg, 'package.json'), JSON.stringify({ name: 'rea-agents', version: '6.4.0' }));
  assert.throws(() => configuration(env, '/node'), /6.5.0/);
  process.stdout.write('9 launcher configuration checks passed; no native REA capability claimed\n');
} finally { process.chdir(originalCwd); fs.rmSync(root, { recursive: true, force: true }); }
