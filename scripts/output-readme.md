# Aurum 试用构建

## 文件

- `edge-extension/`：可直接由 Edge“加载解压缩的扩展”的插件文件夹。
- `aurum-edge-extension.zip`：插件压缩包。加载前先解压，不要直接选择 ZIP。
- `windows/aurum-download-manager.exe`：Windows 桌面程序。
- `windows/aurum-native-host.exe`：Edge 与桌面程序之间的本机通信组件。
- `windows/install-edge-bridge.ps1`：把本机通信组件注册给当前 Windows 用户。
- `aurum-windows-portable.zip`：上述 Windows 文件的压缩包。

## Edge 试用步骤

1. 本机安装 aria2，并确认终端中 `aria2c --version` 可以运行。启动 `windows/aurum-download-manager.exe`。
2. 在 Edge 打开 `edge://extensions`，开启“开发人员模式”，点击“加载解压缩的扩展”，选择本目录下的 `edge-extension` 文件夹。
3. 在插件卡片上复制 32 位 ID。在 PowerShell 中执行：

   ```powershell
   powershell -ExecutionPolicy Bypass -File .\windows\install-edge-bridge.ps1 -ExtensionId 这里填写插件ID
   ```

4. 重新加载插件。在工具栏的 Aurum 图标中选择“每次询问”、“交给 Aurum”或“使用 Edge 自身下载”。右键链接也可以直接选择“使用 Aurum 下载链接”。

插件使用 Edge Native Messaging，注册脚本只写入当前用户的 `HKCU\Software\Microsoft\Edge\NativeMessagingHosts` 项。若以后移动 `windows` 文件夹，请重新运行注册脚本。下载器关闭时，插件发送的链接会排队，待下载器再次启动后创建任务。

这是开发试用构建，尚未发布到 Edge 商店。小文件下载可能在“每次询问”暂停前就已完成；如果不希望插件处理下载，请在插件弹窗选择“使用 Edge 自身下载”。
