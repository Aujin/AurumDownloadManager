# Aurum 试用构建

## 文件

- `edge-extension/`：Edge 插件源文件夹；便携包内也包含同名文件夹。
- `aurum-edge-extension.zip`：插件压缩包。部分 Edge 环境允许拖入扩展页安装；稳定更新请使用下述固定目录。
- `windows/aurum-download-manager.exe`：Windows 桌面程序。
- `windows/aurum-native-host.exe`：Edge、Chrome 与桌面程序之间的本机通信组件。
- `windows/aria2c.exe`：随包提供的 aria2 下载内核，桌面程序会优先使用同目录版本。
- `windows/aria2-COPYING.txt`：aria2 的 GPL 许可文本。
- `windows/aria2-source-1.37.0.zip`：同版本的 aria2 对应源码。
- `windows/install-edge-bridge.ps1`：把本机通信组件注册给当前 Windows 用户。
- `aurum-windows-portable.zip`：Windows 程序、通信组件、aria2 和 Edge 插件的完整便携包。

## Edge 试用步骤

以下步骤以解压 `aurum-windows-portable.zip` 后的目录为准。若直接使用 `output/`，程序及注册脚本位于 `output/windows/`，插件位于 `output/edge-extension/`。

1. 直接启动 `aurum-download-manager.exe`。请把 `aria2c.exe` 与主程序保留在同一文件夹，无需单独安装 aria2。Windows 仍需要系统的 WebView2 运行环境。
   如需开机启动，在程序的“设置 → 常规”中点击“启用开机启动”。确认 Windows 管理员权限后，程序会把便携版文件复制到 `Program Files\Aurum Download Manager`，并为当前用户设置登录启动；下次登录后直接在托盘运行，不弹主窗口。若已经使用旧版开机启动，请从新版便携包运行程序，点击“更新开机启动副本”以复制新版文件并改为静默启动。点击“取消开机启动”只移除启动项，不删除 Program Files 中的文件。
   点击窗口右上角的关闭按钮会将程序隐藏到系统托盘，下载继续运行。在托盘图标上右击可选择“显示主窗口”或“退出 Aurum”；也可左击图标恢复窗口。若需桌面入口，可在“设置 → 常规”点击“创建桌面快捷方式”：已启用开机启动时指向 Program Files 副本，否则指向当前便携版 EXE。
   主界面可以选择新任务的 HTTP(S) 分段连接数；下载中的任务可直接在任务卡片修改。任务卡片还会显示实际连接数、服务器的分段续传支持情况和单连接限速的抽样提示。小文件或不支持 Range 的服务器可能无法使用多条连接。
2. 新版程序会自动把插件复制到固定目录，并注册当前用户的 Edge 和 Chrome 通信组件。在“设置 → 浏览器”可以一键准备通信组件并打开对应浏览器的扩展页，查看固定目录和注册状态。打开插件弹窗后，返回设置页点击“检查连接”会显示最近连接时间。
3. 首次安装时，在 `edge://extensions` 或 `chrome://extensions` 开启“开发人员模式”，点击“加载解压缩的扩展”，选择 Aurum 设置页显示的固定插件目录。插件 ID 固定为 `klblkidgakkmnemkgdhgdpjlhblbhhgb`，版本应为 0.1.2。若之前已安装 0.1.0，可在设置页输入旧插件卡片上的 ID 并点击“授权旧插件”以恢复通信；以后如需改用固定目录，先移除旧的同名插件，避免重复。你也可以在支持 ZIP 拖入安装的 Edge 环境使用插件 ZIP。
4. 以后更新便携版并启动新版 Aurum，它会覆盖固定目录中的插件文件。在原插件卡片上点击“重新加载”，无需再添加一个同名插件。然后在插件弹窗选择“每次询问”、“交给 Aurum”或“使用浏览器自身下载”。右键链接也可以直接选择“使用 Aurum 下载链接”。

插件使用 Native Messaging，程序自动写入当前用户的 Edge 和 Chrome `NativeMessagingHosts` 注册项；也可手动运行 `install-edge-bridge.ps1` 修复。启用开机启动时，桥接配置会指向 Program Files 中的副本。若手动移动便携版文件夹且没有启用开机启动，请重新运行新版程序或点击“修复通信”。下载器关闭时，插件发送的链接会排队，待下载器再次启动后创建任务。

这是开发试用构建，尚未发布到 Edge 商店。小文件下载可能在“每次询问”暂停前就已完成；如果不希望插件处理下载，请在插件弹窗选择“使用 Edge 自身下载”。

本包的 aria2 版本为 1.37.0，来自 [aria2 官方项目](https://github.com/aria2/aria2/releases/tag/release-1.37.0)，按 GPL-2.0-or-later 单独分发；对应源码和许可文件已随包提供。
