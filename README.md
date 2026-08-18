# WorkBuddy Theme Manager

一张图片，生成、应用并守护你的 WorkBuddy 主题。

Turn one image into a WorkBuddy theme that stays.

WorkBuddy Theme Manager 是面向非技术用户的本地桌面工具。选择一张背景图，应用会在本机提取配色、生成主题并应用；WorkBuddy 页面刷新后，Manager 会自动恢复主题。WorkBuddy 以普通模式重新启动时，Manager 会等待你确认，不会在后台擅自重启它。

它不修改 WorkBuddy 安装包，而是通过本机 CDP 会话注入纯视觉样式，随时可以一键恢复官方外观。主题生成、校验、应用、守护与恢复全部内置在同一个客户端，普通用户只需安装本项目。

法律与隐私信息见 [LICENSE](LICENSE)、[PRIVACY.md](PRIVACY.md) 和 [TERMS.md](TERMS.md)。

## 你会得到什么

- 从图片生成主题：选择本地背景图，本机提取配色并生成高对比度主题。
- 主题画廊：浏览随应用打包的预置主题，一键安装并应用。
- 导入与导出 `.wbskin` 主题包：在“我的主题”中迁移经过校验的主题。
- 我的主题：预览、应用、导出或删除保存在本机的主题。
- 常驻守护：WorkBuddy 页面刷新后自动重新应用；需要重启 WorkBuddy 时先征得确认。
- 版本与位置检测：显示 WorkBuddy 版本和安装路径，并支持选择自定义安装位置。
- 开机启动：登录系统后在后台启动 Manager，不主动弹出主窗口。
- 本机诊断：查看并复制版本、运行状态和日志位置，便于定位问题。
- 一键恢复：恢复官方外观并停止主题守护。

## 用户流程

1. 打开 WorkBuddy Theme Manager，侧栏底部会显示检测到的 WorkBuddy 版本。
2. 进入「从图片生成」，选择 PNG、JPEG 或 WebP 背景图并填写主题名称。
3. 应用在本机分析配色并保存主题；如需重启 WorkBuddy，会先提示保存工作并征得确认。
4. Manager 留在菜单栏或系统托盘，WorkBuddy 重载界面时自动重新应用主题。
5. 需要还原时，在「我的主题」点击「恢复官方外观」，或从菜单栏/托盘选择「完全退出」。

图片不会上传；生成时仍执行格式、尺寸和安全校验。单张图片最大 8 MB、最长边 8192 px、总像素不超过 4000 万。

关闭主窗口只会隐藏 Manager，不会结束主题守护。可在“我的主题”底部启用开机启动；登录系统后 Manager 仅在后台运行。完全退出会停止守护；如果 WorkBuddy 正在运行，Manager 会先确认，再恢复普通启动模式。Manager 只请求 WorkBuddy 正常退出，不会强制终止进程。

## 兼容范围

- macOS（Intel / Apple Silicon）
- Windows 10 / 11 x64
- WorkBuddy `5.2.x`、`5.3.x`

macOS 会检查 `/Applications/WorkBuddy.app` 和 `~/Applications/WorkBuddy.app`，并核对应用 bundle ID；Windows 会优先检查常见安装目录，再检查正在运行的 WorkBuddy 和卸载注册表。自动检测失败时，可在“我的主题”底部选择 WorkBuddy 应用；保存的自定义位置优先于 `WORKBUDDY_PATH` 环境变量和自动检测结果。

在 Apple Silicon Mac 上，如果安装的 WorkBuddy 仍是 `x86_64` 版本，系统需要安装 Rosetta 2；Manager 的 Universal 构建不能替代 WorkBuddy 自身的架构支持。

主题不修改 WorkBuddy 安装包。Manager 每次随机分配仅监听 `127.0.0.1` 的端口，并校验端口对应的 WorkBuddy 进程；WorkBuddy 大版本升级后需要重新验证组件选择器和兼容范围。

## 安全模型

- 主题只保存经过校验的 JSON 配置和本地图片。
- 不接受脚本、CSS、可执行文件或远程资源。
- 取色与图片处理全部在本机完成，不上传。
- 主题状态使用带版本的原子写入、备份与故障恢复；无法识别的新版本配置会保留原文件，应用失败后自动恢复普通启动模式。
- 导出先写入临时文件并完成同步，再替换用户选择的目标文件；不会写入 Manager 主题库。
- 主题库备份先在临时目录生成全部包，全部成功后再一次性发布为独立备份目录。

## 开发

环境要求：Node.js 22、Rust（Tauri 2）、npm。

安装依赖并启动开发环境：

```bash
npm install
npm run tauri dev
```

验证：

```bash
npm run build
npm test
npm run check
(cd src-tauri && cargo fmt --check)
(cd src-tauri && cargo test)
(cd src-tauri && cargo clippy --all-targets -- -D warnings)
```

修改前端或 theme engine 后至少运行 `npm run check`；修改 Rust 后端后至少运行 `cargo fmt --check` 与 `cargo test`，提交前按影响范围运行 Clippy。

涉及真实 WorkBuddy 的发布前检查见 [docs/e2e-checklist.md](docs/e2e-checklist.md)，它会重启本机 WorkBuddy，只在隔离测试环境手动执行。

## 打包

macOS Universal DMG（在 macOS 执行）：

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri build -- --target universal-apple-darwin --bundles dmg
```

Windows NSIS 安装包（在 Windows 执行）：

```powershell
npm run tauri build -- --bundles nsis
```

不要把 `npm run tauri build` 当作日常验证；macOS universal 和 Windows NSIS 由 CI 在对应平台构建。

## 常用命令

| 命令 | 作用 |
| --- | --- |
| `npm run tauri dev` | 启动桌面应用开发环境 |
| `npm run build` | TypeScript 检查并构建 Vite 前端 |
| `npm test` | 运行前端与 theme engine 单元测试 |
| `npm run check` | 前端构建 + 前端与 theme engine 单元测试 |
| `cargo fmt --check` | Rust 格式检查（在 `src-tauri` 内执行） |
| `cargo test` | Rust 单元测试（在 `src-tauri` 内执行） |
| `cargo clippy --all-targets -- -D warnings` | Rust 静态检查（在 `src-tauri` 内执行） |

## 安全边界

- CDP 只使用 `127.0.0.1` 随机端口，连接前确认端口属于检测到的 WorkBuddy 进程。
- 不修改 WorkBuddy 安装包、代码签名或运行文件。
- 本地主题图片只从受控的应用数据目录加载。
- 不提交密钥、Token、用户主题数据、构建产物或日志。

## 项目结构

| 路径 | 职责 |
| --- | --- |
| `src/` | React 19 + TypeScript 前端 |
| `src-tauri/src/` | Rust 后端：命令、菜单栏/托盘、守护、主题库与 WorkBuddy 平台实现 |
| `src-tauri/resources/theme-engine/` | 随应用打包的 CommonJS CDP 引擎 |
| `src-tauri/resources/preset-themes/` | 随应用打包的预置主题目录 |
| `docs/` | `.wbskin` 协议与发布前 E2E 清单 |
