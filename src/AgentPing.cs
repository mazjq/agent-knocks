// AgentPing - 轻量级 agent 状态托盘提示器 (UI + emit 入口)
// 纯状态逻辑见 Core.cs。两种模式:
//   1. 默认:    常驻托盘, FileSystemWatcher 监听状态目录, 聚合 + 变色 + 声音 + 气泡
//   2. --emit:  被 agent hook 调用, 读 stdin(JSON)+参数, 写/删状态文件后立即退出
// 仅 C# 5 语法 (旧版 csc.exe); 用 /codepage:65001 编译以保证中文字面量正确。
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

namespace AgentPing
{
    static class Paths
    {
        public static string Root
        {
            get
            {
                return Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                    "AgentPing");
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
            using (Mutex mtx = new Mutex(true, "AgentPing_Tray_Singleton_2026", out createdNew))
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
    //  EMIT 模式
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

                // 解析会话 id / 工作目录
                string session = keyArg;
                if (string.IsNullOrEmpty(session)) session = J.Str(stdin, "session_id");
                if (string.IsNullOrEmpty(session)) session = J.Str(stdin, "session");

                string cwd = J.Str(stdin, "cwd");
                if (string.IsNullOrEmpty(cwd)) cwd = J.Str(stdin, "workdir");

                string title = titleArg;
                if (string.IsNullOrEmpty(title) && !string.IsNullOrEmpty(cwd)) title = LastSegment(cwd);
                bool titleResolved = !string.IsNullOrEmpty(title);

                // 状态推断
                string blob = stdin;
                for (int i = 0; i < args.Length; i++) blob += " " + args[i];
                if (status == "auto") status = Infer.Auto(blob);
                else if (status == "notify") status = Infer.Notification(stdin);

                if (string.IsNullOrEmpty(session)) session = agent + "-default";
                string key = Sanitize(agent) + "__" + Sanitize(session);

                Paths.EnsureDirs();
                string file = Path.Combine(Paths.StateDir, key + ".json");

                Log(agent, status, key, stdin);

                if (status == "end" || status == "exit")
                {
                    try { if (File.Exists(file)) File.Delete(file); } catch { }
                    return 0;
                }

                // 没解析到真实 title 时沿用旧值, 避免项目名被覆盖
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
            catch { return 0; } // emit 永不打断 agent
        }

        // 事件日志 (诊断用), 超过 200KB 自动重置
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
    //  托盘模式
    // ======================================================================
    class TrayContext : ApplicationContext
    {
        [DllImport("user32.dll", CharSet = CharSet.Auto)]
        static extern bool DestroyIcon(IntPtr handle);

        readonly NotifyIcon tray;
        readonly ContextMenuStrip menu;
        readonly FileSystemWatcher watcher;
        readonly System.Windows.Forms.Timer periodic;  // 周期刷新(耗时显示/过期淘汰)
        readonly System.Windows.Forms.Timer debounce;   // 文件变化后快速响应
        readonly Form pump;                              // 隐藏窗口: 给 watcher 提供 UI 线程 marshaling 目标
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

            // 隐藏窗口, 仅用于把后台线程的 watcher 事件 marshal 到 UI 线程 (不显示, 无闪烁)
            pump = new Form();
            pump.ShowInTaskbar = false;
            pump.FormBorderStyle = FormBorderStyle.None;
            pump.StartPosition = FormStartPosition.Manual;
            pump.Location = new Point(-32000, -32000);
            pump.Size = new Size(1, 1);
            { IntPtr force = pump.Handle; } // 强制创建句柄

            menu = new ContextMenuStrip();
            tray = new NotifyIcon();
            tray.Icon = icons[(int)Status.Idle];
            tray.Text = "AgentPing — 空闲";
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
            watcher.SynchronizingObject = pump; // 事件改在 UI 线程触发 -> 防抖定时器可正常工作
            watcher.EnableRaisingEvents = true;

            // 文件变化 -> 120ms 后刷新 (合并连发, 近乎即时)
            debounce = new System.Windows.Forms.Timer();
            debounce.Interval = 120;
            debounce.Tick += delegate { debounce.Stop(); Reload(); };

            // 周期刷新: 更新耗时显示 + 过期淘汰
            periodic = new System.Windows.Forms.Timer();
            periodic.Interval = 2000;
            periodic.Tick += delegate { Reload(); };
            periodic.Start();

            Reload();
        }

        void KickDebounce()
        {
            // watcher.SynchronizingObject = pump 保证本回调已在 UI 线程, 防抖定时器可安全启停
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
            tray.Text = Truncate("AgentPing — " + StatusMap.Label(currentAgg) + store.CountSuffix(), 63);

            WriteHeartbeat(currentAgg, store.Count);
            BuildMenu();
        }

        string lastHeartbeat = null;
        // 把当前聚合状态写到 status.json (供外部查询; 仅状态/数量变化时写)
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
                string head = waiting ? "需要你确认" : "处理完成";
                tray.ShowBalloonTip(2500, s.Agent + " · " + head, Label(s), ToolTipIcon.None);
            }
            catch { }
        }

        static string Label(Session s) { return s.Title + " #" + s.Tag; }

        // ---- 菜单 ----
        void BuildMenu()
        {
            menu.Items.Clear();

            ToolStripMenuItem header = new ToolStripMenuItem(StatusMap.Label(currentAgg) + store.CountSuffix());
            header.Enabled = false;
            menu.Items.Add(header);
            menu.Items.Add(new ToolStripSeparator());

            IList<Session> list = store.Sessions;
            if (list.Count == 0)
            {
                ToolStripMenuItem none = new ToolStripMenuItem("（无活动会话）");
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
                                  StatusMap.Label(s.State) + " · " + Elapsed(s.Updated) + "  [" + Label(s) + "]";
                    ToolStripMenuItem it = new ToolStripMenuItem(line);
                    it.Enabled = false;
                    menu.Items.Add(it);
                }
            }

            menu.Items.Add(new ToolStripSeparator());

            ToolStripMenuItem mute = new ToolStripMenuItem(muted ? "🔔 取消静音" : "🔇 静音");
            mute.Click += delegate { muted = !muted; SaveConfig(); BuildMenu(); };
            menu.Items.Add(mute);

            ToolStripMenuItem test = new ToolStripMenuItem("🔊 测试声音");
            ToolStripMenuItem tWait = new ToolStripMenuItem("等待确认音");
            tWait.Click += delegate { sound.Play(Cue.Waiting); };
            ToolStripMenuItem tDone = new ToolStripMenuItem("完成音");
            tDone.Click += delegate { sound.Play(Cue.Done); };
            test.DropDownItems.Add(tWait);
            test.DropDownItems.Add(tDone);
            menu.Items.Add(test);

            ToolStripMenuItem openDir = new ToolStripMenuItem("📁 打开状态目录");
            openDir.Click += delegate { OpenStateDir(); };
            menu.Items.Add(openDir);

            ToolStripMenuItem auto = new ToolStripMenuItem("⏻ 开机自启");
            auto.Checked = IsAutoStart();
            auto.Click += delegate { SetAutoStart(!IsAutoStart()); BuildMenu(); };
            menu.Items.Add(auto);

            menu.Items.Add(new ToolStripSeparator());

            ToolStripMenuItem quit = new ToolStripMenuItem("❌ 退出");
            quit.Click += delegate { ExitApp(); };
            menu.Items.Add(quit);
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

        // ---- 绘制图标 ----
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

        // ---- 开机自启 ----
        const string RunKey = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
        const string RunName = "AgentPing";

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

        // ---- 配置 ----
        void LoadConfig()
        {
            try
            {
                if (File.Exists(Paths.ConfigFile))
                    muted = System.Text.RegularExpressions.Regex.IsMatch(
                        File.ReadAllText(Paths.ConfigFile), "\"muted\"\\s*:\\s*true");
            }
            catch { }
        }

        void SaveConfig()
        {
            try
            {
                Paths.EnsureDirs();
                File.WriteAllText(Paths.ConfigFile, "{\"muted\":" + (muted ? "true" : "false") + "}",
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
    //  声音引擎: Console.Beep 合成直觉化提示音 (零音频文件)
    //   等待确认: 上升音 660->990 (像在问"在吗?", 引起注意/未完成感)
    //   完成:     上行三连 770->1046->1318 (解决/积极感, "搞定~")
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
