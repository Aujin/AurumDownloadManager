# Aurum Download Manager

面向 Windows、macOS 和 Linux 的桌面下载器。当前开发版已实现 HTTP(S)、磁力链接与 `.torrent` 任务的添加、进度查看、暂停、继续、移除，以及可选的剪切板链接识别。

## 技术结构

- **桌面界面**：Tauri 2 + React + TypeScript。
- **任务控制**：Rust 启动并管理本机 aria2 进程，通过仅监听 `127.0.0.1` 且带随机密钥的 JSON-RPC 通信。
- **任务恢复**：aria2 会话定期写入应用数据目录，正常退出时再次保存。
- **文件位置**：默认保存在系统“下载”目录。

aria2 目前是开发环境依赖，尚未随安装包分发。正式发布前需要确定各平台的打包和 GPL-2.0 许可方案。

## 本地开发

需要 Node.js、Rust MSVC 工具链、Tauri 的系统依赖以及 `aria2c`。安装 aria2 后请确保它位于 `PATH`，也可通过 `ADM_ARIA2_PATH` 指定其完整路径。

```powershell
npm install
npm run tauri dev
```

检查前端和 Rust 代码：

```powershell
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

如果 aria2 尚未安装，界面会显示下载内核未连接，添加任务按钮不可用。

## 接下来

1. 完善种子内文件选择、保存目录设置、限速和下载队列。
2. 为 Chrome、Edge、Firefox 开发浏览器插件，通过 Native Messaging 与桌面端通信。
3. 完成 Windows 安装包与真实下载测试，再验证 macOS、Linux。
4. 单独适配 Safari，并评估移动端后台下载限制。

## 仓库

GitHub：<https://github.com/Aujin/AurumDownloadManager>。本地默认分支为 `main`。
