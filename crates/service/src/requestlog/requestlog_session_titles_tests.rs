use super::{
    expire_omp_session_title_cache_for_tests, list_omp_session_titles_cached,
    list_omp_session_titles_from_root, list_pi_session_titles_from_root,
    merge_request_log_session_titles, ExternalSessionTitleCandidate, RequestLogSessionSource,
    RequestLogSessionTitle, SessionTitleSnapshotCache, MAX_PI_SESSION_ENTRY_BYTES,
    MAX_SESSION_TITLE_LIMIT,
};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Barrier,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
fn omp_subagent_session_uses_parent_title_and_child_id() {
    let root = unique_temp_dir("omp-subagent-parent-title");
    let parent_id = "019fb0d2-4d04-7000-90dd-9c6255e994e4";
    let child_id = "019fca51-ab55-7000-beca-006a4140fdfa";
    write_omp_session(&root, parent_id, "主线程标题", "ignored");
    let parent_stem = root.join(format!("2026-07-30T00-00-00-000Z_{parent_id}"));
    write_omp_session(&parent_stem, child_id, "", "secret child transcript");

    let sessions = list_omp_session_titles_from_root(&root, 20);
    let child = sessions
        .iter()
        .find(|session| session.session_id == child_id)
        .expect("child session");

    assert_eq!(child.title, None);
    assert_eq!(child.parent_session_id.as_deref(), Some(parent_id));
    assert_eq!(child.parent_title.as_deref(), Some("主线程标题"));
    assert_eq!(child.cwd.as_deref(), Some("D:/work/example"));
    assert_eq!(child.source, RequestLogSessionSource::Omp);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pi_agent_run_subagent_uses_parent_title_without_reading_child_transcript() {
    let root = unique_temp_dir("pi-agent-run-parent-title");
    let parent_id = "019fb0d2-4d04-7000-90dd-9c6255e994e7";
    let child_id = "019fca51-ab55-7000-beca-006a4140fdfb";
    write_pi_session(&root, parent_id, "Parent fallback", Some("Pi 主线程"));
    let parent_stem = root.join(format!("2026-07-30T00-00-00-000Z_{parent_id}"));
    let child_root = parent_stem.join("agent-1").join("run-0");
    write_pi_session(
        &child_root,
        child_id,
        "secret child prompt",
        Some("不应展示"),
    );
    fs::rename(
        child_root.join(format!("2026-07-30T00-00-00-000Z_{child_id}.jsonl")),
        child_root.join("session.jsonl"),
    )
    .expect("rename Pi child session");

    let sessions = list_pi_session_titles_from_root(&root, 20);
    let child = sessions
        .iter()
        .find(|session| session.session_id == child_id)
        .expect("child session");

    assert_eq!(child.title, None);
    assert_eq!(child.parent_session_id.as_deref(), Some(parent_id));
    assert_eq!(child.parent_title.as_deref(), Some("Pi 主线程"));
    assert_eq!(child.source, RequestLogSessionSource::Pi);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pi_project_subagent_artifacts_are_not_assigned_to_a_parent() {
    let root = unique_temp_dir("pi-project-artifacts");
    let parent_id = "019fb0d2-4d04-7000-90dd-9c6255e994e8";
    write_pi_session(&root, parent_id, "Parent fallback", Some("Pi 主线程"));
    let artifacts = root.join("subagent-artifacts");
    write_pi_session(
        &artifacts,
        "019fca51-ab55-7000-beca-006a4140fdfc",
        "artifact",
        Some("不应索引"),
    );

    let sessions = list_pi_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, parent_id);
    assert_eq!(sessions[0].parent_session_id, None);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn omp_subagent_parent_title_refreshes_and_prunes_with_cache() {
    let root = unique_temp_dir("omp-subagent-parent-cache");
    let parent_id = "019fb0d2-4d04-7000-90dd-9c6255e994e9";
    let child_id = "019fca51-ab55-7000-beca-006a4140fdfd";
    write_omp_session(&root, parent_id, "初始主标题", "ignored");
    let parent_stem = root.join(format!("2026-07-30T00-00-00-000Z_{parent_id}"));
    let child_path = parent_stem.join(format!("2026-07-30T00-00-00-000Z_{child_id}.jsonl"));
    write_omp_session(&parent_stem, child_id, "", "ignored");

    let first = list_omp_session_titles_cached(&root, 20);
    let first_child = first
        .iter()
        .find(|candidate| candidate.title.session_id == child_id)
        .expect("cached child");
    assert_eq!(
        first_child.title.parent_title.as_deref(),
        Some("初始主标题")
    );

    write_omp_session(&root, parent_id, "更新后的主标题", "ignored");
    expire_omp_session_title_cache_for_tests();
    let refreshed = list_omp_session_titles_cached(&root, 20);
    let refreshed_child = refreshed
        .iter()
        .find(|candidate| candidate.title.session_id == child_id)
        .expect("refreshed child");
    assert_eq!(
        refreshed_child.title.parent_title.as_deref(),
        Some("更新后的主标题")
    );

    fs::remove_file(child_path).expect("remove child session");
    expire_omp_session_title_cache_for_tests();
    assert!(list_omp_session_titles_cached(&root, 20)
        .iter()
        .all(|candidate| candidate.title.session_id != child_id));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn omp_subagent_skips_broken_child_files_locally() {
    let root = unique_temp_dir("omp-subagent-broken-child");
    let parent_id = "019fb0d2-4d04-7000-90dd-9c6255e994ea";
    let child_id = "019fca51-ab55-7000-beca-006a4140fdfe";
    write_omp_session(&root, parent_id, "主线程标题", "ignored");
    let parent_stem = root.join(format!("2026-07-30T00-00-00-000Z_{parent_id}"));
    write_omp_session(&parent_stem, child_id, "", "ignored");
    fs::write(parent_stem.join("broken.jsonl"), "not json\n").expect("write broken child");

    let sessions = list_omp_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 2);
    let parent = sessions
        .iter()
        .find(|session| session.session_id == parent_id)
        .expect("parent session");
    assert_eq!(parent.parent_session_id, None);
    let child = sessions
        .iter()
        .find(|session| session.session_id == child_id)
        .expect("valid child session");
    assert_eq!(child.parent_session_id.as_deref(), Some(parent_id));
    assert_eq!(child.parent_title.as_deref(), Some("主线程标题"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn omp_subagent_grandchild_layout_is_not_scanned() {
    let root = unique_temp_dir("omp-subagent-grandchild-depth");
    let parent_id = "019fb0d2-4d04-7000-90dd-9c6255e994eb";
    let grandchild_id = "019fca51-ab55-7000-beca-006a4140fdff";
    write_omp_session(&root, parent_id, "主线程标题", "ignored");
    let parent_stem = root.join(format!("2026-07-30T00-00-00-000Z_{parent_id}"));
    write_omp_session(&parent_stem.join("nested"), grandchild_id, "", "ignored");

    let sessions = list_omp_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, parent_id);
    assert!(!sessions
        .iter()
        .any(|session| session.session_id == grandchild_id));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn omp_subagent_with_empty_parent_title_is_omitted() {
    let root = unique_temp_dir("omp-subagent-empty-parent-title");
    let parent_id = "019fb0d2-4d04-7000-90dd-9c6255e994ec";
    let child_id = "019fca51-ab55-7000-beca-006a4140fe00";
    write_omp_session(&root, parent_id, "", "ignored");
    let parent_stem = root.join(format!("2026-07-30T00-00-00-000Z_{parent_id}"));
    write_omp_session(&parent_stem, child_id, "", "ignored");

    let sessions = list_omp_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, parent_id);
    assert_eq!(sessions[0].title, None);
    assert!(!sessions
        .iter()
        .any(|session| session.session_id == child_id));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pi_agent_run_requires_session_header_and_decimal_run_directory() {
    let root = unique_temp_dir("pi-agent-run-header-and-decimal");
    let parent_id = "019fb0d2-4d04-7000-90dd-9c6255e994ed";
    write_pi_session(&root, parent_id, "Parent fallback", Some("Pi 主线程"));
    let parent_stem = root.join(format!("2026-07-30T00-00-00-000Z_{parent_id}"));
    let non_decimal = parent_stem.join("agent-1").join("run-abc");
    fs::create_dir_all(&non_decimal).expect("create run-abc directory");
    fs::write(
        non_decimal.join("session.jsonl"),
        format!(
            "{}\n",
            serde_json::json!({
                "type": "session",
                "version": 3,
                "id": "019fca51-ab55-7000-beca-006a4140fe01",
                "timestamp": "2026-07-30T00:00:00.000Z",
                "cwd": "D:/work/pi-example"
            })
        ),
    )
    .expect("write non-decimal run");
    let non_header = parent_stem.join("agent-2").join("run-1");
    fs::create_dir_all(&non_header).expect("create run-1 directory");
    fs::write(
        non_header.join("session.jsonl"),
        "{\"type\":\"message\",\"id\":\"11111111\"}\n",
    )
    .expect("write non-header run");

    let sessions = list_pi_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, parent_id);
    assert_eq!(sessions[0].parent_session_id, None);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pi_agent_run_direct_jsonl_in_derived_directory_is_not_indexed() {
    let root = unique_temp_dir("pi-derived-direct-jsonl");
    let parent_id = "019fb0d2-4d04-7000-90dd-9c6255e994ee";
    let direct_id = "019fca51-ab55-7000-beca-006a4140fe02";
    write_pi_session(&root, parent_id, "Parent fallback", Some("Pi 主线程"));
    let parent_stem = root.join(format!("2026-07-30T00-00-00-000Z_{parent_id}"));
    write_pi_session(
        &parent_stem,
        direct_id,
        "direct child prompt",
        Some("不应索引"),
    );

    let sessions = list_pi_session_titles_from_root(&root, 20);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, parent_id);
    assert_eq!(sessions[0].parent_session_id, None);
    assert!(!sessions
        .iter()
        .any(|session| session.session_id == direct_id));

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
        parent_session_id: None,
        parent_title: None,
    };
    let omp_collision = ExternalSessionTitleCandidate {
        title: RequestLogSessionTitle {
            session_id: shared_id.clone(),
            title: Some("OMP 标题".to_string()),
            cwd: None,
            source: RequestLogSessionSource::Omp,
            parent_session_id: None,
            parent_title: None,
        },
        updated_at: 99,
        cache_path: std::path::PathBuf::from("omp-collision"),
        parent_cache_path: None,
    };
    let omp_newer = ExternalSessionTitleCandidate {
        title: RequestLogSessionTitle {
            session_id: "019fb0d2-4d04-7000-90dd-9c6255e994e5".to_string(),
            title: Some("最新 OMP 标题".to_string()),
            cwd: None,
            source: RequestLogSessionSource::Omp,
            parent_session_id: None,
            parent_title: None,
        },
        updated_at: 20,
        cache_path: std::path::PathBuf::from("omp-newer"),
        parent_cache_path: None,
    };

    let merged =
        merge_request_log_session_titles(vec![(codex, 10)], vec![omp_collision, omp_newer], 2);

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].source, RequestLogSessionSource::Omp);
    assert_eq!(merged[1].session_id, shared_id);
    assert_eq!(merged[1].title.as_deref(), Some("Codex 标题"));
    assert_eq!(merged[1].source, RequestLogSessionSource::Codex);
}

fn snapshot_title(session_id: &str, title: &str) -> RequestLogSessionTitle {
    RequestLogSessionTitle {
        session_id: session_id.to_string(),
        title: Some(title.to_string()),
        cwd: None,
        source: RequestLogSessionSource::Omp,
        parent_session_id: None,
        parent_title: None,
    }
}

fn refresh_snapshot(cache: &Arc<SessionTitleSnapshotCache>, titles: Vec<RequestLogSessionTitle>) {
    assert!(cache
        .snapshot_and_schedule(MAX_SESSION_TITLE_LIMIT, move || Ok(titles))
        .is_empty());
    cache.wait_for_refresh_for_tests();
}

#[test]
fn session_title_snapshot_cold_call_returns_before_blocked_refresh() {
    let cache = Arc::new(SessionTitleSnapshotCache::new());
    let (started_tx, started_rx) = mpsc::channel();
    let release = Arc::new(Barrier::new(2));
    let worker_release = Arc::clone(&release);

    let returned = cache.snapshot_and_schedule(MAX_SESSION_TITLE_LIMIT, move || {
        started_tx.send(()).expect("signal refresh start");
        worker_release.wait();
        Ok(vec![snapshot_title(
            "019fb0d2-4d04-7000-90dd-9c6255e994e4",
            "已发布标题",
        )])
    });

    assert!(returned.is_empty());
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("refresh starts after caller returns");
    assert!(cache.refresh_in_flight_for_tests());

    release.wait();
    cache.wait_for_refresh_for_tests();
    assert_eq!(
        cache.snapshot_and_schedule(MAX_SESSION_TITLE_LIMIT, || panic!(
            "fresh snapshot schedules"
        )),
        vec![snapshot_title(
            "019fb0d2-4d04-7000-90dd-9c6255e994e4",
            "已发布标题"
        )]
    );
}

#[test]
fn session_title_snapshot_stale_call_keeps_child_projection_during_refresh() {
    let cache = Arc::new(SessionTitleSnapshotCache::new());
    let child = RequestLogSessionTitle {
        session_id: "019fca51-ab55-7000-beca-006a4140fdfa".to_string(),
        title: None,
        cwd: Some("D:/work/example".to_string()),
        source: RequestLogSessionSource::Omp,
        parent_session_id: Some("019fb0d2-4d04-7000-90dd-9c6255e994e4".to_string()),
        parent_title: Some("主线程标题".to_string()),
    };
    refresh_snapshot(&cache, vec![child.clone()]);
    cache.expire_for_tests();

    let (started_tx, started_rx) = mpsc::channel();
    let release = Arc::new(Barrier::new(2));
    let worker_release = Arc::clone(&release);
    let stale = cache.snapshot_and_schedule(MAX_SESSION_TITLE_LIMIT, move || {
        started_tx.send(()).expect("signal refresh start");
        worker_release.wait();
        Ok(vec![snapshot_title(
            "019fb0d2-4d04-7000-90dd-9c6255e994e5",
            "新标题",
        )])
    });

    assert_eq!(stale, vec![child]);
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("refresh starts after stale snapshot returns");
    release.wait();
    cache.wait_for_refresh_for_tests();
}

#[test]
fn session_title_snapshot_concurrent_callers_start_one_refresh() {
    let cache = Arc::new(SessionTitleSnapshotCache::new());
    let callers = 5;
    let start = Arc::new(Barrier::new(callers + 1));
    let release = Arc::new(Barrier::new(2));
    let (started_tx, started_rx) = mpsc::channel();
    let refresh_count = Arc::new(AtomicUsize::new(0));
    let mut joins = Vec::new();

    for _ in 0..callers {
        let cache = Arc::clone(&cache);
        let start = Arc::clone(&start);
        let release = Arc::clone(&release);
        let started_tx = started_tx.clone();
        let refresh_count = Arc::clone(&refresh_count);
        joins.push(std::thread::spawn(move || {
            start.wait();
            cache.snapshot_and_schedule(MAX_SESSION_TITLE_LIMIT, move || {
                refresh_count.fetch_add(1, Ordering::SeqCst);
                started_tx.send(()).expect("signal refresh start");
                release.wait();
                Ok(Vec::new())
            })
        }));
    }

    start.wait();
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("one refresh starts");
    for join in joins {
        assert!(join.join().expect("caller thread").is_empty());
    }
    assert_eq!(refresh_count.load(Ordering::SeqCst), 1);

    release.wait();
    cache.wait_for_refresh_for_tests();
}

#[test]
fn session_title_snapshot_error_preserves_previous_generation_and_limit() {
    let cache = Arc::new(SessionTitleSnapshotCache::new());
    let first = snapshot_title("019fb0d2-4d04-7000-90dd-9c6255e994e4", "第一个标题");
    let second = snapshot_title("019fb0d2-4d04-7000-90dd-9c6255e994e5", "第二个标题");
    refresh_snapshot(&cache, vec![first.clone(), second]);
    cache.expire_for_tests();

    assert_eq!(
        cache.snapshot_and_schedule(MAX_SESSION_TITLE_LIMIT, || Err("scan failed".to_string())),
        vec![
            first.clone(),
            snapshot_title("019fb0d2-4d04-7000-90dd-9c6255e994e5", "第二个标题")
        ]
    );
    cache.wait_for_refresh_for_tests();

    assert_eq!(
        cache.snapshot_and_schedule(1, || panic!("failed refresh schedules too early")),
        vec![first]
    );
}
