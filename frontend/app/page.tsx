"use client";

import { useEffect, useState } from "react";
import { getEntityNames } from "@/lib/schema";
import Link from "next/link";
import { Skeleton } from "@/components/skeleton";
import { useT } from "@/lib/i18n";

export default function Home() {
  const t = useT();
  const [entities, setEntities] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    getEntityNames().then((names) => {
      setEntities(names);
      setLoading(false);
    });
  }, []);

  return (
    <div>
      <h1 className="text-2xl font-semibold mb-6">{t("home.title")}</h1>
      {loading ? (
        <div className="grid gap-4 sm:grid-cols-2 md:grid-cols-3">
          {Array.from({ length: 6 }).map((_, i) => (
              <div key={i} className="border border-[var(--border)] rounded-xl p-5 bg-[var(--surface)] shadow-sm">
              <Skeleton className="h-5 w-28" />
              <Skeleton className="h-3 w-20 mt-2" />
            </div>
          ))}
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 md:grid-cols-3">
          {entities.map((name) => (
            <Link
              key={name}
              href={`/${name}`}
              className="block border border-[var(--border)] rounded-xl p-5 bg-[var(--surface)] shadow-sm hover:shadow-md hover:border-[#0d9488]/40 transition-all"
            >
              <span className="font-semibold text-[var(--text)]">{t.entity(name)}</span>
              <p className="text-xs text-[var(--text-muted)] mt-1">{t("home.manage", { name: t.entity(name) })}</p>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
