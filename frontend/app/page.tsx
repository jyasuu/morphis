"use client";

import { useEffect, useState } from "react";
import { getEntityNames } from "@/lib/schema";
import Link from "next/link";

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
      <h1 className="text-2xl font-semibold mb-4">Entities</h1>
      {loading ? (
        <p className="text-zinc-500 text-sm">Loading...</p>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 md:grid-cols-3">
          {entities.map((name) => (
            <Link
              key={name}
              href={`/${name}`}
              className="block border rounded-lg px-4 py-3 hover:bg-zinc-50 hover:border-blue-300 transition-colors"
            >
              <span className="font-medium text-zinc-800">{name}</span>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
