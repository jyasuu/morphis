"use client";

import type { EntityInfo } from "@/lib/types";
import { getFieldControl } from "@/lib/metadata";
import { StatusBadge } from "./status-badge";

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
    return (
      <div className="text-zinc-500 p-4 text-sm">
        Loading...
      </div>
    );
  }

  if (data.length === 0) {
    return (
      <div className="text-zinc-400 p-4 text-sm border rounded-lg">
        No records found.
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
              <td className="px-4 py-2 text-right">
                <button
                  onClick={() => onEdit?.(pkValue(record))}
                  className="text-blue-600 hover:underline mr-3"
                >
                  Edit
                </button>
                <button
                  onClick={() => onDelete?.(pkValue(record))}
                  className="text-red-600 hover:underline"
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
