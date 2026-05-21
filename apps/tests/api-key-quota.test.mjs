import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");
const sourcePath = path.join(appsRoot, "src", "lib", "utils", "api-key-quota.ts");

async function loadApiKeyQuotaModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });

  const tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "codexmanager-api-key-quota-"));
  const tempFile = path.join(tempDir, "api-key-quota.mjs");
  await fs.writeFile(tempFile, compiled.outputText, "utf8");
  return import(pathToFileURL(tempFile).href);
}

const quota = await loadApiKeyQuotaModule();

test("parseQuotaLimitTokens treats M as million-token units", () => {
  assert.equal(quota.parseQuotaLimitTokens("1", "m"), 1_000_000);
  assert.equal(quota.parseQuotaLimitTokens("1.5", "m"), 1_500_000);
  assert.equal(quota.parseQuotaLimitTokens("250", "k"), 250_000);
});

test("formatQuotaLimitValue preserves million-token display", () => {
  assert.equal(quota.resolveQuotaLimitUnit(2_000_000), "m");
  assert.equal(quota.formatQuotaLimitValue(2_500_000, "m"), "2.5");
  assert.equal(quota.formatQuotaLimitValue(500_000, "k"), "500");
});
