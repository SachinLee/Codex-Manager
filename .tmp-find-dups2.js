const fs = require("fs");
const path = require("path");
const dir = "apps/src/lib/i18n/messages";
for (const file of fs.readdirSync(dir).filter((f) => f.endsWith(".ts"))) {
  const full = path.join(dir, file);
  const text = fs.readFileSync(full, "utf8");
  // match both "key": and bare identifier keys
  const keys = [];
  const positions = new Map();
  let i = 0;
  for (const line of text.split(/\r?\n/)) {
    i += 1;
    // skip comments/imports roughly
    const m =
      line.match(/^\s*"([^"]+)"\s*:/) ||
      line.match(/^\s*([A-Za-z_\u4e00-\u9fff][\w\u4e00-\u9fff]*)\s*:/);
    if (!m) continue;
    // ignore import/export type-ish
    if (/^(import|export|from|type|const|let|var|return|if|for|while)$/.test(m[1])) continue;
    const k = m[1];
    keys.push(k);
    if (!positions.has(k)) positions.set(k, []);
    positions.get(k).push(i);
  }
  const dups = [...positions.entries()].filter(([, lines]) => lines.length > 1);
  if (dups.length) {
    console.log("FILE", file, "dups", dups.length);
    for (const [k, lines] of dups.slice(0, 40)) {
      console.log(" ", JSON.stringify(k), "lines", lines.join(","));
    }
  }
}
