"use client";

import Link from "next/link";
import { Icon } from "./icon";

interface Segment {
  label: string;
  href?: string;
}

export function Breadcrumbs({ segments }: { segments: Segment[] }) {
  return (
    <nav className="flex items-center gap-1.5 text-sm text-zinc-400 mb-4 overflow-x-auto">
      {segments.map((seg, i) => (
        <span key={i} className="flex items-center gap-1.5 whitespace-nowrap">
          {i > 0 && <Icon name="chevron-right" className="w-3 h-3 text-zinc-300" />}
          {seg.href ? (
            <Link href={seg.href} className="hover:text-zinc-600 transition-colors">
              {seg.label}
            </Link>
          ) : (
            <span className="text-zinc-700">{seg.label}</span>
          )}
        </span>
      ))}
    </nav>
  );
}
