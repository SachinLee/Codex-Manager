from pathlib import Path
p = Path(r"crates/service/src/gateway/observability/tests/http_bridge_tests.rs")
text = p.read_text(encoding="utf-8")
old = '''            (
                "event: response.completed\\n\\
                 data: {\\"type\\":\\"response.completed\\",\\"response\\":{\\"id\\":\\"resp_eventsource_keepalive\\"}}\\n\\n",
                80,
            ),'''
new = '''            (
                "event: response.completed\\n\\
                 data: {\\"type\\":\\"response.completed\\",\\"response\\":{\\"id\\":\\"resp_eventsource_keepalive\\"}}\\n\\n",
                // Keep well above interval so setup/CPU jitter cannot collapse the silent window.
                250,
            ),'''
if old not in text:
    # try without escaped form from raw file
    old2 = '''                 data: {"type":"response.completed","response":{"id":"resp_eventsource_keepalive"}}\n\n",
                80,
            ),'''
    new2 = '''                 data: {"type":"response.completed","response":{"id":"resp_eventsource_keepalive"}}\n\n",
                // Keep well above interval so setup/CPU jitter cannot collapse the silent window.
                250,
            ),'''
    if old2 not in text:
        raise SystemExit('pattern not found')
    text = text.replace(old2, new2, 1)
else:
    text = text.replace(old, new, 1)
p.write_text(text, encoding='utf-8')
print('silent gap updated')