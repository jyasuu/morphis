"use client";

import { useEffect, useState, useRef } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { getEntityNames } from "@/lib/schema";
import { ThemeToggle } from "./theme-toggle";
import { useT } from "@/lib/i18n";
import { useLocale } from "./locale-provider";

export function NavBar() {
  const t = useT();
  const { setLocale, locales } = useLocale();
  const [entities, setEntities] = useState<string[]>([]);
  const [search, setSearch] = useState("");
  const [focused, setFocused] = useState(false);
  const pathname = usePathname();
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    getEntityNames().then(setEntities);
  }, []);

  const filtered = search
    ? entities.filter((name) => {
        const label = t.entity(name).toLowerCase();
        return label.includes(search.toLowerCase()) || name.includes(search);
      })
    : entities;

  return (
    <header className="bg-[var(--surface)] border-b border-[var(--border)] shadow-sm">
      <div className="flex items-center justify-between px-6 h-12">
        <Link href="/" className="flex items-center gap-2 shrink-0">
          <svg viewBox="0 0 32 32" fill="none" className="w-7 h-7">
            <rect width="32" height="32" rx="6" fill="#0d9488" />
            <path d="M8 12l8 10 8-10" stroke="white" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
            <path d="M8 20l8-10 8 10" stroke="rgba(255,255,255,0.35)" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          <span className="text-lg font-semibold text-[#0d9488] tracking-tight hidden sm:inline">
            Morphis Admin
          </span>
        </Link>
        <div className="flex items-center gap-1 ml-4 flex-1 min-w-0">
          <div className="relative flex-1 max-w-md">
            <input
              ref={inputRef}
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              onFocus={() => setFocused(true)}
              onBlur={() => setTimeout(() => setFocused(false), 200)}
              placeholder={t("nav.search")}
              className="w-full px-3 py-1.5 text-sm rounded-lg border border-[var(--border)] bg-[var(--bg)] text-[var(--text)] placeholder-[var(--text-muted)] outline-none focus:border-[#0d9488] focus:ring-1 focus:ring-[#0d9488]/30 transition-colors"
            />
            {search && focused && filtered.length > 0 && (
              <div className="absolute top-full left-0 right-0 mt-1 bg-[var(--surface)] border border-[var(--border)] rounded-lg shadow-lg z-50 max-h-48 overflow-y-auto">
                {filtered.map((name) => {
                  const active = pathname === `/${name}` || pathname.startsWith(`/${name}/`);
                  return (
                    <Link
                      key={name}
                      href={`/${name}`}
                      className={`block px-3 py-2 text-sm transition-colors ${
                        active
                          ? "bg-[#0d9488]/10 text-[#0d9488] font-medium"
                          : "text-[var(--text-secondary)] hover:bg-[var(--muted)]"
                      }`}
                    >
                      {t.entity(name)}
                    </Link>
                  );
                })}
              </div>
            )}
          </div>
          <nav className="flex items-center gap-1 text-sm overflow-x-auto flex-nowrap scrollbar-none ml-2">
            {filtered.slice(0, 5).map((name) => {
              const active = pathname === `/${name}` || pathname.startsWith(`/${name}/`);
              return (
                <Link
                  key={name}
                  href={`/${name}`}
                  className={`px-2.5 py-1 rounded-md transition-colors whitespace-nowrap ${
                    active
                      ? "bg-[#0d9488]/10 text-[#0d9488] font-medium"
                      : "text-[var(--text-secondary)] hover:bg-[var(--muted)]"
                  }`}
                >
                  {t.entity(name)}
                </Link>
              );
            })}
            {filtered.length > 5 && (
              <span className="text-xs text-[var(--text-muted)] px-1 select-none">
                +{filtered.length - 5}
              </span>
            )}
          </nav>
          {locales.map((l) => (
            <button
              key={l.code}
              onClick={() => setLocale(l.code)}
              className={`px-2 py-1 text-xs rounded-md border transition-colors mr-0.5 last:mr-1 shrink-0 ${
                t.locale === l.code
                  ? "bg-[#0d9488]/10 text-[#0d9488] border-[#0d9488]/30"
                  : "border-[var(--border)] text-[var(--text-secondary)] hover:bg-[var(--muted)]"
              }`}
            >
              {l.label}
            </button>
          ))}
          <ThemeToggle />
        </div>
      </div>
    </header>
  );
}
