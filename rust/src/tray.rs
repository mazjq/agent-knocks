// Agent Knocks - Windows tray (slices 2b/3/4). Tri-state colored dot + tooltip +
// full context menu, intuitive sound earcons, and EN/中文 i18n. Driven by the
// app.rs engine; a Win32 message pump keeps tray-icon's window proc alive.
use crate::app::{now_unix, App};
use crate::core::Status;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIconBuilder};

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

struct Ids {
    open: MenuId,
    quit: MenuId,
    mute: MenuId,
    test_wait: MenuId,
    test_done: MenuId,
    lang_en: MenuId,
    lang_zh: MenuId,
}

fn build_menu(app: &App, agg: Status, l: Lang, muted: bool) -> (Menu, Ids) {
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
    if sessions.is_empty() {
        let _ = menu.append(&MenuItem::new(t_no_sessions(l), false, None));
    } else {
        let now = now_unix();
        for s in &sessions {
            let line = format!(
                "{} {} - {} - {}  [{} #{}]",
                glyph(s.state),
                s.agent,
                t_status(s.state, l),
                elapsed(now, s.updated),
                s.title,
                s.tag
            );
            let _ = menu.append(&MenuItem::new(line, false, None));
        }
    }

    let _ = menu.append(&PredefinedMenuItem::separator());

    let mute = MenuItem::new(t_mute(l, muted), true, None);
    let test_wait = MenuItem::new(t_test_wait(l), true, None);
    let test_done = MenuItem::new(t_test_done(l), true, None);
    let test = Submenu::new(t_test(l), true);
    let _ = test.append(&test_wait);
    let _ = test.append(&test_done);
    let open = MenuItem::new(t_open(l), true, None);

    let lang_en = CheckMenuItem::new("English", true, l == Lang::En, None);
    let lang_zh = CheckMenuItem::new("中文", true, l == Lang::Zh, None);
    let language = Submenu::new(t_language(l), true);
    let _ = language.append(&lang_en);
    let _ = language.append(&lang_zh);

    let quit = MenuItem::new(t_quit(l), true, None);

    let ids = Ids {
        open: open.id().clone(),
        quit: quit.id().clone(),
        mute: mute.id().clone(),
        test_wait: test_wait.id().clone(),
        test_done: test_done.id().clone(),
        lang_en: lang_en.id().clone(),
        lang_zh: lang_zh.id().clone(),
    };

    let _ = menu.append(&mute);
    let _ = menu.append(&test);
    let _ = menu.append(&open);
    let _ = menu.append(&language);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&quit);

    (menu, ids)
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

    let (menu, mut ids) = build_menu(&app, agg, lang, muted);
    let tray = TrayIconBuilder::new()
        .with_tooltip(tooltip(&app, agg, lang))
        .with_icon(dot_icon(color(agg)))
        .with_menu(Box::new(menu))
        .build()
        .expect("build tray icon");

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = recommended_watcher(move |r| {
        let _ = tx.send(r);
    })
    .expect("watcher");
    let _ = watcher.watch(&app.state_dir, RecursiveMode::NonRecursive);

    let menu_rx = MenuEvent::receiver();
    let mut last_tick = Instant::now();

    // rebuild menu+tooltip (labels depend on lang/muted/sessions)
    let refresh = |tray: &tray_icon::TrayIcon, app: &App, agg, lang, muted| -> Ids {
        let _ = tray.set_tooltip(Some(tooltip(app, agg, lang)));
        let (m, ids) = build_menu(app, agg, lang, muted);
        let _ = tray.set_menu(Some(Box::new(m)));
        ids
    };

    loop {
        pump();

        while let Ok(ev) = menu_rx.try_recv() {
            if ev.id == ids.quit {
                return;
            } else if ev.id == ids.open {
                let _ = std::process::Command::new("explorer.exe")
                    .arg(&app.state_dir)
                    .spawn();
            } else if ev.id == ids.mute {
                muted = !muted;
                save_config(&root, muted, lang);
                ids = refresh(&tray, &app, agg, lang, muted);
            } else if ev.id == ids.test_wait {
                let _ = sound.send(Cue::Waiting);
            } else if ev.id == ids.test_done {
                let _ = sound.send(Cue::Done);
            } else if ev.id == ids.lang_en {
                lang = Lang::En;
                save_config(&root, muted, lang);
                ids = refresh(&tray, &app, agg, lang, muted);
            } else if ev.id == ids.lang_zh {
                lang = Lang::Zh;
                save_config(&root, muted, lang);
                ids = refresh(&tray, &app, agg, lang, muted);
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
            ids = refresh(&tray, &app, agg, lang, muted);
            if !muted {
                for c in &cues {
                    let _ = sound.send(if c.waiting { Cue::Waiting } else { Cue::Done });
                }
            }
        }

        std::thread::sleep(Duration::from_millis(60));
    }
}
