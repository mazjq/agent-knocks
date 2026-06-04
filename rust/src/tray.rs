// Agent Knocks - Windows tray visual (slice 2b). Tri-state colored dot + tooltip +
// context menu, driven by the engine (app.rs). A Win32 message pump keeps tray-icon
// alive; the notify watcher feeds reloads. Sound / i18n / full menu land in later slices.
use crate::app::{now_unix, App};
use crate::core::Status;
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};

fn color(s: Status) -> (u8, u8, u8) {
    match s {
        Status::Waiting => (255, 165, 0),
        Status::Processing => (30, 144, 255),
        Status::Done => (50, 205, 50),
        Status::Idle => (150, 150, 150),
    }
}

fn label(s: Status) -> &'static str {
    match s {
        Status::Waiting => "Waiting",
        Status::Processing => "Working",
        Status::Done => "Done",
        Status::Idle => "Idle",
    }
}

fn glyph(s: Status) -> &'static str {
    match s {
        Status::Waiting => "\u{1F7E0}", // orange circle
        Status::Processing => "\u{1F535}", // blue circle
        Status::Done => "\u{1F7E2}",    // green circle
        Status::Idle => "\u{26AA}",     // white circle
    }
}

// 16x16 RGBA filled circle with a 1px anti-aliased edge.
fn dot_icon(rgb: (u8, u8, u8)) -> Icon {
    let size: u32 = 16;
    let (cx, cy, rad) = (7.5f32, 7.5f32, 6.0f32);
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

fn count_suffix(app: &App) -> String {
    let (w, p, d) = app.counts();
    if w + p + d == 0 {
        return String::new();
    }
    let mut parts = Vec::new();
    if w > 0 {
        parts.push(format!("{} waiting", w));
    }
    if p > 0 {
        parts.push(format!("{} working", p));
    }
    if d > 0 {
        parts.push(format!("{} done", d));
    }
    format!("  ({})", parts.join(", "))
}

fn tooltip(app: &App, agg: Status) -> String {
    let t = format!("Agent Knocks - {}{}", label(agg), count_suffix(app));
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

struct Ids {
    open: MenuId,
    quit: MenuId,
}

fn build_menu(app: &App, agg: Status) -> (Menu, Ids) {
    let menu = Menu::new();
    let _ = menu.append(&MenuItem::new(
        format!("{}{}", label(agg), count_suffix(app)),
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
        let _ = menu.append(&MenuItem::new("(no active sessions)", false, None));
    } else {
        let now = now_unix();
        for s in &sessions {
            let line = format!(
                "{} {} - {} - {}  [{} #{}]",
                glyph(s.state),
                s.agent,
                label(s.state),
                elapsed(now, s.updated),
                s.title,
                s.tag
            );
            let _ = menu.append(&MenuItem::new(line, false, None));
        }
    }

    let _ = menu.append(&PredefinedMenuItem::separator());
    let open = MenuItem::new("Open state folder", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let ids = Ids {
        open: open.id().clone(),
        quit: quit.id().clone(),
    };
    let _ = menu.append(&open);
    let _ = menu.append(&quit);
    (menu, ids)
}

// Drain pending Win32 messages so tray-icon's hidden window proc runs.
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

    let mut app = App::new(crate::app::data_root());
    let (mut agg, _) = app.reload();

    let (menu, mut ids) = build_menu(&app, agg);
    let tray = TrayIconBuilder::new()
        .with_tooltip(tooltip(&app, agg))
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

    loop {
        pump();

        while let Ok(ev) = menu_rx.try_recv() {
            if ev.id == ids.quit {
                return;
            } else if ev.id == ids.open {
                let _ = std::process::Command::new("explorer.exe")
                    .arg(&app.state_dir)
                    .spawn();
            }
        }

        let mut changed = false;
        while rx.try_recv().is_ok() {
            changed = true;
        }
        if changed {
            std::thread::sleep(Duration::from_millis(120)); // debounce burst
            while rx.try_recv().is_ok() {}
        }

        if changed || last_tick.elapsed() >= Duration::from_secs(2) {
            last_tick = Instant::now();
            let (a, _cues) = app.reload();
            agg = a;
            let _ = tray.set_icon(Some(dot_icon(color(agg))));
            let _ = tray.set_tooltip(Some(tooltip(&app, agg)));
            let (menu, new_ids) = build_menu(&app, agg);
            let _ = tray.set_menu(Some(Box::new(menu)));
            ids = new_ids;
            // _cues -> sound earcons in a later slice
        }

        std::thread::sleep(Duration::from_millis(60));
    }
}
