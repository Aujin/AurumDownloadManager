mod placement;
mod probe;
mod settings;
mod startup;

use base64::Engine as _;
use placement::{Placement, Placements};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use settings::Settings;
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{atomic::{AtomicBool, Ordering}, Mutex},
    time::Duration,
};
use tauri::{Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

struct Engine {
    child: Option<Child>,
    port: u16,
    secret: String,
    startup_error: Option<String>,
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Engine {
    fn shutdown(&mut self) {
        if let Some(mut child) = self.child.take() {
            self.send_exit_rpc("aria2.saveSession");
            self.send_exit_rpc("aria2.shutdown");
            for _ in 0..20 {
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn send_exit_rpc(&self, method: &str) {
        if let Ok(mut stream) = TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], self.port)),
            Duration::from_secs(1),
        ) {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let body = json!({
                "jsonrpc": "2.0", "id": "exit", "method": method,
                "params": [format!("token:{}", self.secret)]
            }).to_string();
            let request = format!(
                "POST /jsonrpc HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            let _ = stream.write_all(request.as_bytes());
            let mut response = Vec::new();
            let _ = stream.read_to_end(&mut response);
        }
    }
}

struct AppState {
    engine: Mutex<Engine>,
    clipboard_enabled: AtomicBool,
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    placements: Placements,
    placements_path: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineInfo {
    ready: bool,
    message: String,
    download_dir: String,
    temporary_dir: String,
}

fn start_engine(app: &tauri::AppHandle, settings: &Settings) -> Engine {
    match launch_engine(app, settings) {
        Ok(engine) => engine,
        Err(error) => Engine {
            child: None,
            port: 0,
            secret: String::new(),
            startup_error: Some(error),
        },
    }
}

fn launch_engine(app: &tauri::AppHandle, settings: &Settings) -> Result<Engine, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let download_dir = settings::checked_dir(&settings.temporary_dir)?;
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&download_dir).map_err(|e| e.to_string())?;

    let session = data_dir.join("aria2.session");
    if !session.exists() {
        fs::write(&session, "").map_err(|e| e.to_string())?;
    }

    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    drop(listener);

    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let secret = bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    let binary = std::env::var_os("ADM_ARIA2_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            let name = if cfg!(windows) { "aria2c.exe" } else { "aria2c" };
            std::env::current_exe().ok()?.parent().map(|dir| dir.join(name)).filter(|path| path.is_file())
        })
        .unwrap_or_else(|| PathBuf::from("aria2c"));

    let mut command = Command::new(binary);
    command
        .arg("--no-conf=true")
        .arg("--enable-rpc=true")
        .arg("--rpc-listen-all=false")
        .arg(format!("--rpc-listen-port={port}"))
        .arg(format!("--rpc-secret={secret}"))
        .arg(format!("--dir={}", download_dir.display()))
        .arg(format!("--input-file={}", session.display()))
        .arg(format!("--save-session={}", session.display()))
        .arg("--save-session-interval=5")
        .arg("--rpc-save-upload-metadata=true")
        .arg("--continue=true")
        .arg(format!("--max-concurrent-downloads={}", settings.max_concurrent_downloads))
        .arg("--seed-time=0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let child = command.spawn().map_err(|error| {
        format!("无法启动 aria2c：{error}。请安装 aria2，或设置 ADM_ARIA2_PATH。")
    })?;
    Ok(Engine { child: Some(child), port, secret, startup_error: None })
}

fn endpoint(state: &AppState) -> Result<(u16, String), String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    if let Some(error) = &engine.startup_error {
        return Err(error.clone());
    }
    let child = engine.child.as_mut().ok_or("下载内核尚未启动")?;
    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
        return Err(format!("aria2 已退出：{status}"));
    }
    Ok((engine.port, engine.secret.clone()))
}

async fn rpc(state: &AppState, method: &str, mut params: Vec<Value>) -> Result<Value, String> {
    let (port, secret) = endpoint(state)?;
    params.insert(0, json!(format!("token:{secret}")));
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/jsonrpc"))
        .timeout(Duration::from_secs(10))
        .json(&json!({ "jsonrpc": "2.0", "id": "adm", "method": method, "params": params }))
        .send()
        .await
        .map_err(|e| format!("连接下载内核失败：{e}"))?;
    let body: Value = response.json().await.map_err(|e| e.to_string())?;
    if let Some(error) = body.get("error") {
        return Err(error.get("message").and_then(Value::as_str).unwrap_or("下载内核返回错误").to_string());
    }
    body.get("result").cloned().ok_or("下载内核未返回结果".into())
}

#[tauri::command]
async fn engine_info(state: tauri::State<'_, AppState>) -> Result<EngineInfo, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?.clone();
    let download_dir = settings.download_dir;
    let temporary_dir = settings.temporary_dir;
    Ok(match rpc(&state, "aria2.getVersion", vec![]).await {
        Ok(result) => EngineInfo {
            ready: true,
            message: format!("aria2 {}", result.get("version").and_then(Value::as_str).unwrap_or("已连接")),
            download_dir,
            temporary_dir,
        },
        Err(error) => EngineInfo { ready: false, message: error, download_dir, temporary_dir },
    })
}

#[tauri::command]
async fn list_tasks(state: tauri::State<'_, AppState>) -> Result<Vec<Value>, String> {
    let mut tasks = Vec::new();
    for (method, params) in [
        ("aria2.tellActive", vec![]),
        ("aria2.tellWaiting", vec![json!(0), json!(1000)]),
        ("aria2.tellStopped", vec![json!(0), json!(1000)]),
    ] {
        let result = rpc(&state, method, params).await?;
        tasks.extend(result.as_array().ok_or("无效的任务列表")?.iter().cloned());
    }
    for task in &mut tasks {
        placement::decorate_and_finalize(task, &state.placements, &state.placements_path);
    }
    Ok(tasks)
}

fn new_gid() -> String {
    let mut bytes = [0u8; 8];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn task_defaults(state: &AppState, source: Option<&str>, requested_connections: Option<u8>) -> Result<(String, PathBuf, PathBuf, Value, u8), String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?.clone();
    let connections = requested_connections.unwrap_or(settings.default_connections);
    if !(1..=32).contains(&connections) { return Err("连接数应为 1–32".into()); }
    let gid = new_gid();
    let temporary_dir = PathBuf::from(&settings.temporary_dir).join(&gid);
    fs::create_dir_all(&temporary_dir).map_err(|e| e.to_string())?;
    let save_dir = source.map(|url| settings::organized_dir(&settings, url))
        .unwrap_or_else(|| PathBuf::from(&settings.download_dir));
    let mut options = json!({
        "gid": gid,
        "dir": temporary_dir.display().to_string(),
        "split": connections.to_string(),
        "max-connection-per-server": connections.to_string(),
        "min-split-size": "1M",
    });
    if !settings.proxy_url.is_empty() {
        options["all-proxy"] = json!(settings.proxy_url);
    }
    Ok((gid, temporary_dir, save_dir, options, connections))
}

fn record_placement(state: &AppState, gid: &str, temporary_dir: PathBuf, save_dir: PathBuf, connections: u8, probe: Option<probe::ProbeResult>) -> Result<(), String> {
    let mut guard = state.placements.lock().map_err(|e| e.to_string())?;
    guard.insert(gid.to_owned(), Placement {
        save_dir: save_dir.display().to_string(),
        temporary_dir: temporary_dir.display().to_string(),
        moved: false, final_paths: Vec::new(), error: String::new(), moving: false,
        connections, probe,
    });
    placement::persist(&state.placements_path, &guard)
}

#[tauri::command]
async fn add_url(state: tauri::State<'_, AppState>, url: String, connections: Option<u8>) -> Result<String, String> {
    add_url_inner(&state, &url, connections).await
}

async fn add_url_inner(state: &AppState, url: &str, connections: Option<u8>) -> Result<String, String> {
    let url = url.trim();
    let parsed = reqwest::Url::parse(url).map_err(|_| "请输入有效的链接")?;
    if !matches!(parsed.scheme(), "http" | "https" | "magnet") {
        return Err("目前支持 HTTP、HTTPS 和磁力链接".into());
    }
    let is_http = matches!(parsed.scheme(), "http" | "https");
    let proxy_url = state.settings.lock().map_err(|e| e.to_string())?.proxy_url.clone();
    let (gid, temporary_dir, save_dir, options, connections) = task_defaults(state, Some(url), if is_http { connections } else { None })?;
    let result = rpc(state, "aria2.addUri", vec![json!([url]), options]).await?;
    let actual_gid = result.as_str().ok_or("任务创建失败")?.to_owned();
    if actual_gid != gid { return Err("下载内核返回了意外的任务编号".into()); }
    record_placement(state, &gid, temporary_dir, save_dir, connections, is_http.then(probe::ProbeResult::checking))?;
    if is_http {
        let placements = state.placements.clone();
        let path = state.placements_path.clone();
        let gid_for_probe = gid.clone();
        let url_for_probe = url.to_owned();
        tauri::async_runtime::spawn(async move {
            let result = probe::inspect(&url_for_probe, &proxy_url).await;
            if let Ok(mut guard) = placements.lock() {
                if let Some(entry) = guard.get_mut(&gid_for_probe) { entry.probe = Some(result); }
                let _ = placement::persist(&path, &guard);
            }
        });
    }
    Ok(gid)
}

#[tauri::command]
async fn add_torrent(state: tauri::State<'_, AppState>, path: String) -> Result<String, String> {
    let path = PathBuf::from(path);
    if !path.is_file() || path.extension().and_then(|e| e.to_str()).map(|e| !e.eq_ignore_ascii_case("torrent")).unwrap_or(true) {
        return Err("请选择 .torrent 文件".into());
    }
    let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
    if metadata.len() > 16 * 1024 * 1024 {
        return Err("种子文件不能超过 16 MiB".into());
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let (gid, temporary_dir, save_dir, options, connections) = task_defaults(&state, None, None)?;
    let result = rpc(&state, "aria2.addTorrent", vec![json!(encoded), json!([]), options]).await?;
    let actual_gid = result.as_str().ok_or("任务创建失败")?.to_owned();
    if actual_gid != gid { return Err("下载内核返回了意外的任务编号".into()); }
    record_placement(&state, &gid, temporary_dir, save_dir, connections, None)?;
    Ok(gid)
}

#[tauri::command]
async fn task_action(state: tauri::State<'_, AppState>, gid: String, action: String) -> Result<(), String> {
    if gid.len() != 16 || !gid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("无效的任务编号".into());
    }
    let method = match action.as_str() {
        "pause" => "aria2.pause",
        "resume" => "aria2.unpause",
        "remove" => "aria2.remove",
        "clear" => "aria2.removeDownloadResult",
        _ => return Err("无效的操作".into()),
    };
    rpc(&state, method, vec![json!(gid)]).await?;
    if action == "clear" {
        let mut guard = state.placements.lock().map_err(|e| e.to_string())?;
        guard.remove(&gid);
        placement::persist(&state.placements_path, &guard)?;
    }
    Ok(())
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Result<Settings, String> {
    state.settings.lock().map(|settings| settings.clone()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_settings(state: tauri::State<'_, AppState>, next: Settings) -> Result<(), String> {
    next.validate()?;
    if endpoint(&state).is_ok() {
        rpc(&state, "aria2.changeGlobalOption", vec![json!({
            "max-concurrent-downloads": next.max_concurrent_downloads.to_string()
        })]).await?;
    }
    next.save(&state.settings_path)?;
    state.clipboard_enabled.store(next.clipboard_monitoring, Ordering::Relaxed);
    *state.settings.lock().map_err(|e| e.to_string())? = next;
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskConfig {
    gid: String,
    connections: u8,
    proxy_url: String,
    save_dir: String,
    temporary_dir: String,
}

#[tauri::command]
async fn get_task_config(state: tauri::State<'_, AppState>, gid: String) -> Result<TaskConfig, String> {
    validate_gid(&gid)?;
    let options = rpc(&state, "aria2.getOption", vec![json!(gid)]).await?;
    let saved = state.placements.lock().map_err(|e| e.to_string())?.get(&gid).cloned();
    let actual_dir = options.get("dir").and_then(Value::as_str).unwrap_or("").to_owned();
    Ok(TaskConfig {
        gid,
        connections: options.get("split").and_then(Value::as_str).and_then(|v| v.parse().ok()).unwrap_or(1),
        proxy_url: options.get("all-proxy").and_then(Value::as_str).unwrap_or("").to_owned(),
        save_dir: saved.as_ref().map(|p| p.save_dir.clone()).unwrap_or_else(|| actual_dir.clone()),
        temporary_dir: saved.as_ref().map(|p| p.temporary_dir.clone()).unwrap_or(actual_dir),
    })
}

#[tauri::command]
async fn update_task_config(state: tauri::State<'_, AppState>, config: TaskConfig) -> Result<(), String> {
    validate_gid(&config.gid)?;
    if !(1..=32).contains(&config.connections) { return Err("连接数应为 1–32".into()); }
    settings::validate_proxy(&config.proxy_url)?;
    let save_dir = settings::checked_dir(&config.save_dir)?;
    let status = rpc(&state, "aria2.tellStatus", vec![json!(config.gid)]).await?;
    let completed = status.get("status").and_then(Value::as_str) == Some("complete");
    let options = rpc(&state, "aria2.getOption", vec![json!(config.gid)]).await?;
    let current_dir = options.get("dir").and_then(Value::as_str).ok_or("无法读取任务临时目录")?;
    if !completed {
        rpc(&state, "aria2.changeOption", vec![json!(config.gid), json!({
            "split": config.connections.to_string(),
            "max-connection-per-server": config.connections.to_string(),
            "min-split-size": "1M",
            "all-proxy": config.proxy_url,
        })]).await?;
    }
    let mut guard = state.placements.lock().map_err(|e| e.to_string())?;
    let entry = guard.entry(config.gid).or_insert_with(|| Placement {
        save_dir: save_dir.display().to_string(),
        temporary_dir: current_dir.to_owned(),
        moved: false, final_paths: Vec::new(), error: String::new(), moving: false,
        connections: config.connections, probe: None,
    });
    if completed && entry.moved {
        return Err("文件已搬运完成，不能再修改此任务的保存位置".into());
    }
    entry.save_dir = save_dir.display().to_string();
    entry.connections = config.connections;
    entry.error.clear();
    placement::persist(&state.placements_path, &guard)
}

#[tauri::command]
async fn set_task_connections(state: tauri::State<'_, AppState>, gid: String, connections: u8) -> Result<(), String> {
    validate_gid(&gid)?;
    if !(1..=32).contains(&connections) { return Err("连接数应为 1–32".into()); }
    let status = rpc(&state, "aria2.tellStatus", vec![json!(gid)]).await?;
    if !matches!(status.get("status").and_then(Value::as_str), Some("active" | "waiting" | "paused")) {
        return Err("只有进行中或暂停的任务可以调整连接数".into());
    }
    if status.get("bittorrent").is_some() || status.get("infoHash").is_some() {
        return Err("分段连接数仅适用于 HTTP(S) 下载".into());
    }
    rpc(&state, "aria2.changeOption", vec![json!(gid), json!({
        "split": connections.to_string(),
        "max-connection-per-server": connections.to_string(),
        "min-split-size": "1M",
    })]).await?;
    let mut guard = state.placements.lock().map_err(|e| e.to_string())?;
    if let Some(entry) = guard.get_mut(&gid) { entry.connections = connections; }
    placement::persist(&state.placements_path, &guard)
}

fn validate_gid(gid: &str) -> Result<(), String> {
    if gid.len() != 16 || !gid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("无效的任务编号".into());
    }
    Ok(())
}

#[tauri::command]
fn set_clipboard_monitoring(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.clipboard_monitoring = enabled;
    settings.save(&state.settings_path)?;
    state.clipboard_enabled.store(enabled, Ordering::Relaxed);
    Ok(())
}

fn is_download_link(text: &str) -> bool {
    if text.len() > 8192 || text.contains(char::is_whitespace) {
        return false;
    }
    reqwest::Url::parse(text).map(|url| matches!(url.scheme(), "http" | "https" | "magnet")).unwrap_or(false)
}

fn watch_clipboard(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut previous = String::new();
        loop {
            std::thread::sleep(Duration::from_millis(900));
            let state = app.state::<AppState>();
            if !state.clipboard_enabled.load(Ordering::Relaxed) {
                continue;
            }
            if let Ok(text) = app.clipboard().read_text() {
                let candidate = text.trim();
                if candidate != previous && is_download_link(candidate) {
                    previous = candidate.to_string();
                    let _ = app.emit("clipboard-link", candidate);
                }
            }
        }
    });
}

fn watch_extension_inbox(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let inbox = match app.path().app_data_dir() {
            Ok(path) => path.join("extension-inbox"),
            Err(_) => return,
        };
        loop {
            std::thread::sleep(Duration::from_millis(900));
            let Ok(entries) = fs::read_dir(&inbox) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
                let Some(url) = fs::read(&path).ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .and_then(|value| value.get("url").and_then(Value::as_str).map(str::to_owned)) else {
                        let _ = fs::remove_file(path);
                        continue;
                    };
                let state = app.state::<AppState>();
                if tauri::async_runtime::block_on(add_url_inner(&state, &url, None)).is_ok() {
                    let _ = fs::remove_file(path);
                }
            }
        }
    });
}

pub fn run() {
    if startup::run_copy_helper() {
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                let state = window.state::<AppState>();
                if let Ok(mut engine) = state.engine.lock() {
                    engine.shutdown();
                };
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = handle.path().app_data_dir()?;
            fs::create_dir_all(&data_dir)?;
            let default_dir = handle.path().download_dir()?;
            let settings_path = data_dir.join("settings.json");
            let settings = Settings::load(&settings_path, &default_dir);
            let placements_path = data_dir.join("placements.json");
            app.manage(AppState {
                engine: Mutex::new(start_engine(&handle, &settings)),
                clipboard_enabled: AtomicBool::new(settings.clipboard_monitoring),
                settings: Mutex::new(settings),
                settings_path,
                placements: placement::load(&placements_path),
                placements_path,
            });
            watch_clipboard(handle);
            watch_extension_inbox(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_info,
            list_tasks,
            add_url,
            add_torrent,
            task_action,
            get_settings,
            save_settings,
            get_task_config,
            update_task_config,
            set_task_connections,
            set_clipboard_monitoring,
            startup::get_startup_status,
            startup::set_startup_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Aurum Download Manager");
}
