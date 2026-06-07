"use client";

import type { EntityInfo } from "@/lib/types";
import { useState } from "react";
import { showToast } from "./toast";

interface Props {
  entity: EntityInfo;
  initial?: Record<string, unknown>;
  mode: "create" | "edit";
  onSubmit: (values: Record<string, string>) => Promise<void>;
}

export function DynamicForm({ entity, initial, mode, onSubmit }: Props) {
  const scalarFields = entity.fields.filter(
    (f) => f.kind === "scalar" && !f.autoIncrement
  );
  const [values, setValues] = useState<Record<string, string>>(() => {
    const v: Record<string, string> = {};
    for (const f of scalarFields) {
      v[f.name] = initial?.[f.name] != null ? String(initial[f.name]) : "";
    }
    return v;
  });
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      await onSubmit(values);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : "Submission failed";
      setError(msg);
      showToast(msg, "error");
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-4 max-w-lg">
      {scalarFields.map((f) => (
        <div key={f.name}>
          <label htmlFor={f.name} className="block text-sm font-medium text-zinc-700 mb-1">
            {f.name}
            {!f.nullable && (
              <span className="text-red-500 ml-1">*</span>
            )}
          </label>
          <input
            type="text"
            name={f.name}
            id={f.name}
            value={values[f.name]}
            onChange={(e) =>
              setValues((prev) => ({ ...prev, [f.name]: e.target.value }))
            }
            required={!f.nullable}
            className="w-full border rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
        </div>
      ))}
      {error && (
        <div className="text-red-600 text-sm">{error}</div>
      )}
      <button
        type="submit"
        disabled={submitting}
        className="bg-blue-600 text-white px-4 py-2 rounded-lg text-sm hover:bg-blue-700 disabled:opacity-50"
      >
        {submitting ? "Saving..." : mode === "create" ? "Create" : "Update"}
      </button>
    </form>
  );
}
