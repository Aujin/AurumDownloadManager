use serde::Serialize;

pub const EXTENSION_ID: &str = "klblkidgakkmnemkgdhgdpjlhblbhhgb";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStatus {
    supported: bool,
    registered: bool,
    chrome_registered: bool,
    edge_last_seen: Option<u64>,
    chrome_last_seen: Option<u64>,
    extension_id: &'static str,
    extension_path: String,
    message: String,
    additional_extension_ids: Vec<String>,
}

#[cfg(windows)]
mod windows {
    use super::{BridgeStatus, EXTENSION_ID};
    use std::{fs, path::PathBuf, process::Command};
    use tauri::{AppHandle, Manager};
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    const HOST_NAME: &str = "com.aujin.aurumdownloadmanager";
    const EDGE_REGISTRY_KEY: &str =
        r"Software\Microsoft\Edge\NativeMessagingHosts\com.aujin.aurumdownloadmanager";
    const CHROME_REGISTRY_KEY: &str =
        r"Software\Google\Chrome\NativeMessagingHosts\com.aujin.aurumdownloadmanager";
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

    fn valid_extension_id(id: &str) -> bool {
        id.len() == 32 && id.bytes().all(|byte| (b'a'..=b'p').contains(&byte))
    }

    fn allowed_ids(path: &PathBuf) -> Vec<String> {
        let Ok(bytes) = fs::read(path) else { return Vec::new() };
        let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return Vec::new() };
        if manifest.get("name").and_then(|v| v.as_str()) != Some(HOST_NAME) { return Vec::new() }
        manifest.get("allowed_origins").and_then(|v| v.as_array())
            .into_iter().flatten()
            .filter_map(|origin| origin.as_str())
            .filter_map(|origin| origin.strip_prefix("chrome-extension://")?.strip_suffix('/'))
            .filter(|id| valid_extension_id(id))
            .map(str::to_owned)
            .collect()
    }

    pub fn install(app: &AppHandle, extra_id: Option<&str>) -> Result<BridgeStatus, String> {
        if let Some(id) = extra_id {
            if !valid_extension_id(id) { return Err("插件 ID 必须是 Edge 扩展页显示的 32 位 a–p 字符".into()); }
        }
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
        let mut ids = allowed_ids(&manifest);
        ids.push(EXTENSION_ID.to_owned());
        if let Some(id) = extra_id { ids.push(id.to_owned()); }
        ids.sort();
        ids.dedup();
        let origins: Vec<_> = ids.iter().map(|id| format!("chrome-extension://{id}/")).collect();
        let contents = serde_json::json!({
            "name": HOST_NAME,
            "description": "Aurum Download Manager bridge",
            "path": host.display().to_string(),
            "type": "stdio",
            "allowed_origins": origins,
        });
        fs::write(&manifest, serde_json::to_vec_pretty(&contents).map_err(|e| e.to_string())?)
            .map_err(|e| format!("无法写入 Edge 桥接文件：{e}"))?;
        for registry_key in [EDGE_REGISTRY_KEY, CHROME_REGISTRY_KEY] {
            let key = RegKey::predef(HKEY_CURRENT_USER)
                .create_subkey(registry_key)
                .map_err(|e| format!("无法注册浏览器通信组件：{e}"))?.0;
            key.set_value("", &manifest.display().to_string())
                .map_err(|e| format!("无法注册浏览器通信组件：{e}"))?;
        }
        status(app)
    }

    fn registered(key_name: &str) -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(key_name)
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
            .unwrap_or(false)
    }

    fn last_seen(app: &AppHandle, browser: &str) -> Option<u64> {
        let file = app.path().app_data_dir().ok()?.join(format!("browser-{browser}-seen.json"));
        fs::read(file).ok().and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| value.get("at").and_then(|at| at.as_u64()))
    }

    pub fn status(app: &AppHandle) -> Result<BridgeStatus, String> {
        let extension_path = extension_dir(app)?.display().to_string();
        let additional_extension_ids = allowed_ids(&manifest_path(app)?)
            .into_iter().filter(|id| id != EXTENSION_ID).collect();
        let chrome_registered = registered(CHROME_REGISTRY_KEY);
        let registered = registered(EDGE_REGISTRY_KEY);
        Ok(BridgeStatus {
            supported: true,
            registered,
            chrome_registered,
            edge_last_seen: last_seen(app, "edge"),
            chrome_last_seen: last_seen(app, "chrome"),
            extension_id: EXTENSION_ID,
            extension_path,
            additional_extension_ids,
            message: if registered && chrome_registered { "Edge 和 Chrome 通信组件已注册。" }
                else { "浏览器通信组件未完整注册，请点击“准备浏览器插件”。" }.into(),
        })
    }

    pub fn open_install_page(browser: &str) -> Result<(), String> {
        let (exe, page) = match browser {
            "edge" => ("msedge.exe", "edge://extensions"),
            "chrome" => ("chrome.exe", "chrome://extensions"),
            _ => return Err("不支持的浏览器".into()),
        };
        let app_paths = format!(r"Software\Microsoft\Windows\CurrentVersion\App Paths\{exe}");
        let browser_path = RegKey::predef(HKEY_CURRENT_USER).open_subkey(&app_paths)
            .or_else(|_| RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE).open_subkey(&app_paths))
            .ok().and_then(|key| key.get_value::<String, _>("").ok())
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| ["ProgramFiles", "ProgramFiles(x86)"]
                .iter().filter_map(|name| std::env::var_os(name))
                .map(PathBuf::from)
                .map(|base| if browser == "edge" { base.join("Microsoft/Edge/Application/msedge.exe") }
                     else { base.join("Google/Chrome/Application/chrome.exe") })
                .find(|path| path.is_file()))
            .ok_or_else(|| format!("未找到 {browser}，请先安装浏览器"))?;
        Command::new(&browser_path).arg(page).spawn()
            .map_err(|e| format!("无法打开浏览器扩展页：{e}"))?;
        Ok(())
    }
}

#[tauri::command]
pub fn get_edge_bridge_status(app: tauri::AppHandle) -> Result<BridgeStatus, String> {
    #[cfg(windows)]
    { windows::status(&app) }
    #[cfg(not(windows))]
    {
        let _ = app;
        Ok(BridgeStatus { supported: false, registered: false, chrome_registered: false,
            edge_last_seen: None, chrome_last_seen: None, extension_id: EXTENSION_ID,
            extension_path: String::new(), message: "此平台暂未实现 Edge 本机通信组件".into(), additional_extension_ids: Vec::new() })
    }
}

#[tauri::command]
pub fn repair_edge_bridge(app: tauri::AppHandle) -> Result<BridgeStatus, String> {
    #[cfg(windows)]
    { windows::install(&app, None) }
    #[cfg(not(windows))]
    { let _ = app; Err("此平台暂未实现 Edge 本机通信组件".into()) }
}

#[tauri::command]
pub fn allow_edge_extension(app: tauri::AppHandle, extension_id: String) -> Result<BridgeStatus, String> {
    #[cfg(windows)]
    { windows::install(&app, Some(extension_id.trim())) }
    #[cfg(not(windows))]
    { let _ = (app, extension_id); Err("此平台暂未实现 Edge 本机通信组件".into()) }
}

#[tauri::command]
pub fn open_browser_install_page(browser: String) -> Result<(), String> {
    #[cfg(windows)]
    { windows::open_install_page(&browser) }
    #[cfg(not(windows))]
    { let _ = browser; Err("此平台暂未实现浏览器安装入口".into()) }
}

pub fn install_on_start(app: &tauri::AppHandle) {
    #[cfg(windows)]
    if let Err(error) = windows::install(app, None) {
        eprintln!("Aurum Edge bridge setup: {error}");
    }
    #[cfg(not(windows))]
    let _ = app;
}
