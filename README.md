# ADM

ADM 是一个计划面向 Windows、macOS 和 Linux 的桌面下载器。首版目标是统一管理 HTTP(S)、`.torrent` 和磁力链接下载，并提供浏览器插件与可选的剪切板监听。

## 计划中的组成部分

- 桌面端：Tauri 2、React、TypeScript。
- 应用层：Rust，管理下载任务、配置、历史记录和本地通信。
- 下载内核：首版计划集成 aria2，通过本机 JSON-RPC 控制；内核接口独立，便于后续替换。
- 浏览器插件：Chrome、Edge、Firefox，使用 Native Messaging 向桌面端发送下载请求。Safari 单独适配。

## 开发顺序

1. Windows 桌面端：添加链接、种子和磁力任务；暂停、恢复、队列和任务持久化。
2. 剪切板识别与确认，保持可选开启。
3. Chrome/Edge 插件，然后适配 Firefox。
4. macOS、Linux 打包与测试；随后评估 Safari 和移动端。

当前仓库是项目起点，尚未包含可运行的下载器。下载内核的分发方式和许可要求将在打包发布前确认。

## GitHub

本地仓库使用 `main` 分支。GitHub 仓库创建后，可添加远端并推送：

```sh
git remote add origin https://github.com/aujin/ADM.git
git push -u origin main
```
