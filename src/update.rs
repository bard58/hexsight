//! The update note and the feedback link. Both use the public GitHub repo.
//!
//! Hexsight asks the GitHub releases API for the latest tag at most once a day,
//! compares it with its own version, and shows a button when a newer one exists.
//! The button downloads the new exe only when the player clicks it (see the one-click
//! update below). The feedback link opens a new issue
//! page in the browser with the version, the Windows version and the main settings
//! in the text. The player reads and sends it, or closes the tab.

use crate::lcu::Phase;
use serde_json::Value;

/// The public repo, as "owner/name". The source folder holds the placeholder
/// OWNER, and the export for GitHub (`launch\\github\\export.ps1`) puts the real
/// owner in. While the placeholder stays, Hexsight sends no update request.
pub const REPO: &str = "bard58/hexsight";

/// How long Hexsight waits between two update requests.
pub const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

/// The version of this build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// True when `repo` is a real "owner/name" and not the placeholder.
pub fn repo_ready(repo: &str) -> bool {
    match repo.split_once('/') {
        Some((owner, name)) => {
            !owner.is_empty() && !name.is_empty() && owner != "OWNER" && !name.contains('/')
        }
        None => false,
    }
}

/// The URL of the latest release in the API. It is a plain GET with no login.
pub fn api_url(repo: &str) -> String {
    format!("https://api.github.com/repos/{repo}/releases/latest")
}

/// The page where the player gets the new version.
pub fn releases_url(repo: &str) -> String {
    format!("https://github.com/{repo}/releases/latest")
}

/// The tag of a release in the answer of the API, for example "v0.2.0". `None`
/// for a draft, a pre-release, or a tag with odd characters: the tag shows in the
/// window, so only the letters of a version pass.
pub fn latest_tag(release: &Value) -> Option<String> {
    if release["draft"].as_bool() == Some(true) || release["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let tag = release["tag_name"].as_str()?.trim();
    let plain = !tag.is_empty()
        && tag.len() <= 32
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'));
    plain.then(|| tag.to_string())
}

/// A version as numbers, and whether it has a suffix ("0.2.0-beta" is a pre-release).
fn parse_version(v: &str) -> Option<([u64; 3], bool)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let (core, pre) = match v.split_once(['-', '+']) {
        Some((core, _)) => (core, true),
        None => (v, false),
    };
    let mut out = [0u64; 3];
    let mut parts = core.split('.');
    for slot in &mut out {
        match parts.next() {
            Some(p) => *slot = p.parse().ok()?,
            None => break,
        }
    }
    // More than three parts, or an empty text, is no version we know.
    (parts.next().is_none() && !core.is_empty()).then_some((out, pre))
}

/// True when `latest` is a newer version than `current`. Text that is no version
/// is never newer. A pre-release is older than the same version without a suffix.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some((a, a_pre)), Some((b, b_pre))) => (a, !a_pre) > (b, !b_pre),
        _ => false,
    }
}

/// The newer tag of an API answer, or `None` when this build is current or newer.
pub fn newer_tag(release: &Value, current: &str) -> Option<String> {
    latest_tag(release).filter(|t| is_newer(t, current))
}

/// Windows as a short text for the feedback: "Windows 11 (build 26200)". The
/// builds from 22000 on are Windows 11. 0 means that the build is unknown.
pub fn windows_label(build: u32) -> String {
    match build {
        0 => "Windows (unknown build)".to_string(),
        b if b >= 22000 => format!("Windows 11 (build {b})"),
        b => format!("Windows 10 or older (build {b})"),
    }
}

/// What the feedback page gets: no Riot ID, no path, no name, no game data.
pub struct Feedback<'a> {
    pub version: &'a str,
    pub windows: String,
    /// The main settings as "name: value" pairs, in English.
    pub settings: Vec<(&'static str, String)>,
}

/// The text of the new issue, in Markdown. The player adds the rest below.
pub fn issue_body(f: &Feedback) -> String {
    let mut body = format!(
        "**Hexsight version:** {}\n**Windows:** {}\n\n**Settings:**\n",
        f.version, f.windows
    );
    for (name, value) in &f.settings {
        body.push_str(&format!("- {name}: {value}\n"));
    }
    body.push_str("\n**What happened?**\n\n");
    body
}

/// Percent-encode a text for a query value. Letters, digits and `-_.~` stay.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The address of the new issue page with the title and the text filled in.
pub fn issue_url(repo: &str, f: &Feedback) -> String {
    format!(
        "https://github.com/{repo}/issues/new?title={}&body={}",
        encode("Feedback"),
        encode(&issue_body(f))
    )
}

// ---------------------------------------------------------------------------
// One-click update. The pure parts live here: the choice of the file, the check of
// the URLs, the hash, and the swap of the exe. `worker.rs` does the network part.

/// The biggest exe that Hexsight accepts. The release exe has about 6 MB.
pub const MAX_EXE_BYTES: u64 = 30_000_000;

/// The smallest size that an exe in a release can have.
pub const MIN_EXE_BYTES: u64 = 1_000;

/// The biggest `.sha256` file that Hexsight reads.
pub const MAX_SHA_BYTES: u64 = 512;

/// The most redirects that one download follows.
pub const MAX_HOPS: usize = 5;

/// The hosts that a redirect of a download may reach. GitHub serves the files of a
/// release from `release-assets.githubusercontent.com`, and from
/// `objects.githubusercontent.com` before that.
const REDIRECT_HOSTS: [&str; 3] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// The launch argument of the new exe: the id of the old process, which it waits for.
pub const UPDATED_FROM: &str = "--updated-from";

/// The file of a release that fits this PC.
#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// The `.sha256` file of the asset.
    pub sha_url: String,
}

/// The name of the exe in the release of this tag. The portable zip holds the same
/// exe (renamed `Hexsight.exe`), so both kinds of copy use this file.
pub fn asset_name(tag: &str) -> String {
    format!("hexsight-{tag}-windows-x64.exe")
}

/// The lower case host and the rest (path and query) of an https URL. `None` for any
/// other scheme, a user name, a port other than 443, or an odd character. This is a
/// strict check, not a full URL parser.
fn split_https(url: &str) -> Option<(String, &str)> {
    let rest = url.strip_prefix("https://")?;
    if url.chars().any(|c| c.is_control() || c == ' ' || c == '\\') {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let host = authority.strip_suffix(":443").unwrap_or(authority);
    let host_ok = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
    host_ok.then(|| (host.to_ascii_lowercase(), tail))
}

/// True when the URL is the first address of a download: https, host `github.com`,
/// and a path in the releases of `repo`.
pub fn download_url_ok(url: &str, repo: &str) -> bool {
    let Some((host, tail)) = split_https(url) else {
        return false;
    };
    if host != "github.com" || !repo_ready(repo) {
        return false;
    }
    let path = tail.split(['?', '#']).next().unwrap_or("");
    let want = format!("/{}/releases/download/", repo.to_ascii_lowercase());
    path.to_ascii_lowercase().starts_with(&want) && !path.split('/').any(|p| p == ".." || p == ".")
}

/// True when a redirect may go to this URL: https, and one of the GitHub hosts.
pub fn redirect_ok(url: &str) -> bool {
    split_https(url).is_some_and(|(host, _)| REDIRECT_HOSTS.contains(&host.as_str()))
}

/// The address that a `Location` header means, seen from `base`. Only a full https
/// address, a `//host/path` address, and a `/path` address have a meaning.
pub fn resolve_location(base: &str, location: &str) -> Option<String> {
    let location = location.trim();
    if location.starts_with("https://") {
        Some(location.to_string())
    } else if location.starts_with("//") {
        Some(format!("https:{location}"))
    } else if location.starts_with('/') {
        let rest = base.strip_prefix("https://")?;
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        Some(format!("https://{}{location}", &rest[..end]))
    } else {
        None
    }
}

/// The exe and its `.sha256` file in the answer of the API. The release has to be a
/// plain one of this `tag`, both files have to be uploaded, and their sizes and
/// URLs have to pass the checks. No other file name is taken.
pub fn pick_asset(release: &Value, tag: &str, repo: &str) -> Option<Asset> {
    if latest_tag(release).as_deref() != Some(tag) {
        return None;
    }
    let name = asset_name(tag);
    let sha_name = format!("{name}.sha256");
    let find = |wanted: &str| {
        release["assets"]
            .as_array()?
            .iter()
            .find(|a| a["name"].as_str() == Some(wanted))
            .filter(|a| a["state"].as_str() == Some("uploaded"))
    };
    let exe = find(&name)?;
    let sha = find(&sha_name)?;
    let size = exe["size"].as_u64()?;
    if !(MIN_EXE_BYTES..=MAX_EXE_BYTES).contains(&size) || sha["size"].as_u64()? > MAX_SHA_BYTES {
        return None;
    }
    let url = exe["browser_download_url"].as_str()?;
    let sha_url = sha["browser_download_url"].as_str()?;
    (download_url_ok(url, repo) && download_url_ok(sha_url, repo)).then(|| Asset {
        name,
        url: url.to_string(),
        size,
        sha_url: sha_url.to_string(),
    })
}

/// The hash in a `.sha256` file: `<64 hex characters>  <file name>`. The name is
/// optional, but when it is there it has to be `file_name`. The zero hash is no hash.
pub fn parse_sha256(text: &str, file_name: &str) -> Option<[u8; 32]> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    let (hex, name) = match line.split_once(char::is_whitespace) {
        Some((hex, name)) => (hex, Some(name.trim_start().trim_start_matches('*'))),
        None => (line, None),
    };
    if hex.len() != 64 || name.is_some_and(|n| n != file_name) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    (out != [0u8; 32]).then_some(out)
}

/// Why an update did not happen. The window shows the text of the error.
#[derive(Clone, Debug, PartialEq)]
pub enum UpdateError {
    /// A request or a read failed, or a URL was not allowed.
    Net,
    /// The release has no usable file for this PC.
    Asset,
    /// The file is bigger than the cap.
    TooBig,
    /// The hash of the file is not the one in the `.sha256` file.
    Hash,
    /// The file is no Windows program.
    NotExe,
    /// The new file could not be written.
    Disk,
    /// The folder of the exe is read-only for Hexsight.
    NotWritable,
    /// A game or a champ select runs.
    Busy,
    /// The old copy of an earlier update is still in use.
    OldLocked,
    /// The exe could not be renamed. Nothing changed.
    Swap,
    /// The new exe did not start, and the old one is back.
    Restart,
    /// The old exe could not be put back. It waits under this path.
    RollbackFailed(String),
    /// The setting "Check for new versions" is off. No request is sent.
    Disabled,
    /// The release is not newer than this build (any more). There is nothing to install.
    UpToDate,
}

impl UpdateError {
    /// How long the window shows this failure before the button comes back. `None` means
    /// that it stays: a folder that is read-only does not change, and the player has to
    /// read where the old exe is.
    pub fn show_for(&self) -> Option<std::time::Duration> {
        match self {
            UpdateError::NotWritable | UpdateError::RollbackFailed(_) => None,
            UpdateError::Busy => Some(std::time::Duration::from_secs(20)),
            _ => Some(std::time::Duration::from_secs(300)),
        }
    }
}

/// The size of the download against the size in the release. A file that is too short is
/// a cut connection (a network problem). A file that is too long cannot be the release file.
pub fn check_size(got: u64, want: u64) -> Result<(), UpdateError> {
    match got.cmp(&want) {
        std::cmp::Ordering::Equal => Ok(()),
        std::cmp::Ordering::Less => Err(UpdateError::Net),
        std::cmp::Ordering::Greater => Err(UpdateError::Hash),
    }
}

/// What the window shows for the update.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum UpdateState {
    #[default]
    Idle,
    Downloading {
        done: u64,
        total: u64,
    },
    Verifying,
    Installing,
    Failed(UpdateError),
}

impl UpdateState {
    /// The download, the check or the swap runs, so a second click does nothing.
    pub fn busy(&self) -> bool {
        matches!(
            self,
            UpdateState::Downloading { .. } | UpdateState::Verifying | UpdateState::Installing
        )
    }
}

/// The percent of a download, from 0 to 100. 0 while the total is unknown.
pub fn percent(done: u64, total: u64) -> u32 {
    (done.min(total) * 100).checked_div(total).unwrap_or(0) as u32
}

/// Why the update waits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Blocked {
    ChampSelect,
    Game,
}

/// Hexsight never replaces itself in a champ select or in a game. The phase, the game
/// window and the level come from the poll thread. `live_port_open` is the result of
/// a look at port 2999 of the Live Client.
pub fn update_allowed(
    phase: Phase,
    game_window: Option<bool>,
    live_level: i64,
    live_port_open: bool,
) -> Result<(), Blocked> {
    match phase {
        Phase::ChampSelect => Err(Blocked::ChampSelect),
        Phase::InGame => Err(Blocked::Game),
        _ if game_window == Some(true) || live_level > 0 || live_port_open => Err(Blocked::Game),
        _ => Ok(()),
    }
}

/// True when something listens on port 2999 of this PC, the port of the Live Client of
/// a running game. The connect has a short timeout and starts no thread.
pub fn live_port_open() -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 2999));
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(200)).is_ok()
}

/// Reads `source` into `sink` in chunks of 64 KB, and gives back the number of bytes and
/// the SHA-256 hash. It stops with `TooBig` as soon as more than `cap` bytes arrive.
/// `progress` gets the number of bytes so far after each chunk.
pub fn copy_hashed<R: std::io::Read, W: std::io::Write>(
    mut source: R,
    mut sink: W,
    cap: u64,
    mut progress: impl FnMut(u64),
) -> Result<(u64, [u8; 32]), UpdateError> {
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = match source.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(UpdateError::Net),
        };
        total += n as u64;
        if total > cap {
            return Err(UpdateError::TooBig);
        }
        ctx.update(&buf[..n]);
        sink.write_all(&buf[..n]).map_err(|_| UpdateError::Disk)?;
        progress(total);
    }
    sink.flush().map_err(|_| UpdateError::Disk)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(ctx.finish().as_ref());
    Ok((total, out))
}

/// True when the file starts with the two letters of a Windows program ("MZ").
pub fn looks_like_exe(path: &std::path::Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 2];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && &head == b"MZ"
}

/// `path` with `suffix` added to the file name: `Hexsight.exe` -> `Hexsight.exe.new`.
fn with_suffix(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    name.into()
}

/// Where the download waits before the swap.
pub fn new_path(exe: &std::path::Path) -> std::path::PathBuf {
    with_suffix(exe, ".new")
}

/// Where the running exe waits after the swap.
pub fn old_path(exe: &std::path::Path) -> std::path::PathBuf {
    with_suffix(exe, ".old")
}

/// How long a copy must run before it deletes the old exe of an update. Until then, the
/// old exe is the only copy that is known to work.
pub const HEALTHY_RUN: std::time::Duration = std::time::Duration::from_secs(60);

/// Delete a half download of an earlier update. It is of no use. Errors are ignored.
pub fn cleanup_download(exe: &std::path::Path) {
    let _ = std::fs::remove_file(new_path(exe));
}

/// Delete the old exe of an earlier update. Call it only after the new copy has run well
/// for `HEALTHY_RUN`. Errors are ignored: a file that is still in use goes at a later start.
pub fn cleanup_old(exe: &std::path::Path) {
    let _ = std::fs::remove_file(old_path(exe));
}

/// True when Hexsight can create a file in this folder. The test file goes away at once.
pub fn folder_writable(dir: &std::path::Path) -> bool {
    let probe = dir.join(format!(".hexsight-write-test-{}", std::process::id()));
    let made = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    let _ = std::fs::remove_file(&probe);
    made
}

/// How often a rename is tried. A virus scanner often holds a fresh file open for a moment.
const RENAME_TRIES: u32 = 5;

/// The rename with a few more tries and a short pause between them.
fn retry_rename(
    rename: &dyn Fn(&std::path::Path, &std::path::Path) -> std::io::Result<()>,
    from: &std::path::Path,
    to: &std::path::Path,
    tries: u32,
    pause: std::time::Duration,
) -> std::io::Result<()> {
    let mut last = Ok(());
    for attempt in 0..tries.max(1) {
        if attempt > 0 {
            std::thread::sleep(pause);
        }
        last = rename(from, to);
        if last.is_ok() {
            break;
        }
    }
    last
}

/// The swap with a rename function, so a test can make one step fail. Each rename gets
/// a few tries with `pause` between them.
fn swap_with(
    exe: &std::path::Path,
    new: &std::path::Path,
    rename: &dyn Fn(&std::path::Path, &std::path::Path) -> std::io::Result<()>,
    pause: std::time::Duration,
) -> Result<(), UpdateError> {
    let old = old_path(exe);
    let filled = std::fs::metadata(new).is_ok_and(|m| m.is_file() && m.len() > 0);
    if !filled {
        return Err(UpdateError::Disk);
    }
    // A copy from an earlier update goes first. When it stays, nothing changes.
    if old.exists() && std::fs::remove_file(&old).is_err() {
        let _ = std::fs::remove_file(new);
        return Err(UpdateError::OldLocked);
    }
    // Windows lets a running exe change its name.
    if retry_rename(rename, exe, &old, RENAME_TRIES, pause).is_err() {
        let _ = std::fs::remove_file(new);
        return Err(UpdateError::Swap);
    }
    if retry_rename(rename, new, exe, RENAME_TRIES, pause).is_err() {
        // Without the exe name the next start finds nothing, so this one tries harder.
        let back = retry_rename(rename, &old, exe, RENAME_TRIES * 2, pause);
        let _ = std::fs::remove_file(new);
        return match back {
            Ok(()) => Err(UpdateError::Swap),
            Err(_) => Err(UpdateError::RollbackFailed(old.display().to_string())),
        };
    }
    Ok(())
}

/// Put the new file in the place of the exe. The old exe waits as `<exe>.old`. When
/// a step fails, the old exe has its name again and the error says what failed.
pub fn swap_in(exe: &std::path::Path, new: &std::path::Path) -> Result<(), UpdateError> {
    swap_with(
        exe,
        new,
        &|from, to| std::fs::rename(from, to),
        std::time::Duration::from_millis(200),
    )
}

/// Take the old exe back after the new one did not start.
pub fn undo_swap(exe: &std::path::Path) -> Result<(), UpdateError> {
    let old = old_path(exe);
    retry_rename(
        &|from, to| std::fs::rename(from, to),
        &old,
        exe,
        RENAME_TRIES * 2,
        std::time::Duration::from_millis(200),
    )
    .map_err(|_| UpdateError::RollbackFailed(old.display().to_string()))
}

/// Start `exe` and tell it which process to wait for.
pub fn spawn_new(exe: &std::path::Path, old_pid: u32) -> std::io::Result<std::process::Child> {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg(UPDATED_FROM).arg(old_pid.to_string());
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn()
}

/// What a look at the new copy shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StartProbe {
    /// The process has ended.
    Dead,
    /// The process runs, but it has not said that it is up.
    Silent,
    /// The process runs and has said that it is up.
    Up,
}

/// Watch the new copy after its start. True when it says that it is up and still runs
/// `grace` later, or when it runs without a word until `timeout` (a slow start). False
/// as soon as it has ended: a scanner killed it, or it crashed at once. `probe` looks
/// once, `sleep` waits one step.
pub fn wait_for_start(
    mut probe: impl FnMut() -> StartProbe,
    mut sleep: impl FnMut(std::time::Duration),
    step: std::time::Duration,
    grace: std::time::Duration,
    timeout: std::time::Duration,
) -> bool {
    let mut waited = std::time::Duration::ZERO;
    let mut up_since = None;
    while waited < timeout {
        match probe() {
            StartProbe::Dead => return false,
            StartProbe::Up => {
                let since = *up_since.get_or_insert(waited);
                if waited.saturating_sub(since) >= grace {
                    return true;
                }
            }
            StartProbe::Silent => {}
        }
        sleep(step);
        waited += step;
    }
    probe() != StartProbe::Dead
}

/// Swap the files, start the new exe with `spawn`, and let `confirm` watch that it really
/// runs. When the start fails, or `confirm` says no, the old exe comes back. Only a
/// start that worked lets the caller end this process.
pub fn install_and_restart<T>(
    exe: &std::path::Path,
    new: &std::path::Path,
    spawn: impl FnOnce(&std::path::Path) -> std::io::Result<T>,
    confirm: impl FnOnce(T) -> bool,
) -> Result<(), UpdateError> {
    swap_in(exe, new)?;
    let started = spawn(exe).ok().is_some_and(confirm);
    if !started {
        undo_swap(exe)?;
        return Err(UpdateError::Restart);
    }
    Ok(())
}

/// The process id after `--updated-from`. `None` when the argument is missing, has no
/// value, or the value is no number above 0.
pub fn parse_updated_from(args: impl IntoIterator<Item = String>) -> Option<u32> {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == UPDATED_FROM {
            return args.next()?.parse().ok().filter(|&p| p > 0);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_newer_version_is_found_by_its_numbers() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        // 10 is more than 9: the parts compare as numbers, not as text.
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("v1.2", "1.1.9"));
        // The same or an older version is not newer.
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("0.9.0", "0.10.0"));
    }

    #[test]
    fn a_pre_release_is_older_than_its_release_and_text_is_never_newer() {
        assert!(!is_newer("0.2.0-beta", "0.2.0"));
        assert!(is_newer("0.2.0", "0.2.0-beta"));
        assert!(is_newer("0.3.0-rc1", "0.2.0"));
        for bad in [
            "",
            "latest",
            "v",
            "1.2.3.4",
            "a.b.c",
            "1..2",
            "nightly-2026",
        ] {
            assert!(!is_newer(bad, "0.1.0"), "{bad}");
            assert!(!is_newer("9.9.9", bad), "{bad}");
        }
    }

    #[test]
    fn only_a_plain_release_tag_shows() {
        let tag = |v: Value| latest_tag(&v);
        assert_eq!(
            tag(json!({"tag_name": "v0.2.0"})),
            Some("v0.2.0".to_string())
        );
        assert_eq!(
            tag(json!({"tag_name": " v0.2.0 ", "draft": false, "prerelease": false})),
            Some("v0.2.0".to_string())
        );
        // A draft or a pre-release is not an update.
        assert_eq!(tag(json!({"tag_name": "v0.2.0", "prerelease": true})), None);
        assert_eq!(tag(json!({"tag_name": "v0.2.0", "draft": true})), None);
        // The tag shows in the window, so a tag with other letters is dropped.
        assert_eq!(tag(json!({"tag_name": "v1 <b>new</b>"})), None);
        assert_eq!(tag(json!({"tag_name": "x".repeat(40)})), None);
        assert_eq!(tag(json!({"tag_name": ""})), None);
        assert_eq!(tag(json!({"name": "v0.2.0"})), None);
        assert_eq!(tag(json!({"message": "Not Found"})), None);
        assert_eq!(tag(Value::Null), None);
    }

    #[test]
    fn the_note_shows_only_for_a_newer_release() {
        let release = json!({"tag_name": "v0.2.0", "html_url": "https://example.com/x"});
        assert_eq!(newer_tag(&release, "0.1.0"), Some("v0.2.0".to_string()));
        assert_eq!(newer_tag(&release, "0.2.0"), None);
        assert_eq!(newer_tag(&release, "0.3.0"), None);
        // This build's own version is a real version.
        assert!(parse_version(VERSION).is_some(), "{VERSION}");
    }

    #[test]
    fn the_placeholder_repo_sends_no_request() {
        assert!(!repo_ready("OWNER/hexsight"));
        assert!(repo_ready("someone/hexsight"));
        for bad in ["", "hexsight", "/hexsight", "someone/", "a/b/c"] {
            assert!(!repo_ready(bad), "{bad}");
        }
        assert_eq!(
            api_url("someone/hexsight"),
            "https://api.github.com/repos/someone/hexsight/releases/latest"
        );
        assert_eq!(
            releases_url("someone/hexsight"),
            "https://github.com/someone/hexsight/releases/latest"
        );
    }

    #[test]
    fn windows_reads_as_a_short_text() {
        assert_eq!(windows_label(26200), "Windows 11 (build 26200)");
        assert_eq!(windows_label(22000), "Windows 11 (build 22000)");
        assert_eq!(windows_label(19045), "Windows 10 or older (build 19045)");
        assert_eq!(windows_label(0), "Windows (unknown build)");
    }

    fn sample() -> Feedback<'static> {
        Feedback {
            version: "0.1.0",
            windows: windows_label(26200),
            settings: vec![
                ("Language", "auto".into()),
                ("Highlight top-tier cards", "off".into()),
                ("Check for updates", "on".into()),
            ],
        }
    }

    /// The text of a percent-encoded query value, decoded again.
    fn decode(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                out.push(u8::from_str_radix(&text[i + 1..i + 3], 16).unwrap());
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn the_issue_page_gets_the_version_windows_and_settings_only() {
        let url = issue_url("someone/hexsight", &sample());
        assert!(
            url.starts_with("https://github.com/someone/hexsight/issues/new?title=Feedback&body="),
            "{url}"
        );
        // One line, no space, no new line: the browser gets a valid address.
        assert!(!url.contains([' ', '\n', '"', '<']), "{url}");
        let body = decode(url.split_once("&body=").unwrap().1);
        assert!(body.contains("**Hexsight version:** 0.1.0"));
        assert!(body.contains("**Windows:** Windows 11 (build 26200)"));
        assert!(body.contains("- Highlight top-tier cards: off"));
        assert!(body.contains("**What happened?**"));
        assert_eq!(body, issue_body(&sample()));
    }

    #[test]
    fn the_issue_text_holds_no_personal_data() {
        // The same text and the real user name, the machine name and the data folder: none is in it.
        let url = issue_url(REPO, &sample());
        let body = decode(url.split_once("&body=").unwrap().1).to_lowercase();
        let mut private: Vec<String> = ["USERNAME", "COMPUTERNAME", "USERPROFILE", "LOCALAPPDATA"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .map(|v| v.to_lowercase())
            .filter(|v| v.len() > 2)
            .collect();
        private.extend(
            [
                "c:\\",
                "\\users\\",
                "appdata",
                "puuid",
                "riot id",
                "#",
                "lockfile",
                "authorization",
            ]
            .map(String::from),
        );
        for s in private {
            assert!(!body.contains(&s), "the issue text holds {s:?}: {body}");
        }
    }

    #[test]
    fn a_text_is_encoded_for_a_query() {
        assert_eq!(encode("a b&c=d#e\n"), "a%20b%26c%3Dd%23e%0A");
        assert_eq!(encode("Feedback-1_2.3~"), "Feedback-1_2.3~");
        assert_eq!(decode(&encode("Çok iyi *bold*")), "Çok iyi *bold*");
    }

    // ---- One-click update

    const TAG: &str = "v0.2.0";

    /// An asset entry of the API.
    fn asset(name: &str, url: &str, size: u64, state: &str) -> Value {
        json!({"name": name, "browser_download_url": url, "size": size, "state": state})
    }

    fn dl(file: &str) -> String {
        format!("https://github.com/someone/hexsight/releases/download/{TAG}/{file}")
    }

    /// A good release of `TAG` with the exe and its hash file.
    fn release() -> Value {
        let exe = asset_name(TAG);
        json!({
            "tag_name": TAG,
            "assets": [
                asset(&format!("{exe}.sha256"), &dl(&format!("{exe}.sha256")), 100, "uploaded"),
                asset("hexsight-v0.2.0-windows-x64-portable.zip", &dl("hexsight-v0.2.0-windows-x64-portable.zip"), 2_000_000, "uploaded"),
                asset(&exe, &dl(&exe), 6_000_000, "uploaded"),
            ]
        })
    }

    #[test]
    fn the_exe_and_its_hash_file_are_picked_by_their_exact_names() {
        let got = pick_asset(&release(), TAG, "someone/hexsight").unwrap();
        assert_eq!(got.name, "hexsight-v0.2.0-windows-x64.exe");
        assert_eq!(got.size, 6_000_000);
        assert_eq!(got.url, dl("hexsight-v0.2.0-windows-x64.exe"));
        assert_eq!(got.sha_url, dl("hexsight-v0.2.0-windows-x64.exe.sha256"));
    }

    #[test]
    fn a_release_with_a_missing_or_odd_file_gives_no_asset() {
        let repo = "someone/hexsight";
        let exe = asset_name(TAG);
        let with = |f: &dyn Fn(&mut Value)| {
            let mut r = release();
            f(&mut r);
            pick_asset(&r, TAG, repo)
        };
        assert!(pick_asset(&release(), TAG, repo).is_some());
        // The exe or the hash file is missing.
        assert!(with(&|r| drop(r["assets"].as_array_mut().unwrap().remove(2))).is_none());
        assert!(with(&|r| drop(r["assets"].as_array_mut().unwrap().remove(0))).is_none());
        // The tag of the names is not the tag of the release.
        assert!(pick_asset(&release(), "v0.3.0", repo).is_none());
        // An upload that is not finished.
        assert!(with(&|r| r["assets"][2]["state"] = json!("starter")).is_none());
        assert!(with(&|r| r["assets"][0]["state"] = json!("open")).is_none());
        // The size is 0, too small, or above the cap.
        for size in [0, MIN_EXE_BYTES - 1, MAX_EXE_BYTES + 1] {
            assert!(
                with(&|r| r["assets"][2]["size"] = json!(size)).is_none(),
                "{size}"
            );
        }
        assert!(with(&|r| r["assets"][2]["size"] = json!(MAX_EXE_BYTES)).is_some());
        assert!(with(&|r| r["assets"][0]["size"] = json!(MAX_SHA_BYTES + 1)).is_none());
        // A URL outside the allow list.
        assert!(
            with(&|r| r["assets"][2]["browser_download_url"] = json!("http://github.com/x"))
                .is_none()
        );
        let other = format!("https://github.com/evil/hexsight/releases/download/{TAG}/{exe}");
        assert!(with(&|r| r["assets"][2]["browser_download_url"] = json!(other)).is_none());
        // A draft or a pre-release is no update. A release without assets neither.
        assert!(with(&|r| r["prerelease"] = json!(true)).is_none());
        assert!(pick_asset(&json!({"tag_name": TAG}), TAG, repo).is_none());
        assert!(pick_asset(&Value::Null, TAG, repo).is_none());
        // The placeholder repo sends no request.
        assert!(pick_asset(&release(), TAG, "OWNER/hexsight").is_none());
    }

    #[test]
    fn a_download_starts_only_on_github_in_the_releases_of_the_repo() {
        let repo = "someone/hexsight";
        let good = dl("hexsight-v0.2.0-windows-x64.exe");
        assert!(download_url_ok(&good, repo));
        // The owner and the name compare without case. The default port is fine.
        assert!(download_url_ok(&good.replace("someone", "Someone"), repo));
        assert!(download_url_ok(
            &good.replace("github.com", "GitHub.com:443"),
            repo
        ));
        for bad in [
            "",
            "https://",
            "http://github.com/someone/hexsight/releases/download/v1/a.exe",
            "ftp://github.com/someone/hexsight/releases/download/v1/a.exe",
            "https://github.com.evil.com/someone/hexsight/releases/download/v1/a.exe",
            "https://evilgithub.com/someone/hexsight/releases/download/v1/a.exe",
            "https://github.com@evil.com/someone/hexsight/releases/download/v1/a.exe",
            "https://evil.com@github.com/someone/hexsight/releases/download/v1/a.exe",
            "https://github.com:8443/someone/hexsight/releases/download/v1/a.exe",
            "https://github.com/other/hexsight/releases/download/v1/a.exe",
            "https://github.com/someone/hexsight-fork/releases/download/v1/a.exe",
            "https://github.com/someone/hexsight/releases/latest",
            "https://github.com/someone/hexsight/releases/download/../../../evil/x",
            "https://github.com\\@evil.com/someone/hexsight/releases/download/v1/a.exe",
            "https://github.com/someone/hexsight/releases/download/v1/a b.exe",
            "https://objects.githubusercontent.com/someone/hexsight/releases/download/v1/a.exe",
        ] {
            assert!(!download_url_ok(bad, repo), "{bad}");
        }
        assert!(!download_url_ok(&good, "OWNER/hexsight"));
    }

    #[test]
    fn a_redirect_may_reach_only_the_github_hosts() {
        for ok in [
            "https://github.com/x",
            "https://objects.githubusercontent.com/github-production-release-asset/1?sig=a",
            "https://release-assets.githubusercontent.com/github-production-release-asset/1",
            "https://RELEASE-assets.githubusercontent.com:443/a",
        ] {
            assert!(redirect_ok(ok), "{ok}");
        }
        for bad in [
            "",
            "http://objects.githubusercontent.com/a",
            "https://githubusercontent.com/a",
            "https://evil.githubusercontent.com/a",
            "https://objects.githubusercontent.com.evil.com/a",
            "https://objects.githubusercontent.com@evil.com/a",
            "https://github.com:444/a",
            "https://evil.com/github.com",
            "https://evil.com/?h=objects.githubusercontent.com",
            "//objects.githubusercontent.com/a",
            "/a",
        ] {
            assert!(!redirect_ok(bad), "{bad}");
        }
    }

    #[test]
    fn a_location_header_gets_an_address_or_nothing() {
        let base = "https://github.com/a/b/releases/download/v1/x.exe";
        assert_eq!(
            resolve_location(base, "https://objects.githubusercontent.com/z?a=1"),
            Some("https://objects.githubusercontent.com/z?a=1".to_string())
        );
        assert_eq!(
            resolve_location(base, "//release-assets.githubusercontent.com/z"),
            Some("https://release-assets.githubusercontent.com/z".to_string())
        );
        assert_eq!(
            resolve_location(base, "/other/path"),
            Some("https://github.com/other/path".to_string())
        );
        // A relative path, another scheme and an empty text have no meaning here.
        for bad in ["", "z", "../z", "http://github.com/z", "file:///c:/x"] {
            assert_eq!(resolve_location(base, bad), None, "{bad}");
        }
        // A resolved address still goes through the redirect check.
        let evil = resolve_location(base, "//evil.com/z").unwrap();
        assert!(!redirect_ok(&evil));
    }

    const ABC: [u8; 32] = [
        0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22,
        0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00,
        0x15, 0xad,
    ];
    const ABC_HEX: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn a_hash_file_gives_the_hash_only_for_the_right_name() {
        let name = "hexsight-v0.2.0-windows-x64.exe";
        assert_eq!(
            parse_sha256(&format!("{ABC_HEX}  {name}\n"), name),
            Some(ABC)
        );
        assert_eq!(
            parse_sha256(&format!("{ABC_HEX} *{name}\r\n"), name),
            Some(ABC)
        );
        assert_eq!(parse_sha256(&format!("\n  {ABC_HEX}\n"), name), Some(ABC));
        assert_eq!(parse_sha256(&ABC_HEX.to_uppercase(), name), Some(ABC));
        for bad in [
            String::new(),
            "\n\n".to_string(),
            format!("{ABC_HEX}  other.exe"),
            format!("{} {name}", &ABC_HEX[..63]),
            format!("{ABC_HEX}0  {name}"),
            format!("{}g  {name}", &ABC_HEX[..63]),
            format!("{}  {name}", "0".repeat(64)),
            format!("sha256:{ABC_HEX}"),
            "hello".to_string(),
        ] {
            assert_eq!(parse_sha256(&bad, name), None, "{bad:?}");
        }
    }

    #[test]
    fn the_copy_hashes_every_byte_and_stops_at_the_cap() {
        let mut out = Vec::new();
        let mut seen = Vec::new();
        let got = copy_hashed(&b"abc"[..], &mut out, 10, |n| seen.push(n)).unwrap();
        // The known SHA-256 value of "abc".
        assert_eq!(got, (3, ABC));
        assert_eq!(out, b"abc");
        assert_eq!(seen, vec![3]);
        // A long input comes in chunks and still gives the hash of the whole.
        let big = vec![7u8; 200_000];
        let mut sink = Vec::new();
        let (n, hash) =
            copy_hashed(std::io::Cursor::new(&big), &mut sink, 200_000, |_| {}).unwrap();
        assert_eq!((n, sink.len()), (200_000, 200_000));
        let want = ring::digest::digest(&ring::digest::SHA256, &big);
        assert_eq!(hash.as_slice(), want.as_ref());
        // One byte over the cap fails, and the sink never gets more than the cap.
        let mut sink = Vec::new();
        let got = copy_hashed(std::io::Cursor::new(&big), &mut sink, 199_999, |_| {});
        assert_eq!(got, Err(UpdateError::TooBig));
        assert!(sink.len() <= 199_999);
        // An empty input is no error here. The caller checks the size.
        assert_eq!(copy_hashed(&b""[..], Vec::new(), 5, |_| {}).unwrap().0, 0);
    }

    /// A reader that fails after a few bytes, and a writer that always fails.
    struct Broken;
    impl std::io::Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("cut"))
        }
    }
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_cut_download_and_a_full_disk_are_told_apart() {
        assert_eq!(
            copy_hashed(Broken, Vec::new(), 10, |_| {}),
            Err(UpdateError::Net)
        );
        assert_eq!(
            copy_hashed(&b"abc"[..], Broken, 10, |_| {}),
            Err(UpdateError::Disk)
        );
    }

    #[test]
    fn the_percent_never_passes_100() {
        assert_eq!(percent(0, 0), 0);
        assert_eq!(percent(5, 0), 0);
        assert_eq!(percent(0, 200), 0);
        assert_eq!(percent(50, 200), 25);
        assert_eq!(percent(200, 200), 100);
        assert_eq!(percent(900, 200), 100);
    }

    #[test]
    fn a_tag_that_is_not_newer_gives_no_update_to_download() {
        let same = json!({"tag_name": format!("v{VERSION}")});
        assert_eq!(newer_tag(&same, VERSION), None);
        let pre = json!({"tag_name": "v99.0.0", "prerelease": true});
        assert_eq!(newer_tag(&pre, VERSION), None);
    }

    #[test]
    fn the_update_waits_in_a_champ_select_and_in_a_game() {
        let all = [
            Phase::Offline,
            Phase::Idle,
            Phase::Lobby,
            Phase::ChampSelect,
            Phase::InGame,
            Phase::PostGame,
        ];
        for phase in all {
            let calm = update_allowed(phase, None, 0, false);
            match phase {
                Phase::ChampSelect => assert_eq!(calm, Err(Blocked::ChampSelect)),
                Phase::InGame => assert_eq!(calm, Err(Blocked::Game)),
                _ => assert_eq!(calm, Ok(()), "{phase:?}"),
            }
            // A game window, a level or an open port 2999 block in every phase.
            assert!(
                update_allowed(phase, Some(true), 0, false).is_err(),
                "{phase:?}"
            );
            assert!(update_allowed(phase, None, 3, false).is_err(), "{phase:?}");
            assert!(update_allowed(phase, None, 0, true).is_err(), "{phase:?}");
            // A closed game window is no game.
            if !matches!(phase, Phase::ChampSelect | Phase::InGame) {
                assert!(
                    update_allowed(phase, Some(false), 0, false).is_ok(),
                    "{phase:?}"
                );
            }
        }
    }

    #[test]
    fn the_new_exe_gets_the_process_to_wait_for() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            parse_updated_from(args(&["--updated-from", "123"])),
            Some(123)
        );
        assert_eq!(
            parse_updated_from(args(&["-x", "--updated-from", "7", "y"])),
            Some(7)
        );
        for bad in [
            &[][..],
            &["--updated-from"],
            &["--updated-from", "abc"],
            &["--updated-from", "-5"],
            &["--updated-from", "0"],
            &["--updated-from", "99999999999"],
            &["--updated-from=123"],
            &["123"],
        ] {
            assert_eq!(parse_updated_from(args(bad)), None, "{bad:?}");
        }
    }

    // ---- The swap, in a folder of the temp folder

    const ZERO: std::time::Duration = std::time::Duration::ZERO;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("hexsight-test-swap-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A folder with `Hexsight.exe` ("old") and `Hexsight.exe.new` ("new").
    fn swap_dir(tag: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let dir = temp_dir(tag);
        let exe = dir.join("Hexsight.exe");
        std::fs::write(&exe, b"old").unwrap();
        let new = new_path(&exe);
        std::fs::write(&new, b"new").unwrap();
        (dir, exe, new)
    }

    #[test]
    fn the_files_of_an_update_have_the_names_of_the_exe_with_a_suffix() {
        let exe = std::path::Path::new(r"C:\x\Hexsight.exe");
        assert_eq!(
            new_path(exe),
            std::path::Path::new(r"C:\x\Hexsight.exe.new")
        );
        assert_eq!(
            old_path(exe),
            std::path::Path::new(r"C:\x\Hexsight.exe.old")
        );
    }

    #[test]
    fn the_swap_puts_the_new_file_in_place_and_keeps_the_old_one() {
        let (dir, exe, new) = swap_dir("ok");
        swap_in(&exe, &new).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(old_path(&exe)).unwrap(), b"old");
        assert!(!new.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_copy_of_an_earlier_update_does_not_stop_the_swap() {
        let (dir, exe, new) = swap_dir("stale");
        std::fs::write(old_path(&exe), b"older").unwrap();
        swap_in(&exe, &new).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(old_path(&exe)).unwrap(), b"old");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_or_empty_new_file_changes_nothing() {
        let (dir, exe, new) = swap_dir("missing");
        std::fs::remove_file(&new).unwrap();
        assert_eq!(swap_in(&exe, &new), Err(UpdateError::Disk));
        std::fs::write(&new, b"").unwrap();
        assert_eq!(swap_in(&exe, &new), Err(UpdateError::Disk));
        assert_eq!(std::fs::read(&exe).unwrap(), b"old");
        assert!(!old_path(&exe).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_exe_fails_the_first_rename_and_deletes_the_download() {
        let (dir, exe, new) = swap_dir("noexe");
        std::fs::remove_file(&exe).unwrap();
        assert_eq!(swap_in(&exe, &new), Err(UpdateError::Swap));
        assert!(!new.exists() && !exe.exists() && !old_path(&exe).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_old_copy_that_cannot_go_stops_the_swap_before_any_rename() {
        let (dir, exe, new) = swap_dir("locked");
        // A folder with this name cannot be removed as a file.
        std::fs::create_dir(old_path(&exe)).unwrap();
        assert_eq!(swap_in(&exe, &new), Err(UpdateError::OldLocked));
        assert_eq!(std::fs::read(&exe).unwrap(), b"old");
        assert!(!new.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_second_rename_gives_the_old_exe_its_name_back() {
        let (dir, exe, new) = swap_dir("second");
        let calls = std::cell::Cell::new(0);
        let rename = |from: &std::path::Path, to: &std::path::Path| {
            calls.set(calls.get() + 1);
            // The move of the new file never works.
            if from.to_string_lossy().ends_with(".new") {
                Err(std::io::Error::other("denied"))
            } else {
                std::fs::rename(from, to)
            }
        };
        assert_eq!(
            swap_with(&exe, &new, &rename, ZERO),
            Err(UpdateError::Swap)
        );
        // One rename of the exe, the tries of the move, and one rename back.
        assert_eq!(calls.get(), 1 + RENAME_TRIES + 1);
        assert_eq!(std::fs::read(&exe).unwrap(), b"old");
        assert!(!old_path(&exe).exists() && !new.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_rollback_tells_where_the_old_exe_is() {
        let (dir, exe, new) = swap_dir("rollback");
        let calls = std::cell::Cell::new(0);
        let rename = |from: &std::path::Path, to: &std::path::Path| {
            calls.set(calls.get() + 1);
            if calls.get() >= 2 {
                Err(std::io::Error::other("denied"))
            } else {
                std::fs::rename(from, to)
            }
        };
        let got = swap_with(&exe, &new, &rename, ZERO);
        let old = old_path(&exe);
        assert_eq!(
            got,
            Err(UpdateError::RollbackFailed(old.display().to_string()))
        );
        // The put-back has more tries than the other steps.
        assert_eq!(calls.get(), 1 + RENAME_TRIES + RENAME_TRIES * 2);
        // The old exe is safe under its second name.
        assert_eq!(std::fs::read(&old).unwrap(), b"old");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_rename_that_a_scanner_blocks_for_a_moment_works_at_a_later_try() {
        // The move of the new file fails twice (a scanner holds it), then it works.
        let (dir, exe, new) = swap_dir("scanner");
        let blocked = std::cell::Cell::new(2);
        let rename = |from: &std::path::Path, to: &std::path::Path| {
            if from.to_string_lossy().ends_with(".new") && blocked.get() > 0 {
                blocked.set(blocked.get() - 1);
                return Err(std::io::Error::other("sharing violation"));
            }
            std::fs::rename(from, to)
        };
        assert_eq!(swap_with(&exe, &new, &rename, ZERO), Ok(()));
        assert_eq!(blocked.get(), 0);
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(old_path(&exe)).unwrap(), b"old");
        // A restore that fails at first also gets more tries.
        let (dir2, exe2, new2) = swap_dir("scanner-back");
        let back_blocked = std::cell::Cell::new(3);
        let rename = |from: &std::path::Path, to: &std::path::Path| {
            if from.to_string_lossy().ends_with(".new") {
                return Err(std::io::Error::other("denied"));
            }
            if from.to_string_lossy().ends_with(".old") && back_blocked.get() > 0 {
                back_blocked.set(back_blocked.get() - 1);
                return Err(std::io::Error::other("sharing violation"));
            }
            std::fs::rename(from, to)
        };
        assert_eq!(
            swap_with(&exe2, &new2, &rename, ZERO),
            Err(UpdateError::Swap)
        );
        assert_eq!(std::fs::read(&exe2).unwrap(), b"old");
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(dir2);
    }

    #[test]
    fn a_start_that_works_ends_the_update_and_a_start_that_fails_undoes_it() {
        let (dir, exe, new) = swap_dir("start");
        let mut started = None;
        install_and_restart(
            &exe,
            &new,
            |p| {
                started = Some(p.to_path_buf());
                Ok(())
            },
            |()| true,
        )
        .unwrap();
        assert_eq!(started.as_deref(), Some(exe.as_path()));
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(&dir);

        let (dir, exe, new) = swap_dir("start-fail");
        let got = install_and_restart(
            &exe,
            &new,
            |_| Err::<(), _>(std::io::Error::other("blocked")),
            |()| true,
        );
        assert_eq!(got, Err(UpdateError::Restart));
        // The old exe is back under its own name, and no leftover stays.
        assert_eq!(std::fs::read(&exe).unwrap(), b"old");
        assert!(!old_path(&exe).exists() && !new.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_new_copy_that_dies_after_its_start_is_undone() {
        // The start works, but a scanner kills the new copy: confirm says no.
        let (dir, exe, new) = swap_dir("start-dies");
        std::fs::write(&exe, b"old").unwrap();
        let got = install_and_restart(&exe, &new, |_| Ok(7u32), |pid| pid == 0);
        assert_eq!(got, Err(UpdateError::Restart));
        assert_eq!(std::fs::read(&exe).unwrap(), b"old");
        assert!(!old_path(&exe).exists() && !new.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Run `wait_for_start` over a list of looks, one per step. The last look repeats.
    fn watch(looks: &[StartProbe], timeout_steps: u32) -> (bool, u32) {
        let step = std::time::Duration::from_millis(100);
        let n = std::cell::Cell::new(0usize);
        let sleeps = std::cell::Cell::new(0u32);
        let ok = wait_for_start(
            || {
                let i = n.get().min(looks.len() - 1);
                n.set(n.get() + 1);
                looks[i]
            },
            |_| sleeps.set(sleeps.get() + 1),
            step,
            step * 3,
            step * timeout_steps,
        );
        (ok, sleeps.get())
    }

    #[test]
    fn the_watch_over_the_new_copy_ends_the_update_only_for_a_copy_that_lives() {
        use StartProbe::*;
        // It dies at once, or after it said that it is up: no.
        assert_eq!(watch(&[Dead], 50), (false, 0));
        assert_eq!(watch(&[Silent, Silent, Dead], 50), (false, 2));
        assert_eq!(watch(&[Silent, Up, Up, Dead], 50), (false, 3));
        // It says that it is up and lives for the grace time: yes, early.
        assert_eq!(watch(&[Silent, Up], 50), (true, 4));
        // It never says anything but lives until the time ends: yes.
        assert_eq!(watch(&[Silent], 10), (true, 10));
        // It dies in the very last look.
        assert_eq!(watch(&[Silent, Silent, Silent, Dead], 3), (false, 3));
    }

    #[test]
    fn the_files_of_an_update_go_at_different_times() {
        let (dir, exe, new) = swap_dir("cleanup");
        std::fs::write(old_path(&exe), b"old").unwrap();
        // The half download goes at the start. The old exe stays: it may be needed.
        cleanup_download(&exe);
        assert!(!new.exists() && old_path(&exe).exists());
        // After a healthy run, the old exe goes too.
        cleanup_old(&exe);
        assert!(!old_path(&exe).exists());
        assert_eq!(std::fs::read(&exe).unwrap(), b"old");
        // Nothing to delete is no error.
        cleanup_download(&exe);
        cleanup_old(&exe);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_running_exe_can_be_swapped_by_name() {
        // The core assumption of the update: Windows lets a running exe change its name,
        // and a new file can take the old name at once.
        let dir = temp_dir("running");
        let exe = dir.join("Hexsight.exe");
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        std::fs::copy(std::path::Path::new(&system).join(r"System32\cmd.exe"), &exe).unwrap();
        // The shell waits for input that never comes, so it runs until the test ends it.
        let mut child = std::process::Command::new(&exe)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(child.try_wait().unwrap().is_none(), "the copy runs");
        let new = new_path(&exe);
        std::fs::write(&new, b"MZ new").unwrap();
        let got = swap_in(&exe, &new);
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(got, Ok(()));
        assert_eq!(std::fs::read(&exe).unwrap(), b"MZ new");
        assert!(std::fs::metadata(old_path(&exe)).unwrap().len() > 10_000);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_short_file_is_a_net_problem_and_a_wrong_file_is_a_hash_problem() {
        assert_eq!(check_size(100, 100), Ok(()));
        assert_eq!(check_size(99, 100), Err(UpdateError::Net));
        assert_eq!(check_size(0, 100), Err(UpdateError::Net));
        assert_eq!(check_size(101, 100), Err(UpdateError::Hash));
    }

    #[test]
    fn a_failure_goes_away_after_a_while_unless_the_player_must_read_it() {
        let secs = |e: UpdateError| e.show_for().map(|d| d.as_secs());
        assert_eq!(secs(UpdateError::NotWritable), None);
        assert_eq!(secs(UpdateError::RollbackFailed("x".into())), None);
        assert_eq!(secs(UpdateError::Busy), Some(20));
        for e in [
            UpdateError::Net,
            UpdateError::Hash,
            UpdateError::Swap,
            UpdateError::Restart,
        ] {
            assert_eq!(secs(e), Some(300));
        }
    }

    #[test]
    fn a_folder_you_can_write_to_passes_and_leaves_no_file() {
        let dir = temp_dir("probe");
        assert!(folder_writable(&dir));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        assert!(!folder_writable(&dir.join("missing")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn only_a_file_that_starts_with_mz_looks_like_a_program() {
        let dir = temp_dir("mz");
        let (good, bad, short) = (dir.join("a"), dir.join("b"), dir.join("c"));
        std::fs::write(&good, b"MZ\x90\x00").unwrap();
        std::fs::write(&bad, b"<html>").unwrap();
        std::fs::write(&short, b"M").unwrap();
        assert!(looks_like_exe(&good));
        assert!(!looks_like_exe(&bad) && !looks_like_exe(&short));
        assert!(!looks_like_exe(&dir.join("missing")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_state_is_busy_from_the_first_byte_to_the_swap() {
        assert!(!UpdateState::Idle.busy());
        assert!(!UpdateState::Failed(UpdateError::Net).busy());
        assert!(UpdateState::Downloading { done: 0, total: 0 }.busy());
        assert!(UpdateState::Verifying.busy());
        assert!(UpdateState::Installing.busy());
    }
}
