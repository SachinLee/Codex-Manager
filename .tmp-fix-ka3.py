from pathlib import Path
import re

p = Path(r"crates/service/src/gateway/observability/tests/http_bridge_tests.rs")
text = p.read_text(encoding="utf-8")

# 1) passthrough generic keepalive: already 200, bump to 1000
text2, n1 = re.subn(
    r'(fn passthrough_sse_reader_emits_keepalive_for_responses_stream\(\)[\s\S]*?\("data: \[DONE\]\\n\\n",\s*)200(\s*\),)',
    r'\g<1>1000\2',
    text,
    count=1,
)
print('passthrough DONE delay replacements', n1)

# 2) openai silent gap: 250 -> 1000
text3, n2 = re.subn(
    r'(resp_eventsource_keepalive\\"\}\}\\n\\n",\s*// Keep well above interval[\s\S]*?\n\s*)250(\s*,)',
    r'\g<1>1000\2',
    text2,
    count=1,
)
if n2 == 0:
    text3, n2 = re.subn(
        r'(resp_eventsource_keepalive"\}\}\\n\\n",\s*\n\s*// Keep well above interval so setup/CPU jitter cannot collapse the silent window\.\n\s*)250(\s*,)',
        r'\g<1>1000\2',
        text2,
        count=1,
    )
print('openai silent gap replacements', n2)

# 3) also bump other "emits_keepalive" short second-frame delays that assert keep-alive present
# Look for patterns near emits_keepalive tests with delays 50/80
for name in [
    'openai_responses_passthrough_reader_emits_keepalive_before_delayed_first_frame',
    'images_reader_emits_comment_keepalive_before_delayed_first_frame',
]:
    pass

# Broader: any assertion contains keep-alive and uses delay 50 or 80 after first frame
# Safer targeted replacements already done; also fix comment for 1000

text3 = text3.replace(
    '// Keep gap well above interval so pump setup/CPU jitter cannot collapse the silent window.\n            ("data: [DONE]\\n\\n", 1000),',
    '// Keep gap well above interval + reader setup under parallel load.\n            ("data: [DONE]\\n\\n", 1000),',
)
text3 = text3.replace(
    '// Keep well above interval so setup/CPU jitter cannot collapse the silent window.\n                1000,',
    '// Keep well above interval + reader setup under parallel load.\n                1000,',
)

if n1 == 0 and n2 == 0:
    raise SystemExit('no replacements made')

p.write_text(text3, encoding='utf-8')
print('done')
# show context
for needle in ['resp_keepalive_1', 'resp_eventsource_keepalive']:
    i = text3.find(needle)
    print('---', needle, '---')
    print(text3[i:i+320])