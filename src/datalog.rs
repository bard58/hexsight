//! A small log of the data downloads that failed: `hexsight.log` in the data
//! folder. One line for each failure: the UTC time, the host and the path of the
//! URL, the reason (an HTTP status or an error kind), and the bytes that came.
//! It holds no player data: the URLs of Hexsight carry only the patch, the
//! client language and champion ids, and the line drops a query and a login.
//! At `MAX_BYTES` the file moves to `hexsight.log.1` (an older one goes), so the
//! two files never hold more than twice that.

use crate::history;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

/// The name of the log in the data folder.
pub const FILE: &str = "hexsight.log";

/// The name of the older log after a move.
pub const OLD_FILE: &str = "hexsight.log.1";

/// The log moves to `OLD_FILE` before it grows past this size.
pub const MAX_BYTES: u64 = 256 * 1024;

/// Several threads download. One write at a time keeps each line whole.
static WRITE: Mutex<()> = Mutex::new(());

/// "https://data.aramkit.com/data/x/74.json?a=b" -> "data.aramkit.com/data/x/74.json".
/// No scheme, no login before the host, no query and no fragment.
fn host_and_path(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.rsplit('@').next().unwrap_or(host);
    format!("{host}/{path}")
}

/// One line of the log. `now_ms` is the time in Unix milliseconds.
pub fn line(now_ms: i64, url: &str, why: &str, bytes: usize) -> String {
    let line = format!(
        "{} {} {why}, {bytes} bytes",
        history::iso_date(now_ms),
        host_and_path(url)
    );
    // One failure is one line, whatever an error text holds.
    line.replace(['\r', '\n'], " ")
}

/// Add one line to the log in `dir`. A log that would pass `MAX_BYTES` moves to
/// `OLD_FILE` first. Ignore errors: a log must never stop a download.
pub fn append(dir: &Path, line: &str) {
    let _one = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let path = dir.join(FILE);
    let size = std::fs::metadata(&path).map_or(0, |m| m.len());
    let mut open = std::fs::OpenOptions::new();
    open.create(true).append(true);
    // A rename on Windows replaces the older file. When the move fails (a locked
    // or read-only old file), the log starts again, so it never passes the cap.
    if size > 0
        && size + line.len() as u64 + 2 > MAX_BYTES
        && std::fs::rename(&path, dir.join(OLD_FILE)).is_err()
    {
        open.append(false).write(true).truncate(true);
    }
    if let Ok(mut f) = open.open(&path) {
        let _ = f.write_all(format!("{line}\r\n").as_bytes());
    }
}

/// The time now in Unix milliseconds. Only the real log needs it: the tests never write the log.
#[cfg(not(test))]
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, empty folder under the temp folder. The real data folder is never used.
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("hexsight-test-log-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_log_line_has_the_time_the_host_the_path_the_reason_and_the_bytes() {
        let url = "https://data.aramkit.com/data/16.21-20261024-79beb9ff65a7/stats/all/champion-details/60074.json";
        assert_eq!(
            line(1_792_747_231_762, url, "status 404", 0),
            "2026-10-23T09:20:31Z data.aramkit.com/data/16.21-20261024-79beb9ff65a7/stats/all/champion-details/60074.json status 404, 0 bytes"
        );
        assert_eq!(
            line(1_000, "http://127.0.0.1:9/a.png", "no connection", 12),
            "1970-01-01T00:00:01Z 127.0.0.1:9/a.png no connection, 12 bytes"
        );
    }

    #[test]
    fn a_log_line_never_holds_a_query_a_login_or_a_second_line() {
        // Short invented values, so the export scan finds no login, mail or Riot ID shape here.
        let l = line(
            1_000,
            "https://me:pw@127.0.0.1:9/v1/x.json?player=ab#cd",
            "timeout\r\nmore",
            3,
        );
        assert_eq!(
            l,
            "1970-01-01T00:00:01Z 127.0.0.1:9/v1/x.json timeout  more, 3 bytes"
        );
        assert!(!l.contains("pw") && !l.contains("player") && !l.contains(['@', '?', '#']));
        assert_eq!(host_and_path("example.test"), "example.test/");
    }

    #[test]
    fn lines_go_to_the_end_of_the_log() {
        let dir = temp_dir("append");
        append(&dir, "one");
        append(&dir, "two");
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE)).unwrap(),
            "one\r\ntwo\r\n"
        );
        assert!(!dir.join(OLD_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_full_log_moves_to_the_old_file_and_replaces_it() {
        let dir = temp_dir("rotate");
        std::fs::write(dir.join(OLD_FILE), "the oldest lines\r\n").unwrap();
        let full = "x".repeat(MAX_BYTES as usize - 10);
        std::fs::write(dir.join(FILE), &full).unwrap();
        // This line fits no more: the log moves first, then the line starts a new log.
        append(&dir, "a line of twenty bytes");
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE)).unwrap(),
            "a line of twenty bytes\r\n"
        );
        assert_eq!(std::fs::read_to_string(dir.join(OLD_FILE)).unwrap(), full);
        // A short line that fits stays in the same log.
        append(&dir, "next");
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE)).unwrap(),
            "a line of twenty bytes\r\nnext\r\n"
        );
        assert!(std::fs::metadata(dir.join(FILE)).unwrap().len() <= MAX_BYTES);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_full_log_that_cannot_move_starts_again_and_stays_under_the_cap() {
        let dir = temp_dir("stuck");
        // A folder with the name of the old file: the move fails, as for a locked or read-only file.
        std::fs::create_dir_all(dir.join(OLD_FILE)).unwrap();
        std::fs::write(dir.join(OLD_FILE).join("keep.txt"), "x").unwrap();
        std::fs::write(dir.join(FILE), "x".repeat(MAX_BYTES as usize - 10)).unwrap();
        append(&dir, "a line of twenty bytes");
        assert!(std::fs::metadata(dir.join(FILE)).unwrap().len() <= MAX_BYTES);
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE)).unwrap(),
            "a line of twenty bytes\r\n"
        );
        assert!(dir.join(OLD_FILE).join("keep.txt").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
