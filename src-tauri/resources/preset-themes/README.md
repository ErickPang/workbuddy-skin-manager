# 预置主题目录

每个预置主题使用一个独立子目录，目录名必须与 `manifest.json` 的 `id` 相同：

```text
preset-themes/
  ocean-mist/
    manifest.json
    theme.json
    assets/background.png
```

主题数据格式沿用 `.wbskin v1` 的 `manifest.json` 和 `theme.json`，但不需要压缩打包。启动应用后，目录中的有效主题会显示在“主题画廊”。用户点击应用时，应用会将该预置复制到本机主题库。
