"use client";

import { createContext, useContext, useState, useEffect, type ReactNode } from "react";

export type Locale = "en" | "zh-TW";

interface LocaleContextValue {
  locale: Locale;
  setLocale: (l: Locale) => void;
}

const LocaleContext = createContext<LocaleContextValue>({
  locale: "en",
  setLocale: () => {},
});

export function useLocale() {
  return useContext(LocaleContext);
}

export function LocaleProvider({ children }: { children: ReactNode }) {
  const [locale, setLocale] = useState<Locale>("en");

  useEffect(() => {
    const stored = localStorage.getItem("morphis-locale") as Locale | null;
    if (stored === "en" || stored === "zh-TW") {
      setLocale(stored);
    } else {
      const lang = navigator.language;
      if (lang.startsWith("zh")) {
        setLocale("zh-TW");
      }
    }
  }, []);

  function setAndStore(l: Locale) {
    setLocale(l);
    localStorage.setItem("morphis-locale", l);
  }

  return (
    <LocaleContext.Provider value={{ locale, setLocale: setAndStore }}>
      {children}
    </LocaleContext.Provider>
  );
}
