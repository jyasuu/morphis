"use client";

import type { EntityInfo } from "@/lib/types";
import { useState } from "react";
import { getFieldControl } from "@/lib/metadata";
import { showToast } from "./toast";
import { Card } from "./card";
import { useT } from "@/lib/i18n";

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
  const visibleFields = scalarFields.filter(
    (f) => !getFieldControl(entity.name, f.name).hidden
  );
  const [values, setValues] = useState<Record<string, string>>(() => {
    const v: Record<string, string> = {};
    for (const f of scalarFields) {
      v[f.name] = initial?.[f.name] != null ? String(initial[f.name]) : "";
    }
    return v;
  });
  const t = useT();
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      await onSubmit(values);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : t("form.submissionFailed");
      setError(msg);
      showToast(msg, "error");
    } finally {
      setSubmitting(false);
    }
  }

  function renderField(fieldName: string) {
    const f = visibleFields.find((sf) => sf.name === fieldName)!;
    const ctrl = getFieldControl(entity.name, fieldName);
    const id = `field-${fieldName}`;

    if (ctrl.control === "select" && ctrl.options) {
      return (
        <div key={fieldName}>
          <label htmlFor={id} className="block text-sm font-medium text-[var(--text-secondary)] mb-1">
            {t.field(entity.name, fieldName)}
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
            className="w-full border border-[var(--border)] rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 bg-[var(--surface)]"
          >
            <option value="">{f.nullable ? t("form.emptyOption") : t("form.selectOption")}</option>
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
        <label htmlFor={id} className="block text-sm font-medium text-[var(--text-secondary)] mb-1">
          {t.field(entity.name, fieldName)}
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
          className="w-full border border-[var(--border)] rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 bg-[var(--surface)]"
        />
      </div>
    );
  }

  return (
    <Card>
      <form onSubmit={handleSubmit} className="space-y-4 max-w-lg">
        {visibleFields.map((f) => renderField(f.name))}
        {error && (
          <div className="text-red-600 text-sm">{error}</div>
        )}
        <button
          type="submit"
          disabled={submitting}
          className="bg-[#0d9488] text-white px-4 py-2 rounded-lg text-sm hover:bg-[#0f766e] disabled:opacity-50"
        >
          {submitting ? t("form.saving") : mode === "create" ? t("form.create") : t("form.update")}
        </button>
      </form>
    </Card>
  );
}
