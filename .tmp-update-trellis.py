from pathlib import Path
import json
task_path = Path('.trellis/tasks/07-17-integrate-main-custom-features/task.json')
task = json.loads(task_path.read_text(encoding='utf-8'))
task['commit'] = 'bd1484b9'
task['notes'] = (
    'First merge main@514f3dba (ae9aed07); second merge origin/main@482f7ffa (e099182a). '
    'Post-merge fixes: keepalive, token stats compile, fixture fields, schema ensure for '
    'cache_write/aggregate_api (15ff244f), encrypted-content item strip + latency test '
    'capacity (bd1484b9). cargo test -p codexmanager-core --lib: 401/401. '
    'cargo test -p codexmanager-service --lib: 4 previously failing tests now pass '
    '(full suite re-run optional). origin/main@482f7ffa fully contained. package.json untracked.'
)
task_path.write_text(json.dumps(task, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
impl = Path('.trellis/tasks/07-17-integrate-main-custom-features/implement.md')
text = impl.read_text(encoding='utf-8')
if 'bd1484b9' not in text:
    text = text.rstrip() + '''

## Progress log (2026-07-22 evening)

- Fixed `ensure_request_token_stats_table` dual schema after second main merge: cache_write + aggregate API columns, daily rollups ensure, hourly rollup aggregation. Core lib tests 401/401 (`15ff244f`).
- Fetched origin/main: still `482f7ffa`, already ancestor of integrate branch (no third merge needed).
- Service post-merge fixes (`bd1484b9`): restored main's `item_requires_encrypted_content` removal for reasoning/encrypted_content items; hardened proxy latency fake proxy accept capacity for warmup+10 samples under parallel load.
- Focused retests: strip_encrypted_content, stripped_candidate, images_reader partial events, latency 204 all pass.
'''
    impl.write_text(text, encoding='utf-8')
print('trellis updated')
