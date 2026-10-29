//! HTTP GET with a disk cache in the `cache` folder of the data folder. The data
//! folder is `%LOCALAPPDATA%\Hexsight`, or `Hexsight-data` next to the exe in
//! portable mode.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

/// When on, no cached file is fresh. Set it with `force_refresh`.
static FORCE: AtomicBool = AtomicBool::new(false);

/// The name this app had before it became Hexsight. Its data folder
/// `%LOCALAPPDATA%\Augur` moves to the new one at the first start. Legacy name, keep it.
const LEGACY_NAME: &str = "Augur";

/// The name of the data folder under `%LOCALAPPDATA%`.
const APP_NAME: &str = "Hexsight";

/// The marker file of portable mode. When it sits next to the exe, the data
/// folder is next to the exe too. Its content does not matter.
pub const PORTABLE_MARKER: &str = "Hexsight.portable";

/// The data folder next to the exe in portable mode.
const PORTABLE_DIR: &str = "Hexsight-data";

/// Where the data folder is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// `%LOCALAPPDATA%\Hexsight`: there is no marker next to the exe.
    Local,
    /// `<exe folder>\Hexsight-data`: the marker sits next to the exe.
    Portable,
    /// The marker sits next to the exe, but Hexsight cannot write there (for
    /// example under Program Files, or on a read-only stick). The data is in
    /// `%LOCALAPPDATA%\Hexsight`.
    PortableFallback,
}

impl Place {
    /// True when the marker sits next to the exe, also when the fallback happened.
    pub fn portable(self) -> bool {
        self != Place::Local
    }
}

/// Choose the data folder. `exe_dir` is the folder of the running exe, and
/// `marker` says if the marker file is in it. `writable` checks the portable
/// folder. It runs only when the marker is there. `local` is `%LOCALAPPDATA%`.
/// Without it, the temp folder `temp` stands in, as before portable mode.
fn choose_data_dir(
    exe_dir: Option<&Path>,
    marker: bool,
    writable: impl FnOnce(&Path) -> bool,
    local: Option<&Path>,
    temp: &Path,
) -> (PathBuf, Place) {
    let local_dir = local.unwrap_or(temp).join(APP_NAME);
    match exe_dir {
        Some(exe) if marker => {
            let portable = exe.join(PORTABLE_DIR);
            if writable(&portable) {
                (portable, Place::Portable)
            } else {
                (local_dir, Place::PortableFallback)
            }
        }
        _ => (local_dir, Place::Local),
    }
}

/// Make `dir` (and its parents), then write and delete a small file in it.
/// True when all of it works. A failed check removes the folder it made.
fn can_write(dir: &Path) -> bool {
    let existed = dir.is_dir();
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    // A name of its own for each check: an old probe file or folder with the
    // same name cannot make the check fail.
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let probe = dir.join(format!(".write-test-{}-{nanos}", std::process::id()));
    let ok = std::fs::write(&probe, b"ok").is_ok();
    let _ = std::fs::remove_file(&probe);
    if !ok && !existed {
        // `remove_dir` removes an empty folder only.
        let _ = std::fs::remove_dir(dir);
    }
    ok
}

/// The data folder and where it is. Hexsight chooses it once, at the first call.
struct DataDir {
    path: PathBuf,
    place: Place,
}

static DATA: OnceLock<DataDir> = OnceLock::new();

fn data_dir() -> &'static DataDir {
    DATA.get_or_init(|| {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        let marker = exe_dir
            .as_ref()
            .is_some_and(|d| d.join(PORTABLE_MARKER).is_file());
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        let (path, place) = choose_data_dir(
            exe_dir.as_deref(),
            marker,
            can_write,
            local.as_deref(),
            &std::env::temp_dir(),
        );
        // The tests never touch the real data folders. Portable mode moves no legacy data.
        #[cfg(not(test))]
        if place != Place::Portable
            && let Some(base) = path.parent()
        {
            migrate_legacy_dir(base);
        }
        DataDir { path, place }
    })
}

/// Where the data folder is. The About group of the Settings page shows it.
pub fn place() -> Place {
    data_dir().place
}

/// What `migrate_legacy_dir` did.
#[derive(Debug, PartialEq, Eq)]
enum Migration {
    /// The new folder holds settings or history already, or there is no legacy folder.
    None,
    /// The legacy folder was renamed to the new name.
    Moved,
    /// The rename failed (the old app still runs, or a file is locked), or the new
    /// folder existed without settings and history. The missing settings, history
    /// and cache files were copied, and the old folder stays.
    Copied,
}

/// Move the legacy data folder to the new name when the legacy one exists and
/// the new one holds no settings and no history. Run it before anything creates
/// the new folder. A new folder with a settings or history file stays as it is.
/// A new folder without them (for example one that only holds a cache) gets the
/// missing files of the legacy folder. A file in the new folder is never replaced.
fn migrate_legacy_dir(base: &Path) -> Migration {
    let new = base.join(APP_NAME);
    let old = base.join(LEGACY_NAME);
    if !old.is_dir()
        || ["settings.json", "history.json"]
            .iter()
            .any(|name| new.join(name).exists())
    {
        return Migration::None;
    }
    if !new.exists() && std::fs::rename(&old, &new).is_ok() {
        return Migration::Moved;
    }
    // A rename can fail while the old app keeps a file open. Copy what matters.
    let _ = std::fs::create_dir_all(&new);
    for name in ["settings.json", "history.json"] {
        copy_new_file(&old.join(name), &new.join(name));
    }
    copy_dir(&old.join("cache"), &new.join("cache"));
    Migration::Copied
}

/// Copy one file, unless `to` exists already. Ignore errors.
fn copy_new_file(from: &Path, to: &Path) {
    if !to.exists() {
        let _ = std::fs::copy(from, to);
    }
}

/// Copy the files of `from` (and its folders) into `to`. Files that exist in
/// `to` stay. Ignore errors.
fn copy_dir(from: &Path, to: &Path) {
    let Ok(entries) = std::fs::read_dir(from) else {
        return;
    };
    let _ = std::fs::create_dir_all(to);
    for e in entries.flatten() {
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()));
        } else if kind.is_file() {
            copy_new_file(&e.path(), &to.join(e.file_name()));
        }
    }
}

/// The cache folder in the data folder. It makes the folder when it is missing.
pub fn dir() -> PathBuf {
    let d = data_dir().path.join("cache");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// The data folder of Hexsight: `%LOCALAPPDATA%\Hexsight`, or `Hexsight-data`
/// next to the exe in portable mode. It holds the settings, the history, the
/// cache folder, the saved scan, `crash.log`, and the data log `hexsight.log`.
/// It makes the folder when it is missing.
pub fn app_dir() -> PathBuf {
    let d = data_dir().path.clone();
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Like `app_dir`, but it never chooses the data folder and never waits. It is
/// `None` before the first choice. The panic hook uses it: a panic during the
/// choice must not start the choice again, because that can hang the app.
pub fn chosen_app_dir() -> Option<PathBuf> {
    let d = DATA.get()?.path.clone();
    let _ = std::fs::create_dir_all(&d);
    Some(d)
}

fn key(url: &str) -> String {
    // FNV-1a keeps the file name short and stable.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in url.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    let ext = url
        .rsplit('.')
        .next()
        .filter(|e| e.len() <= 4 && !e.contains('/'))
        .unwrap_or("bin");
    format!("{h:016x}.{ext}")
}

/// Agent for the web hosts. It starts no thread per request (see `net`). The user agent names
/// the public repo (`update::REPO`), as PRIVACY.md says.
pub fn web_agent() -> ureq::Agent {
    crate::net::agent(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent(user_agent())
            .build(),
    )
}

/// "Hexsight/0.1 (+https://github.com/<owner>/<name>)".
fn user_agent() -> String {
    format!("Hexsight/0.1 (+https://github.com/{})", crate::update::REPO)
}

/// Turn the forced refresh on or off. While it is on, `get` and `get_json`
/// download again, even when the cached file is young. When the download
/// fails, they still return the stale file.
pub fn force_refresh(on: bool) {
    FORCE.store(on, Ordering::Relaxed);
}

/// Delete the cache files that are older than `max_age`. Ignore errors.
pub fn cleanup(max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir()) else {
        return;
    };
    let now = SystemTime::now();
    for e in entries.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        let old = meta
            .modified()
            .is_ok_and(|t| now.duration_since(t).is_ok_and(|age| age > max_age));
        if meta.is_file() && old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Make sure `body` is the kind of file that `url` names. This keeps an HTML
/// error page with status 200 out of the cache. `json` forces the JSON check.
fn valid_body(url: &str, body: &[u8], json: bool) -> bool {
    let url = url.to_ascii_lowercase();
    if json || url.ends_with(".json") {
        serde_json::from_slice::<serde_json::Value>(body).is_ok()
    } else if url.ends_with(".png") {
        body.starts_with(b"\x89PNG\r\n\x1a\n")
    } else if url.ends_with(".jpg") || url.ends_with(".jpeg") {
        body.starts_with(&[0xFF, 0xD8])
    } else {
        // Any other file: not empty, and not an HTML page.
        body.first().is_some_and(|b| *b != b'<')
    }
}

/// Write to a temp file next to `path`, then rename it over `path`.
/// A cut write never leaves a broken file. Ignore errors.
fn write_atomic(path: &Path, bytes: &[u8]) {
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, bytes)
        .and_then(|()| std::fs::rename(&tmp, path))
        .is_err()
    {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Why a download gave no usable file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fail {
    /// The server answered with this HTTP error status, for example 404.
    Status(u16),
    /// No answer in time.
    Timeout,
    /// No connection: no network, an unknown host, or a closed port.
    Network,
    /// The answer stopped early, or it was too large.
    Body,
    /// The file came, but it is not the kind that the URL names (an HTML page,
    /// or JSON that does not parse).
    Invalid,
    /// Another error of the HTTP client (TLS, a redirect, a bad address).
    Other,
}

impl Fail {
    /// The kind of a ureq error. `in_body` is true for an error while the body is read.
    fn of(e: &ureq::Error, in_body: bool) -> Fail {
        match e {
            ureq::Error::StatusCode(code) => Fail::Status(*code),
            ureq::Error::Timeout(_) => Fail::Timeout,
            _ if in_body => Fail::Body,
            ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Io(_) => {
                Fail::Network
            }
            ureq::Error::BodyExceedsLimit(_) | ureq::Error::BodyStalled => Fail::Body,
            _ => Fail::Other,
        }
    }
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fail::Status(code) => write!(f, "status {code}"),
            Fail::Timeout => f.write_str("timeout"),
            Fail::Network => f.write_str("no connection"),
            Fail::Body => f.write_str("body cut or too large"),
            Fail::Invalid => f.write_str("not a valid file"),
            Fail::Other => f.write_str("client error"),
        }
    }
}

/// A failed download: why, and how many bytes came (0 when no body came).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failure {
    pub why: Fail,
    pub bytes: usize,
}

/// Download `url`. An error for a network error, an HTTP error status, or a
/// body that stops early.
fn download(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, Failure> {
    let failed = |in_body| {
        move |e: ureq::Error| Failure {
            why: Fail::of(&e, in_body),
            bytes: 0,
        }
    };
    let mut r = agent.get(url).call().map_err(failed(false))?;
    r.body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_vec()
        .map_err(failed(true))
}

/// What `get_in` found: the bytes (a fresh file, a download, or a stale file),
/// and why the download failed when one ran and failed. A stale file can
/// answer after a failed download, so both can be there.
#[derive(Debug)]
struct Fetched {
    bytes: Option<Vec<u8>>,
    failed: Option<Failure>,
}

/// Shared part of `get` and `get_json`. Cached bytes and downloaded bytes must
/// pass `valid`. A downloaded body that fails `valid` is not stored. Each failed
/// download goes to the data log, also when a stale file answers.
fn get_checked(
    agent: &ureq::Agent,
    url: &str,
    max_age: Duration,
    valid: impl Fn(&[u8]) -> bool,
) -> Result<Vec<u8>, Failure> {
    let got = get_in(
        &dir(),
        agent,
        url,
        max_age,
        FORCE.load(Ordering::Relaxed),
        valid,
    );
    if let Some(f) = &got.failed {
        log_failure(url, f);
    }
    match (got.bytes, got.failed) {
        (Some(bytes), _) => Ok(bytes),
        (None, Some(f)) => Err(f),
        // `get_in` always downloads when it has no file, so this does not happen.
        (None, None) => Err(Failure {
            why: Fail::Invalid,
            bytes: 0,
        }),
    }
}

/// Write a failed download to `hexsight.log` in the data folder. The tests
/// never write it: they must not touch the real data folder.
fn log_failure(url: &str, f: &Failure) {
    #[cfg(not(test))]
    crate::datalog::append(
        &app_dir(),
        &crate::datalog::line(crate::datalog::now_ms(), url, &f.why.to_string(), f.bytes),
    );
    #[cfg(test)]
    let _ = (url, f);
}

/// `get_checked` with the cache folder and the forced refresh as arguments.
/// The tests give a temp folder, so they never touch the real cache.
fn get_in(
    folder: &Path,
    agent: &ureq::Agent,
    url: &str,
    max_age: Duration,
    force: bool,
    valid: impl Fn(&[u8]) -> bool,
) -> Fetched {
    let path = folder.join(key(url));
    // A cached file counts only when it passes `valid`. This also replaces a
    // bad file that an older version stored.
    let cached = || std::fs::read(&path).ok().filter(|b| valid(b));
    let fresh = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .map(|t| SystemTime::now().duration_since(t).unwrap_or_default() < max_age)
        .unwrap_or(false);
    if fresh
        && !force
        && let Some(b) = cached()
    {
        return Fetched {
            bytes: Some(b),
            failed: None,
        };
    }
    let failed = match download(agent, url) {
        Ok(bytes) if valid(&bytes) => {
            write_atomic(&path, &bytes);
            return Fetched {
                bytes: Some(bytes),
                failed: None,
            };
        }
        Ok(bytes) => Failure {
            why: Fail::Invalid,
            bytes: bytes.len(),
        },
        Err(f) => f,
    };
    // The download failed. A valid stale file is better than nothing.
    Fetched {
        bytes: cached(),
        failed: Some(failed),
    }
}

/// Return cached bytes when younger than `max_age`. Else download and store.
/// A body that fails the `valid_body` check is not stored. It counts as a
/// failed download. After a failed download, return stale bytes when they
/// exist and pass the check.
pub fn get(agent: &ureq::Agent, url: &str, max_age: Duration) -> Option<Vec<u8>> {
    get_checked(agent, url, max_age, |b| valid_body(url, b, false)).ok()
}

/// Like `get`. The bytes must also parse as JSON.
pub fn get_json(agent: &ureq::Agent, url: &str, max_age: Duration) -> Option<serde_json::Value> {
    get_json_why(agent, url, max_age).ok()
}

/// Like `get_json`, with the reason when no file came.
pub fn get_json_why(
    agent: &ureq::Agent,
    url: &str,
    max_age: Duration,
) -> Result<serde_json::Value, Failure> {
    let bytes = get_checked(agent, url, max_age, |b| valid_body(url, b, true))?;
    serde_json::from_slice(&bytes).map_err(|_| {
        let f = Failure {
            why: Fail::Invalid,
            bytes: bytes.len(),
        };
        log_failure(url, &f);
        f
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, empty folder under the temp folder.
    fn temp_base(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("hexsight-test-cache-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_user_agent_names_the_public_repo() {
        // PRIVACY.md says that each request carries the address of the project.
        assert_eq!(
            user_agent(),
            format!("Hexsight/0.1 (+https://github.com/{})", crate::update::REPO)
        );
        assert!(!user_agent().contains("desktop helper"));
    }

    fn legacy_folder(base: &Path) -> PathBuf {
        let old = base.join(LEGACY_NAME);
        std::fs::create_dir_all(old.join("cache")).unwrap();
        std::fs::write(old.join("settings.json"), "{\"language\":\"tr\"}").unwrap();
        std::fs::write(old.join("history.json"), "[]").unwrap();
        std::fs::write(old.join("cache").join("a.json"), "{}").unwrap();
        old
    }

    #[test]
    fn the_legacy_folder_moves_to_the_new_name() {
        let base = temp_base("move");
        let old = legacy_folder(&base);
        assert_eq!(migrate_legacy_dir(&base), Migration::Moved);
        assert!(!old.exists());
        let new = base.join(APP_NAME);
        assert_eq!(
            std::fs::read_to_string(new.join("settings.json")).unwrap(),
            "{\"language\":\"tr\"}"
        );
        assert!(new.join("history.json").is_file() && new.join("cache").join("a.json").is_file());
        // The second start finds the new folder and does nothing.
        assert_eq!(migrate_legacy_dir(&base), Migration::None);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn an_existing_new_folder_or_no_legacy_folder_means_no_move() {
        let base = temp_base("none");
        assert_eq!(migrate_legacy_dir(&base), Migration::None);
        let old = legacy_folder(&base);
        // A new folder with a settings file is the real one: nothing moves, nothing is replaced.
        std::fs::create_dir_all(base.join(APP_NAME)).unwrap();
        std::fs::write(
            base.join(APP_NAME).join("settings.json"),
            "{\"language\":\"en\"}",
        )
        .unwrap();
        assert_eq!(migrate_legacy_dir(&base), Migration::None);
        assert!(old.join("settings.json").is_file());
        assert!(!base.join(APP_NAME).join("history.json").exists());
        assert_eq!(
            std::fs::read_to_string(base.join(APP_NAME).join("settings.json")).unwrap(),
            "{\"language\":\"en\"}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_new_folder_without_settings_and_history_gets_the_missing_files_only() {
        let base = temp_base("empty-new");
        let old = legacy_folder(&base);
        // The new folder exists with a cache file only (for example from a run before the first start).
        let new = base.join(APP_NAME);
        std::fs::create_dir_all(new.join("cache")).unwrap();
        std::fs::write(new.join("cache").join("a.json"), "{\"fresh\":true}").unwrap();
        assert_eq!(migrate_legacy_dir(&base), Migration::Copied);
        assert_eq!(
            std::fs::read_to_string(new.join("settings.json")).unwrap(),
            "{\"language\":\"tr\"}"
        );
        assert!(new.join("history.json").is_file());
        // The old folder stays, and a file of the new folder is not replaced.
        assert!(old.join("settings.json").is_file());
        assert_eq!(
            std::fs::read_to_string(new.join("cache").join("a.json")).unwrap(),
            "{\"fresh\":true}"
        );
        // The next start finds the settings and does nothing.
        assert_eq!(migrate_legacy_dir(&base), Migration::None);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(windows)]
    #[test]
    fn a_failed_rename_copies_the_files_and_keeps_the_old_folder() {
        use std::os::windows::fs::OpenOptionsExt;
        let base = temp_base("copy");
        let old = legacy_folder(&base);
        // An open handle with no sharing blocks the rename of the folder, as the old app would.
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(old.join("history.json"))
            .unwrap();
        assert_eq!(migrate_legacy_dir(&base), Migration::Copied);
        let new = base.join(APP_NAME);
        assert!(old.join("settings.json").is_file());
        assert_eq!(
            std::fs::read_to_string(new.join("settings.json")).unwrap(),
            "{\"language\":\"tr\"}"
        );
        assert!(new.join("cache").join("a.json").is_file());
        drop(lock);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ---- Portable mode.

    #[test]
    fn a_marker_and_a_writable_folder_put_the_data_next_to_the_exe() {
        let exe = Path::new(r"D:\Games\Hexsight");
        let (path, place) = choose_data_dir(
            Some(exe),
            true,
            |_| true,
            Some(Path::new(r"C:\Profile\AppData\Local")),
            Path::new(r"C:\Temp"),
        );
        assert_eq!(path, exe.join("Hexsight-data"));
        assert_eq!(place, Place::Portable);
        assert!(place.portable());
    }

    #[test]
    fn a_marker_and_a_folder_that_is_not_writable_fall_back_to_localappdata() {
        let local = Path::new(r"C:\Profile\AppData\Local");
        let mut checked = None;
        let writable = |d: &Path| {
            checked = Some(d.to_path_buf());
            false
        };
        let (path, place) = choose_data_dir(
            Some(Path::new(r"C:\Program Files\Hexsight")),
            true,
            writable,
            Some(local),
            Path::new(r"C:\Temp"),
        );
        assert_eq!(path, local.join("Hexsight"));
        assert_eq!(place, Place::PortableFallback);
        assert!(place.portable());
        // The check ran on the portable folder, not on the exe folder.
        assert_eq!(
            checked,
            Some(PathBuf::from(r"C:\Program Files\Hexsight\Hexsight-data"))
        );
    }

    #[test]
    fn no_marker_keeps_the_data_in_localappdata_and_checks_nothing() {
        let local = Path::new(r"C:\Profile\AppData\Local");
        let (path, place) = choose_data_dir(
            Some(Path::new(r"D:\Games\Hexsight")),
            false,
            |_| panic!("no check without the marker"),
            Some(local),
            Path::new(r"C:\Temp"),
        );
        assert_eq!(path, local.join("Hexsight"));
        assert_eq!(place, Place::Local);
        assert!(!place.portable());
        // No exe folder (current_exe failed) is the same as no marker.
        assert_eq!(
            choose_data_dir(None, false, |_| true, Some(local), Path::new(r"C:\Temp")),
            (local.join("Hexsight"), Place::Local)
        );
    }

    #[test]
    fn no_localappdata_means_the_temp_folder_unless_the_portable_folder_works() {
        let temp = Path::new(r"C:\Temp");
        let exe = Path::new(r"E:\Hexsight");
        assert_eq!(
            choose_data_dir(Some(exe), false, |_| true, None, temp),
            (temp.join("Hexsight"), Place::Local)
        );
        assert_eq!(
            choose_data_dir(Some(exe), true, |_| true, None, temp),
            (exe.join("Hexsight-data"), Place::Portable)
        );
        assert_eq!(
            choose_data_dir(Some(exe), true, |_| false, None, temp),
            (temp.join("Hexsight"), Place::PortableFallback)
        );
    }

    #[test]
    fn a_new_folder_is_writable_and_a_folder_under_a_file_is_not() {
        let base = temp_base("write");
        let d = base.join("stick").join("Hexsight-data");
        assert!(can_write(&d));
        assert!(d.is_dir());
        // The check leaves no file behind.
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0);
        // A file with the name of a folder in the path blocks the folder.
        std::fs::write(base.join("file"), "x").unwrap();
        assert!(!can_write(&base.join("file").join("Hexsight-data")));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn an_old_probe_file_or_folder_does_not_block_the_check() {
        let base = temp_base("old-probe");
        let d = base.join("Hexsight-data");
        // A probe folder and a read-only probe file from an older version.
        std::fs::create_dir_all(d.join(".write-test")).unwrap();
        let old = d.join(".write-test.old");
        std::fs::write(&old, "x").unwrap();
        let writable = std::fs::metadata(&old).unwrap().permissions();
        let mut read_only = writable.clone();
        read_only.set_readonly(true);
        std::fs::set_permissions(&old, read_only).unwrap();
        assert!(can_write(&d));
        // The check leaves only the old entries.
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 2);
        let _ = std::fs::set_permissions(&old, writable);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_panic_hook_gets_the_chosen_folder_without_a_new_choice() {
        // After the choice, the hook gets the same folder as everyone else.
        let _ = place();
        assert_eq!(chosen_app_dir(), Some(app_dir()));
    }

    #[test]
    fn the_test_exe_in_the_target_folder_has_no_marker() {
        // A debug or release exe in the cargo target folder runs in the normal mode,
        // so the test hooks that set LOCALAPPDATA keep working.
        let exe = std::env::current_exe().unwrap();
        assert!(!exe.parent().unwrap().join(PORTABLE_MARKER).exists());
        assert_eq!(place(), Place::Local);
    }

    const HTML: &[u8] = b"<!DOCTYPE html><html><body>Login</body></html>";
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    #[test]
    fn json_url_needs_a_json_body() {
        let url = "https://example.test/v1/items.json";
        assert!(valid_body(url, br#"[{"id":1}]"#, false));
        assert!(!valid_body(url, HTML, false));
        assert!(!valid_body(url, b"", false));
        // A download that stopped early is not JSON.
        assert!(!valid_body(url, br#"[{"id":1},"#, false));
    }

    #[test]
    fn json_flag_forces_the_json_check() {
        let url = "https://example.test/mayham/16.20/augments_tr_tr";
        assert!(valid_body(url, br#"{"1":{}}"#, true));
        assert!(!valid_body(url, HTML, true));
        // Without the flag, this URL names a plain file.
        assert!(valid_body(url, br#"{"1":{}}"#, false));
        assert!(!valid_body(url, HTML, false));
    }

    #[test]
    fn png_url_needs_the_png_signature() {
        let url = "https://example.test/icons/a.png";
        assert!(valid_body(url, PNG, false));
        assert!(!valid_body(url, HTML, false));
        assert!(!valid_body(url, b"\x89PNG", false));
        assert!(!valid_body(url, b"", false));
    }

    #[test]
    fn jpeg_url_needs_the_jpeg_signature() {
        for url in [
            "https://example.test/a.jpg",
            "https://example.test/a.jpeg",
            "https://example.test/A.JPG",
        ] {
            assert!(valid_body(url, &[0xFF, 0xD8, 0xFF, 0xE0], false));
            assert!(!valid_body(url, HTML, false));
            assert!(!valid_body(url, PNG, false));
            assert!(!valid_body(url, b"", false));
        }
    }

    #[test]
    fn other_url_needs_a_body_that_is_not_html() {
        let url = "https://example.test/data.bin";
        assert!(valid_body(url, b"abc", false));
        assert!(!valid_body(url, b"", false));
        assert!(!valid_body(url, HTML, false));
    }

    // ---- Downloads and the cache folder. A temp folder stands in for the real one.

    const DETAILS: &[u8] =
        br#"{"augments":{"all":[{"id":3036,"tier":"S","sampleCount":900,"winRate":0.6}]}}"#;

    /// The URL of a champion file on a local port that nothing listens on: the network is down.
    fn offline_url() -> String {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        format!("http://127.0.0.1:{port}/stats/all/champion-details/74.json")
    }

    /// A local server that answers one request with `body` and status 200.
    fn serve_once(body: &'static [u8]) -> String {
        serve_status(200, body)
    }

    /// A local server that answers one request with `body` and this HTTP status.
    fn serve_status(status: u16, body: &'static [u8]) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let Ok((mut s, _)) = listener.accept() else {
                return;
            };
            let mut req = Vec::new();
            let mut buf = [0u8; 1024];
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => req.extend_from_slice(&buf[..n]),
                }
            }
            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(head.as_bytes());
            let _ = s.write_all(body);
        });
        format!("http://127.0.0.1:{port}/stats/all/champion-details/74.json")
    }

    fn json_ok(url: &str) -> impl Fn(&[u8]) -> bool + '_ {
        move |b| valid_body(url, b, true)
    }

    /// Make the cached file of `url` older than the stats age (6 hours).
    fn make_stale(folder: &Path, url: &str) {
        let f = std::fs::File::options()
            .write(true)
            .open(folder.join(key(url)))
            .unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(7 * 3600))
            .unwrap();
    }

    const STATS_AGE: Duration = Duration::from_secs(6 * 3600);

    #[test]
    fn a_fresh_valid_file_answers_without_the_network_also_when_forced() {
        let folder = temp_base("fresh-offline");
        let url = offline_url();
        std::fs::write(folder.join(key(&url)), DETAILS).unwrap();
        let agent = web_agent();
        assert_eq!(
            get_in(&folder, &agent, &url, STATS_AGE, false, json_ok(&url))
                .bytes
                .as_deref(),
            Some(DETAILS)
        );
        // "Download again" while offline: the download fails, and the cached file still answers.
        assert_eq!(
            get_in(&folder, &agent, &url, STATS_AGE, true, json_ok(&url))
                .bytes
                .as_deref(),
            Some(DETAILS)
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_stale_file_answers_when_the_network_is_down() {
        let folder = temp_base("stale-offline");
        let url = offline_url();
        std::fs::write(folder.join(key(&url)), DETAILS).unwrap();
        make_stale(&folder, &url);
        assert_eq!(
            get_in(&folder, &web_agent(), &url, STATS_AGE, false, json_ok(&url))
                .bytes
                .as_deref(),
            Some(DETAILS)
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn no_file_and_no_network_give_nothing() {
        // The first look at a champion in a new patch while the network is down.
        // `fetch_loop` then marks the champion for a new try, and the window says it tries again.
        let folder = temp_base("missing-offline");
        let url = offline_url();
        let got = get_in(&folder, &web_agent(), &url, STATS_AGE, false, json_ok(&url));
        assert_eq!(got.bytes, None);
        assert_eq!(
            got.failed,
            Some(Failure {
                why: Fail::Network,
                bytes: 0
            })
        );
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_download_fills_the_cache_and_the_next_call_needs_no_network() {
        let folder = temp_base("download");
        let url = serve_once(DETAILS);
        let agent = web_agent();
        assert_eq!(
            get_in(&folder, &agent, &url, STATS_AGE, false, json_ok(&url))
                .bytes
                .as_deref(),
            Some(DETAILS)
        );
        // The server answered once and is gone. The fresh file answers now.
        assert_eq!(
            get_in(&folder, &agent, &url, STATS_AGE, false, json_ok(&url))
                .bytes
                .as_deref(),
            Some(DETAILS)
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[cfg(windows)]
    #[test]
    fn a_locked_fresh_file_and_no_network_give_nothing() {
        use std::os::windows::fs::OpenOptionsExt;
        // Another program (a virus scan, a script) holds the file with no sharing.
        // The valid, fresh file then gives nothing, as a missing file would.
        let folder = temp_base("locked-offline");
        let url = offline_url();
        let path = folder.join(key(&url));
        std::fs::write(&path, DETAILS).unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert_eq!(
            get_in(&folder, &web_agent(), &url, STATS_AGE, false, json_ok(&url)).bytes,
            None
        );
        drop(lock);
        assert_eq!(
            get_in(&folder, &web_agent(), &url, STATS_AGE, false, json_ok(&url))
                .bytes
                .as_deref(),
            Some(DETAILS)
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[cfg(windows)]
    #[test]
    fn a_blocked_rename_keeps_the_old_file_but_returns_the_download() {
        use std::os::windows::fs::OpenOptionsExt;
        // A second writer or reader holds the cached file without delete sharing.
        // The new download still answers, but the file on disk keeps the old bytes
        // and the old time. The time of a cache file is no proof of what the app got.
        const OLD: &[u8] = br#"{"augments":{"all":[]}}"#;
        let folder = temp_base("rename-blocked");
        let url = serve_once(DETAILS);
        let path = folder.join(key(&url));
        std::fs::write(&path, OLD).unwrap();
        make_stale(&folder, &url);
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        let reader = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        assert_eq!(
            get_in(&folder, &web_agent(), &url, STATS_AGE, false, json_ok(&url))
                .bytes
                .as_deref(),
            Some(DETAILS)
        );
        drop(reader);
        assert_eq!(std::fs::read(&path).unwrap(), OLD);
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before
        );
        // No temp file stays behind.
        assert!(!path.with_extension("tmp").exists());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_stale_file_answers_and_the_failed_download_is_still_reported() {
        // The data log gets a line also when the old file covers the failure.
        let folder = temp_base("stale-reported");
        let url = offline_url();
        std::fs::write(folder.join(key(&url)), DETAILS).unwrap();
        make_stale(&folder, &url);
        let got = get_in(&folder, &web_agent(), &url, STATS_AGE, false, json_ok(&url));
        assert_eq!(got.bytes.as_deref(), Some(DETAILS));
        assert_eq!(got.failed.map(|f| f.why), Some(Fail::Network));
        // A fresh file needs no download, so nothing failed.
        std::fs::write(folder.join(key(&url)), DETAILS).unwrap();
        assert_eq!(
            get_in(&folder, &web_agent(), &url, STATS_AGE, false, json_ok(&url)).failed,
            None
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_missing_file_on_the_server_is_status_404() {
        let folder = temp_base("status-404");
        let url = serve_status(404, b"{\"error\":\"not found\"}");
        let got = get_in(&folder, &web_agent(), &url, STATS_AGE, false, json_ok(&url));
        assert_eq!(got.bytes, None);
        assert_eq!(
            got.failed,
            Some(Failure {
                why: Fail::Status(404),
                bytes: 0
            })
        );
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn an_html_page_with_status_200_is_an_invalid_file_with_its_size() {
        let folder = temp_base("invalid-body");
        let url = serve_once(HTML);
        let got = get_in(&folder, &web_agent(), &url, STATS_AGE, false, json_ok(&url));
        assert_eq!(got.bytes, None);
        assert_eq!(
            got.failed,
            Some(Failure {
                why: Fail::Invalid,
                bytes: HTML.len()
            })
        );
        // The page is not stored.
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn each_reason_has_a_short_text_for_the_log() {
        let texts: Vec<String> = [
            Fail::Status(503),
            Fail::Timeout,
            Fail::Network,
            Fail::Body,
            Fail::Invalid,
            Fail::Other,
        ]
        .iter()
        .map(|f| f.to_string())
        .collect();
        assert_eq!(
            texts,
            [
                "status 503",
                "timeout",
                "no connection",
                "body cut or too large",
                "not a valid file",
                "client error"
            ]
        );
    }
}
