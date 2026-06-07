"use client";

import type { EntityInfo } from "@/lib/types";
import { getFieldControl } from "@/lib/metadata";
import { StatusBadge } from "./status-badge";
import { EmptyState } from "./empty-state";
import { TableSkeleton } from "./skeleton";
import { Icon } from "./icon";

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

export function DynamicTable({
  entity,
  data,
  pkValue,
  onEdit,
  onDelete,
  onView,
  onSort,
  onRowClick,
  sortField,
  sortDir,
  perm,
  loading,
}: Props) {
  const scalarFields = entity.fields.filter((f) => f.kind === "scalar");
  const hiddenFields = new Set(entity.autoIncrementFields);

  if (loading) {
    return <div className="p-4"><TableSkeleton rows={6} cols={scalarFields.length} /></div>;
  }

  if (data.length === 0) {
    return (
      <div className="border-b border-zinc-100">
        <EmptyState title="No records found" description="Try adjusting your search or filters" />
      </div>
    );
  }

  return (
    <div className="overflow-x-auto border rounded-lg">
      <table className="min-w-full text-sm">
        <thead>
          <tr className="bg-zinc-100 border-b">
            {scalarFields
              .filter((f) => !hiddenFields.has(f.name))
              .map((f) => (
                <th
                  key={f.name}
                  className={`text-left px-4 py-2 font-medium text-zinc-600 ${
                    onSort ? "cursor-pointer hover:bg-zinc-200 select-none" : ""
                  }`}
                  onClick={() => onSort?.(f.name)}
                >
                  <span className="inline-flex items-center gap-1">
                    {f.name}
                    {sortField === f.name && (
                      <Icon name={sortDir === "asc" ? "chevron-up" : "chevron-down"} className="w-3 h-3" />
                    )}
                  </span>
                </th>
              ))}
            <th className="px-4 py-2 font-medium text-zinc-600 text-right">
              Actions
            </th>
          </tr>
        </thead>
        <tbody>
          {data.map((record, i) => (
            <tr
              key={pkValue(record) || i}
              className={`border-b last:border-0 ${
                onRowClick ? "cursor-pointer" : ""
              } hover:bg-zinc-100 even:bg-zinc-50/50`}
              onClick={() => onRowClick?.(pkValue(record))}
            >
              {scalarFields
                .filter((f) => !hiddenFields.has(f.name))
                .map((f) => {
                  const ctrl = getFieldControl(entity.name, f.name);
                  const isSelect = ctrl.control === "select" && ctrl.options;
                  return (
                    <td key={f.name} className="px-4 py-2 text-zinc-800">
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
                    className="inline-flex items-center px-2.5 py-1 text-xs font-medium rounded-full bg-zinc-100 text-zinc-600 hover:bg-zinc-200 transition-colors mr-1"
                  >
                    View
                  </button>
                )}
                {perm?.update !== false && onEdit && (
                  <button
                    onClick={() => onEdit(pkValue(record))}
                    className="inline-flex items-center px-2.5 py-1 text-xs font-medium rounded-full bg-[#0d9488]/10 text-[#0d9488] hover:bg-[#0d9488]/20 transition-colors mr-1"
                  >
                    Edit
                  </button>
                )}
                {perm?.delete !== false && onDelete && (
                  <button
                    onClick={() => onDelete(pkValue(record))}
                    className="inline-flex items-center px-2.5 py-1 text-xs font-medium rounded-full bg-red-50 text-red-700 hover:bg-red-100 transition-colors"
                  >
                    Delete
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
