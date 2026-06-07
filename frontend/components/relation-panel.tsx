"use client";

import { useState, useMemo, useCallback } from "react";
import { useMutation } from "urql";
import type { EntityInfo, FieldInfo } from "@/lib/types";
import { buildCreateMutation, buildUpdateMutation, buildDeleteMutation } from "@/lib/query-builder";
import { DynamicTable } from "./dynamic-table";
import { DynamicForm } from "./dynamic-form";
import { Modal } from "./modal";
import { showToast } from "./toast";
import { getPermissions } from "@/lib/metadata";
import { ConfirmDialog } from "./confirm-dialog";

interface Props {
  entity: EntityInfo;
  field: FieldInfo;
  parentPkValue: string;
  records: Record<string, unknown>[];
  entityLookup: (name: string) => EntityInfo | null;
  onMutation: () => void;
  onView?: (entityName: string, pk: string) => void;
  basePath?: string;
}

export function RelationPanel({
  entity,
  field,
  parentPkValue,
  records,
  entityLookup,
  onMutation,
  onView,
}: Props) {
  const [modalOpen, setModalOpen] = useState<"create" | "edit" | null>(null);
  const [editingRecord, setEditingRecord] = useState<Record<string, unknown> | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);

  const relatedEntity = entityLookup(field.relatedEntity!);
  const relatedPk = relatedEntity?.primaryKey ?? "id";

  const relatedPerms = relatedEntity ? getPermissions(relatedEntity.name) : null;

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

  function handleDeleteClick(pk: string) {
    setDeleteTarget(pk);
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    const res = await deleteMut({ id: deleteTarget });
    setDeleteTarget(null);
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
        {relatedPerms?.create !== false && (
          <button
            onClick={() => {
              setEditingRecord(null);
              setModalOpen("create");
            }}
            className="text-sm bg-[#0d9488] text-white px-3 py-1 rounded-lg hover:bg-[#0f766e]"
          >
            + New
          </button>
        )}
      </div>

      <DynamicTable
        entity={relatedEntity}
        data={records}
        pkValue={pkValue}
        onRowClick={onView ? (pk) => onView(relatedEntity.name, pk) : undefined}
        onView={onView ? (pk) => onView(relatedEntity.name, pk) : undefined}
        onEdit={relatedPerms?.update !== false ? (pk) => {
          const rec = records.find((r) => String(r[relatedPk]) === pk) ?? null;
          setEditingRecord(rec);
          setModalOpen("edit");
        } : undefined}
        onDelete={relatedPerms?.delete !== false ? handleDeleteClick : undefined}
        perm={{ update: relatedPerms?.update, delete: relatedPerms?.delete }}
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

      <ConfirmDialog
        open={deleteTarget !== null}
        title="Delete record"
        message="Are you sure you want to delete this record?"
        onConfirm={confirmDelete}
        onCancel={() => setDeleteTarget(null)}
      />
    </div>
  );
}
