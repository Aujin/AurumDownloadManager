import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { SettingsPanel, TaskConfigPanel, type Settings, type StartupStatus, type TaskConfig } from "./settings";

type TaskStatus = "active" | "waiting" | "paused" | "complete" | "error" | "removed";
type Task = {
  gid: string;
  status: TaskStatus;
  totalLength: string;
  completedLength: string;
  downloadSpeed: string;
  errorMessage?: string;
  files?: { path: string }[];
  bittorrent?: { info?: { name?: string } };
  saveDir?: string;
  temporaryDir?: string;
  moving?: boolean;
  moveError?: string;
  finalPaths?: string[];
  connections?: string;
  configuredConnections?: number;
  infoHash?: string;
  probe?: { resume: string; singleConnectionLimit: string; note: string };
};
type EngineInfo = { ready: boolean; message: string; downloadDir: string; temporaryDir: string };
type Filter = "all" | "active" | "completed";

function formatSize(value: string | number, rate = false) {
  let n = Number(value) || 0;
  const units = rate ? ["B/s", "KB/s", "MB/s", "GB/s"] : ["B", "KB", "MB", "GB", "TB"];
  let index = 0;
  while (n >= 1024 && index < units.length - 1) { n /= 1024; index++; }
  return `${n >= 10 || index === 0 ? n.toFixed(0) : n.toFixed(1)} ${units[index]}`;
}

function taskName(task: Task) {
  return task.bittorrent?.info?.name || task.files?.[0]?.path.split(/[\\/]/).pop() || `任务 ${task.gid.slice(0, 8)}`;
}

const labels: Record<TaskStatus, string> = {
  active: "下载中", waiting: "排队中", paused: "已暂停", complete: "已完成", error: "失败", removed: "已移除",
};

export default function App() {
  const [engine, setEngine] = useState<EngineInfo>({ ready: false, message: "正在连接下载内核…", downloadDir: "", temporaryDir: "" });
  const [tasks, setTasks] = useState<Task[]>([]);
  const [filter, setFilter] = useState<Filter>("all");
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [clipboardEnabled, setClipboardEnabled] = useState(false);
  const [clipboardCandidate, setClipboardCandidate] = useState("");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [newConnections, setNewConnections] = useState(8);
  const [changingConnections, setChangingConnections] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [startup, setStartup] = useState<StartupStatus | null>(null);
  const [taskConfig, setTaskConfig] = useState<TaskConfig | null>(null);
  const [taskConfigIsTorrent, setTaskConfigIsTorrent] = useState(false);

  async function refresh() {
    const next = await invoke<EngineInfo>("engine_info");
    setEngine(next);
    if (next.ready) setTasks(await invoke<Task[]>("list_tasks"));
  }

  useEffect(() => {
    let alive = true;
    const tick = async () => {
      try { if (alive) await refresh(); }
      catch (error) { if (alive) setNotice(String(error)); }
    };
    void tick();
    void invoke<Settings>("get_settings").then((value) => {
      if (alive) { setSettings(value); setNewConnections(value.defaultConnections); setClipboardEnabled(value.clipboardMonitoring); }
    }).catch((error) => { if (alive) setNotice(String(error)); });
    void invoke<StartupStatus>("get_startup_status").then((value) => { if (alive) setStartup(value); })
      .catch((error) => { if (alive) setNotice(String(error)); });
    const interval = window.setInterval(tick, 2000);
    const unlisten = listen<string>("clipboard-link", ({ payload }) => setClipboardCandidate(payload));
    return () => { alive = false; window.clearInterval(interval); void unlisten.then((stop) => stop()); };
  }, []);

  async function saveSettings(next: Settings) {
    await invoke("save_settings", { next });
    setSettings(next);
    setNewConnections(next.defaultConnections);
    setClipboardEnabled(next.clipboardMonitoring);
    await refresh();
  }

  async function changeStartup(enabled: boolean) {
    const next = await invoke<StartupStatus>("set_startup_enabled", { enabled });
    setStartup(next);
    return next;
  }

  async function openTaskConfig(task: Task) {
    try {
      const config = await invoke<TaskConfig>("get_task_config", { gid: task.gid });
      setTaskConfigIsTorrent(Boolean(task.bittorrent));
      setTaskConfig(config);
    } catch (error) { setNotice(String(error)); }
  }

  async function saveTaskConfig(config: TaskConfig) {
    await invoke("update_task_config", { config });
    await refresh();
  }

  async function submitUrl(value = url) {
    const candidate = value.trim();
    if (!candidate) return;
    setBusy(true); setNotice("");
    try {
      await invoke("add_url", { url: candidate, connections: newConnections });
      setUrl(""); setClipboardCandidate("");
      await refresh();
    } catch (error) { setNotice(String(error)); }
    finally { setBusy(false); }
  }

  async function changeTaskConnections(task: Task, connections: number) {
    setChangingConnections(task.gid); setNotice("");
    try { await invoke("set_task_connections", { gid: task.gid, connections }); await refresh(); }
    catch (error) { setNotice(String(error)); }
    finally { setChangingConnections(""); }
  }

  async function chooseTorrent() {
    const path = await open({ multiple: false, filters: [{ name: "BitTorrent", extensions: ["torrent"] }] });
    if (!path || Array.isArray(path)) return;
    setBusy(true); setNotice("");
    try { await invoke("add_torrent", { path }); await refresh(); }
    catch (error) { setNotice(String(error)); }
    finally { setBusy(false); }
  }

  async function act(task: Task, action: "pause" | "resume" | "remove" | "clear") {
    try { await invoke("task_action", { gid: task.gid, action }); await refresh(); }
    catch (error) { setNotice(String(error)); }
  }

  async function toggleClipboard() {
    const enabled = !clipboardEnabled;
    try {
      await invoke("set_clipboard_monitoring", { enabled });
      setClipboardEnabled(enabled);
      setSettings((previous) => previous ? { ...previous, clipboardMonitoring: enabled } : previous);
      if (!enabled) setClipboardCandidate("");
    } catch (error) { setNotice(String(error)); }
  }

  const visible = useMemo(() => tasks.filter((task) => {
    if (filter === "active") return ["active", "waiting", "paused"].includes(task.status);
    if (filter === "completed") return ["complete", "error", "removed"].includes(task.status);
    return true;
  }), [tasks, filter]);
  const activeCount = tasks.filter((task) => task.status === "active").length;
  const totalSpeed = tasks.reduce((sum, task) => sum + Number(task.downloadSpeed || 0), 0);

  return <div className="app-shell">
    <aside className="sidebar">
      <div className="brand"><span className="brand-icon">A</span><span><strong>Aurum</strong><small>DOWNLOAD MANAGER</small></span></div>
      <div className="nav-label">工作区</div>
      <button className={`nav-item ${filter === "all" ? "selected" : ""}`} onClick={() => setFilter("all")}>全部任务 <span>{tasks.length}</span></button>
      <button className={`nav-item ${filter === "active" ? "selected" : ""}`} onClick={() => setFilter("active")}>进行中 <span>{activeCount}</span></button>
      <button className={`nav-item ${filter === "completed" ? "selected" : ""}`} onClick={() => setFilter("completed")}>已结束</button>
      <button className="nav-item settings-nav" onClick={() => setSettingsOpen(true)}>设置 <span>⚙</span></button>
      <div className="sidebar-bottom"><span className={`status-dot ${engine.ready ? "online" : ""}`} />{engine.ready ? "下载内核已连接" : "下载内核未连接"}</div>
    </aside>
    <main className="main-area">
      <header className="topbar"><div><p className="eyebrow">AURUM / DOWNLOADS</p><h1>下载任务</h1><p className="subtitle">链接、种子与磁力任务，统一管理。</p></div><div className="speed-card"><span>当前下载速度</span><strong>{formatSize(totalSpeed, true)}</strong></div></header>
      <section className="add-panel">
        <div className="add-panel-header"><div><h2>添加新任务</h2><p>粘贴 HTTP、HTTPS 或 magnet 链接</p></div><button className="secondary-button" onClick={chooseTorrent} disabled={!engine.ready || busy}>选择 .torrent 文件</button></div>
        <form onSubmit={(event) => { event.preventDefault(); void submitUrl(); }} className="url-form"><input aria-label="下载链接" placeholder="https://example.com/file.zip 或 magnet:?xt=…" value={url} onChange={(event) => setUrl(event.target.value)} /><button type="submit" disabled={!engine.ready || busy || !url.trim()}>开始下载</button></form>
        <div className="new-connections"><label htmlFor="new-connections">新任务分段连接数</label><select id="new-connections" value={newConnections} onChange={(event) => setNewConnections(Number(event.target.value))}>{Array.from({ length: 32 }, (_, i) => <option key={i + 1} value={i + 1}>{i + 1}</option>)}</select><small>适用于 HTTP(S)。分段写入同一个文件；实际连接数取决于文件大小和服务器支持。</small></div>
        <div className="add-footer"><span>保存：{engine.downloadDir || "系统下载文件夹"}<br />半成品：{engine.temporaryDir || "未设置"}</span><label className="clipboard-control"><input type="checkbox" checked={clipboardEnabled} onChange={toggleClipboard} />监听剪切板</label></div>
      </section>
      {clipboardCandidate && <div className="clipboard-banner"><div><strong>剪切板中发现下载链接</strong><p>{clipboardCandidate}</p></div><div><button onClick={() => void submitUrl(clipboardCandidate)}>添加任务</button><button className="text-button" onClick={() => setClipboardCandidate("")}>忽略</button></div></div>}
      {!engine.ready && <div className="notice warning"><strong>下载内核尚未就绪</strong><span>{engine.message}</span></div>}
      {notice && <div className="notice"><span>{notice}</span><button onClick={() => setNotice("")}>关闭</button></div>}
      <section className="task-section"><div className="section-heading"><h2>{filter === "active" ? "进行中" : filter === "completed" ? "已结束" : "全部任务"}</h2><span>{visible.length} 个任务</span></div>
        {visible.length === 0 ? <div className="empty-state"><div className="empty-symbol">↓</div><h3>还没有下载任务</h3><p>输入链接或选择种子文件，开始你的第一个下载。</p></div> : <div className="task-list">{visible.map((task) => {
          const total = Number(task.totalLength || 0);
          const done = Number(task.completedLength || 0);
          const percent = total > 0 ? Math.min(100, Math.round(done / total * 100)) : 0;
          return <article className="task-card" key={task.gid}><div className="file-icon">{task.bittorrent ? "BT" : "↓"}</div><div className="task-detail"><div className="task-head"><strong title={taskName(task)}>{taskName(task)}</strong><span className={`task-status ${task.status}`}>{task.moving ? "搬运中" : labels[task.status]}</span></div><div className="task-meta"><span>{formatSize(done)} / {total ? formatSize(total) : "未知大小"}</span><span>{task.status === "active" ? formatSize(task.downloadSpeed, true) : `${percent}%`}</span></div><div className="progress-track"><div style={{ width: `${percent}%` }} /></div>{task.saveDir && <p className="task-path">完成后保存至：{task.saveDir}</p>}{task.probe && <p className="task-probe">续传：{task.probe.resume === "supported" ? "支持 Range，可尝试续传" : task.probe.resume === "unsupported" ? "服务器不支持 Range" : task.probe.resume === "checking" ? "检测中" : "无法确认"}　单连接限速：{task.probe.singleConnectionLimit === "suspected" ? "疑似存在" : task.probe.singleConnectionLimit === "not_observed" ? "未发现明显限制" : task.probe.singleConnectionLimit === "checking" ? "检测中" : "无法判断"}<span title={task.probe.note}>{task.probe.note && ` · ${task.probe.note}`}</span></p>}{task.errorMessage && <p className="task-error">{task.errorMessage}</p>}{task.moveError && <p className="task-error">{task.moveError}</p>}</div><div className="task-actions">{["active", "waiting", "paused"].includes(task.status) && !task.bittorrent && !task.infoHash && <label className="task-connections">连接数<select aria-label={`${taskName(task)} 分段连接数`} value={task.configuredConnections || settings?.defaultConnections || 8} disabled={changingConnections === task.gid} onChange={(event) => void changeTaskConnections(task, Number(event.target.value))}>{Array.from({ length: 32 }, (_, i) => <option key={i + 1} value={i + 1}>{i + 1}</option>)}</select><small>当前 {task.connections || "0"}</small></label>}{(["waiting", "active", "paused"].includes(task.status) || !!task.moveError) && <button onClick={() => void openTaskConfig(task)}>{task.moveError ? "调整并重试" : "调整"}</button>}{task.status === "active" && <button onClick={() => void act(task, "pause")}>暂停</button>}{task.status === "paused" && <button onClick={() => void act(task, "resume")}>继续</button>}{["waiting", "active", "paused"].includes(task.status) ? <button onClick={() => void act(task, "remove")}>移除</button> : <button onClick={() => void act(task, "clear")}>清除记录</button>}</div></article>;
        })}</div>}
      </section>
    </main>
    {settingsOpen && settings && <SettingsPanel initial={settings} startup={startup} onStartupChange={changeStartup} onSave={saveSettings} onClose={() => setSettingsOpen(false)} />}
    {taskConfig && <TaskConfigPanel initial={taskConfig} isTorrent={taskConfigIsTorrent} onSave={saveTaskConfig} onClose={() => setTaskConfig(null)} />}
  </div>;
}
