import { useState } from "react";
import privacyText from "../../PRIVACY.md?raw";
import termsText from "../../TERMS.md?raw";
import type { DiagnosticInfo } from "../types";
import { ModalShell } from "./ModalShell";

export type LegalDocument = "privacy" | "terms";

export function DiagnosticsModal({ diagnostics, onClose, onError }: { diagnostics: DiagnosticInfo; onClose: () => void; onError: (message: string) => void }) {
  const [copied, setCopied] = useState(false);
  const content = JSON.stringify(diagnostics, null, 2);

  async function copyDiagnostics() {
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
    } catch (error) {
      onError(`无法复制诊断信息: ${String(error)}`);
    }
  }

  return (
    <ModalShell
      title="诊断信息"
      onClose={onClose}
      actions={<><button type="button" onClick={copyDiagnostics}>{copied ? "已复制" : "复制"}</button><button type="button" onClick={onClose}>关闭</button></>}
    >
      <p className="diagnostics-note">内容仅在本机生成，复制前请确认安装路径等信息适合提供给支持人员。</p>
      <pre>{content}</pre>
    </ModalShell>
  );
}

export function LegalDocumentModal({ document, onClose }: { document: LegalDocument; onClose: () => void }) {
  const content = document === "privacy" ? privacyText : termsText;
  const title = document === "privacy" ? "隐私政策" : "使用条款";

  return (
    <ModalShell title={title} onClose={onClose} actions={<button type="button" onClick={onClose}>关闭</button>}>
      <pre>{content}</pre>
    </ModalShell>
  );
}
