# WorkBuddy Skin Manager E2E Checklist

该检查会重启本机 WorkBuddy，只在准备发布的 macOS 测试环境中手动执行。

1. 确认 WorkBuddy 版本属于 `5.2.x`，并正常启动。
2. 启动 Manager，导入 `examples/Hello-Kitty.wbskin`。
3. 确认主题卡片图片通过本地 asset protocol 正常显示。
4. 点击“应用并重启”，确认 WorkBuddy 重启、主题生效且状态显示随机 CDP 端口。
5. 在 WorkBuddy 内切换主要页面，确认 shell、主内容区、侧栏和输入区颜色正常。
6. 重新加载 WorkBuddy 页面，等待最多 60 秒，确认守护任务重新应用主题。
7. 关闭 Manager 主窗口，确认应用只隐藏到菜单栏且主题守护仍工作。
8. 点击“恢复官方外观”，确认 WorkBuddy 以普通模式重启、主题状态被清除。
9. 再次应用主题后选择菜单栏“完全退出”，确认退出前恢复官方外观。
10. 检查 Manager 日志目录中没有新增未处理错误。
