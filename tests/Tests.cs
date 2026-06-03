// AgentPing 核心逻辑测试 (自包含断言 runner, 与 Core.cs 一起编译)
// 运行: run-tests.ps1  ; 退出码 0=全过, 1=有失败
using System;
using System.Collections.Generic;
using AgentPing;

static class Tests
{
    static int failed = 0;
    static int passed = 0;

    static void Check(bool cond, string name)
    {
        if (cond) { passed++; Console.WriteLine("  PASS  " + name); }
        else { failed++; Console.WriteLine("  FAIL  " + name); }
    }

    static void Eq(object actual, object expected, string name)
    {
        bool ok = (actual == null && expected == null) ||
                  (actual != null && actual.Equals(expected));
        if (ok) { passed++; Console.WriteLine("  PASS  " + name); }
        else { failed++; Console.WriteLine("  FAIL  " + name + "  (got=" + actual + " want=" + expected + ")"); }
    }

    static string Json(string agent, string session, string status, string title, long ts)
    {
        return "{\"agent\":\"" + agent + "\",\"session\":\"" + session + "\",\"status\":\"" +
               status + "\",\"title\":\"" + title + "\",\"ts\":" + ts + "}";
    }

    static int Main()
    {
        long now = Time.ToUnix(DateTime.UtcNow);
        DateTime NOW = DateTime.Now;

        // ---- J.Str / J.Long ----
        Eq(J.Str("{\"session_id\":\"abc123\"}", "session_id"), "abc123", "J.Str basic");
        Eq(J.Str("{\"cwd\":\"E:\\\\AI\\\\X\"}", "cwd"), "E:\\AI\\X", "J.Str unescape backslash");
        Eq(J.Long("{\"ts\":1780000000}", "ts"), 1780000000L, "J.Long");
        Eq(J.Str("{}", "missing"), null, "J.Str missing -> null");

        // ---- StatusMap ----
        Eq(StatusMap.FromString("waiting"), Status.Waiting, "FromString waiting");
        Eq(StatusMap.FromString("done"), Status.Done, "FromString done");
        Eq(StatusMap.FromString("processing"), Status.Processing, "FromString processing");
        Eq(StatusMap.Norm("Approval"), "waiting", "Norm approval->waiting");
        Eq(StatusMap.Norm("FINISHED"), "done", "Norm finished->done");
        Eq(StatusMap.Norm("xyz"), "processing", "Norm unknown->processing");
        Check(Status.Waiting > Status.Processing, "priority waiting>processing");
        Check(Status.Processing > Status.Done, "priority processing>done");
        Check(Status.Done > Status.Idle, "priority done>idle");

        // ---- Session.Parse ----
        Session s1 = Session.Parse("claude__abc", Json("claude", "sess-9KZ", "waiting", "MyTools", now), NOW);
        Eq(s1.Agent, "claude", "Parse agent");
        Eq(s1.Title, "MyTools", "Parse title");
        Eq(s1.State, Status.Waiting, "Parse state");
        Eq(s1.Tag, "S9KZ", "Parse tag last4 (SESS9KZ -> S9KZ)");
        Eq(Session.ShortTag("conversation-XY12"), "XY12", "ShortTag last4");

        // ---- Aggregate priority across multiple sessions ----
        StateStore st = new StateStore();
        List<Session> snap = new List<Session>();
        snap.Add(Session.Parse("claude__a", Json("claude", "a", "processing", "P1", now), NOW));
        snap.Add(Session.Parse("claude__b", Json("claude", "b", "done", "P2", now), NOW));
        st.Sync(snap, NOW);
        Eq(st.Aggregate(), Status.Processing, "Aggregate: processing>done");

        snap.Add(Session.Parse("codex__c", Json("codex", "c", "waiting", "P3", now), NOW));
        st.Sync(snap, NOW);
        Eq(st.Aggregate(), Status.Waiting, "Aggregate: waiting wins");
        Eq(st.Count, 3, "three distinct sessions coexist (multi-window)");

        // ---- Transitions / cues ----
        StateStore t = new StateStore();
        // 首次出现 processing -> 无声
        List<Session> a = new List<Session>();
        a.Add(Session.Parse("c__1", Json("claude", "1", "processing", "X", now), NOW));
        SyncResult r1 = t.Sync(a, NOW);
        Eq(r1.Cues.Count, 0, "processing entry: no cue");

        // processing -> waiting : 触发 waiting cue
        a[0] = Session.Parse("c__1", Json("claude", "1", "waiting", "X", now), NOW);
        SyncResult r2 = t.Sync(a, NOW);
        Eq(r2.Cues.Count, 1, "->waiting: one cue");
        Check(r2.Cues.Count == 1 && r2.Cues[0].Waiting, "->waiting: cue is Waiting");

        // waiting -> waiting (重复 sync) : 不重复发声
        SyncResult r3 = t.Sync(a, NOW);
        Eq(r3.Cues.Count, 0, "waiting steady: no repeat cue");

        // waiting -> processing : 无声 (批准后回到处理中, 关键修复点)
        a[0] = Session.Parse("c__1", Json("claude", "1", "processing", "X", now), NOW);
        SyncResult r4 = t.Sync(a, NOW);
        Eq(r4.Cues.Count, 0, "waiting->processing: no cue (resume blue)");
        Eq(t.Aggregate(), Status.Processing, "after approval -> blue/processing");

        // processing -> done : 触发 done cue
        a[0] = Session.Parse("c__1", Json("claude", "1", "done", "X", now), NOW);
        SyncResult r5 = t.Sync(a, NOW);
        Eq(r5.Cues.Count, 1, "->done: one cue");
        Check(r5.Cues.Count == 1 && !r5.Cues[0].Waiting, "->done: cue is Done");

        // ---- Prune (TTL) ----
        StateStore p = new StateStore();
        p.DoneTtl = TimeSpan.FromSeconds(60);
        long old = Time.ToUnix(DateTime.UtcNow.AddSeconds(-120));
        List<Session> ps = new List<Session>();
        ps.Add(Session.Parse("c__old", Json("claude", "old", "done", "X", old), NOW.AddSeconds(-120)));
        ps.Add(Session.Parse("c__new", Json("claude", "new", "done", "X", now), NOW));
        SyncResult pr = p.Sync(ps, NOW);
        Check(pr.Expired.Contains("c__old"), "stale done expired");
        Eq(p.Count, 1, "fresh done kept");
        Eq(p.Aggregate(), Status.Done, "fresh done -> green");

        // ---- Infer.Notification: 空闲 vs 权限 ----
        Eq(Infer.Notification("{\"message\":\"Claude is waiting for your input\"}"), "done",
           "idle notification -> done (no false alarm)");
        Eq(Infer.Notification("{\"message\":\"Claude needs your permission to use Bash\"}"), "waiting",
           "permission notification -> waiting");
        Eq(Infer.Notification("{\"message\":\"\"}"), "waiting", "empty notification -> waiting (default)");

        // ---- Infer.Auto (codex notify text) ----
        Eq(Infer.Auto("turn-ended"), "done", "auto: turn-ended -> done");
        Eq(Infer.Auto("{\"type\":\"agent-turn-complete\"}"), "done", "auto: turn-complete -> done");
        Eq(Infer.Auto("permission needed"), "waiting", "auto: permission -> waiting");

        Console.WriteLine("");
        Console.WriteLine("==== " + passed + " passed, " + failed + " failed ====");
        return failed == 0 ? 0 : 1;
    }
}
