import { spawnSync } from "node:child_process";

const RUNTIME_TESTS = [
  "tests/runtime-capabilities.test.mjs",
  "tests/gateway-endpoints.test.mjs",
  "tests/transport-errors.test.mjs",
  "tests/gateway-settings.test.mjs",
  "tests/settings-page-helpers.test.mjs",
  "tests/transport-web-commands.test.mjs",
  "tests/request-logs-layout.test.mjs",
  "tests/codex-profile-cache.test.mjs",
  "tests/i18n-page-coverage.test.mjs",
  "tests/account-list-cache.test.mjs",
  "tests/tauri-command-registry.test.mjs",
  "tests/dashboard-direct-mode.test.mjs",
  "tests/rpc-http.test.mjs",
  "tests/app-updates.test.mjs",
  "tests/account-auth.test.mjs",
  "tests/account-maintenance.test.mjs",
  "tests/api-key-quota.test.mjs",
  "tests/usage-response.test.mjs",
  "tests/ccswitch.test.mjs",
  "tests/billing-mode-lock.test.mjs",
  "tests/top-level-routes.test.mjs",
  "tests/timeout.test.mjs",
  "tests/request-utils.test.mjs",
  "tests/app-bootstrap-startup.test.mjs",
];

function normalizeFilter(value) {
  return value
    .replace(/^--/, "")
    .replace(/^tests[\\/]/, "")
    .replace(/\.test\.mjs$/, "")
    .trim();
}

function matchesFilter(testPath, filter) {
  const normalizedPath = normalizeFilter(testPath);
  return (
    normalizedPath === filter ||
    normalizedPath.endsWith(`/${filter}`) ||
    normalizedPath.endsWith(`\\${filter}`)
  );
}

const filters = process.argv.slice(2).map(normalizeFilter).filter(Boolean);
const selected =
  filters.length === 0
    ? RUNTIME_TESTS
    : RUNTIME_TESTS.filter((testPath) =>
        filters.some((filter) => matchesFilter(testPath, filter)),
      );

if (selected.length === 0) {
  console.error(`No runtime tests matched: ${filters.join(", ")}`);
  process.exit(1);
}

const result = spawnSync(process.execPath, ["--test", ...selected], {
  stdio: "inherit",
});

process.exit(result.status ?? 1);
