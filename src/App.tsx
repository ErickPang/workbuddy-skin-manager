import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowClockwise,
  CheckCircle,
  Desktop,
  DownloadSimple,
  FolderOpen,
  Heart,
  Info,
  Play,
  ShieldCheck,
  SpinnerGap,
  Swatches,
  Trash,
  UploadSimple,
  Warning,
} from "@phosphor-icons/react";
import { Button, Spinner, Theme } from "@radix-ui/themes";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import appIcon from "./assets/app-icon.png";
import { DiagnosticsModal, LegalDocumentModal, type LegalDocument } from "./components/InfoModals";
import { getMessages, type View } from "./copy";
import {
  invokeWithOverwriteConfirmation,
  invokeWithRestartConfirmation,
  importThemePackages,
  loadAutostart,
  loadCoreData,
  runMutationAndRefresh,
} from "./services/tauri";
import type {
  BrokenTheme,
  DiagnosticInfo,
  InstalledTheme,
  Notice,
  ThemeLibraryBackup,
  WorkBuddyStatus,
} from "./types";
import { installedThemeIds } from "./themeCatalog";
import "./App.css";

interface SelectedImage {
  path: string;
  name: string;
}

const uiCopy = getMessages();

function App() {
  const [status, setStatus] = useState<WorkBuddyStatus | null>(null);
  const [themes, setThemes] = useState<InstalledTheme[]>([]);
  const [brokenThemes, setBrokenThemes] = useState<BrokenTheme[]>([]);
  const [presetThemes, setPresetThemes] = useState<InstalledTheme[]>([]);
  const [selectedImage, setSelectedImage] = useState<SelectedImage | null>(null);
  const [themeName, setThemeName] = useState("");
  const [generatedThemeId, setGeneratedThemeId] = useState<string | null>(null);
  const [view, setView] = useState<View>("gallery");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [legalDocument, setLegalDocument] = useState<LegalDocument | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticInfo | null>(null);
  const [autostartEnabled, setAutostartEnabled] = useState<boolean | null>(null);

  const refreshStatus = useCallback(async () => {
    setStatus(await invoke<WorkBuddyStatus>("get_workbuddy_status"));
  }, []);

  const refresh = useCallback(async () => {
    const { status: nextStatus, library, presetThemes: nextPresets } = await loadCoreData();
    setStatus(nextStatus);
    setThemes(library.themes);
    setBrokenThemes(library.brokenThemes);
    setPresetThemes(nextPresets);
  }, []);

  const refreshAutostart = useCallback(async () => {
    setAutostartEnabled(await loadAutostart());
  }, []);

  useEffect(() => {
    refresh()
      .catch((error) => setNotice({ tone: "error", message: String(error) }))
      .finally(() => setLoading(false));
  }, [refresh]);

  useEffect(() => {
    refreshAutostart().catch((error) => {
      setAutostartEnabled(null);
      setNotice({ tone: "error", message: String(error) });
    });
  }, [refreshAutostart]);

  useEffect(() => {
    let timer: number | null = null;
    const stop = () => {
      if (timer !== null) window.clearInterval(timer);
      timer = null;
    };
    const start = () => {
      if (timer === null) {
        timer = window.setInterval(() => void refreshStatus().catch(() => {}), 30_000);
      }
    };
    const handleVisibility = () => {
      if (document.visibilityState === "visible") {
        void refreshStatus().catch(() => {});
        start();
      } else {
        stop();
      }
    };
    handleVisibility();
    document.addEventListener("visibilitychange", handleVisibility);
    return () => {
      stop();
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  }, [refreshStatus]);

  useEffect(() => {
    const disposers: Array<() => void> = [];
    Promise.all([
      listen<string>("runtime-error", (event) => setNotice({ tone: "error", message: event.payload })),
      listen("theme-restart-required", () => {
        setNotice({
          tone: "info",
          message: "WorkBuddy 当前以普通模式运行。主题守护不会自动重启它，请在准备好后重新应用主题。",
        });
        void refreshStatus().catch((error) => setNotice({ tone: "error", message: String(error) }));
      }),
    ])
      .then((nextDisposers) => { disposers.push(...nextDisposers); })
      .catch((error) => setNotice({ tone: "error", message: String(error) }));
    return () => disposers.forEach((dispose) => dispose());
  }, [refreshStatus]);

  useEffect(() => {
    if (status?.restartRequired) {
      setNotice((current) => current ?? {
        tone: "info",
        message: "主题正在等待重新应用。Manager 不会在后台擅自重启 WorkBuddy。",
      });
    }
  }, [status?.restartRequired]);

  const activeTheme = useMemo(
    () => themes.find((theme) => theme.manifest.id === status?.activeThemeId) ?? null,
    [status?.activeThemeId, themes],
  );
  const installedIds = useMemo(() => installedThemeIds(themes), [themes]);
  const viewCopy = uiCopy.views[view];

  async function chooseImage() {
    try {
      const path = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "背景图片", extensions: ["png", "jpg", "jpeg", "webp"] }],
      });
      if (!path) return;
      const name = path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") || "自定义主题";
      setSelectedImage({ path, name });
      setThemeName(name);
      setGeneratedThemeId(null);
      setNotice({ tone: "info", message: "图片已选定。确认主题名称后即可生成并应用。" });
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    }
  }

  async function importThemePackage() {
    try {
      const selection = await open({
        multiple: true,
        directory: false,
        filters: [{ name: "WorkBuddy 主题包", extensions: ["wbskin"] }],
      });
      if (!selection) return;
      const paths = Array.isArray(selection) ? selection : [selection];
      setBusy("import");
      setNotice({ tone: "info", message: `正在导入并校验 ${paths.length} 个主题包。` });
      const result = await importThemePackages(paths, (path) =>
        invokeWithOverwriteConfirmation<InstalledTheme>(
          "import_theme_package",
          { path },
          "主题库中已存在相同 ID 的主题。覆盖后原主题将被替换，确认继续吗？",
        ),
      );
      await refresh();
      if (result.failures.length > 0) {
        const first = result.failures[0];
        const fileName = first.path.split(/[\\/]/).pop() ?? first.path;
        setNotice({
          tone: "error",
          message: `导入完成：成功 ${result.imported} 个，取消 ${result.skipped} 个，失败 ${result.failures.length} 个。首个失败：${fileName}（${first.error}）`,
        });
      } else if (result.imported > 0) {
        setNotice({
          tone: "success",
          message: `已导入 ${result.imported} 个主题${result.skipped ? `，取消 ${result.skipped} 个` : ""}，可在“我的主题”中应用。`,
        });
      } else {
        setNotice({ tone: "info", message: "导入操作已取消，现有主题未修改。" });
      }
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function exportThemePackage(theme: InstalledTheme) {
    try {
      const selectedPath = await save({
        defaultPath: `${theme.manifest.id}.wbskin`,
        filters: [{ name: "WorkBuddy 主题包", extensions: ["wbskin"] }],
      });
      if (!selectedPath) return;
      const path = selectedPath.toLowerCase().endsWith(".wbskin") ? selectedPath : `${selectedPath}.wbskin`;
      setBusy(`export:${theme.manifest.id}`);
      setNotice({ tone: "info", message: `正在导出 ${theme.manifest.name}。` });
      await invoke("export_theme_package", { id: theme.manifest.id, path });
      setNotice({ tone: "success", message: `${theme.manifest.name} 已导出。` });
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function exportThemeLibrary() {
    try {
      const directory = await open({ multiple: false, directory: true, title: "选择主题库备份目录" });
      if (!directory) return;
      setBusy("export-library");
      setNotice({ tone: "info", message: "正在逐个导出主题库。" });
      const backup = await invoke<ThemeLibraryBackup>("export_theme_library", { directory });
      setNotice({
        tone: "success",
        message: `已将 ${backup.count} 个主题备份到 ${backup.path}。`,
      });
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function chooseWorkBuddyPath() {
    try {
      const path = await open({
        multiple: false,
        directory: false,
        title: "选择 WorkBuddy 应用",
        filters: [{ name: "WorkBuddy", extensions: ["app", "exe"] }],
      });
      if (!path) return;
      setBusy("workbuddy-path");
      const nextStatus = await invoke<WorkBuddyStatus>("set_workbuddy_path", { path });
      setStatus(nextStatus);
      setNotice({ tone: "success", message: "WorkBuddy 安装位置已保存。" });
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function resetWorkBuddyPath() {
    setBusy("workbuddy-path");
    try {
      const nextStatus = await invoke<WorkBuddyStatus>("set_workbuddy_path", { path: null });
      setStatus(nextStatus);
      setNotice({ tone: "success", message: "已恢复自动检测 WorkBuddy。" });
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function showDiagnostics() {
    try {
      setDiagnostics(await invoke<DiagnosticInfo>("get_diagnostics"));
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    }
  }

  async function toggleAutostart() {
    if (autostartEnabled === null) return;
    const enabled = !autostartEnabled;
    setBusy("autostart");
    try {
      await invoke("set_autostart_enabled", { enabled });
      setAutostartEnabled(enabled);
      setNotice({ tone: "success", message: enabled ? "已启用开机启动，登录后 Manager 会在后台运行。" : "已关闭开机启动。" });
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function generateAndApply() {
    if (!selectedImage || !themeName.trim()) {
      setNotice({ tone: "error", message: "请先选择图片并填写主题名称。" });
      return;
    }
    setBusy("generate");
    setNotice({ tone: "info", message: "正在本机提取配色、生成主题并应用到 WorkBuddy。" });
    try {
      await runMutationAndRefresh(async () => {
        const created = await invoke<InstalledTheme>("create_theme_from_image", {
          path: selectedImage.path,
          name: themeName.trim(),
        });
        setGeneratedThemeId(created.manifest.id);
        const applied = await invokeWithRestartConfirmation<void>(
          "apply_theme",
          { id: created.manifest.id },
          "应用主题需要关闭并重启 WorkBuddy。请先保存正在进行的工作，确认继续吗？",
        );
        if (applied === null) {
          setView("library");
          setNotice({ tone: "info", message: `${created.manifest.name} 已生成并保存，应用操作已取消。` });
          return;
        }
        setView("gallery");
        setNotice({ tone: "success", message: `${created.manifest.name} 已生成并通过组件验证。` });
      }, refresh);
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function applyTheme(theme: InstalledTheme) {
    setBusy(`apply:${theme.manifest.id}`);
    setNotice({ tone: "info", message: `正在应用 ${theme.manifest.name}。` });
    try {
      await runMutationAndRefresh(async () => {
        const applied = await invokeWithRestartConfirmation<void>(
          "apply_theme",
          { id: theme.manifest.id },
          "应用主题需要关闭并重启 WorkBuddy。请先保存正在进行的工作，确认继续吗？",
        );
        if (applied === null) {
          setNotice({ tone: "info", message: "应用操作已取消。" });
          return;
        }
        setNotice({ tone: "success", message: `${theme.manifest.name} 已应用并通过组件验证。` });
      }, refresh);
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function installAndApplyPreset(theme: InstalledTheme) {
    setBusy(`preset:${theme.manifest.id}`);
    setNotice({ tone: "info", message: `正在安装并应用预置主题 ${theme.manifest.name}。` });
    try {
      await runMutationAndRefresh(async () => {
        const installed = await invokeWithOverwriteConfirmation<InstalledTheme>(
          "install_preset_theme",
          { id: theme.manifest.id },
          `主题库中已存在 ID 为“${theme.manifest.id}”的主题。安装预置主题会覆盖它，确认继续吗？`,
        );
        if (installed === null) {
          setNotice({ tone: "info", message: "安装操作已取消，现有主题未修改。" });
          return;
        }
        const applied = await invokeWithRestartConfirmation<void>(
          "apply_theme",
          { id: installed.manifest.id },
          "应用主题需要关闭并重启 WorkBuddy。请先保存正在进行的工作，确认继续吗？",
        );
        if (applied === null) {
          setNotice({ tone: "info", message: `${theme.manifest.name} 已安装，应用操作已取消。` });
          return;
        }
        setNotice({ tone: "success", message: `${theme.manifest.name} 已安装并应用。` });
      }, refresh);
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function restoreWorkBuddy() {
    setBusy("restore");
    try {
      await runMutationAndRefresh(async () => {
        const restored = await invokeWithRestartConfirmation<void>(
          "restore_workbuddy",
          {},
          "恢复官方外观需要关闭并重启 WorkBuddy。请先保存正在进行的工作，确认继续吗？",
        );
        if (restored === null) {
          setNotice({ tone: "info", message: "恢复操作已取消。" });
          return;
        }
        setNotice({ tone: "success", message: "WorkBuddy 已恢复官方外观。" });
      }, refresh);
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function deleteTheme(theme: InstalledTheme) {
    if (!window.confirm(`从本机删除“${theme.manifest.name}”？`)) return;
    setBusy(`delete:${theme.manifest.id}`);
    try {
      await invoke("delete_theme", { id: theme.manifest.id });
      await refresh();
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function deleteBrokenTheme(theme: BrokenTheme) {
    if (!window.confirm(`删除无法读取的主题“${theme.id}”？`)) return;
    setBusy(`delete:${theme.id}`);
    try {
      await invoke("delete_theme", { id: theme.id });
      await refresh();
      setNotice({ tone: "success", message: `无法读取的主题“${theme.id}”已删除。` });
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  return (
    <Theme accentColor="ruby" grayColor="mauve" radius="large" scaling="95%">
      <div className="app-frame">
        <aside className="app-sidebar">
          <div className="sidebar-brand"><img className="brand-mark" src={appIcon} alt="" aria-hidden="true" /><span><strong>WorkBuddy</strong><small>Theme Manager</small></span></div>
          <nav className="sidebar-nav" aria-label="主题工具导航">
            <button className={view === "gallery" ? "is-active" : ""} type="button" onClick={() => setView("gallery")}><Swatches size={18} />主题画廊</button>
            <button className={view === "create" ? "is-active" : ""} type="button" onClick={() => setView("create")}><UploadSimple size={18} />从图片生成</button>
            <button className={view === "library" ? "is-active" : ""} type="button" onClick={() => setView("library")}><Heart size={18} />我的主题</button>
          </nav>
          <div className="sidebar-footer"><span className={`connection-dot ${status?.running && status.managerCompatible ? "is-ready" : ""}`} /><span><strong>{!status?.installed ? "未检测到 WorkBuddy" : !status.managerCompatible ? "版本暂不兼容" : status.cdpAvailable ? "主题运行中" : status.running ? "WorkBuddy 正在运行" : "WorkBuddy 已安装"}</strong><small>{status?.installed ? (status.version ? `WorkBuddy ${status.version}` : "版本未知") : "请检查安装位置"}</small></span></div>
        </aside>

        <main className="studio-shell">
          <header className="app-header">
            <span>WorkBuddy Theme Manager</span>
            <div className="header-actions">
              <div className="header-status"><span className={`connection-dot ${status?.cdpAvailable ? "is-ready" : ""}`} />{status?.cdpAvailable ? `主题运行中 · ${status.cdpPort}` : status?.restartRequired ? "主题等待重新应用" : "本机主题工具"}</div>
              <div className="header-legal">
                <button type="button" onClick={showDiagnostics}><Info size={12} />诊断信息</button>
                <button type="button" onClick={() => setLegalDocument("privacy")}>隐私政策</button>
                <button type="button" onClick={() => setLegalDocument("terms")}>使用条款</button>
              </div>
            </div>
          </header>

          <section className="intro-block" aria-labelledby="page-title">
            <p>{viewCopy.kicker}</p>
            <h1 id="page-title">{viewCopy.title}</h1>
            <span>{viewCopy.detail}</span>
          </section>

          {notice && <NoticeBar notice={notice} onClose={() => setNotice(null)} />}

        {status && !status.installed && <section className="setup-banner" aria-label="WorkBuddy 安装位置设置">
          <div><Warning size={20} weight="fill" /><span><strong>未检测到 WorkBuddy</strong><small>选择应用安装位置后即可生成和应用主题。</small></span></div>
          <Button onClick={chooseWorkBuddyPath} disabled={busy !== null}><FolderOpen size={17} />选择安装位置</Button>
        </section>}

        {view === "create" && <section className="workflow" aria-label="主题生成流程">
          <ol className="steps">
            <Step active={!selectedImage} label="导入图片" index="01" done={Boolean(selectedImage)} />
            <Step active={Boolean(selectedImage) && !generatedThemeId} label="生成主题" index="02" done={Boolean(generatedThemeId)} />
            <Step active={Boolean(generatedThemeId) && status?.activeThemeId !== generatedThemeId} label="应用到 WorkBuddy" index="03" done={generatedThemeId !== null && status?.activeThemeId === generatedThemeId} />
          </ol>

          <div className={`creator ${selectedImage ? "has-image" : ""}`}>
            <div className="image-stage">
              {selectedImage ? (
                <>
                  <img src={convertFileSrc(selectedImage.path)} alt={`已选择的主题背景：${selectedImage.name}`} />
                  <button className="replace-image" type="button" onClick={chooseImage}>更换图片</button>
                </>
              ) : (
                <button className="image-picker" type="button" onClick={chooseImage} disabled={busy !== null}>
                  <span className="picker-icon"><UploadSimple size={26} /></span>
                  <strong>选择一张背景图片</strong>
                  <span>PNG、JPEG 或 WebP · 最大 8 MB</span>
                </button>
              )}
            </div>

            <div className="creator-panel">
              <span className="eyebrow">Theme recipe</span>
              <h2>{selectedImage ? "准备生成" : "从图片开始"}</h2>
              <p>{selectedImage ? "系统会在设备本机分析颜色，自动建立高对比度界面配色。" : "先选择图片。建议使用没有文字和 UI 元素的纯背景素材。"}</p>
              <label className="theme-name">
                <span>主题名称</span>
                <input value={themeName} onChange={(event) => setThemeName(event.target.value)} disabled={!selectedImage || busy !== null} maxLength={80} placeholder="例如：晚樱" />
              </label>
              <Button className="generate-button" size="4" onClick={generateAndApply} disabled={!selectedImage || busy !== null}>
                {busy === "generate" ? <Spinner /> : <Play weight="fill" size={18} />}
                {busy === "generate" ? "正在生成并应用" : "生成主题并应用"}
              </Button>
              <small>不上传图片，不修改 WorkBuddy 安装文件。</small>
            </div>
          </div>
        </section>}

        {view === "library" && <section className="library-overview" aria-label="当前主题管理">
          <div>
            <p>当前使用</p>
            <h2>{activeTheme?.manifest.name ?? "官方外观"}</h2>
            <span>{status?.restartRequired ? "WorkBuddy 需要由你确认重启后才能重新应用主题。" : status?.activeThemeId ? "主题守护已开启，WorkBuddy 重载后会自动恢复。" : "当前未启用自定义主题。"}</span>
          </div>
          <div className="library-actions">
            <Button className="import-button" size="3" onClick={importThemePackage} disabled={busy !== null}>
              {busy === "import" ? <Spinner /> : <UploadSimple size={18} />}批量导入
            </Button>
            <Button className="backup-button" size="3" onClick={exportThemeLibrary} disabled={busy !== null || themes.length === 0}>
              {busy === "export-library" ? <Spinner /> : <DownloadSimple size={18} />}备份主题库
            </Button>
            <Button className="restore-button" size="3" onClick={restoreWorkBuddy} disabled={busy !== null}>
              {busy === "restore" ? <Spinner /> : <ShieldCheck size={18} />}恢复官方外观
            </Button>
          </div>
        </section>}

        {view === "library" && brokenThemes.length > 0 && <section className="broken-themes" aria-labelledby="broken-themes-title">
          <div className="broken-themes-head"><Warning size={18} weight="fill" /><div><h2 id="broken-themes-title">有 {brokenThemes.length} 个主题无法读取</h2><p>文件仍保留在本机。可删除后重新导入原主题包。</p></div></div>
          <div className="broken-theme-list">
            {brokenThemes.map((theme) => <div className="broken-theme-item" key={theme.id}>
              <span><strong>{theme.id}</strong><small>{theme.reason}</small></span>
              <button type="button" aria-label={`删除损坏主题 ${theme.id}`} onClick={() => deleteBrokenTheme(theme)} disabled={busy !== null || status?.configuredThemeId === theme.id}><Trash size={17} /></button>
            </div>)}
          </div>
        </section>}

        {(view === "gallery" || view === "library") && <section className="theme-section" aria-labelledby="themes-title">
          <div className="section-head">
            <div><p>{view === "gallery" ? "浏览并应用" : "已安装"}</p><h2 id="themes-title">{view === "gallery" ? "主题画廊" : "我的主题"}</h2></div>
            <Button variant="ghost" onClick={() => refresh().catch((error) => setNotice({ tone: "error", message: String(error) }))} disabled={busy !== null}>
              <ArrowClockwise size={17} />刷新
            </Button>
          </div>

          {loading ? <LoadingState /> : (view === "gallery" ? presetThemes : themes).length ? (
            <div className="theme-grid">
              {(view === "gallery" ? presetThemes : themes).map((theme) => {
                const active = theme.manifest.id === status?.activeThemeId;
                const presetInstalled = view === "gallery" && installedIds.has(theme.manifest.id);
                const image = convertFileSrc(theme.previewPath ?? theme.backgroundPath);
                return (
                  <article className={`theme-card ${active ? "is-active" : ""}`} key={theme.manifest.id}>
                    <img src={image} alt={`${theme.manifest.name} 主题背景`} />
                    <div className="theme-card-copy">
                      <span>{view === "gallery" ? "预置主题" : active ? "正在使用" : "已安装"}</span>
                      <h3>{theme.manifest.name}</h3>
                      <p>{theme.manifest.description}</p>
                    </div>
                    <div className="theme-card-actions">
                      <Button size="2" onClick={() => view === "gallery" ? installAndApplyPreset(theme) : applyTheme(theme)} disabled={active || presetInstalled || busy !== null}>
                        {busy === `apply:${theme.manifest.id}` || busy === `preset:${theme.manifest.id}` ? <Spinner /> : active || presetInstalled ? <CheckCircle weight="fill" size={15} /> : <Play weight="fill" size={15} />}{active ? "已应用" : presetInstalled ? "已安装" : view === "gallery" ? "安装并应用" : "应用"}
                      </Button>
                      {view === "library" && <div className="theme-secondary-actions">
                        <button className="card-icon-button" aria-label={`导出 ${theme.manifest.name}`} title="导出主题包" type="button" onClick={() => exportThemePackage(theme)} disabled={busy !== null}>{busy === `export:${theme.manifest.id}` ? <Spinner /> : <DownloadSimple size={17} />}</button>
                        <button className="card-icon-button delete-button" aria-label={`删除 ${theme.manifest.name}`} title="删除主题" type="button" onClick={() => deleteTheme(theme)} disabled={busy !== null || status?.configuredThemeId === theme.manifest.id}><Trash size={17} /></button>
                      </div>}
                    </div>
                  </article>
                );
              })}
            </div>
          ) : <EmptyThemes gallery={view === "gallery"} onChoose={() => { setView("create"); void chooseImage(); }} />}
        </section>}

        {view === "library" && <section className="runtime-strip" aria-label="运行状态">
          <div><Swatches size={19} /><span><strong>{activeTheme?.manifest.name ?? "官方外观"}</strong><small>当前外观</small></span></div>
          <div><CheckCircle size={19} /><span><strong>{status?.cdpAvailable ? `127.0.0.1:${status.cdpPort}` : "应用时自动连接"}</strong><small>本机 CDP</small></span></div>
          <div className="runtime-path"><Desktop size={19} /><span><strong title={status?.installed ? status.appPath : undefined}>{status?.installed ? status.appPath : "未检测到安装"}</strong><small>安装位置</small></span></div>
          <div className="path-controls">
            <label className="autostart-toggle" title="登录系统后在后台启动 Manager">
              <input type="checkbox" checked={autostartEnabled ?? false} onChange={toggleAutostart} disabled={busy !== null || autostartEnabled === null} />
              <span aria-hidden="true" /><em>开机启动</em>
            </label>
            <button type="button" onClick={chooseWorkBuddyPath} disabled={busy !== null} title={status?.configuredThemeId ? "重新定位当前主题对应的 WorkBuddy，以便安全恢复" : "选择 WorkBuddy 安装位置"}><FolderOpen size={15} />{status?.configuredThemeId ? "重新定位" : "更改位置"}</button>
            {status?.customPath && <button type="button" onClick={resetWorkBuddyPath} disabled={busy !== null || status.configuredThemeId !== null}>自动检测</button>}
          </div>
        </section>}
        {legalDocument && <LegalDocumentModal document={legalDocument} onClose={() => setLegalDocument(null)} />}
        {diagnostics && <DiagnosticsModal diagnostics={diagnostics} onClose={() => setDiagnostics(null)} onError={(message) => setNotice({ tone: "error", message })} />}
        </main>
      </div>
    </Theme>
  );
}

function Step({ active, done, index, label }: { active: boolean; done: boolean; index: string; label: string }) {
  return <li className={active ? "is-active" : ""}><span>{done ? <CheckCircle weight="fill" size={16} /> : index}</span>{label}</li>;
}

function NoticeBar({ notice, onClose }: { notice: Notice; onClose: () => void }) {
  const Icon = notice.tone === "success" ? CheckCircle : notice.tone === "error" ? Warning : Info;
  return <div className={`notice notice-${notice.tone}`} role={notice.tone === "error" ? "alert" : "status"}><Icon weight="fill" size={18} /><span>{notice.message}</span><button onClick={onClose} type="button">关闭</button></div>;
}

function EmptyThemes({ gallery, onChoose }: { gallery: boolean; onChoose: () => void }) {
  return gallery
    ? <div className="empty-themes"><SpinnerGap size={22} /><strong>{uiCopy.emptyGallery.title}</strong><span>{uiCopy.emptyGallery.detail}</span></div>
    : <div className="empty-themes"><SpinnerGap size={22} /><strong>还没有安装主题</strong><span>从一张图片开始，生成的主题会保存在这台设备上。</span><Button variant="soft" onClick={onChoose}><UploadSimple size={16} />选择图片</Button></div>;
}

function LoadingState() {
  return <div className="loading-grid"><span /><span /><span /></div>;
}

export default App;
