use std::collections::{BTreeMap, HashMap};
use std::io::Read as _;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use base64::engine::general_purpose::{STANDARD as BASE64_STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use chrono::{SecondsFormat, Utc};
use codexmanager_core::storage::{AccountAgentIdentity, Storage};
use crypto_box::SecretKey as Curve25519SecretKey;
use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::{Signer as _, SigningKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha512};

const AGENT_ASSERTION_SCHEME: &str = "AgentAssertion";
const AGENT_IDENTITY_AUTHAPI_BASE_URL: &str = "https://auth.openai.com/api/accounts";
const AGENT_TASK_REGISTRATION_TIMEOUT: Duration = Duration::from_secs(30);
const AGENT_TASK_REGISTRATION_RESPONSE_LIMIT: u64 = 64 * 1024;

static ACCOUNT_AGENT_TASK_LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedAgentIdentityAuthorization {
    pub(crate) value: String,
    pub(crate) task_id: String,
    pub(crate) is_fedramp: bool,
}

#[derive(Serialize)]
struct RegisterAgentTaskRequest {
    timestamp: String,
    signature: String,
}

#[derive(Deserialize)]
struct RegisterAgentTaskResponse {
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default, rename = "taskId")]
    task_id_camel: Option<String>,
    #[serde(default)]
    encrypted_task_id: Option<String>,
    #[serde(default, rename = "encryptedTaskId")]
    encrypted_task_id_camel: Option<String>,
}

pub(crate) fn authorization_header_for_agent_identity(
    identity: &AccountAgentIdentity,
) -> Result<String, String> {
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    authorization_header_for_agent_identity_at(identity, &timestamp)
}

pub(crate) fn validate_agent_identity(identity: &AccountAgentIdentity) -> Result<(), String> {
    required_value(&identity.agent_runtime_id, "agent_runtime_id")?;
    signing_key_from_pkcs8_base64(&identity.agent_private_key)?;
    Ok(())
}

pub(crate) fn resolve_account_agent_identity_authorization(
    storage: &Storage,
    client: &reqwest::blocking::Client,
    account_id: &str,
) -> Result<Option<ResolvedAgentIdentityAuthorization>, String> {
    resolve_account_agent_identity_authorization_with(storage, account_id, None, |identity| {
        register_agent_identity_task(client, identity, AGENT_IDENTITY_AUTHAPI_BASE_URL)
    })
}

pub(crate) fn recover_account_agent_identity_authorization(
    storage: &Storage,
    client: &reqwest::blocking::Client,
    account_id: &str,
    failed_task_id: &str,
) -> Result<Option<ResolvedAgentIdentityAuthorization>, String> {
    let failed_task_id = required_value(failed_task_id, "failed task_id")?;
    resolve_account_agent_identity_authorization_with(
        storage,
        account_id,
        Some(failed_task_id),
        |identity| register_agent_identity_task(client, identity, AGENT_IDENTITY_AUTHAPI_BASE_URL),
    )
}

fn resolve_account_agent_identity_authorization_with<F>(
    storage: &Storage,
    account_id: &str,
    failed_task_id: Option<&str>,
    register_task: F,
) -> Result<Option<ResolvedAgentIdentityAuthorization>, String>
where
    F: FnOnce(&AccountAgentIdentity) -> Result<String, String>,
{
    let account_id = required_value(account_id, "account_id")?;
    let identity = load_account_agent_identity(storage, account_id)?;
    let Some(identity) = identity else {
        return Ok(None);
    };
    if task_can_be_reused(&identity, failed_task_id) {
        return resolved_authorization(&identity).map(Some);
    }

    let task_lock = account_agent_task_lock(account_id);
    let _guard = crate::lock_utils::lock_recover(&task_lock, "account_agent_task_lock");

    // Re-read under the per-account lock. Request paths use separate SQLite
    // handles, so the caller's snapshot cannot prove that registration is
    // still needed after waiting for another request.
    let mut identity = load_account_agent_identity(storage, account_id)?
        .ok_or_else(|| "agent identity disappeared during task registration".to_string())?;
    if task_can_be_reused(&identity, failed_task_id) {
        return resolved_authorization(&identity).map(Some);
    }

    let task_id = register_task(&identity)?;
    let task_id = required_value(&task_id, "registered task_id")?.to_string();
    let updated = storage
        .update_account_agent_identity_task_id(
            account_id,
            &identity.agent_runtime_id,
            &identity.agent_private_key,
            Some(&task_id),
        )
        .map_err(|err| format!("persist agent identity task failed: {err}"))?;
    if !updated {
        return Err("agent identity disappeared before task persistence".to_string());
    }
    identity.task_id = Some(task_id);
    resolved_authorization(&identity).map(Some)
}

fn account_agent_task_lock(account_id: &str) -> Arc<Mutex<()>> {
    let locks = ACCOUNT_AGENT_TASK_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    crate::lock_utils::lock_recover(locks, "account_agent_task_locks")
        .entry(account_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn load_account_agent_identity(
    storage: &Storage,
    account_id: &str,
) -> Result<Option<AccountAgentIdentity>, String> {
    storage
        .find_account_agent_identity(account_id)
        .map_err(|err| format!("load agent identity failed: {err}"))
}

fn task_can_be_reused(identity: &AccountAgentIdentity, failed_task_id: Option<&str>) -> bool {
    let Some(task_id) = identity
        .task_id
        .as_deref()
        .map(str::trim)
        .filter(|task_id| !task_id.is_empty())
    else {
        return false;
    };
    failed_task_id.is_none_or(|failed_task_id| task_id != failed_task_id)
}

fn resolved_authorization(
    identity: &AccountAgentIdentity,
) -> Result<ResolvedAgentIdentityAuthorization, String> {
    let task_id = identity
        .task_id
        .as_deref()
        .map(str::trim)
        .filter(|task_id| !task_id.is_empty())
        .ok_or_else(|| "agent identity task_id is empty".to_string())?;
    Ok(ResolvedAgentIdentityAuthorization {
        value: authorization_header_for_agent_identity(identity)?,
        task_id: task_id.to_string(),
        is_fedramp: identity.chatgpt_account_is_fedramp,
    })
}

fn register_agent_identity_task(
    client: &reqwest::blocking::Client,
    identity: &AccountAgentIdentity,
    authapi_base_url: &str,
) -> Result<String, String> {
    validate_agent_identity(identity)?;
    let runtime_id = required_value(&identity.agent_runtime_id, "agent_runtime_id")?;
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let signing_key = signing_key_from_pkcs8_base64(&identity.agent_private_key)?;
    let payload = format!("{runtime_id}:{timestamp}");
    let request = RegisterAgentTaskRequest {
        timestamp,
        signature: BASE64_STANDARD.encode(signing_key.sign(payload.as_bytes()).to_bytes()),
    };
    let url = format!(
        "{}/v1/agent/{}/task/register",
        authapi_base_url.trim_end_matches('/'),
        runtime_id
    );
    let response = client
        .post(&url)
        .timeout(AGENT_TASK_REGISTRATION_TIMEOUT)
        .json(&request)
        .send()
        .map_err(|err| format!("agent task registration request failed: {err}"))?;
    let status = response.status();
    let mut body = Vec::new();
    response
        .take(AGENT_TASK_REGISTRATION_RESPONSE_LIMIT + 1)
        .read_to_end(&mut body)
        .map_err(|err| format!("read agent task registration response failed: {err}"))?;
    if body.len() as u64 > AGENT_TASK_REGISTRATION_RESPONSE_LIMIT {
        return Err("agent task registration response exceeded 64 KiB".to_string());
    }
    if !status.is_success() {
        return Err(format!(
            "agent task registration returned status {}",
            status.as_u16()
        ));
    }
    let response: RegisterAgentTaskResponse = serde_json::from_slice(&body)
        .map_err(|err| format!("agent task registration response is invalid: {err}"))?;
    task_id_from_registration_response(identity, response)
}

fn task_id_from_registration_response(
    identity: &AccountAgentIdentity,
    response: RegisterAgentTaskResponse,
) -> Result<String, String> {
    if let Some(task_id) = response
        .task_id
        .or(response.task_id_camel)
        .and_then(non_empty_owned)
    {
        return Ok(task_id);
    }
    let encrypted_task_id = response
        .encrypted_task_id
        .or(response.encrypted_task_id_camel)
        .and_then(non_empty_owned)
        .ok_or_else(|| "agent task registration response omitted task id".to_string())?;
    decrypt_agent_task_id(identity, &encrypted_task_id)
}

fn decrypt_agent_task_id(
    identity: &AccountAgentIdentity,
    encrypted_task_id: &str,
) -> Result<String, String> {
    let signing_key = signing_key_from_pkcs8_base64(&identity.agent_private_key)?;
    let ciphertext = BASE64_STANDARD
        .decode(encrypted_task_id.trim())
        .map_err(|err| format!("encrypted agent task id is not valid base64: {err}"))?;
    let plaintext = curve25519_secret_key_from_signing_key(&signing_key)
        .unseal(&ciphertext)
        .map_err(|_| "failed to decrypt encrypted agent task id".to_string())?;
    let task_id = String::from_utf8(plaintext)
        .map_err(|err| format!("decrypted agent task id is not valid UTF-8: {err}"))?;
    non_empty_owned(task_id).ok_or_else(|| "decrypted agent task id is empty".to_string())
}

fn curve25519_secret_key_from_signing_key(signing_key: &SigningKey) -> Curve25519SecretKey {
    let digest = Sha512::digest(signing_key.to_bytes());
    let mut secret_key = [0_u8; 32];
    secret_key.copy_from_slice(&digest[..32]);
    secret_key[0] &= 248;
    secret_key[31] &= 127;
    secret_key[31] |= 64;
    Curve25519SecretKey::from(secret_key)
}

fn non_empty_owned(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub(crate) fn is_agent_identity_task_invalid_response(status: u16, body: &[u8]) -> bool {
    if status != 401 {
        return false;
    }
    let lower = String::from_utf8_lossy(body).to_ascii_lowercase();
    let compact: String = lower
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect();
    [
        r#""code":"invalid_task_id""#,
        r#""code":"task_not_found""#,
        r#""code":"task_expired""#,
        r#""error":"invalid_task_id""#,
    ]
    .iter()
    .any(|marker| compact.contains(marker))
        || [
            "invalid_task_id",
            "task_not_found",
            "task_expired",
            "invalid task_id",
            "invalid task id",
            "task_id is invalid",
            "task id is invalid",
            "task not found",
            "task expired",
            "unknown task_id",
            "unknown task id",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
}

pub(crate) fn is_agent_identity_task_invalid_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    let unauthorized = lower.contains("status=401")
        || lower.contains("status 401")
        || lower.contains("401 unauthorized");
    unauthorized && is_agent_identity_task_invalid_response(401, message.as_bytes())
}

fn authorization_header_for_agent_identity_at(
    identity: &AccountAgentIdentity,
    timestamp: &str,
) -> Result<String, String> {
    let runtime_id = required_value(&identity.agent_runtime_id, "agent_runtime_id")?;
    let task_id = identity
        .task_id
        .as_deref()
        .ok_or_else(|| "agent identity task_id is empty".to_string())?;
    let task_id = required_value(task_id, "task_id")?;
    let signing_key = signing_key_from_pkcs8_base64(&identity.agent_private_key)?;
    let signed_payload = format!("{runtime_id}:{task_id}:{timestamp}");
    let signature = BASE64_STANDARD.encode(signing_key.sign(signed_payload.as_bytes()).to_bytes());
    let envelope = BTreeMap::from([
        ("agent_runtime_id", runtime_id),
        ("signature", signature.as_str()),
        ("task_id", task_id),
        ("timestamp", timestamp),
    ]);
    let serialized = serde_json::to_vec(&envelope)
        .map_err(|err| format!("failed to serialize agent assertion: {err}"))?;
    Ok(format!(
        "{AGENT_ASSERTION_SCHEME} {}",
        URL_SAFE_NO_PAD.encode(serialized)
    ))
}

pub(crate) fn format_upstream_authorization(auth_token: &str) -> String {
    let trimmed = auth_token.trim();
    if trimmed.starts_with(&format!("{AGENT_ASSERTION_SCHEME} ")) {
        trimmed.to_string()
    } else {
        format!("Bearer {trimmed}")
    }
}

fn required_value<'a>(value: &'a str, field: &str) -> Result<&'a str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("agent identity {field} is empty"))
    } else {
        Ok(trimmed)
    }
}

fn signing_key_from_pkcs8_base64(private_key: &str) -> Result<SigningKey, String> {
    let private_key = BASE64_STANDARD
        .decode(private_key.trim())
        .map_err(|err| format!("agent identity private key is not valid base64: {err}"))?;
    SigningKey::from_pkcs8_der(&private_key)
        .map_err(|err| format!("agent identity private key is not valid PKCS#8: {err}"))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Barrier};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use base64::engine::general_purpose::{STANDARD as BASE64_STANDARD, URL_SAFE_NO_PAD};
    use base64::Engine as _;
    use codexmanager_core::storage::{now_ts, Account};
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    use ed25519_dalek::{Signature, SigningKey, Verifier as _};
    use tiny_http::{Response, Server, StatusCode};

    use super::*;

    fn identity() -> (AccountAgentIdentity, SigningKey) {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let private_key = signing_key.to_pkcs8_der().expect("encode private key");
        let now = now_ts();
        (
            AccountAgentIdentity {
                account_id: "account-1".to_string(),
                agent_runtime_id: "agent-runtime-1".to_string(),
                agent_private_key: BASE64_STANDARD.encode(private_key.as_bytes()),
                task_id: Some("task-1".to_string()),
                chatgpt_user_id: "user-1".to_string(),
                chatgpt_account_is_fedramp: false,
                auth_mode: "agentIdentity".to_string(),
                workspace_id: Some("workspace-1".to_string()),
                created_at: now,
                updated_at: now,
            },
            signing_key,
        )
    }

    fn insert_identity(storage: &Storage, identity: &AccountAgentIdentity) {
        let now = now_ts();
        storage
            .insert_account(&Account {
                id: identity.account_id.clone(),
                label: identity.account_id.clone(),
                issuer: "https://auth.openai.com".to_string(),
                chatgpt_account_id: Some("workspace-1".to_string()),
                workspace_id: Some("workspace-1".to_string()),
                group_name: None,
                sort: 0,
                status: "active".to_string(),
                created_at: now,
                updated_at: now,
            })
            .expect("insert account");
        storage
            .upsert_account_agent_identity(identity)
            .expect("insert identity");
    }

    #[test]
    fn agent_assertion_matches_codex_agent_identity_wire_format() {
        let (identity, signing_key) = identity();
        let timestamp = "2026-07-21T12:00:00Z";
        let header = authorization_header_for_agent_identity_at(&identity, timestamp)
            .expect("build agent assertion");
        let encoded = header
            .strip_prefix("AgentAssertion ")
            .expect("agent assertion scheme");
        let envelope: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded).expect("decode assertion"))
                .expect("parse assertion");

        assert_eq!(envelope["agent_runtime_id"], "agent-runtime-1");
        assert_eq!(envelope["task_id"], "task-1");
        assert_eq!(envelope["timestamp"], timestamp);
        let signature_bytes = BASE64_STANDARD
            .decode(envelope["signature"].as_str().expect("signature"))
            .expect("decode signature");
        let signature = Signature::from_slice(&signature_bytes).expect("parse signature");
        signing_key
            .verifying_key()
            .verify(
                format!("agent-runtime-1:task-1:{timestamp}").as_bytes(),
                &signature,
            )
            .expect("verify signature");
    }

    #[test]
    fn upstream_authorization_preserves_agent_assertion_and_wraps_bearer() {
        assert_eq!(
            format_upstream_authorization("AgentAssertion encoded"),
            "AgentAssertion encoded"
        );
        assert_eq!(
            format_upstream_authorization("access-token"),
            "Bearer access-token"
        );
    }

    #[test]
    fn missing_task_is_registered_and_persisted_before_assertion() {
        let storage = Storage::open_in_memory().expect("open storage");
        storage.init().expect("init storage");
        let (mut identity, _) = identity();
        identity.account_id = "account-missing-task".to_string();
        identity.task_id = None;
        insert_identity(&storage, &identity);

        let authorization = resolve_account_agent_identity_authorization_with(
            &storage,
            &identity.account_id,
            None,
            |_| Ok("task-registered".to_string()),
        )
        .expect("resolve identity")
        .expect("identity authorization");

        assert_eq!(authorization.task_id, "task-registered");
        assert!(authorization.value.starts_with("AgentAssertion "));
        assert_eq!(
            storage
                .find_account_agent_identity(&identity.account_id)
                .expect("load identity")
                .expect("identity")
                .task_id
                .as_deref(),
            Some("task-registered")
        );
    }

    #[test]
    fn expired_task_is_replaced_once_and_stale_recovery_reuses_replacement() {
        let storage = Storage::open_in_memory().expect("open storage");
        storage.init().expect("init storage");
        let (mut identity, _) = identity();
        identity.account_id = "account-expired-task".to_string();
        insert_identity(&storage, &identity);
        let calls = AtomicUsize::new(0);

        let recovered = resolve_account_agent_identity_authorization_with(
            &storage,
            &identity.account_id,
            Some("task-1"),
            |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok("task-2".to_string())
            },
        )
        .expect("recover task")
        .expect("identity authorization");
        assert_eq!(recovered.task_id, "task-2");

        let reused = resolve_account_agent_identity_authorization_with(
            &storage,
            &identity.account_id,
            Some("task-1"),
            |_| panic!("stale recovery must reuse the persisted replacement"),
        )
        .expect("reuse replacement")
        .expect("identity authorization");
        assert_eq!(reused.task_id, "task-2");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn concurrent_missing_task_registration_runs_only_once() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!(
            "codexmanager-agent-task-{}-{suffix}.sqlite",
            std::process::id()
        ));
        let account_id = format!("account-concurrent-task-{suffix}");
        {
            let storage = Storage::open(&db_path).expect("open storage");
            storage.init().expect("init storage");
            let (mut identity, _) = identity();
            identity.account_id = account_id.clone();
            identity.task_id = None;
            insert_identity(&storage, &identity);
        }

        let worker_count = 6;
        let barrier = Arc::new(Barrier::new(worker_count));
        let registrations = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::new();
        for _ in 0..worker_count {
            let db_path = db_path.clone();
            let account_id = account_id.clone();
            let barrier = Arc::clone(&barrier);
            let registrations = Arc::clone(&registrations);
            workers.push(thread::spawn(move || {
                let storage = Storage::open(db_path).expect("open worker storage");
                barrier.wait();
                resolve_account_agent_identity_authorization_with(
                    &storage,
                    &account_id,
                    None,
                    |_| {
                        registrations.fetch_add(1, Ordering::SeqCst);
                        thread::sleep(Duration::from_millis(40));
                        Ok("task-concurrent".to_string())
                    },
                )
                .expect("resolve concurrent identity")
                .expect("identity authorization")
                .task_id
            }));
        }
        for worker in workers {
            assert_eq!(worker.join().expect("join worker"), "task-concurrent");
        }
        assert_eq!(registrations.load(Ordering::SeqCst), 1);

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
    }

    #[test]
    fn task_registration_uses_expected_path_and_signed_payload() {
        let server = Server::http("127.0.0.1:0").expect("start task registration server");
        let base_url = format!("http://{}", server.server_addr());
        let (request_tx, request_rx) = mpsc::channel();
        let server_handle = thread::spawn(move || {
            let mut request = server
                .recv_timeout(Duration::from_secs(5))
                .expect("registration server timeout")
                .expect("registration request");
            let path = request.url().to_string();
            let mut body = String::new();
            request
                .as_reader()
                .read_to_string(&mut body)
                .expect("read registration request");
            request_tx.send((path, body)).expect("record request");
            request
                .respond(
                    Response::from_string(r#"{"taskId":"task-from-server"}"#)
                        .with_status_code(StatusCode(200))
                        .with_header(
                            tiny_http::Header::from_bytes("Content-Type", "application/json")
                                .expect("content type"),
                        ),
                )
                .expect("respond registration request");
        });
        let (identity, signing_key) = identity();
        let task_id =
            register_agent_identity_task(&reqwest::blocking::Client::new(), &identity, &base_url)
                .expect("register task");
        assert_eq!(task_id, "task-from-server");
        let (path, body) = request_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("receive request");
        server_handle.join().expect("join server");
        assert_eq!(path, "/v1/agent/agent-runtime-1/task/register");
        let body: serde_json::Value = serde_json::from_str(&body).expect("parse request body");
        let timestamp = body["timestamp"].as_str().expect("timestamp");
        let signature = BASE64_STANDARD
            .decode(body["signature"].as_str().expect("signature"))
            .expect("decode signature");
        signing_key
            .verifying_key()
            .verify(
                format!("agent-runtime-1:{timestamp}").as_bytes(),
                &Signature::from_slice(&signature).expect("parse signature"),
            )
            .expect("verify registration signature");
    }

    #[test]
    fn task_registration_http_error_does_not_echo_response_body() {
        let server = Server::http("127.0.0.1:0").expect("start task registration server");
        let base_url = format!("http://{}", server.server_addr());
        let server_handle = thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(5))
                .expect("registration server timeout")
                .expect("registration request");
            request
                .respond(
                    Response::from_string(r#"{"signature":"must-not-leak"}"#)
                        .with_status_code(StatusCode(401)),
                )
                .expect("respond registration request");
        });
        let (identity, _) = identity();
        let error =
            register_agent_identity_task(&reqwest::blocking::Client::new(), &identity, &base_url)
                .expect_err("registration should fail");
        server_handle.join().expect("join server");

        assert_eq!(error, "agent task registration returned status 401");
        assert!(!error.contains("must-not-leak"));
    }

    #[test]
    fn encrypted_task_registration_response_is_decrypted() {
        let (identity, signing_key) = identity();
        let secret = curve25519_secret_key_from_signing_key(&signing_key);
        let encrypted = secret
            .public_key()
            .seal(&mut rand::rngs::OsRng, b"task-encrypted")
            .expect("seal task id");
        let task_id = task_id_from_registration_response(
            &identity,
            RegisterAgentTaskResponse {
                task_id: None,
                task_id_camel: None,
                encrypted_task_id: Some(BASE64_STANDARD.encode(encrypted)),
                encrypted_task_id_camel: None,
            },
        )
        .expect("decrypt task id");
        assert_eq!(task_id, "task-encrypted");
    }

    #[test]
    fn invalid_task_detection_requires_unauthorized_task_failure() {
        assert!(is_agent_identity_task_invalid_response(
            401,
            br#"{"error":{"code":"task_expired"}}"#,
        ));
        assert!(is_agent_identity_task_invalid_error(
            "usage endpoint failed: status=401 body=task not found"
        ));
        assert!(!is_agent_identity_task_invalid_response(
            403,
            br#"{"code":"invalid_task_id"}"#,
        ));
        assert!(!is_agent_identity_task_invalid_error(
            "usage endpoint failed: status=500 body=task expired"
        ));
    }
}
