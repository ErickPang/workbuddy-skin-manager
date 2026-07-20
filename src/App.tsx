import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowClockwise,
  CheckCircle,
  Desktop,
  Gear,
  Heart,
  Info,
  Play,
  Pulse,
  ShieldCheck,
  Swatches,
  Trash,
  UploadSimple,
  Warning,
  Wrench,
} from "@phosphor-icons/react";
import { Badge, Button, Spinner, Theme } from "@radix-ui/themes";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

type View = "library" | "status" | "diagnostics" | "settings";

interface Compatibility {
  manager: string;
  workbuddy: string[];
}

interface ThemeManifest {
  schemaVersion: number;
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  preview?: string;
  compatibility: Compatibility;
}

interface ThemePalette {
  background: string;
  panel: string;
  panelAlt: string;
  text: string;
  muted: string;
  accent: string;
  accentText: string;
  border: string;
  hover: string;
  active: string;
  subtle: string;
}

interface InstalledTheme {
  manifest: ThemeManifest;
  theme: {
    palette: ThemePalette;
    background: { image: string; position: string; size: string };
  };
  previewPath: string | null;
  backgroundPath: string;
}

interface WorkBuddyStatus {
  installed: boolean;
  appPath: string;
  version: string | null;
  cdpAvailable: boolean;
  cdpPort: number | null;
  activeThemeId: string | null;
  configuredThemeId: string | null;
}

interface Notice {
  tone: "success" | "error" | "info";
  message: string;
}

const navItems: Array<{ id: View; label: string; icon: typeof Swatches }> = [
  { id: "library", label: "主题库", icon: Swatches },
  { id: "status", label: "运行状态", icon: Pulse },
  { id: "diagnostics", label: "诊断", icon: Wrench },
  { id: "settings", label: "设置", icon: Gear },
];

function App() {
  const [view, setView] = useState<View>("library");
  const [status, setStatus] = useState<WorkBuddyStatus | null>(null);
  const [themes, setThemes] = useState<InstalledTheme[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);

  const refresh = useCallback(async () => {
    const [nextStatus, nextThemes] = await Promise.all([
      invoke<WorkBuddyStatus>("get_workbuddy_status"),
      invoke<InstalledTheme[]>("list_themes"),
    ]);
    setStatus(nextStatus);
    setThemes(nextThemes);
    setSelectedId((current) => {
      if (current && nextThemes.some((item) => item.manifest.id === current)) return current;
      return nextStatus.activeThemeId ?? nextStatus.configuredThemeId ?? nextThemes[0]?.manifest.id ?? null;
    });
  }, []);

  useEffect(() => {
    refresh()
      .catch((error) => setNotice({ tone: "error", message: String(error) }))
      .finally(() => setLoading(false));
  }, [refresh]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<string>("runtime-error", (event) => {
      setNotice({ tone: "error", message: event.payload });
    })
      .then((dispose) => { unlisten = dispose; })
      .catch((error) => setNotice({ tone: "error", message: String(error) }));
    return () => unlisten?.();
  }, []);

  const selectedTheme = useMemo(
    () => themes.find((item) => item.manifest.id === selectedId) ?? null,
    [selectedId, themes],
  );

  async function importPackage() {
    try {
      const path = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "WorkBuddy Skin", extensions: ["wbskin", "zip"] }],
      });
      if (!path) return;
      setBusy("import");
      setNotice(null);
      const imported = await invoke<InstalledTheme>("import_theme", { path });
      await refresh();
      setSelectedId(imported.manifest.id);
      setView("library");
      setNotice({ tone: "success", message: `${imported.manifest.name} 已安全导入主题库。` });
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function refreshStatus() {
    setBusy("refresh");
    try {
      await refresh();
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  async function applyTheme(theme: InstalledTheme) {
    setBusy(`apply:${theme.manifest.id}`);
    setNotice({ tone: "info", message: "正在重启 WorkBuddy、注入主题并验证真实组件。" });
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

  async function restoreWorkBuddy() {
    setBusy("restore");
    setNotice({ tone: "info", message: "正在清除主题并恢复 WorkBuddy 普通启动模式。" });
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
    if (!window.confirm(`从本机主题库删除“${theme.manifest.name}”？`)) return;
    setBusy(`delete:${theme.manifest.id}`);
    try {
      await invoke("delete_theme", { id: theme.manifest.id });
      await refresh();
      setNotice({ tone: "success", message: `${theme.manifest.name} 已从主题库删除。` });
    } catch (error) {
      setNotice({ tone: "error", message: String(error) });
    } finally {
      setBusy(null);
    }
  }

  return (
    <Theme accentColor="ruby" grayColor="mauve" radius="large" scaling="95%">
      <div className="app-shell">
        <aside className="sidebar">
          <div className="brand">
            <div className="brand-mark" aria-hidden="true"><Heart weight="fill" size={18} /></div>
            <div>
              <strong>Skin Manager</strong>
              <span>for WorkBuddy</span>
            </div>
          </div>

          <nav aria-label="Manager 导航">
            {navItems.map((item) => {
              const Icon = item.icon;
              return (
                <button
                  className={`nav-item ${view === item.id ? "is-active" : ""}`}
                  key={item.id}
                  onClick={() => setView(item.id)}
                  type="button"
                >
                  <Icon size={19} weight={view === item.id ? "fill" : "regular"} />
                  <span>{item.label}</span>
                  {item.id === "library" && themes.length > 0 && <span className="nav-count">{themes.length}</span>}
                </button>
              );
            })}
          </nav>

          <div className="sidebar-status">
            <div className={`status-indicator ${status?.installed ? "is-ready" : "is-error"}`} />
            <div>
              <strong>{status?.installed ? `WorkBuddy ${status.version ?? ""}` : "未检测到 WorkBuddy"}</strong>
              <span>{status?.activeThemeId ? "主题运行中" : status?.configuredThemeId ? "主题等待恢复" : "官方外观"}</span>
            </div>
          </div>
        </aside>

        <main className="main-content">
          <header className="topbar">
            <div>
              <p className="page-kicker">{pageKicker(view)}</p>
              <h1>{pageTitle(view)}</h1>
            </div>
            <div className="topbar-actions">
              <Badge color={status?.cdpAvailable ? "green" : "gray"} variant="soft" size="2">
                {status?.cdpAvailable ? `CDP ${status.cdpPort} 已验证` : "CDP 未验证"}
              </Badge>
              <Button onClick={importPackage} disabled={busy !== null} size="3">
                {busy === "import" ? <Spinner /> : <UploadSimple size={18} />}
                导入主题
              </Button>
            </div>
          </header>

          {notice && <NoticeBar notice={notice} onClose={() => setNotice(null)} />}

          {loading ? (
            <LoadingState />
          ) : view === "library" ? (
            <LibraryView
              activeThemeId={status?.activeThemeId ?? undefined}
              busy={busy}
              onApply={applyTheme}
              onDelete={deleteTheme}
              onImport={importPackage}
              onSelect={setSelectedId}
              selectedTheme={selectedTheme}
              protectedThemeId={status?.configuredThemeId ?? undefined}
              themes={themes}
            />
          ) : view === "status" ? (
            <StatusView busy={busy} onRefresh={refreshStatus} onRestore={restoreWorkBuddy} status={status} />
          ) : view === "diagnostics" ? (
            <DiagnosticsView status={status} themes={themes} />
          ) : (
            <SettingsView />
          )}
        </main>
      </div>
    </Theme>
  );
}

function LibraryView({
  activeThemeId,
  busy,
  onApply,
  onDelete,
  onImport,
  onSelect,
  selectedTheme,
  protectedThemeId,
  themes,
}: {
  activeThemeId?: string;
  busy: string | null;
  onApply: (theme: InstalledTheme) => void;
  onDelete: (theme: InstalledTheme) => void;
  onImport: () => void;
  onSelect: (id: string) => void;
  selectedTheme: InstalledTheme | null;
  protectedThemeId?: string;
  themes: InstalledTheme[];
}) {
  if (!selectedTheme) {
    return (
      <section className="empty-state">
        <div className="empty-icon"><Swatches size={30} /></div>
        <h2>还没有本地主题</h2>
        <p>导入由 WorkBuddy Skin Studio 导出的 `.wbskin` 文件。Manager 会先完成安全校验。</p>
        <Button onClick={onImport} size="3"><UploadSimple size={18} />选择主题包</Button>
      </section>
    );
  }

  const image = convertFileSrc(selectedTheme.previewPath ?? selectedTheme.backgroundPath);
  const isActive = activeThemeId === selectedTheme.manifest.id;
  const applying = busy === `apply:${selectedTheme.manifest.id}`;

  return (
    <div className="library-view">
      <section className="theme-feature" style={{ backgroundColor: selectedTheme.theme.palette.background }}>
        {image && <img src={image} alt={`${selectedTheme.manifest.name} 主题预览`} />}
        <div className="feature-scrim" />
        <div className="feature-content">
          <div className="feature-meta">
            <Badge color={isActive ? "green" : "gray"} variant="solid">{isActive ? "当前使用" : "本地主题"}</Badge>
            <span>{selectedTheme.manifest.author}</span>
          </div>
          <h2>{selectedTheme.manifest.name}</h2>
          <p>{selectedTheme.manifest.description || "这个主题没有附加说明。"}</p>
          <div className="feature-actions">
            <Button size="3" onClick={() => onApply(selectedTheme)} disabled={busy !== null || isActive}>
              {applying ? <Spinner /> : isActive ? <CheckCircle size={18} weight="fill" /> : <Play size={18} weight="fill" />}
              {applying ? "正在应用" : isActive ? "已经生效" : "应用并重启"}
            </Button>
            <span>兼容 WorkBuddy {selectedTheme.manifest.compatibility.workbuddy.join(", ")}</span>
          </div>
        </div>
      </section>

      <section className="theme-library" aria-labelledby="local-themes-title">
        <div className="section-heading">
          <h2 id="local-themes-title">本地主题</h2>
          <span>{themes.length} 个已验证主题</span>
        </div>
        <div className="theme-grid">
          {themes.map((theme) => {
            const cardImage = convertFileSrc(theme.previewPath ?? theme.backgroundPath);
            const active = activeThemeId === theme.manifest.id;
            const protectedTheme = protectedThemeId === theme.manifest.id;
            return (
              <article className={`theme-card ${selectedTheme.manifest.id === theme.manifest.id ? "is-selected" : ""}`} key={theme.manifest.id}>
                <button
                  aria-pressed={selectedTheme.manifest.id === theme.manifest.id}
                  className="theme-card-select"
                  onClick={() => onSelect(theme.manifest.id)}
                  type="button"
                >
                  <div className="theme-card-image" style={{ backgroundColor: theme.theme.palette.background }}>
                    {cardImage && <img src={cardImage} alt="" />}
                    {active && <span className="active-label"><CheckCircle weight="fill" size={15} />使用中</span>}
                  </div>
                  <div className="theme-card-body">
                    <h3>{theme.manifest.name}</h3>
                    <p>{theme.manifest.author}</p>
                  </div>
                </button>
                <button
                  aria-label={`删除 ${theme.manifest.name}`}
                  className="icon-button"
                  disabled={protectedTheme || busy !== null}
                  onClick={() => onDelete(theme)}
                  type="button"
                >
                  <Trash size={17} />
                </button>
              </article>
            );
          })}
        </div>
      </section>
    </div>
  );
}

function StatusView({ busy, onRefresh, onRestore, status }: { busy: string | null; onRefresh: () => void; onRestore: () => void; status: WorkBuddyStatus | null }) {
  return (
    <section className="details-panel">
      <div className="details-intro">
        <Desktop size={30} />
        <div><h2>WorkBuddy 运行状态</h2><p>Manager 只连接本机 127.0.0.1，不修改 WorkBuddy 安装包。</p></div>
      </div>
      <dl className="status-list">
        <StatusRow label="应用安装" value={status?.installed ? "已检测到" : "未检测到"} healthy={Boolean(status?.installed)} />
        <StatusRow label="应用版本" value={status?.version ?? "未知"} healthy={Boolean(status?.version)} />
        <StatusRow label="CDP 连接" value={status?.cdpAvailable ? `127.0.0.1:${status.cdpPort}` : "未验证"} healthy={Boolean(status?.cdpAvailable)} />
        <StatusRow
          label="当前主题"
          value={status?.activeThemeId ?? (status?.configuredThemeId ? `${status.configuredThemeId}（未生效）` : "官方外观")}
          healthy={Boolean(status?.activeThemeId)}
          neutral={!status?.configuredThemeId}
        />
      </dl>
      <div className="panel-actions">
        <Button variant="soft" onClick={onRefresh} disabled={busy !== null}>
          {busy === "refresh" ? <Spinner /> : <ArrowClockwise size={18} />}刷新状态
        </Button>
        <Button color="gray" variant="outline" onClick={onRestore} disabled={busy !== null}>
          {busy === "restore" ? <Spinner /> : <ShieldCheck size={18} />}恢复官方外观
        </Button>
      </div>
    </section>
  );
}

function DiagnosticsView({ status, themes }: { status: WorkBuddyStatus | null; themes: InstalledTheme[] }) {
  const checks = [
    { label: "WorkBuddy 安装", pass: Boolean(status?.installed), detail: status?.appPath ?? "等待自动检测" },
    { label: "版本识别", pass: Boolean(status?.version), detail: status?.version ?? "无法读取版本" },
    { label: "主题库", pass: themes.length > 0, detail: themes.length > 0 ? `${themes.length} 个主题可用` : "等待导入主题" },
    { label: "CDP 会话", pass: Boolean(status?.cdpAvailable), detail: status?.cdpAvailable ? "本机端口已连接" : "应用主题时自动启动" },
  ];
  return (
    <section className="details-panel">
      <div className="details-intro"><Wrench size={30} /><div><h2>环境诊断</h2><p>这些检查不会上传文件，也不会读取 WorkBuddy 对话内容。</p></div></div>
      <div className="diagnostic-grid">
        {checks.map((check) => (
          <div className="diagnostic-item" key={check.label}>
            {check.pass ? <CheckCircle size={21} weight="fill" /> : <Info size={21} weight="fill" />}
            <div><strong>{check.label}</strong><span>{check.detail}</span></div>
          </div>
        ))}
      </div>
    </section>
  );
}

function SettingsView() {
  return (
    <section className="details-panel">
      <div className="details-intro"><Gear size={30} /><div><h2>Manager 设置</h2><p>首个 MVP 保持默认行为明确，不在后台静默修改系统设置。</p></div></div>
      <div className="setting-block">
        <div><strong>主题包安全策略</strong><span>只允许 JSON 和本地 PNG、JPEG、WebP 资源</span></div>
        <Badge color="green" variant="soft">强制启用</Badge>
      </div>
      <div className="setting-block">
        <div><strong>本机 CDP</strong><span>每次随机分配回环端口，并校验 WorkBuddy 进程归属</span></div>
        <Badge color="green" variant="soft">随机回环端口</Badge>
      </div>
    </section>
  );
}

function StatusRow({ healthy, label, neutral = false, value }: { healthy: boolean; label: string; neutral?: boolean; value: string }) {
  return <div><dt>{label}</dt><dd><span className={`row-state ${healthy ? "is-ready" : neutral ? "is-neutral" : "is-error"}`} />{value}</dd></div>;
}

function NoticeBar({ notice, onClose }: { notice: Notice; onClose: () => void }) {
  const Icon = notice.tone === "success" ? CheckCircle : notice.tone === "error" ? Warning : Info;
  return <div aria-live="polite" className={`notice notice-${notice.tone}`} role={notice.tone === "error" ? "alert" : "status"}><Icon size={19} weight="fill" /><span>{notice.message}</span><button onClick={onClose} type="button">关闭</button></div>;
}

function LoadingState() {
  return <div className="loading-state" aria-label="正在读取 Manager 状态" role="status"><div className="skeleton skeleton-large" /><div className="skeleton-row"><div className="skeleton" /><div className="skeleton" /></div></div>;
}

function pageKicker(view: View) {
  return { library: "管理你的 WorkBuddy 外观", status: "连接与恢复", diagnostics: "本机检查", settings: "安全与行为" }[view];
}

function pageTitle(view: View) {
  return { library: "主题库", status: "运行状态", diagnostics: "诊断", settings: "设置" }[view];
}

export default App;
