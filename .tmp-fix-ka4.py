from pathlib import Path
p = Path(r"crates/service/src/gateway/observability/tests/http_bridge_tests.rs")
text = p.read_text(encoding="utf-8")
# bump 1000 to 2000 in the two keepalive emission tests only (near those markers)
text = text.replace(
'''            // Keep gap well above interval + reader setup under parallel load.
            ("data: [DONE]\\n\\n", 1000),''',
'''            // Keep gap well above interval + reader setup under parallel load.
            ("data: [DONE]\\n\\n", 2000),''')
text = text.replace(
'''                // Keep well above interval + reader setup under parallel load.
                1000,''',
'''                // Keep well above interval + reader setup under parallel load.
                2000,''')
p.write_text(text, encoding='utf-8')
print('bumped to 2000')