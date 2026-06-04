// Agent Knocks - state core (no UI deps, shared by tray + tests)
// C# 5 syntax only (in-box csc.exe). Language-neutral: all display text lives in the UI layer.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace AgentKnocks
{
    // 优先级: Waiting > Processing > Done > Idle
    public enum Status { Idle = 0, Done = 1, Processing = 2, Waiting = 3 }

    public enum Cue { Waiting, Done }

    // ---- JSON 小工具 (扁平字段正则提取, 避免引 JSON 库) ----
    public static class J
    {
        public static string Str(string json, string field)
        {
            if (string.IsNullOrEmpty(json)) return null;
            Match m = Regex.Match(json, "\"" + Regex.Escape(field) + "\"\\s*:\\s*\"((?:[^\"\\\\]|\\\\.)*)\"");
            if (!m.Success) return null;
            return m.Groups[1].Value
                .Replace("\\\\", "\\").Replace("\\\"", "\"")
                .Replace("\\n", "\n").Replace("\\t", "\t").Replace("\\/", "/");
        }

        public static long Long(string json, string field)
        {
            if (string.IsNullOrEmpty(json)) return 0;
            Match m = Regex.Match(json, "\"" + Regex.Escape(field) + "\"\\s*:\\s*(\\d+)");
            long v;
            if (m.Success && long.TryParse(m.Groups[1].Value, out v)) return v;
            return 0;
        }

        public static string Esc(string s)
        {
            if (s == null) return "";
            return s.Replace("\\", "\\\\").Replace("\"", "\\\"").Replace("\n", "\\n").Replace("\r", "");
        }
    }

    public static class StatusMap
    {
        public static Status FromString(string s)
        {
            if (s == "waiting") return Status.Waiting;
            if (s == "done") return Status.Done;
            if (s == "processing") return Status.Processing;
            return Status.Processing;
        }

        // 归一化各种外部写法 -> processing/waiting/done
        public static string Norm(string s)
        {
            s = (s == null) ? "" : s.Trim().ToLowerInvariant();
            if (s == "waiting" || s == "wait" || s == "confirm" || s == "approval" ||
                s == "permission" || s == "needs_input") return "waiting";
            if (s == "done" || s == "complete" || s == "completed" || s == "finished" ||
                s == "stop") return "done";
            return "processing";
        }

        // Display labels are localized in the UI layer (I18n). Core stays language-neutral.

        public static string Glyph(Status s)
        {
            switch (s)
            {
                case Status.Waiting: return "🟠";
                case Status.Processing: return "🔵";
                case Status.Done: return "🟢";
                default: return "⚪";
            }
        }
    }

    public class Session
    {
        public string Agent;
        public string Key;
        public string Title;
        public string Tag;     // 短会话标签, 区分同项目多窗口
        public Status State;
        public DateTime Updated;

        public static Session Parse(string key, string json, DateTime fileTimeLocal)
        {
            if (string.IsNullOrEmpty(json)) return null;
            Session s = new Session();
            s.Key = key;
            s.Agent = J.Str(json, "agent");
            if (string.IsNullOrEmpty(s.Agent)) s.Agent = key;
            s.Title = J.Str(json, "title");
            if (string.IsNullOrEmpty(s.Title)) s.Title = s.Agent;
            s.State = StatusMap.FromString(J.Str(json, "status"));
            string sess = J.Str(json, "session");
            s.Tag = ShortTag(string.IsNullOrEmpty(sess) ? key : sess);
            long ts = J.Long(json, "ts");
            s.Updated = (ts > 0) ? Time.FromUnix(ts).ToLocalTime() : fileTimeLocal;
            return s;
        }

        public static string ShortTag(string s)
        {
            if (string.IsNullOrEmpty(s)) return "----";
            StringBuilder sb = new StringBuilder();
            foreach (char c in s)
                if (char.IsLetterOrDigit(c)) sb.Append(char.ToUpperInvariant(c));
            string a = sb.ToString();
            if (a.Length == 0) return "----";
            return a.Length <= 4 ? a : a.Substring(a.Length - 4);
        }
    }

    public class Fired
    {
        public Session S;
        public bool Waiting;
        public Fired(Session s, bool waiting) { S = s; Waiting = waiting; }
    }

    public class SyncResult
    {
        public List<Fired> Cues = new List<Fired>();
        public List<string> Expired = new List<string>(); // 应删除的过期 key
    }

    // ---- 纯状态机: 摄入会话快照 -> 过期淘汰 + 跃迁检测 + 聚合 ----
    public class StateStore
    {
        public TimeSpan DoneTtl = TimeSpan.FromMinutes(1);
        public TimeSpan ProcessingTtl = TimeSpan.FromMinutes(45);
        public TimeSpan WaitingTtl = TimeSpan.FromHours(3);

        Dictionary<string, Session> sessions = new Dictionary<string, Session>();
        Dictionary<string, Status> lastSeen = new Dictionary<string, Status>();

        public IList<Session> Sessions
        {
            get { return new List<Session>(sessions.Values); }
        }
        public int Count { get { return sessions.Count; } }

        // current = 当前磁盘上解析出的全部会话快照
        public SyncResult Sync(IList<Session> current, DateTime now)
        {
            SyncResult res = new SyncResult();

            // 1. 过期淘汰
            Dictionary<string, Session> keep = new Dictionary<string, Session>();
            for (int i = 0; i < current.Count; i++)
            {
                Session s = current[i];
                if (s == null) continue;
                TimeSpan age = now - s.Updated;
                if (s.State == Status.Done && age > DoneTtl) { res.Expired.Add(s.Key); continue; }
                if (s.State == Status.Processing && age > ProcessingTtl) { res.Expired.Add(s.Key); continue; }
                if (s.State == Status.Waiting && age > WaitingTtl) { res.Expired.Add(s.Key); continue; }
                keep[s.Key] = s;
            }
            sessions = keep;

            // 2. 跃迁检测 (仅 进入 waiting / done 时发声)
            foreach (KeyValuePair<string, Session> kv in sessions)
            {
                Status nowState = kv.Value.State;
                Status prev;
                bool known = lastSeen.TryGetValue(kv.Key, out prev);
                if (!known || prev != nowState)
                {
                    if (nowState == Status.Waiting) res.Cues.Add(new Fired(kv.Value, true));
                    else if (nowState == Status.Done) res.Cues.Add(new Fired(kv.Value, false));
                }
            }

            // 3. 刷新 lastSeen (只保留当前存在的会话)
            Dictionary<string, Status> newSeen = new Dictionary<string, Status>();
            foreach (KeyValuePair<string, Session> kv in sessions)
                newSeen[kv.Key] = kv.Value.State;
            lastSeen = newSeen;

            return res;
        }

        public Status Aggregate()
        {
            Status agg = Status.Idle;
            foreach (KeyValuePair<string, Session> kv in sessions)
                if (kv.Value.State > agg) agg = kv.Value.State;
            return agg;
        }

        // Expose per-state counts; the UI formats them in the active language.
        public void Counts(out int waiting, out int processing, out int done)
        {
            waiting = 0; processing = 0; done = 0;
            foreach (KeyValuePair<string, Session> kv in sessions)
            {
                if (kv.Value.State == Status.Waiting) waiting++;
                else if (kv.Value.State == Status.Processing) processing++;
                else if (kv.Value.State == Status.Done) done++;
            }
        }
    }

    // ---- emit 侧推断逻辑 (可测试) ----
    public static class Infer
    {
        // codex 等 --status auto: 从事件文本推断
        public static string Auto(string blob)
        {
            string low = (blob == null ? "" : blob).ToLowerInvariant();
            if (low.Contains("turn-ended") || low.Contains("turn-complete") ||
                low.Contains("agent-turn-complete") || low.Contains("complete") ||
                low.Contains("finished")) return "done";
            if (low.Contains("approval") || low.Contains("permission") ||
                low.Contains("confirm") || low.Contains("input")) return "waiting";
            return "processing";
        }

        // Claude Notification: 区分"权限/确认请求"(waiting) 与"空闲等你输入"(idle->done)
        // 空闲提示不应触发等待报警 (否则每轮结束都变橙灯)
        public static string Notification(string stdinJson)
        {
            string msg = J.Str(stdinJson, "message");
            string low = (msg == null ? "" : msg).ToLowerInvariant();
            if (low.Length > 0)
            {
                // 空闲类: "Claude is waiting for your input" / "idle"
                // Stop hook 已报过完成, 这条是 ~60s 后的闲置提醒 -> 忽略, 否则重复弹"处理完成"
                if (low.Contains("waiting for your input") || low.Contains("idle"))
                    return "ignore";
                // 权限/确认类
                if (low.Contains("permission") || low.Contains("approve") ||
                    low.Contains("confirm") || low.Contains("needs your"))
                    return "waiting";
            }
            // 默认: 当作需要注意 -> 等待确认
            return "waiting";
        }
    }

    public static class Time
    {
        public static DateTime FromUnix(long s)
        {
            return new DateTime(1970, 1, 1, 0, 0, 0, DateTimeKind.Utc).AddSeconds(s);
        }
        public static long ToUnix(DateTime utc)
        {
            return (long)(utc - new DateTime(1970, 1, 1, 0, 0, 0, DateTimeKind.Utc)).TotalSeconds;
        }
    }
}
