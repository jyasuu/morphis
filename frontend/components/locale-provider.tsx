"use client";

import { createContext, useContext, useState, useEffect, useCallback, type ReactNode } from "react";

interface LocaleInfo {
  code: string;
  label: string;
}

interface LocaleContextValue {
  locale: string;
  setLocale: (l: string) => void;
  messages: Record<string, unknown> | null;
  locales: LocaleInfo[];
}

const LocaleContext = createContext<LocaleContextValue>({
  locale: "en",
  setLocale: () => {},
  messages: null,
  locales: [],
});

export function useLocale() {
  return useContext(LocaleContext);
}

const cache = new Map<string, Record<string, unknown>>();

export function LocaleProvider({ children }: { children: ReactNode }) {
  const [locale, setLocale] = useState("en");
  const [messages, setMessages] = useState<Record<string, unknown> | null>(null);
  const [locales, setLocales] = useState<LocaleInfo[]>([]);

  // Load available locales list
  useEffect(() => {
    fetch("/locales/index.json")
      .then((r) => r.json())
      .then((list: LocaleInfo[]) => setLocales(list))
      .catch(() => setLocales([{ code: "en", label: "EN" }]));
  }, []);

  const loadMessages = useCallback(async (l: string) => {
    if (cache.has(l)) {
      setMessages(cache.get(l)!);
      return;
    }
    try {
      const res = await fetch(`/locales/${l}.json`);
      if (!res.ok) throw new Error(`Failed to load locale: ${l}`);
      const data = await res.json();
      cache.set(l, data);
      setMessages(data);
    } catch {
      setMessages(null);
    }
  }, []);

  useEffect(() => {
    const stored = localStorage.getItem("morphis-locale");
    const initial = stored ?? (navigator.language.startsWith("zh") ? "zh-TW" : "en");
    setLocale(initial);
    loadMessages(initial);
  }, [loadMessages]);

  function setAndStore(l: string) {
    setLocale(l);
    localStorage.setItem("morphis-locale", l);
    loadMessages(l);
  }

  return (
    <LocaleContext.Provider value={{ locale, setLocale: setAndStore, messages, locales }}>
      {children}
    </LocaleContext.Provider>
  );
}
