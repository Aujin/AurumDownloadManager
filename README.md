# Aurum Download Manager

面向 Windows、macOS 和 Linux 的桌面下载器。当前开发版已实现 HTTP(S)、磁力链接与 `.torrent` 任务的添加、进度查看、暂停、继续、移除，以及可选的剪切板链接识别。

## 技术结构

- **桌面界面**：Tauri 2 + React + TypeScript。
- **任务控制**：Rust 启动并管理本机 aria2 进程，通过仅监听 `127.0.0.1` 且带随机密钥的 JSON-RPC 通信。
- **任务恢复**：aria2 会话定期写入应用数据目录，正常退出时再次保存。
- **文件位置**：新任务先写入可设置的半成品目录，完成后搬运到保存目录。默认半成品目录是系统“下载”目录下的 `.aurum-partials`，可在“设置 → 保存位置”更改。

## 设置与任务调整

侧栏的“设置”提供常规、保存位置、连接、代理和浏览器页面。可以设置剪切板监听、默认保存目录、半成品目录、默认连接数、同时下载数以及新任务的 HTTP 代理。按文件后缀分类默认关闭，只有主动勾选后才影响新建的链接任务。

主界面可以为新 HTTP(S) 任务选择 1–32 条分段连接；任务卡片也能在下载中、排队中或暂停时直接修改，并显示当前实际连接数。aria2 将不同区间写入同一个文件，分段门槛设为 1 MiB；小文件或不支持 Range 的服务器仍可能只使用一条连接。添加任务后，程序会在后台请求一小段内容判断服务器能否按区间续传。对至少 2 MiB 的文件，还会抽样比较单连接与双连接速度；“疑似单连接限速”只是这次抽样的提示，不能保证所有时段的速度。

进行中或排队中的任务可点击“调整”，即时修改连接数、HTTP 代理以及下载完成后的保存目录。连接数和代理交由 aria2 动态更新，可能短暂重建网络连接。修改保存目录不会搬移仍在传输的半成品；任务完成后才从该任务的临时目录搬运。搬运失败时会显示原因，可更改目标目录并重试。新的全局半成品目录仅影响之后创建的任务。

Edge 和 Chrome 插件试用版共用 `browser-extension/edge/`，可在插件弹窗里选择每次询问、交给 Aurum 或使用浏览器下载。右键链接也可直接交给 Aurum。插件通过 Native Messaging 把链接送到桌面程序；目前尚未完成商店发布。

Windows 版启动时会自动注册 Edge 和 Chrome Native Messaging，并将插件文件同步到固定的用户目录。插件 0.1.2 使用固定 ID；从旧版迁移可在“设置 → 浏览器”授权旧插件 ID，或从固定目录加载新版并移除重复项。后续更新只需在扩展页重新加载原插件。设置页可打开浏览器扩展页并检查最近连接时间。

Windows 便携版把 aria2 1.37.0、许可文本和对应源码一同放在 `output/windows/`。桌面程序优先使用同目录的 `aria2c.exe`，无需单独安装。macOS、Linux 的可分发包仍待制作。

关闭主窗口时程序会留在系统托盘继续下载，通过托盘菜单退出。在 Windows 的“设置 → 常规”中可创建桌面快捷方式；若已启用开机启动，快捷方式指向 Program Files 中的副本，否则指向当前便携版 EXE。

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

## 生成试用文件

在 Windows PowerShell 中从项目根目录运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-output.ps1
```

结果会生成在本地的 `output/`：`edge-extension/` 是可以直接在 Edge 的扩展管理页加载的文件夹，`windows/` 包含桌面程序、本机通信组件、aria2 和 `edge-extension/` 插件文件夹。`aurum-windows-portable.zip` 包含全部 Windows 试用文件及插件；另有单独的插件 ZIP。具体加载和注册步骤见生成的 `output/README.md`。插件无需额外编译，构建脚本会把源文件复制到输出目录。

开发环境若没有安装 aria2，界面会显示下载内核未连接；便携版使用随包提供的 aria2。

## 接下来

1. 完善种子内文件选择、限速和下载队列。
2. 完成 Edge、Chrome 插件的实际侧载验收，并适配 Firefox。
3. 继续验证公网下载和 macOS、Linux 可分发包。
4. 单独适配 Safari，并评估移动端后台下载限制。

## 仓库

GitHub：<https://github.com/Aujin/AurumDownloadManager>。本地默认分支为 `main`。
