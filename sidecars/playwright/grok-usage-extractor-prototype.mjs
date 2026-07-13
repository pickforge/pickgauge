export const GROK_USAGE_PROTOCOL_VERSION = 1;
export const GROK_LIVE_EXTRACTION_ENABLED = false;


export function validateGrokLiveExtractionRequest() {
  return rejected("xai_permission_required");
}

export async function extractGrokUsagePrototype(page) {
  if (!GROK_LIVE_EXTRACTION_ENABLED) {
    return validateGrokLiveExtractionRequest();
  }
  const text = await page.locator("body").innerText({ timeout: 2_000 }).catch(() => "");
  return parseGrokUsageFixture(text);
}

export function parseGrokUsageFixture(input) {
  if (typeof input !== "string" || input.length === 0 || input.length > 20_000) {
    return rejected("invalid_fixture");
  }

  const text = input.replace(/\s+/gu, " ").trim();
  if (
    /\b(log\s*in|login|sign[\s-]*in|signin|sign[\s-]*up|authenticate|password|captcha|verify you are human|verification code|2fa|continue with)\b/iu.test(
      text,
    )
  ) {
    return rejected("auth_or_verification_page");
  }

  const fixture = /^Grok Usage Weekly usage ([0-9]{1,3}(?:\.[0-9]{1,2})?)% used Resets (\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z) Product breakdown (.+)$/iu.exec(
    text,
  );
  if (fixture === null) {
    return rejected("unexpected_ui");
  }

  const usedPercent = Number.parseFloat(fixture[1]);
  const resetAt = fixture[2];
  const resetMilliseconds = Date.parse(resetAt);
  if (
    !validPercent(usedPercent) ||
    Number.isNaN(resetMilliseconds) ||
    new Date(resetMilliseconds).toISOString() !== resetAt.replace("Z", ".000Z")
  ) {
    return rejected("invalid_usage_summary");
  }

  const productSection = fixture[3];
  const productMatches = [
    ...productSection.matchAll(
      /\b(API|Build|Chat|Imagine|Voice)\s+([0-9]{1,3}(?:\.[0-9]{1,2})?)% used\b/giu,
    ),
  ];
  if (productMatches.length === 0) {
    return rejected("missing_product_breakdown");
  }

  const seenProducts = new Set();
  const products = [];
  for (const match of productMatches) {
    const product = match[1].toLowerCase();
    const productUsedPercent = Number.parseFloat(match[2]);
    if (seenProducts.has(product) || !validPercent(productUsedPercent)) {
      return rejected("invalid_product_breakdown");
    }
    seenProducts.add(product);
    products.push({ product, usedPercent: productUsedPercent });
  }

  const unknownProductFields = productMatches.reduce(
    (section, match) => section.replace(match[0], ""),
    productSection,
  );
  if (unknownProductFields.trim().length > 0) {
    return rejected("unexpected_product_fields");
  }

  return {
    ok: true,
    status: "parsed",
    protocolVersion: GROK_USAGE_PROTOCOL_VERSION,
    usage: {
      usedPercent,
      remainingPercent: Number((100 - usedPercent).toFixed(2)),
      resetAt,
      products,
    },
  };
}

function validPercent(value) {
  return Number.isFinite(value) && value >= 0 && value <= 100;
}

function rejected(code) {
  return {
    ok: false,
    status: "rejected",
    protocolVersion: GROK_USAGE_PROTOCOL_VERSION,
    code,
  };
}
