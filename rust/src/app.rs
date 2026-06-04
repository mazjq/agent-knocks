// Agent Knocks - runtime engine (platform-agnostic): watch the state dir,
// aggregate sessions, prune, emit transition cues, write status.json.
// The tray-icon visual layer (next slice) sits on top of this.
#![allow(dead_code)]

use crate::core::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// Per-platform data root. AGENTKNOCKS_ROOT overrides (used for tests / isolated runs).
// Windows: %LOCALAPPDATA%\AgentKnocks.
pub fn data_root() -> PathBuf {
    if let Ok(o) = std::env::var("AGENTKNOCKS_ROOT") {
        if !o.is_empty() {
            return PathBuf::from(o);
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(local).join("AgentKnocks");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local/share/AgentKnocks");
    }
    PathBuf::from("AgentKnocks")
}

pub struct App {
    pub root: PathBuf,
    pub state_dir: PathBuf,
    store: StateStore,
    last_heartbeat: String,
}

impl App {
    pub fn new(root: PathBuf) -> App {
        let state_dir = root.join("state");
        let _ = std::fs::create_dir_all(&state_dir);
        App {
            root,
            state_dir,
            store: StateStore::new(),
            last_heartbeat: String::new(),
        }
    }

    // Read all state files -> parse -> sync -> delete expired -> write heartbeat.
    // Returns the new aggregate and any transition cues to play.
    pub fn reload(&mut self) -> (Status, Vec<Fired>) {
        let mut snap = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.state_dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let stem = match p.file_stem().and_then(|s| s.to_str()) {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                let txt = match std::fs::read_to_string(&p) {
                    Ok(t) => t,
                    Err(_) => continue, // mid-rename etc. — skip this pass
                };
                if let Some(s) = Session::parse(&stem, &txt, file_mtime_unix(&p)) {
                    snap.push(s);
                }
            }
        }
        let now = now_unix();
        let res = self.store.sync(snap, now);
        for key in &res.expired {
            let _ = std::fs::remove_file(self.state_dir.join(format!("{}.json", key)));
        }
        let agg = self.store.aggregate();
        self.write_heartbeat(agg, self.store.count());
        (agg, res.cues)
    }

    // Write status.json on aggregate/count change (external consumers + latency probe).
    fn write_heartbeat(&mut self, agg: Status, n: usize) {
        let key = format!("{:?}/{}", agg, n);
        if key == self.last_heartbeat {
            return;
        }
        self.last_heartbeat = key;
        let content = format!(
            "{{\"agg\":\"{}\",\"sessions\":{},\"ts\":{}}}",
            agg_str(agg),
            n,
            now_unix()
        );
        let _ = std::fs::write(self.root.join("status.json"), content);
    }

    pub fn sessions(&self) -> Vec<Session> {
        self.store.sessions()
    }
    pub fn counts(&self) -> (i32, i32, i32) {
        self.store.counts()
    }
    pub fn count(&self) -> usize {
        self.store.count()
    }
}

pub fn agg_str(s: Status) -> &'static str {
    match s {
        Status::Waiting => "waiting",
        Status::Processing => "processing",
        Status::Done => "done",
        Status::Idle => "idle",
    }
}

fn file_mtime_unix(p: &Path) -> i64 {
    std::fs::metadata(p)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or_else(now_unix)
}

// One reload pass then exit (testing / external poll).
pub fn run_once(root: PathBuf) {
    let mut app = App::new(root);
    let (agg, _) = app.reload();
    let (w, p, d) = app.counts();
    println!(
        "agg={} sessions={} (waiting={} working={} done={})",
        agg_str(agg),
        app.count(),
        w,
        p,
        d
    );
}

// Headless watch loop: notify the state dir + periodic prune; print cues.
// (The tray-icon visual will replace the println sink with color + sound.)
pub fn run_headless(root: PathBuf, max_secs: Option<u64>) {
    use notify::{recommended_watcher, RecursiveMode, Watcher};

    let mut app = App::new(root);
    let (agg, _) = app.reload();
    println!(
        "[agentknocks] watching {} - agg={}",
        app.state_dir.display(),
        agg_str(agg)
    );

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = match recommended_watcher(move |res| {
        let _ = tx.send(res);
    }) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("watch error: {e}");
            return;
        }
    };
    if let Err(e) = watcher.watch(&app.state_dir, RecursiveMode::NonRecursive) {
        eprintln!("watch error: {e}");
        return;
    }

    let start = Instant::now();
    loop {
        // periodic every <=2s; FS events wake sooner
        let got = rx.recv_timeout(Duration::from_secs(2)).is_ok();
        if got {
            std::thread::sleep(Duration::from_millis(120)); // debounce burst
            while rx.try_recv().is_ok() {}
        }
        let (_agg, cues) = app.reload();
        for c in &cues {
            println!(
                "[cue] {} - {} #{}",
                if c.waiting { "WAITING" } else { "DONE" },
                c.session.title,
                c.session.tag
            );
        }
        if let Some(limit) = max_secs {
            if start.elapsed().as_secs() >= limit {
                println!("[agentknocks] headless run done ({}s)", limit);
                return;
            }
        }
    }
}
