import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const validator = join(repoRoot, "scripts", "validate-headless-appimage.mjs");
const packageVersion = JSON.parse(readFileSync(join(repoRoot, "package.json"), "utf8")).version;

const root = mkdtempSync(join(tmpdir(), "pickgauge-headless-validator-smoke-"));
const fakeAppImage = join(root, "PickGauge.AppImage");

writeFileSync(
  fakeAppImage,
  `#!/bin/sh
set -eu
[ "\${APPIMAGE_EXTRACT_AND_RUN:-}" = "1" ]
[ -z "\${DISPLAY:-}" ]
[ -z "\${WAYLAND_DISPLAY:-}" ]
[ -z "\${XAUTHORITY:-}" ]
case "\${1:-}" in
  --version)
    printf 'pickgauge ${packageVersion}\\n'
    ;;
  usage)
    if [ "$#" -eq 1 ]; then
      case "\${PICKGAUGE_VALIDATOR_SCENARIO:-valid}" in
        invalid-human-header) printf 'Service Plan Week 5h Resets Source Staleness\\n' ;;
        *) printf 'Service      Plan             5h       Week     Resets                       Source   Staleness\\n' ;;
      esac
    elif [ "$#" -eq 2 ] && [ "\${2:-}" = "--json" ]; then
      case "\${PICKGAUGE_VALIDATOR_SCENARIO:-valid}" in
        gtk-stderr)
          printf 'Gtk-WARNING: cannot open display\\nthread main panicked at GTK initialization\\n' >&2
          printf '{"version":1,"services":[]}\\n'
          ;;
        malformed) printf 'not-json\\n' ;;
        invalid-schema) printf '{"version":2,"services":{}}\\n' ;;
        *) printf '{"version":1,"services":[]}\\n' ;;
      esac
    else
      exit 64
    fi
    ;;
  *) exit 64 ;;
esac
`,
  { mode: 0o755 },
);

function run(scenario) {
  return spawnSync(process.execPath, [validator, "--appimage", fakeAppImage], {
    cwd: repoRoot,
    encoding: "utf8",
    env: { ...process.env, PICKGAUGE_VALIDATOR_SCENARIO: scenario },
  });
}

function test(name, fn) {
  fn();
  console.log(`ok - ${name}`);
}

try {
  test("accepts valid deterministic headless AppImage output", () => {
    const result = run("valid");
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stderr, "");
    assert.match(result.stdout, /Validated headless AppImage:/);
  });

  test("rejects an invalid human usage header", () => {
    const result = run("invalid-human-header");
    assert.equal(result.status, 1);
    assert.match(result.stderr, /usage table header mismatch/);
  });

  test("rejects GTK panic-like stderr", () => {
    const result = run("gtk-stderr");
    assert.equal(result.status, 1);
    assert.match(result.stderr, /wrote stderr/);
    assert.match(result.stderr, /Gtk-WARNING/);
    assert.match(result.stderr, /panicked/);
  });

  test("rejects malformed usage output", () => {
    const result = run("malformed");
    assert.equal(result.status, 1);
    assert.match(result.stderr, /returned malformed JSON/);
  });

  test("rejects invalid usage schema", () => {
    const result = run("invalid-schema");
    assert.equal(result.status, 1);
    assert.match(result.stderr, /schema must be v1/);
  });
} finally {
  rmSync(root, { recursive: true, force: true });
}
