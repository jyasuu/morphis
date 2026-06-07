"use client";

import { useEffect, useState } from "react";
import { Icon } from "./icon";

interface Toast {
  message: string;
  type: "success" | "error";
}

let pushToast: ((t: Toast) => void) | null = null;

export function showToast(message: string, type: "success" | "error" = "success") {
  pushToast?.({ message, type });
}

export function ToastContainer() {
  const [toast, setToast] = useState<Toast | null>(null);

  useEffect(() => {
    pushToast = setToast;
    return () => {
      pushToast = null;
    };
  }, []);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 3000);
    return () => clearTimeout(t);
  }, [toast]);

  if (!toast) return null;

  return (
    <div className="fixed bottom-4 right-4 z-50 animate-in slide-in-from-right-2 fade-in duration-200">
      <div
        className={`flex items-center gap-2.5 px-4 py-3 rounded-lg text-sm text-white shadow-lg ${
          toast.type === "success" ? "bg-emerald-600" : "bg-red-600"
        }`}
      >
        <Icon name={toast.type === "success" ? "check" : "x"} className="w-4 h-4" />
        <span>{toast.message}</span>
      </div>
    </div>
  );
}
