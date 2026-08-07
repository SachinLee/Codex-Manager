import re, subprocess
from pathlib import Path

def keys_from_text(text):
    return sorted(set(re.findall(r"t\([\"']([^\"']{2,60})[\"']\)", text)))

def git_show(path):
    return subprocess.check_output(["git", "show", f"codex/custom-features:{path}"], text=True, errors="replace")

pairs = [
    ("aggregate-api", "apps/src/app/aggregate-api/page.tsx"),
    ("models", "apps/src/app/models/page.tsx"),
    ("logs", "apps/src/app/logs/page-sections.tsx"),
]
for name, path in pairs:
    custom = set(keys_from_text(git_show(path)))
    head = set(keys_from_text(Path(path).read_text(encoding="utf-8")))
    only_c = sorted(custom - head)
    only_h = sorted(head - custom)
    print(f"===== {name} =====")
    print(f"custom_keys={len(custom)} head_keys={len(head)} only_custom={len(only_c)} only_head={len(only_h)}")
    print("ONLY_CUSTOM:")
    for k in only_c[:60]:
        print(" -", k)
    print("ONLY_HEAD:")
    for k in only_h[:40]:
        print(" -", k)
