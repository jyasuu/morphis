"use client";

import type { EntityInfo } from "@/lib/types";
import { getFieldControl } from "@/lib/metadata";
import { StatusBadge } from "./status-badge";
import { EmptyState } from "./empty-state";
import { TableSkeleton } from "./skeleton";

interface Props {
  entity: EntityInfo;
  data: Record<string, unknown>[];
  pkValue: (record: Record<string, unknown>) => string;
  onEdit?: (pk: string) => void;
  onDelete?: (pk: string) => void;
  loading?: boolean;
}

export function DynamicTable({
  entity,
  data,
  pkValue,
  onEdit,
  onDelete,
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
                  className="text-left px-4 py-2 font-medium text-zinc-600"
                >
                  {f.name}
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
              className="border-b last:border-0 hover:bg-zinc-100 even:bg-zinc-50/50"
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
              <td className="px-4 py-2 text-right whitespace-nowrap">
                <button
                  onClick={() => onEdit?.(pkValue(record))}
                  className="inline-flex items-center px-2.5 py-1 text-xs font-medium rounded-full bg-blue-50 text-blue-700 hover:bg-blue-100 transition-colors mr-2"
                >
                  Edit
                </button>
                <button
                  onClick={() => onDelete?.(pkValue(record))}
                  className="inline-flex items-center px-2.5 py-1 text-xs font-medium rounded-full bg-red-50 text-red-700 hover:bg-red-100 transition-colors"
                >
                  Delete
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
