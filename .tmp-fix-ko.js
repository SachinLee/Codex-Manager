const fs = require("fs");
const p = "apps/src/lib/i18n/messages/ko.ts";
let text = fs.readFileSync(p, "utf8");
const old = '  "健康": "정상",\n  "检查中": "확인 중",\n  "保存用于系统级路由和诊断的代理配置。"';
const neu = '  "健康": "정상",\n  "保存用于系统级路由和诊断的代理配置。"';
if (!text.includes(old)) {
  // try CRLF
  const old2 = old.replace(/\n/g, "\r\n");
  const neu2 = neu.replace(/\n/g, "\r\n");
  if (!text.includes(old2)) {
    console.error("pattern not found");
    process.exit(1);
  }
  text = text.replace(old2, neu2);
} else {
  text = text.replace(old, neu);
}
fs.writeFileSync(p, text);
console.log("removed duplicate 检查中 from ko.ts");
