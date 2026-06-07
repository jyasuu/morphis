"use client";

import { useEffect, useState } from "react";
import { getEntityNames } from "@/lib/schema";
import Link from "next/link";
import { Skeleton } from "@/components/skeleton";

export default function Home() {
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
      <h1 className="text-2xl font-semibold mb-6">Entities</h1>
      {loading ? (
        <div className="grid gap-4 sm:grid-cols-2 md:grid-cols-3">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="border border-zinc-200 rounded-xl p-5 bg-white shadow-sm">
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
              className="block border border-zinc-200 rounded-xl p-5 bg-white shadow-sm hover:shadow-md hover:border-[#0d9488]/40 transition-all"
            >
              <span className="font-semibold text-zinc-800 capitalize">{name.replace(/_/g, " ")}</span>
              <p className="text-xs text-zinc-400 mt-1">Manage {name}</p>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
