import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");
const sourcePath = path.join(appsRoot, "src", "lib", "dashboard", "format.ts");

async function loadDashboardFormatModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const patchedSource = source.replace(
    'import { formatCompactNumber } from "@/lib/utils/usage";',
    `function formatCompactNumber(value, fallback = "-", maxFractionDigits = 1, preserveTrailingZeros = false) {
      if (typeof value !== "number" || !Number.isFinite(value)) return fallback;
      const units = [{ value: 1e9, suffix: "B" }, { value: 1e6, suffix: "M" }, { value: 1e3, suffix: "K" }];
      const normalized = Math.max(0, value);
      for (const unit of units) {
        if (normalized >= unit.value) {
          const fixed = (normalized / unit.value).toFixed(maxFractionDigits);
          return \`\${preserveTrailingZeros ? fixed : fixed.replace(/\\.0+$/, "").replace(/(\\.\\d*[1-9])0+$/, "$1")}\${unit.suffix}\`;
        }
      }
      return String(Math.round(normalized));
    }`,
  );
  const compiled = ts.transpileModule(patchedSource, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });

  const tempDir = await fs.mkdtemp(
    path.join(os.tmpdir(), "codexmanager-dashboard-format-"),
  );
  const tempFile = path.join(tempDir, "dashboard-format.mjs");
  await fs.writeFile(tempFile, compiled.outputText, "utf8");
  return import(pathToFileURL(tempFile).href);
}

const format = await loadDashboardFormatModule();

test("formatTokenAmountZh uses compact Chinese token units", () => {
  assert.equal(format.formatTokenAmountZh(0), "0");
  assert.equal(format.formatTokenAmountZh(999), "999");
  assert.equal(format.formatTokenAmountZh(12_300), "12.3K");
  assert.equal(format.formatTokenAmountZh(1_200_000), "1.2M");
  assert.equal(format.formatTokenAmountZh(12_000_000), "1.2千万");
  assert.equal(format.formatTokenAmountZh(120_000_000), "1.2亿");
});
