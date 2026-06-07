"use client";

import { useEffect, useState, useCallback } from "react";
import { useQuery, useMutation } from "urql";
import { useParams, useRouter } from "next/navigation";
import type { EntityInfo } from "@/lib/types";
import { getEntity, getCachedEntity } from "@/lib/schema";
import { buildDetailQuery, buildUpdateMutation } from "@/lib/query-builder";
import { DynamicForm } from "@/components/dynamic-form";
import { RelationPanel } from "@/components/relation-panel";
import { Card } from "@/components/card";
import { EmptyState } from "@/components/empty-state";
import { Skeleton } from "@/components/skeleton";
import { Icon } from "@/components/icon";
import { showToast } from "@/components/toast";
import { getPermissions } from "@/lib/metadata";

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

  const perms = getPermissions(entityName);

  const handleMutation = useCallback(() => {
    reexecute({ requestPolicy: "network-only" });
  }, [reexecute]);

  if (result.fetching) {
    return <div className="p-4"><div className="skeleton h-5 w-48 mb-3" /><div className="skeleton h-20 w-full" /></div>;
  }

  const record = (result.data as any)?.[entityName] as
    | Record<string, unknown>
    | undefined;

  if (!record) {
    return <EmptyState icon="search" title="Record not found" description={`No ${entityName} with ID "${id}" exists`} />;
  }

  return (
    <div>
      <div className="flex items-center gap-3 mb-4">
        <button
          onClick={() => router.push(`/${entityName}`)}
          className="inline-flex items-center gap-1 text-sm text-zinc-500 hover:text-zinc-700 transition-colors"
        >
          <Icon name="arrow-left" className="w-4 h-4" /> Back to {entityName}
        </button>
      </div>
      <div className="space-y-6">
        <Card>
          <h1 className="text-xl font-semibold mb-1">
            {entityName}: {String(record[entity.primaryKey] ?? id)}
          </h1>
          {perms.update ? (
            <>
              <p className="text-xs text-zinc-400 mb-4">Edit record</p>
              <DynamicForm
                entity={entity}
                initial={record}
                mode="edit"
                onSubmit={handleSubmit}
              />
            </>
          ) : (
            <div className="text-sm text-zinc-600 space-y-1">
              {entity.fields.filter((f) => f.kind === "scalar" && !f.autoIncrement).map((f) => (
                <div key={f.name} className="flex gap-2">
                  <span className="font-medium text-zinc-500 min-w-[120px]">{f.name}</span>
                  <span>{String(record[f.name] ?? "")}</span>
                </div>
              ))}
            </div>
          )}
        </Card>

        {hasManyFields.length > 0 && (
          <Card>
            <h2 className="text-lg font-semibold mb-4">Related</h2>
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
                  onView={(relName, relPk) => router.push(`/${relName}/${encodeURIComponent(relPk)}`)}
                />
              );
            })}
          </Card>
        )}
      </div>
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
    return <div className="space-y-3"><Skeleton className="h-4 w-24" /><Skeleton className="h-6 w-48" /><Skeleton className="h-32 w-full" /></div>;
  }

  return (
    <EntityDetailContent
      entity={entity}
      entityName={entityName}
      id={id}
    />
  );
}
