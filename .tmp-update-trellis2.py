from pathlib import Path
import json
from datetime import datetime

task_path = Path('.trellis/tasks/07-17-integrate-main-custom-features/task.json')
task = json.loads(task_path.read_text(encoding='utf-8'))
task['notes'] = (
    "First merge main@514f3dba (ae9aed07); second merge origin/main@482f7ffa (e099182a). "
    "Post-merge fixes: keepalive variants, token stats schema ensure, encrypted-content strip. "
    "Test hardening: fake proxy accept window + full header read; keepalive silent gaps 2000ms; auth temp DB path uniqueness. "
    "Validation: cargo test -p codexmanager-core --lib 401/401; cargo test -p codexmanager-service --lib 1276/1276; "
    "cargo test -p codexmanager-web 25/25 (earlier). origin/main@482f7ffa still fully contained (no third merge needed). "
    "package.json untracked. Frontend build/runtime still pending."
)
task['status'] = 'in_progress'
# commit will update commit field after commit
task_path.write_text(json.dumps(task, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
print('task.json updated')

impl = Path('.trellis/tasks/07-17-integrate-main-custom-features/implement.md')
text = impl.read_text(encoding='utf-8')
stamp = datetime.now().strftime('%Y-%m-%d %H:%M')
log = f"""

## Progress log ({stamp})

- Hardened flaky service tests under full parallel suite:
  - `latency.rs` fake proxy: long overall accept deadline, full request-header read, blocking accepted sockets, 2 runtime workers
  - SSE keepalive emission tests: silent gap raised to 2000ms so reader setup under load cannot collapse the window
  - auth account temp DB path uniqueness (nanos suffix)
- Validation: `cargo test -p codexmanager-service --lib` → **1276 passed; 0 failed**
- `git fetch origin main`: origin/main still ancestor of integrate branch (no new main commits to merge)
- Remaining acceptance: `cargo test --workspace`, frontend `test:runtime` / `build` / `build:desktop`
"""
if 'Hardened flaky service tests under full parallel suite' not in text:
    impl.write_text(text.rstrip() + log, encoding='utf-8')
    print('implement.md updated')
else:
    print('implement.md already has log')