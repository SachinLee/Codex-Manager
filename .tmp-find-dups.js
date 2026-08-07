const fs = require("fs");
const text = fs.readFileSync("apps/src/lib/i18n/messages/ko.ts", "utf8");
const keys = [...text.matchAll(/^\s*"([^"]+)"\s*:/gm)].map((m) => m[1]);
const counts = new Map();
const positions = new Map();
let i = 0;
for (const line of text.split(/\r?\n/)) {
  i += 1;
  const m = line.match(/^\s*"([^"]+)"\s*:/);
  if (!m) continue;
  const k = m[1];
  counts.set(k, (counts.get(k) || 0) + 1);
  if (!positions.has(k)) positions.set(k, []);
  positions.get(k).push(i);
}
const dups = [...counts.entries()].filter(([, c]) => c > 1).sort((a, b) => b[1] - a[1]);
console.log("total keys", keys.length, "dups", dups.length);
for (const [k, c] of dups) {
  console.log(c, JSON.stringify(k), "lines", positions.get(k).join(","));
}
