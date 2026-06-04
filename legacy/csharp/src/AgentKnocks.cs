// Agent Knocks - lightweight tray status indicator for AI coding agents (UI + emit entry).
// Pure state logic lives in Core.cs. Two modes:
//   1. default:  resident tray, FileSystemWatcher on the state dir, aggregate + recolor + sound + balloon
//   2. --emit:   invoked by an agent hook; reads stdin(JSON)+args, writes/removes a state file, exits
// C# 5 syntax only (in-box csc.exe). Build with /codepage:65001 so non-ASCII string literals are correct.
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32;

namespace AgentKnocks
{
    static class App
    {
        public const string Name = "Agent Knocks";   // display name
        public const string Id = "AgentKnocks";        // install dir / registry / mutex key
    }

    // ======================================================================
    //  I18n: all user-facing strings. Default English; 中文 optional via tray menu.
    // ======================================================================
    static class I18n
    {
        public static string Cur = "en";                    // "en" | "zh"
        static bool Zh { get { return Cur == "zh"; } }
        public static bool IsValid(string c) { return c == "en" || c == "zh"; }

        public static string Status(AgentKnocks.Status s)
        {
            switch (s)
            {
                case AgentKnocks.Status.Waiting:    return Zh ? "等待确认" : "Waiting";
                case AgentKnocks.Status.Processing: return Zh ? "处理中"   : "Working";
                case AgentKnocks.Status.Done:       return Zh ? "已完成"   : "Done";
                default:                                 return Zh ? "空闲"     : "Idle";
            }
        }

        public static string CountSuffix(StateStore st)
        {
            int w, p, d; st.Counts(out w, out p, out d);
            if (w + p + d == 0) return "";
            List<string> parts = new List<string>();
            if (w > 0) parts.Add(w + (Zh ? " 等待"   : " waiting"));
            if (p > 0) parts.Add(p + (Zh ? " 处理中" : " working"));
            if (d > 0) parts.Add(d + (Zh ? " 完成"   : " done"));
            return parts.Count > 0 ? "  (" + string.Join(", ", parts.ToArray()) + ")" : "";
        }

        public static string Mute(bool muted) { return muted ? (Zh ? "🔔 取消静音" : "🔔 Unmute") : (Zh ? "🔇 静音" : "🔇 Mute"); }
        public static string TestSound  { get { return Zh ? "🔊 测试声音"      : "🔊 Test sound"; } }
        public static string SoundWait  { get { return Zh ? "等待确认音"        : "Waiting sound"; } }
        public static string SoundDone  { get { return Zh ? "完成音"            : "Done sound"; } }
        public static string OpenDir    { get { return Zh ? "📁 打开状态目录"   : "📁 Open state folder"; } }
        public static string AutoStart  { get { return Zh ? "⏻ 开机自启"        : "⏻ Start at login"; } }
        public static string Language   { get { return Zh ? "🌐 语言"           : "🌐 Language"; } }
        public static string Quit       { get { return Zh ? "❌ 退出"           : "❌ Quit"; } }
        public static string NoSessions { get { return Zh ? "（无活动会话）"     : "(no active sessions)"; } }
        public static string HeadWait   { get { return Zh ? "需要你确认"         : "Needs your confirmation"; } }
        public static string HeadDone   { get { return Zh ? "处理完成"           : "Done"; } }
    }

    static class Paths
    {
        public static string Root
        {
            get
            {
                return Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                    App.Id);
            }
        }
        public static string StateDir { get { return Path.Combine(Root, "state"); } }
        public static string ConfigFile { get { return Path.Combine(Root, "config.json"); } }
        public static string LogFile { get { return Path.Combine(Root, "events.log"); } }
        public static void EnsureDirs() { Directory.CreateDirectory(StateDir); }
    }

    static class Program
    {
        [STAThread]
        static int Main(string[] args)
        {
            for (int i = 0; i < args.Length; i++)
                if (args[i] == "--emit") return EmitMode.Run(args);

            bool createdNew;
            using (Mutex mtx = new Mutex(true, App.Id + "_Tray_Singleton_2026", out createdNew))
            {
                if (!createdNew) return 0;
                Application.EnableVisualStyles();
                Application.SetCompatibleTextRenderingDefault(false);
                Application.Run(new TrayContext());
            }
            return 0;
        }
    }

    // ======================================================================
    //  EMIT mode
    // ======================================================================
    static class EmitMode
    {
        public static int Run(string[] args)
        {
            try
            {
                string agent = ArgVal(args, "--agent", "agent");
                string status = ArgVal(args, "--status", "processing");
                string titleArg = ArgVal(args, "--title", null);
                string keyArg = ArgVal(args, "--key", null);
                string stdin = ReadStdin();

                // resolve session id / working dir
                string session = keyArg;
                if (string.IsNullOrEmpty(session)) session = J.Str(stdin, "session_id");
                if (string.IsNullOrEmpty(session)) session = J.Str(stdin, "session");

                string cwd = J.Str(stdin, "cwd");
                if (string.IsNullOrEmpty(cwd)) cwd = J.Str(stdin, "workdir");

                string title = titleArg;
                if (string.IsNullOrEmpty(title) && !string.IsNullOrEmpty(cwd)) title = LastSegment(cwd);
                bool titleResolved = !string.IsNullOrEmpty(title);

                // status inference
                string blob = stdin;
                for (int i = 0; i < args.Length; i++) blob += " " + args[i];
                if (status == "auto") status = Infer.Auto(blob);
                else if (status == "notify") status = Infer.Notification(stdin);

                if (string.IsNullOrEmpty(session)) session = agent + "-default";
                string key = Sanitize(agent) + "__" + Sanitize(session);

                Paths.EnsureDirs();
                string file = Path.Combine(Paths.StateDir, key + ".json");

                Log(agent, status, key, stdin);

                // no-op: idle reminders etc. that should not change state; log then exit (don't touch the file)
                if (status == "ignore") return 0;

                if (status == "end" || status == "exit")
                {
                    try { if (File.Exists(file)) File.Delete(file); } catch { }
                    return 0;
                }

                // when no real title parsed, keep the previous one so the project name isn't overwritten
                if (!titleResolved && File.Exists(file))
                {
                    try
                    {
                        string prev = J.Str(File.ReadAllText(file), "title");
                        if (!string.IsNullOrEmpty(prev)) title = prev;
                    }
                    catch { }
                }
                if (string.IsNullOrEmpty(title)) title = agent;

                string norm = StatusMap.Norm(status);
                long ts = Time.ToUnix(DateTime.UtcNow);
                string json = "{\"agent\":\"" + J.Esc(agent) + "\",\"session\":\"" + J.Esc(session) +
                              "\",\"status\":\"" + norm + "\",\"title\":\"" + J.Esc(title) +
                              "\",\"ts\":" + ts.ToString(CultureInfo.InvariantCulture) + "}";

                string tmp = file + ".tmp";
                File.WriteAllText(tmp, json, new UTF8Encoding(false));
                if (File.Exists(file)) File.Delete(file);
                File.Move(tmp, file);
                return 0;
            }
            catch { return 0; } // emit never disrupts the agent
        }

        // event log (diagnostics), auto-reset past 200KB
        static void Log(string agent, string status, string key, string stdin)
        {
            try
            {
                Paths.EnsureDirs();
                try
                {
                    FileInfo fi = new FileInfo(Paths.LogFile);
                    if (fi.Exists && fi.Length > 200 * 1024) File.Delete(Paths.LogFile);
                }
                catch { }
                string msg = J.Str(stdin, "message");
                string hook = J.Str(stdin, "hook_event_name");
                string line = DateTime.Now.ToString("HH:mm:ss.fff") + "  " + agent +
                              "  status=" + status + "  key=" + key +
                              (hook != null ? "  hook=" + hook : "") +
                              (msg != null ? "  msg=" + msg.Replace("\n", " ") : "") + Environment.NewLine;
                File.AppendAllText(Paths.LogFile, line, new UTF8Encoding(false));
            }
            catch { }
        }

        static string ArgVal(string[] args, string name, string def)
        {
            for (int i = 0; i < args.Length - 1; i++)
                if (args[i] == name) return args[i + 1];
            return def;
        }

        static string ReadStdin()
        {
            try
            {
                Stream s = Console.OpenStandardInput();
                if (s == null) return "";
                using (StreamReader r = new StreamReader(s, Encoding.UTF8))
                    return r.ReadToEnd();
            }
            catch { return ""; }
        }

        static string LastSegment(string p)
        {
            if (string.IsNullOrEmpty(p)) return p;
            p = p.Replace('/', '\\').TrimEnd('\\');
            int idx = p.LastIndexOf('\\');
            return idx >= 0 ? p.Substring(idx + 1) : p;
        }

        static string Sanitize(string s)
        {
            if (string.IsNullOrEmpty(s)) return "x";
            StringBuilder sb = new StringBuilder();
            foreach (char c in s)
                sb.Append(char.IsLetterOrDigit(c) || c == '-' || c == '_' ? c : '-');
            return sb.ToString();
        }
    }

    // ======================================================================
    //  Tray mode
    // ======================================================================
    class TrayContext : ApplicationContext
    {
        [DllImport("user32.dll", CharSet = CharSet.Auto)]
        static extern bool DestroyIcon(IntPtr handle);

        readonly NotifyIcon tray;
        readonly ContextMenuStrip menu;
        readonly FileSystemWatcher watcher;
        readonly System.Windows.Forms.Timer periodic;  // periodic refresh (elapsed display / TTL prune)
        readonly System.Windows.Forms.Timer debounce;   // quick response after a file change
        readonly Form pump;                              // hidden window: marshals watcher events to the UI thread
        readonly SoundEngine sound;
        readonly StateStore store = new StateStore();
        readonly Icon[] icons = new Icon[4];

        bool muted = false;
        Status currentAgg = Status.Idle;

        public TrayContext()
        {
            Paths.EnsureDirs();
            LoadConfig();
            sound = new SoundEngine();

            for (int i = 0; i < 4; i++) icons[i] = MakeDotIcon(ColorFor((Status)i));

            // hidden window, only to marshal background watcher events onto the UI thread (not shown, no flicker)
            pump = new Form();
            pump.ShowInTaskbar = false;
            pump.FormBorderStyle = FormBorderStyle.None;
            pump.StartPosition = FormStartPosition.Manual;
            pump.Location = new Point(-32000, -32000);
            pump.Size = new Size(1, 1);
            { IntPtr force = pump.Handle; } // force handle creation

            menu = new ContextMenuStrip();
            tray = new NotifyIcon();
            tray.Icon = icons[(int)Status.Idle];
            tray.Text = App.Name + " — " + I18n.Status(Status.Idle);
            tray.Visible = true;
            tray.ContextMenuStrip = menu;
            tray.DoubleClick += delegate { OpenStateDir(); };

            watcher = new FileSystemWatcher(Paths.StateDir, "*.json");
            watcher.NotifyFilter = NotifyFilters.LastWrite | NotifyFilters.FileName | NotifyFilters.CreationTime;
            FileSystemEventHandler onChange = delegate { KickDebounce(); };
            watcher.Changed += onChange;
            watcher.Created += onChange;
            watcher.Deleted += onChange;
            watcher.Renamed += delegate { KickDebounce(); };
            watcher.SynchronizingObject = pump; // raise events on the UI thread -> debounce timer works
            watcher.EnableRaisingEvents = true;

            // file change -> refresh after 120ms (coalesce bursts, near-instant)
            debounce = new System.Windows.Forms.Timer();
            debounce.Interval = 120;
            debounce.Tick += delegate { debounce.Stop(); Reload(); };

            // periodic refresh: update elapsed display + TTL prune
            periodic = new System.Windows.Forms.Timer();
            periodic.Interval = 2000;
            periodic.Tick += delegate { Reload(); };
            periodic.Start();

            Reload();
        }

        void KickDebounce()
        {
            // watcher.SynchronizingObject = pump ensures this callback is already on the UI thread
            debounce.Stop();
            debounce.Start();
        }

        void Reload()
        {
            string[] files;
            try { files = Directory.GetFiles(Paths.StateDir, "*.json"); }
            catch { files = new string[0]; }

            List<Session> snap = new List<Session>();
            foreach (string f in files)
            {
                try
                {
                    string txt = ReadShared(f);
                    Session s = Session.Parse(Path.GetFileNameWithoutExtension(f), txt, File.GetLastWriteTime(f));
                    if (s != null) snap.Add(s);
                }
                catch { }
            }

            SyncResult res = store.Sync(snap, DateTime.Now);

            foreach (string key in res.Expired)
                try { File.Delete(Path.Combine(Paths.StateDir, key + ".json")); } catch { }

            foreach (Fired c in res.Cues)
                Notify(c.S, c.Waiting);

            currentAgg = store.Aggregate();
            tray.Icon = icons[(int)currentAgg];
            tray.Text = Truncate(App.Name + " — " + I18n.Status(currentAgg) + I18n.CountSuffix(store), 63);

            WriteHeartbeat(currentAgg, store.Count);
            BuildMenu();
        }

        string lastHeartbeat = null;
        // write the current aggregate status to status.json (for external consumers; only on status/count change)
        void WriteHeartbeat(Status agg, int n)
        {
            try
            {
                string key = agg + "/" + n;
                if (key == lastHeartbeat) return;
                lastHeartbeat = key;
                string content = "{\"agg\":\"" + agg.ToString().ToLowerInvariant() +
                                 "\",\"sessions\":" + n + ",\"ts\":" +
                                 Time.ToUnix(DateTime.UtcNow) + "}";
                File.WriteAllText(Path.Combine(Paths.Root, "status.json"), content, new UTF8Encoding(false));
            }
            catch { }
        }

        void Notify(Session s, bool waiting)
        {
            if (!muted) sound.Play(waiting ? Cue.Waiting : Cue.Done);
            try
            {
                string head = waiting ? I18n.HeadWait : I18n.HeadDone;
                tray.ShowBalloonTip(2500, s.Agent + " · " + head, Label(s), ToolTipIcon.None);
            }
            catch { }
        }

        static string Label(Session s) { return s.Title + " #" + s.Tag; }

        // ---- menu ----
        void BuildMenu()
        {
            menu.Items.Clear();

            ToolStripMenuItem header = new ToolStripMenuItem(I18n.Status(currentAgg) + I18n.CountSuffix(store));
            header.Enabled = false;
            menu.Items.Add(header);
            menu.Items.Add(new ToolStripSeparator());

            IList<Session> list = store.Sessions;
            if (list.Count == 0)
            {
                ToolStripMenuItem none = new ToolStripMenuItem(I18n.NoSessions);
                none.Enabled = false;
                menu.Items.Add(none);
            }
            else
            {
                List<Session> sorted = new List<Session>(list);
                sorted.Sort(delegate (Session a, Session b)
                {
                    int c = ((int)b.State).CompareTo((int)a.State);
                    return c != 0 ? c : b.Updated.CompareTo(a.Updated);
                });
                foreach (Session s in sorted)
                {
                    string line = StatusMap.Glyph(s.State) + " " + s.Agent + " · " +
                                  I18n.Status(s.State) + " · " + Elapsed(s.Updated) + "  [" + Label(s) + "]";
                    ToolStripMenuItem it = new ToolStripMenuItem(line);
                    it.Enabled = false;
                    menu.Items.Add(it);
                }
            }

            menu.Items.Add(new ToolStripSeparator());

            ToolStripMenuItem mute = new ToolStripMenuItem(I18n.Mute(muted));
            mute.Click += delegate { muted = !muted; SaveConfig(); BuildMenu(); };
            menu.Items.Add(mute);

            ToolStripMenuItem test = new ToolStripMenuItem(I18n.TestSound);
            ToolStripMenuItem tWait = new ToolStripMenuItem(I18n.SoundWait);
            tWait.Click += delegate { sound.Play(Cue.Waiting); };
            ToolStripMenuItem tDone = new ToolStripMenuItem(I18n.SoundDone);
            tDone.Click += delegate { sound.Play(Cue.Done); };
            test.DropDownItems.Add(tWait);
            test.DropDownItems.Add(tDone);
            menu.Items.Add(test);

            ToolStripMenuItem openDir = new ToolStripMenuItem(I18n.OpenDir);
            openDir.Click += delegate { OpenStateDir(); };
            menu.Items.Add(openDir);

            ToolStripMenuItem lang = new ToolStripMenuItem(I18n.Language);
            ToolStripMenuItem en = new ToolStripMenuItem("English");
            en.Checked = (I18n.Cur == "en");
            en.Click += delegate { SetLang("en"); };
            ToolStripMenuItem zh = new ToolStripMenuItem("中文");
            zh.Checked = (I18n.Cur == "zh");
            zh.Click += delegate { SetLang("zh"); };
            lang.DropDownItems.Add(en);
            lang.DropDownItems.Add(zh);
            menu.Items.Add(lang);

            ToolStripMenuItem auto = new ToolStripMenuItem(I18n.AutoStart);
            auto.Checked = IsAutoStart();
            auto.Click += delegate { SetAutoStart(!IsAutoStart()); BuildMenu(); };
            menu.Items.Add(auto);

            menu.Items.Add(new ToolStripSeparator());

            ToolStripMenuItem quit = new ToolStripMenuItem(I18n.Quit);
            quit.Click += delegate { ExitApp(); };
            menu.Items.Add(quit);
        }

        void SetLang(string code)
        {
            if (!I18n.IsValid(code) || I18n.Cur == code) { BuildMenu(); return; }
            I18n.Cur = code;
            SaveConfig();
            Reload();   // refresh tooltip + menu immediately
        }

        void ExitApp()
        {
            periodic.Stop();
            debounce.Stop();
            watcher.EnableRaisingEvents = false;
            tray.Visible = false;
            tray.Dispose();
            if (pump != null) pump.Dispose();
            ExitThread();
        }

        // ---- icon drawing ----
        static Color ColorFor(Status s)
        {
            switch (s)
            {
                case Status.Waiting: return Color.FromArgb(255, 165, 0);
                case Status.Processing: return Color.FromArgb(30, 144, 255);
                case Status.Done: return Color.FromArgb(50, 205, 50);
                default: return Color.FromArgb(150, 150, 150);
            }
        }

        Icon MakeDotIcon(Color c)
        {
            using (Bitmap bmp = new Bitmap(16, 16))
            {
                using (Graphics g = Graphics.FromImage(bmp))
                {
                    g.SmoothingMode = System.Drawing.Drawing2D.SmoothingMode.AntiAlias;
                    g.Clear(Color.Transparent);
                    using (SolidBrush b = new SolidBrush(c)) g.FillEllipse(b, 2, 2, 12, 12);
                    using (Pen p = new Pen(Color.FromArgb(60, 0, 0, 0))) g.DrawEllipse(p, 2, 2, 12, 12);
                }
                IntPtr h = bmp.GetHicon();
                Icon ico = (Icon)Icon.FromHandle(h).Clone();
                DestroyIcon(h);
                return ico;
            }
        }

        static string Elapsed(DateTime since)
        {
            TimeSpan d = DateTime.Now - since;
            if (d.TotalSeconds < 60) return ((int)d.TotalSeconds) + "s";
            if (d.TotalMinutes < 60) return ((int)d.TotalMinutes) + "m" + d.Seconds + "s";
            return ((int)d.TotalHours) + "h" + d.Minutes + "m";
        }

        static string Truncate(string s, int max)
        {
            if (s == null) return "";
            return s.Length <= max ? s : s.Substring(0, max);
        }

        void OpenStateDir()
        {
            try { System.Diagnostics.Process.Start("explorer.exe", "\"" + Paths.StateDir + "\""); }
            catch { }
        }

        // ---- autostart ----
        const string RunKey = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
        static string RunName { get { return App.Id; } }

        bool IsAutoStart()
        {
            try
            {
                using (RegistryKey k = Registry.CurrentUser.OpenSubKey(RunKey))
                    return k != null && k.GetValue(RunName) != null;
            }
            catch { return false; }
        }

        void SetAutoStart(bool on)
        {
            try
            {
                using (RegistryKey k = Registry.CurrentUser.OpenSubKey(RunKey, true))
                {
                    if (k == null) return;
                    if (on) k.SetValue(RunName, "\"" + Application.ExecutablePath + "\"");
                    else if (k.GetValue(RunName) != null) k.DeleteValue(RunName, false);
                }
            }
            catch { }
        }

        // ---- config ----
        void LoadConfig()
        {
            try
            {
                if (File.Exists(Paths.ConfigFile))
                {
                    string cfg = File.ReadAllText(Paths.ConfigFile);
                    muted = System.Text.RegularExpressions.Regex.IsMatch(cfg, "\"muted\"\\s*:\\s*true");
                    System.Text.RegularExpressions.Match m =
                        System.Text.RegularExpressions.Regex.Match(cfg, "\"lang\"\\s*:\\s*\"(\\w+)\"");
                    if (m.Success && I18n.IsValid(m.Groups[1].Value)) I18n.Cur = m.Groups[1].Value;
                }
            }
            catch { }
        }

        void SaveConfig()
        {
            try
            {
                Paths.EnsureDirs();
                File.WriteAllText(Paths.ConfigFile,
                    "{\"muted\":" + (muted ? "true" : "false") + ",\"lang\":\"" + I18n.Cur + "\"}",
                    new UTF8Encoding(false));
            }
            catch { }
        }

        static string ReadShared(string path)
        {
            using (FileStream fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite))
            using (StreamReader r = new StreamReader(fs, Encoding.UTF8))
                return r.ReadToEnd();
        }
    }

    // ======================================================================
    //  Sound engine: Console.Beep synthesizes intuitive earcons (zero audio files)
    //   Waiting: rising 660->990 (like asking "you there?", draws attention / unfinished)
    //   Done:    ascending triad 770->1046->1318 (resolved / positive, "all set")
    // ======================================================================
    class SoundEngine
    {
        readonly object gate = new object();
        Cue pending;
        bool has = false;
        readonly AutoResetEvent signal = new AutoResetEvent(false);

        public SoundEngine()
        {
            Thread t = new Thread(Loop);
            t.IsBackground = true;
            t.Start();
        }

        public void Play(Cue c)
        {
            lock (gate) { pending = c; has = true; }
            signal.Set();
        }

        void Loop()
        {
            while (true)
            {
                signal.WaitOne();
                Cue c;
                lock (gate) { if (!has) continue; c = pending; has = false; }
                try
                {
                    if (c == Cue.Waiting)
                    {
                        Console.Beep(660, 130);
                        Console.Beep(990, 170);
                    }
                    else
                    {
                        Console.Beep(770, 90);
                        Console.Beep(1046, 90);
                        Console.Beep(1318, 150);
                    }
                }
                catch { }
            }
        }
    }
}
