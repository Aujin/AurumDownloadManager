use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupStatus {
    supported: bool,
    pub(crate) enabled: bool,
    installed_path: String,
    warning: Option<String>,
    legacy_command: bool,
    installed_outdated: bool,
}

#[cfg(windows)]
mod windows {
    use super::StartupStatus;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use std::{
        env, fs,
        path::{Path, PathBuf},
        process::Command,
        sync::Mutex,
    };
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "AurumDownloadManager";
    const NATIVE_KEY: &str =
        r"Software\Microsoft\Edge\NativeMessagingHosts\com.aujin.aurumdownloadmanager";
    const HELPER_ARG: &str = "--install-startup-files";
    const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
    static AUTO_UPDATE_WARNING: Mutex<Option<String>> = Mutex::new(None);
    const REQUIRED_FILES: &[&str] = &["aurum-download-manager.exe", "aria2c.exe", "aurum-native-host.exe"];
    const OPTIONAL_FILES: &[&str] = &[
        "aria2-COPYING.txt",
        "aria2-source-1.37.0.zip",
        "install-edge-bridge.ps1",
        "README.md",
    ];
    const EDGE_EXTENSION_FILES: &[&str] = &["manifest.json", "background.js", "popup.html", "popup.js", "icon128.png"];

    fn install_root() -> Result<PathBuf, String> {
        let program_files = env::var_os("ProgramFiles").ok_or("无法找到 Program Files 目录")?;
        Ok(PathBuf::from(program_files).join("Aurum Download Manager"))
    }

    fn install_dir() -> Result<PathBuf, String> {
        Ok(install_root()?.join("versions").join(CURRENT_VERSION))
    }

    fn installed_exe() -> Result<PathBuf, String> {
        Ok(install_dir()?.join("aurum-download-manager.exe"))
    }

    fn startup_command(exe: &Path) -> String {
        format!("\"{}\" --startup-hidden", exe.display())
    }

    fn parse_version(version: &str) -> Option<[u32; 3]> {
        let values: Vec<_> = version.split('.').map(str::parse::<u32>).collect::<Result<_, _>>().ok()?;
        values.try_into().ok()
    }

    fn active_exe() -> Result<Option<(PathBuf, bool)>, String> {
        let value = run_key()?.get_value::<String, _>(VALUE_NAME).unwrap_or_default();
        Ok(parse_active_command(&value, &install_root()?))
    }

    fn parse_active_command(value: &str, root: &Path) -> Option<(PathBuf, bool)> {
        let Some(rest) = value.strip_prefix('"') else { return None };
        let Some((path, args)) = rest.split_once('"') else { return None };
        if args != " --startup-hidden" && !args.is_empty() { return None; }
        let exe = PathBuf::from(path);
        let legacy = root.join("aurum-download-manager.exe");
        let versioned = exe.file_name().and_then(|name| name.to_str()) == Some("aurum-download-manager.exe")
            && exe.parent().and_then(Path::parent) == Some(root.join("versions").as_path())
            && exe.parent().and_then(Path::file_name).and_then(|name| name.to_str())
                .and_then(parse_version).is_some();
        if exe != legacy && !versioned { return None; }
        Some((exe, args.is_empty()))
    }

    fn installed_version(exe: &Path) -> Option<[u32; 3]> {
        if !exe.is_file() { return Some([0, 0, 0]); }
        if exe.parent().and_then(Path::parent) == Some(install_root().ok()?.join("versions").as_path()) {
            return exe.parent()?.file_name()?.to_str().and_then(parse_version);
        }
        let script = "[Diagnostics.FileVersionInfo]::GetVersionInfo($env:ADM_VERSION_TARGET).ProductVersion";
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("ADM_VERSION_TARGET", exe);
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        let output = command.output().ok()?;
        if !output.status.success() { return None; }
        parse_version(String::from_utf8_lossy(&output.stdout).trim())
    }

    fn run_key() -> Result<RegKey, String> {
        let current_user = RegKey::predef(HKEY_CURRENT_USER);
        current_user
            .create_subkey(RUN_KEY)
            .map(|(key, _)| key)
            .map_err(|e| format!("无法打开开机启动设置：{e}"))
    }

    pub fn status() -> Result<StartupStatus, String> {
        let active = active_exe()?;
        let installed_outdated = active.as_ref()
            .and_then(|(exe, _)| installed_version(exe))
            .zip(parse_version(CURRENT_VERSION))
            .map(|(installed, current)| installed < current)
            .unwrap_or(false);
        let warning = AUTO_UPDATE_WARNING.lock().ok().and_then(|value| value.clone());
        Ok(StartupStatus {
            supported: true,
            enabled: active.is_some(),
            installed_path: active.as_ref().map(|(exe, _)| exe.clone()).unwrap_or(installed_exe()?).display().to_string(),
            warning,
            legacy_command: active.as_ref().map(|(_, legacy)| *legacy).unwrap_or(false),
            installed_outdated,
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
        if source == destination || (installed_exe()?.is_file() && destination.join("aria2c.exe").is_file()
            && destination.join("aurum-native-host.exe").is_file()) {
            for name in REQUIRED_FILES {
                if !destination.join(name).is_file() {
                    return Err(format!("Program Files 中缺少 {name}，请从完整便携包重新启用。"));
                }
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
            if let Some((active, _)) = active_exe()? {
                if installed_version(&active).zip(parse_version(CURRENT_VERSION))
                    .map(|(installed, current)| installed > current).unwrap_or(false) {
                    return Err("开机启动副本比当前运行的便携版更新，请使用更新的便携版。".into());
                }
            }
            install_files()?;
            warning = migrate_edge_bridge().err();
            run_key()?
                .set_value(VALUE_NAME, &startup_command(&installed_exe()?))
                .map_err(|e| format!("无法写入开机启动设置：{e}"))?;
            if let Err(error) = update_existing_shortcut(&installed_exe()?) { warning = Some(error); }
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
        if let Ok(mut value) = AUTO_UPDATE_WARNING.lock() { *value = status.warning.clone(); }
        Ok(status)
    }

    pub fn auto_update_if_newer() -> Result<(), String> {
        if status()?.installed_outdated {
            if let Err(error) = set_enabled(true) {
                if let Ok(mut value) = AUTO_UPDATE_WARNING.lock() { *value = Some(error.clone()); }
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn startup_host_path() -> Option<PathBuf> {
        active_exe().ok().flatten().map(|(exe, _)| exe.with_file_name("aurum-native-host.exe"))
            .filter(|path| path.is_file())
    }

    fn update_existing_shortcut(target: &Path) -> Result<(), String> {
        let script = "$ErrorActionPreference = 'Stop'; $desktop = [Environment]::GetFolderPath('DesktopDirectory'); if (-not $desktop) { exit 0 }; $link = Join-Path $desktop 'Aurum Download Manager.lnk'; if (-not (Test-Path -LiteralPath $link)) { exit 0 }; $shell = New-Object -ComObject WScript.Shell; $shortcut = $shell.CreateShortcut($link); $old = [IO.Path]::GetFullPath($shortcut.TargetPath); if ([IO.Path]::GetFileName($old) -ieq 'aurum-download-manager.exe') { $shortcut.TargetPath = $env:ADM_SHORTCUT_TARGET; $shortcut.WorkingDirectory = Split-Path -Parent $env:ADM_SHORTCUT_TARGET; $shortcut.IconLocation = $env:ADM_SHORTCUT_TARGET + ',0'; $shortcut.Save() }";
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("ADM_SHORTCUT_TARGET", target);
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        let result = command.output().map_err(|e| format!("无法更新桌面快捷方式：{e}"))?;
        if !result.status.success() {
            return Err(format!("无法更新桌面快捷方式：{}", String::from_utf8_lossy(&result.stderr).trim()));
        }
        Ok(())
    }

    pub fn create_desktop_shortcut() -> Result<String, String> {
        let target = if let Some((exe, _)) = active_exe()? {
            exe
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

    #[cfg(test)]
    mod tests {
        use super::{copy_portable_files, parse_active_command, parse_version};
        use std::{fs, path::Path, time::{SystemTime, UNIX_EPOCH}};

        #[test]
        fn compares_versions_numerically() {
            assert!(parse_version("0.1.10") > parse_version("0.1.9"));
            assert_eq!(parse_version("0.1.1"), Some([0, 1, 1]));
            assert_eq!(parse_version("0.1.1-preview"), None);
        }

        #[test]
        fn accepts_legacy_and_versioned_startup_commands() {
            let root = Path::new(r"C:\Program Files\Aurum Download Manager");
            let legacy = r#""C:\Program Files\Aurum Download Manager\aurum-download-manager.exe""#;
            let current = r#""C:\Program Files\Aurum Download Manager\versions\0.1.1\aurum-download-manager.exe" --startup-hidden"#;
            assert!(parse_active_command(legacy, root).unwrap().1);
            assert!(!parse_active_command(current, root).unwrap().1);
            assert!(parse_active_command(r#""C:\Other\aurum-download-manager.exe" --startup-hidden"#, root).is_none());
        }

        #[test]
        fn stages_complete_portable_version() {
            let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let root = std::env::temp_dir().join(format!("aurum-startup-copy-test-{}-{nonce}", std::process::id()));
            let source = root.join("source");
            let destination = root.join("versions").join("0.1.1");
            fs::create_dir_all(source.join("edge-extension")).unwrap();
            for name in ["aurum-download-manager.exe", "aria2c.exe", "aurum-native-host.exe"] {
                fs::write(source.join(name), name).unwrap();
            }
            fs::write(source.join("edge-extension/manifest.json"), "{}").unwrap();
            let result = copy_portable_files(&source, &destination);
            assert!(result.is_ok(), "{result:?}");
            for name in ["aurum-download-manager.exe", "aria2c.exe", "aurum-native-host.exe", "edge-extension/manifest.json"] {
                assert_eq!(fs::read(source.join(name)).unwrap(), fs::read(destination.join(name)).unwrap());
            }
            fs::remove_dir_all(root).unwrap();
        }
    }
}

#[cfg(windows)]
pub use windows::run_copy_helper;

#[cfg(windows)]
pub use windows::{auto_update_if_newer, startup_host_path};

#[cfg(not(windows))]
pub fn auto_update_if_newer() -> Result<(), String> { Ok(()) }

#[cfg(not(windows))]
pub fn startup_host_path() -> Option<std::path::PathBuf> { None }

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
            legacy_command: false,
            installed_outdated: false,
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
