# WorkBuddy Skin Manager

面向非技术用户的 WorkBuddy 主题安装器。它导入由 WorkBuddy Skin Studio 生成的 `.wbskin` 数据包，管理本地主题，并负责以 CDP 模式重启 WorkBuddy、应用主题和验证组件是否生效。

## 用户流程

1. 打开 WorkBuddy Skin Manager。
2. 点击“导入主题”，选择 `.wbskin` 文件。
3. 预览主题后点击“应用并重启”。
4. Manager 留在菜单栏，WorkBuddy 重载界面时会自动重新应用当前主题。
5. 需要卸载时点击“恢复官方外观”，或从菜单栏选择“完全退出”。

关闭主窗口只会隐藏 Manager，不会结束主题守护。完全退出会停止守护；如果 WorkBuddy 正在运行，Manager 会先恢复普通启动模式。

## 当前兼容范围

- macOS（Intel / Apple Silicon）
- Windows 10 / 11 x64
- WorkBuddy 兼容范围由主题包 manifest 声明
- `.wbskin` schema v1

Manager 会在 macOS 的 `/Applications/WorkBuddy.app` 检测 WorkBuddy。Windows 会依次检查正在运行的 WorkBuddy、卸载注册表和常见安装目录。自定义安装目录未被识别时，可以将 `WORKBUDDY_PATH` 环境变量设置为 WorkBuddy 应用或 `WorkBuddy.exe` 的完整路径。

主题不是修改 WorkBuddy 安装包，而是在本机通过 CDP 注入运行时样式。Manager 每次随机分配仅监听 `127.0.0.1` 的端口，并校验端口对应的 WorkBuddy 进程；WorkBuddy 大版本升级后需要重新验证组件选择器和兼容范围。

## 安全模型

`.wbskin` 是数据包，不允许包含 JavaScript、CSS、可执行文件、远程资源、符号链接或越界路径。Manager 会限制文件数量、压缩包体积和解压体积，并在安装前校验 manifest、颜色和图片路径。

格式说明见 [docs/wbskin-v1.md](docs/wbskin-v1.md)，Hello Kitty 测试包见 [examples/Hello-Kitty.wbskin](examples/Hello-Kitty.wbskin)。

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

## 项目分工

- WorkBuddy Skin Studio：编辑、预览并导出 `.wbskin`。
- WorkBuddy Skin Manager：导入、校验、安装、应用、守护与恢复。

这种拆分让售卖的主题只包含配置和图片，安装能力统一由 Manager 维护。
