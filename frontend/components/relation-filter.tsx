"use client";

import { useState } from "react";
import { useQuery } from "urql";
import type { RelationFilterMeta } from "@/lib/metadata";

interface Props {
  filter: RelationFilterMeta;
  onSelect: (values: string[]) => void;
}

function buildOptionsQuery(entityName: string, field: string): string {
  return `query ${entityName}Options {
    ${entityName}List(limit: 200) {
      ${field}
    }
  }`;
}

export function RelationFilter({ filter, onSelect }: Props) {
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const queryStr = buildOptionsQuery(filter.relationEntity, filter.field);
  const [result] = useQuery({ query: queryStr });

  const options = result.data
    ? [
        ...new Set(
          ((result.data as any)?.[`${filter.relationEntity}List`] as Record<string, unknown>[])?.map(
            (r) => String(r[filter.field] ?? "")
          ) ?? []
        ),
      ]
        .filter(Boolean)
        .sort()
    : [];

  function toggle(value: string) {
    const next = new Set(checked);
    if (next.has(value)) {
      next.delete(value);
    } else {
      next.add(value);
    }
    setChecked(next);
    onSelect([...next]);
  }

  return (
    <div className="flex-1">
      <label className="block text-xs text-[var(--text-secondary)] mb-1">{filter.label}</label>
      <div className="flex flex-wrap gap-2">
        {options.map((opt) => (
          <label
            key={opt}
            className={`flex items-center gap-1 px-2.5 py-1 rounded-lg text-sm cursor-pointer border transition-colors ${
              checked.has(opt)
                ? "bg-[#0d9488]/10 border-[#0d9488] text-[#0d9488]"
                : "bg-[var(--surface)] border-[var(--border)] text-[var(--text)] hover:bg-[var(--muted)]"
            }`}
          >
            <input
              type="checkbox"
              checked={checked.has(opt)}
              onChange={() => toggle(opt)}
              className="sr-only"
            />
            {opt}
          </label>
        ))}
      </div>
    </div>
  );
}
