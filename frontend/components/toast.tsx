"use client";

import { useEffect, useState } from "react";

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
    <div
      className={`fixed bottom-4 right-4 px-4 py-2 rounded-lg text-sm text-white shadow-lg transition-all z-50 ${
        toast.type === "success" ? "bg-green-600" : "bg-red-600"
      }`}
    >
      {toast.message}
    </div>
  );
}
