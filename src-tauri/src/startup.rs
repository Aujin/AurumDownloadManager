use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupStatus {
    supported: bool,
    enabled: bool,
    installed_path: String,
    warning: Option<String>,
}

#[cfg(windows)]
mod windows {
    use super::StartupStatus;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use std::{
        env, fs,
        path::{Path, PathBuf},
        process::Command,
    };
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "AurumDownloadManager";
    const NATIVE_KEY: &str =
        r"Software\Microsoft\Edge\NativeMessagingHosts\com.aujin.aurumdownloadmanager";
    const HELPER_ARG: &str = "--install-startup-files";
    const REQUIRED_FILES: &[&str] = &["aurum-download-manager.exe", "aria2c.exe"];
    const OPTIONAL_FILES: &[&str] = &[
        "aurum-native-host.exe",
        "aria2-COPYING.txt",
        "aria2-source-1.37.0.zip",
        "install-edge-bridge.ps1",
        "README.md",
    ];
    const EDGE_EXTENSION_FILES: &[&str] = &["manifest.json", "background.js", "popup.html", "popup.js", "icon128.png"];

    fn install_dir() -> Result<PathBuf, String> {
        let program_files = env::var_os("ProgramFiles").ok_or("无法找到 Program Files 目录")?;
        Ok(PathBuf::from(program_files).join("Aurum Download Manager"))
    }

    fn installed_exe() -> Result<PathBuf, String> {
        Ok(install_dir()?.join("aurum-download-manager.exe"))
    }

    fn startup_command() -> Result<String, String> {
        Ok(format!("\"{}\"", installed_exe()?.display()))
    }

    fn run_key() -> Result<RegKey, String> {
        let current_user = RegKey::predef(HKEY_CURRENT_USER);
        current_user
            .create_subkey(RUN_KEY)
            .map(|(key, _)| key)
            .map_err(|e| format!("无法打开开机启动设置：{e}"))
    }

    pub fn status() -> Result<StartupStatus, String> {
        let command = startup_command()?;
        let enabled = run_key()?
            .get_value::<String, _>(VALUE_NAME)
            .map(|value| value == command)
            .unwrap_or(false);
        Ok(StartupStatus {
            supported: true,
            enabled,
            installed_path: installed_exe()?.display().to_string(),
            warning: None,
        })
    }

    fn copy_portable_files(source: &Path, destination: &Path) -> Result<(), String> {
        for name in REQUIRED_FILES {
            if !source.join(name).is_file() {
                return Err(format!(
                    "便携版缺少 {name}，请从完整 ZIP 解压后再启用开机启动"
                ));
            }
        }
        fs::create_dir_all(destination).map_err(|e| format!("无法创建 Program Files 目录：{e}"))?;
        // Copy the executable last, so a partial copy never replaces an otherwise complete version.
        for name in REQUIRED_FILES
            .iter()
            .chain(OPTIONAL_FILES.iter())
            .filter(|name| **name != "aurum-download-manager.exe")
        {
            let from = source.join(name);
            if from.is_file() {
                fs::copy(&from, destination.join(name))
                    .map_err(|e| format!("无法复制 {name}：{e}"))?;
            }
        }
        let extension = source.join("edge-extension");
        if extension.is_dir() {
            let destination_extension = destination.join("edge-extension");
            fs::create_dir_all(&destination_extension)
                .map_err(|e| format!("无法创建 Edge 插件目录：{e}"))?;
            for name in EDGE_EXTENSION_FILES {
                let from = extension.join(name);
                if from.is_file() {
                    fs::copy(&from, destination_extension.join(name))
                        .map_err(|e| format!("无法复制 Edge 插件文件 {name}：{e}"))?;
                }
            }
        }
        fs::copy(
            source.join("aurum-download-manager.exe"),
            destination.join("aurum-download-manager.exe"),
        )
        .map_err(|e| format!("无法复制主程序：{e}"))?;
        Ok(())
    }

    fn install_files() -> Result<(), String> {
        let current_exe = env::current_exe().map_err(|e| e.to_string())?;
        let source = current_exe.parent().ok_or("无法找到便携版目录")?;
        let destination = install_dir()?;
        if source == destination {
            if !destination.join("aria2c.exe").is_file() {
                return Err("Program Files 中缺少 aria2c.exe，请从完整便携包重新启用。".into());
            }
            return Ok(());
        }
        let script = "$ErrorActionPreference = 'Stop'; $p = Start-Process -FilePath $env:ADM_STARTUP_SOURCE_EXE -ArgumentList '--install-startup-files' -Verb RunAs -Wait -PassThru; exit $p.ExitCode";
        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                script,
            ])
            .env("ADM_STARTUP_SOURCE_EXE", &current_exe);
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        let result = command
            .status()
            .map_err(|e| format!("无法请求管理员权限：{e}"))?;
        if !result.success() {
            return Err("未能复制文件到 Program Files。请确认已允许管理员权限，并关闭正在运行的已复制版本。".into());
        }
        if !installed_exe()?.is_file() || !destination.join("aria2c.exe").is_file() {
            return Err("复制未完成，未设置开机启动。".into());
        }
        Ok(())
    }

    fn migrate_edge_bridge() -> Result<(), String> {
        let current_user = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(key) = current_user.open_subkey(NATIVE_KEY) else {
            return Ok(());
        };
        let manifest_path: String = key
            .get_value("")
            .map_err(|e| format!("无法读取 Edge 桥接配置：{e}"))?;
        let mut manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(&manifest_path).map_err(|e| format!("无法读取 Edge 桥接文件：{e}"))?,
        )
        .map_err(|e| format!("Edge 桥接文件无效：{e}"))?;
        let host = install_dir()?.join("aurum-native-host.exe");
        if !host.is_file() {
            return Err("便携版缺少 Edge 桥接程序；请从完整 ZIP 解压后重试。".into());
        }
        manifest["path"] = serde_json::Value::String(host.display().to_string());
        let data_dir = env::var_os("APPDATA").ok_or("无法找到用户数据目录")?;
        let data_dir = PathBuf::from(data_dir).join("com.aujin.aurumdownloadmanager");
        fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
        let new_manifest = data_dir.join("native-host.json");
        fs::write(
            &new_manifest,
            serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("无法保存 Edge 桥接文件：{e}"))?;
        let (key, _) = current_user
            .create_subkey(NATIVE_KEY)
            .map_err(|e| e.to_string())?;
        key.set_value("", &new_manifest.display().to_string())
            .map_err(|e| format!("无法更新 Edge 桥接注册：{e}"))?;
        Ok(())
    }

    pub fn set_enabled(enabled: bool) -> Result<StartupStatus, String> {
        let mut warning = None;
        if enabled {
            install_files()?;
            warning = migrate_edge_bridge().err();
            run_key()?
                .set_value(VALUE_NAME, &startup_command()?)
                .map_err(|e| format!("无法写入开机启动设置：{e}"))?;
        } else {
            let key = run_key()?;
            match key.delete_value(VALUE_NAME) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(format!("无法取消开机启动：{e}")),
            }
        }
        let mut status = status()?;
        status.warning = warning;
        Ok(status)
    }

    pub fn create_desktop_shortcut() -> Result<String, String> {
        let target = if status()?.enabled {
            installed_exe()?
        } else {
            env::current_exe().map_err(|e| format!("无法定位当前程序：{e}"))?
        };
        if !target.is_file() {
            return Err(format!("快捷方式目标不存在：{}", target.display()));
        }
        let script = "$ErrorActionPreference = 'Stop'; $desktop = [Environment]::GetFolderPath('DesktopDirectory'); if (-not $desktop) { throw '无法定位桌面目录' }; $link = Join-Path $desktop 'Aurum Download Manager.lnk'; $shell = New-Object -ComObject WScript.Shell; $shortcut = $shell.CreateShortcut($link); $shortcut.TargetPath = $env:ADM_SHORTCUT_TARGET; $shortcut.WorkingDirectory = Split-Path -Parent $env:ADM_SHORTCUT_TARGET; $shortcut.IconLocation = $env:ADM_SHORTCUT_TARGET + ',0'; $shortcut.Description = 'Aurum Download Manager'; $shortcut.Save(); if (-not (Test-Path -LiteralPath $link)) { throw '快捷方式保存失败' }; [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($link))";
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("ADM_SHORTCUT_TARGET", &target);
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        let result = command.output().map_err(|e| format!("无法创建桌面快捷方式：{e}"))?;
        if !result.status.success() {
            return Err(format!("无法创建桌面快捷方式：{}", String::from_utf8_lossy(&result.stderr).trim()));
        }
        let encoded = String::from_utf8_lossy(&result.stdout);
        let path = STANDARD.decode(encoded.trim()).map_err(|e| e.to_string())?;
        let path = String::from_utf8(path).map_err(|e| e.to_string())?;
        if !Path::new(&path).is_file() {
            return Err("快捷方式创建后未能在桌面找到".into());
        }
        Ok(path)
    }

    pub fn run_copy_helper() -> bool {
        if env::args_os().nth(1).as_deref() != Some(std::ffi::OsStr::new(HELPER_ARG)) {
            return false;
        }
        let result = env::current_exe()
            .map_err(|e| e.to_string())
            .and_then(|exe| {
                let source = exe.parent().ok_or("无法找到便携版目录")?;
                copy_portable_files(source, &install_dir()?)
            });
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
}

#[cfg(windows)]
pub use windows::run_copy_helper;

#[cfg(not(windows))]
pub fn run_copy_helper() -> bool {
    false
}

#[tauri::command]
pub fn get_startup_status() -> Result<StartupStatus, String> {
    #[cfg(windows)]
    {
        windows::status()
    }
    #[cfg(not(windows))]
    {
        Ok(StartupStatus {
            supported: false,
            enabled: false,
            installed_path: String::new(),
            warning: None,
        })
    }
}

#[tauri::command]
pub async fn set_startup_enabled(enabled: bool) -> Result<StartupStatus, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || windows::set_enabled(enabled))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("此平台暂不支持开机启动".into())
    }
}

#[tauri::command]
pub async fn create_desktop_shortcut() -> Result<String, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(windows::create_desktop_shortcut)
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        Err("此平台暂不支持创建桌面快捷方式".into())
    }
}
