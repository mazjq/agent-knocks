// Agent Knocks - Windows tray (slices 2b/3/4). Tri-state colored dot + tooltip +
// full context menu, intuitive sound earcons, and EN/中文 i18n. Driven by the
// app.rs engine; a Win32 message pump keeps tray-icon's window proc alive.
use crate::app::{now_unix, App};
use crate::core::{Session, Status};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, MouseButton, TrayIconBuilder, TrayIconEvent};

// ---- language ----

#[derive(Clone, Copy, PartialEq)]
enum Lang {
    En,
    Zh,
}

fn t_status(s: Status, l: Lang) -> &'static str {
    match (l, s) {
        (Lang::En, Status::Waiting) => "Waiting",
        (Lang::En, Status::Processing) => "Working",
        (Lang::En, Status::Done) => "Done",
        (Lang::En, Status::Idle) => "Idle",
        (Lang::Zh, Status::Waiting) => "等待确认",
        (Lang::Zh, Status::Processing) => "处理中",
        (Lang::Zh, Status::Done) => "已完成",
        (Lang::Zh, Status::Idle) => "空闲",
    }
}

fn t_count_word(l: Lang, which: u8) -> &'static str {
    // which: 0=waiting 1=working 2=done
    match (l, which) {
        (Lang::En, 0) => "waiting",
        (Lang::En, 1) => "working",
        (Lang::En, _) => "done",
        (Lang::Zh, 0) => "等待",
        (Lang::Zh, 1) => "处理中",
        (Lang::Zh, _) => "完成",
    }
}

fn t_no_sessions(l: Lang) -> &'static str {
    match l {
        Lang::En => "(no active sessions)",
        Lang::Zh => "（无活动会话）",
    }
}
fn t_mute(l: Lang, muted: bool) -> &'static str {
    match (l, muted) {
        (Lang::En, false) => "🔇 Mute",
        (Lang::En, true) => "🔔 Unmute",
        (Lang::Zh, false) => "🔇 静音",
        (Lang::Zh, true) => "🔔 取消静音",
    }
}
fn t_test(l: Lang) -> &'static str {
    match l {
        Lang::En => "🔊 Test sound",
        Lang::Zh => "🔊 测试声音",
    }
}
fn t_test_wait(l: Lang) -> &'static str {
    match l {
        Lang::En => "Waiting sound",
        Lang::Zh => "等待确认音",
    }
}
fn t_test_done(l: Lang) -> &'static str {
    match l {
        Lang::En => "Done sound",
        Lang::Zh => "完成音",
    }
}
fn t_open(l: Lang) -> &'static str {
    match l {
        Lang::En => "📁 Open state folder",
        Lang::Zh => "📁 打开状态目录",
    }
}
fn t_language(l: Lang) -> &'static str {
    match l {
        Lang::En => "🌐 Language",
        Lang::Zh => "🌐 语言",
    }
}
fn t_quit(l: Lang) -> &'static str {
    match l {
        Lang::En => "❌ Quit",
        Lang::Zh => "❌ 退出",
    }
}
fn t_autostart(l: Lang) -> &'static str {
    match l {
        Lang::En => "⏻ Start at login",
        Lang::Zh => "⏻ 开机自启",
    }
}
fn t_head_wait(l: Lang) -> &'static str {
    match l {
        Lang::En => "needs your confirmation",
        Lang::Zh => "需要你确认",
    }
}
fn t_head_done(l: Lang) -> &'static str {
    match l {
        Lang::En => "done",
        Lang::Zh => "处理完成",
    }
}
fn t_jump_hint(l: Lang) -> &'static str {
    match l {
        Lang::En => "↗ jump",
        Lang::Zh => "↗ 跳转",
    }
}
fn t_clear_done(l: Lang) -> &'static str {
    match l {
        Lang::En => "🧹 Clear completed",
        Lang::Zh => "🧹 清除已完成",
    }
}

// ---- icon / colors ----

fn color(s: Status) -> (u8, u8, u8) {
    match s {
        Status::Waiting => (255, 165, 0),
        Status::Processing => (30, 144, 255),
        Status::Done => (50, 205, 50),
        Status::Idle => (150, 150, 150),
    }
}

fn glyph(s: Status) -> &'static str {
    match s {
        Status::Waiting => "\u{1F7E0}",
        Status::Processing => "\u{1F535}",
        Status::Done => "\u{1F7E2}",
        Status::Idle => "\u{26AA}",
    }
}

// 32x32 RGBA filled circle (~84% fill) with a 1px anti-aliased edge — crisp and a
// touch larger than the old 16px dot.
fn dot_icon(rgb: (u8, u8, u8)) -> Icon {
    let size: u32 = 32;
    let (cx, cy, rad) = (15.5f32, 15.5f32, 13.5f32);
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            let a = if d <= rad - 0.5 {
                255.0
            } else if d <= rad + 0.5 {
                255.0 * (rad + 0.5 - d)
            } else {
                0.0
            };
            let i = ((y * size + x) * 4) as usize;
            rgba[i] = rgb.0;
            rgba[i + 1] = rgb.1;
            rgba[i + 2] = rgb.2;
            rgba[i + 3] = a as u8;
        }
    }
    Icon::from_rgba(rgba, size, size).expect("icon from rgba")
}

fn count_suffix(app: &App, l: Lang) -> String {
    let (w, p, d) = app.counts();
    if w + p + d == 0 {
        return String::new();
    }
    let sp = if l == Lang::Zh { " " } else { " " };
    let mut parts = Vec::new();
    if w > 0 {
        parts.push(format!("{}{}{}", w, sp, t_count_word(l, 0)));
    }
    if p > 0 {
        parts.push(format!("{}{}{}", p, sp, t_count_word(l, 1)));
    }
    if d > 0 {
        parts.push(format!("{}{}{}", d, sp, t_count_word(l, 2)));
    }
    format!("  ({})", parts.join(", "))
}

fn tooltip(app: &App, agg: Status, l: Lang) -> String {
    let t = format!("Agent Knocks - {}{}", t_status(agg, l), count_suffix(app, l));
    if t.chars().count() > 120 {
        t.chars().take(120).collect()
    } else {
        t
    }
}

fn elapsed(now: i64, updated: i64) -> String {
    let s = (now - updated).max(0);
    if s < 60 {
        format!("{}s", s)
    } else if s < 3600 {
        format!("{}m{}s", s / 60, s % 60)
    } else {
        format!("{}h{}m", s / 3600, (s % 3600) / 60)
    }
}

// ---- menu ----

// Stable menu-item ids. The menu is rebuilt every ≤2s (session timers tick), which
// previously regenerated auto-incrementing MenuIds; a click on an item whose id had
// just been swapped under it was dropped (issue #4: Mute appeared to do nothing).
// Fixing the ids makes `ev.id == ID_*` rebuild-proof for every action. Per-session
// lines use a stable `session:<key>` id so jumping survives rebuilds too.
const ID_OPEN: &str = "open";
const ID_QUIT: &str = "quit";
const ID_MUTE: &str = "mute";
const ID_TEST_WAIT: &str = "test_wait";
const ID_TEST_DONE: &str = "test_done";
const ID_LANG_EN: &str = "lang_en";
const ID_LANG_ZH: &str = "lang_zh";
const ID_START_LOGIN: &str = "start_login";
const ID_CLEAR_DONE: &str = "clear_done";
const SESSION_ID_PREFIX: &str = "session:";

// Returns the menu plus the per-session id->Session map (for click-to-focus). All
// other items carry the stable ID_* ids above, matched directly in the run loop.
fn build_menu(app: &App, agg: Status, l: Lang, muted: bool) -> (Menu, Vec<(MenuId, Session)>) {
    let menu = Menu::new();
    let _ = menu.append(&MenuItem::new(
        format!("{}{}", t_status(agg, l), count_suffix(app, l)),
        false,
        None,
    ));
    let _ = menu.append(&PredefinedMenuItem::separator());

    let mut sessions = app.sessions();
    sessions.sort_by(|a, b| {
        (b.state as i32)
            .cmp(&(a.state as i32))
            .then(b.updated.cmp(&a.updated))
    });
    let mut session_items: Vec<(MenuId, Session)> = Vec::new();
    if sessions.is_empty() {
        let _ = menu.append(&MenuItem::new(t_no_sessions(l), false, None));
    } else {
        let now = now_unix();
        for s in &sessions {
            // clickable (enabled) so you can jump to a session anytime from the menu,
            // not just during the brief toast
            let line = format!(
                "{} {} - {} - {}  [{} #{}]  {}",
                glyph(s.state),
                s.agent,
                t_status(s.state, l),
                elapsed(now, s.updated),
                s.title,
                s.tag,
                t_jump_hint(l)
            );
            let id = MenuId::new(format!("{}{}", SESSION_ID_PREFIX, s.key));
            let it = MenuItem::with_id(id.clone(), line, true, None);
            session_items.push((id, (*s).clone()));
            let _ = menu.append(&it);
        }
    }

    let _ = menu.append(&PredefinedMenuItem::separator());

    let mute = MenuItem::with_id(ID_MUTE, t_mute(l, muted), true, None);
    let test_wait = MenuItem::with_id(ID_TEST_WAIT, t_test_wait(l), true, None);
    let test_done = MenuItem::with_id(ID_TEST_DONE, t_test_done(l), true, None);
    let test = Submenu::new(t_test(l), true);
    let _ = test.append(&test_wait);
    let _ = test.append(&test_done);
    let open = MenuItem::with_id(ID_OPEN, t_open(l), true, None);

    let lang_en = CheckMenuItem::with_id(ID_LANG_EN, "English", true, l == Lang::En, None);
    let lang_zh = CheckMenuItem::with_id(ID_LANG_ZH, "中文", true, l == Lang::Zh, None);
    let language = Submenu::new(t_language(l), true);
    let _ = language.append(&lang_en);
    let _ = language.append(&lang_zh);

    let start_login =
        CheckMenuItem::with_id(ID_START_LOGIN, t_autostart(l), true, autostart_enabled(), None);
    let (_, _, done_n) = app.counts();
    let clear_done = MenuItem::with_id(ID_CLEAR_DONE, t_clear_done(l), done_n > 0, None);
    let quit = MenuItem::with_id(ID_QUIT, t_quit(l), true, None);

    let _ = menu.append(&mute);
    let _ = menu.append(&test);
    let _ = menu.append(&open);
    let _ = menu.append(&clear_done);
    let _ = menu.append(&language);
    let _ = menu.append(&start_login);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&quit);

    (menu, session_items)
}

// ---- sound (intuitive earcons via Win32 Beep, on a background thread) ----

#[derive(Clone, Copy)]
pub enum Cue {
    Waiting,
    Done,
}

#[link(name = "kernel32")]
extern "system" {
    fn Beep(dwfreq: u32, dwduration: u32) -> i32;
}

fn spawn_sound() -> Sender<Cue> {
    let (tx, rx) = std::sync::mpsc::channel::<Cue>();
    std::thread::spawn(move || {
        while let Ok(cue) = rx.recv() {
            unsafe {
                match cue {
                    Cue::Waiting => {
                        Beep(660, 130);
                        Beep(990, 170);
                    }
                    Cue::Done => {
                        Beep(770, 90);
                        Beep(1046, 90);
                        Beep(1318, 150);
                    }
                }
            }
        }
    });
    tx
}

// ---- config (muted + lang), mirrors the C# config.json ----

fn load_config(root: &Path) -> (bool, Lang) {
    let p = root.join("config.json");
    let txt = std::fs::read_to_string(&p).unwrap_or_default();
    let c: String = txt.chars().filter(|ch| !ch.is_whitespace()).collect();
    let muted = c.contains("\"muted\":true");
    let lang = if c.contains("\"lang\":\"zh\"") { Lang::Zh } else { Lang::En };
    (muted, lang)
}

fn save_config(root: &Path, muted: bool, l: Lang) {
    let lang = if l == Lang::Zh { "zh" } else { "en" };
    let content = format!("{{\"muted\":{},\"lang\":\"{}\"}}", muted, lang);
    let _ = std::fs::create_dir_all(root);
    let _ = std::fs::write(root.join("config.json"), content);
}

// ---- autostart (Windows HKCU Run, path quoted like the C# build) ----
// (Cross-platform auto-launch — macOS LaunchAgent / Linux .desktop — lands when
// those targets are added.)

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "AgentKnocks";

pub fn set_autostart(on: bool) {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(_) => return,
    };
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok((run, _)) = hkcu.create_subkey(RUN_KEY) {
        if on {
            let val = format!("\"{}\"", exe.display());
            let _ = run.set_value(RUN_NAME, &val);
        } else {
            let _ = run.delete_value(RUN_NAME);
        }
    }
}

fn autostart_enabled() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(RUN_KEY)
        .ok()
        .and_then(|run| run.get_value::<String, _>(RUN_NAME).ok())
        .is_some()
}

// ---- notification toast + click-to-focus ----

// Show a Windows toast (POWERSHELL_APP_ID works for unpackaged apps; branding is
// "Windows PowerShell" until a custom AppUserModelID is registered).
fn show_toast(title: &str, body: &str) {
    use tauri_winrt_notification::Toast;
    let _ = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(body)
        .show();
}

// Force a window to the foreground, restoring it if minimized. Uses the
// AttachThreadInput dance + BringWindowToTop so it actually raises (a plain
// SetForegroundWindow from a background process only flashes the taskbar).
fn focus_window(hwnd: i64) {
    if hwnd == 0 {
        return;
    }
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
        SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    unsafe {
        let h = hwnd as isize as HWND;
        // Only un-minimize; never touch a normal/maximized/fullscreen window's state
        // (SW_SHOW/SW_RESTORE on a maximized window would window-ize it).
        if IsIconic(h) != 0 {
            ShowWindow(h, SW_RESTORE);
        }
        let fg = GetForegroundWindow();
        let cur = GetCurrentThreadId();
        let fg_tid = GetWindowThreadProcessId(fg, std::ptr::null_mut());
        let tgt_tid = GetWindowThreadProcessId(h, std::ptr::null_mut());
        let a_fg = fg_tid != 0 && fg_tid != cur;
        let a_tgt = tgt_tid != 0 && tgt_tid != cur && tgt_tid != fg_tid;
        if a_fg {
            AttachThreadInput(cur, fg_tid, 1);
        }
        if a_tgt {
            AttachThreadInput(cur, tgt_tid, 1);
        }
        BringWindowToTop(h);
        SetForegroundWindow(h);
        if a_tgt {
            AttachThreadInput(cur, tgt_tid, 0);
        }
        if a_fg {
            AttachThreadInput(cur, fg_tid, 0);
        }
    }
}

// Snapshot of visible, titled, top-level windows: (hwnd, title, pid, proc). Minimized
// windows still appear (they keep WS_VISIBLE). `proc` is the owning process's exe name,
// used to find apps (e.g. Codex) whose window can't be matched by title or ancestry.
fn collect_windows() -> Vec<(i64, String, i64, String)> {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };
    struct C {
        items: Vec<(i64, String, i64)>,
    }
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> i32 {
        let c = &mut *(lparam as *mut C);
        if IsWindowVisible(hwnd) != 0 {
            let len = GetWindowTextLengthW(hwnd);
            if len > 0 {
                let mut buf = vec![0u16; (len + 1) as usize];
                let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
                if n > 0 {
                    let mut pid: u32 = 0;
                    GetWindowThreadProcessId(hwnd, &mut pid);
                    c.items.push((
                        hwnd as isize as i64,
                        String::from_utf16_lossy(&buf[..n as usize]),
                        pid as i64,
                    ));
                }
            }
        }
        1
    }
    unsafe {
        let mut c = C { items: Vec::new() };
        EnumWindows(Some(cb), &mut c as *mut C as LPARAM);
        let names = process_names();
        c.items
            .into_iter()
            .map(|(h, t, p)| {
                let name = names.get(&(p as u32)).cloned().unwrap_or_default();
                (h, t, p, name)
            })
            .collect()
    }
}

// pid -> exe name (e.g. "Codex.exe") for every running process, via a Toolhelp snapshot.
fn process_names() -> std::collections::HashMap<u32, String> {
    use std::collections::HashMap;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    let mut names: HashMap<u32, String> = HashMap::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return names;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut e) != 0 {
            loop {
                let s = &e.szExeFile;
                let len = s.iter().position(|&c| c == 0).unwrap_or(s.len());
                names.insert(e.th32ProcessID, String::from_utf16_lossy(&s[..len]));
                if Process32NextW(snap, &mut e) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    names
}

// Focus a session's window: a standalone GUI host (Codex) is found by process name;
// otherwise scope to the agent's host process and match the cwd folder names (deepest
// first) against window titles, then raise it.
fn focus_session(s: &Session) {
    let wins = collect_windows();
    let names = crate::core::cwd_names(&s.cwd, &s.title, 4);
    let name_refs: Vec<&str> = names.iter().map(|n| n.as_str()).collect();
    let host_proc = crate::core::host_process(&s.agent);
    if let Some(h) = crate::core::select_window(s.hwnd, s.pid, &name_refs, host_proc, &wins) {
        focus_window(h);
    }
}

// Focus the highest-priority session (waiting first).
fn focus_top_session(app: &App) {
    let mut sessions = app.sessions();
    sessions.sort_by(|a, b| {
        (b.state as i32)
            .cmp(&(a.state as i32))
            .then(b.updated.cmp(&a.updated))
    });
    if let Some(s) = sessions.first() {
        focus_session(s);
    }
}

// ---- Win32 message pump ----

fn pump() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

pub fn run() {
    use notify::{recommended_watcher, RecursiveMode, Watcher};

    let root = crate::app::data_root();
    let (mut muted, mut lang) = load_config(&root);
    let sound = spawn_sound();

    let mut app = App::new(root.clone());
    let (mut agg, _) = app.reload();

    let (menu, mut sessions) = build_menu(&app, agg, lang, muted);
    let tray = TrayIconBuilder::new()
        .with_tooltip(tooltip(&app, agg, lang))
        .with_icon(dot_icon(color(agg)))
        .with_menu(Box::new(menu))
        .build()
        .expect("build tray icon");
    // left-click = jump to the agent that needs you; right-click = menu
    let _ = tray.set_show_menu_on_left_click(false);

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = recommended_watcher(move |r| {
        let _ = tx.send(r);
    })
    .expect("watcher");
    let _ = watcher.watch(&app.state_dir, RecursiveMode::NonRecursive);

    let menu_rx = MenuEvent::receiver();
    let tray_rx = TrayIconEvent::receiver();
    let mut last_tick = Instant::now();

    // rebuild menu+tooltip (labels depend on lang/muted/sessions); returns the new
    // per-session id->Session map. Item ids are stable (ID_*), so this is rebuild-safe.
    let refresh = |tray: &tray_icon::TrayIcon, app: &App, agg, lang, muted| -> Vec<(MenuId, Session)> {
        let _ = tray.set_tooltip(Some(tooltip(app, agg, lang)));
        let (m, sessions) = build_menu(app, agg, lang, muted);
        let _ = tray.set_menu(Some(Box::new(m)));
        sessions
    };

    loop {
        pump();

        while let Ok(ev) = menu_rx.try_recv() {
            if ev.id == ID_QUIT {
                return;
            } else if ev.id == ID_OPEN {
                let _ = std::process::Command::new("explorer.exe")
                    .arg(&app.state_dir)
                    .spawn();
            } else if ev.id == ID_MUTE {
                muted = !muted;
                save_config(&root, muted, lang);
                sessions = refresh(&tray, &app, agg, lang, muted);
            } else if ev.id == ID_TEST_WAIT {
                let _ = sound.send(Cue::Waiting);
            } else if ev.id == ID_TEST_DONE {
                let _ = sound.send(Cue::Done);
            } else if ev.id == ID_LANG_EN {
                lang = Lang::En;
                save_config(&root, muted, lang);
                sessions = refresh(&tray, &app, agg, lang, muted);
            } else if ev.id == ID_LANG_ZH {
                lang = Lang::Zh;
                save_config(&root, muted, lang);
                sessions = refresh(&tray, &app, agg, lang, muted);
            } else if ev.id == ID_START_LOGIN {
                set_autostart(!autostart_enabled());
                sessions = refresh(&tray, &app, agg, lang, muted);
            } else if ev.id == ID_CLEAR_DONE {
                app.clear_done();
                agg = app.aggregate();
                let _ = tray.set_icon(Some(dot_icon(color(agg))));
                sessions = refresh(&tray, &app, agg, lang, muted);
            } else if let Some(item) = sessions.iter().find(|(id, _)| *id == ev.id) {
                focus_session(&item.1);
            }
        }

        // left-click the tray dot -> focus the agent that needs you
        while let Ok(ev) = tray_rx.try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = ev
            {
                focus_top_session(&app);
            }
        }

        let mut changed = false;
        while rx.try_recv().is_ok() {
            changed = true;
        }
        if changed {
            std::thread::sleep(Duration::from_millis(120));
            while rx.try_recv().is_ok() {}
        }

        if changed || last_tick.elapsed() >= Duration::from_secs(2) {
            last_tick = Instant::now();
            let (a, cues) = app.reload();
            agg = a;
            let _ = tray.set_icon(Some(dot_icon(color(agg))));
            sessions = refresh(&tray, &app, agg, lang, muted);
            for c in &cues {
                if !muted {
                    let _ = sound.send(if c.waiting { Cue::Waiting } else { Cue::Done });
                }
                let head = if c.waiting {
                    t_head_wait(lang)
                } else {
                    t_head_done(lang)
                };
                show_toast(
                    &format!("{} · {}", c.session.agent, head),
                    &format!("{} #{}", c.session.title, c.session.tag),
                );
            }
        }

        std::thread::sleep(Duration::from_millis(60));
    }
}
