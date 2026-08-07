from pathlib import Path
path = Path(r"crates/service/src/gateway/upstream/support/payload_rewrite.rs")
text = path.read_text(encoding="utf-8")
old = '''fn strip_encrypted_content_value(value: &mut Value) -> bool {
    match value {
        Value::Object(map) => {
            let mut changed = map.remove("encrypted_content").is_some();
            for child in map.values_mut() {
                if strip_encrypted_content_value(child) {
                    changed = true;
                }
            }
            changed
        }
        Value::Array(items) => {
            let mut changed = false;
            for item in items.iter_mut() {
                if strip_encrypted_content_value(item) {
                    changed = true;
                }
            }
            changed
        }
        _ => false,
    }
}
'''
new = '''fn item_requires_encrypted_content(value: &Value) -> bool {
    let Value::Object(map) = value else {
        return false;
    };
    if !map.contains_key("encrypted_content") {
        return false;
    }
    map.get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|item_type| {
            item_type.eq_ignore_ascii_case("reasoning")
                || item_type.eq_ignore_ascii_case("encrypted_content")
        })
}

fn strip_encrypted_content_value(value: &mut Value) -> bool {
    match value {
        Value::Object(map) => {
            let mut changed = map.remove("encrypted_content").is_some();
            for child in map.values_mut() {
                if strip_encrypted_content_value(child) {
                    changed = true;
                }
            }
            changed
        }
        Value::Array(items) => {
            let mut changed = false;
            items.retain_mut(|item| {
                if item_requires_encrypted_content(item) {
                    changed = true;
                    return false;
                }
                if strip_encrypted_content_value(item) {
                    changed = true;
                }
                true
            });
            changed
        }
        _ => false,
    }
}
'''
if old not in text:
    raise SystemExit('strip block not found')
path.write_text(text.replace(old, new, 1), encoding='utf-8')
print('payload_rewrite fixed')
