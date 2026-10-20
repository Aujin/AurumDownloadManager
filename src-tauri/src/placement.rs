use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, fs, path::{Path, PathBuf}, sync::{Arc, Mutex}};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub save_dir: String,
    pub temporary_dir: String,
    pub moved: bool,
    #[serde(default)]
    pub final_paths: Vec<String>,
    #[serde(default)]
    pub error: String,
    #[serde(skip)]
    pub moving: bool,
    #[serde(default)]
    pub connections: u8,
    #[serde(default)]
    pub probe: Option<crate::probe::ProbeResult>,
}

pub type Placements = Arc<Mutex<HashMap<String, Placement>>>;

pub fn load(path: &Path) -> Placements {
    let data = fs::read(path).ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    Arc::new(Mutex::new(data))
}

pub fn persist(path: &Path, placements: &HashMap<String, Placement>) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(placements).map_err(|e| e.to_string())?;
    fs::write(path, data).map_err(|e| e.to_string())
}

pub fn decorate_and_finalize(task: &mut Value, placements: &Placements, path: &Path) {
    let Some(gid) = task.get("gid").and_then(Value::as_str).map(str::to_owned) else { return };
    let completed = task.get("status").and_then(Value::as_str) == Some("complete");
    let mut guard = match placements.lock() { Ok(guard) => guard, Err(_) => return };
    let Some(placement) = guard.get_mut(&gid) else { return };
    if let Some(object) = task.as_object_mut() {
        object.insert("saveDir".into(), Value::String(placement.save_dir.clone()));
        object.insert("temporaryDir".into(), Value::String(placement.temporary_dir.clone()));
        object.insert("moving".into(), Value::Bool(placement.moving));
        object.insert("moveError".into(), Value::String(placement.error.clone()));
        object.insert("finalPaths".into(), serde_json::to_value(&placement.final_paths).unwrap_or(Value::Null));
        if placement.connections > 0 { object.insert("configuredConnections".into(), Value::from(placement.connections)); }
        if let Some(probe) = &placement.probe { object.insert("probe".into(), serde_json::to_value(probe).unwrap_or(Value::Null)); }
    }
    if !completed || placement.moved || placement.moving || !placement.error.is_empty() { return; }
    placement.moving = true;
    let current = placement.clone();
    let files = task.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
    let map = Arc::clone(placements);
    let store_path = path.to_path_buf();
    std::thread::spawn(move || {
        let result = move_completed_files(&current, &files);
        if let Ok(mut guard) = map.lock() {
            if let Some(entry) = guard.get_mut(&gid) {
                entry.moving = false;
                match result {
                    Ok(paths) => { entry.moved = true; entry.final_paths = paths; entry.error.clear(); }
                    Err(error) => { entry.error = error; }
                }
            }
            let _ = persist(&store_path, &guard);
        }
    });
}

fn move_completed_files(placement: &Placement, files: &[Value]) -> Result<Vec<String>, String> {
    let staging = PathBuf::from(&placement.temporary_dir);
    let destination = PathBuf::from(&placement.save_dir);
    fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    let mut paths = Vec::new();
    for file in files {
        if file.get("selected").and_then(Value::as_str) == Some("false") { continue; }
        let Some(source_text) = file.get("path").and_then(Value::as_str) else { continue };
        if source_text.is_empty() { continue; }
        let source = PathBuf::from(source_text);
        let relative = source.strip_prefix(&staging).map_err(|_| "下载文件不在临时目录内，已停止自动搬运".to_string())?;
        let target = destination.join(relative);
        if source.exists() && target.exists() {
            return Err(format!("目标文件已存在，请更改保存路径：{}", target.display()));
        }
        if let Some(parent) = target.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
        if source.exists() {
            if fs::rename(&source, &target).is_err() {
                let copied = fs::copy(&source, &target).map_err(|e| format!("搬运文件失败：{e}"))?;
                let size = fs::metadata(&source).map_err(|e| e.to_string())?.len();
                if copied != size { return Err("跨盘复制未完成，原文件仍保留在临时目录".into()); }
                fs::remove_file(&source).map_err(|e| e.to_string())?;
            }
        } else if !target.exists() {
            return Err(format!("找不到已完成的文件：{}", source.display()));
        }
        paths.push(target.display().to_string());
    }
    if paths.is_empty() { return Err("下载任务没有可搬运的文件".into()); }
    Ok(paths)
}
