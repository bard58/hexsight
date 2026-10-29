//! The update note and the feedback link. Both use the public GitHub repo.
//!
//! Hexsight asks the GitHub releases API for the latest tag at most once a day,
//! compares it with its own version, and shows a link when a newer one exists.
//! It never downloads or installs anything. The feedback link opens a new issue
//! page in the browser with the version, the Windows version and the main settings
//! in the text. The player reads and sends it, or closes the tab.

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
}
