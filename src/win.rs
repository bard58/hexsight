//! Small Win32 helpers: find a process folder, read a global key state.

use std::path::PathBuf;
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

/// Folder of the first running process with this exe name.
pub fn process_dir(exe: &str) -> Option<PathBuf> {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = None;
        let mut ok = Process32FirstW(snap, &mut entry) != 0;
        while ok {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if name.eq_ignore_ascii_case(exe) {
                found = image_path(entry.th32ProcessID);
                break;
            }
            ok = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
        found.and_then(|p| p.parent().map(|d| d.to_path_buf()))
    }
}

unsafe fn image_path(pid: u32) -> Option<PathBuf> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size) != 0;
        CloseHandle(h);
        ok.then(|| PathBuf::from(String::from_utf16_lossy(&buf[..size as usize])))
    }
}

/// True while the virtual key is down. Works when another window has focus.
pub fn key_down(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
}

/// True when no other app holds `vk` as a global hotkey. Hexsight reads its hotkeys
/// with `GetAsyncKeyState`, so a clash cannot fail at the start. This asks
/// Windows to register the key for a moment: it refuses when another app has
/// the same key. The key is released at once, so Hexsight never keeps it.
pub fn hotkey_free(vk: i32) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
    };
    const ID: i32 = 0x4147; // any number, only for this call
    unsafe {
        let ok = RegisterHotKey(std::ptr::null_mut(), ID, MOD_NOREPEAT, vk as u32) != 0;
        if ok {
            UnregisterHotKey(std::ptr::null_mut(), ID);
        }
        ok
    }
}

/// Win32 virtual key of an egui key, for the keys that can be a hotkey: F1 to
/// F12, A to Z, and 0 to 9. The codes of letters and digits are their ASCII codes.
pub fn vk_for_key(key: egui::Key) -> Option<i32> {
    match key.name().as_bytes() {
        [c @ (b'A'..=b'Z' | b'0'..=b'9')] => Some(*c as i32),
        [b'F', n @ ..] => match std::str::from_utf8(n).ok()?.parse::<i32>() {
            Ok(n @ 1..=12) => Some(0x6F + n),
            _ => None,
        },
        _ => None,
    }
}

/// Name of a hotkey on the screen: "F8", "A", "5". Other codes show as "Key 45".
pub fn key_label(vk: i32) -> String {
    match vk {
        0x70..=0x7B => format!("F{}", vk - 0x6F),
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        _ => format!("Key {vk}"),
    }
}

/// Screen rect of a window in pixels: left, top, right, bottom.
pub fn window_rect(hwnd: isize) -> Option<[i32; 4]> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let ok = unsafe { GetWindowRect(hwnd as _, &mut r) } != 0;
    ok.then_some([r.left, r.top, r.right, r.bottom])
}

/// First Windows build with the acrylic backdrop for normal windows
/// (`DWMWA_SYSTEMBACKDROP_TYPE`): Windows 11 22H2.
const GLASS_BUILD: u32 = 22621;

/// Windows build number, for example 26200. 0 when the registry has none.
/// Test hook: HEXSIGHT_WINDOWS_BUILD=<number> acts as another build, for example
/// 19045 to see the screens of Windows 10.
pub fn windows_build() -> u32 {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    if let Some(build) = std::env::var("HEXSIGHT_WINDOWS_BUILD")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        return build;
    }
    let key = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let name = wide("CurrentBuildNumber");
    let mut buf = [0u16; 16];
    let mut size = std::mem::size_of_val(&buf) as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    } == 0;
    if !ok {
        return 0;
    }
    // `size` counts bytes and includes the end mark.
    parse_build(&buf[..(size as usize / 2).min(buf.len())])
}

/// "26200" as UTF-16, with or without an end mark -> 26200. 0 for other text.
fn parse_build(text: &[u16]) -> u32 {
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
        .trim()
        .parse()
        .unwrap_or(0)
}

/// True when a Windows with this build number has the acrylic backdrop.
fn build_has_glass(build: u32) -> bool {
    build >= GLASS_BUILD
}

/// True when this Windows can show the acrylic backdrop. On older builds
/// (Windows 10) the extended frame would show through the see-through fill,
/// so Hexsight keeps the glass off there.
pub fn glass_supported() -> bool {
    static SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SUPPORTED.get_or_init(|| build_has_glass(windows_build()))
}

/// Windows 11 window look: dark frame, round corners, and an acrylic
/// backdrop behind the transparent client area. `glass` counts only when
/// this Windows supports it (see `glass_supported`).
pub fn style_window(hwnd: isize, glass: bool) {
    use windows_sys::Win32::Graphics::Dwm::*;
    use windows_sys::Win32::UI::Controls::MARGINS;
    let glass = glass && glass_supported();
    let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
    let set = |attr: i32, v: u32| unsafe {
        DwmSetWindowAttribute(hwnd, attr as _, (&v as *const u32).cast(), 4);
    };
    set(DWMWA_USE_IMMERSIVE_DARK_MODE, 1);
    set(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND as u32);
    set(DWMWA_BORDER_COLOR, 0x0033_3333);
    // Glass off (also used in overlay mode): no backdrop, no extended frame.
    let m = if glass { -1 } else { 0 };
    let margins = MARGINS {
        cxLeftWidth: m,
        cxRightWidth: m,
        cyTopHeight: m,
        cyBottomHeight: m,
    };
    unsafe {
        DwmExtendFrameIntoClientArea(hwnd, &margins);
    }
    set(
        DWMWA_SYSTEMBACKDROP_TYPE,
        if glass {
            DWMSBT_TRANSIENTWINDOW
        } else {
            DWMSBT_NONE
        } as u32,
    );
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The name of the mutex that keeps one copy of Hexsight.
#[cfg(not(feature = "e2e"))]
const INSTANCE_MUTEX: &str = r"Local\Hexsight.SingleInstance";
/// A test build has its own mutex, so it runs next to a real Hexsight.
#[cfg(feature = "e2e")]
const INSTANCE_MUTEX: &str = r"Local\Hexsight.SingleInstance.E2E";

/// Try to take the mutex of this name. `Some(handle)` means that this process owns it now
/// (the handle stays open for the life of the process on purpose). `None` means that
/// another process holds it. The handle of that failed try is closed here: a retry loop
/// that leaked it would keep the mutex alive for ever.
fn take_mutex(name: &str) -> Option<windows_sys::Win32::Foundation::HANDLE> {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    unsafe {
        let name = wide(name);
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if GetLastError() != ERROR_ALREADY_EXISTS {
            return Some(handle);
        }
        if !handle.is_null() {
            CloseHandle(handle);
        }
        None
    }
}

/// Bring the window of the running Hexsight to the front. A test build never does: the
/// window of the same name could be a real Hexsight.
#[cfg_attr(feature = "e2e", allow(dead_code))]
fn focus_running_copy() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };
    unsafe {
        let title = wide("Hexsight");
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}

/// Returns false when another Hexsight already runs. That copy comes to the front.
pub fn single_instance() -> bool {
    if take_mutex(INSTANCE_MUTEX).is_some() {
        return true;
    }
    #[cfg(not(feature = "e2e"))]
    focus_running_copy();
    false
}

/// Like `single_instance`, for the copy that an update starts. The old copy is still
/// closing, so the mutex is not free yet: try again every 200 ms for up to `timeout`.
/// The old window stays where it is during the wait. When the time ends, it behaves
/// as `single_instance`.
pub fn single_instance_wait(timeout: std::time::Duration) -> bool {
    wait_for_mutex(INSTANCE_MUTEX, timeout) || single_instance()
}

fn wait_for_mutex(name: &str, timeout: std::time::Duration) -> bool {
    let end = std::time::Instant::now() + timeout;
    loop {
        if take_mutex(name).is_some() {
            return true;
        }
        if std::time::Instant::now() >= end {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// The event with which a copy that an update started tells the old copy "I am up".
/// The name holds the id of the old process.
fn ready_name(old_pid: u32) -> String {
    format!(r"Local\Hexsight.UpdateReady.{old_pid}")
}

/// The event of the old copy. The new copy sets it. The handle closes on drop.
pub struct ReadyEvent(windows_sys::Win32::Foundation::HANDLE);

impl ReadyEvent {
    /// Make the event of this process. Call it before the new copy starts.
    pub fn create() -> Option<ReadyEvent> {
        use windows_sys::Win32::System::Threading::CreateEventW;
        let name = wide(&ready_name(std::process::id()));
        let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, name.as_ptr()) };
        (!handle.is_null()).then_some(ReadyEvent(handle))
    }

    /// True when the new copy has set the event.
    pub fn is_set(&self) -> bool {
        use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
        use windows_sys::Win32::System::Threading::WaitForSingleObject;
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_OBJECT_0 }
    }
}

impl Drop for ReadyEvent {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// The new copy tells the old one (by its process id) that it is up. Nothing happens when
/// the old copy has no event, for example after a manual start with the argument.
pub fn signal_ready(old_pid: u32) {
    use windows_sys::Win32::System::Threading::{EVENT_MODIFY_STATE, OpenEventW, SetEvent};
    unsafe {
        let name = wide(&ready_name(old_pid));
        let handle = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
        #[cfg(feature = "e2e")]
        crate::e2e::log(&format!(
            "ready event signalled old_pid={old_pid} opened={}",
            !handle.is_null()
        ));
        if !handle.is_null() {
            SetEvent(handle);
            CloseHandle(handle);
        }
    }
}

/// Wait until the process with this id has ended, for `timeout` at most. True when it has
/// ended. A process that cannot be opened (it is gone, or it is not ours) counts as ended.
pub fn wait_for_exit(pid: u32, timeout: std::time::Duration) -> bool {
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::System::Threading::{PROCESS_SYNCHRONIZE, WaitForSingleObject};
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return true;
        }
        let ms = timeout.as_millis().min(u128::from(u32::MAX - 1)) as u32;
        let ended = WaitForSingleObject(handle, ms) == WAIT_OBJECT_0;
        CloseHandle(handle);
        ended
    }
}

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The name of our Run value.
const RUN_VALUE: &str = "Hexsight";

/// The Run value of the app before it became Hexsight. Legacy name, keep it.
const LEGACY_RUN_VALUE: &str = "Augur";

/// Start with Windows (HKCU Run key). The app then starts minimized.
pub fn set_autostart(on: bool) -> bool {
    set_run_value(RUN_VALUE, on)
}

pub fn autostart_enabled() -> bool {
    run_value_exists(RUN_VALUE)
}

fn set_run_value(value: &str, on: bool) -> bool {
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
    };
    let key = wide(RUN_KEY);
    let name = wide(value);
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else {
                return false;
            };
            let cmd = wide(&format!("\"{}\" --background", exe.display()));
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                cmd.as_ptr().cast(),
                (cmd.len() * 2) as u32,
            ) == 0
        } else {
            let r = RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr());
            r == 0 || r == 2 // 2 = not found
        }
    }
}

fn run_value_exists(value: &str) -> bool {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
    let key = wide(RUN_KEY);
    let name = wide(value);
    let mut size = 0u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        ) == 0
    }
}

/// What the start does with the legacy Run value.
#[derive(Debug, PartialEq, Eq)]
enum AutostartMigration {
    /// There is no legacy value.
    Nothing,
    /// The legacy value was on: delete it and set the new value.
    Move,
}

/// The legacy value means that the user had "Start with Windows" on. A portable
/// copy leaves it alone: it never writes a Run value that the user did not set.
fn autostart_migration(legacy_present: bool, portable: bool) -> AutostartMigration {
    if legacy_present && !portable {
        AutostartMigration::Move
    } else {
        AutostartMigration::Nothing
    }
}

/// At the start: take over the "Start with Windows" setting of the legacy app.
/// The legacy value goes away, and the new value comes in its place.
pub fn migrate_legacy_autostart(portable: bool) {
    if autostart_migration(run_value_exists(LEGACY_RUN_VALUE), portable) == AutostartMigration::Move
    {
        // Set the new value first, so a failure never leaves the user with none.
        if set_run_value(RUN_VALUE, true) {
            set_run_value(LEGACY_RUN_VALUE, false);
        }
    }
}

/// Give unused memory pages back to Windows. Pages come back on demand.
/// Hexsight calls this after start and during games, when it mostly waits.
pub fn trim_memory() {
    use windows_sys::Win32::System::ProcessStatus::K32EmptyWorkingSet;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        K32EmptyWorkingSet(GetCurrentProcess());
    }
}

/// Hide a window from screen captures (the card reader, screenshots, streams).
/// Used for the overlay, so the card reader never reads its own list.
pub fn exclude_from_capture(hwnd: isize, on: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
    };
    unsafe {
        SetWindowDisplayAffinity(
            hwnd as _,
            if on { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE },
        );
    }
}

/// The first top-level window with this title.
pub fn find_window(title: &str) -> Option<isize> {
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;
    let t = wide(title);
    let hwnd = unsafe { FindWindowW(std::ptr::null(), t.as_ptr()) };
    (!hwnd.is_null()).then_some(hwnd as isize)
}

/// Make a window a layer over the game, and keep it one. The layer never takes
/// the focus, has no taskbar button, hides from screen captures (the card
/// reader never sees it), and sits above the game on `rect` (left, top, right,
/// bottom, in pixels). Returns true when it had to change something.
pub fn fix_layer(hwnd: isize, rect: [i32; 4]) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowDisplayAffinity, GetWindowLongPtrW, HWND_TOPMOST, SWP_NOACTIVATE,
        SetWindowLongPtrW, SetWindowPos, WDA_EXCLUDEFROMCAPTURE, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    };
    let h = hwnd as _;
    let mut changed = false;
    unsafe {
        let style = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let want = (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize;
        if style & want != want {
            SetWindowLongPtrW(h, GWL_EXSTYLE, style | want);
            changed = true;
        }
        let mut affinity = 0u32;
        if GetWindowDisplayAffinity(h, &mut affinity) == 0 || affinity != WDA_EXCLUDEFROMCAPTURE {
            exclude_from_capture(hwnd, true);
            changed = true;
        }
        if window_rect(hwnd) != Some(rect) {
            SetWindowPos(
                h,
                HWND_TOPMOST,
                rect[0],
                rect[1],
                rect[2] - rect[0],
                rect[3] - rect[1],
                SWP_NOACTIVATE,
            );
            changed = true;
        }
    }
    changed
}

/// Show a minimized window without taking the focus (the game keeps it),
/// and put it above normal windows.
pub fn show_no_activate(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_TOP, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        ShowWindow,
    };
    unsafe {
        ShowWindow(hwnd as _, SW_SHOWNOACTIVATE);
        SetWindowPos(
            hwnd as _,
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        );
    }
}

/// The rect that covers all monitors: (x, y, width, height).
pub fn virtual_screen() -> (f32, f32, f32, f32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN) as f32,
            GetSystemMetrics(SM_YVIRTUALSCREEN) as f32,
            GetSystemMetrics(SM_CXVIRTUALSCREEN) as f32,
            GetSystemMetrics(SM_CYVIRTUALSCREEN) as f32,
        )
    }
}

/// The time zone of this PC, read once. The function it returns gives the seconds
/// that the local time is ahead of UTC at a UTC instant (Unix seconds), with the
/// daylight saving rule of the zone (+10800 for Turkey). It gives 0 when Windows
/// does not tell the zone, and for an instant that Windows cannot convert.
pub fn local_offset() -> impl Fn(i64) -> i64 {
    use windows_sys::Win32::System::Time::{
        GetTimeZoneInformation, TIME_ZONE_ID_INVALID, TIME_ZONE_INFORMATION,
    };
    // A TIME_ZONE_INFORMATION holds only numbers, so all zeros is a valid value.
    let mut zone: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    let known = unsafe { GetTimeZoneInformation(&mut zone) } != TIME_ZONE_ID_INVALID;
    move |utc| if known { offset_in(&zone, utc) } else { 0 }
}

/// The offset of `zone` at a UTC instant: the local clock that Windows shows, minus the UTC clock.
fn offset_in(zone: &windows_sys::Win32::System::Time::TIME_ZONE_INFORMATION, utc: i64) -> i64 {
    use crate::history::{civil_from_days, days_from_civil};
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::Time::SystemTimeToTzSpecificLocalTime;
    // A Windows date runs from 1601 to 30827. A game is never older than 1970.
    if !(0..=253_402_300_799).contains(&utc) {
        return 0;
    }
    let (year, month, day) = civil_from_days(utc.div_euclid(86_400));
    let rest = utc.rem_euclid(86_400);
    let at = SYSTEMTIME {
        wYear: year as u16,
        wMonth: month as u16,
        wDayOfWeek: 0,
        wDay: day as u16,
        wHour: (rest / 3600) as u16,
        wMinute: (rest % 3600 / 60) as u16,
        wSecond: (rest % 60) as u16,
        wMilliseconds: 0,
    };
    let mut local = SYSTEMTIME::default();
    if unsafe { SystemTimeToTzSpecificLocalTime(zone, &at, &mut local) } == 0 {
        return 0;
    }
    let days = days_from_civil(
        i64::from(local.wYear),
        u32::from(local.wMonth),
        u32::from(local.wDay),
    );
    days * 86_400
        + i64::from(local.wHour) * 3600
        + i64::from(local.wMinute) * 60
        + i64::from(local.wSecond)
        - utc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_pc_has_a_local_offset_that_a_real_zone_can_have() {
        let offset = local_offset();
        for utc in [1_700_000_000, 1_792_747_231, 1_800_000_000] {
            let o = offset(utc);
            // Real zones lie between UTC-12 and UTC+14, in steps of 15 minutes.
            assert!(
                (-12 * 3600..=14 * 3600).contains(&o) && o % 900 == 0,
                "{utc}: {o}"
            );
        }
        // An instant that Windows cannot convert gives 0.
        assert_eq!(offset(-1), 0);
    }

    fn utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn parses_the_build_number() {
        assert_eq!(parse_build(&utf16("26200")), 26200);
        // The registry text ends with a zero.
        assert_eq!(parse_build(&wide("22621")), 22621);
        assert_eq!(parse_build(&[]), 0);
        assert_eq!(parse_build(&utf16("abc")), 0);
    }

    #[test]
    fn glass_starts_at_windows_11_22h2() {
        // Windows 10 22H2 and Windows 11 21H2 have no system backdrop type.
        assert!(!build_has_glass(19045));
        assert!(!build_has_glass(22000));
        assert!(!build_has_glass(22620));
        assert!(build_has_glass(22621));
        assert!(build_has_glass(26200));
    }

    #[test]
    fn this_pc_answers_with_a_build() {
        // Every Windows that runs the tests has a build number.
        assert!(windows_build() > 10000);
    }

    #[test]
    fn hotkey_keys_map_to_virtual_keys() {
        use egui::Key;
        assert_eq!(vk_for_key(Key::F1), Some(0x70));
        assert_eq!(vk_for_key(Key::F8), Some(0x77));
        assert_eq!(vk_for_key(Key::F9), Some(0x78));
        assert_eq!(vk_for_key(Key::F12), Some(0x7B));
        assert_eq!(vk_for_key(Key::A), Some(0x41));
        assert_eq!(vk_for_key(Key::F), Some(0x46));
        assert_eq!(vk_for_key(Key::Z), Some(0x5A));
        assert_eq!(vk_for_key(Key::Num0), Some(0x30));
        assert_eq!(vk_for_key(Key::Num9), Some(0x39));
        // Other keys cannot be a hotkey.
        for key in [
            Key::Escape,
            Key::Space,
            Key::Tab,
            Key::F13,
            Key::ArrowUp,
            Key::Minus,
            Key::Enter,
        ] {
            assert_eq!(vk_for_key(key), None, "{key:?}");
        }
    }

    #[test]
    fn exactly_48_keys_can_be_hotkeys() {
        // F1 to F12, A to Z, 0 to 9: each has its own code and a plain name.
        let mut codes: Vec<i32> = egui::Key::ALL
            .iter()
            .filter_map(|&k| vk_for_key(k))
            .collect();
        assert_eq!(codes.len(), 12 + 26 + 10);
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), 48);
        for vk in codes {
            assert!(!key_label(vk).starts_with("Key "), "{vk}");
        }
    }

    #[test]
    fn hotkeys_have_a_name_on_the_screen() {
        assert_eq!(key_label(0x77), "F8");
        assert_eq!(key_label(0x78), "F9");
        assert_eq!(key_label(0x70), "F1");
        assert_eq!(key_label(0x7B), "F12");
        assert_eq!(key_label(0x41), "A");
        assert_eq!(key_label(0x35), "5");
        // Insert is not in the set. A hand-edited settings file can still use it.
        assert_eq!(key_label(0x2D), "Key 45");
    }

    #[test]
    fn a_key_that_is_registered_is_not_free() {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
        };
        // F24 (0x87) has no key on a keyboard, so no app holds it.
        let vk = 0x87;
        unsafe {
            if RegisterHotKey(std::ptr::null_mut(), 0x4148, MOD_NOREPEAT, vk as u32) == 0 {
                return;
            }
            assert!(!hotkey_free(vk), "a key that this thread holds is not free");
            UnregisterHotKey(std::ptr::null_mut(), 0x4148);
        }
        assert!(hotkey_free(vk));
        // The check leaves nothing behind: it can run again and again.
        assert!(hotkey_free(vk) && hotkey_free(vk));
    }

    #[test]
    fn the_legacy_run_value_moves_only_when_it_exists() {
        assert_eq!(autostart_migration(true, false), AutostartMigration::Move);
        assert_eq!(
            autostart_migration(false, false),
            AutostartMigration::Nothing
        );
        assert_ne!(RUN_VALUE, LEGACY_RUN_VALUE);
    }

    #[test]
    fn a_portable_copy_never_takes_over_the_legacy_run_value() {
        // The new value would point at the portable exe, for example on a stick.
        assert_eq!(autostart_migration(true, true), AutostartMigration::Nothing);
        assert_eq!(
            autostart_migration(false, true),
            AutostartMigration::Nothing
        );
    }

    #[test]
    fn no_window_has_no_rect() {
        assert_eq!(window_rect(0), None);
    }

    #[test]
    fn a_mutex_that_is_closed_can_be_taken_again() {
        let name = format!(r"Local\Hexsight.Test.{}", std::process::id());
        let first = take_mutex(&name).expect("the first try owns it");
        // The mutex is held: every try fails, and none leaks a handle.
        for _ in 0..5 {
            assert!(take_mutex(&name).is_none());
        }
        assert!(!wait_for_mutex(
            &name,
            std::time::Duration::from_millis(250)
        ));
        // The owner lets go. Because the failed tries closed their handles, the next try wins.
        unsafe { CloseHandle(first) };
        assert!(take_mutex(&name).is_some());
    }

    #[test]
    fn the_wait_ends_when_the_old_copy_lets_go_of_the_mutex() {
        let name = format!(r"Local\Hexsight.Test.Wait.{}", std::process::id());
        let held = take_mutex(&name).expect("owned") as usize;
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            unsafe { CloseHandle(held as windows_sys::Win32::Foundation::HANDLE) };
        });
        let t = std::time::Instant::now();
        assert!(wait_for_mutex(&name, std::time::Duration::from_secs(10)));
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
        releaser.join().unwrap();
    }

    #[test]
    fn the_wait_for_a_process_sees_a_running_and_an_ended_process() {
        // This test process runs: the wait times out.
        assert!(!wait_for_exit(
            std::process::id(),
            std::time::Duration::from_millis(100)
        ));
        // A short command ends, so the wait returns early.
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "exit", "0"])
            .spawn()
            .expect("cmd");
        let pid = child.id();
        assert!(wait_for_exit(pid, std::time::Duration::from_secs(10)));
        let _ = child.wait();
        // A process id that is gone counts as ended.
        assert!(wait_for_exit(pid, std::time::Duration::from_millis(100)));
    }

    #[test]
    fn the_new_copy_tells_the_old_one_that_it_is_up() {
        // This process plays the old copy.
        let event = ReadyEvent::create().expect("event");
        assert!(!event.is_set());
        // A signal for another process id changes nothing.
        signal_ready(std::process::id().wrapping_add(1));
        assert!(!event.is_set());
        signal_ready(std::process::id());
        assert!(event.is_set());
        // Without an event, the signal does nothing and does not fail.
        drop(event);
        signal_ready(std::process::id());
    }
}
