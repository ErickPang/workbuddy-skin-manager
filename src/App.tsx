import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowClockwise,
  CheckCircle,
  Desktop,
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
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

interface Compatibility {
  manager: string;
  workbuddy: string[];
}

interface ThemeManifest {
  id: string;
  name: string;
  author: string;
  description: string;
  compatibility: Compatibility;
}

interface InstalledTheme {
  manifest: ThemeManifest;
  theme: { palette: { background: string; accent: string } };
  previewPath: string | null;
  backgroundPath: string;
}

interface WorkBuddyStatus {
  installed: boolean;
  appPath: string;
  version: string | null;
  managerCompatible: boolean;
  cdpAvailable: boolean;
  cdpPort: number | null;
  activeThemeId: string | null;
  configuredThemeId: string | null;
}

interface Notice {
  tone: "success" | "error" | "info";
  message: string;
}

interface SelectedImage {
  path: string;
  name: string;
}

type View = "gallery" | "create" | "library";

function App() {
  const [status, setStatus] = useState<WorkBuddyStatus | null>(null);
  const [themes, setThemes] = useState<InstalledTheme[]>([]);
  const [presetThemes, setPresetThemes] = useState<InstalledTheme[]>([]);
  const [selectedImage, setSelectedImage] = useState<SelectedImage | null>(null);
  const [themeName, setThemeName] = useState("");
  const [generatedThemeId, setGeneratedThemeId] = useState<string | null>(null);
  const [view, setView] = useState<View>("gallery");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);

  const refresh = useCallback(async () => {
    const [nextStatus, nextThemes, nextPresets] = await Promise.all([
      invoke<WorkBuddyStatus>("get_workbuddy_status"),
      invoke<InstalledTheme[]>("list_themes"),
      invoke<InstalledTheme[]>("list_preset_themes"),
    ]);
    setStatus(nextStatus);
    setThemes(nextThemes);
    setPresetThemes(nextPresets);
  }, []);

  useEffect(() => {
    refresh()
      .catch((error) => setNotice({ tone: "error", message: String(error) }))
      .finally(() => setLoading(false));
  }, [refresh]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<string>("runtime-error", (event) => setNotice({ tone: "error", message: event.payload }))
      .then((dispose) => { unlisten = dispose; })
      .catch((error) => setNotice({ tone: "error", message: String(error) }));
    return () => unlisten?.();
  }, []);

  const activeTheme = useMemo(
    () => themes.find((theme) => theme.manifest.id === status?.activeThemeId) ?? null,
    [status?.activeThemeId, themes],
  );
  const viewCopy = {
    gallery: { kicker: "主题画廊", title: "浏览你的 WorkBuddy 外观", detail: "选择已生成的主题，直接应用到 WorkBuddy。" },
    create: { kicker: "从图片生成", title: "用图片生成你的 WorkBuddy 主题", detail: "本机取色、本机保存、自动应用。图片不会离开设备。" },
    library: { kicker: "我的主题", title: "管理保存在本机的主题", detail: "主题仅保存于本机，可随时应用或删除。" },
  }[view];

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

  async function generateAndApply() {
    if (!selectedImage || !themeName.trim()) {
      setNotice({ tone: "error", message: "请先选择图片并填写主题名称。" });
      return;
    }
    setBusy("generate");
    setNotice({ tone: "info", message: "正在本机提取配色、生成主题并应用到 WorkBuddy。" });
    try {
      const created = await invoke<InstalledTheme>("create_theme_from_image", {
        path: selectedImage.path,
        name: themeName.trim(),
      });
      await invoke("apply_theme", { id: created.manifest.id });
      await refresh();
      setGeneratedThemeId(created.manifest.id);
      setView("gallery");
      setNotice({ tone: "success", message: `${created.manifest.name} 已生成并通过组件验证。` });
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
      await invoke("apply_theme", { id: theme.manifest.id });
      await refresh();
      setNotice({ tone: "success", message: `${theme.manifest.name} 已应用并通过组件验证。` });
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
      const installed = await invoke<InstalledTheme>("install_preset_theme", { id: theme.manifest.id });
      await invoke("apply_theme", { id: installed.manifest.id });
      await refresh();
      setNotice({ tone: "success", message: `${theme.manifest.name} 已安装并应用。` });
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function restoreWorkBuddy() {
    setBusy("restore");
    try {
      await invoke("restore_workbuddy");
      await refresh();
      setNotice({ tone: "success", message: "WorkBuddy 已恢复官方外观。" });
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

  return (
    <Theme accentColor="ruby" grayColor="mauve" radius="large" scaling="95%">
      <div className="app-frame">
        <aside className="app-sidebar">
          <div className="sidebar-brand"><span className="brand-mark"><Heart weight="fill" size={16} /></span><span><strong>WorkBuddy</strong><small>Skin Studio</small></span></div>
          <nav className="sidebar-nav" aria-label="主题工具导航">
            <button className={view === "gallery" ? "is-active" : ""} type="button" onClick={() => setView("gallery")}><Swatches size={18} />主题画廊</button>
            <button className={view === "create" ? "is-active" : ""} type="button" onClick={() => setView("create")}><UploadSimple size={18} />从图片生成</button>
            <button className={view === "library" ? "is-active" : ""} type="button" onClick={() => setView("library")}><Heart size={18} />我的主题</button>
          </nav>
          <div className="sidebar-footer"><span className={`connection-dot ${status?.installed ? "is-ready" : ""}`} /><span><strong>{status?.installed ? "WorkBuddy 已连接" : "等待 WorkBuddy"}</strong><small>{status?.installed ? (status.version ? `WorkBuddy ${status.version}` : "版本未知") : "未检测到安装"}</small></span></div>
        </aside>

        <main className="studio-shell">
          <header className="app-header">
            <span>WorkBuddy Skin Studio</span>
            <div className="header-status"><span className={`connection-dot ${status?.cdpAvailable ? "is-ready" : ""}`} />{status?.cdpAvailable ? `主题运行中 · ${status.cdpPort}` : "本机主题工具"}</div>
          </header>

          <section className="intro-block" aria-labelledby="page-title">
            <p>{viewCopy.kicker}</p>
            <h1 id="page-title">{viewCopy.title}</h1>
            <span>{viewCopy.detail}</span>
          </section>

          {notice && <NoticeBar notice={notice} onClose={() => setNotice(null)} />}

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
            <span>{status?.activeThemeId ? "主题守护已开启，WorkBuddy 重载后会自动恢复。" : "当前未启用自定义主题。"}</span>
          </div>
          <Button className="restore-button" size="3" onClick={restoreWorkBuddy} disabled={busy !== null}>
            {busy === "restore" ? <Spinner /> : <ShieldCheck size={18} />}恢复官方外观
          </Button>
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
                      <Button size="2" onClick={() => view === "gallery" ? installAndApplyPreset(theme) : applyTheme(theme)} disabled={active || busy !== null}>
                        {busy === `apply:${theme.manifest.id}` || busy === `preset:${theme.manifest.id}` ? <Spinner /> : <Play weight="fill" size={15} />}{active ? "已应用" : view === "gallery" ? "安装并应用" : "应用"}
                      </Button>
                      {view === "library" && <button className="delete-button" aria-label={`删除 ${theme.manifest.name}`} type="button" onClick={() => deleteTheme(theme)} disabled={busy !== null || status?.configuredThemeId === theme.manifest.id}><Trash size={17} /></button>}
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
          <span className="runtime-note">恢复官方外观会停止主题守护</span>
        </section>}
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
    ? <div className="empty-themes"><SpinnerGap size={22} /><strong>还没有预置主题</strong><span>将主题目录放入 `src-tauri/resources/preset-themes` 后，重新启动应用即可加载。</span></div>
    : <div className="empty-themes"><SpinnerGap size={22} /><strong>还没有安装主题</strong><span>从一张图片开始，生成的主题会保存在这台设备上。</span><Button variant="soft" onClick={onChoose}><UploadSimple size={16} />选择图片</Button></div>;
}

function LoadingState() {
  return <div className="loading-grid"><span /><span /><span /></div>;
}

export default App;
