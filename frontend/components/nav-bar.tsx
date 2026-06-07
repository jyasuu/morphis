"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { getEntityNames } from "@/lib/schema";

export function NavBar() {
  const [entities, setEntities] = useState<string[]>([]);
  const pathname = usePathname();

  useEffect(() => {
    getEntityNames().then(setEntities);
  }, []);

  return (
    <header className="bg-white border-b border-zinc-200 shadow-sm">
      <div className="flex items-center justify-between px-6 h-12">
        <Link href="/" className="text-lg font-semibold text-zinc-800 tracking-tight">
          Morphis Admin
        </Link>
        <nav className="flex items-center gap-1 text-sm overflow-x-auto flex-nowrap scrollbar-none ml-4">
          {entities.map((name) => {
            const active = pathname === `/${name}` || pathname.startsWith(`/${name}/`);
            return (
              <Link
                key={name}
                href={`/${name}`}
                className={`px-2.5 py-1 rounded-md transition-colors ${
                  active
                    ? "bg-blue-100 text-blue-700 font-medium"
                    : "text-zinc-600 hover:bg-zinc-100"
                }`}
              >
                {name}
              </Link>
            );
          })}
        </nav>
      </div>
    </header>
  );
}
