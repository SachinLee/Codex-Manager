import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const appsRoot = path.resolve(import.meta.dirname, "..");
const pageSectionsPath = path.join(
  appsRoot,
  "src",
  "app",
  "logs",
  "page-sections.tsx",
);

async function readPageSectionsSource() {
  return fs.readFile(pageSectionsPath, "utf8");
}

function indexOfOrThrow(source, needle) {
  const index = source.indexOf(needle);
  assert.notEqual(index, -1, `missing source fragment: ${needle}`);
  return index;
}

test("request logs table keeps route details after token metrics", async () => {
  const source = await readPageSectionsSource();
  const tableStart = indexOfOrThrow(source, '<Table className="min-w-');
  const tableSource = source.slice(tableStart);

  const accountHeader = indexOfOrThrow(tableSource, '{t("账号 / 密钥")}');
  const modelHeader = indexOfOrThrow(tableSource, '{t("模型 / 推理 / 等级")}');
  const tokenHeader = indexOfOrThrow(tableSource, '{t("Token")}');
  const routeHeader = indexOfOrThrow(tableSource, '{t("类型 / 方法 / 路径")}');
  const errorHeader = indexOfOrThrow(tableSource, '{t("错误")}');

  assert.ok(accountHeader < modelHeader);
  assert.ok(modelHeader < tokenHeader);
  assert.ok(tokenHeader < routeHeader);
  assert.ok(routeHeader < errorHeader);
});
