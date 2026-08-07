import json
from pathlib import Path
p = Path('.trellis/tasks/07-17-integrate-main-custom-features/task.json')
task = json.loads(p.read_text(encoding='utf-8'))
task['commit'] = '1742e652'
p.write_text(json.dumps(task, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
print(task['commit'], task['status'])