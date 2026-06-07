"use client";

import { useState, useMemo, useCallback } from "react";
import { useMutation } from "urql";
import type { EntityInfo, FieldInfo } from "@/lib/types";
import { buildCreateMutation, buildUpdateMutation, buildDeleteMutation } from "@/lib/query-builder";
import { DynamicTable } from "./dynamic-table";
import { DynamicForm } from "./dynamic-form";
import { Modal } from "./modal";
import { showToast } from "./toast";

interface Props {
  entity: EntityInfo;
  field: FieldInfo;
  parentPkValue: string;
  records: Record<string, unknown>[];
  entityLookup: (name: string) => EntityInfo | null;
  onMutation: () => void;
}

export function RelationPanel({
  entity,
  field,
  parentPkValue,
  records,
  entityLookup,
  onMutation,
}: Props) {
  const [modalOpen, setModalOpen] = useState<"create" | "edit" | null>(null);
  const [editingRecord, setEditingRecord] = useState<Record<string, unknown> | null>(null);

  const relatedEntity = entityLookup(field.relatedEntity!);
  const relatedPk = relatedEntity?.primaryKey ?? "id";

  const createMutation = relatedEntity ? buildCreateMutation(relatedEntity) : "";
  const updateMutation = relatedEntity ? buildUpdateMutation(relatedEntity) : "";
  const deleteMutation = relatedEntity ? buildDeleteMutation(relatedEntity) : "";

  const [, createMut] = useMutation(createMutation);
  const [, updateMut] = useMutation(updateMutation);
  const [, deleteMut] = useMutation(deleteMutation);

  async function handleCreate(values: Record<string, string>) {
    if (!relatedEntity) return;
    const input: Record<string, unknown> = {};
    for (const f of relatedEntity.fields) {
      if (f.kind === "scalar" && !f.autoIncrement) {
        const v = values[f.name];
        input[f.name] = v === "" && f.nullable ? null : v;
      }
    }
    const res = await createMut({ input });
    if (res.error) throw new Error(res.error.message);
    showToast("Created successfully");
    setModalOpen(null);
    onMutation();
  }

  async function handleEdit(values: Record<string, string>) {
    if (!relatedEntity) return;
    const input: Record<string, unknown> = {};
    for (const f of relatedEntity.fields) {
      if (f.kind === "scalar" && !f.autoIncrement) {
        const v = values[f.name];
        input[f.name] = v === "" && f.nullable ? null : v;
      }
    }
    const pk = editingRecord?.[relatedPk];
    if (!pk) return;
    const res = await updateMut({ id: String(pk), input });
    if (res.error) throw new Error(res.error.message);
    showToast("Updated successfully");
    setModalOpen(null);
    setEditingRecord(null);
    onMutation();
  }

  async function handleDelete(pk: string) {
    if (!window.confirm("Delete this record?")) return;
    const res = await deleteMut({ id: pk });
    if (res.error) {
      showToast(`Delete failed: ${res.error.message}`, "error");
    } else {
      showToast("Deleted successfully");
      onMutation();
    }
  }

  const pkValue = useCallback(
    (record: Record<string, unknown>): string => {
      return String(record[relatedPk] ?? "");
    },
    [relatedPk]
  );

  const createInitial = useMemo(() => {
    if (!relatedEntity) return undefined;
    const auto: Record<string, unknown> = {};
    for (const f of relatedEntity.fields) {
      if (f.kind === "scalar" && f.name === entity.primaryKey) {
        auto[f.name] = parentPkValue;
      }
    }
    return Object.keys(auto).length > 0 ? auto : undefined;
  }, [relatedEntity, entity.primaryKey, parentPkValue]);

  if (!relatedEntity) return null;

  return (
    <div className="mb-6">
      <div className="flex items-center justify-between mb-2">
        <h3 className="text-lg font-medium">{field.name}</h3>
        <button
          onClick={() => {
            setEditingRecord(null);
            setModalOpen("create");
          }}
          className="text-sm bg-blue-600 text-white px-3 py-1 rounded-lg hover:bg-blue-700"
        >
          + New
        </button>
      </div>

      <DynamicTable
        entity={relatedEntity}
        data={records}
        pkValue={pkValue}
        onEdit={(pk) => {
          const rec = records.find((r) => String(r[relatedPk]) === pk) ?? null;
          setEditingRecord(rec);
          setModalOpen("edit");
        }}
        onDelete={handleDelete}
      />

      <Modal
        isOpen={modalOpen === "create"}
        onClose={() => setModalOpen(null)}
        title={`New ${relatedEntity.name}`}
      >
        <DynamicForm
          entity={relatedEntity}
          initial={createInitial}
          mode="create"
          onSubmit={handleCreate}
        />
      </Modal>

      <Modal
        isOpen={modalOpen === "edit"}
        onClose={() => {
          setModalOpen(null);
          setEditingRecord(null);
        }}
        title={`Edit ${relatedEntity.name}`}
      >
        {editingRecord && (
          <DynamicForm
            entity={relatedEntity}
            initial={editingRecord}
            mode="edit"
            onSubmit={handleEdit}
          />
        )}
      </Modal>
    </div>
  );
}
