// Agent Knocks (Rust port) — entry point.
//   --emit : invoked by an agent hook; reads stdin(JSON)+args, writes/removes a state
//            file, exits. Pure observer (no stdout, exit 0) — never blocks the agent.
//   default: the resident tray (not yet implemented in the Rust port; the C# build
//            still ships the tray until this reaches parity).
// No console window in release (like the C# winexe): the tray runs detached, and
// emit hooks don't flash a console. Debug builds keep a console for dev/test stdout.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod core;
#[cfg(windows)]
mod tray;
use app::{data_root, now_unix};
use core::*;

use std::io::{Read, Write};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--emit") {
        std::process::exit(emit(&args));
    }
    if args.iter().any(|a| a == "--once") {
        app::run_once(data_root());
        return;
    }
    #[cfg(windows)]
    {
        if args.iter().any(|a| a == "--enable-autostart") {
            tray::set_autostart(true);
            return;
        }
        if args.iter().any(|a| a == "--disable-autostart") {
            tray::set_autostart(false);
            return;
        }
    }
    // --watch-secs N: headless watch loop, exits after N seconds (testing / non-Windows).
    if let Some(max) = arg_val(&args, "--watch-secs").and_then(|s| s.parse::<u64>().ok()) {
        app::run_headless(data_root(), Some(max));
        return;
    }
    // Default: the tray. (mac/Linux event loop is a later slice -> headless for now.)
    #[cfg(windows)]
    {
        tray::run();
    }
    #[cfg(not(windows))]
    {
        app::run_headless(data_root(), None);
    }
}

// ---- emit mode ----

fn emit(args: &[String]) -> i32 {
    // never disrupt the agent: swallow everything, always exit 0
    let agent = arg_val(args, "--agent").unwrap_or_else(|| "agent".to_string());
    let mut status = arg_val(args, "--status").unwrap_or_else(|| "processing".to_string());
    let title_arg = arg_val(args, "--title");
    let key_arg = arg_val(args, "--key");
    let stdin = read_stdin();

    // resolve session id / working dir
    let mut session = key_arg.unwrap_or_default();
    if session.is_empty() {
        session = json_str(&stdin, "session_id").unwrap_or_default();
    }
    if session.is_empty() {
        session = json_str(&stdin, "session").unwrap_or_default();
    }

    let mut cwd = json_str(&stdin, "cwd").unwrap_or_default();
    if cwd.is_empty() {
        cwd = json_str(&stdin, "workdir").unwrap_or_default();
    }

    let mut title = title_arg.unwrap_or_default();
    if title.is_empty() && !cwd.is_empty() {
        title = last_segment(&cwd);
    }
    let title_resolved = !title.is_empty();

    // status inference
    if status == "auto" {
        let mut blob = stdin.clone();
        for a in args {
            blob.push(' ');
            blob.push_str(a);
        }
        status = infer_auto(&blob).to_string();
    } else if status == "notify" {
        status = infer_notification(&stdin).to_string();
    }

    if session.is_empty() {
        session = format!("{}-default", agent);
    }
    let key = format!("{}__{}", sanitize(&agent), sanitize(&session));

    let root = data_root();
    let state_dir = root.join("state");
    let _ = std::fs::create_dir_all(&state_dir);
    let file = state_dir.join(format!("{}.json", key));

    log_event(&root, &agent, &status, &key, &stdin);

    // no-op: idle reminders etc. — log then exit without touching the state file
    if status == "ignore" {
        return 0;
    }
    if status == "end" || status == "exit" {
        let _ = std::fs::remove_file(&file);
        return 0;
    }

    // keep the previous title when none was parsed, so the project name isn't overwritten
    if !title_resolved && file.exists() {
        if let Ok(prev) = std::fs::read_to_string(&file) {
            let p = json_str(&prev, "title").unwrap_or_default();
            if !p.is_empty() {
                title = p;
            }
        }
    }
    if title.is_empty() {
        title = agent.clone();
    }

    let norm = status_norm(&status);
    let ts = now_unix();
    let json = format!(
        "{{\"agent\":\"{}\",\"session\":\"{}\",\"status\":\"{}\",\"title\":\"{}\",\"ts\":{}}}",
        json_esc(&agent),
        json_esc(&session),
        norm,
        json_esc(&title),
        ts
    );

    // atomic-ish: write tmp then rename
    let tmp = state_dir.join(format!("{}.json.tmp", key));
    if std::fs::write(&tmp, json.as_bytes()).is_ok() {
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::rename(&tmp, &file);
    }
    0
}

// ---- helpers ----

fn arg_val(args: &[String], name: &str) -> Option<String> {
    let mut i = 0;
    while i + 1 < args.len() {
        if args[i] == name {
            return Some(args[i + 1].clone());
        }
        i += 1;
    }
    None
}

fn read_stdin() -> String {
    // hooks pipe JSON with EOF; if there's no pipe this returns empty.
    let mut s = String::new();
    let _ = std::io::stdin().read_to_string(&mut s);
    s
}

fn last_segment(p: &str) -> String {
    let p = p.replace('/', "\\");
    let p = p.trim_end_matches('\\');
    match p.rsplit('\\').next() {
        Some(seg) => seg.to_string(),
        None => p.to_string(),
    }
}

fn sanitize(s: &str) -> String {
    if s.is_empty() {
        return "x".to_string();
    }
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect()
}

// Diagnostic log, auto-reset past 200KB (mirrors the C# events.log).
fn log_event(root: &PathBuf, agent: &str, status: &str, key: &str, stdin: &str) {
    let _ = std::fs::create_dir_all(root);
    let log = root.join("events.log");
    if let Ok(meta) = std::fs::metadata(&log) {
        if meta.len() > 200 * 1024 {
            let _ = std::fs::remove_file(&log);
        }
    }
    let msg = json_str(stdin, "message").unwrap_or_default().replace('\n', " ");
    let hook = json_str(stdin, "hook_event_name").unwrap_or_default();
    let mut line = format!("{}  status={}  key={}", agent, status, key);
    if !hook.is_empty() {
        line.push_str(&format!("  hook={}", hook));
    }
    if !msg.is_empty() {
        line.push_str(&format!("  msg={}", msg));
    }
    line.push('\n');
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
        let _ = f.write_all(line.as_bytes());
    }
}
