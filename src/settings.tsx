import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

export type Settings = {
  downloadDir: string;
  temporaryDir: string;
  organizeByExtension: boolean;
  clipboardMonitoring: boolean;
  defaultConnections: number;
  maxConcurrentDownloads: number;
  proxyUrl: string;
  browserHandoff: "ask" | "always" | "browser";
};

type Tab = "general" | "downloads" | "connection" | "proxy" | "browser";
const tabs: { id: Tab; label: string }[] = [
  { id: "general", label: "常规" },
  { id: "downloads", label: "保存位置" },
  { id: "connection", label: "连接" },
  { id: "proxy", label: "代理" },
  { id: "browser", label: "浏览器" },
];

async function chooseDirectory() {
  const result = await open({ directory: true, multiple: false });
  return typeof result === "string" ? result : null;
}

export function SettingsPanel({ initial, onSave, onClose }: {
  initial: Settings;
  onSave: (next: Settings) => Promise<void>;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<Settings>(initial);
  const [tab, setTab] = useState<Tab>("general");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  function change<K extends keyof Settings>(key: K, value: Settings[K]) {
    setDraft((previous) => ({ ...previous, [key]: value }));
  }
  async function save() {
    setSaving(true); setError("");
    try { await onSave(draft); onClose(); }
    catch (cause) { setError(String(cause)); }
    finally { setSaving(false); }
  }
  return <div className="modal-backdrop" role="presentation" onMouseDown={onClose}>
    <section className="settings-modal" role="dialog" aria-modal="true" aria-label="设置" onMouseDown={(event) => event.stopPropagation()}>
      <div className="modal-top"><div><p className="eyebrow">PREFERENCES</p><h2>设置</h2></div><button className="icon-button" onClick={onClose} aria-label="关闭">×</button></div>
      <div className="settings-layout">
        <nav className="settings-tabs">{tabs.map((item) => <button key={item.id} className={tab === item.id ? "active" : ""} onClick={() => setTab(item.id)}>{item.label}</button>)}</nav>
        <div className="settings-content">
          {tab === "general" && <><h3>常规</h3><label className="setting-check"><input type="checkbox" checked={draft.clipboardMonitoring} onChange={(event) => change("clipboardMonitoring", event.target.checked)} /><span><strong>监听剪切板链接</strong><small>识别 HTTP(S) 和磁力链接后弹出确认，不自动开始下载。</small></span></label></>}
          {tab === "downloads" && <><h3>保存位置</h3><div className="setting-field"><label>默认保存目录</label><div className="path-row"><input value={draft.downloadDir} onChange={(event) => change("downloadDir", event.target.value)} /><button onClick={async () => { const path = await chooseDirectory(); if (path) change("downloadDir", path); }}>浏览</button></div><small>仅影响新任务；正在下载的任务可单独修改完成后的保存位置。</small></div><div className="setting-field"><label>半成品临时目录</label><div className="path-row"><input value={draft.temporaryDir} onChange={(event) => change("temporaryDir", event.target.value)} /><button onClick={async () => { const path = await chooseDirectory(); if (path) change("temporaryDir", path); }}>浏览</button></div><small>新任务从此处下载，完成后移到保存目录。正在下载的半成品不会在传输中搬移。</small></div><label className="setting-check"><input type="checkbox" checked={draft.organizeByExtension} onChange={(event) => change("organizeByExtension", event.target.checked)} /><span><strong>按文件后缀分配文件夹</strong><small>默认关闭。开启后将新建的链接任务分到文档、媒体、压缩包等文件夹；种子任务仍使用默认目录。</small></span></label></>}
          {tab === "connection" && <><h3>连接</h3><div className="setting-field"><label>新任务默认连接数</label><input type="number" min={1} max={32} value={draft.defaultConnections} onChange={(event) => change("defaultConnections", Number(event.target.value))} /><small>适用于 HTTP(S) 分段下载。具体速度仍取决于服务器支持。</small></div><div className="setting-field"><label>同时下载任务数</label><input type="number" min={1} max={32} value={draft.maxConcurrentDownloads} onChange={(event) => change("maxConcurrentDownloads", Number(event.target.value))} /><small>保存后立即调整 aria2 的全局任务队列。</small></div></>}
          {tab === "proxy" && <><h3>代理</h3><div className="setting-field"><label>新任务默认 HTTP 代理</label><input placeholder="留空为直连；例如 http://127.0.0.1:7890" value={draft.proxyUrl} onChange={(event) => change("proxyUrl", event.target.value)} /><small>当前仅支持不含账号密码的 HTTP 代理。进行中的任务可在任务设置中单独修改。</small></div></>}
          {tab === "browser" && <><h3>浏览器下载接管</h3><div className="setting-field"><label>Edge 插件默认处理方式</label><select value={draft.browserHandoff} onChange={(event) => change("browserHandoff", event.target.value as Settings["browserHandoff"])}><option value="ask">每次询问</option><option value="always">交给 Aurum</option><option value="browser">使用 Edge 自身下载</option></select><small>Edge 插件会读取此默认值。你还可以在插件弹窗中覆盖它；在“每次询问”模式下，下载会暂时暂停，等待选择。</small></div></>}
        </div>
      </div>
      {error && <p className="modal-error">{error}</p>}
      <div className="modal-actions"><button className="secondary-button" onClick={onClose}>取消</button><button className="primary-button" disabled={saving} onClick={() => void save()}>{saving ? "保存中…" : "保存设置"}</button></div>
    </section>
  </div>;
}

export type TaskConfig = {
  gid: string;
  connections: number;
  proxyUrl: string;
  saveDir: string;
  temporaryDir: string;
};

export function TaskConfigPanel({ initial, onSave, onClose, isTorrent }: {
  initial: TaskConfig;
  onSave: (next: TaskConfig) => Promise<void>;
  onClose: () => void;
  isTorrent: boolean;
}) {
  const [draft, setDraft] = useState(initial);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  async function save() {
    setSaving(true); setError("");
    try { await onSave(draft); onClose(); }
    catch (cause) { setError(String(cause)); }
    finally { setSaving(false); }
  }
  return <div className="modal-backdrop" role="presentation" onMouseDown={onClose}>
    <section className="task-modal" role="dialog" aria-modal="true" aria-label="调整任务" onMouseDown={(event) => event.stopPropagation()}>
      <div className="modal-top"><div><p className="eyebrow">TASK OPTIONS</p><h2>调整任务</h2></div><button className="icon-button" onClick={onClose} aria-label="关闭">×</button></div>
      <div className="setting-field"><label>完成后的保存目录</label><div className="path-row"><input value={draft.saveDir} onChange={(event) => setDraft({ ...draft, saveDir: event.target.value })} /><button onClick={async () => { const path = await chooseDirectory(); if (path) setDraft({ ...draft, saveDir: path }); }}>浏览</button></div><small>可以立即更改目标；半成品继续留在原临时目录，下载完成后搬到新位置。</small></div>
      <div className="setting-field"><label>当前半成品位置</label><input value={draft.temporaryDir} readOnly /><small>为了保持断点续传，传输中的半成品不会直接搬移。</small></div>
      <div className="setting-field"><label>{isTorrent ? "HTTP 分段连接数（不影响 BT peer 数）" : "下载连接数"}</label><input type="number" min={1} max={32} value={draft.connections} disabled={isTorrent} onChange={(event) => setDraft({ ...draft, connections: Number(event.target.value) })} /></div>
      <div className="setting-field"><label>此任务的 HTTP 代理</label><input placeholder="留空为直连；例如 http://127.0.0.1:7890" value={draft.proxyUrl} onChange={(event) => setDraft({ ...draft, proxyUrl: event.target.value })} /><small>修改连接数或代理后，aria2 会自动重启此任务的网络连接。BT peer 连接不受 HTTP 代理设置控制。</small></div>
      {error && <p className="modal-error">{error}</p>}
      <div className="modal-actions"><button className="secondary-button" onClick={onClose}>取消</button><button className="primary-button" disabled={saving} onClick={() => void save()}>{saving ? "应用中…" : "应用到任务"}</button></div>
    </section>
  </div>;
}
