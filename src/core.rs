// Agent Knocks - state core (no UI, platform-agnostic). Ported from the C# Core.cs.
// Zero external dependencies: a tiny flat-JSON extractor + a pure state machine,
// so `cargo test` stays fast and the logic is identical across platforms.
#![allow(dead_code)]

use std::collections::HashMap;

// Priority: Waiting > Processing > Done > Idle.
// Ord derives from declaration order, so the discriminants encode the priority.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Status {
    Idle = 0,
    Done = 1,
    Processing = 2,
    Waiting = 3,
}

impl Status {
    // Parse the normalized status string written into state files.
    pub fn from_status_str(s: &str) -> Status {
        match s {
            "waiting" => Status::Waiting,
            "done" => Status::Done,
            "processing" => Status::Processing,
            _ => Status::Processing,
        }
    }
}

// Normalize various external spellings -> processing / waiting / done.
pub fn status_norm(s: &str) -> &'static str {
    let s = s.trim().to_lowercase();
    match s.as_str() {
        "waiting" | "wait" | "confirm" | "approval" | "permission" | "needs_input" => "waiting",
        "done" | "complete" | "completed" | "finished" | "stop" => "done",
        _ => "processing",
    }
}

// ---- tiny flat-JSON helpers (regex-free, UTF-8 safe) ----

// Extract a string field value, undoing JSON escapes. None if absent.
pub fn json_str(json: &str, field: &str) -> Option<String> {
    let needle = format!("\"{}\"", field); // quoted -> no prefix collision (session vs session_id)
    let pos = json.find(&needle)? + needle.len();
    let mut chars = json[pos..].chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == ':' {
            chars.next();
        } else {
            break;
        }
    }
    if chars.peek() != Some(&'"') {
        return None;
    }
    chars.next(); // opening quote
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(e) = chars.next() {
                match e {
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '/' => out.push('/'),
                    other => out.push(other),
                }
            }
        } else if c == '"' {
            return Some(out);
        } else {
            out.push(c);
        }
    }
    Some(out)
}

// Extract an integer field value; 0 if absent/unparseable.
pub fn json_long(json: &str, field: &str) -> i64 {
    let needle = format!("\"{}\"", field);
    let pos = match json.find(&needle) {
        Some(p) => p + needle.len(),
        None => return 0,
    };
    let mut chars = json[pos..].chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == ':' {
            chars.next();
        } else {
            break;
        }
    }
    let mut num = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            num.push(c);
            chars.next();
        } else {
            break;
        }
    }
    num.parse::<i64>().unwrap_or(0)
}

// Escape a string for embedding in a JSON value (drops CR, like the C# version).
pub fn json_esc(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\r' => {}
            _ => o.push(c),
        }
    }
    o
}

// ---- session ----

#[derive(Clone, Debug)]
pub struct Session {
    pub agent: String,
    pub key: String,
    pub title: String,
    pub tag: String, // short session tag, disambiguates same-project windows
    pub state: Status,
    pub updated: i64, // unix seconds
    pub hwnd: i64,    // host window handle captured at emit time (0 if none); click-to-focus
    pub pid: i64,     // owning process of the host window; scopes focus to the agent's app
    pub cwd: String,  // agent working dir; its folder segments target the right window
}

impl Session {
    pub fn parse(key: &str, json: &str, file_ts: i64) -> Option<Session> {
        if json.is_empty() {
            return None;
        }
        let agent = non_empty(json_str(json, "agent")).unwrap_or_else(|| key.to_string());
        let title = non_empty(json_str(json, "title")).unwrap_or_else(|| agent.clone());
        let state = Status::from_status_str(&json_str(json, "status").unwrap_or_default());
        let sess = json_str(json, "session").unwrap_or_default();
        let tag = short_tag(if sess.is_empty() { key } else { &sess });
        let ts = json_long(json, "ts");
        let updated = if ts > 0 { ts } else { file_ts };
        Some(Session {
            agent,
            key: key.to_string(),
            title,
            tag,
            state,
            updated,
            hwnd: json_long(json, "hwnd"),
            pid: json_long(json, "pid"),
            cwd: json_str(json, "cwd").unwrap_or_default(),
        })
    }
}

fn non_empty(o: Option<String>) -> Option<String> {
    o.filter(|s| !s.is_empty())
}

// Last 4 alphanumeric chars, uppercased; "----" if none.
pub fn short_tag(s: &str) -> String {
    let a: String = s
        .chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if a.is_empty() {
        return "----".to_string();
    }
    let chars: Vec<char> = a.chars().collect();
    if chars.len() <= 4 {
        a
    } else {
        chars[chars.len() - 4..].iter().collect()
    }
}

// ---- state store: ingest snapshots -> prune + transition cues + aggregate ----

#[derive(Clone, Debug)]
pub struct Fired {
    pub session: Session,
    pub waiting: bool,
}

#[derive(Debug)]
pub struct SyncResult {
    pub cues: Vec<Fired>,
    pub expired: Vec<String>,
}

pub struct StateStore {
    pub done_ttl: i64,       // seconds
    pub processing_ttl: i64, // seconds
    pub waiting_ttl: i64,    // seconds
    sessions: HashMap<String, Session>,
    last_seen: HashMap<String, Status>,
}

impl StateStore {
    pub fn new() -> StateStore {
        StateStore {
            // `done` persists until SessionEnd removes the session (you close the
            // terminal); this TTL is only a safety net for abrupt closes where the
            // SessionEnd hook never fires.
            done_ttl: 30 * 60,
            processing_ttl: 45 * 60,
            waiting_ttl: 3 * 60 * 60,
            sessions: HashMap::new(),
            last_seen: HashMap::new(),
        }
    }

    // `current` = all sessions parsed from disk right now.
    pub fn sync(&mut self, current: Vec<Session>, now: i64) -> SyncResult {
        let mut res = SyncResult {
            cues: Vec::new(),
            expired: Vec::new(),
        };

        // 1. prune expired
        let mut keep: HashMap<String, Session> = HashMap::new();
        for s in current.into_iter() {
            let age = now - s.updated;
            let expired = match s.state {
                Status::Done => age > self.done_ttl,
                Status::Processing => age > self.processing_ttl,
                Status::Waiting => age > self.waiting_ttl,
                Status::Idle => false,
            };
            if expired {
                res.expired.push(s.key.clone());
            } else {
                keep.insert(s.key.clone(), s);
            }
        }
        self.sessions = keep;

        // 2. transition detection (fire only on entering waiting / done)
        for (key, sess) in self.sessions.iter() {
            let now_state = sess.state;
            if self.last_seen.get(key).copied() != Some(now_state) {
                if now_state == Status::Waiting {
                    res.cues.push(Fired {
                        session: sess.clone(),
                        waiting: true,
                    });
                } else if now_state == Status::Done {
                    res.cues.push(Fired {
                        session: sess.clone(),
                        waiting: false,
                    });
                }
            }
        }

        // 3. refresh last_seen (only sessions that currently exist)
        let mut new_seen = HashMap::new();
        for (key, sess) in self.sessions.iter() {
            new_seen.insert(key.clone(), sess.state);
        }
        self.last_seen = new_seen;

        res
    }

    pub fn aggregate(&self) -> Status {
        let mut agg = Status::Idle;
        for s in self.sessions.values() {
            if s.state > agg {
                agg = s.state;
            }
        }
        agg
    }

    pub fn count(&self) -> usize {
        self.sessions.len()
    }

    // (waiting, processing, done) counts — the UI formats these per language.
    pub fn counts(&self) -> (i32, i32, i32) {
        let (mut w, mut p, mut d) = (0, 0, 0);
        for s in self.sessions.values() {
            match s.state {
                Status::Waiting => w += 1,
                Status::Processing => p += 1,
                Status::Done => d += 1,
                _ => {}
            }
        }
        (w, p, d)
    }

    pub fn sessions(&self) -> Vec<Session> {
        self.sessions.values().cloned().collect()
    }
}

// ---- emit-side inference ----

// codex etc. --status auto: infer from event text.
pub fn infer_auto(blob: &str) -> &'static str {
    let low = blob.to_lowercase();
    if low.contains("turn-ended")
        || low.contains("turn-complete")
        || low.contains("agent-turn-complete")
        || low.contains("complete")
        || low.contains("finished")
    {
        return "done";
    }
    if low.contains("approval")
        || low.contains("permission")
        || low.contains("confirm")
        || low.contains("input")
    {
        return "waiting";
    }
    "processing"
}

// Claude Notification: distinguish permission/confirm (waiting) from idle.
// The idle "waiting for your input" notification fires ~60s after Stop, which
// already reported done -> return "ignore" so we don't re-fire the completion cue.
pub fn infer_notification(stdin_json: &str) -> &'static str {
    let low = json_str(stdin_json, "message").unwrap_or_default().to_lowercase();
    if !low.is_empty() {
        if low.contains("waiting for your input") || low.contains("idle") {
            return "ignore";
        }
        if low.contains("permission")
            || low.contains("approve")
            || low.contains("confirm")
            || low.contains("needs your")
        {
            return "waiting";
        }
    }
    "waiting"
}

// The standalone GUI process that hosts an agent's window, when the agent runs as
// its own single-window desktop app rather than inside a terminal/IDE. Returned as a
// lowercase exe name (matched against window owners at click time), or "" if none.
//
// Codex needs this: its emit process tree is already broken when the hook fires (the
// parent has exited), so window ancestry can't be captured, and its window title is
// the static string "Codex", so cwd-folder matching can't find it either. The one
// reliable signal is the owning process name. Claude deliberately has no host here:
// "Claude Code" runs inside Code.exe / a terminal, not the separate "Claude" desktop
// app, so a process-name match would focus the wrong window — it uses ancestry+cwd.
pub fn host_process(agent: &str) -> &'static str {
    match agent.trim().to_lowercase().as_str() {
        "codex" => "codex.exe",
        _ => "",
    }
}

// Choose which window to focus for a session. `windows` is (hwnd, title, pid, proc).
//
// 1. If the agent has a standalone GUI host (`host_proc`, e.g. Codex), find its
//    window by owning process name — the strongest signal for apps whose ancestry is
//    broken and whose title is static.
// 2. Otherwise scope candidates to the agent's host process (`target_pid`) when
//    known, so a browser or other app whose title merely mentions the project is
//    never focused. Within those, try each candidate folder name (deepest first: the
//    cwd folder, then its parents) against the titles; the deepest match is most
//    specific. Falls back to the captured host window itself, then none.
pub fn select_window(
    target_hwnd: i64,
    target_pid: i64,
    names: &[&str],
    host_proc: &str,
    windows: &[(i64, String, i64, String)],
) -> Option<i64> {
    let hp = host_proc.trim().to_lowercase();
    if !hp.is_empty() {
        let matches: Vec<i64> = windows
            .iter()
            .filter(|(_, _, _, pn)| pn.to_lowercase() == hp)
            .map(|(h, _, _, _)| *h)
            .collect();
        if matches.len() == 1 {
            return Some(matches[0]);
        }
        if matches.len() > 1 {
            // ambiguous (multiple windows of the host app): prefer the captured handle
            if target_hwnd != 0 && matches.contains(&target_hwnd) {
                return Some(target_hwnd);
            }
            return Some(matches[0]);
        }
        // 0 windows owned by the host process -> fall through to cwd matching
    }

    let host: Vec<&(i64, String, i64, String)> = if target_pid != 0 {
        windows.iter().filter(|(_, _, p, _)| *p == target_pid).collect()
    } else {
        windows.iter().collect()
    };
    for name in names {
        let needle = name.trim().to_lowercase();
        if needle.is_empty() {
            continue;
        }
        let matches: Vec<i64> = host
            .iter()
            .filter(|(_, t, _, _)| t.to_lowercase().contains(&needle))
            .map(|(h, _, _, _)| *h)
            .collect();
        if matches.len() == 1 {
            return Some(matches[0]);
        }
        if matches.len() > 1 {
            // ambiguous: prefer the captured handle if it's among the matches, else first
            if target_hwnd != 0 && matches.contains(&target_hwnd) {
                return Some(target_hwnd);
            }
            return Some(matches[0]);
        }
        // 0 matches for this name -> try the next (shallower) name
    }
    if target_hwnd != 0 && windows.iter().any(|(h, _, _, _)| *h == target_hwnd) {
        return Some(target_hwnd);
    }
    None
}

// Candidate folder names to match against window titles, deepest first: the cwd's
// folder segments (the workspace folder is always one of them), capped to `max`.
// Falls back to `fallback` (the display title) when cwd is empty.
pub fn cwd_names(cwd: &str, fallback: &str, max: usize) -> Vec<String> {
    let mut names: Vec<String> = cwd
        .split(|c| c == '\\' || c == '/')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !(s.len() == 2 && s.ends_with(':'))) // skip "E:"
        .map(|s| s.to_string())
        .rev() // deepest first
        .collect();
    names.truncate(max);
    if names.is_empty() && !fallback.trim().is_empty() {
        names.push(fallback.to_string());
    }
    names
}

// =====================================================================
//  Tests — mirror tests/Tests.cs (the C# 38-assertion suite).
// =====================================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn mk(agent: &str, session: &str, status: &str, title: &str, ts: i64) -> String {
        format!(
            "{{\"agent\":\"{}\",\"session\":\"{}\",\"status\":\"{}\",\"title\":\"{}\",\"ts\":{}}}",
            agent, session, status, title, ts
        )
    }

    #[test]
    fn json_helpers() {
        assert_eq!(json_str("{\"session_id\":\"abc123\"}", "session_id").as_deref(), Some("abc123"));
        assert_eq!(json_str("{\"cwd\":\"E:\\\\AI\\\\X\"}", "cwd").as_deref(), Some("E:\\AI\\X"));
        assert_eq!(json_long("{\"ts\":1780000000}", "ts"), 1780000000);
        assert_eq!(json_str("{}", "missing"), None);
        // quoted-needle must not match a prefix field
        assert_eq!(json_str("{\"session_id\":\"x\"}", "session"), None);
        // UTF-8 title round-trips
        assert_eq!(json_str("{\"title\":\"数据清洗\"}", "title").as_deref(), Some("数据清洗"));
    }

    #[test]
    fn status_map() {
        assert_eq!(Status::from_status_str("waiting"), Status::Waiting);
        assert_eq!(Status::from_status_str("done"), Status::Done);
        assert_eq!(Status::from_status_str("processing"), Status::Processing);
        assert_eq!(status_norm("Approval"), "waiting");
        assert_eq!(status_norm("FINISHED"), "done");
        assert_eq!(status_norm("xyz"), "processing");
        assert!(Status::Waiting > Status::Processing);
        assert!(Status::Processing > Status::Done);
        assert!(Status::Done > Status::Idle);
    }

    #[test]
    fn session_parse() {
        let s1 = Session::parse("claude__abc", &mk("claude", "sess-9KZ", "waiting", "MyTools", 1780000000), 0).unwrap();
        assert_eq!(s1.agent, "claude");
        assert_eq!(s1.title, "MyTools");
        assert_eq!(s1.state, Status::Waiting);
        assert_eq!(s1.tag, "S9KZ"); // SESS9KZ -> last4
        assert_eq!(short_tag("conversation-XY12"), "XY12");
    }

    #[test]
    fn aggregate_priority() {
        let now = 1780000000;
        let mut st = StateStore::new();
        let snap = vec![
            Session::parse("claude__a", &mk("claude", "a", "processing", "P1", now), now).unwrap(),
            Session::parse("claude__b", &mk("claude", "b", "done", "P2", now), now).unwrap(),
        ];
        st.sync(snap, now);
        assert_eq!(st.aggregate(), Status::Processing);

        let snap2 = vec![
            Session::parse("claude__a", &mk("claude", "a", "processing", "P1", now), now).unwrap(),
            Session::parse("claude__b", &mk("claude", "b", "done", "P2", now), now).unwrap(),
            Session::parse("codex__c", &mk("codex", "c", "waiting", "P3", now), now).unwrap(),
        ];
        st.sync(snap2, now);
        assert_eq!(st.aggregate(), Status::Waiting);
        assert_eq!(st.count(), 3);
    }

    #[test]
    fn transitions_and_cues() {
        let now = 1780000000;
        let mut t = StateStore::new();

        let a = vec![Session::parse("c__1", &mk("claude", "1", "processing", "X", now), now).unwrap()];
        let r1 = t.sync(a, now);
        assert_eq!(r1.cues.len(), 0, "processing entry: no cue");

        let a = vec![Session::parse("c__1", &mk("claude", "1", "waiting", "X", now), now).unwrap()];
        let r2 = t.sync(a, now);
        assert_eq!(r2.cues.len(), 1);
        assert!(r2.cues[0].waiting, "->waiting: cue is Waiting");

        let a = vec![Session::parse("c__1", &mk("claude", "1", "waiting", "X", now), now).unwrap()];
        let r3 = t.sync(a, now);
        assert_eq!(r3.cues.len(), 0, "waiting steady: no repeat cue");

        let a = vec![Session::parse("c__1", &mk("claude", "1", "processing", "X", now), now).unwrap()];
        let r4 = t.sync(a, now);
        assert_eq!(r4.cues.len(), 0, "waiting->processing: no cue (resume blue)");
        assert_eq!(t.aggregate(), Status::Processing);

        let a = vec![Session::parse("c__1", &mk("claude", "1", "done", "X", now), now).unwrap()];
        let r5 = t.sync(a, now);
        assert_eq!(r5.cues.len(), 1);
        assert!(!r5.cues[0].waiting, "->done: cue is Done");
    }

    #[test]
    fn prune_ttl() {
        let now = 1780000000;
        let mut p = StateStore::new();
        p.done_ttl = 60;
        let old = now - 120;
        let ps = vec![
            Session::parse("c__old", &mk("claude", "old", "done", "X", old), old).unwrap(),
            Session::parse("c__new", &mk("claude", "new", "done", "X", now), now).unwrap(),
        ];
        let pr = p.sync(ps, now);
        assert!(pr.expired.contains(&"c__old".to_string()), "stale done expired");
        assert_eq!(p.count(), 1, "fresh done kept");
        assert_eq!(p.aggregate(), Status::Done);
    }

    #[test]
    fn infer_notification_cases() {
        assert_eq!(infer_notification("{\"message\":\"Claude is waiting for your input\"}"), "ignore");
        assert_eq!(infer_notification("{\"message\":\"Claude needs your permission to use Bash\"}"), "waiting");
        assert_eq!(infer_notification("{\"message\":\"\"}"), "waiting");
    }

    #[test]
    fn infer_auto_cases() {
        assert_eq!(infer_auto("turn-ended"), "done");
        assert_eq!(infer_auto("{\"type\":\"agent-turn-complete\"}"), "done");
        assert_eq!(infer_auto("permission needed"), "waiting");
    }

    // windows tuple: (hwnd, title, pid, proc)
    fn w(hwnd: i64, title: &str, pid: i64, proc: &str) -> (i64, String, i64, String) {
        (hwnd, title.to_string(), pid, proc.to_string())
    }

    #[test]
    fn select_title_wins_over_handle() {
        let wins = vec![
            w(111, "MyProj - file.rs - Visual Studio Code", 1, "Code.exe"),
            w(222, "Other - main.rs - Visual Studio Code", 1, "Code.exe"),
        ];
        assert_eq!(select_window(222, 0, &["MyProj"], "", &wins), Some(111));
    }

    #[test]
    fn select_handle_fallback_when_no_title_match() {
        let wins = vec![w(111, "Alpha", 1, "Code.exe"), w(222, "Beta", 1, "Code.exe")];
        assert_eq!(select_window(222, 0, &["MyProj"], "", &wins), Some(222));
        assert_eq!(select_window(0, 0, &["MyProj"], "", &wins), None);
    }

    #[test]
    fn select_multi_title_match_prefers_handle() {
        let wins = vec![
            w(111, "Proj one - Visual Studio Code", 1, "Code.exe"),
            w(222, "Proj two - Visual Studio Code", 1, "Code.exe"),
        ];
        assert_eq!(select_window(222, 0, &["Proj"], "", &wins), Some(222));
        assert_eq!(select_window(0, 0, &["Proj"], "", &wins), Some(111));
    }

    #[test]
    fn select_none_when_no_match() {
        let wins = vec![w(111, "Something Else", 1, "Code.exe")];
        assert_eq!(select_window(0, 0, &["MyProj"], "", &wins), None);
        assert_eq!(select_window(0, 0, &[""], "", &wins), None);
        assert_eq!(select_window(0, 0, &["MyProj"], "", &[]), None);
    }

    #[test]
    fn select_walks_up_to_parent_folder() {
        let wins = vec![w(111, "file.rs - MyTools - Visual Studio Code", 1, "Code.exe")];
        assert_eq!(select_window(0, 0, &["agent-knocks", "MyTools"], "", &wins), Some(111));
    }

    #[test]
    fn select_deepest_name_wins() {
        let wins = vec![
            w(111, "agent-knocks - Visual Studio Code", 1, "Code.exe"),
            w(222, "MyTools - Visual Studio Code", 1, "Code.exe"),
        ];
        assert_eq!(select_window(0, 0, &["agent-knocks", "MyTools"], "", &wins), Some(111));
    }

    #[test]
    fn select_scopes_to_host_process() {
        // VSCode (host pid 5000) shows the workspace "MyTools"; a browser tab (pid
        // 9000) on the GitHub repo has "agent-knocks" in its title.
        let wins = vec![
            w(111, "MyTools - main.rs - Visual Studio Code", 5000, "Code.exe"),
            w(222, "mazjq/agent-knocks - Google Chrome", 9000, "chrome.exe"),
        ];
        // scoped to the host process -> the browser is ignored; "agent-knocks" finds
        // nothing in-process, so it falls to "MyTools" -> the VSCode window.
        assert_eq!(select_window(0, 5000, &["agent-knocks", "MyTools"], "", &wins), Some(111));
        // without a host pid, the loose match would hit the browser (documented fallback)
        assert_eq!(select_window(0, 0, &["agent-knocks", "MyTools"], "", &wins), Some(222));
    }

    #[test]
    fn host_process_mapping() {
        assert_eq!(host_process("codex"), "codex.exe");
        assert_eq!(host_process("Codex"), "codex.exe");
        assert_eq!(host_process("claude"), ""); // Claude Code lives in Code.exe, not the Claude app
        assert_eq!(host_process("pi"), "");
    }

    #[test]
    fn select_codex_by_host_process() {
        // Codex's window title is the static "Codex" and its cwd folder ("defender")
        // appears nowhere; its emit captured a bogus pid/hwnd (the foreground VSCode).
        // The host-process match must still find the real Codex window.
        let wins = vec![
            w(111, "defender - main.rs - Visual Studio Code", 5000, "Code.exe"),
            w(222, "Codex", 7000, "Codex.exe"),
        ];
        // even with a wrong captured hwnd/pid pointing at VSCode, host_proc wins
        assert_eq!(select_window(111, 5000, &["defender", "Codex-proj"], "codex.exe", &wins), Some(222));
        // with nothing captured, still resolves
        assert_eq!(select_window(0, 0, &["defender"], "codex.exe", &wins), Some(222));
    }

    #[test]
    fn select_host_process_falls_through_when_absent() {
        // No Codex window present (e.g. Codex run as a CLI) -> fall back to cwd match.
        let wins = vec![w(111, "defender - Visual Studio Code", 5000, "Code.exe")];
        assert_eq!(select_window(0, 0, &["defender"], "codex.exe", &wins), Some(111));
        assert_eq!(select_window(0, 0, &["nomatch"], "codex.exe", &wins), None);
    }

    #[test]
    fn select_host_process_multi_prefers_handle() {
        let wins = vec![
            w(111, "Codex", 7000, "Codex.exe"),
            w(222, "Codex", 7001, "Codex.exe"),
        ];
        assert_eq!(select_window(222, 0, &[""], "codex.exe", &wins), Some(222));
        assert_eq!(select_window(0, 0, &[""], "codex.exe", &wins), Some(111));
    }

    #[test]
    fn cwd_names_deepest_first() {
        assert_eq!(
            cwd_names("C:\\dev\\acme\\tools\\agent-knocks", "x", 3),
            vec!["agent-knocks", "tools", "acme"]
        );
        assert_eq!(cwd_names("", "proj", 3), vec!["proj"]); // empty cwd -> fallback
        assert_eq!(cwd_names("/a/b/c/d", "x", 2), vec!["d", "c"]); // cap respected
    }

    #[test]
    fn counts_breakdown() {
        let now = 1780000000;
        let mut st = StateStore::new();
        st.sync(
            vec![
                Session::parse("c__1", &mk("claude", "1", "waiting", "X", now), now).unwrap(),
                Session::parse("c__2", &mk("claude", "2", "processing", "X", now), now).unwrap(),
                Session::parse("c__3", &mk("claude", "3", "processing", "X", now), now).unwrap(),
                Session::parse("c__4", &mk("claude", "4", "done", "X", now), now).unwrap(),
            ],
            now,
        );
        assert_eq!(st.counts(), (1, 2, 1));
    }
}
