"use client";

import { useEffect, useState, useCallback } from "react";
import { useQuery, useMutation } from "urql";
import { useParams, useRouter } from "next/navigation";
import type { EntityInfo } from "@/lib/types";
import { getEntity, getCachedEntity } from "@/lib/schema";
import { buildDetailQuery, buildUpdateMutation } from "@/lib/query-builder";
import { DynamicForm } from "@/components/dynamic-form";
import { RelationPanel } from "@/components/relation-panel";
import { showToast } from "@/components/toast";

function EntityDetailContent({
  entity,
  entityName,
  id,
}: {
  entity: EntityInfo;
  entityName: string;
  id: string;
}) {
  const router = useRouter();
  const detailQuery = buildDetailQuery(entity, getCachedEntity);
  const updateMutation = buildUpdateMutation(entity);

  const [result, reexecute] = useQuery({
    query: detailQuery,
    variables: { id },
  });

  const [, updateMut] = useMutation(updateMutation);

  const hasManyFields = entity.fields.filter((f) => f.kind === "has_many");

  async function handleSubmit(values: Record<string, string>) {
    const input: Record<string, unknown> = {};
    for (const f of entity.fields) {
      if (f.kind === "scalar" && !f.autoIncrement) {
        const v = values[f.name];
        input[f.name] = v === "" && f.nullable ? null : v;
      }
    }
    const res = await updateMut({ id, input });
    if (res.error) throw new Error(res.error.message);
    showToast("Updated successfully");
    router.push(`/${entityName}`);
  }

  const handleMutation = useCallback(() => {
    reexecute({ requestPolicy: "network-only" });
  }, [reexecute]);

  if (result.fetching) {
    return <div className="text-zinc-500 text-sm">Loading record...</div>;
  }

  const record = (result.data as any)?.[entityName] as
    | Record<string, unknown>
    | undefined;

  if (!record) {
    return <div className="text-red-500 text-sm">Record not found.</div>;
  }

  return (
    <div>
      <div className="flex items-center gap-3 mb-4">
        <button
          onClick={() => router.push(`/${entityName}`)}
          className="text-sm text-blue-600 hover:underline"
        >
          &larr; Back to {entityName}
        </button>
      </div>
      <h1 className="text-2xl font-semibold mb-4">
        {entityName}: {String(record[entity.primaryKey] ?? id)}
      </h1>
      <DynamicForm
        entity={entity}
        initial={record}
        mode="edit"
        onSubmit={handleSubmit}
      />

      {hasManyFields.length > 0 && (
        <div className="mt-8 border-t pt-6">
          <h2 className="text-xl font-semibold mb-4">Related</h2>
          {hasManyFields.map((f) => {
            const relatedRecords = (record[f.name] as Record<string, unknown>[]) ?? [];
            return (
              <RelationPanel
                key={f.name}
                entity={entity}
                field={f}
                parentPkValue={String(record[entity.primaryKey] ?? id)}
                records={relatedRecords}
                entityLookup={getCachedEntity}
                onMutation={handleMutation}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

export default function EntityDetailPage() {
  const params = useParams();
  const router = useRouter();
  const entityName = params.entity as string;
  const id = params.id as string;

  const [entity, setEntity] = useState<EntityInfo | null>(null);

  useEffect(() => {
    getEntity(entityName).then(setEntity);
  }, [entityName]);

  if (!entity) {
    return <div className="text-zinc-500 text-sm">Loading...</div>;
  }

  return (
    <EntityDetailContent
      entity={entity}
      entityName={entityName}
      id={id}
    />
  );
}
