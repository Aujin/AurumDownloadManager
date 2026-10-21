use serde::Serialize;

pub const EXTENSION_ID: &str = "klblkidgakkmnemkgdhgdpjlhblbhhgb";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStatus {
    supported: bool,
    registered: bool,
    extension_id: &'static str,
    extension_path: String,
    message: String,
}

#[cfg(windows)]
mod windows {
    use super::{BridgeStatus, EXTENSION_ID};
    use std::{fs, path::PathBuf};
    use tauri::{AppHandle, Manager};
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    const HOST_NAME: &str = "com.aujin.aurumdownloadmanager";
    const REGISTRY_KEY: &str =
        r"Software\Microsoft\Edge\NativeMessagingHosts\com.aujin.aurumdownloadmanager";
    const EXTENSION_FILES: &[&str] = &[
        "background.js", "popup.html", "popup.js", "icon128.png", "manifest.json",
    ];

    fn extension_dir(app: &AppHandle) -> Result<PathBuf, String> {
        Ok(app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("edge-extension"))
    }

    fn manifest_path(app: &AppHandle) -> Result<PathBuf, String> {
        Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("native-host.json"))
    }

    fn host_path() -> Result<PathBuf, String> {
        let current = std::env::current_exe().map_err(|e| e.to_string())?;
        let current_host = current.parent().ok_or("无法定位 Aurum 目录")?.join("aurum-native-host.exe");
        if crate::startup::get_startup_status()?.enabled {
            let installed = PathBuf::from(std::env::var_os("ProgramFiles").ok_or("无法定位 Program Files")?)
                .join("Aurum Download Manager").join("aurum-native-host.exe");
            if installed.is_file() { return Ok(installed); }
        }
        if current_host.is_file() { Ok(current_host) }
        else { Err("当前目录缺少 aurum-native-host.exe；请使用完整便携包。".into()) }
    }

    pub fn install(app: &AppHandle) -> Result<BridgeStatus, String> {
        let current = std::env::current_exe().map_err(|e| e.to_string())?;
        let source = current.parent().ok_or("无法定位 Aurum 目录")?.join("edge-extension");
        let destination = extension_dir(app)?;
        if !source.join("manifest.json").is_file() {
            return Err("当前目录缺少 edge-extension 插件文件夹；请使用完整便携包。".into());
        }
        fs::create_dir_all(&destination).map_err(|e| format!("无法创建插件目录：{e}"))?;
        for name in EXTENSION_FILES {
            let from = source.join(name);
            if !from.is_file() { return Err(format!("插件文件缺失：{name}")); }
            fs::copy(&from, destination.join(name)).map_err(|e| format!("无法更新插件文件 {name}：{e}"))?;
        }

        let host = host_path()?;
        let manifest = manifest_path(app)?;
        if let Some(parent) = manifest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("无法创建桥接目录：{e}"))?;
        }
        let contents = serde_json::json!({
            "name": HOST_NAME,
            "description": "Aurum Download Manager bridge",
            "path": host.display().to_string(),
            "type": "stdio",
            "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
        });
        fs::write(&manifest, serde_json::to_vec_pretty(&contents).map_err(|e| e.to_string())?)
            .map_err(|e| format!("无法写入 Edge 桥接文件：{e}"))?;
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(REGISTRY_KEY)
            .map_err(|e| format!("无法注册 Edge 通信组件：{e}"))?.0;
        key.set_value("", &manifest.display().to_string())
            .map_err(|e| format!("无法注册 Edge 通信组件：{e}"))?;
        status(app)
    }

    pub fn status(app: &AppHandle) -> Result<BridgeStatus, String> {
        let extension_path = extension_dir(app)?.display().to_string();
        let registered = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(REGISTRY_KEY)
            .ok()
            .and_then(|key| key.get_value::<String, _>("").ok())
            .and_then(|path| fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .map(|manifest| {
                manifest.get("allowed_origins").and_then(|v| v.as_array())
                    .map(|origins| origins.iter().any(|origin| origin.as_str() == Some(&format!("chrome-extension://{EXTENSION_ID}/"))))
                    .unwrap_or(false)
                    && manifest.get("path").and_then(|v| v.as_str()).map(|p| PathBuf::from(p).is_file()).unwrap_or(false)
            })
            .unwrap_or(false);
        Ok(BridgeStatus {
            supported: true,
            registered,
            extension_id: EXTENSION_ID,
            extension_path,
            message: if registered { "Edge 通信组件已注册。更新插件文件后，请在 Edge 扩展页重新加载插件。" }
                else { "Edge 通信组件未注册，请点击“修复 Edge 连接”。" }.into(),
        })
    }
}

#[tauri::command]
pub fn get_edge_bridge_status(app: tauri::AppHandle) -> Result<BridgeStatus, String> {
    #[cfg(windows)]
    { windows::status(&app) }
    #[cfg(not(windows))]
    {
        let _ = app;
        Ok(BridgeStatus { supported: false, registered: false, extension_id: EXTENSION_ID,
            extension_path: String::new(), message: "此平台暂未实现 Edge 本机通信组件".into() })
    }
}

#[tauri::command]
pub fn repair_edge_bridge(app: tauri::AppHandle) -> Result<BridgeStatus, String> {
    #[cfg(windows)]
    { windows::install(&app) }
    #[cfg(not(windows))]
    { let _ = app; Err("此平台暂未实现 Edge 本机通信组件".into()) }
}

pub fn install_on_start(app: &tauri::AppHandle) {
    #[cfg(windows)]
    if let Err(error) = windows::install(app) {
        eprintln!("Aurum Edge bridge setup: {error}");
    }
    #[cfg(not(windows))]
    let _ = app;
}
