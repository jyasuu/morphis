"use client";

import { useEffect, useState } from "react";
import { useQuery } from "urql";
import type { RelationFilterMeta } from "@/lib/metadata";

interface Props {
  filter: RelationFilterMeta;
  onSelect: (query: string) => void;
}

function buildOptionsQuery(entityName: string, field: string): string {
  return `query ${entityName}Options {
    ${entityName}List(limit: 200) {
      ${field}
    }
  }`;
}

export function RelationFilter({ filter, onSelect }: Props) {
  const [selected, setSelected] = useState("");
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

  function handleChange(value: string) {
    setSelected(value);
    onSelect(value);
  }

  return (
    <div className="flex-1 min-w-[140px]">
      <label className="block text-xs text-zinc-500 mb-0.5">{filter.label}</label>
      <select
        value={selected}
        onChange={(e) => handleChange(e.target.value)}
        className="w-full border rounded-lg px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 bg-white"
      >
        <option value="">All</option>
        {options.map((opt) => (
          <option key={opt} value={opt}>{opt}</option>
        ))}
      </select>
    </div>
  );
}
