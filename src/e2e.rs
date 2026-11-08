//! A test-only harness for the update button (cargo feature `e2e`, off by default).
//!
//! The release workflow and the CI never enable it. A build with the feature reads these
//! variables:
//!
//! - `HEXSIGHT_E2E_API`: an origin `http://127.0.0.1:<port>` of a fake release server. It
//!   replaces `https://api.github.com`, and the download checks accept this origin too.
//! - `HEXSIGHT_E2E_VERSION`: replaces the own version in the update compare.
//! - `HEXSIGHT_E2E_CLICK=1`: sends "Update now" once, as soon as an update is found.
//! - `HEXSIGHT_E2E_LOG`: a file. One line per event is appended to it.
//!
//! A copy that an update started (`--updated-from`) ignores the version and the click, so
//! it does not update again in a loop. Nothing here starts a thread.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::update::{self, UpdateState};

/// False for a copy that an update started.
pub fn active() -> bool {
    static ACTIVE: OnceLock<bool> = OnceLock::new();
    *ACTIVE.get_or_init(|| update::parse_updated_from(std::env::args()).is_none())
}

/// The origin `http://127.0.0.1:<port>` of the text, without a trailing slash. `None` for
/// any other host, scheme, port text or extra part.
pub fn parse_origin(text: &str) -> Option<String> {
    let text = text.trim().trim_end_matches('/');
    let port = text.strip_prefix("http://127.0.0.1:")?;
    let valid = (1..=5).contains(&port.len())
        && port.bytes().all(|b| b.is_ascii_digit())
        && port.parse::<u32>().is_ok_and(|p| (1..=65535).contains(&p));
    valid.then(|| text.to_string())
}

/// The fake server origin from `HEXSIGHT_E2E_API`.
pub fn api_origin() -> Option<&'static str> {
    static ORIGIN: OnceLock<Option<String>> = OnceLock::new();
    ORIGIN
        .get_or_init(|| {
            std::env::var("HEXSIGHT_E2E_API")
                .ok()
                .and_then(|v| parse_origin(&v))
        })
        .as_deref()
}

/// The version from `HEXSIGHT_E2E_VERSION`, unless this is a copy that an update started.
pub fn version() -> Option<String> {
    if !active() {
        return None;
    }
    std::env::var("HEXSIGHT_E2E_VERSION")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// True when the update click is wanted and this is no copy that an update started.
pub fn click_wanted() -> bool {
    active() && std::env::var("HEXSIGHT_E2E_CLICK").is_ok_and(|v| v.trim() == "1")
}

/// True once: when the click is wanted, an update is stored and the click is not sent yet.
pub fn click_due(update_found: bool) -> bool {
    static SENT: AtomicBool = AtomicBool::new(false);
    if !update_found || !click_wanted() || SENT.swap(true, Ordering::Relaxed) {
        return false;
    }
    log("click sent");
    true
}

/// True when `url` is on exactly this origin: the origin, then a `/` and a path. A user
/// name, another port or a longer host name does not match.
pub fn url_on_origin(url: &str, origin: &str) -> bool {
    if url.chars().any(|c| c.is_control() || c == ' ' || c == '\\') {
        return false;
    }
    url.strip_prefix(origin)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#']))
}

/// True when `url` is a download of the releases of `repo` on this origin.
pub fn download_ok(url: &str, origin: &str, repo: &str) -> bool {
    if !url_on_origin(url, origin) || !update::repo_ready(repo) {
        return false;
    }
    let tail = &url[origin.len()..];
    let path = tail.split(['?', '#']).next().unwrap_or("");
    let want = format!("/{}/releases/download/", repo.to_ascii_lowercase());
    path.to_ascii_lowercase().starts_with(&want) && !path.split('/').any(|p| p == ".." || p == ".")
}

/// Append one line with a time to `HEXSIGHT_E2E_LOG`. Nothing happens without the variable.
pub fn log(event: &str) {
    let Ok(path) = std::env::var("HEXSIGHT_E2E_LOG") else {
        return;
    };
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(
            f,
            "{}.{:03} pid={} {event}",
            crate::history::iso_date(ms).trim_end_matches('Z'),
            ms % 1000,
            std::process::id()
        );
    }
}

/// Log a change of the update state. The progress of a download is one line only.
pub fn state(state: &UpdateState) {
    static LAST: Mutex<String> = Mutex::new(String::new());
    let label = match state {
        UpdateState::Downloading { total, .. } => format!("Downloading total={total}"),
        other => format!("{other:?}"),
    };
    let Ok(mut last) = LAST.lock() else {
        return;
    };
    if *last != label {
        log(&format!("state={label}"));
        *last = label;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: &str = "http://127.0.0.1:4000";
    const REPO: &str = "bard58/hexsight";

    #[test]
    fn only_a_local_origin_with_a_port_parses() {
        assert_eq!(parse_origin(ORIGIN).as_deref(), Some(ORIGIN));
        assert_eq!(
            parse_origin(" http://127.0.0.1:4000/ ").as_deref(),
            Some(ORIGIN)
        );
        for bad in [
            "",
            "http://127.0.0.1",
            "http://127.0.0.1:",
            "http://127.0.0.1:0",
            "http://127.0.0.1:65536",
            "http://127.0.0.1:12a",
            "https://127.0.0.1:4000",
            "http://localhost:4000",
            "http://127.0.0.1:4000/x",
            "http://127.0.0.1.evil.com:4000",
            "http://evil.com@127.0.0.1:4000",
        ] {
            assert_eq!(parse_origin(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_allow_list_takes_exactly_the_given_origin() {
        let good = format!("{ORIGIN}/bard58/hexsight/releases/download/v0.1.9/a.exe");
        assert!(download_ok(&good, ORIGIN, REPO));
        assert!(download_ok(&good.replace("bard58", "Bard58"), ORIGIN, REPO));
        assert!(url_on_origin(&format!("{ORIGIN}/dl/a.exe"), ORIGIN));
        for bad in [
            "http://127.0.0.1:4001/bard58/hexsight/releases/download/v1/a.exe",
            "http://127.0.0.1:40000/bard58/hexsight/releases/download/v1/a.exe",
            "https://127.0.0.1:4000/bard58/hexsight/releases/download/v1/a.exe",
            "http://127.0.0.1:4000@evil.com/bard58/hexsight/releases/download/v1/a.exe",
            "http://127.0.0.1:4000.evil.com/bard58/hexsight/releases/download/v1/a.exe",
            "http://localhost:4000/bard58/hexsight/releases/download/v1/a.exe",
            "http://evil.com/?h=http://127.0.0.1:4000",
            "http://127.0.0.1:4000/other/hexsight/releases/download/v1/a.exe",
            "http://127.0.0.1:4000/bard58/hexsight/releases/latest",
            "http://127.0.0.1:4000/bard58/hexsight/releases/download/../../x",
            "http://127.0.0.1:4000/bard58/hexsight/releases/download/v1/a b.exe",
            "",
        ] {
            assert!(!download_ok(bad, ORIGIN, REPO), "{bad}");
        }
        assert!(!download_ok(&good, ORIGIN, "OWNER/hexsight"));
        assert!(!url_on_origin("http://127.0.0.1:4001/dl/a", ORIGIN));
        assert!(!url_on_origin("http://127.0.0.1:40000/dl/a", ORIGIN));
        assert!(!url_on_origin(
            "http://127.0.0.1:4000@evil.com/dl/a",
            ORIGIN
        ));
        assert!(!url_on_origin("https://api.github.com/x", ORIGIN));
    }

    #[test]
    fn the_normal_github_rules_stay_with_no_origin_set() {
        if std::env::var_os("HEXSIGHT_E2E_API").is_some() {
            return;
        }
        assert_eq!(api_origin(), None);
        let github = "https://github.com/bard58/hexsight/releases/download/v1/a.exe";
        assert!(update::download_url_ok(github, REPO));
        assert!(!update::download_url_ok(
            "http://127.0.0.1:4000/bard58/hexsight/releases/download/v1/a.exe",
            REPO
        ));
        assert!(!update::redirect_ok("http://127.0.0.1:4000/dl/a.exe"));
        assert_eq!(
            update::api_url(REPO),
            "https://api.github.com/repos/bard58/hexsight/releases/latest"
        );
    }
}
