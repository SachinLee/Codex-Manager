import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");
const sourcePath = path.join(appsRoot, "src", "lib", "api", "aggregate-capabilities.ts");

async function loadModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });
  const tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "codexmanager-capabilities-"));
  const tempFile = path.join(tempDir, "aggregate-capabilities.mjs");
  await fs.writeFile(tempFile, compiled.outputText, "utf8");
  return import(pathToFileURL(tempFile).href);
}

const capabilityApi = await loadModule();

test("capability snapshot uses conservative defaults for missing and unknown fields", () => {
  const result = capabilityApi.normalizeAggregateApiCapabilities({
    apiId: "agg-1",
    routingMode: "future-mode",
    items: [{ capabilityKey: "responses.hosted_tool.image_generation" }],
  });

  assert.equal(result.routingMode, "enforce");
  assert.deepEqual(result.routingModeOptions, ["off", "observe", "enforce"]);
  assert.equal(result.items[0].effectiveState, "unknown");
  assert.equal(result.items[0].overrideState, "auto");
  assert.equal(result.items[0].scope.protocol, "responses");
});

test("capability attempts discard raw payload fields", () => {
  const attempts = capabilityApi.normalizeAggregateApiCapabilityAttempts({
    items: [{
      traceId: "trace-1",
      outcome: "rejected",
      prompt: "must-not-survive",
      requestBody: { secret: true },
    }],
  });

  assert.equal(attempts.length, 1);
  assert.equal(attempts[0].traceId, "trace-1");
  assert.equal(attempts[0].deliveryStarted, false);
  assert.equal("prompt" in attempts[0], false);
  assert.equal("requestBody" in attempts[0], false);
});
