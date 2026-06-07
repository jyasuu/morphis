"use client";

import { useState, useCallback } from "react";
import { useQuery } from "urql";
import { getFieldControl } from "@/lib/metadata";
import type { RelationFilterMeta } from "@/lib/metadata";
import { Icon } from "./icon";

interface FilterRowState {
  id: string;
  field: string;
  operator: "equals" | "contains" | "has";
  value: string;
}

interface FilterFieldDef {
  key: string;
  label: string;
  kind: "direct" | "relation";
  operators: ("equals" | "contains" | "has")[];
  control: "text" | "select";
  options?: { label: string; value: string }[];
  relationEntity?: string;
  relationField?: string;
}

interface Props {
  entityName: string;
  filterFields: { name: string; scalarType: string }[];
  relationFilters: RelationFilterMeta[];
  onFilterChange: (params: {
    query: string;
    filter: Record<string, string>;
    logic?: "and" | "or";
    terms?: string[];
  }) => void;
}

let rowIdCounter = 0;
function nextId() {
  return `f_${++rowIdCounter}`;
}

function buildFieldDefs(
  entityName: string,
  filterFields: { name: string; scalarType: string }[],
  relationFilters: RelationFilterMeta[]
): FilterFieldDef[] {
  const defs: FilterFieldDef[] = [];
  for (const f of filterFields) {
    const ctrl = getFieldControl(entityName, f.name);
    defs.push({
      key: `direct:${f.name}`,
      label: f.name,
      kind: "direct",
      operators: ["equals", "contains"],
      control: ctrl.control,
      options: ctrl.options,
    });
  }
  for (const rf of relationFilters) {
    defs.push({
      key: `relation:${rf.relationEntity}:${rf.field}`,
      label: rf.label,
      kind: "relation",
      operators: ["has"],
      control: "select",
      relationEntity: rf.relationEntity,
      relationField: rf.field,
    });
  }
  return defs;
}

function buildOptionsQuery(entityName: string, field: string): string {
  return `query ${entityName}Options {
    ${entityName}List(limit: 200) {
      ${field}
    }
  }`;
}

function RelationValueSelect({
  entityName,
  field,
  selected,
  onChange,
}: {
  entityName: string;
  field: string;
  selected: Set<string>;
  onChange: (s: Set<string>) => void;
}) {
  const queryStr = buildOptionsQuery(entityName, field);
  const [result] = useQuery({ query: queryStr });

  const options = result.data
    ? [
        ...new Set(
          ((result.data as any)?.[`${entityName}List`] as Record<string, unknown>[])?.map(
            (r) => String(r[field] ?? "")
          ) ?? []
        ),
      ]
        .filter(Boolean)
        .sort()
    : [];

  function toggle(opt: string) {
    const next = new Set(selected);
    if (next.has(opt)) { next.delete(opt); } else { next.add(opt); }
    onChange(next);
  }

  return (
    <div className="flex flex-wrap gap-1.5 min-w-[200px]">
      {options.map((opt) => (
        <label
          key={opt}
          className={`flex items-center gap-1 px-2 py-0.5 rounded-lg text-sm cursor-pointer border transition-colors ${
            selected.has(opt)
              ? "bg-blue-100 border-blue-400 text-blue-800"
              : "bg-white border-zinc-300 text-zinc-700 hover:bg-zinc-50"
          }`}
        >
          <input
            type="checkbox"
            checked={selected.has(opt)}
            onChange={() => toggle(opt)}
            className="sr-only"
          />
          {opt}
        </label>
      ))}
    </div>
  );
}

export function AdvancedFilter({
  entityName,
  filterFields,
  relationFilters,
  onFilterChange,
}: Props) {
  const fieldDefs = buildFieldDefs(entityName, filterFields, relationFilters);
  const defaultLogic = relationFilters[0]?.defaultLogic ?? "and";
  const [logic, setLogic] = useState<"and" | "or">(defaultLogic);
  const [rows, setRows] = useState<FilterRowState[]>([]);

  const emit = useCallback(
    (currentRows: FilterRowState[], currentLogic: "and" | "or") => {
      const directFilter: Record<string, string> = {};
      const searchTerms: string[] = [];
      const terms: string[] = [];

      for (const row of currentRows) {
        if (!row.field || !row.value) continue;
        const def = fieldDefs.find((d) => d.key === row.field);
        if (!def) continue;

        if (def.kind === "direct" && row.operator === "equals") {
          directFilter[row.field.replace("direct:", "")] = row.value;
        } else {
          const rowTerms = def.kind === "relation" ? row.value.split(",").filter(Boolean) : [row.value];
          for (const t of rowTerms) {
            searchTerms.push(t);
            terms.push(t);
          }
        }
      }

      const query = searchTerms.join(" ");
      onFilterChange({
        query,
        filter: directFilter,
        logic: currentLogic,
        terms: currentLogic === "and" && terms.length > 1 ? terms : undefined,
      });
    },
    [fieldDefs, onFilterChange]
  );

  function addRow() {
    const next = [
      ...rows,
      {
        id: nextId(),
        field: fieldDefs[0]?.key ?? "",
        operator: "equals" as const,
        value: "",
      },
    ];
    setRows(next);
    emit(next, logic);
  }

  function removeRow(id: string) {
    const next = rows.filter((r) => r.id !== id);
    setRows(next);
    emit(next, logic);
  }

  function updateRow(id: string, patch: Partial<FilterRowState>) {
    const next = rows.map((r) => (r.id === id ? { ...r, ...patch } : r));
    setRows(next);
    emit(next, logic);
  }

  function toggleLogic() {
    const next = logic === "and" ? "or" : "and";
    setLogic(next);
    emit(rows, next);
  }

  function getFieldDef(fieldKey: string) {
    return fieldDefs.find((d) => d.key === fieldKey);
  }

  const hasAnyFilter = rows.some((r) => r.value.length > 0);

  function clearAll() {
    setRows([]);
    onFilterChange({ query: "", filter: {} });
  }

  return (
    <div className="space-y-2">
      {rows.length > 0 && (
        <div className="flex items-center gap-2 text-xs text-zinc-500 mb-2">
          <span>Match</span>
          <button
            onClick={toggleLogic}
            className="font-semibold text-blue-600 hover:text-blue-800 uppercase tracking-wide"
          >
            {logic}
          </button>
          <span>of the following:</span>
          {hasAnyFilter && (
            <button
              onClick={clearAll}
              className="ml-auto inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-red-600 hover:bg-red-50 transition-colors"
            >
              <Icon name="x" className="w-3 h-3" /> Clear all
            </button>
          )}
        </div>
      )}

      {rows.map((row) => {
        const def = getFieldDef(row.field);
        return (
          <div key={row.id} className="flex items-center gap-2 flex-wrap">
            <select
              value={row.field}
              onChange={(e) =>
                updateRow(row.id, {
                  field: e.target.value,
                  operator: getFieldDef(e.target.value)?.operators[0] ?? "equals",
                  value: "",
                })
              }
              className="border border-zinc-200 rounded-lg px-2.5 py-1.5 text-sm bg-white min-w-[130px] focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-400 transition-shadow"
            >
              {fieldDefs.map((d) => (
                <option key={d.key} value={d.key}>
                  {d.label}
                </option>
              ))}
            </select>

            <select
              value={row.operator}
              onChange={(e) =>
                updateRow(row.id, { operator: e.target.value as any })
              }
              className="border border-zinc-200 rounded-lg px-2.5 py-1.5 text-sm bg-white min-w-[90px] focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-400 transition-shadow"
            >
              {(def?.operators ?? ["equals"]).map((op) => (
                <option key={op} value={op}>
                  {op === "equals" ? "=" : op === "contains" ? "contains" : "has"}
                </option>
              ))}
            </select>

            {def?.kind === "relation" && def.relationEntity && def.relationField ? (
              <RelationValueSelect
                entityName={def.relationEntity}
                field={def.relationField}
                selected={new Set(row.value ? row.value.split(",") : [])}
                onChange={(s) => updateRow(row.id, { value: [...s].join(",") })}
              />
            ) : def?.control === "select" && def.options ? (
              <select
                value={row.value}
                onChange={(e) => updateRow(row.id, { value: e.target.value })}
                className="border border-zinc-200 rounded-lg px-2.5 py-1.5 text-sm bg-white min-w-[130px] focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-400 transition-shadow"
              >
                <option value="">--</option>
                {def.options.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            ) : (
              <input
                type="text"
                value={row.value}
                onChange={(e) => updateRow(row.id, { value: e.target.value })}
                placeholder="value"
                className="border border-zinc-200 rounded-lg px-2.5 py-1.5 text-sm min-w-[130px] focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-400 transition-shadow"
              />
            )}

            <button
              onClick={() => removeRow(row.id)}
              className="inline-flex items-center justify-center w-7 h-7 rounded-full text-zinc-400 hover:text-red-600 hover:bg-red-50 transition-colors"
            >
              <Icon name="x" className="w-4 h-4" />
            </button>
          </div>
        );
      })}

      <button
        onClick={addRow}
        className="inline-flex items-center gap-1 text-sm text-blue-600 hover:text-blue-800 px-2 py-1 rounded-md hover:bg-blue-50 transition-colors"
      >
        + Add filter
      </button>
    </div>
  );
}
