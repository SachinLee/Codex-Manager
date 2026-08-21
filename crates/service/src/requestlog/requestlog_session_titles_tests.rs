use super::{
    expire_omp_session_title_cache_for_tests, list_omp_session_titles_cached,
    list_omp_session_titles_from_root, list_pi_session_titles_from_root,
    merge_request_log_session_titles, ExternalSessionTitleCandidate, RequestLogSessionSource,
    RequestLogSessionTitle, MAX_PI_SESSION_ENTRY_BYTES,
};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(name: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir().join(format!("codexmanager-{name}-{nonce}"))
}

fn write_omp_session(root: &Path, id: &str, title: &str, transcript: &str) {
    fs::create_dir_all(root).expect("create fixture directory");
    let title_slot = serde_json::json!({
        "type": "title",
        "v": 1,
        "title": title,
        "source": "auto",
        "updatedAt": "2026-07-30T00:00:00.000Z",
        "pad": ""
    });
    let header = serde_json::json!({
        "type": "session",
        "version": 3,
        "id": id,
        "timestamp": "2026-07-30T00:00:00.000Z",
        "cwd": "D:/work/example",
        "title": title,
        "titleSource": "auto"
    });
    fs::write(
        root.join(format!("2026-07-30T00-00-00-000Z_{id}.jsonl")),
        format!("{title_slot}\n{header}\n{transcript}\n"),
    )
    .expect("write OMP fixture");
}

fn write_pi_session(root: &Path, id: &str, prompt: &str, name: Option<&str>) {
    fs::create_dir_all(root).expect("create fixture directory");
    let header = serde_json::json!({
        "type": "session",
        "version": 3,
        "id": id,
        "timestamp": "2026-07-30T00:00:00.000Z",
        "cwd": "D:/work/pi-example"
    });
    let user_message = serde_json::json!({
        "type": "message",
        "id": "11111111",
        "parentId": null,
        "timestamp": "2026-07-30T00:00:01.000Z",
        "message": { "role": "user", "content": prompt }
    });
    let mut entries = vec![header.to_string(), user_message.to_string()];
    if let Some(name) = name {
        entries.push(
            serde_json::json!({
                "type": "session_info",
                "id": "22222222",
                "parentId": "11111111",
                "timestamp": "2026-07-30T00:00:02.000Z",
                "name": name
            })
            .to_string(),
        );
    }
    fs::write(
        root.join(format!("2026-07-30T00-00-00-000Z_{id}.jsonl")),
        format!("{}\n", entries.join("\n")),
    )
    .expect("write Pi fixture");
}
#[test]
fn omp_title_index_reads_only_session_metadata() {
    let root = unique_temp_dir("omp-session-title");
    write_omp_session(
        &root,
        "019fb0d2-4d04-7000-90dd-9c6255e994e4",
        "修复登录超时",
        r#"{\"type\":\"message\",\"message\":{\"content\":\"secret transcript must not become title\"}}"#,
    );

    let sessions = list_omp_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0].session_id,
        "019fb0d2-4d04-7000-90dd-9c6255e994e4"
    );
    assert_eq!(sessions[0].title.as_deref(), Some("修复登录超时"));
    assert_eq!(sessions[0].cwd.as_deref(), Some("D:/work/example"));
    assert_eq!(sessions[0].source, RequestLogSessionSource::Omp);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn omp_title_index_reads_session_metadata_from_project_directory() {
    let root = unique_temp_dir("omp-project-session-title");
    let id = "019fca51-ab55-7000-beca-006a4140fdfa";
    write_omp_session(
        &root.join("abs-Codex-Manager"),
        id,
        "分析 VSCode Java 编译错误及插件冲突",
        "ignored transcript",
    );

    let sessions = list_omp_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, id);
    assert_eq!(
        sessions[0].title.as_deref(),
        Some("分析 VSCode Java 编译错误及插件冲突")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn omp_title_index_never_uses_transcript_as_a_title_fallback() {
    let root = unique_temp_dir("omp-session-without-title");
    write_omp_session(
        &root,
        "019fb0d2-4d04-7000-90dd-9c6255e994e5",
        "",
        r#"{\"type\":\"message\",\"message\":{\"content\":\"secret transcript must not become title\"}}"#,
    );

    let sessions = list_omp_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].title, None);
    assert_eq!(sessions[0].cwd.as_deref(), Some("D:/work/example"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn omp_title_index_does_not_read_past_the_session_header() {
    let root = unique_temp_dir("omp-session-metadata-boundary");
    let id = "019fb0d2-4d04-7000-90dd-9c6255e994e6";
    write_omp_session(&root, id, "元数据标题", "ignored");
    let path = root.join(format!("2026-07-30T00-00-00-000Z_{id}.jsonl"));
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open fixture");
    file.write_all(&[0xff])
        .expect("append invalid transcript byte");

    let sessions = list_omp_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].title.as_deref(), Some("元数据标题"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn omp_title_index_rejects_non_uuid_session_ids() {
    let root = unique_temp_dir("omp-invalid-session-id");
    write_omp_session(
        &root,
        "019fb0d2-4d04-7000-90dd-9c6255e994e4,target-session-id",
        "不应匹配",
        "ignored",
    );

    assert!(list_omp_session_titles_from_root(&root, 20).is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn omp_title_index_skips_invalid_metadata_and_missing_roots() {
    let root = unique_temp_dir("omp-invalid-session-title");
    fs::create_dir_all(&root).expect("create fixture root");
    fs::write(
        root.join("broken.jsonl"),
        "not json\n{\"type\":\"message\"}\n",
    )
    .expect("write invalid fixture");

    assert!(list_omp_session_titles_from_root(&root, 20).is_empty());
    assert!(list_omp_session_titles_from_root(&root.join("missing"), 20).is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pi_title_index_uses_first_user_prompt_when_unnamed() {
    let root = unique_temp_dir("pi-session-title-prompt");
    let id = "019fb0d2-4d04-7000-90dd-9c6255e994e7";
    write_pi_session(&root, id, "  Fix   Pi\nsession title  ", None);

    let sessions = list_pi_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, id);
    assert_eq!(sessions[0].title.as_deref(), Some("Fix Pi session title"));
    assert_eq!(sessions[0].cwd.as_deref(), Some("D:/work/pi-example"));
    assert_eq!(sessions[0].source, RequestLogSessionSource::Pi);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pi_title_index_prefers_explicit_name() {
    let root = unique_temp_dir("pi-session-title-name");
    let id = "019fb0d2-4d04-7000-90dd-9c6255e994e8";
    write_pi_session(
        &root,
        id,
        "Fallback request title",
        Some("Named Pi session"),
    );

    let sessions = list_pi_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].title.as_deref(), Some("Named Pi session"));
    assert_eq!(sessions[0].source, RequestLogSessionSource::Pi);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pi_title_index_skips_oversized_entries_without_losing_later_names() {
    let root = unique_temp_dir("pi-session-title-oversized-entry");
    let id = "019fb0d2-4d04-7000-90dd-9c6255e994e9";
    write_pi_session(&root, id, "Fallback request title", None);
    let path = root.join(format!("2026-07-30T00-00-00-000Z_{id}.jsonl"));
    let oversized_entry = format!(
        "{{\"type\":\"message\",\"payload\":\"{}\"}}",
        "x".repeat(MAX_PI_SESSION_ENTRY_BYTES)
    );
    let session_info = serde_json::json!({
        "type": "session_info",
        "id": "33333333",
        "parentId": "11111111",
        "timestamp": "2026-07-30T00:00:03.000Z",
        "name": "Name after oversized entry"
    });
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open fixture");
    writeln!(file, "{oversized_entry}").expect("append oversized entry");
    writeln!(file, "{session_info}").expect("append session name");

    let sessions = list_pi_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0].title.as_deref(),
        Some("Name after oversized entry")
    );

    let _ = fs::remove_dir_all(root);
}
#[test]
fn omp_title_cache_refreshes_changed_metadata_after_expiry() {
    let root = unique_temp_dir("omp-session-title-cache");
    let id = "019fb0d2-4d04-7000-90dd-9c6255e994e4";
    write_omp_session(&root, id, "初始标题", "ignored");

    let first = list_omp_session_titles_cached(&root, 20);
    assert_eq!(first[0].title.title.as_deref(), Some("初始标题"));

    write_omp_session(&root, id, "更新后的标题", "ignored");
    expire_omp_session_title_cache_for_tests();

    let refreshed = list_omp_session_titles_cached(&root, 20);
    assert_eq!(refreshed[0].title.title.as_deref(), Some("更新后的标题"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn omp_title_cache_refreshes_and_prunes_project_directory_metadata() {
    let root = unique_temp_dir("omp-project-session-title-cache");
    let project = root.join("abs-Codex-Manager");
    let id = "019fca51-ab55-7000-beca-006a4140fdfa";
    let file = project.join(format!("2026-07-30T00-00-00-000Z_{id}.jsonl"));
    write_omp_session(&project, id, "初始标题", "ignored");

    let first = list_omp_session_titles_cached(&root, 20);
    assert_eq!(first[0].title.title.as_deref(), Some("初始标题"));

    write_omp_session(&project, id, "更新后的标题", "ignored");
    expire_omp_session_title_cache_for_tests();
    let refreshed = list_omp_session_titles_cached(&root, 20);
    assert_eq!(refreshed[0].title.title.as_deref(), Some("更新后的标题"));

    fs::remove_file(file).expect("remove fixture session");
    expire_omp_session_title_cache_for_tests();
    assert!(list_omp_session_titles_cached(&root, 20).is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn omp_title_index_does_not_scan_grandchild_directories() {
    let root = unique_temp_dir("omp-project-session-depth");
    let project = root.join("abs-Codex-Manager");
    write_omp_session(
        &project,
        "019fca51-ab55-7000-beca-006a4140fdfa",
        "项目会话",
        "ignored",
    );
    write_omp_session(
        &project.join("nested"),
        "019fca5e-1275-7000-90ec-b9a1300e064d",
        "不应扫描",
        "ignored",
    );

    let sessions = list_omp_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].title.as_deref(), Some("项目会话"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn omp_title_cache_discovers_project_directory_added_after_initial_scan() {
    let root = unique_temp_dir("omp-project-session-title-added-after-cache");
    let root_id = "019fb0d2-4d04-7000-90dd-9c6255e994e4";
    let project_id = "019fca51-ab55-7000-beca-006a4140fdfa";
    write_omp_session(&root, root_id, "根目录会话", "ignored");

    let first = list_omp_session_titles_cached(&root, 20);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].title.session_id, root_id);

    write_omp_session(
        &root.join("abs-Codex-Manager"),
        project_id,
        "新增项目会话",
        "ignored",
    );
    expire_omp_session_title_cache_for_tests();

    let refreshed = list_omp_session_titles_cached(&root, 20);
    assert_eq!(refreshed.len(), 2);
    assert!(refreshed
        .iter()
        .any(|entry| entry.title.session_id == project_id));

    let _ = fs::remove_dir_all(root);
}
#[test]
fn session_title_merge_prefers_codex_on_id_collision_and_enforces_limit() {
    let shared_id = "019fb0d2-4d04-7000-90dd-9c6255e994e4".to_string();
    let codex = RequestLogSessionTitle {
        session_id: shared_id.clone(),
        title: Some("Codex 标题".to_string()),
        cwd: None,
        source: RequestLogSessionSource::Codex,
    };
    let omp_collision = ExternalSessionTitleCandidate {
        title: RequestLogSessionTitle {
            session_id: shared_id.clone(),
            title: Some("OMP 标题".to_string()),
            cwd: None,
            source: RequestLogSessionSource::Omp,
        },
        updated_at: 99,
    };
    let omp_newer = ExternalSessionTitleCandidate {
        title: RequestLogSessionTitle {
            session_id: "019fb0d2-4d04-7000-90dd-9c6255e994e5".to_string(),
            title: Some("最新 OMP 标题".to_string()),
            cwd: None,
            source: RequestLogSessionSource::Omp,
        },
        updated_at: 20,
    };

    let merged =
        merge_request_log_session_titles(vec![(codex, 10)], vec![omp_collision, omp_newer], 2);

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].source, RequestLogSessionSource::Omp);
    assert_eq!(merged[1].session_id, shared_id);
    assert_eq!(merged[1].title.as_deref(), Some("Codex 标题"));
    assert_eq!(merged[1].source, RequestLogSessionSource::Codex);
}
