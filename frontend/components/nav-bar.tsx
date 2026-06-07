"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { getEntityNames } from "@/lib/schema";
import { ThemeToggle } from "./theme-toggle";
import { useT } from "@/lib/i18n";

export function NavBar() {
  const t = useT();
  const [entities, setEntities] = useState<string[]>([]);
  const pathname = usePathname();

  useEffect(() => {
    getEntityNames().then(setEntities);
  }, []);

  return (
    <header className="bg-[var(--surface)] border-b border-[var(--border)] shadow-sm">
      <div className="flex items-center justify-between px-6 h-12">
        <Link href="/" className="flex items-center gap-2">
          <svg
            viewBox="0 0 32 32"
            fill="none"
            className="w-7 h-7"
          >
            <rect width="32" height="32" rx="6" fill="#0d9488" />
            <path d="M8 12l8 10 8-10" stroke="white" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
            <path d="M8 20l8-10 8 10" stroke="rgba(255,255,255,0.35)" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          <span className="text-lg font-semibold text-[#0d9488] tracking-tight">
            Morphis Admin
          </span>
        </Link>
        <div className="flex items-center gap-1 ml-4">
          <nav className="flex items-center gap-1 text-sm overflow-x-auto flex-nowrap scrollbar-none">
            {entities.map((name) => {
              const active = pathname === `/${name}` || pathname.startsWith(`/${name}/`);
              return (
                <Link
                  key={name}
                  href={`/${name}`}
                  className={`px-2.5 py-1 rounded-md transition-colors ${
                    active
                      ? "bg-[#0d9488]/10 text-[#0d9488] font-medium"
                      : "text-[var(--text-secondary)] hover:bg-[var(--muted)]"
                  }`}
                >
                  {t.entity(name)}
                </Link>
              );
            })}
          </nav>
          <ThemeToggle />
        </div>
      </div>
    </header>
  );
}
