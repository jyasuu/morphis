"use client";

import { useState, useEffect, useRef } from "react";
import { Icon } from "./icon";
import { useT } from "@/lib/i18n";

interface Props {
  onSearch: (query: string) => void;
  placeholder?: string;
}

export function SearchBar({ onSearch, placeholder: placeholderProp }: Props) {
  const t = useT();
  const placeholder = placeholderProp || t("list.search", { name: "" });
  const [value, setValue] = useState("");
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      onSearch(value);
    }, 300);
    return () => clearTimeout(timer.current);
  }, [value, onSearch]);

  return (
    <div className="relative w-full max-w-md">
      <span className="absolute left-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)] pointer-events-none"><Icon name="search" className="w-4 h-4" /></span>
      <input
        type="text"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        placeholder={placeholder}
        className="w-full pl-9 pr-3 py-2 text-sm border border-[var(--border)] rounded-lg bg-[var(--surface)] focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-400 transition-shadow"
      />
    </div>
  );
}
