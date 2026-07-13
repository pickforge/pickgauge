import assert from "node:assert/strict";
import test from "node:test";
import {
  GROK_LIVE_EXTRACTION_ENABLED,
  GROK_USAGE_PROTOCOL_VERSION,
  extractGrokUsagePrototype,
  parseGrokUsageFixture,
  validateGrokLiveExtractionRequest,
} from "./grok-usage-extractor-prototype.mjs";

const validFixture = [
  "Grok Usage",
  "Weekly usage 37.5% used",
  "Resets 2026-07-19T20:00:00Z",
  "Product breakdown",
  "Build 61% used",
  "Chat 12.5% used",
].join("\n");

test("extracts only the allowlisted Grok usage contract", () => {
  assert.deepEqual(parseGrokUsageFixture(validFixture), {
    ok: true,
    status: "parsed",
    protocolVersion: GROK_USAGE_PROTOCOL_VERSION,
    usage: {
      usedPercent: 37.5,
      remainingPercent: 62.5,
      resetAt: "2026-07-19T20:00:00Z",
      products: [
        { product: "build", usedPercent: 61 },
        { product: "chat", usedPercent: 12.5 },
      ],
    },
  });
});

test("never returns raw fixture text or sensitive unknown fields", () => {
  const parsed = parseGrokUsageFixture(validFixture);
  const secret = "user@example.com session=secret-cookie-value";
  const rejected = parseGrokUsageFixture(`${validFixture} Account ${secret}`);
  const parsedJson = JSON.stringify(parsed);
  const rejectedJson = JSON.stringify(rejected);

  assert.equal(parsed.ok, true);
  assert.equal(parsedJson.includes("Grok Usage"), false);
  assert.equal(parsedJson.includes("Product breakdown"), false);
  assert.equal(rejected.ok, false);
  assert.equal(rejectedJson.includes(secret), false);
  assert.equal(rejectedJson.includes("user@example.com"), false);
  assert.equal(rejectedJson.includes("secret-cookie-value"), false);
});

test("rejects authentication and verification pages", () => {
  for (const text of [
    "Grok Usage Sign in with X",
    "Grok Usage Sign-in with X",
    "Grok Usage Login",
    "Grok Usage Continue with Google",
    "Grok Usage Password",
    "Grok Usage 2FA",
    "Grok Usage Verify you are human",
    "Grok Usage verification code",
  ]) {
    assert.equal(parseGrokUsageFixture(text).code, "auth_or_verification_page");
  }
});

test("fails closed on changed, ambiguous, or malformed usage summaries", () => {
  for (const text of [
    "Usage Weekly 37% used Product breakdown Build 12% used",
    validFixture.replace("37.5%", "137.5%"),
    validFixture.replace("2026-07-19T20:00:00Z", "next Sunday"),
    validFixture.replace("Weekly usage 37.5% used", "Weekly usage 37.5% used Weekly usage 38% used"),
    validFixture.replace("2026-07-19T20:00:00Z", "2026-02-30T20:00:00Z"),
    validFixture.replace("Weekly usage", "Account user@example.com Weekly usage"),
  ]) {
    assert.equal(parseGrokUsageFixture(text).ok, false);
  }
});

test("rejects missing, duplicated, malformed, and unknown product fields", () => {
  for (const text of [
    validFixture.replace("Build 61% used\nChat 12.5% used", ""),
    `${validFixture} Build 20% used`,
    validFixture.replace("Build 61% used", "Build 161% used"),
    `${validFixture} Account user@example.com`,
    `${validFixture} Credits $20`,
  ]) {
    assert.equal(parseGrokUsageFixture(text).ok, false);
  }
});

test("rounds computed remaining percentage to the protocol precision", () => {
  const parsed = parseGrokUsageFixture(validFixture.replace("37.5%", "99.99%"));

  assert.equal(parsed.ok, true);
  assert.equal(parsed.usage.remainingPercent, 0.01);
});

test("live extraction remains structurally disabled before touching a page", async () => {
  assert.equal(GROK_LIVE_EXTRACTION_ENABLED, false);
  const rejection = {
    ok: false,
    status: "rejected",
    code: "xai_permission_required",
    protocolVersion: GROK_USAGE_PROTOCOL_VERSION,
  };
  assert.deepEqual(validateGrokLiveExtractionRequest(), rejection);
  assert.deepEqual(await extractGrokUsagePrototype(pageThatMustNotBeRead()), rejection);
});

function pageThatMustNotBeRead() {
  return {
    locator() {
      throw new Error("permission gate must reject before reading the page");
    },
  };
}
