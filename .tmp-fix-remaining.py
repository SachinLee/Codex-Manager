from pathlib import Path

# accounts-page-view
p = Path('apps/src/app/accounts/accounts-page-view.tsx')
text = p.read_text(encoding='utf-8')
text = text.replace('''<<<<<<< HEAD
import type { Account, AccountDailyUsageStat } from "@/types";
=======
import type {
  AccountProxySettings,
  AccountProxySource,
} from "@/lib/api/account-client";
import type { Account, ProxyProfile } from "@/types";
import { AccountProxyCell } from "@/components/accounts/account-proxy-cell";
import { AccountProxyGeoStatusGrid } from "@/components/accounts/account-proxy-status-grid";
import { AccountProxyStatusHeader } from "@/components/accounts/account-proxy-status-header";
>>>>>>> origin/main''',
'''import type {
  AccountProxySettings,
  AccountProxySource,
} from "@/lib/api/account-client";
import type { Account, AccountDailyUsageStat, ProxyProfile } from "@/types";
import { AccountProxyCell } from "@/components/accounts/account-proxy-cell";
import { AccountProxyGeoStatusGrid } from "@/components/accounts/account-proxy-status-grid";
import { AccountProxyStatusHeader } from "@/components/accounts/account-proxy-status-header";''')
text = text.replace('''<<<<<<< HEAD
                <TableHead className="w-[170px]">{t("今日使用")}</TableHead>
                <TableHead className="w-[156px]">{t("顺序")}</TableHead>
                <TableHead>{t("状态")}</TableHead>
=======
                <TableHead className="w-[132px]">{t("顺序")}</TableHead>
                <TableHead className="min-w-[180px]">{t("账号代理")}</TableHead>
                <TableHead className="w-[112px]">{t("状态")}</TableHead>
>>>>>>> origin/main''',
'''                <TableHead className="w-[170px]">{t("今日使用")}</TableHead>
                <TableHead className="w-[132px]">{t("顺序")}</TableHead>
                <TableHead className="min-w-[180px]">{t("账号代理")}</TableHead>
                <TableHead className="w-[112px]">{t("状态")}</TableHead>''')
if '<<<<<<<' in text:
    raise SystemExit('accounts still has markers')
p.write_text(text, encoding='utf-8')
print('fixed accounts-page-view')

# delivery: prefer custom reasoning guard + adaptive keepalive
p = Path('crates/service/src/gateway/observability/http_bridge/delivery.rs')
text = p.read_text(encoding='utf-8')
text = text.replace('''<<<<<<< HEAD
    let reasoning_guard_scope =
        ReasoningGuardScope::new(reasoning_guard_source_id, fallback_model, request_path);
    let keepalive_frame = resolve_stream_keepalive_frame(response_adapter, request_path);
=======
    let keepalive_frame = SseKeepAliveFrame::Comment;
>>>>>>> origin/main''',
'''    let reasoning_guard_scope =
        ReasoningGuardScope::new(reasoning_guard_source_id, fallback_model, request_path);
    let keepalive_frame = resolve_stream_keepalive_frame(response_adapter, request_path);''')
if '<<<<<<<' in text:
    raise SystemExit('delivery still has markers')
# inject helper if missing
if 'fn resolve_stream_keepalive_frame' not in text:
    helper = '''
fn resolve_stream_keepalive_frame(
    response_adapter: ResponseAdapter,
    request_path: &str,
) -> SseKeepAliveFrame {
    match response_adapter {
        ResponseAdapter::Passthrough => {
            if request_path.starts_with("/v1/responses") {
                SseKeepAliveFrame::OpenAIResponses
            } else {
                SseKeepAliveFrame::Comment
            }
        }
        ResponseAdapter::AnthropicMessagesFromResponses
        | ResponseAdapter::ResponsesFromAnthropicMessages
        | ResponseAdapter::ChatCompletionsFromResponses
        | ResponseAdapter::CompactFromChatCompletions
        | ResponseAdapter::ImagesB64JsonFromResponses
        | ResponseAdapter::ImagesUrlFromResponses
        | ResponseAdapter::GeminiJson
        | ResponseAdapter::GeminiCliJson
        | ResponseAdapter::GeminiSse
        | ResponseAdapter::GeminiCliSse => SseKeepAliveFrame::Comment,
    }
}

'''
    needle = '\n#[cfg(test)]\n#[path = "delivery_tests.rs"]\nmod tests;\n'
    if needle not in text:
        raise SystemExit('delivery test needle missing')
    text = text.replace(needle, helper + needle, 1)
    print('injected resolve_stream_keepalive_frame')
p.write_text(text, encoding='utf-8')
print('fixed delivery.rs')

# mod.rs keepalive helpers keep HEAD body
p = Path('crates/service/src/gateway/observability/http_bridge/mod.rs')
text = p.read_text(encoding='utf-8')
old = '''<<<<<<< HEAD
    stream_readers::reload_from_env();
    reasoning_guard::clear_runtime_state();
}

/// 函数 `current_sse_keepalive_interval_ms`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn current_sse_keepalive_interval_ms() -> u64 {
    stream_readers::current_sse_keepalive_interval_ms()
}

/// 函数 `set_sse_keepalive_interval_ms`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn set_sse_keepalive_interval_ms(interval_ms: u64) -> Result<u64, String> {
    stream_readers::set_sse_keepalive_interval_ms(interval_ms)
=======
>>>>>>> origin/main
}
'''
new = '''    stream_readers::reload_from_env();
    reasoning_guard::clear_runtime_state();
}

/// 函数 `current_sse_keepalive_interval_ms`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn current_sse_keepalive_interval_ms() -> u64 {
    stream_readers::current_sse_keepalive_interval_ms()
}

/// 函数 `set_sse_keepalive_interval_ms`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn set_sse_keepalive_interval_ms(interval_ms: u64) -> Result<u64, String> {
    stream_readers::set_sse_keepalive_interval_ms(interval_ms)
}
'''
if old not in text:
    raise SystemExit('http_bridge/mod.rs pattern missing')
p.write_text(text.replace(old, new), encoding='utf-8')
print('fixed http_bridge/mod.rs')

# payload_rewrite: keep custom helpers + both tests
p = Path('crates/service/src/gateway/upstream/support/payload_rewrite.rs')
text = p.read_text(encoding='utf-8')
# Read full conflict and replace with union
start = text.index('<<<<<<< HEAD')
end = text.index('>>>>>>> origin/main') + len('>>>>>>> origin/main')
# Build union version from known structure
ours = Path('crates/service/src/gateway/upstream/support/payload_rewrite.rs')
# Use git versions
import subprocess
head = subprocess.check_output(['git','show','HEAD:crates/service/src/gateway/upstream/support/payload_rewrite.rs'], text=True)
main = subprocess.check_output(['git','show','origin/main:crates/service/src/gateway/upstream/support/payload_rewrite.rs'], text=True)
# head has helpers + continuation test; main has strip test only after strip_encrypted_content_from_body
# Take head file entirely then append main's strip test if not present
if 'strip_encrypted_content_removes_items_that_require_the_field' not in head:
    # Insert main's second test before closing of tests module
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
    # Ensure imports in head tests
    if 'use serde_json::{json, Value};' not in head and 'use super::*;' in head:
        head = head.replace(
            'mod tests {\n    use super::*;',
            'mod tests {\n    use super::*;\n    use serde_json::{json, Value};',
            1,
        )
    if 'fn contains_encrypted_content' not in head:
        helper = '''
    fn contains_encrypted_content(value: &Value) -> bool {
        match value {
            Value::Object(map) => {
                map.contains_key("encrypted_content")
                    || map.values().any(contains_encrypted_content)
            }
            Value::Array(items) => items.iter().any(contains_encrypted_content),
            _ => false,
        }
    }
'''
        head = head.replace('mod tests {\n    use super::*;', 'mod tests {\n    use super::*;' + helper, 1)
    # insert before final closing of tests
    idx = head.rfind('}\n')
    # better: before last "}\n" of file which closes tests then nothing
    if not head.rstrip().endswith('}'):
        raise SystemExit('unexpected head ending')
    # find last test end
    head = head.rstrip() + '\n' + main_test + '\n}\n'
p.write_text(head if 'strip_encrypted_content_removes_items' in head or True else head, encoding='utf-8')
# rewrite carefully: use processed head
# The assignment above is messy - rewrite file cleanly
final = head
if final.count('fn strip_encrypted_content_removes_items_that_require_the_field') == 0:
    raise SystemExit('failed to add main test')
# Fix double closing braces if any
# head originally ended with }\n for tests module. We appended test + }\n so OK if original ended with tests close.
# Actually head ends with:
#   }
# }
# for test fn and module. We did rstrip + main_test + }\n which might add extra.
# Let's rebuild from HEAD content programmatically more carefully.
head = subprocess.check_output(['git','show','HEAD:crates/service/src/gateway/upstream/support/payload_rewrite.rs'], text=True)
# Remove final closing of tests module and file end, inject helpers/tests
if '#[cfg(test)]' not in head:
    raise SystemExit('no tests in head')
# Ensure contains_encrypted_content + json import + main test before last module close
parts = head.rsplit('}', 1)  # wrong
# find "mod tests {" section
test_idx = head.index('#[cfg(test)]')
prefix = head[:test_idx]
tests = head[test_idx:]
# Normalize tests module content
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
    # insert before final closing brace of module
    # tests ends with "}\n"
    tests = tests.rstrip()
    if not tests.endswith('}'):
        raise SystemExit('tests module end missing')
    # remove last closing brace of module
    tests_body = tests[:-1].rstrip()
    tests = tests_body + '\n' + main_test + '\n}\n'
final = prefix + tests
p.write_text(final, encoding='utf-8')
print('fixed payload_rewrite.rs')
print('markers left:', sum(1 for line in Path('.').rglob('*') if False))
for f in [
 'apps/src/app/accounts/accounts-page-view.tsx',
 'crates/service/src/gateway/observability/http_bridge/delivery.rs',
 'crates/service/src/gateway/observability/http_bridge/mod.rs',
 'crates/service/src/gateway/upstream/support/payload_rewrite.rs',
]:
    t = Path(f).read_text(encoding='utf-8')
    print(f, 'markers', t.count('<<<<<<<'))
