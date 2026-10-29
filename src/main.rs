#![windows_subsystem = "windows"]

mod app;
mod badges;
mod cache;
mod data;
mod datalog;
mod history;
mod i18n;
mod itemset;
mod kit;
mod lcu;
mod model;
mod net;
mod ocr;
mod suggest;
mod theme;
mod update;
mod win;
mod worker;

/// One line of `crash.log`: the UTC time, the version of Hexsight, the name of the
/// thread that failed, the Windows build, and the text of the panic. A panic
/// text can have several lines. They join with " | ", so one crash is one line.
fn crash_line(version: &str, now_ms: i64, thread: &str, build: u32, info: &str) -> String {
    let info = info.trim().replace(['\r', '\n'], " | ");
    format!(
        "{} · Hexsight {version} · thread {thread} · Windows build {build} · {info}",
        history::iso_date(now_ms)
    )
}

/// Test hook: HEXSIGHT_OCR_FILE=<screenshot> runs the card reader on an image file
/// and writes the result to HEXSIGHT_OCR_OUT (default: ocr-result.txt), then exits.
/// The result starts with the line "presence=yes" or "presence=no" (the cheap card check).
fn ocr_file_test(path: &str) {
    unsafe {
        let _ = windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
        );
    }
    let out_path = std::env::var("HEXSIGHT_OCR_OUT").unwrap_or_else(|_| "ocr-result.txt".into());
    let mut out = String::new();
    let img = image::open(path).expect("image").to_rgba8();
    // Same crop as the live capture: x 10-90 %, y 12-78 %.
    let shot = ocr::shot_from_image(&img);
    let (w, h) = (shot.crop.w as u32, shot.crop.h as u32);
    // The answer of the cheap check that gates the heartbeat reads.
    out.push_str(&format!(
        "presence={}\n",
        if ocr::cards_visible(&shot.bgra, w, h) {
            "yes"
        } else {
            "no"
        }
    ));
    let locale = std::env::var("HEXSIGHT_LOCALE").unwrap_or_else(|_| "tr_TR".into());
    let sd = data::load_static(&cache::web_agent(), &locale).expect("static data");
    let names = sd
        .augments
        .iter()
        .flat_map(|(id, a)| [(*id, a.name.as_str()), (*id, a.name_en.as_str())]);
    let matcher = ocr::Matcher::new(names);
    let engine = ocr::Ocr::new().expect("ocr engine");
    // The whole capture alone first, for comparison, then the reading of a live scan.
    let t = std::time::Instant::now();
    let whole = engine.recognize(&shot.bgra, w, h).unwrap_or_default();
    out.push_str(&format!(
        "whole_ms={} whole_lines={} whole_found={}\n",
        t.elapsed().as_millis(),
        whole.len(),
        matcher.find(&whole).len()
    ));
    let t = std::time::Instant::now();
    let read = engine.read_cards(&shot, &matcher, None);
    out.push_str(&format!(
        "ocr_ms={} lines={}\n",
        t.elapsed().as_millis(),
        read.lines.len()
    ));
    for l in &read.lines {
        out.push_str(&format!("  [{:.2},{:.2}] {}\n", l.x, l.y, l.text));
    }
    out.push_str("found:");
    for c in &read.matched {
        out.push_str(&format!(
            " {} ({}, x {:.2}, y {:.2})",
            c.id, sd.augments[&c.id].name, c.x, c.y
        ));
    }
    out.push('\n');
    let _ = std::fs::write(out_path, out);
}

fn main() -> eframe::Result {
    if let Ok(p) = std::env::var("HEXSIGHT_OCR_FILE") {
        ocr_file_test(&p);
        return Ok(());
    }
    // One copy only. A second start brings the first window to the front.
    if !win::single_instance() {
        return Ok(());
    }
    // The "Start with Windows" setting of the legacy app moves to the new name.
    // A portable copy leaves it alone.
    win::migrate_legacy_autostart(cache::place().portable());
    // With panic = "abort" and no console, keep a trace of a crash.
    std::panic::set_hook(Box::new(|info| {
        // The data folder: `%LOCALAPPDATA%\Hexsight`, or `Hexsight-data` next to the exe in portable mode.
        // `place()` above chose it. The hook never chooses it again, so a panic during the choice cannot hang.
        let Some(dir) = cache::chosen_app_dir() else {
            return;
        };
        let path = dir.join("crash.log");
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64);
            let thread = std::thread::current();
            let line = crash_line(
                env!("CARGO_PKG_VERSION"),
                now,
                thread.name().unwrap_or("unnamed"),
                win::windows_build(),
                &info.to_string(),
            );
            let _ = writeln!(f, "{line}");
        }
    }));
    let mut settings = app::Settings::load();
    // A saved position on a monitor that is gone would hide the window.
    if let Some([x, y]) = settings.pos {
        let (vx, vy, vw, vh) = win::virtual_screen();
        if x < vx - 100.0 || y < vy - 10.0 || x > vx + vw - 100.0 || y > vy + vh - 60.0 {
            settings.pos = None;
        }
    }
    // Frameless and transparent: the app draws its own title bar, and
    // Windows 11 adds round corners and the acrylic backdrop.
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Hexsight")
        .with_inner_size([app::MAIN_W, 380.0])
        .with_min_inner_size([app::MAIN_W, 100.0])
        .with_decorations(false)
        .with_transparent(true);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-64.png")) {
        viewport = viewport.with_icon(icon);
    }
    if let Some([x, y]) = settings.pos {
        viewport = viewport.with_position([x, y]);
    }
    let options = eframe::NativeOptions {
        viewport,
        // Repaint only on events. The worker wakes the UI when data changes.
        run_and_return: false,
        ..Default::default()
    };
    eframe::run_native(
        "Hexsight",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, settings)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_line_has_the_version_the_time_the_thread_and_the_windows_build() {
        let line = crash_line(
            "0.1.0",
            1_792_747_231_762,
            "ocr",
            26200,
            "panicked at src/ocr.rs:10:5:\nindex out of bounds",
        );
        assert_eq!(
            line,
            "2026-10-23T09:20:31Z · Hexsight 0.1.0 · thread ocr · Windows build 26200 · panicked at src/ocr.rs:10:5: | index out of bounds"
        );
        // One crash is one line, whatever the text of the panic holds.
        assert_eq!(line.lines().count(), 1);
        assert_eq!(
            crash_line("0.1.0", 0, "main", 0, "boom\r\nnext\n")
                .lines()
                .count(),
            1
        );
    }

    #[test]
    fn a_thread_without_a_name_still_gives_a_line() {
        let line = crash_line("0.2.0", 1_000, "unnamed", 22621, "boom");
        assert!(
            line.contains("thread unnamed")
                && line.contains("Windows build 22621")
                && line.contains("1970-01-01T00:00:01Z")
        );
    }
}
