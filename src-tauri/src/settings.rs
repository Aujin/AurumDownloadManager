use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub download_dir: String,
    pub temporary_dir: String,
    pub organize_by_extension: bool,
    pub clipboard_monitoring: bool,
    pub default_connections: u8,
    pub max_concurrent_downloads: u8,
    pub proxy_url: String,
    pub browser_handoff: String,
}

impl Settings {
    pub fn default_for(download_dir: &Path) -> Self {
        let temporary_dir = download_dir.join(".aurum-partials");
        Self {
            download_dir: download_dir.display().to_string(),
            temporary_dir: temporary_dir.display().to_string(),
            organize_by_extension: false,
            clipboard_monitoring: false,
            default_connections: 8,
            max_concurrent_downloads: 4,
            proxy_url: String::new(),
            browser_handoff: "ask".into(),
        }
    }

    pub fn load(path: &Path, download_dir: &Path) -> Self {
        fs::read(path).ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_else(|| Self::default_for(download_dir))
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(1..=32).contains(&self.default_connections) {
            return Err("默认连接数应为 1–32".into());
        }
        if !(1..=32).contains(&self.max_concurrent_downloads) {
            return Err("同时下载任务数应为 1–32".into());
        }
        if !matches!(self.browser_handoff.as_str(), "ask" | "always" | "browser") {
            return Err("无效的浏览器接管模式".into());
        }
        checked_dir(&self.download_dir)?;
        checked_dir(&self.temporary_dir)?;
        validate_proxy(&self.proxy_url)?;
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, bytes).map_err(|e| e.to_string())
    }
}

pub fn checked_dir(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value.trim());
    if !path.is_absolute() {
        return Err("请选择绝对路径".into());
    }
    fs::create_dir_all(&path).map_err(|e| format!("无法创建目录：{e}"))?;
    if !path.is_dir() {
        return Err("所选路径不是目录".into());
    }
    Ok(path)
}

pub fn validate_proxy(value: &str) -> Result<(), String> {
    if value.trim().is_empty() { return Ok(()); }
    let url = reqwest::Url::parse(value).map_err(|_| "代理地址格式无效")?;
    if url.scheme() != "http" || url.host_str().is_none() || url.port().is_none() {
        return Err("目前仅支持 http://主机:端口 格式的代理".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("当前版本不保存代理账号密码，请使用无认证代理".into());
    }
    Ok(())
}

pub fn organized_dir(settings: &Settings, url: &str) -> PathBuf {
    let base = PathBuf::from(&settings.download_dir);
    if !settings.organize_by_extension { return base; }
    let extension = reqwest::Url::parse(url).ok()
        .and_then(|u| Path::new(u.path()).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase));
    let group = match extension.as_deref() {
        Some("zip" | "rar" | "7z" | "tar" | "gz") => "压缩包",
        Some("pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt") => "文档",
        Some("mp4" | "mkv" | "mov" | "mp3" | "flac" | "wav") => "媒体",
        Some("exe" | "msi" | "dmg" | "deb" | "rpm") => "程序",
        _ => "其他",
    };
    base.join(group)
}
