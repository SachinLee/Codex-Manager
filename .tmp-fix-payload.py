import subprocess
from pathlib import Path

def git_show(path):
    data = subprocess.check_output(['git', 'show', path])
    return data.decode('utf-8')

head = git_show('HEAD:crates/service/src/gateway/upstream/support/payload_rewrite.rs')
main_test = '''
    #[test]
    fn strip_encrypted_content_removes_items_that_require_the_field() {
        let body = json!({
            "model": "gpt-5.6-sol",
            "encrypted_content": "legacy-root-secret",
            "metadata": {
                "encrypted_content": "metadata-secret",
                "keep": "metadata",
                "reasoning_envelope": {
                    "type": "reasoning",
                    "id": "metadata-reasoning",
                    "summary": ["keep summary"],
                    "encrypted_content": "metadata-reasoning-secret"
                }
            },
            "input": [
                {
                    "type": "reasoning",
                    "id": "rs_1",
                    "summary": [],
                    "encrypted_content": "reasoning-secret"
                },
                {
                    "type": "agent_message",
                    "content": [
                        { "type": "input_text", "text": "keep me" },
                        {
                            "type": "encrypted_content",
                            "encrypted_content": "nested-secret"
                        }
                    ]
                },
                {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "continue" }]
                }
            ]
        });

        let rewritten = strip_encrypted_content_from_body(
            serde_json::to_vec(&body)
                .expect("serialize body")
                .as_slice(),
        )
        .expect("rewrite body");
        let value: Value = serde_json::from_slice(&rewritten).expect("parse rewritten body");

        assert!(!contains_encrypted_content(&value));
        assert_eq!(
            value["metadata"],
            json!({
                "keep": "metadata",
                "reasoning_envelope": {
                    "type": "reasoning",
                    "id": "metadata-reasoning",
                    "summary": ["keep summary"]
                }
            }),
            "ordinary object properties must remain after their encrypted field is removed"
        );

        let input = value["input"].as_array().expect("input array");
        assert_eq!(input.len(), 2, "reasoning item must be removed");
        assert_eq!(input[0]["type"], "agent_message");
        assert_eq!(
            input[0]["content"],
            json!([{ "type": "input_text", "text": "keep me" }]),
            "encrypted content part must be removed without dropping normal text"
        );
        assert_eq!(input[1]["type"], "message");
    }
'''

test_idx = head.index('#[cfg(test)]')
prefix = head[:test_idx]
tests = head[test_idx:]
if 'use serde_json::{json, Value};' not in tests:
    tests = tests.replace('use super::*;', 'use super::*;\n    use serde_json::{json, Value};', 1)
if 'fn contains_encrypted_content' not in tests:
    tests = tests.replace(
        'use serde_json::{json, Value};',
        '''use serde_json::{json, Value};

    fn contains_encrypted_content(value: &Value) -> bool {
        match value {
            Value::Object(map) => {
                map.contains_key("encrypted_content")
                    || map.values().any(contains_encrypted_content)
            }
            Value::Array(items) => items.iter().any(contains_encrypted_content),
            _ => false,
        }
    }''',
        1,
    )
if 'strip_encrypted_content_removes_items_that_require_the_field' not in tests:
    tests = tests.rstrip()
    if not tests.endswith('}'):
        raise SystemExit('tests module end missing')
    tests = tests[:-1].rstrip() + '\n' + main_test + '\n}\n'

final = prefix + tests
Path('crates/service/src/gateway/upstream/support/payload_rewrite.rs').write_text(final, encoding='utf-8')
print('fixed payload_rewrite')
print('markers', final.count('<<<<<<<'))
