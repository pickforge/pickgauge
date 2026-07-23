#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const [reportPath, ...expectedLockfiles] = process.argv.slice(2);

if (!reportPath || expectedLockfiles.length === 0) {
  throw new Error("Usage: check-osv-severity <report.json> <lockfile>...");
}

const report = JSON.parse(readFileSync(resolve(reportPath), "utf8"));
if (!Array.isArray(report.results)) {
  throw new Error("OSV report is missing its results array");
}

const scannedPaths = report.results.map((result) => result.source?.path).filter(Boolean);
for (const lockfile of expectedLockfiles) {
  const suffix = `/${lockfile}`;
  if (!scannedPaths.some((path) => path === lockfile || path.endsWith(suffix))) {
    throw new Error(`OSV report is missing lockfile: ${lockfile}`);
  }
}

const vulnerabilitiesById = new Map();
for (const result of report.results) {
  for (const entry of result.packages ?? []) {
    for (const vulnerability of entry.vulnerabilities ?? []) {
      if (vulnerability.id) {
        vulnerabilitiesById.set(vulnerability.id, vulnerability);
      }
    }
  }
}

const findings = [];
const informationalFindings = [];
for (const result of report.results) {
  for (const entry of result.packages ?? []) {
    for (const group of entry.groups ?? []) {
      const rawSeverity = group.max_severity;
      const severityText = String(rawSeverity ?? "").trim();
      const severity = Number(severityText);
      const ids = Array.isArray(group.ids) ? group.ids : [];
      const isUnscored = ids.length > 0 && (severityText === "" || !Number.isFinite(severity));
      const isInformational =
        isUnscored &&
        ids.every((id) => {
          const vulnerability = vulnerabilitiesById.get(id);
          return Boolean(
            vulnerability &&
              (vulnerability.database_specific?.informational ||
                vulnerability.affected?.some(
                  (affected) => affected.database_specific?.informational,
                ) ||
                vulnerability.withdrawn),
          );
        });
      const finding = {
        ids: ids.join(", ") || "unknown advisory",
        package: `${entry.package?.name ?? "unknown"}@${entry.package?.version ?? "unknown"}`,
        rawSeverity,
        source: result.source?.path ?? "unknown source",
      };

      if (isInformational) {
        informationalFindings.push(finding);
      } else if (isUnscored || (Number.isFinite(severity) && severity >= 7)) {
        findings.push(finding);
      }
    }
  }
}

console.log(
  `OSV scanned ${expectedLockfiles.length} lockfiles; high/critical findings: ${findings.length}`,
);
for (const finding of informationalFindings) {
  console.log(
    `Skipped informational ${finding.ids}: ${finding.package} in ${finding.source}`,
  );
}
for (const finding of findings) {
  console.error(
    `${finding.ids}: ${finding.package} (raw max_severity ${JSON.stringify(finding.rawSeverity)}) in ${finding.source}`,
  );
}

if (findings.length > 0) {
  process.exitCode = 1;
}
