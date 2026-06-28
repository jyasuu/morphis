"use client";

import { useState } from "react";
import { getFieldControl } from "@/lib/metadata";
import { useT } from "@/lib/i18n";

interface Props {
  entityName: string;
  fields: { name: string; scalarType: string }[];
  onFilter: (filter: Record<string, string>) => void;
}

export function SearchFilter({ entityName, fields, onFilter }: Props) {
  const t = useT();
  const [values, setValues] = useState<Record<string, string>>({});

  function handleChange(name: string, value: string) {
    const next = { ...values, [name]: value };
    setValues(next);
    onFilter(next);
  }

  return (
    <div className="flex flex-wrap gap-3">
      {fields.map((f) => {
        const ctrl = getFieldControl(entityName, f.name);

        if (ctrl.control === "select" && ctrl.options) {
          return (
            <div key={f.name} className="flex-1 min-w-[140px]">
              <label className="block text-xs text-[var(--text-secondary)] mb-0.5">{t.field(entityName, f.name)}</label>
              <select
                value={values[f.name] ?? ""}
                onChange={(e) => handleChange(f.name, e.target.value)}
                className="w-full border border-[var(--border)] rounded-lg px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 bg-[var(--surface)]"
              >
                <option value="">{t("searchFilter.all")}</option>
                {ctrl.options.map((o) => (
                  <option key={o.value} value={o.value}>{o.label}</option>
                ))}
              </select>
            </div>
          );
        }

        return (
          <div key={f.name} className="flex-1 min-w-[140px]">
            <label className="block text-xs text-[var(--text-secondary)] mb-0.5">{t.field(entityName, f.name)}</label>
            <input
              type="text"
              value={values[f.name] ?? ""}
              onChange={(e) => handleChange(f.name, e.target.value)}
              placeholder={t.field(entityName, f.name)}
              className="w-full border border-[var(--border)] rounded-lg px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 bg-[var(--surface)]"
            />
          </div>
        );
      })}
    </div>
  );
}
