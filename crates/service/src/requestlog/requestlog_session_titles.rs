use crate::codex_session::{self, SessionListOptions};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_SESSION_TITLE_LIMIT: usize = 2_000;
const MAX_SESSION_TITLE_LIMIT: usize = 2_000;
const MAX_OMP_SESSION_FILES: usize = 4_000;
const MAX_OMP_DIRECTORY_ENTRIES: usize = 16_000;
const MAX_OMP_PROJECT_DIRECTORIES: usize = 512;
const MAX_OMP_TITLE_SLOT_BYTES: usize = 1_024;
const MAX_OMP_SESSION_HEADER_BYTES: usize = 3 * 1024;
const MAX_SESSION_ID_BYTES: usize = 256;
const MAX_TITLE_BYTES: usize = 256;
const MAX_CWD_BYTES: usize = 4 * 1024;
const OMP_CACHE_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RequestLogSessionSource {
    Codex,
    Omp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestLogSessionTitle {
    pub(crate) session_id: String,
    pub(crate) title: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) source: RequestLogSessionSource,
}

#[derive(Debug, Clone)]
struct OmpSessionTitleCandidate {
    title: RequestLogSessionTitle,
    updated_at: i64,
}

#[derive(Debug, Clone)]
struct CachedOmpSessionTitle {
    modified_at: Option<SystemTime>,
    size: u64,
    candidate: Option<OmpSessionTitleCandidate>,
}

#[derive(Default)]
struct OmpSessionTitleCache {
    root: Option<PathBuf>,
    refreshed_at: Option<Instant>,
    entries: HashMap<PathBuf, CachedOmpSessionTitle>,
}

static OMP_SESSION_TITLE_CACHE: LazyLock<Mutex<OmpSessionTitleCache>> =
    LazyLock::new(|| Mutex::new(OmpSessionTitleCache::default()));

#[derive(Clone)]
struct OmpSessionDirectory {
    path: PathBuf,
    cache_path: PathBuf,
    handle: Arc<File>,
}

struct OmpSessionFile {
    directory: OmpSessionDirectory,
    path: PathBuf,
    cache_path: PathBuf,
    #[cfg(windows)]
    name: std::ffi::OsString,
}

pub(crate) fn list_request_log_session_titles(
    limit: Option<i64>,
) -> Result<Vec<RequestLogSessionTitle>, String> {
    let limit = normalize_session_title_limit(limit);
    let codex_sessions = codex_session::list_sessions_with_options(
        &codex_session::default_codex_db_path(),
        &SessionListOptions {
            limit: Some(limit as i64),
            ..Default::default()
        },
    )
    .unwrap_or_default();
    let codex_titles = codex_sessions
        .into_iter()
        .filter_map(|session| {
            let session_id = normalize_session_id(Some(session.session_id.as_str()))?;
            Some((
                RequestLogSessionTitle {
                    session_id,
                    title: normalize_title(session.title.as_deref()),
                    cwd: normalize_cwd(session.cwd.as_deref()),
                    source: RequestLogSessionSource::Codex,
                },
                session.updated_at.unwrap_or_default(),
            ))
        })
        .collect();
    Ok(merge_request_log_session_titles(
        codex_titles,
        list_omp_session_titles_cached(&resolve_omp_sessions_root(), limit),
        limit,
    ))
}

fn merge_request_log_session_titles(
    codex_titles: Vec<(RequestLogSessionTitle, i64)>,
    omp_titles: Vec<OmpSessionTitleCandidate>,
    limit: usize,
) -> Vec<RequestLogSessionTitle> {
    let mut titles = HashMap::<String, (RequestLogSessionTitle, i64, u8)>::new();
    for (title, updated_at) in codex_titles {
        titles.insert(title.session_id.clone(), (title, updated_at, 1));
    }
    for candidate in omp_titles {
        let session_id = candidate.title.session_id.clone();
        titles
            .entry(session_id)
            .or_insert((candidate.title, candidate.updated_at, 0));
    }
    let mut titles = titles.into_values().collect::<Vec<_>>();
    titles.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| left.0.session_id.cmp(&right.0.session_id))
    });
    titles.truncate(limit.min(MAX_SESSION_TITLE_LIMIT));
    titles.into_iter().map(|(title, _, _)| title).collect()
}

pub(crate) fn list_omp_session_titles_from_root(
    root: &Path,
    limit: usize,
) -> Vec<RequestLogSessionTitle> {
    let Some(directory) = open_omp_session_directory(root) else {
        return Vec::new();
    };
    let paths = collect_omp_session_paths(&directory);
    let mut titles = paths
        .into_iter()
        .filter_map(|path| read_omp_session_title(&path, None))
        .collect::<Vec<_>>();
    titles.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.title.session_id.cmp(&right.title.session_id))
    });
    titles.truncate(limit.min(MAX_SESSION_TITLE_LIMIT));
    titles
        .into_iter()
        .map(|candidate| candidate.title)
        .collect()
}

fn list_omp_session_titles_cached(root: &Path, limit: usize) -> Vec<OmpSessionTitleCandidate> {
    let prior_entries = {
        let cache = OMP_SESSION_TITLE_CACHE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cache.root.as_deref() == Some(root)
            && cache
                .refreshed_at
                .is_some_and(|refreshed_at| refreshed_at.elapsed() < OMP_CACHE_REFRESH_INTERVAL)
        {
            return cached_omp_session_titles(&cache.entries, limit);
        }
        if cache.root.as_deref() == Some(root) {
            cache.entries.clone()
        } else {
            HashMap::new()
        }
    };

    let Some(directory) = open_omp_session_directory(root) else {
        let mut cache = OMP_SESSION_TITLE_CACHE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.root = Some(root.to_path_buf());
        cache.refreshed_at = Some(Instant::now());
        cache.entries.clear();
        return Vec::new();
    };
    let paths = collect_omp_session_paths(&directory);
    let mut next_entries = HashMap::with_capacity(paths.len());
    for path in paths {
        let metadata = match read_omp_session_metadata(&path) {
            Some(metadata) => metadata,
            None => continue,
        };
        let modified_at = metadata.modified().ok();
        let candidate = if let Some(entry) = prior_entries
            .get(&path.cache_path)
            .filter(|entry| entry.modified_at == modified_at && entry.size == metadata.len())
        {
            entry.candidate.clone()
        } else {
            read_omp_session_title(&path, Some(&metadata))
        };
        next_entries.insert(
            path.cache_path,
            CachedOmpSessionTitle {
                modified_at,
                size: metadata.len(),
                candidate,
            },
        );
    }

    let mut cache = OMP_SESSION_TITLE_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.root = Some(root.to_path_buf());
    cache.refreshed_at = Some(Instant::now());
    cache.entries = next_entries;
    cached_omp_session_titles(&cache.entries, limit)
}

fn cached_omp_session_titles(
    entries: &HashMap<PathBuf, CachedOmpSessionTitle>,
    limit: usize,
) -> Vec<OmpSessionTitleCandidate> {
    let mut titles = entries
        .values()
        .filter_map(|entry| entry.candidate.clone())
        .collect::<Vec<_>>();
    titles.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.title.session_id.cmp(&right.title.session_id))
    });
    titles.truncate(limit.min(MAX_SESSION_TITLE_LIMIT));
    titles
}

fn open_omp_session_directory(root: &Path) -> Option<OmpSessionDirectory> {
    let metadata = fs::symlink_metadata(root).ok()?;
    if !metadata.is_dir() || metadata_is_unsafe_link_or_reparse(&metadata) {
        return None;
    }
    let handle = open_directory_no_follow(root).ok()?;
    let opened_metadata = handle.metadata().ok()?;
    if !opened_metadata.is_dir() || metadata_is_unsafe_link_or_reparse(&opened_metadata) {
        return None;
    }
    let path = stable_directory_path(root, &handle)?;
    Some(OmpSessionDirectory {
        path,
        cache_path: root.to_path_buf(),
        handle: Arc::new(handle),
    })
}

fn open_omp_session_child_directory(
    directory: &OmpSessionDirectory,
    name: &std::ffi::OsStr,
) -> Option<OmpSessionDirectory> {
    if !is_single_normal_path_component(name) {
        return None;
    }
    #[cfg(not(windows))]
    {
        let path = directory.path.join(name);
        let metadata = fs::symlink_metadata(&path).ok()?;
        if !metadata.is_dir() || metadata_is_unsafe_link_or_reparse(&metadata) {
            return None;
        }
        let handle = open_directory_no_follow(&path).ok()?;
        let opened_metadata = handle.metadata().ok()?;
        if !opened_metadata.is_dir() || metadata_is_unsafe_link_or_reparse(&opened_metadata) {
            return None;
        }
        return Some(OmpSessionDirectory {
            path,
            cache_path: directory.cache_path.join(name),
            handle: Arc::new(handle),
        });
    }
    #[cfg(windows)]
    {
        open_windows_relative_directory_no_follow(directory, name)
    }
}

#[cfg(not(windows))]
fn stable_directory_path(_path: &Path, handle: &File) -> Option<PathBuf> {
    use std::os::fd::AsRawFd;

    let descriptor_path = PathBuf::from(format!("/proc/self/fd/{}", handle.as_raw_fd()));
    fs::read_dir(&descriptor_path).ok().map(|_| descriptor_path)
}

#[cfg(windows)]
fn stable_directory_path(path: &Path, _handle: &File) -> Option<PathBuf> {
    fs::canonicalize(path).ok()
}

fn collect_omp_session_paths(directory: &OmpSessionDirectory) -> Vec<OmpSessionFile> {
    let mut entries_seen = 0;
    let mut paths = Vec::new();
    let mut project_directories = Vec::new();
    collect_omp_session_paths_in_directory(
        directory,
        true,
        &mut entries_seen,
        &mut paths,
        &mut project_directories,
    );
    for project_directory in project_directories {
        if entries_seen >= MAX_OMP_DIRECTORY_ENTRIES || paths.len() >= MAX_OMP_SESSION_FILES {
            break;
        }
        collect_omp_session_paths_in_directory(
            &project_directory,
            false,
            &mut entries_seen,
            &mut paths,
            &mut Vec::new(),
        );
    }
    paths
}

fn collect_omp_session_paths_in_directory(
    directory: &OmpSessionDirectory,
    collect_project_directories: bool,
    entries_seen: &mut usize,
    paths: &mut Vec<OmpSessionFile>,
    project_directories: &mut Vec<OmpSessionDirectory>,
) {
    #[cfg(windows)]
    {
        collect_windows_omp_session_paths(
            directory,
            collect_project_directories,
            entries_seen,
            paths,
            project_directories,
        );
    }
    #[cfg(not(windows))]
    {
        if *entries_seen >= MAX_OMP_DIRECTORY_ENTRIES || paths.len() >= MAX_OMP_SESSION_FILES {
            return;
        }
        let Ok(entries) = fs::read_dir(&directory.path) else {
            return;
        };
        for entry in entries.flatten() {
            if *entries_seen >= MAX_OMP_DIRECTORY_ENTRIES || paths.len() >= MAX_OMP_SESSION_FILES {
                return;
            }
            *entries_seen += 1;
            let name = entry.file_name();
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.is_file()
                && !metadata_is_unsafe_link_or_reparse(&metadata)
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
            {
                paths.push(OmpSessionFile {
                    directory: directory.clone(),
                    path,
                    cache_path: directory.cache_path.join(&name),
                });
            } else if collect_project_directories
                && project_directories.len() < MAX_OMP_PROJECT_DIRECTORIES
                && metadata.is_dir()
                && !metadata_is_unsafe_link_or_reparse(&metadata)
            {
                if let Some(project_directory) = open_omp_session_child_directory(directory, &name)
                {
                    project_directories.push(project_directory);
                }
            }
        }
    }
}

fn read_omp_session_metadata(path: &OmpSessionFile) -> Option<Metadata> {
    let file = open_omp_session_file(path).ok()?;
    let metadata = file.metadata().ok()?;
    (metadata.is_file() && !metadata_is_unsafe_link_or_reparse(&metadata)).then_some(metadata)
}

fn read_omp_session_title(
    path: &OmpSessionFile,
    expected_metadata: Option<&Metadata>,
) -> Option<OmpSessionTitleCandidate> {
    let mut file = open_omp_session_file(path).ok()?;
    let opened_metadata = file.metadata().ok()?;
    if !opened_metadata.is_file()
        || metadata_is_unsafe_link_or_reparse(&opened_metadata)
        || expected_metadata.is_some_and(|expected| !metadata_matches(expected, &opened_metadata))
    {
        return None;
    }
    let title_slot = parse_json_object(&read_jsonl_line(&mut file, MAX_OMP_TITLE_SLOT_BYTES)?)?;
    let session_header =
        parse_json_object(&read_jsonl_line(&mut file, MAX_OMP_SESSION_HEADER_BYTES)?)?;
    if title_slot.get("type").and_then(Value::as_str) != Some("title")
        || session_header.get("type").and_then(Value::as_str) != Some("session")
    {
        return None;
    }
    let session_id = normalize_omp_session_id(session_header.get("id").and_then(Value::as_str))?;
    let title = normalize_title(title_slot.get("title").and_then(Value::as_str))
        .or_else(|| normalize_title(session_header.get("title").and_then(Value::as_str)));
    let cwd = normalize_cwd(session_header.get("cwd").and_then(Value::as_str));
    Some(OmpSessionTitleCandidate {
        title: RequestLogSessionTitle {
            session_id,
            title,
            cwd,
            source: RequestLogSessionSource::Omp,
        },
        updated_at: system_time_to_seconds(opened_metadata.modified().ok()),
    })
}

#[cfg(not(windows))]
fn open_omp_session_file(path: &OmpSessionFile) -> std::io::Result<File> {
    open_read_only_no_follow(&path.path)
}

#[cfg(windows)]
fn open_omp_session_file(path: &OmpSessionFile) -> std::io::Result<File> {
    open_windows_relative_file_no_follow(&path.directory.handle, &path.name)
}

fn read_jsonl_line(file: &mut File, max_bytes: usize) -> Option<String> {
    let mut bytes = Vec::with_capacity(max_bytes.min(256));
    while bytes.len() < max_bytes {
        let mut byte = [0_u8; 1];
        if file.read(&mut byte).ok()? == 0 {
            return None;
        }
        if byte[0] == b'\n' {
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            return String::from_utf8(bytes).ok();
        }
        bytes.push(byte[0]);
    }
    None
}

fn metadata_matches(left: &Metadata, right: &Metadata) -> bool {
    left.len() == right.len() && left.modified().ok() == right.modified().ok()
}

fn open_read_only_no_follow(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

#[cfg(not(windows))]
fn open_directory_no_follow(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    options.open(path)
}

#[cfg(windows)]
fn open_directory_no_follow(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    options.open(path)
}

#[cfg(windows)]
fn collect_windows_omp_session_paths(
    directory: &OmpSessionDirectory,
    collect_project_directories: bool,
    entries_seen: &mut usize,
    paths: &mut Vec<OmpSessionFile>,
    project_directories: &mut Vec<OmpSessionDirectory>,
) {
    use std::mem::size_of;
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::AsRawHandle;

    const FILE_DIRECTORY_INFORMATION: u32 = 1;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    const STATUS_NO_MORE_FILES: i32 = 0x8000_0006_u32 as i32;
    let mut restart_scan = 1_u8;
    let mut buffer = vec![0_u64; 8 * 1024];
    let buffer_len = buffer.len() * size_of::<u64>();
    while *entries_seen < MAX_OMP_DIRECTORY_ENTRIES && paths.len() < MAX_OMP_SESSION_FILES {
        let mut io_status = IoStatusBlock {
            status: 0,
            information: 0,
        };
        // SAFETY: `directory.handle` is a live directory handle owned by `Arc<File>`.
        // `buffer` is aligned storage with `buffer_len` writable bytes, and `io_status`
        // points to initialized writable storage for the duration of this call.
        let status = unsafe {
            nt_query_directory_file(
                directory.handle.as_raw_handle(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut io_status,
                buffer.as_mut_ptr().cast(),
                buffer_len as u32,
                FILE_DIRECTORY_INFORMATION,
                0,
                std::ptr::null_mut(),
                restart_scan,
            )
        };
        restart_scan = 0;
        if status == STATUS_NO_MORE_FILES {
            return;
        }
        if status < 0 || io_status.information == 0 || io_status.information > buffer_len {
            return;
        }
        // SAFETY: `buffer` is initialized aligned storage. Viewing its complete allocation
        // as bytes preserves its validity and the following parser only reads `information`.
        let buffer_bytes =
            unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), buffer_len) };

        let mut offset = 0_usize;
        while offset < io_status.information
            && *entries_seen < MAX_OMP_DIRECTORY_ENTRIES
            && paths.len() < MAX_OMP_SESSION_FILES
        {
            if io_status.information - offset < size_of::<FileDirectoryInformationHeader>() {
                return;
            }
            // SAFETY: the range check above proves a full header remains in `buffer_bytes`.
            // `read_unaligned` avoids creating a potentially misaligned Rust reference.
            let header = unsafe {
                std::ptr::read_unaligned(
                    buffer_bytes
                        .as_ptr()
                        .add(offset)
                        .cast::<FileDirectoryInformationHeader>(),
                )
            };
            let name_offset = offset + size_of::<FileDirectoryInformationHeader>();
            let name_len = header.file_name_length as usize;
            let Some(name_end) = name_offset.checked_add(name_len) else {
                return;
            };
            if name_len % 2 != 0 || name_end > io_status.information {
                return;
            }
            *entries_seen += 1;
            if header.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
                let chars = buffer_bytes[name_offset..name_end]
                    .chunks_exact(2)
                    .map(|bytes| u16::from_ne_bytes([bytes[0], bytes[1]]))
                    .collect::<Vec<_>>();
                let name = std::ffi::OsString::from_wide(&chars);
                if is_single_normal_path_component(&name) {
                    if header.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                        if collect_project_directories
                            && project_directories.len() < MAX_OMP_PROJECT_DIRECTORIES
                        {
                            if let Some(project_directory) =
                                open_omp_session_child_directory(directory, &name)
                            {
                                project_directories.push(project_directory);
                            }
                        }
                    } else if Path::new(&name)
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
                    {
                        paths.push(OmpSessionFile {
                            directory: directory.clone(),
                            path: directory.path.join(&name),
                            cache_path: directory.cache_path.join(&name),
                            name,
                        });
                    }
                }
            }
            if header.next_entry_offset == 0 {
                break;
            }
            let next = header.next_entry_offset as usize;
            let Some(next_offset) = offset.checked_add(next) else {
                return;
            };
            if next < size_of::<FileDirectoryInformationHeader>()
                || next_offset > io_status.information
            {
                return;
            }
            offset = next_offset;
        }
    }
}

#[cfg(windows)]
fn open_windows_relative_directory_no_follow(
    parent: &OmpSessionDirectory,
    name: &std::ffi::OsStr,
) -> Option<OmpSessionDirectory> {
    const FILE_LIST_DIRECTORY: u32 = 0x0000_0001;
    const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x0000_0020;
    const FILE_DIRECTORY_FILE: u32 = 0x0000_0001;
    const FILE_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let handle = open_windows_relative_no_follow(
        &parent.handle,
        name,
        FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_SYNCHRONOUS_IO_NONALERT | FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT,
    )
    .ok()?;
    let metadata = handle.metadata().ok()?;
    (metadata.is_dir() && !metadata_is_unsafe_link_or_reparse(&metadata)).then(|| {
        OmpSessionDirectory {
            path: parent.path.join(name),
            cache_path: parent.cache_path.join(name),
            handle: Arc::new(handle),
        }
    })
}

#[cfg(windows)]
fn open_windows_relative_file_no_follow(
    directory: &File,
    name: &std::ffi::OsStr,
) -> std::io::Result<File> {
    const FILE_READ_DATA: u32 = 0x0000_0001;
    const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x0000_0020;
    const FILE_NON_DIRECTORY_FILE: u32 = 0x0000_0040;
    const FILE_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    open_windows_relative_no_follow(
        directory,
        name,
        FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_SYNCHRONOUS_IO_NONALERT | FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT,
    )
}

#[cfg(windows)]
fn open_windows_relative_no_follow(
    directory: &File,
    name: &std::ffi::OsStr,
    desired_access: u32,
    create_options: u32,
) -> std::io::Result<File> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};

    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE: u32 = 0x0000_0004;
    const FILE_OPEN: u32 = 0x0000_0001;
    const OBJ_CASE_INSENSITIVE: u32 = 0x0000_0040;
    if !is_single_normal_path_component(name) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "session path must be a single normal path component",
        ));
    }
    let mut wide = name.encode_wide().collect::<Vec<_>>();
    let byte_len = wide
        .len()
        .checked_mul(2)
        .and_then(|length| u16::try_from(length).ok())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "session path is too long")
        })?;
    let mut unicode_name = UnicodeString {
        length: byte_len,
        maximum_length: byte_len,
        buffer: wide.as_mut_ptr(),
    };
    let mut attributes = ObjectAttributes {
        length: std::mem::size_of::<ObjectAttributes>() as u32,
        root_directory: directory.as_raw_handle(),
        object_name: &mut unicode_name,
        attributes: OBJ_CASE_INSENSITIVE,
        security_descriptor: std::ptr::null_mut(),
        security_quality_of_service: std::ptr::null_mut(),
    };
    let mut io_status = IoStatusBlock {
        status: 0,
        information: 0,
    };
    let mut handle = std::ptr::null_mut();
    // SAFETY: `directory` is a live handle owned by the caller, `wide` and all
    // NT argument structures remain valid for this synchronous call, and their
    // pointers reference initialized writable storage with the declared layouts.
    let status = unsafe {
        nt_create_file(
            &mut handle,
            desired_access,
            &mut attributes,
            &mut io_status,
            std::ptr::null_mut(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
            create_options,
            std::ptr::null_mut(),
            0,
        )
    };
    if status < 0 || handle.is_null() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "unable to open session metadata safely",
        ));
    }
    // SAFETY: a successful `NtCreateFile` returned a non-null, uniquely owned
    // handle. `File` takes ownership exactly once and closes it on drop.
    Ok(unsafe { File::from_raw_handle(handle) })
}

fn is_single_normal_path_component(name: &std::ffi::OsStr) -> bool {
    let mut components = Path::new(name).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

#[cfg(windows)]
#[repr(C)]
struct IoStatusBlock {
    status: i32,
    information: usize,
}

#[cfg(windows)]
#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

#[cfg(windows)]
#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: *mut std::ffi::c_void,
    object_name: *mut UnicodeString,
    attributes: u32,
    security_descriptor: *mut std::ffi::c_void,
    security_quality_of_service: *mut std::ffi::c_void,
}

#[cfg(windows)]
#[repr(C)]
struct FileDirectoryInformationHeader {
    next_entry_offset: u32,
    file_index: u32,
    creation_time: i64,
    last_access_time: i64,
    last_write_time: i64,
    change_time: i64,
    end_of_file: i64,
    allocation_size: i64,
    file_attributes: u32,
    file_name_length: u32,
}

#[cfg(windows)]
#[link(name = "ntdll")]
unsafe extern "system" {
    #[link_name = "NtCreateFile"]
    fn nt_create_file(
        file_handle: *mut *mut std::ffi::c_void,
        desired_access: u32,
        object_attributes: *mut ObjectAttributes,
        io_status_block: *mut IoStatusBlock,
        allocation_size: *mut i64,
        file_attributes: u32,
        share_access: u32,
        create_disposition: u32,
        create_options: u32,
        ea_buffer: *mut std::ffi::c_void,
        ea_length: u32,
    ) -> i32;

    #[link_name = "NtQueryDirectoryFile"]
    fn nt_query_directory_file(
        file_handle: *mut std::ffi::c_void,
        event: *mut std::ffi::c_void,
        apc_routine: *mut std::ffi::c_void,
        apc_context: *mut std::ffi::c_void,
        io_status_block: *mut IoStatusBlock,
        file_information: *mut std::ffi::c_void,
        length: u32,
        file_information_class: u32,
        return_single_entry: u8,
        file_name: *mut UnicodeString,
        restart_scan: u8,
    ) -> i32;
}

fn metadata_is_unsafe_link_or_reparse(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

fn parse_json_object(line: &str) -> Option<serde_json::Map<String, Value>> {
    serde_json::from_str::<Value>(line)
        .ok()?
        .as_object()
        .cloned()
}

fn normalize_session_title_limit(limit: Option<i64>) -> usize {
    limit
        .unwrap_or(DEFAULT_SESSION_TITLE_LIMIT as i64)
        .clamp(1, MAX_SESSION_TITLE_LIMIT as i64) as usize
}

fn normalize_session_id(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (value.len() <= MAX_SESSION_ID_BYTES
        && !value.is_empty()
        && value.bytes().all(|byte| matches!(byte, b'!'..=b'~')))
    .then(|| value.to_string())
}

fn normalize_omp_session_id(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            matches!(index, 8 | 13 | 18 | 23) && byte == b'-'
                || !matches!(index, 8 | 13 | 18 | 23) && byte.is_ascii_hexdigit()
        }))
    .then(|| value.to_ascii_lowercase())
}

fn normalize_title(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (value.len() <= MAX_TITLE_BYTES && !value.is_empty() && !value.chars().any(char::is_control))
        .then(|| value.to_string())
}

fn normalize_cwd(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (value.len() <= MAX_CWD_BYTES && !value.is_empty() && !value.chars().any(char::is_control))
        .then(|| value.to_string())
}

fn system_time_to_seconds(value: Option<SystemTime>) -> i64 {
    value
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default()
}

fn resolve_omp_sessions_root() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .or_else(|| {
            let drive = std::env::var_os("HOMEDRIVE")?;
            let path = std::env::var_os("HOMEPATH")?;
            Some(PathBuf::from(drive).join(path).into_os_string())
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let config_dir = std::env::var_os("PI_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| ".omp".into());
    let mut root = home.join(config_dir);
    if let Some(profile) = std::env::var("OMP_PROFILE")
        .ok()
        .or_else(|| std::env::var("PI_PROFILE").ok())
        .filter(|profile| profile != "default")
        .filter(|profile| valid_omp_profile(profile))
    {
        root = root.join("profiles").join(profile);
    }
    root.join("agent").join("sessions")
}

fn valid_omp_profile(profile: &str) -> bool {
    let bytes = profile.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

#[cfg(test)]
fn expire_omp_session_title_cache_for_tests() {
    let mut cache = OMP_SESSION_TITLE_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.refreshed_at = None;
}
#[cfg(test)]
#[path = "requestlog_session_titles_tests.rs"]
mod tests;
