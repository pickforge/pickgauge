import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const checker = join(repoRoot, "scripts", "check-osv-severity.mjs");
const fixtures = join(repoRoot, "tests", "fixtures", "osv");

function run(fixture, ...lockfiles) {
  return spawnSync(process.execPath, [checker, join(fixtures, fixture), ...lockfiles], {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

test("blocks a scored-high advisory even when it is informational", () => {
  const result = run("scored-high.json", "bun.lock");

  assert.equal(result.status, 1);
  assert.match(result.stderr, /GHSA-HIGH-0001/);
  assert.match(result.stderr, /raw max_severity "9\.8"/);
});

test("passes an advisory scored below 7", () => {
  const result = run("scored-low.json", "bun.lock");

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /high\/critical findings: 0/);
  assert.equal(result.stderr, "");
});

test("blocks missing, empty, and non-numeric advisory scores", () => {
  const result = run("unscored.json", "bun.lock");

  assert.equal(result.status, 1);
  assert.match(result.stderr, /GHSA-MISSING-0001.*raw max_severity undefined/);
  assert.match(result.stderr, /GHSA-EMPTY-0001.*raw max_severity ""/);
  assert.match(result.stderr, /GHSA-NONNUMERIC-0001.*raw max_severity "unknown"/);
});

test("skips unscored informational and withdrawn advisories", () => {
  const result = run("informational.json", "src-tauri/Cargo.lock");

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Skipped informational RUSTSEC-UNMAINTAINED-0001/);
  assert.match(result.stdout, /Skipped informational RUSTSEC-WITHDRAWN-0001/);
  assert.equal(result.stderr, "");
});

test("blocks an unscored group unless every advisory is informational", () => {
  const result = run("unscored-non-informational.json", "src-tauri/Cargo.lock");

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /RUSTSEC-UNMAINTAINED-0001, RUSTSEC-UNSCORED-0001.*raw max_severity undefined/,
  );
  assert.doesNotMatch(result.stdout, /Skipped informational/);
});

test("fails when an expected lockfile is missing from the report", () => {
  const result = run("scored-low.json", "src-tauri/Cargo.lock");

  assert.equal(result.status, 1);
  assert.match(result.stderr, /OSV report is missing lockfile: src-tauri\/Cargo\.lock/);
});
