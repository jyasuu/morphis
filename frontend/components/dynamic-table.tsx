"use client";

import { useState, useRef, useEffect, useCallback } from "react";
import type { EntityInfo } from "@/lib/types";
import { getFieldControl } from "@/lib/metadata";
import { StatusBadge } from "./status-badge";
import { EmptyState } from "./empty-state";
import { TableSkeleton } from "./skeleton";
import { Icon } from "./icon";
import { useT } from "@/lib/i18n";

interface Props {
  entity: EntityInfo;
  data: Record<string, unknown>[];
  pkValue: (record: Record<string, unknown>) => string;
  onEdit?: (pk: string) => void;
  onDelete?: (pk: string) => void;
  onView?: (pk: string) => void;
  onRowClick?: (pk: string) => void;
  onSort?: (field: string) => void;
  sortField?: string;
  sortDir?: "asc" | "desc";
  perm?: { update?: boolean; delete?: boolean };
  loading?: boolean;
}

const STORAGE_KEY = "dt-columns";

function loadColumnVisibility(entityName: string, visible: string[]): Set<string> {
  try {
    const raw = localStorage.getItem(`${STORAGE_KEY}:${entityName}`);
    if (raw) {
      const saved = JSON.parse(raw) as string[];
      const valid = saved.filter((n) => visible.includes(n));
      if (valid.length > 0) return new Set(valid);
    }
  } catch { /* ignore */ }
  return new Set(visible);
}

function saveColumnVisibility(entityName: string, cols: Set<string>) {
  try {
    localStorage.setItem(`${STORAGE_KEY}:${entityName}`, JSON.stringify([...cols]));
  } catch { /* ignore */ }
}

export function DynamicTable({
  entity,
  data,
  pkValue,
  onEdit,
  onDelete,
  onView,
  onRowClick,
  onSort,
  sortField,
  sortDir,
  perm,
  loading,
}: Props) {
  const t = useT();
  const scalarFields = entity.fields.filter((f) => f.kind === "scalar");
  const hiddenFields = new Set([
    ...entity.autoIncrementFields,
    ...scalarFields.filter((f) => getFieldControl(entity.name, f.name).hidden).map((f) => f.name),
  ]);
  const defaultVisible = scalarFields
    .filter((f) => !hiddenFields.has(f.name))
    .map((f) => f.name);

  const [visibleColumns, setVisibleColumns] = useState<Set<string>>(() =>
    loadColumnVisibility(entity.name, defaultVisible)
  );
  const [showColumnMenu, setShowColumnMenu] = useState(false);
  const columnMenuRef = useRef<HTMLDivElement>(null);
  const [colWidths, setColWidths] = useState<Record<string, number>>({});
  const resizing = useRef<{ name: string; startX: number; startW: number } | null>(null);

  const visibleScalarFields = scalarFields.filter((f) => visibleColumns.has(f.name));

  useEffect(() => {
    saveColumnVisibility(entity.name, visibleColumns);
  }, [entity.name, visibleColumns]);

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (columnMenuRef.current && !columnMenuRef.current.contains(e.target as Node)) {
        setShowColumnMenu(false);
      }
    }
    if (showColumnMenu) document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [showColumnMenu]);

  const startResize = useCallback((e: React.MouseEvent, name: string) => {
    e.preventDefault();
    resizing.current = { name, startX: e.clientX, startW: colWidths[name] || 120 };
    const handler = (ev: MouseEvent) => {
      if (!resizing.current) return;
      const diff = Math.max(60, resizing.current.startW + (ev.clientX - resizing.current.startX));
      setColWidths((prev) => ({ ...prev, [name]: diff }));
    };
    const stop = () => {
      resizing.current = null;
      document.removeEventListener("mousemove", handler);
      document.removeEventListener("mouseup", stop);
    };
    document.addEventListener("mousemove", handler);
    document.addEventListener("mouseup", stop);
  }, [colWidths]);

  function toggleColumn(name: string) {
    setVisibleColumns((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name); else next.add(name);
      return next;
    });
  }

  function exportCsv() {
    const header = visibleScalarFields.map((f) => f.name);
    const rows = data.map((r) => header.map((h) => {
      const v = r[h];
      const s = v == null ? "" : String(v);
      return s.includes(",") || s.includes('"') || s.includes("\n")
        ? `"${s.replace(/"/g, '""')}"`
        : s;
    }));
    const csv = [header.join(","), ...rows.map((r) => r.join(","))].join("\n");
    const blob = new Blob([csv], { type: "text/csv;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${entity.name}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }

  if (loading) {
    return <div className="p-4"><TableSkeleton rows={6} cols={Math.max(1, visibleScalarFields.length)} /></div>;
  }

  if (data.length === 0) {
    return (
      <div className="border-b border-[var(--border-light)]">
        <EmptyState title={t("list.noRecords")} description={t("list.noRecordsHint")} />
      </div>
    );
  }

  return (
    <div>
      <div className="flex items-center justify-end gap-2 px-4 py-2 border-b border-[var(--border-light)] bg-[var(--surface)]">
        <div className="relative" ref={columnMenuRef}>
          <button
            onClick={() => setShowColumnMenu((v) => !v)}
            className="inline-flex items-center gap-1 px-2.5 py-1 text-xs font-medium rounded-lg border border-[var(--border)] text-[var(--text-secondary)] hover:bg-[var(--muted)] transition-colors"
          >
            <Icon name="eye" className="w-3.5 h-3.5" />
            {t("table.columns")}
          </button>
          {showColumnMenu && (
            <div className="absolute right-0 top-full mt-1 bg-[var(--surface)] border border-[var(--border)] rounded-lg shadow-lg z-50 min-w-[160px] py-1">
              {defaultVisible.map((name) => {
                const field = scalarFields.find((f) => f.name === name);
                return (
                  <label
                    key={name}
                    className="flex items-center gap-2 px-3 py-1.5 text-sm cursor-pointer hover:bg-[var(--muted)] transition-colors"
                  >
                    <input
                      type="checkbox"
                      checked={visibleColumns.has(name)}
                      onChange={() => toggleColumn(name)}
                      className="rounded border-[var(--border)] text-[#0d9488] focus:ring-[#0d9488]/30"
                    />
                    {t.field(entity.name, name)}
                  </label>
                );
              })}
            </div>
          )}
        </div>
        <button
          onClick={exportCsv}
          className="inline-flex items-center gap-1 px-2.5 py-1 text-xs font-medium rounded-lg border border-[var(--border)] text-[var(--text-secondary)] hover:bg-[var(--muted)] transition-colors"
        >
          <Icon name="file-doc" className="w-3.5 h-3.5" />
          CSV
        </button>
      </div>
      <div className="overflow-x-auto border border-[var(--border)] rounded-lg">
        <table className="min-w-full text-sm">
          <thead>
            <tr className="bg-[var(--muted)] border-b border-[var(--border)]">
              {visibleScalarFields.map((f) => (
                <th
                  key={f.name}
                  className={`text-left px-4 py-2 font-medium text-[var(--text-secondary)] relative select-none ${
                    onSort ? "cursor-pointer hover:bg-[var(--hover)]" : ""
                  }`}
                  style={{ width: colWidths[f.name] ?? undefined }}
                  onClick={() => onSort?.(f.name)}
                >
                  <span className="inline-flex items-center gap-1">
                    {t.field(entity.name, f.name)}
                    {sortField === f.name && (
                      <Icon name={sortDir === "asc" ? "chevron-up" : "chevron-down"} className="w-3 h-3" />
                    )}
                  </span>
                  <div
                    className="absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-[#0d9488]/50"
                    onMouseDown={(e) => startResize(e, f.name)}
                  />
                </th>
              ))}
              <th className="px-4 py-2 font-medium text-[var(--text-secondary)] text-right">
                {t("table.actions")}
              </th>
            </tr>
          </thead>
          <tbody>
            {data.map((record, i) => (
              <tr
                key={pkValue(record) || i}
                className={`border-b border-[var(--border)] last:border-0 ${
                  onRowClick ? "cursor-pointer" : ""
                } hover:bg-[var(--muted)] even:bg-[var(--surface)] odd:bg-[var(--surface)]`}
                onClick={() => onRowClick?.(pkValue(record))}
              >
                {visibleScalarFields.map((f) => {
                  const ctrl = getFieldControl(entity.name, f.name);
                  const isSelect = ctrl.control === "select" && ctrl.options;
                  return (
                    <td key={f.name} className="px-4 py-2 text-[var(--text)] truncate max-w-[250px]">
                      {isSelect ? (
                        <StatusBadge value={String(record[f.name] ?? "")} />
                      ) : (
                        String(record[f.name] ?? "")
                      )}
                    </td>
                  );
                })}
                <td className="px-4 py-2 text-right whitespace-nowrap" onClick={(e) => e.stopPropagation()}>
                  {onView && (
                    <button
                      onClick={() => onView(pkValue(record))}
                      className="inline-flex items-center px-2.5 py-1 text-xs font-medium rounded-full bg-[var(--muted)] text-[var(--text-secondary)] hover:bg-[var(--hover)] transition-colors mr-1"
                    >
                      {t("table.view")}
                    </button>
                  )}
                  {perm?.update !== false && onEdit && (
                    <button
                      onClick={() => onEdit(pkValue(record))}
                      className="inline-flex items-center px-2.5 py-1 text-xs font-medium rounded-full bg-[#0d9488]/10 text-[#0d9488] hover:bg-[#0d9488]/20 transition-colors mr-1"
                    >
                      {t("table.edit")}
                    </button>
                  )}
                  {perm?.delete !== false && onDelete && (
                    <button
                      onClick={() => onDelete(pkValue(record))}
                      className="inline-flex items-center px-2.5 py-1 text-xs font-medium rounded-full bg-red-50 text-red-700 hover:bg-red-100 transition-colors"
                    >
                      {t("table.delete")}
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
