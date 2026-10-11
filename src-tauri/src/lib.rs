use base64::Engine as _;
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use serde_json::{json, Value};
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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineInfo {
    ready: bool,
    message: String,
    download_dir: String,
}

fn start_engine(app: &tauri::AppHandle) -> Engine {
    match launch_engine(app) {
        Ok(engine) => engine,
        Err(error) => Engine {
            child: None,
            port: 0,
            secret: String::new(),
            startup_error: Some(error),
        },
    }
}

fn launch_engine(app: &tauri::AppHandle) -> Result<Engine, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let download_dir = app.path().download_dir().map_err(|e| e.to_string())?;
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
        .unwrap_or_else(|| PathBuf::from("aria2c"));

    let mut command = Command::new(binary);
    command
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
        .arg("--max-concurrent-downloads=4")
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
async fn engine_info(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<EngineInfo, String> {
    let download_dir = app.path().download_dir().map(|p| p.display().to_string()).unwrap_or_default();
    Ok(match rpc(&state, "aria2.getVersion", vec![]).await {
        Ok(result) => EngineInfo {
            ready: true,
            message: format!("aria2 {}", result.get("version").and_then(Value::as_str).unwrap_or("已连接")),
            download_dir,
        },
        Err(error) => EngineInfo { ready: false, message: error, download_dir },
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
    Ok(tasks)
}

#[tauri::command]
async fn add_url(state: tauri::State<'_, AppState>, url: String) -> Result<String, String> {
    let url = url.trim();
    let parsed = reqwest::Url::parse(url).map_err(|_| "请输入有效的链接")?;
    if !matches!(parsed.scheme(), "http" | "https" | "magnet") {
        return Err("目前支持 HTTP、HTTPS 和磁力链接".into());
    }
    let result = rpc(&state, "aria2.addUri", vec![json!([url])]).await?;
    result.as_str().map(str::to_owned).ok_or("任务创建失败".into())
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
    let result = rpc(&state, "aria2.addTorrent", vec![json!(encoded)]).await?;
    result.as_str().map(str::to_owned).ok_or("任务创建失败".into())
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
    Ok(())
}

#[tauri::command]
fn set_clipboard_monitoring(state: tauri::State<'_, AppState>, enabled: bool) {
    state.clipboard_enabled.store(enabled, Ordering::Relaxed);
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

pub fn run() {
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
            app.manage(AppState {
                engine: Mutex::new(start_engine(&handle)),
                clipboard_enabled: AtomicBool::new(false),
            });
            watch_clipboard(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_info,
            list_tasks,
            add_url,
            add_torrent,
            task_action,
            set_clipboard_monitoring,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Aurum Download Manager");
}
