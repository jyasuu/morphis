"use client";

import { useEffect, useState } from "react";
import { Icon } from "./icon";
import { useT } from "@/lib/i18n";

interface Props {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel,
  cancelLabel,
  danger = true,
  onConfirm,
  onCancel,
}: Props) {
  const t = useT();
  const confirmText = confirmLabel ?? t("confirm.confirm");
  const cancelText = cancelLabel ?? t("confirm.cancel");
  useEffect(() => {
    if (!open) return;
    function handler(e: KeyboardEvent) {
      if (e.key === "Escape") onCancel();
    }
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, onCancel]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="fixed inset-0 bg-black/30 backdrop-blur-sm" onClick={onCancel} />
      <div className="relative bg-[var(--surface)] rounded-xl shadow-2xl max-w-sm w-full mx-4 p-6 animate-in zoom-in-95 duration-150">
        <h3 className="text-base font-semibold text-[var(--text)] mb-2">{title}</h3>
        <p className="text-sm text-[var(--text-secondary)] mb-5">{message}</p>
        <div className="flex justify-end gap-2">
          <button
            onClick={onCancel}
            className="px-4 py-2 text-sm font-medium rounded-lg border border-[var(--border)] text-[var(--text-secondary)] hover:bg-[var(--muted)] transition-colors"
          >
            {cancelText}
          </button>
          <button
            onClick={onConfirm}
            className={`px-4 py-2 text-sm font-medium rounded-lg text-white transition-colors ${
              danger
                ? "bg-red-600 hover:bg-red-700"
                : "bg-[#0d9488] hover:bg-[#0f766e]"
            }`}
          >
            {confirmText}
          </button>
        </div>
      </div>
    </div>
  );
}

interface ConfirmState {
  open: boolean;
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  danger: boolean;
  resolve: (v: boolean) => void;
}

export function useConfirm() {
  const [state, setState] = useState<ConfirmState | null>(null);

  function confirm(
    title: string,
    message: string,
    confirmLabel = "Delete",
    cancelLabel = "Cancel",
    danger = true
  ): Promise<boolean> {
    return new Promise((resolve) => {
      setState({ open: true, title, message, confirmLabel, cancelLabel, danger, resolve });
    });
  }

  const dialogEl = state ? (
    <ConfirmDialog
      open={state.open}
      title={state.title}
      message={state.message}
      confirmLabel={state.confirmLabel}
      cancelLabel={state.cancelLabel}
      danger={state.danger}
      onConfirm={() => {
        state.resolve(true);
        setState(null);
      }}
      onCancel={() => {
        state.resolve(false);
        setState(null);
      }}
    />
  ) : null;

  return { confirm, dialog: dialogEl };
}
