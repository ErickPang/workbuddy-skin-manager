import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ModalShell } from "./ModalShell";

describe("ModalShell", () => {
  it("labels the dialog, focuses its first action, and closes with Escape", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(
      <ModalShell title="诊断信息" onClose={onClose} actions={<button type="button">关闭</button>}>
        <p>诊断内容</p>
      </ModalShell>,
    );

    expect(screen.getByRole("dialog", { name: "诊断信息" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "关闭" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("keeps Tab focus inside the dialog and restores the previous focus", async () => {
    const user = userEvent.setup();
    const trigger = document.createElement("button");
    trigger.textContent = "打开";
    document.body.append(trigger);
    trigger.focus();
    const { unmount } = render(
      <ModalShell title="条款" onClose={() => {}} actions={<button type="button">关闭</button>}>
        <input aria-label="内容输入" />
      </ModalShell>,
    );

    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(screen.getByRole("textbox", { name: "内容输入" })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "关闭" })).toHaveFocus();

    unmount();
    expect(trigger).toHaveFocus();
    trigger.remove();
  });
});
