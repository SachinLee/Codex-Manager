import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const appsRoot = path.resolve(import.meta.dirname, "..");
const accountClientPath = path.join(appsRoot, "src", "lib", "api", "account-client.ts");
const hookPath = path.join(
  appsRoot,
  "src",
  "hooks",
  "useAggregateApiZeroBalanceStatuses.ts",
);
const pagePath = path.join(appsRoot, "src", "app", "aggregate-api", "page.tsx");

test("aggregate API page keeps zero-balance state and reset separate from cooldown", async () => {
  const [accountClient, hook, page] = await Promise.all([
    fs.readFile(accountClientPath, "utf8"),
    fs.readFile(hookPath, "utf8"),
    fs.readFile(pagePath, "utf8"),
  ]);

  assert.match(accountClient, /listAggregateApiZeroBalanceStatuses/);
  assert.match(accountClient, /resetAggregateApiZeroBalanceStatus/);
  assert.match(hook, /aggregate-api-zero-balance-status/);
  assert.match(hook, /listAggregateApiZeroBalanceStatuses/);
  assert.match(page, /useAggregateApiZeroBalanceStatuses\(isQueryEnabled\)/);
  assert.match(page, /resetAggregateApiZeroBalanceStatus\(apiId\)/);
  assert.match(page, /余额为 0/);
  assert.match(page, /解除余额禁用/);
  assert.match(page, /resetAggregateApiRuntimeStatus\(apiId\)/);
  assert.match(page, /aggregate-api-zero-balance-status/);
});
