"use client";

import type { EntityInfo } from "@/lib/types";
import { useState } from "react";
import { getFieldControl } from "@/lib/metadata";
import { showToast } from "./toast";
import { Card } from "./card";

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

  function renderField(fieldName: string) {
    const f = scalarFields.find((sf) => sf.name === fieldName)!;
    const ctrl = getFieldControl(entity.name, fieldName);
    const id = `field-${fieldName}`;

    if (ctrl.control === "select" && ctrl.options) {
      return (
        <div key={fieldName}>
          <label htmlFor={id} className="block text-sm font-medium text-zinc-700 mb-1">
            {fieldName}
            {!f.nullable && <span className="text-red-500 ml-1">*</span>}
          </label>
          <select
            name={fieldName}
            id={id}
            value={values[fieldName]}
            onChange={(e) =>
              setValues((prev) => ({ ...prev, [fieldName]: e.target.value }))
            }
            required={!f.nullable}
            className="w-full border rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 bg-white"
          >
            <option value="">{f.nullable ? "--" : "-- Select --"}</option>
            {ctrl.options.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </div>
      );
    }

    return (
      <div key={fieldName}>
        <label htmlFor={id} className="block text-sm font-medium text-zinc-700 mb-1">
          {fieldName}
          {!f.nullable && <span className="text-red-500 ml-1">*</span>}
        </label>
        <input
          type="text"
          name={fieldName}
          id={id}
          value={values[fieldName]}
          onChange={(e) =>
            setValues((prev) => ({ ...prev, [fieldName]: e.target.value }))
          }
          required={!f.nullable}
          className="w-full border rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
      </div>
    );
  }

  return (
    <Card>
      <form onSubmit={handleSubmit} className="space-y-4 max-w-lg">
        {scalarFields.map((f) => renderField(f.name))}
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
    </Card>
  );
}
