//! Read the offered augment cards from the screen.
//!
//! No Riot API gives the offered augments, so we do what the other helpers do:
//! capture the game window, run the Windows built-in OCR (no extra download),
//! and fuzzy-match each text line to the known augment names.
//! Nothing reads game memory. The last capture stays in memory (one image at
//! most). Hexsight writes it to disk only when the user asks for it.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetClientRect, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
};
use windows::core::{HSTRING, w};

#[derive(Clone, Debug)]
pub struct Line {
    pub text: String,
    /// Center, as a fraction of the captured region (0..1).
    pub x: f32,
    pub y: f32,
    pub h: f32,
}

/// A screen area in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crop {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Crop {
    /// The screen pixel at a fraction of the area (0 is the left or top edge,
    /// 1 is the right or bottom edge).
    pub fn at(&self, fx: f32, fy: f32) -> (f32, f32) {
        (
            self.x as f32 + fx * self.w as f32,
            self.y as f32 + fy * self.h as f32,
        )
    }
}

/// One offered card: the augment and where its title line sits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Card {
    pub id: i64,
    /// Center of the title line, as a fraction of the captured region (0..1).
    pub x: f32,
    pub y: f32,
    /// Height of the title line, as a fraction of the captured region.
    pub h: f32,
}

/// Where a scan read the cards, in screen pixels: the game area, and the
/// capture inside it. The card positions are fractions of the capture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub game: Crop,
    pub crop: Crop,
}

/// One capture: the pixels and where they come from.
pub struct Shot {
    /// BGRA rows, top down: `crop.w * crop.h * 4` bytes.
    pub bgra: Vec<u8>,
    pub crop: Crop,
    /// The game area that `crop` sits in: the game window, or the screen.
    pub game: Crop,
    /// True when the area is the game window. False for the screen (F9 fallback).
    pub game_window: bool,
}

impl Shot {
    /// Where this scan read. `None` for the screen fallback: badges need the game window.
    pub fn geometry(&self) -> Option<Geometry> {
        self.game_window.then_some(Geometry {
            game: self.game,
            crop: self.crop,
        })
    }
}

/// What the card reader saw in one scan. The Settings page shows the numbers,
/// and the button "Save the last scan" writes the rest to disk.
pub struct ScanRecord {
    pub at: Instant,
    /// The capture. `None` when nothing could be captured.
    pub shot: Option<Shot>,
    /// Text lines that the engine read, without the lines under the overlay.
    pub lines: Vec<Line>,
    /// Lines that sat under the overlay window. They are not in `lines`.
    pub dropped: usize,
    /// Augments that matched, with the place of the title line of each card.
    pub matched: Vec<Card>,
    /// The OCR engine failed on this capture.
    pub engine_failed: bool,
}

/// Largest side of an OCR bitmap when Windows does not tell us. Some Windows
/// builds allow 2600 pixels. Others allow 10000 (the engine says so).
const FALLBACK_MAX_DIMENSION: u32 = 2600;

pub struct Ocr {
    engine: OcrEngine,
    /// The engine rejects a bitmap with a longer side than this.
    max_side: u32,
    /// Language tag of the engine, for example "en-US".
    language: String,
}

impl Ocr {
    pub fn new() -> Option<Ocr> {
        // English is the OCR engine Windows ships by default. It reads Turkish
        // and other Latin text well enough after folding (see `fold`).
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()
            .ok()
            .or_else(|| {
                Language::CreateLanguage(&HSTRING::from("en-US"))
                    .ok()
                    .and_then(|l| OcrEngine::TryCreateFromLanguage(&l).ok())
            })?;
        // Test hook: HEXSIGHT_OCR_MAX_SIDE=<pixels> acts as a smaller engine limit.
        let max_side = std::env::var("HEXSIGHT_OCR_MAX_SIDE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| OcrEngine::MaxImageDimension().unwrap_or(FALLBACK_MAX_DIMENSION));
        let language = engine
            .RecognizerLanguage()
            .and_then(|l| l.LanguageTag())
            .map(|t| t.to_string_lossy())
            .unwrap_or_else(|_| "unknown".into());
        Some(Ocr {
            engine,
            max_side,
            language,
        })
    }

    /// Language tag of the engine, for example "en-US".
    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn recognize(&self, bgra: &[u8], w: u32, h: u32) -> Option<Vec<Line>> {
        // Windows OCR fails on a bitmap that is too large (4K and ultrawide
        // screens). Halve it until it fits. Text on such screens is big enough.
        let small = shrink_to_fit(bgra, w, h, self.max_side);
        let (bgra, w, h) = match &small {
            Some((px, sw, sh)) => (px.as_slice(), *sw, *sh),
            None => (bgra, w, h),
        };
        let writer = DataWriter::new().ok()?;
        writer.WriteBytes(bgra).ok()?;
        let buf = writer.DetachBuffer().ok()?;
        let bmp = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
            &buf,
            BitmapPixelFormat::Bgra8,
            w as i32,
            h as i32,
            BitmapAlphaMode::Ignore,
        )
        .ok()?;
        let res = self.engine.RecognizeAsync(&bmp).ok()?.join().ok()?;
        let mut out = Vec::new();
        for line in res.Lines().ok()? {
            let text = line.Text().map(|t| t.to_string_lossy()).unwrap_or_default();
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, 0f32, 0f32);
            for word in line.Words().ok()? {
                if let Ok(r) = word.BoundingRect() {
                    x0 = x0.min(r.X);
                    y0 = y0.min(r.Y);
                    x1 = x1.max(r.X + r.Width);
                    y1 = y1.max(r.Y + r.Height);
                }
            }
            if x1 > 0.0 {
                out.push(Line {
                    text,
                    x: (x0 + x1) / 2.0 / w as f32,
                    y: (y0 + y1) / 2.0 / h as f32,
                    h: (y1 - y0) / h as f32,
                });
            }
        }
        Some(out)
    }

    /// Read the title band of a capture (see `TITLE_BAND`), scaled to about
    /// `BAND_HEIGHT` pixels. The lines come back with their places as
    /// fractions of the whole capture, as `recognize` gives them.
    pub fn recognize_band(&self, bgra: &[u8], w: u32, h: u32) -> Option<Vec<Line>> {
        let (top, rows) = band_rows(h);
        if rows < 8 || w < 8 {
            return None;
        }
        let start = top as usize * w as usize * 4;
        let band = bgra.get(start..start + rows as usize * w as usize * 4)?;
        let scale = (BAND_HEIGHT / rows as f32)
            .clamp(0.75, 2.0)
            .min(self.max_side as f32 / w as f32);
        let (nw, nh) = (
            (w as f32 * scale).round() as u32,
            (rows as f32 * scale).round() as u32,
        );
        let img = image::RgbaImage::from_raw(w, rows, band.to_vec())?;
        let big = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
        let lines = self.recognize(big.as_raw(), nw, nh)?;
        Some(
            lines
                .into_iter()
                .map(|l| band_to_capture(l, top, rows, h))
                .collect(),
        )
    }

    /// Read the offered cards in a capture. Windows OCR sometimes gives up on
    /// the whole capture: it finds no text angle and returns no lines, even
    /// with three clear cards on it (4 scans of 5 in a real 1080p game). The
    /// title band alone reads well and fast. So the band goes first, and the
    /// whole capture only when the band does not give two cards. `overlay` is
    /// the screen rect of the overlay while it shows in recordings.
    pub fn read_cards(&self, shot: &Shot, m: &Matcher, overlay: Option<[i32; 4]>) -> Reading {
        let (w, h) = (shot.crop.w as u32, shot.crop.h as u32);
        let read = |lines: Option<Vec<Line>>, from_band: bool| {
            let engine_failed = lines.is_none();
            let mut lines = lines.unwrap_or_default();
            let mut dropped = 0;
            if let Some(rect) = overlay {
                (lines, dropped) = drop_inside(lines, shot.crop, rect);
            }
            let matched = m.find(&lines);
            Reading {
                lines,
                dropped,
                matched,
                engine_failed,
                from_band,
            }
        };
        let band = read(self.recognize_band(&shot.bgra, w, h), true);
        if band.matched.len() >= 2 {
            return band;
        }
        let whole = read(self.recognize(&shot.bgra, w, h), false);
        if whole.matched.len() >= band.matched.len() {
            whole
        } else {
            band
        }
    }
}

/// What `Ocr::read_cards` read in one capture.
pub struct Reading {
    /// Text lines, without the lines under the overlay.
    pub lines: Vec<Line>,
    /// Lines that sat under the overlay window.
    pub dropped: usize,
    /// Augments that matched, left to right.
    pub matched: Vec<Card>,
    /// The OCR engine failed on the capture.
    pub engine_failed: bool,
    /// The title band alone gave this reading (not the whole capture). Only
    /// such a reading may be reused for an equal band (see `band_signature`).
    pub from_band: bool,
}

/// The card titles sit in this band of the game area, as fractions of its
/// height: at 41 % in a 1080p game window, at 46 % in a 1417x843 screenshot.
const TITLE_BAND: (f32, f32) = (0.33, 0.52);

/// The band is scaled to about this height in pixels before OCR: 1.5 times
/// on a 1080p game. The engine then reads "Genel" and not "Cenel".
const BAND_HEIGHT: f32 = 308.0;

/// The rows of a capture that hold the title band: the first row and the
/// number of rows. `crop_h` is the height of a capture from `crop_of`, which
/// starts at 12 % of the game height and holds 66 % of it.
fn band_rows(crop_h: u32) -> (u32, u32) {
    let at = |f: f32| {
        ((f - 0.12) / 0.66 * crop_h as f32)
            .round()
            .clamp(0.0, crop_h as f32) as u32
    };
    let (top, bottom) = (at(TITLE_BAND.0), at(TITLE_BAND.1));
    (top, bottom.saturating_sub(top))
}

/// A line read in the band (`rows` rows from row `top`), with its place as a
/// fraction of the whole capture (`h` rows).
fn band_to_capture(l: Line, top: u32, rows: u32, h: u32) -> Line {
    let (t, r) = (top as f32 / h as f32, rows as f32 / h as f32);
    Line {
        y: t + l.y * r,
        h: l.h * r,
        ..l
    }
}

/// Columns and rows of the small grid that `band_signature` averages the band to.
const SIG_COLS: usize = 128;
const SIG_ROWS: usize = 24;
/// Each cell mean loses its low 3 bits (32 levels). A slow glow of a few levels
/// in a flat area does not change the signature. A changed title does: a title
/// is light text on a dark body, so the cells under it move by tens of levels.
/// A cell at the edge of a title can cross a step by chance. That is a safe
/// miss: the band is read again. The risk to avoid is the other one, a stale
/// reading, and `SIG_REUSE_MAX` in `worker.rs` limits it.
const SIG_SHIFT: u32 = 3;

/// A short hash of a small copy of the title band (see `TITLE_BAND`). The band
/// is averaged to a `SIG_COLS` x `SIG_ROWS` grid, and each cell loses its low
/// bits. Two captures of the same pick screen give the same signature, so the
/// reader can reuse the last result and skip OCR. A reroll or a pick changes
/// the text, so it gives another signature. The cost is one pass over a quarter
/// of the band pixels (0.21 ms in a release build, 23 ms in a debug build),
/// against about 90 ms for an OCR read.
/// `None` for a capture that is too small to judge.
pub fn band_signature(bgra: &[u8], w: u32, h: u32) -> Option<u64> {
    let (top, rows) = band_rows(h);
    let (w, top, rows) = (w as usize, top as usize, rows as usize);
    if w < SIG_COLS * 2 || rows < SIG_ROWS * 2 || bgra.len() < (top + rows) * w * 4 {
        return None;
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for gy in 0..SIG_ROWS {
        let (y0, y1) = (top + rows * gy / SIG_ROWS, top + rows * (gy + 1) / SIG_ROWS);
        for gx in 0..SIG_COLS {
            let (x0, x1) = (w * gx / SIG_COLS, w * (gx + 1) / SIG_COLS);
            let (mut sum, mut n) = (0u32, 0u32);
            // Every second pixel of every second row is enough.
            for y in (y0..y1).step_by(2) {
                for px in bgra[(y * w + x0) * 4..(y * w + x1) * 4]
                    .chunks_exact(4)
                    .step_by(2)
                {
                    sum += px[0] as u32 + px[1] as u32 + px[2] as u32;
                    n += 3;
                }
            }
            let mean = sum.checked_div(n).unwrap_or(0);
            hash ^= (mean >> SIG_SHIFT) as u64;
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    Some(hash)
}

/// Screen rect of the League game client area. `None` without a game window,
/// or while it is minimized.
fn game_window_rect() -> Option<RECT> {
    unsafe {
        let hwnd = FindWindowW(w!("RiotWindowClass"), w!("League of Legends (TM) Client")).ok()?;
        if hwnd.is_invalid() {
            return None;
        }
        let mut r = RECT::default();
        if GetClientRect(hwnd, &mut r).is_err() || r.right <= 0 {
            return None;
        }
        let mut p = POINT { x: 0, y: 0 };
        let _ = ClientToScreen(hwnd, &mut p);
        Some(RECT {
            left: p.x,
            top: p.y,
            right: p.x + r.right,
            bottom: p.y + r.bottom,
        })
    }
}

/// Which desktop gets the input now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Desk {
    /// The normal desktop of this session.
    Normal,
    /// The lock screen, a secure prompt (UAC), a screen saver, or a remote
    /// session that is disconnected. A screen capture shows nothing there.
    Locked,
    /// The normal desktop, but inside a remote desktop session. Games often
    /// give a black image there.
    Remote,
}

/// The desktop state from three facts: could Hexsight open the input desktop,
/// what is its name (when known), and does this session run on a remote desktop.
/// The normal desktop is called "Default".
fn desk_from(opened: bool, name: Option<&str>, remote: bool) -> Desk {
    if !opened || name.is_some_and(|n| !n.eq_ignore_ascii_case("Default")) {
        Desk::Locked
    } else if remote {
        Desk::Remote
    } else {
        Desk::Normal
    }
}

/// Ask Windows which desktop gets the input. While the screen is locked, or a
/// secure prompt shows, the input desktop belongs to the system and a normal
/// program cannot open it. This is a few cheap calls (microseconds).
pub fn desk_state() -> Desk {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::StationsAndDesktops::{
        CloseDesktop, DESKTOP_ACCESS_FLAGS, DESKTOP_CONTROL_FLAGS, GetUserObjectInformationW,
        OpenInputDesktop, UOI_NAME,
    };
    use windows::Win32::UI::WindowsAndMessaging::SM_REMOTESESSION;
    unsafe {
        let remote = GetSystemMetrics(SM_REMOTESESSION) != 0;
        // DESKTOP_READOBJECTS (1) for the name, and DESKTOP_SWITCHDESKTOP (0x100),
        // which the system desktop refuses to a normal program.
        let Ok(desk) =
            OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_ACCESS_FLAGS(0x101))
        else {
            return desk_from(false, None, remote);
        };
        let mut buf = [0u16; 64];
        let mut need = 0u32;
        let got = GetUserObjectInformationW(
            HANDLE(desk.0),
            UOI_NAME,
            Some(buf.as_mut_ptr().cast()),
            std::mem::size_of_val(&buf) as u32,
            Some(&mut need),
        );
        let _ = CloseDesktop(desk);
        let name = got.is_ok().then(|| {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            String::from_utf16_lossy(&buf[..len])
        });
        desk_from(true, name.as_deref(), remote)
    }
}

/// True while the League game window exists and shows. The test hook
/// HEXSIGHT_SCAN_IMAGE replaces the whole screen, so it counts as a game window.
pub fn game_window_found() -> bool {
    std::env::var_os("HEXSIGHT_SCAN_IMAGE").is_some() || game_window_rect().is_some()
}

/// The game area, and whether it is the game window (true) or the primary
/// screen (false). The screen is a fallback for F9 only, so automatic scans
/// never read the desktop.
fn game_rect(fallback: bool) -> Option<(RECT, bool)> {
    if let Some(r) = game_window_rect() {
        return Some((r, true));
    }
    fallback.then(|| unsafe {
        (
            RECT {
                left: 0,
                top: 0,
                right: GetSystemMetrics(SM_CXSCREEN),
                bottom: GetSystemMetrics(SM_CYSCREEN),
            },
            false,
        )
    })
}

/// The cards sit in the middle of the screen: x 10-90 %, y 12-78 % of the game area.
pub fn crop_of(left: i32, top: i32, width: i32, height: i32) -> Crop {
    Crop {
        x: left + width / 10,
        y: top + height * 12 / 100,
        w: width * 8 / 10,
        h: height * 66 / 100,
    }
}

/// Capture the middle of the game area (see `crop_of`). Without a game window,
/// capture the primary screen only when `fallback` is set.
pub fn capture_shot(fallback: bool) -> Option<Shot> {
    // Test hook: HEXSIGHT_SCAN_IMAGE=<png> acts as the screen.
    if let Some(path) = std::env::var_os("HEXSIGHT_SCAN_IMAGE") {
        return image::open(path)
            .ok()
            .map(|img| shot_from_image(&img.to_rgba8()));
    }
    let (r, game_window) = game_rect(fallback)?;
    let (gw, gh) = (r.right - r.left, r.bottom - r.top);
    if gw < 200 || gh < 200 {
        return None;
    }
    let crop = crop_of(r.left, r.top, gw, gh);
    let bgra = capture(crop.x, crop.y, crop.w, crop.h)?;
    Some(Shot {
        bgra,
        crop,
        game: Crop {
            x: r.left,
            y: r.top,
            w: gw,
            h: gh,
        },
        game_window,
    })
}

/// A capture from an image of the whole screen, with the crop of the live
/// capture. The test hooks use it.
pub fn shot_from_image(img: &image::RgbaImage) -> Shot {
    let (iw, ih) = img.dimensions();
    let crop = crop_of(0, 0, iw as i32, ih as i32);
    let mut bgra = Vec::with_capacity((crop.w * crop.h * 4) as usize);
    for y in crop.y..crop.y + crop.h {
        for x in crop.x..crop.x + crop.w {
            let p = img.get_pixel(x as u32, y as u32).0;
            bgra.extend_from_slice(&[p[2], p[1], p[0], 255]);
        }
    }
    Shot {
        bgra,
        crop,
        game: Crop {
            x: 0,
            y: 0,
            w: iw as i32,
            h: ih as i32,
        },
        game_window: true,
    }
}

pub fn capture(x: i32, y: i32, w: i32, h: i32) -> Option<Vec<u8>> {
    unsafe {
        let screen = GetDC(Some(HWND::default()));
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp.into());
        let ok = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY).is_ok();
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = w;
        info.bmiHeader.biHeight = -h; // top-down rows
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        let mut buf = vec![0u8; (w * h * 4) as usize];
        // GetDIBits needs the bitmap out of the DC first.
        SelectObject(mem, old);
        let lines = if ok {
            GetDIBits(
                mem,
                bmp,
                0,
                h as u32,
                Some(buf.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(Some(HWND::default()), screen);
        (lines == h).then_some(buf)
    }
}

/// Halve a BGRA image. Each new pixel is the average of a 2x2 block. An odd
/// last row or column is dropped. Returns the pixels, the width and the height.
fn downsample2(src: &[u8], w: usize, h: usize) -> (Vec<u8>, usize, usize) {
    let (nw, nh) = (w / 2, h / 2);
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        let top = 2 * y * w * 4;
        let bottom = top + w * 4;
        for x in 0..nw {
            let (a, b) = (top + 8 * x, bottom + 8 * x);
            let o = (y * nw + x) * 4;
            for c in 0..4 {
                let sum = src[a + c] as u32
                    + src[a + 4 + c] as u32
                    + src[b + c] as u32
                    + src[b + 4 + c] as u32;
                out[o + c] = ((sum + 2) / 4) as u8;
            }
        }
    }
    (out, nw, nh)
}

/// Halve the image until both sides are at most `max` pixels. Returns `None`
/// when the image fits already (or the buffer is too short), so the caller
/// keeps its own buffer.
fn shrink_to_fit(bgra: &[u8], w: u32, h: u32, max: u32) -> Option<(Vec<u8>, u32, u32)> {
    if (w <= max && h <= max) || w < 2 || h < 2 || bgra.len() < w as usize * h as usize * 4 {
        return None;
    }
    let (mut px, mut w, mut h) = downsample2(bgra, w as usize, h as usize);
    while (w > max as usize || h > max as usize) && w >= 2 && h >= 2 {
        (px, w, h) = downsample2(&px, w, h);
    }
    Some((px, w as u32, h as u32))
}

// ---------------------------------------------------------------------------
// Own windows and saved scans. Pure functions, unit tested below.

/// Screen pixel of the center of a line in a capture.
fn line_pixel(l: &Line, crop: Crop) -> (i32, i32) {
    let (x, y) = crop.at(l.x, l.y);
    (x.round() as i32, y.round() as i32)
}

/// Drop the lines whose center falls inside `rect` (left, top, right, bottom,
/// in screen pixels). When the overlay shows in recordings, the reader would
/// else read the text of its own overlay. Returns the lines that stay and the
/// number of lines dropped. The right and the bottom edge are outside.
pub fn drop_inside(lines: Vec<Line>, crop: Crop, rect: [i32; 4]) -> (Vec<Line>, usize) {
    let [left, top, right, bottom] = rect;
    let before = lines.len();
    let kept: Vec<Line> = lines
        .into_iter()
        .filter(|l| {
            let (x, y) = line_pixel(l, crop);
            !(x >= left && x < right && y >= top && y < bottom)
        })
        .collect();
    let dropped = before - kept.len();
    (kept, dropped)
}

/// True when `rect` (left, top, right, bottom, in screen pixels) covers a part
/// of the capture area.
pub fn crop_overlaps(crop: Crop, rect: [i32; 4]) -> bool {
    let [left, top, right, bottom] = rect;
    left < crop.x + crop.w && right > crop.x && top < crop.y + crop.h && bottom > crop.y
}

/// The text file of a saved scan. `facts` are the rows of the Settings page
/// (label and value). `name_of` gives the name of an augment id.
pub fn scan_text(
    rec: &ScanRecord,
    facts: &[(String, String)],
    name_of: &dyn Fn(i64) -> String,
) -> String {
    let mut out = String::from("Hexsight last scan\n");
    for (label, value) in facts {
        out.push_str(&format!("{label}: {value}\n"));
    }
    match &rec.shot {
        Some(s) => {
            let c = s.crop;
            let what = if s.game_window {
                "game window"
            } else {
                "screen"
            };
            out.push_str(&format!(
                "Capture: x {} y {} width {} height {} ({what})\n",
                c.x, c.y, c.w, c.h
            ));
        }
        None => out.push_str("Capture: none (no game window, or the capture failed)\n"),
    }
    if rec.engine_failed {
        out.push_str("The OCR engine failed on this capture.\n");
    }
    out.push_str("\nx and y are fractions of the capture, from 0 to 1.\n");
    out.push_str(&format!("Lines ({}):\n", rec.lines.len()));
    for l in &rec.lines {
        out.push_str(&format!("  [{:.2}, {:.2}] {}\n", l.x, l.y, l.text));
    }
    if rec.dropped > 0 {
        out.push_str(&format!(
            "{} more lines under the overlay are not listed.\n",
            rec.dropped
        ));
    }
    out.push_str(&format!("\nNames matched ({}):\n", rec.matched.len()));
    for c in &rec.matched {
        out.push_str(&format!(
            "  {} {} (x {:.2}, y {:.2})\n",
            c.id,
            name_of(c.id),
            c.x,
            c.y
        ));
    }
    out
}

/// Write `last-scan.png` (the capture) and `last-scan.txt` in `dir`. Without a
/// capture, an old PNG goes away, so the two files always match.
pub fn save_scan(dir: &Path, rec: &ScanRecord, text: &str) -> Result<(), String> {
    let png = dir.join("last-scan.png");
    match &rec.shot {
        Some(s) => {
            let rgb: Vec<u8> = s
                .bgra
                .chunks_exact(4)
                .flat_map(|p| [p[2], p[1], p[0]])
                .collect();
            let img = image::RgbImage::from_raw(s.crop.w as u32, s.crop.h as u32, rgb)
                .ok_or("the capture has a wrong size")?;
            let tmp = dir.join("last-scan.tmp");
            img.save_with_format(&tmp, image::ImageFormat::Png)
                .map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &png).map_err(|e| e.to_string())?;
        }
        None => {
            let _ = std::fs::remove_file(&png);
        }
    }
    // The BOM tells each Windows editor that the text is UTF-8. Names such as
    // "Güm ve Bas" then read right.
    std::fs::write(dir.join("last-scan.txt"), format!("\u{feff}{text}")).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Matching. Pure functions, unit tested below.

/// Lower case, remove accents and Turkish letter forms, keep letters and digits.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        let c = match c {
            'ı' | 'î' | 'ï' | 'í' | 'ì' | 'i' => 'i',
            'ş' | 'ș' => 's',
            'ğ' => 'g',
            'ü' | 'û' | 'ú' | 'ù' => 'u',
            'ö' | 'ô' | 'ó' | 'ò' => 'o',
            'ç' => 'c',
            'â' | 'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            c => c,
        };
        if c.is_alphanumeric() {
            out.push(c);
        }
    }
    out
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Similarity 0..1 between two folded strings.
pub fn similarity(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let n = a.len().max(b.len());
    if n == 0 {
        return 0.0;
    }
    1.0 - levenshtein(&a, &b) as f32 / n as f32
}

/// Fold, then merge letters the English OCR engine confuses on game fonts:
/// l/1/j/! -> i, m -> n, q -> c, 0 -> o, 5 -> s. Before the fold, the signs
/// that the engine reads for Turkish letters become letters: € -> c (ç), $ and
/// # -> s (ş). Example: "Planı" reads as "Plam", "Güç" as "Giiq", "Kaçış" as
/// "Ka€l#". Both sides of a match go through this.
pub fn shape(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| match c {
            '€' | '¢' => 'c',
            '$' | '#' => 's',
            c => c,
        })
        .collect();
    fold(&s)
        .chars()
        .map(|c| match c {
            'l' | '1' | 'j' | '!' | '|' => 'i',
            'm' => 'n',
            'q' => 'c',
            '0' => 'o',
            '5' => 's',
            c => c,
        })
        .collect()
}

pub struct Matcher {
    /// (shaped name, augment id). Local and English names.
    names: Vec<(String, i64)>,
}

impl Matcher {
    pub fn new<'a>(names: impl Iterator<Item = (i64, &'a str)>) -> Matcher {
        let mut v: Vec<(String, i64)> = names
            .map(|(id, n)| (shape(n), id))
            .filter(|(n, _)| n.len() >= 3)
            .collect();
        v.sort();
        v.dedup();
        Matcher { names: v }
    }

    fn best(&self, text: &str) -> Option<(i64, f32)> {
        self.best_with(text, None, &[])
    }

    /// The best name for `text`, without the ids in `skip`. `floor` replaces
    /// the usual bar (by name length) when it is set.
    fn best_with(&self, text: &str, floor: Option<f32>, skip: &[i64]) -> Option<(i64, f32)> {
        let t = shape(text);
        if t.len() < 3 {
            return None;
        }
        let max_gap = if floor.is_some() { 0.5 } else { 0.35 };
        let mut best: Option<(i64, f32)> = None;
        for (name, id) in &self.names {
            // Skip far length differences early. Titles are whole lines.
            let (ln, lt) = (name.chars().count() as f32, t.chars().count() as f32);
            if (ln - lt).abs() / ln.max(lt) > max_gap || skip.contains(id) {
                continue;
            }
            let s = similarity(&t, name);
            // Short names must be near exact. Long names allow more OCR noise.
            let need = floor.unwrap_or(match ln as usize {
                0..7 => 0.9,
                7..15 => 0.78,
                _ => 0.72,
            });
            if s >= need && best.is_none_or(|b| s > b.1) {
                best = Some((*id, s));
            }
        }
        best
    }

    /// A pick screen shows three cards. With two found, the title of the third
    /// sits in the same row, in the free column. OCR often garbles a short
    /// title ("Taşkın" reads as "Ta$an"), so a line at that place gets the
    /// lower bar `THIRD_CARD_BAR`. The place keeps the lower bar safe: the
    /// other text of the cards sits in other rows.
    fn third_card(&self, lines: &[Line], two: &[Card]) -> Option<Card> {
        let (a, b) = (two[0], two[1]);
        lines
            .iter()
            .map(|l| Card {
                id: 0,
                x: l.x,
                y: l.y,
                h: l.h,
            })
            .zip(lines)
            .filter(|(c, _)| {
                on_one_row(c, &a)
                    && on_one_row(c, &b)
                    && (c.x - a.x).abs() >= 0.12
                    && (c.x - b.x).abs() >= 0.12
            })
            .filter_map(|(c, l)| {
                self.best_with(&l.text, Some(THIRD_CARD_BAR), &[a.id, b.id])
                    .map(|(id, s)| (Card { id, ..c }, s))
            })
            .max_by(|x, y| x.1.total_cmp(&y.1))
            .map(|h| h.0)
    }

    /// Offered augments, left to right, at most 3. Each card comes with the
    /// place of its title line (fractions of the captured area). A title on two
    /// lines gives the place of its first line.
    pub fn find(&self, lines: &[Line]) -> Vec<Card> {
        // (card, score)
        let mut hits: Vec<(Card, f32)> = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            let mut cands = vec![self.best(&l.text).map(|(id, s)| {
                (
                    Card {
                        id,
                        x: l.x,
                        y: l.y,
                        h: l.h,
                    },
                    s,
                )
            })];
            // Long titles can wrap to two lines on a card. Keep the better reading.
            if let Some(n) = lines.get(i + 1)
                && (n.x - l.x).abs() < 0.08
                && (n.y - l.y).abs() < l.h * 2.5
            {
                cands.push(self.best(&format!("{} {}", l.text, n.text)).map(|(id, s)| {
                    (
                        Card {
                            id,
                            x: (l.x + n.x) / 2.0,
                            y: l.y,
                            h: l.h,
                        },
                        s,
                    )
                }));
            }
            if let Some(h) = cands
                .into_iter()
                .flatten()
                .max_by(|a, b| a.1.total_cmp(&b.1))
            {
                hits.push(h);
            }
        }
        // One augment per card column: keep the best score per id, then per column.
        let mut by_id: HashMap<i64, (Card, f32)> = HashMap::new();
        for (card, s) in hits {
            let e = by_id.entry(card.id).or_insert((card, s));
            if s > e.1 {
                *e = (card, s);
            }
        }
        let mut v: Vec<(Card, f32)> = by_id.into_values().collect();
        v.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        let mut cols: Vec<(Card, f32)> = Vec::new();
        for h in v {
            match cols.last_mut() {
                Some(last) if (h.0.x - last.0.x).abs() < 0.1 => {
                    if h.1 > last.1 {
                        *last = h;
                    }
                }
                _ => cols.push(h),
            }
        }
        let mut row: Vec<Card> = same_row(cols).into_iter().take(3).map(|h| h.0).collect();
        if row.len() == 2
            && let Some(third) = self.third_card(lines, &row)
        {
            row.push(third);
            row.sort_by(|a, b| a.x.total_cmp(&b.x));
        }
        row
    }
}

/// The similarity that a line in the free column of the title row needs to
/// be the third card (see `Matcher::third_card`).
const THIRD_CARD_BAR: f32 = 0.6;

/// True when two title lines sit in one row: their y centers differ by no more
/// than one title height. The three cards of a pick screen show their titles in
/// one row.
fn on_one_row(a: &Card, b: &Card) -> bool {
    (a.y - b.y).abs() <= a.h.max(b.h)
}

/// Keep the cards that share one row. Two chat lines that hold augment names,
/// or the main window of Hexsight over the game, give names at different heights.
/// They are no pick screen. With several rows, the row with the most cards wins,
/// and then the row with the better scores. The cards keep their order (left to
/// right). One card alone is no row: the caller needs two cards at least.
fn same_row(cols: Vec<(Card, f32)>) -> Vec<(Card, f32)> {
    let mut best: Vec<usize> = Vec::new();
    let mut best_score = f32::MIN;
    for anchor in &cols {
        let row: Vec<usize> = (0..cols.len())
            .filter(|&i| on_one_row(&anchor.0, &cols[i].0))
            .collect();
        let score: f32 = row.iter().map(|&i| cols[i].1).sum();
        if row.len() > best.len() || (row.len() == best.len() && score > best_score) {
            best = row;
            best_score = score;
        }
    }
    cols.into_iter()
        .enumerate()
        .filter(|(i, _)| best.contains(i))
        .map(|(_, c)| c)
        .collect()
}

/// Mean brightness of a BGRA capture: 0 (black) to 255 (white). It reads every
/// 8th pixel, so a 4 MB capture costs a few microseconds.
pub fn mean_brightness(bgra: &[u8]) -> f32 {
    let (mut sum, mut n) = (0u64, 0u64);
    for px in bgra.chunks_exact(4).step_by(8) {
        sum += px[0] as u64 + px[1] as u64 + px[2] as u64;
        n += 3;
    }
    if n == 0 { 0.0 } else { sum as f32 / n as f32 }
}

/// A short hash of a BGRA capture (FNV-1a over every 61st byte). A frozen
/// capture gives the same hash for each scan. A running game never does: its
/// minimap and its clock change all the time.
pub fn capture_hash(bgra: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bgra.iter().step_by(61) {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Distance between the centers of two cards, in pixels of a 1080-high game.
/// The cards are centered and scale with the game height.
const CARD_PITCH: f32 = 368.0;
/// From the center of a card to the middle of its light frame, and to a point
/// just inside the dark body next to the frame (same unit).
const CARD_RIM: f32 = 149.0;
const CARD_INNER: f32 = 118.0;
/// Rows that cross the frame of a card between the corner decorations, and
/// the two empty rows of the body under the description text (fractions of
/// the game height).
const RIM_ROWS: [f32; 6] = [0.30, 0.36, 0.42, 0.48, 0.54, 0.60];
const BODY_ROWS: [f32; 2] = [0.585, 0.600];
const BODY_DX: [f32; 7] = [-90.0, -60.0, -30.0, 0.0, 30.0, 60.0, 90.0];
/// A card shows when this many of its 14 body points have the body color,
/// or this many of its 12 frame points are lighter than the body next to them.
const BODY_HITS: usize = 8;
const RIM_HITS: usize = 7;
/// Two cards are enough: a tooltip or a flip can hide the third.
const CARDS_NEEDED: usize = 2;

/// The body of a card is a flat dark teal (about r 1, g 15, b 16). The same
/// color, dimmed, shows while the screen fades in.
fn is_card_body(r: i32, g: i32, b: i32) -> bool {
    r <= 12 && (7..=32).contains(&g) && (7..=36).contains(&b) && (g - b).abs() <= 7 && g > r + 4
}

/// Cheap check on a capture from `crop_of` (BGRA): do the three augment cards
/// stand in the middle? It reads 114 points (per card 14 body points and 12
/// frame pairs) and takes a few microseconds, so the heartbeat scan runs it
/// before OCR and skips OCR on a frame without cards (see `worker::ocr_loop`).
///
/// A card shows by one of two signs. The first is the flat dark teal body. The
/// second is the light frame, which stays while the game plays a gold, flip
/// or fade animation over the body. Frames that are not sure give `true`: a
/// wrong yes costs one OCR pass, a wrong no costs a pick screen.
pub fn cards_visible(bgra: &[u8], w: u32, h: u32) -> bool {
    let (w, h) = (w as usize, h as usize);
    // A capture that is not as `crop_of` makes it: do not judge.
    if w < 64 || h < 64 || bgra.len() < w * h * 4 {
        return true;
    }
    // The capture holds 66 % of the game height, and the cards scale with it.
    let game_h = h as f32 / 0.66;
    let unit = game_h / 1080.0;
    let cx = w as f32 / 2.0;
    let rgb = |card: f32, dx: f32, row: f32| -> Option<[i32; 3]> {
        let x = (cx + (card * CARD_PITCH + dx) * unit).round();
        let y = ((row - 0.12) * game_h).round();
        if x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32 {
            return None;
        }
        let i = (y as usize * w + x as usize) * 4;
        Some([bgra[i + 2] as i32, bgra[i + 1] as i32, bgra[i] as i32])
    };
    let lum = |p: [i32; 3]| (p[0] * 77 + p[1] * 150 + p[2] * 29) >> 8;
    let mut cards = 0;
    for card in [-1.0, 0.0, 1.0] {
        let body = BODY_ROWS
            .iter()
            .flat_map(|&row| BODY_DX.iter().map(move |&dx| (dx, row)))
            .filter(|&(dx, row)| rgb(card, dx, row).is_some_and(|p| is_card_body(p[0], p[1], p[2])))
            .count();
        let rim = RIM_ROWS
            .iter()
            .flat_map(|&row| [-1.0, 1.0].map(move |side| (side, row)))
            .filter(|&(side, row)| {
                match (
                    rgb(card, side * CARD_RIM, row),
                    rgb(card, side * CARD_INNER, row),
                ) {
                    (Some(rim), Some(inner)) => lum(rim) - lum(inner) >= 25,
                    _ => false,
                }
            })
            .count();
        if body >= BODY_HITS || rim >= RIM_HITS {
            cards += 1;
        }
    }
    cards >= CARDS_NEEDED
}

/// A capture with a mean brightness under this value is black.
pub const BLACK_BELOW: f32 = 3.0;

/// This many scans with the same hash, while the game level rises, mean that
/// the capture is frozen.
pub const FROZEN_SCANS: u32 = 5;

/// How the capture looks: fine, black, frozen, or not possible (locked screen,
/// remote desktop).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaptureHealth {
    #[default]
    Fine,
    /// Windows gave a black image (some flip-model DirectX games do this to a
    /// screen capture that is not Borderless).
    Black,
    /// The image does not change while the game goes on.
    Frozen,
    /// The screen is locked, or a secure prompt covers it (see `Desk::Locked`).
    /// There is nothing to capture, so the reader waits.
    Locked,
    /// The image is black and the session is a remote desktop. Borderless would not help.
    Remote,
}

/// The capture state from the desktop and from the image check. The desktop
/// comes first: on a locked screen the image says nothing. In a remote session
/// only a black image is blamed on the remote desktop.
pub fn capture_state(desk: Desk, image: CaptureHealth) -> CaptureHealth {
    match (desk, image) {
        (Desk::Locked, _) => CaptureHealth::Locked,
        (Desk::Remote, CaptureHealth::Black) => CaptureHealth::Remote,
        (_, h) => h,
    }
}

/// Judges the scans of one game. Feed it each capture of the game window with
/// the level of the game (`CaptureWatch::step`).
#[derive(Default)]
pub struct CaptureWatch {
    hash: Option<u64>,
    /// Scans in a row with the same hash (the first one counts).
    same: u32,
    /// The game level at the first scan of this run of the same hash.
    level_at_start: i64,
    health: CaptureHealth,
}

impl CaptureWatch {
    /// One scan: the pixels (BGRA) and the level of the game. Returns the
    /// health after this scan. A black image shows at once. A frozen image
    /// shows after `FROZEN_SCANS` scans with the same hash, and only when the
    /// level went up meanwhile: a pick screen is still for a few seconds on purpose.
    pub fn step(&mut self, bgra: &[u8], level: i64) -> CaptureHealth {
        if mean_brightness(bgra) < BLACK_BELOW {
            self.hash = None;
            self.same = 0;
            self.health = CaptureHealth::Black;
            return self.health;
        }
        let hash = capture_hash(bgra);
        if self.hash == Some(hash) {
            self.same += 1;
        } else {
            self.hash = Some(hash);
            self.same = 1;
            self.level_at_start = level;
        }
        self.health = if self.same >= FROZEN_SCANS && level > self.level_at_start {
            CaptureHealth::Frozen
        } else {
            CaptureHealth::Fine
        };
        self.health
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, x: f32, y: f32) -> Line {
        Line {
            text: text.into(),
            x,
            y,
            h: 0.03,
        }
    }

    /// Real Windows OCR lines from the in-game ARAM Mayhem pick screen
    /// (wiki screenshot rebuilt at 1080p, 2026-10-24). Only the text is kept.
    #[test]
    fn reads_the_real_pick_screen() {
        let m = Matcher::new(
            [
                (1079, "Symphony of War"),
                (1058, "Mystic Punch"),
                (1343, "Sneakerhead"),
                (1001, "Titan's Resolve"),
                (1205, "ADAPt"),
            ]
            .into_iter(),
        );
        let lines = vec![
            line("Symphony of War", 0.19, 0.51),
            line("Damage Speed", 0.19, 0.56),
            line("Cain the Lethal Tempo and Conqueror", 0.19, 0.64),
            line("Keystone Runes.", 0.19, 0.67),
            line("Mystic Punch", 0.50, 0.51),
            line("Speed", 0.50, 0.56),
            line("On-Hit reduce the cooldown of your", 0.51, 0.64),
            line("basic abilities by 20% of their remaining", 0.50, 0.67),
            line("cooldown.", 0.50, 0.70),
            line("Sneakerhead", 0.81, 0.51),
            line("Speed", 0.81, 0.56),
            line("Immediate: Cain a random pair of upgraded", 0.81, 0.64),
            line("boots.", 0.81, 0.67),
            line("Requirement: Complete its quest to swap", 0.81, 0.74),
            line("for another.", 0.81, 0.76),
            line("Reward: Jarvan I's Boot", 0.81, 0.83),
        ];
        let ids: Vec<i64> = m.find(&lines).iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![1079, 1058, 1343]);
    }

    /// The augment ids of a `find` result, left to right.
    fn ids(found: Vec<Card>) -> Vec<i64> {
        found.into_iter().map(|c| c.id).collect()
    }

    #[test]
    fn folds_turkish() {
        assert_eq!(fold("Kılıç Dansı!"), "kilicdansi");
        assert_eq!(fold("İŞÇİ Güç"), "isciguc");
    }

    #[test]
    fn reads_turkish_letters_as_the_engine_sees_them() {
        // "Kaçış Planı" on a real 1080p card, read in the title band.
        assert_eq!(shape("Ka€l# Plant"), "kacispiant");
        let m = Matcher::new(
            [
                (9001, "Kaçış Planı"),
                (2016, "Doping"),
                (3051, "Mavi Bariyer"),
            ]
            .into_iter(),
        );
        let lines = vec![
            line("Doping", 0.26, 0.44),
            line("Mavi Bariyer", 0.50, 0.44),
            line("Ka€l# Plant", 0.74, 0.44),
        ];
        assert_eq!(ids(m.find(&lines)), vec![2016, 3051, 9001]);
    }

    #[test]
    fn the_third_card_gets_a_lower_bar_in_its_place() {
        let m = Matcher::new(
            [
                (1002, "Dâhi Mucit"),
                (1141, "Hepsi Senin İçin"),
                (9002, "Taşkın"),
                (9003, "Hasar Kalkanı"),
            ]
            .into_iter(),
        );
        // A real 1080p card row: "Taşkın" reads as "Ta$an" (5 letters, so the usual bar is 0.9).
        let mut lines = vec![
            line("Dâhi Mucit", 0.26, 0.44),
            line("Hepsi Senin İçin", 0.50, 0.44),
            line("Ta$an", 0.74, 0.44),
            line("H asar", 0.74, 0.48),
            line("Mana bedelleri iki katlna qjkar.", 0.74, 0.54),
        ];
        assert_eq!(ids(m.find(&lines)), vec![1002, 1141, 9002]);
        // The same text in another row (a description line) is no title.
        lines[2].y = 0.60;
        assert_eq!(ids(m.find(&lines)), vec![1002, 1141]);
        // In the row, but in the column of a found card: not the third card.
        lines[2] = line("Ta$an", 0.30, 0.44);
        assert_eq!(ids(m.find(&lines)), vec![1002, 1141]);
    }

    #[test]
    fn the_title_band_holds_the_titles() {
        // A 1080p game window: the capture is 712 rows from row 129, and the
        // titles sit at row 444 (41 %).
        let (top, rows) = band_rows(712);
        assert!(top < 444 - 129 && 444 - 129 < top + rows, "{top} {rows}");
        // A 1417x843 screenshot: 556 rows from row 101, titles at row 388 (46 %).
        let (top2, rows2) = band_rows(556);
        assert!(
            top2 < 388 - 101 && 388 - 101 < top2 + rows2,
            "{top2} {rows2}"
        );
        // A line in the middle of the band is in the middle of the band in the capture.
        let l = band_to_capture(line("Doping", 0.26, 0.5), top, rows, 712);
        assert!((l.y - (top as f32 + rows as f32 / 2.0) / 712.0).abs() < 1e-4);
        assert_eq!(l.x, 0.26);
        assert!((l.h - 0.03 * rows as f32 / 712.0).abs() < 1e-6);
        // Too small to read.
        assert_eq!(band_rows(0), (0, 0));
    }

    #[test]
    fn matches_noisy_ocr_and_orders_by_x() {
        let m = Matcher::new(
            [
                (1, "Titan's Resolve"),
                (2, "Kılıç Dansı"),
                (3, "Goliath"),
                (4, "Apex Inventor"),
                (5, "ADAPt"),
            ]
            .into_iter(),
        );
        let lines = vec![
            line("Gain 10 Ability Haste and deal more damage", 0.5, 0.6),
            line("Kilic Dansi", 0.5, 0.3),
            line("Titans Resolue", 0.2, 0.3),
            line("Apex lnventor", 0.8, 0.3),
            line("Goliath grows", 0.2, 0.7),
        ];
        let found = m.find(&lines);
        assert_eq!(
            found.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![1, 2, 4]
        );
        // Each card keeps the x center of its title.
        for (c, x) in found.iter().zip([0.2, 0.5, 0.8]) {
            assert!((c.x - x).abs() < 0.001, "x {} for augment {}", c.x, c.id);
        }
    }

    #[test]
    fn each_card_keeps_the_y_and_the_height_of_its_title() {
        let m = Matcher::new(
            [
                (1, "Titan's Resolve"),
                (2, "Kılıç Dansı"),
                (4, "Apex Inventor"),
            ]
            .into_iter(),
        );
        let lines = vec![
            Line {
                text: "Titans Resolue".into(),
                x: 0.2,
                y: 0.47,
                h: 0.031,
            },
            Line {
                text: "Kilic Dansi".into(),
                x: 0.5,
                y: 0.48,
                h: 0.029,
            },
            // The description under the title is no card.
            Line {
                text: "Gain 10 Ability Haste and deal more damage".into(),
                x: 0.5,
                y: 0.6,
                h: 0.02,
            },
            Line {
                text: "Apex lnventor".into(),
                x: 0.8,
                y: 0.49,
                h: 0.03,
            },
        ];
        let found = m.find(&lines);
        let at: Vec<(i64, f32, f32)> = found.iter().map(|c| (c.id, c.y, c.h)).collect();
        assert_eq!(
            at,
            vec![(1, 0.47, 0.031), (2, 0.48, 0.029), (4, 0.49, 0.03)]
        );
    }

    #[test]
    fn short_names_need_near_exact() {
        let m = Matcher::new([(5, "ADAPt"), (6, "Goliath")].into_iter());
        assert_eq!(
            m.find(&[line("ADAPt", 0.5, 0.3)]),
            vec![Card {
                id: 5,
                x: 0.5,
                y: 0.3,
                h: 0.03
            }]
        );
        assert!(m.find(&[line("Adept", 0.5, 0.3)]).is_empty());
    }

    /// Real Windows OCR (en-US engine) output from Turkish card titles.
    #[test]
    fn reads_real_turkish_ocr_noise() {
        let m = Matcher::new(
            [
                (3078, "Güm ve Bas"),
                (3116, "İndir İndir İndir Onları"),
                (1099, "Saldırı Gücüne Güç Kat"),
                (1058, "Mistik Yumruk"),
                (1080, "Ya Tankla Ya Terk Et"),
                (1205, "Kaçış Planı"),
                (1001, "Titanın Azmi"),
            ]
            .into_iter(),
        );
        let lines = vec![
            line("Güm ve Bas", 0.22, 0.49),
            line("Hasar verdiginde 6 saniyeligine yuk", 0.22, 0.61),
            line("indir indir indir", 0.50, 0.48),
            line("Onlam", 0.50, 0.53),
            line("Saldn•n GÜcÜne Giiq", 0.78, 0.49),
            line("Kat", 0.78, 0.53),
        ];
        assert_eq!(ids(m.find(&lines)), vec![3078, 3116, 1099]);
        let small = vec![
            line("Mistik Yumruk", 0.22, 0.48),
            line("Ya Tanida Ya Terk Et", 0.50, 0.48),
            line("Kacns Plam", 0.78, 0.48),
        ];
        assert_eq!(ids(m.find(&small)), vec![1058, 1080, 1205]);
    }

    #[test]
    fn description_lines_do_not_match() {
        let m = Matcher::new([(1, "Güm ve Bas"), (2, "Kaçış Planı")].into_iter());
        let lines = vec![
            line("Hasar verdiginde 6 saniyeligine yuk", 0.5, 0.6),
            line("kazanirsin. Her 10 yukte bir ekstra", 0.5, 0.64),
            line("dayaniklilik ve beceri hizi elde edersin.", 0.5, 0.68),
        ];
        assert!(m.find(&lines).is_empty());
    }

    #[test]
    fn joins_wrapped_titles() {
        let m = Matcher::new([(9, "Juggernaut Spirit of Mayhem")].into_iter());
        let lines = vec![
            line("Juggernaut Spirit", 0.5, 0.30),
            line("of Mayhem", 0.5, 0.33),
        ];
        // The card sits at the first line of its title.
        assert_eq!(
            m.find(&lines),
            vec![Card {
                id: 9,
                x: 0.5,
                y: 0.30,
                h: 0.03
            }]
        );
    }

    /// BGRA test image where each pixel has its own value in every channel.
    fn gradient(w: usize, h: usize) -> Vec<u8> {
        (0..w * h * 4).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn downsample_averages_2x2_blocks() {
        // 4x2 image, two blocks. Channels of block 1: B 0,4,8,12 and so on.
        #[rustfmt::skip]
        let src = [
            0, 0, 0, 255,   8, 16, 24, 255,   100, 100, 100, 255,   100, 100, 100, 255,
            4, 8, 12, 255,  12, 24, 36, 255,  100, 100, 100, 255,   104, 100, 100, 255,
        ];
        let (px, w, h) = downsample2(&src, 4, 2);
        assert_eq!((w, h), (2, 1));
        // (0 + 8 + 4 + 12) / 4 = 6, (0 + 16 + 8 + 24) / 4 = 12, (0 + 24 + 12 + 36) / 4 = 18.
        assert_eq!(&px[0..4], &[6, 12, 18, 255]);
        // Block 2: (100 + 100 + 100 + 104 + 2) / 4 = 101. The +2 rounds to the nearest value.
        assert_eq!(&px[4..8], &[101, 100, 100, 255]);
        assert_eq!(px.len(), 8);
    }

    #[test]
    fn downsample_drops_an_odd_edge() {
        let (px, w, h) = downsample2(&gradient(5, 3), 5, 3);
        assert_eq!((w, h), (2, 1));
        // Two pixels of four bytes.
        assert_eq!(px.len(), 2 * 4);
    }

    #[test]
    fn downsample_keeps_a_flat_color() {
        let src = vec![[10u8, 20, 30, 255]; 16 * 8].concat();
        let (px, w, h) = downsample2(&src, 16, 8);
        assert_eq!((w, h), (8, 4));
        assert!(px.chunks(4).all(|p| p == [10, 20, 30, 255]));
    }

    #[test]
    fn shrinks_until_both_sides_fit() {
        let src = gradient(16, 4);
        // Fits already: the caller keeps its own buffer.
        assert!(shrink_to_fit(&src, 16, 4, 16).is_none());
        // 16 > 10: one halving gives 8x2.
        let (px, w, h) = shrink_to_fit(&src, 16, 4, 10).unwrap();
        assert_eq!((w, h, px.len()), (8, 2, 8 * 2 * 4));
        // 16 > 5: two halvings give 4x1.
        let (_, w, h) = shrink_to_fit(&src, 16, 4, 5).unwrap();
        assert_eq!((w, h), (4, 1));
        // The taller side counts too.
        let (_, w, h) = shrink_to_fit(&gradient(4, 16), 4, 16, 10).unwrap();
        assert_eq!((w, h), (2, 8));
        // A buffer that is too short never panics.
        assert!(shrink_to_fit(&src[..10], 16, 4, 5).is_none());
    }

    /// A 4K crop (80 % of 3840 px) must fall under the 2600 px limit.
    #[test]
    fn a_4k_crop_fits_after_one_halving() {
        let (w, h) = (3072u32, 1425u32);
        let src = vec![0u8; (w * h * 4) as usize];
        let (px, nw, nh) = shrink_to_fit(&src, w, h, 2600).unwrap();
        assert_eq!((nw, nh), (1536, 712));
        assert_eq!(px.len(), (nw * nh * 4) as usize);
    }

    /// Lines whose centers are at these fractions of a capture.
    fn at(points: &[(f32, f32)]) -> Vec<Line> {
        points
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| line(&format!("line {i}"), x, y))
            .collect()
    }

    const CROP: Crop = Crop {
        x: 100,
        y: 50,
        w: 1000,
        h: 500,
    };

    #[test]
    fn lines_under_the_overlay_are_dropped() {
        // Centers on the screen: (300, 150), (600, 300) and (900, 450).
        let (kept, dropped) = drop_inside(
            at(&[(0.2, 0.2), (0.5, 0.5), (0.8, 0.8)]),
            CROP,
            [250, 100, 700, 350],
        );
        assert_eq!(dropped, 2);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].text, "line 2");
    }

    #[test]
    fn the_rect_keeps_its_right_and_bottom_edge_outside() {
        // (300, 150) sits on the left and top edge: inside.
        // (700, 350) sits on the right and bottom edge: outside.
        let (kept, dropped) =
            drop_inside(at(&[(0.2, 0.2), (0.6, 0.6)]), CROP, [300, 150, 700, 350]);
        assert_eq!(dropped, 1);
        assert_eq!(kept[0].text, "line 1");
    }

    #[test]
    fn an_overlay_away_from_the_capture_drops_nothing() {
        let (kept, dropped) = drop_inside(at(&[(0.1, 0.1), (0.9, 0.9)]), CROP, [0, 0, 90, 40]);
        assert_eq!((kept.len(), dropped), (2, 0));
        let (kept, dropped) = drop_inside(Vec::new(), CROP, [0, 0, 5000, 5000]);
        assert_eq!((kept.len(), dropped), (0, 0));
    }

    #[test]
    fn a_window_over_the_capture_is_found() {
        // The capture covers x 100 to 1100 and y 50 to 550.
        assert!(crop_overlaps(CROP, [0, 0, 5000, 5000]));
        assert!(crop_overlaps(CROP, [500, 200, 600, 300]));
        // A window that only crosses the left edge or the top edge.
        assert!(crop_overlaps(CROP, [0, 0, 101, 51]));
        // A window that only touches an edge does not cover a pixel.
        assert!(!crop_overlaps(CROP, [0, 0, 100, 5000]));
        assert!(!crop_overlaps(CROP, [1100, 0, 2000, 5000]));
        assert!(!crop_overlaps(CROP, [0, 550, 5000, 900]));
        // A window on the second monitor.
        assert!(!crop_overlaps(CROP, [1920, 0, 2400, 700]));
    }

    fn record(shot: Option<Shot>) -> ScanRecord {
        ScanRecord {
            at: Instant::now(),
            shot,
            lines: vec![line("Mistik Yumruk", 0.22, 0.48), line("Kralin", 0.5, 0.5)],
            dropped: 1,
            matched: vec![Card {
                id: 1058,
                x: 0.22,
                y: 0.48,
                h: 0.03,
            }],
            engine_failed: false,
        }
    }

    #[test]
    fn the_scan_text_lists_the_lines_and_the_matches() {
        let shot = Shot {
            bgra: vec![0; 16],
            crop: Crop {
                x: 192,
                y: 130,
                w: 2,
                h: 2,
            },
            game: Crop {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
            game_window: true,
        };
        let facts = vec![("OCR language".to_string(), "en-US".to_string())];
        let text = scan_text(&record(Some(shot)), &facts, &|id| format!("Name {id}"));
        assert!(text.starts_with("Hexsight last scan\nOCR language: en-US\n"));
        assert!(text.contains("Capture: x 192 y 130 width 2 height 2 (game window)\n"));
        assert!(text.contains("Lines (2):\n  [0.22, 0.48] Mistik Yumruk\n"));
        assert!(text.contains("1 more lines under the overlay are not listed.\n"));
        assert!(text.contains("Names matched (1):\n  1058 Name 1058 (x 0.22, y 0.48)\n"));
        let none = scan_text(&record(None), &[], &|_| String::new());
        assert!(none.contains("Capture: none"));
    }

    #[test]
    fn a_saved_scan_has_a_png_and_a_text_file() {
        let dir = std::env::temp_dir().join(format!("hexsight-test-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // One row of two pixels: blue 10, green 20, red 30, then white.
        let bgra = vec![10, 20, 30, 0, 255, 255, 255, 0];
        let shot = Shot {
            bgra,
            crop: Crop {
                x: 0,
                y: 0,
                w: 2,
                h: 1,
            },
            game: Crop {
                x: 0,
                y: 0,
                w: 2,
                h: 1,
            },
            game_window: false,
        };
        save_scan(&dir, &record(Some(shot)), "text").unwrap();
        let img = image::open(dir.join("last-scan.png")).unwrap().to_rgb8();
        assert_eq!(img.dimensions(), (2, 1));
        assert_eq!(img.get_pixel(0, 0).0, [30, 20, 10]);
        assert_eq!(img.get_pixel(1, 0).0, [255, 255, 255]);
        assert_eq!(
            std::fs::read_to_string(dir.join("last-scan.txt")).unwrap(),
            "\u{feff}text"
        );
        // A scan without a capture removes the old PNG.
        save_scan(&dir, &record(None), "again").unwrap();
        assert!(!dir.join("last-scan.png").exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("last-scan.txt")).unwrap(),
            "\u{feff}again"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_capture_crop_is_the_middle_of_the_game_area() {
        // 1920x1080 at the screen origin: x 10-90 %, y 12-78 %.
        assert_eq!(
            crop_of(0, 0, 1920, 1080),
            Crop {
                x: 192,
                y: 129,
                w: 1536,
                h: 712
            }
        );
        // A game window away from the origin moves the crop with it.
        assert_eq!(
            crop_of(100, 50, 1000, 500),
            Crop {
                x: 200,
                y: 110,
                w: 800,
                h: 330
            }
        );
    }

    #[test]
    fn a_fraction_of_the_capture_is_a_screen_pixel() {
        let crop = Crop {
            x: 100,
            y: 50,
            w: 1000,
            h: 500,
        };
        assert_eq!(crop.at(0.0, 0.0), (100.0, 50.0));
        assert_eq!(crop.at(1.0, 1.0), (1100.0, 550.0));
        assert_eq!(crop.at(0.25, 0.5), (350.0, 300.0));
    }

    #[test]
    fn badges_need_the_game_window() {
        let game = Crop {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let crop = crop_of(0, 0, 1920, 1080);
        let mut shot = Shot {
            bgra: Vec::new(),
            crop,
            game,
            game_window: true,
        };
        assert_eq!(shot.geometry(), Some(Geometry { game, crop }));
        // The screen fallback (F9) has no game window.
        shot.game_window = false;
        assert_eq!(shot.geometry(), None);
    }

    #[test]
    fn an_image_acts_as_the_screen() {
        let img = image::RgbaImage::from_pixel(1000, 500, image::Rgba([1, 2, 3, 255]));
        let shot = shot_from_image(&img);
        assert_eq!(
            shot.crop,
            Crop {
                x: 100,
                y: 60,
                w: 800,
                h: 330
            }
        );
        // The image is the whole game area.
        assert_eq!(
            shot.game,
            Crop {
                x: 0,
                y: 0,
                w: 1000,
                h: 500
            }
        );
        assert_eq!(shot.bgra.len(), 800 * 330 * 4);
        // BGRA: blue comes first.
        assert_eq!(&shot.bgra[0..4], &[3, 2, 1, 255]);
    }

    fn names() -> Matcher {
        Matcher::new(
            [
                (1079, "Symphony of War"),
                (1058, "Mystic Punch"),
                (1343, "Sneakerhead"),
                (1001, "Titan's Resolve"),
            ]
            .into_iter(),
        )
    }

    #[test]
    fn two_chat_lines_at_different_heights_are_no_pick_screen() {
        // A friend types two augment names in the chat. They are at different heights,
        // and Hexsight must not make an offer from them.
        let lines = vec![
            line("Symphony of War", 0.12, 0.30),
            line("Mystic Punch", 0.55, 0.45),
        ];
        let found = names().find(&lines);
        assert!(found.len() < 2, "{found:?}");
        // The same names in one row are two cards.
        let row = vec![
            line("Symphony of War", 0.12, 0.45),
            line("Mystic Punch", 0.55, 0.46),
        ];
        assert_eq!(ids(names().find(&row)), vec![1079, 1058]);
    }

    #[test]
    fn three_chat_lines_on_three_rows_are_no_pick_screen() {
        let lines = vec![
            line("Symphony of War", 0.10, 0.20),
            line("Mystic Punch", 0.45, 0.35),
            line("Sneakerhead", 0.80, 0.50),
        ];
        assert!(names().find(&lines).len() < 2);
    }

    #[test]
    fn a_stray_name_in_the_chat_does_not_join_the_row_of_the_cards() {
        // Three cards at y 0.51, and a chat line with a fourth name at the left edge, lower down.
        // The first three columns by x would be the chat line and two cards.
        let lines = vec![
            line("Titans Resolve", 0.04, 0.90),
            line("Symphony of War", 0.19, 0.51),
            line("Mystic Punch", 0.50, 0.51),
            line("Sneakerhead", 0.81, 0.52),
        ];
        assert_eq!(ids(names().find(&lines)), vec![1079, 1058, 1343]);
    }

    #[test]
    fn the_biggest_row_wins_when_two_rows_have_names() {
        // Two names in one row, and one name alone in another row.
        let lines = vec![
            line("Symphony of War", 0.19, 0.30),
            line("Mystic Punch", 0.50, 0.51),
            line("Sneakerhead", 0.81, 0.51),
        ];
        assert_eq!(ids(names().find(&lines)), vec![1058, 1343]);
    }

    #[test]
    fn the_row_tolerance_is_one_title_height() {
        let at = |dy: f32| {
            vec![
                line("Symphony of War", 0.2, 0.50),
                line("Mystic Punch", 0.6, 0.50 + dy),
            ]
        };
        // The height of a title line in these tests is 0.03.
        assert_eq!(names().find(&at(0.029)).len(), 2);
        assert_eq!(names().find(&at(-0.029)).len(), 2);
        assert_eq!(names().find(&at(0.0)).len(), 2);
        assert!(names().find(&at(0.035)).len() < 2);
        assert!(names().find(&at(-0.035)).len() < 2);
    }

    #[test]
    fn a_wrapped_title_keeps_the_row_of_its_first_line() {
        // The long title wraps to two lines. Its card sits at the first line, so the row stays.
        let m =
            Matcher::new([(9, "Juggernaut Spirit of Mayhem"), (1058, "Mystic Punch")].into_iter());
        let lines = vec![
            line("Juggernaut Spirit", 0.2, 0.50),
            line("of Mayhem", 0.2, 0.53),
            line("Mystic Punch", 0.6, 0.51),
        ];
        assert_eq!(ids(m.find(&lines)), vec![9, 1058]);
    }

    /// A BGRA image of `w` x `h` pixels with one color.
    fn flat(w: usize, h: usize, bgr: [u8; 3]) -> Vec<u8> {
        vec![[bgr[0], bgr[1], bgr[2], 255]; w * h].concat()
    }

    #[test]
    fn a_black_capture_has_a_mean_brightness_under_three() {
        assert_eq!(mean_brightness(&flat(64, 32, [0, 0, 0])), 0.0);
        assert!(mean_brightness(&flat(64, 32, [2, 2, 2])) < BLACK_BELOW);
        assert!(mean_brightness(&flat(64, 32, [3, 3, 3])) >= BLACK_BELOW);
        assert_eq!(mean_brightness(&flat(64, 32, [255, 255, 255])), 255.0);
        assert_eq!(mean_brightness(&[]), 0.0);
        // The alpha byte does not count: black with full alpha is still black.
        assert!(mean_brightness(&[0, 0, 0, 255, 0, 0, 0, 255]) < BLACK_BELOW);
    }

    /// A busy BGRA image: `seed` changes the pixels, as a running game does.
    fn busy(seed: usize) -> Vec<u8> {
        (0..64 * 32 * 4)
            .map(|i| ((i * 7 + seed * 13) % 200 + 30) as u8)
            .collect()
    }

    #[test]
    fn a_black_scan_shows_at_once() {
        let mut w = CaptureWatch::default();
        assert_eq!(w.step(&flat(64, 32, [0, 0, 0]), 5), CaptureHealth::Black);
        // The next real image brings it back.
        assert_eq!(w.step(&busy(1), 5), CaptureHealth::Fine);
    }

    #[test]
    fn the_same_image_for_five_scans_while_the_level_rises_is_frozen() {
        let mut w = CaptureWatch::default();
        let img = busy(1);
        // Four scans with the same image: not yet, and the level is the same.
        for _ in 0..4 {
            assert_eq!(w.step(&img, 6), CaptureHealth::Fine);
        }
        // The fifth scan has the same image, but the level did not rise (a pick screen that stands still).
        assert_eq!(w.step(&img, 6), CaptureHealth::Fine);
        // The level rises while the image stays: frozen.
        assert_eq!(w.step(&img, 7), CaptureHealth::Frozen);
        assert_eq!(w.step(&img, 7), CaptureHealth::Frozen);
        // A changing image clears it.
        assert_eq!(w.step(&busy(2), 7), CaptureHealth::Fine);
    }

    #[test]
    fn a_running_game_is_never_frozen() {
        let mut w = CaptureWatch::default();
        for scan in 0..30 {
            assert_eq!(
                w.step(&busy(scan), 1 + scan as i64 / 3),
                CaptureHealth::Fine,
                "scan {scan}"
            );
        }
    }

    /// A capture (as `crop_of` makes it) of a game of `game_h` rows, with
    /// `cards` cards from the left. Each card has a light frame and a body.
    fn card_capture(
        game_h: usize,
        game_w: usize,
        cards: usize,
        body: [u8; 3],
    ) -> (Vec<u8>, u32, u32) {
        let crop = crop_of(0, 0, game_w as i32, game_h as i32);
        let (w, h) = (crop.w as usize, crop.h as usize);
        let unit = game_h as f32 / 1080.0;
        let mut bgra = vec![255u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                // A busy background that is never the color of a card body.
                let n = ((x * 7 + y * 13) % 40) as u8;
                let mut px = [90 + n, 100 + n, 60 + n];
                let (gx, gy) = ((x as i32 + crop.x) as f32, (y as i32 + crop.y) as f32);
                for k in 0..cards {
                    let dx =
                        (gx - (game_w as f32 / 2.0 + (k as f32 - 1.0) * CARD_PITCH * unit)).abs();
                    if dx <= 160.0 * unit && (193.0 * unit..=718.0 * unit).contains(&gy) {
                        px = if dx >= 138.0 * unit {
                            [190, 190, 190]
                        } else {
                            body
                        };
                    }
                }
                bgra[(y * w + x) * 4..][..4].copy_from_slice(&[px[2], px[1], px[0], 255]);
            }
        }
        (bgra, w as u32, h as u32)
    }

    const TEAL: [u8; 3] = [1, 15, 16];

    #[test]
    fn three_cards_are_seen_at_any_size_and_shape() {
        // 1080p, 720p, 1440p, 21:9 and 16:10 windows: the cards scale with the height.
        for (gw, gh) in [
            (1920, 1080),
            (1280, 720),
            (2560, 1440),
            (2560, 1080),
            (1728, 1080),
        ] {
            let (bgra, w, h) = card_capture(gh, gw, 3, TEAL);
            assert!(cards_visible(&bgra, w, h), "{gw}x{gh}");
        }
    }

    #[test]
    fn two_cards_are_enough_and_one_is_not() {
        let (bgra, w, h) = card_capture(1080, 1920, 3, TEAL);
        // A tooltip hides the middle card.
        let mut hidden = bgra.clone();
        let crop = crop_of(0, 0, 1920, 1080);
        for y in 0..h as usize {
            for x in 0..w as usize {
                if ((x as i32 + crop.x) - 960).abs() < 200 {
                    hidden[(y * w as usize + x) * 4..][..4].copy_from_slice(&[120, 100, 80, 255]);
                }
            }
        }
        assert!(cards_visible(&hidden, w, h));
        let (one, w, h) = card_capture(1080, 1920, 1, TEAL);
        assert!(!cards_visible(&one, w, h));
    }

    #[test]
    fn cards_with_a_gold_body_are_seen_by_their_frames() {
        // The reveal animation paints the bodies gold. The light frames stay.
        let (bgra, w, h) = card_capture(1080, 1920, 3, [150, 120, 40]);
        assert!(cards_visible(&bgra, w, h));
    }

    #[test]
    fn plain_screens_have_no_cards() {
        // Black, flat grey, and a busy background without cards.
        for fill in [0u8, 60, 200] {
            assert!(
                !cards_visible(&flat(1536, 712, [fill, fill, fill]), 1536, 712),
                "fill {fill}"
            );
        }
        let (bgra, w, h) = card_capture(1080, 1920, 0, TEAL);
        assert!(!cards_visible(&bgra, w, h));
    }

    /// Real card bands (tests/fixtures/card_band): the rows 44-134 and the
    /// columns 56-328 of a 384x178 capture, which is a game screen of 1920x1080
    /// cut by `crop_of` and shrunk 4 times. Only the band is kept, with no chat
    /// and no names. The rest of the capture is filled with a flat grey.
    fn band_capture(png: &[u8]) -> (Vec<u8>, u32, u32) {
        let band = image::load_from_memory(png).unwrap().to_rgba8();
        let (w, h) = (384usize, 178usize);
        let mut bgra = flat(w, h, [128, 128, 128]);
        for (x, y, p) in band.enumerate_pixels() {
            let i = ((y as usize + 44) * w + x as usize + 56) * 4;
            bgra[i..i + 4].copy_from_slice(&[p[2], p[1], p[0], 255]);
        }
        (bgra, w as u32, h as u32)
    }

    #[test]
    fn real_card_bands_show_cards() {
        // Plain, tooltip over two cards, and the gold and olive frames at the
        // start of a pick (the body is not teal there).
        let bands: [(&str, &[u8]); 4] = [
            (
                "plain",
                include_bytes!("../tests/fixtures/card_band/cards_plain.png"),
            ),
            (
                "tooltip",
                include_bytes!("../tests/fixtures/card_band/cards_tooltip.png"),
            ),
            (
                "gold frame",
                include_bytes!("../tests/fixtures/card_band/cards_gold_frame.png"),
            ),
            (
                "olive frame",
                include_bytes!("../tests/fixtures/card_band/cards_olive_frame.png"),
            ),
        ];
        for (name, png) in bands {
            let (bgra, w, h) = band_capture(png);
            assert!(cards_visible(&bgra, w, h), "{name}");
        }
    }

    #[test]
    fn real_bands_without_cards_show_none() {
        // An empty arena, a fight, the shop panel (teal like a card body), and
        // a reveal animation that OCR cannot read either.
        let bands: [(&str, &[u8]); 4] = [
            (
                "empty arena",
                include_bytes!("../tests/fixtures/card_band/no_empty_arena.png"),
            ),
            (
                "fight",
                include_bytes!("../tests/fixtures/card_band/no_arena_fight.png"),
            ),
            (
                "shop",
                include_bytes!("../tests/fixtures/card_band/no_shop.png"),
            ),
            (
                "animation",
                include_bytes!("../tests/fixtures/card_band/no_animation.png"),
            ),
        ];
        for (name, png) in bands {
            let (bgra, w, h) = band_capture(png);
            assert!(!cards_visible(&bgra, w, h), "{name}");
        }
    }

    #[test]
    fn a_capture_of_the_wrong_size_is_not_judged() {
        // Not a capture of crop_of: scan it (a wrong yes is cheap).
        assert!(cards_visible(&[0; 40], 100, 100));
        assert!(cards_visible(&[], 0, 0));
    }

    /// Runs the check on real frames, with the label of each frame.
    /// HEXSIGHT_CARD_FRAMES=<folder with .jpg frames of the whole game screen>
    /// HEXSIGHT_CARD_LABELS=<file with lines "name<TAB>label"> (label CARDS, NO or AMBIG).
    /// Run: HEXSIGHT_CARD_FRAMES=... HEXSIGHT_CARD_LABELS=... cargo test card_presence -- --ignored --nocapture
    #[test]
    #[ignore]
    fn card_presence_on_real_frames() {
        let (Some(dir), Some(labels)) = (
            std::env::var_os("HEXSIGHT_CARD_FRAMES"),
            std::env::var_os("HEXSIGHT_CARD_LABELS"),
        ) else {
            panic!("set HEXSIGHT_CARD_FRAMES and HEXSIGHT_CARD_LABELS");
        };
        let labels = std::fs::read_to_string(labels).unwrap();
        let (mut cards, mut hit, mut none, mut skipped) = (0, 0, 0, 0);
        let (mut missed, mut wrongly_seen, mut ambig) = (Vec::new(), Vec::new(), Vec::new());
        let mut shots = Vec::new();
        for line in labels.lines().filter(|l| !l.trim().is_empty()) {
            let (name, label) = line.split_once('\t').unwrap();
            let img = image::open(Path::new(&dir).join(format!("{name}.jpg")))
                .unwrap()
                .to_rgba8();
            let shot = shot_from_image(&img);
            let seen = cards_visible(&shot.bgra, shot.crop.w as u32, shot.crop.h as u32);
            match label.trim() {
                "CARDS" => {
                    cards += 1;
                    hit += seen as usize;
                    if !seen {
                        missed.push(name.to_string());
                    }
                }
                "NO" => {
                    none += 1;
                    skipped += !seen as usize;
                    if seen {
                        wrongly_seen.push(name.to_string());
                    }
                }
                _ => ambig.push(format!("{name}={seen}")),
            }
            shots.push(shot);
        }
        // Time per call: every frame 50 times.
        let t = Instant::now();
        let mut yes = 0usize;
        for _ in 0..50 {
            for s in &shots {
                yes += cards_visible(&s.bgra, s.crop.w as u32, s.crop.h as u32) as usize;
            }
        }
        let calls = 50 * shots.len();
        println!("recall {hit}/{cards}, skip {skipped}/{none}, yes {yes}/{calls}");
        println!("missed CARDS frames: {missed:?}");
        println!("NO frames seen as cards: {wrongly_seen:?}");
        println!("AMBIG frames: {ambig:?}");
        println!(
            "time per call: {:.2} us",
            t.elapsed().as_secs_f64() * 1e6 / calls as f64
        );
        assert_eq!(hit, cards, "a pick screen was missed");
    }

    #[test]
    fn four_equal_scans_are_not_enough() {
        let mut w = CaptureWatch::default();
        let img = busy(3);
        assert_eq!(w.step(&img, 1), CaptureHealth::Fine);
        assert_eq!(w.step(&img, 2), CaptureHealth::Fine);
        assert_eq!(w.step(&img, 3), CaptureHealth::Fine);
        assert_eq!(w.step(&img, 4), CaptureHealth::Fine);
        assert_eq!(w.step(&img, 5), CaptureHealth::Frozen);
    }

    // ---- The band signature: equal band, equal signature.

    const SIG_W: usize = 1536;
    const SIG_H: usize = 712;

    /// A gray capture of the size of a 1080p game (see `crop_of`). `draw` may
    /// paint the title area: it gets the pixel buffer.
    fn band_image(gray: u8, draw: impl Fn(&mut [u8])) -> Vec<u8> {
        let mut img = flat(SIG_W, SIG_H, [gray; 3]);
        draw(&mut img);
        img
    }

    /// Paint the pixel (x, y) with a gray value.
    fn put(img: &mut [u8], x: usize, y: usize, v: u8) {
        let i = (y * SIG_W + x) * 4;
        img[i..i + 3].copy_from_slice(&[v; 3]);
    }

    /// A "title": light text in the band, as a solid bar or as stripes.
    fn title(img: &mut [u8], stripes: bool) {
        for y in 320..350 {
            for x in 100..400 {
                if !stripes || (x / 10) % 2 == 0 {
                    put(img, x, y, 230);
                }
            }
        }
    }

    #[test]
    fn the_same_band_gives_the_same_signature() {
        let a = band_image(100, |i| title(i, false));
        let b = band_image(100, |i| title(i, false));
        let sig = band_signature(&a, SIG_W as u32, SIG_H as u32);
        assert!(sig.is_some());
        assert_eq!(sig, band_signature(&b, SIG_W as u32, SIG_H as u32));
    }

    #[test]
    fn a_few_levels_of_noise_in_a_flat_area_do_not_change_the_signature() {
        // A gray of 100 sits in the middle of its 8-level step (96 to 103). A cell at the edge of
        // a title can cross a step with the same noise. That is a safe miss: the band is read again.
        let (w, h) = (SIG_W as u32, SIG_H as u32);
        let sig = band_signature(&band_image(100, |_| {}), w, h);
        assert!(sig.is_some());
        assert_eq!(sig, band_signature(&band_image(102, |_| {}), w, h));
        assert_eq!(sig, band_signature(&band_image(98, |_| {}), w, h));
        // Another step is another signature.
        assert_ne!(sig, band_signature(&band_image(110, |_| {}), w, h));
    }

    #[test]
    fn a_changed_title_changes_the_signature() {
        let (w, h) = (SIG_W as u32, SIG_H as u32);
        let a = band_signature(&band_image(100, |i| title(i, false)), w, h);
        // A reroll: other text in the same place.
        assert_ne!(
            a,
            band_signature(&band_image(100, |i| title(i, true)), w, h)
        );
        // The cards are gone (a pick).
        assert_ne!(a, band_signature(&band_image(100, |_| {}), w, h));
        // One short word of 60 x 20 pixels is enough.
        let small = band_image(100, |i| {
            for y in 330..350 {
                for x in 700..760 {
                    put(i, x, y, 230);
                }
            }
        });
        assert_ne!(
            band_signature(&band_image(100, |_| {}), w, h),
            band_signature(&small, w, h)
        );
    }

    #[test]
    fn a_change_outside_the_band_is_not_seen() {
        // The signature reads the title band only. Rows above the band (the top of the capture) are free.
        let (top, _) = band_rows(SIG_H as u32);
        let a = band_image(100, |i| title(i, false));
        let b = band_image(100, |i| {
            title(i, false);
            for y in 0..(top as usize - 2) {
                for x in 0..SIG_W {
                    put(i, x, y, 255);
                }
            }
        });
        assert_eq!(
            band_signature(&a, SIG_W as u32, SIG_H as u32),
            band_signature(&b, SIG_W as u32, SIG_H as u32)
        );
    }

    #[test]
    fn a_capture_that_is_too_small_or_too_short_has_no_signature() {
        assert_eq!(band_signature(&flat(100, 100, [9; 3]), 100, 100), None);
        assert_eq!(band_signature(&[], SIG_W as u32, SIG_H as u32), None);
        // A buffer cut short.
        let img = band_image(100, |_| {});
        assert_eq!(
            band_signature(&img[..img.len() / 2], SIG_W as u32, SIG_H as u32),
            None
        );
    }

    #[test]
    fn the_signature_costs_far_less_than_a_read() {
        // About 90 ms for an OCR read (1536x712). A debug build is 10 to 30 times slower than the
        // release build for this loop, so the debug limit is loose.
        let img = band_image(100, |i| title(i, false));
        let t = Instant::now();
        let n = 20;
        for _ in 0..n {
            assert!(band_signature(&img, SIG_W as u32, SIG_H as u32).is_some());
        }
        let ms = t.elapsed().as_secs_f64() * 1e3 / n as f64;
        println!("band signature: {ms:.2} ms per call");
        let limit = if cfg!(debug_assertions) { 90.0 } else { 3.0 };
        assert!(ms < limit, "{ms:.2} ms");
    }

    // ---- The desktop and the capture state.

    #[test]
    fn a_desktop_that_cannot_be_opened_or_is_not_default_is_locked() {
        assert_eq!(desk_from(true, Some("Default"), false), Desk::Normal);
        assert_eq!(desk_from(true, Some("default"), false), Desk::Normal);
        // The name is not known: the desktop opened, so it is the normal one.
        assert_eq!(desk_from(true, None, false), Desk::Normal);
        // The lock screen and the secure prompt: refused, or another name.
        assert_eq!(desk_from(false, None, false), Desk::Locked);
        assert_eq!(desk_from(true, Some("Winlogon"), false), Desk::Locked);
        assert_eq!(desk_from(true, Some("Screen-saver"), false), Desk::Locked);
        // A remote session on the normal desktop is only remote. A locked one is locked.
        assert_eq!(desk_from(true, Some("Default"), true), Desk::Remote);
        assert_eq!(desk_from(false, None, true), Desk::Locked);
    }

    #[test]
    fn the_desktop_decides_the_capture_state_before_the_image() {
        use CaptureHealth::*;
        for image in [Fine, Black, Frozen] {
            // A locked screen: nothing to judge in the image.
            assert_eq!(capture_state(Desk::Locked, image), Locked);
            // A normal desktop: the image check stands.
            assert_eq!(capture_state(Desk::Normal, image), image);
        }
        // A remote desktop with a black image is the remote desktop. The rest stands.
        assert_eq!(capture_state(Desk::Remote, Black), Remote);
        assert_eq!(capture_state(Desk::Remote, Fine), Fine);
        assert_eq!(capture_state(Desk::Remote, Frozen), Frozen);
    }

    #[test]
    fn looking_for_the_game_window_and_the_desktop_is_cheap() {
        // The game window is found with FindWindowW on every scan and every poll. A WinEvent hook
        // would replace only this call. It costs well under a millisecond, so the hook would not pay.
        let n = 1000;
        let t = Instant::now();
        for _ in 0..n {
            let _ = game_window_rect();
        }
        let window_us = t.elapsed().as_secs_f64() * 1e6 / n as f64;
        let t = Instant::now();
        for _ in 0..n {
            let _ = desk_state();
        }
        let desk_us = t.elapsed().as_secs_f64() * 1e6 / n as f64;
        println!(
            "game_window_rect: {window_us:.1} us per call, desk_state: {desk_us:.1} us per call"
        );
        assert!(window_us < 1000.0 && desk_us < 1000.0);
    }

    // ---- The memory of a read.

    /// Private bytes of this process (its commit charge), in MB.
    fn private_mb() -> f64 {
        use windows_sys::Win32::System::ProcessStatus::{
            K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let mut c: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        unsafe {
            K32GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut c as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
                c.cb,
            )
        };
        c.PrivateUsage as f64 / (1024.0 * 1024.0)
    }

    /// Growth of the private bytes over `n` runs of `f`, after 3 runs to warm up, in MB per run.
    fn growth_per_run(n: usize, mut f: impl FnMut()) -> f64 {
        for _ in 0..3 {
            f();
        }
        let before = private_mb();
        for _ in 0..n {
            f();
        }
        (private_mb() - before) / n as f64
    }

    /// Finds the step of a read that keeps memory. In a real game Hexsight grew by
    /// about 4.5 MB every 30 s, the pause of a full read (`FULL_READ_EVERY` in worker.rs).
    /// Run: cargo test --release read_memory -- --ignored --nocapture
    #[test]
    #[ignore]
    fn read_memory_per_step() {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(
                windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
            );
        }
        let (w, h) = (SIG_W as u32, SIG_H as u32);
        let img = band_image(100, |i| title(i, false));
        let ocr = Ocr::new().expect("Windows OCR");
        let n = 30;
        let capture_mb = growth_per_run(n, || {
            let _ = capture(0, 0, w as i32, h as i32);
        });
        let writer_mb = growth_per_run(n, || {
            let writer = DataWriter::new().unwrap();
            writer.WriteBytes(&img).unwrap();
            let _ = writer.DetachBuffer().unwrap();
        });
        let bitmap_mb = growth_per_run(n, || {
            let writer = DataWriter::new().unwrap();
            writer.WriteBytes(&img).unwrap();
            let buf = writer.DetachBuffer().unwrap();
            let _ = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
                &buf,
                BitmapPixelFormat::Bgra8,
                w as i32,
                h as i32,
                BitmapAlphaMode::Ignore,
            )
            .unwrap();
        });
        let band_mb = growth_per_run(n, || {
            let _ = ocr.recognize_band(&img, w, h);
        });
        let whole_mb = growth_per_run(n, || {
            let _ = ocr.recognize(&img, w, h);
        });
        println!(
            "MB kept per run: capture {capture_mb:.3}, writer {writer_mb:.3}, bitmap {bitmap_mb:.3}, band read {band_mb:.3}, whole read {whole_mb:.3}"
        );
    }
}
