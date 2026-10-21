use serde_json::{json, Value};
use std::{env, fs, io::{self, Read, Write}, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

const APP_ID: &str = "com.aujin.aurumdownloadmanager";

fn app_data_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    { return env::var_os("APPDATA").map(|v| PathBuf::from(v).join(APP_ID)).ok_or("APPDATA 未设置".into()); }
    #[cfg(target_os = "macos")]
    { return env::var_os("HOME").map(|v| PathBuf::from(v).join("Library/Application Support").join(APP_ID)).ok_or("HOME 未设置".into()); }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        if let Some(path) = env::var_os("XDG_DATA_HOME") { return Ok(PathBuf::from(path).join(APP_ID)); }
        env::var_os("HOME").map(|v| PathBuf::from(v).join(".local/share").join(APP_ID)).ok_or("HOME 未设置".into())
    }
}

fn read_message() -> Result<Value, String> {
    let mut length = [0u8; 4];
    io::stdin().read_exact(&mut length).map_err(|e| e.to_string())?;
    let length = u32::from_le_bytes(length) as usize;
    if length > 65536 { return Err("消息过大".into()); }
    let mut payload = vec![0; length];
    io::stdin().read_exact(&mut payload).map_err(|e| e.to_string())?;
    serde_json::from_slice(&payload).map_err(|e| e.to_string())
}

fn reply(value: Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    let mut out = io::stdout().lock();
    out.write_all(&(bytes.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
    out.write_all(&bytes).and_then(|_| out.flush()).map_err(|e| e.to_string())
}

fn handle(message: Value) -> Result<Value, String> {
    let data_dir = app_data_dir()?;
    match message.get("type").and_then(Value::as_str) {
        Some("settings") => {
            if let Some(browser) = message.get("browser").and_then(Value::as_str).filter(|name| matches!(*name, "edge" | "chrome")) {
                fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
                let at = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
                fs::write(data_dir.join(format!("browser-{browser}-seen.json")),
                    serde_json::to_vec(&json!({ "at": at })).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            }
            let mode = fs::read(data_dir.join("settings.json")).ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .and_then(|value| value.get("browserHandoff").and_then(Value::as_str).map(str::to_owned))
                .filter(|value| matches!(value.as_str(), "ask" | "always" | "browser"))
                .unwrap_or_else(|| "ask".into());
            Ok(json!({ "ok": true, "mode": mode }))
        }
        Some("enqueue") => {
            let url = message.get("url").and_then(Value::as_str).ok_or("缺少下载链接")?.trim();
            if url.len() > 8192 || !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("magnet:?")) {
                return Err("仅支持 HTTP(S) 或 magnet 链接".into());
            }
            let inbox = data_dir.join("extension-inbox");
            fs::create_dir_all(&inbox).map_err(|e| e.to_string())?;
            let unique = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
            let stem = format!("{}-{unique}", std::process::id());
            let pending = inbox.join(format!("{stem}.pending"));
            let ready = inbox.join(format!("{stem}.json"));
            fs::write(&pending, serde_json::to_vec(&json!({ "url": url })).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            fs::rename(pending, ready).map_err(|e| e.to_string())?;
            Ok(json!({ "ok": true }))
        }
        Some("ping") => Ok(json!({ "ok": true })),
        _ => Err("未知操作".into()),
    }
}

fn main() {
    let response = match read_message().and_then(handle) {
        Ok(value) => value,
        Err(error) => json!({ "ok": false, "error": error }),
    };
    let _ = reply(response);
}
