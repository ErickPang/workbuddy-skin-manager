# WorkBuddy Skin Manager

面向非技术用户的一体化 WorkBuddy 主题工具。选择一张本地图片，即可在同一个桌面应用中生成、管理、应用和守护主题。

## 用户流程

1. 打开 WorkBuddy Skin Manager。
2. 点击“从图片创建”，选择 PNG、JPEG 或 WebP 背景图。
3. 应用会在本机分析图片配色、保存主题并自动“应用并重启”。
4. Manager 留在菜单栏，WorkBuddy 重载界面时会自动重新应用当前主题。
5. 需要卸载时点击“恢复官方外观”，或从菜单栏选择“完全退出”。

图片不会上传；创建时仍执行主题库的格式、尺寸和安全校验。单张图片限制为 8 MB、最长边 8192 px、总像素不超过 4000 万。

关闭主窗口只会隐藏 Manager，不会结束主题守护。完全退出会停止守护；如果 WorkBuddy 正在运行，Manager 会先恢复普通启动模式。

## 当前兼容范围

- macOS（Intel / Apple Silicon）
- Windows 10 / 11 x64
- WorkBuddy 兼容范围由主题包 manifest 声明
- Manager 当前已验证 WorkBuddy `5.2.x`、`5.3.x`

Manager 会在 macOS 的 `/Applications/WorkBuddy.app` 和 `~/Applications/WorkBuddy.app` 检测 WorkBuddy。Windows 会依次检查正在运行的 WorkBuddy、卸载注册表和常见安装目录。自定义安装目录未被识别时，可以将 `WORKBUDDY_PATH` 环境变量设置为 WorkBuddy 应用或 `WorkBuddy.exe` 的完整路径。

在 Apple Silicon Mac 上，如果安装的 WorkBuddy 仍是 `x86_64` 版本，系统需要已安装 Rosetta 2；Manager 的 Universal 构建不能替代 WorkBuddy 自身的架构支持。

主题不是修改 WorkBuddy 安装包，而是在本机通过 CDP 注入运行时样式。Manager 每次随机分配仅监听 `127.0.0.1` 的端口，并校验端口对应的 WorkBuddy 进程；WorkBuddy 大版本升级后需要重新验证组件选择器和兼容范围。

## 安全模型

主题由应用从图片生成，只保存经过校验的 JSON 配置和本地图片；不接受脚本、CSS、可执行文件或远程资源。

## 开发

```bash
npm install
npm run tauri dev
```

验证：

```bash
npm run build
npm test
cd src-tauri && cargo test
```

涉及真实 WorkBuddy 进程的发布前检查见 [docs/e2e-checklist.md](docs/e2e-checklist.md)。

生成 macOS 安装包（在 macOS 执行）：

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri build -- --target universal-apple-darwin --bundles dmg
```

生成 Windows 安装包（在 Windows 执行）：

```powershell
npm run tauri build -- --bundles nsis
```

## 一体化范围

- 从图片提取本地配色并生成主题。
- 校验、安装、应用、守护与恢复主题。
- 主题库、运行时应用、守护与恢复全部内置。

普通用户只需要安装本项目。
