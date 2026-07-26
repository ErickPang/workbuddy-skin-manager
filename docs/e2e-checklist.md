# WorkBuddy Skin Manager E2E Checklist

该检查会重启本机 WorkBuddy，只在准备发布的 macOS 或 Windows 测试环境中手动执行。两个平台必须分别完成一次完整检查。

1. 确认测试环境安装的是本次待验证的 WorkBuddy `5.2.x` 或 `5.3.x`，且测试主题 manifest 声明了对应兼容范围，并正常启动。
2. 打开 Manager 的“诊断”页面，确认显示了当前平台真实的 WorkBuddy 安装路径和版本。
3. 启动 Manager，导入 `examples/Hello-Kitty.wbskin`。
4. 确认主题卡片图片通过本地 asset protocol 正常显示。
5. 点击“应用并重启”，确认 WorkBuddy 重启、主题生效且状态显示随机 CDP 端口。
6. 在 WorkBuddy 内切换主要页面，确认 shell、主内容区、侧栏和输入区颜色正常。
7. 重新加载 WorkBuddy 页面，等待最多 60 秒，确认守护任务重新应用主题；重新启动 Manager 时应立即执行首次恢复检查。
8. 关闭 Manager 主窗口，确认应用只隐藏到菜单栏或系统托盘且主题守护仍工作。
9. 点击“恢复官方外观”，确认 WorkBuddy 以普通模式重启、主题状态被清除。
10. 再次应用主题后选择托盘菜单“完全退出”，确认退出前恢复官方外观。
11. 检查 Manager 日志目录中没有新增未处理错误。
12. 导入一个背景图片接近但不超过 8 MB 的 `.wbskin`，确认不仅颜色生效，主内容区背景也真实显示；主题引擎结果中的 `backgroundPresent` 必须为 `true`，`backgroundImage` 不能为 `none` 或原始 Base64 data URL。
