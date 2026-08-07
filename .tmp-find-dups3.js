const fs = require("fs");
const path = require("path");
const dir = "apps/src/lib/i18n/messages";

function extractKeys(text) {
  // crude but effective for object-literal message catalogs
  const keys = [];
  // "quoted key":
  for (const m of text.matchAll(/(?:^|[,{]\s*)"([^"\\]*(?:\\.[^"\\]*)*)"\s*:/g)) {
    keys.push(m[1]);
  }
  // bare unicode/ident key:
  for (const m of text.matchAll(/(?:^|[,{]\s*)([A-Za-z_\u4e00-\u9fff$][\w\u4e00-\u9fff$]*)\s*:/g)) {
    const k = m[1];
    if (["import","export","from","type","const","let","var","return","if","for","while","true","false","null","undefined","as","satisfies"].includes(k)) continue;
    keys.push(k);
  }
  return keys;
}

for (const file of fs.readdirSync(dir).filter((f) => f.endsWith(".ts") && !f.includes("types"))) {
  // only top-level message files for now
  if (file.includes("sections")) continue;
  const full = path.join(dir, file);
  if (!fs.statSync(full).isFile()) continue;
  const text = fs.readFileSync(full, "utf8");
  const keys = extractKeys(text);
  const counts = new Map();
  for (const k of keys) counts.set(k, (counts.get(k) || 0) + 1);
  const dups = [...counts.entries()].filter(([, c]) => c > 1).sort((a,b)=>b[1]-a[1]);
  if (dups.length) {
    console.log("FILE", file, "total", keys.length, "dups", dups.length);
    for (const [k,c] of dups.slice(0, 50)) console.log(" ", c, JSON.stringify(k));
  }
}

// also sections
const sec = path.join(dir, "sections");
if (fs.existsSync(sec)) {
  for (const file of fs.readdirSync(sec).filter((f) => f.endsWith(".ts"))) {
    const full = path.join(sec, file);
    const text = fs.readFileSync(full, "utf8");
    const keys = extractKeys(text);
    const counts = new Map();
    for (const k of keys) counts.set(k, (counts.get(k) || 0) + 1);
    const dups = [...counts.entries()].filter(([, c]) => c > 1).sort((a,b)=>b[1]-a[1]);
    if (dups.length) {
      console.log("SECTION", file, "dups", dups.length);
      for (const [k,c] of dups.slice(0, 20)) console.log(" ", c, JSON.stringify(k));
    }
  }
}
