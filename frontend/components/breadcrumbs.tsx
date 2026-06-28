"use client";

import Link from "next/link";
import { Icon } from "./icon";
import { useT } from "@/lib/i18n";

interface Segment {
  label: string;
  href?: string;
}

export function Breadcrumbs({ segments }: { segments: Segment[] }) {
  return (
    <nav className="flex items-center gap-1.5 text-sm text-[var(--text-muted)] mb-4 overflow-x-auto">
      {segments.map((seg, i) => (
        <span key={i} className="flex items-center gap-1.5 whitespace-nowrap">
          {i > 0 && <Icon name="chevron-right" className="w-3 h-3 text-[var(--text-muted)]" />}
          {seg.href ? (
            <Link href={seg.href} className="hover:text-[var(--text-secondary)] transition-colors">
              {seg.label}
            </Link>
          ) : (
            <span className="text-[var(--text)]">{seg.label}</span>
          )}
        </span>
      ))}
    </nav>
  );
}
