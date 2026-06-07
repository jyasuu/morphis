"use client";

import { useEffect, useState } from "react";
import { useMutation } from "urql";
import { useParams, useRouter } from "next/navigation";
import type { EntityInfo } from "@/lib/types";
import { getEntity } from "@/lib/schema";
import { buildCreateMutation } from "@/lib/query-builder";
import { DynamicForm } from "@/components/dynamic-form";
import { Card } from "@/components/card";
import { Skeleton } from "@/components/skeleton";
import { showToast } from "@/components/toast";

function EntityCreateContent({
  entity,
  entityName,
}: {
  entity: EntityInfo;
  entityName: string;
}) {
  const router = useRouter();
  const createMutation = buildCreateMutation(entity);
  const [, createMut] = useMutation(createMutation);

  async function handleSubmit(values: Record<string, string>) {
    const input: Record<string, unknown> = {};
    for (const f of entity.fields) {
      if (f.kind === "scalar" && !f.autoIncrement) {
        const v = values[f.name];
        input[f.name] = v === "" && f.nullable ? null : v;
      }
    }
    const res = await createMut({ input });
    if (res.error) throw new Error(res.error.message);
    showToast("Created successfully");
    router.push(`/${entityName}`);
  }

  return (
    <div>
      <div className="flex items-center gap-3 mb-4">
        <button
          onClick={() => router.push(`/${entityName}`)}
          className="inline-flex items-center gap-1 text-sm text-zinc-500 hover:text-zinc-700 transition-colors"
        >
          &larr; Back to {entityName}
        </button>
      </div>
      <Card>
        <h1 className="text-xl font-semibold mb-1">New {entityName}</h1>
        <p className="text-xs text-zinc-400 mb-4">Create a new record</p>
        <DynamicForm entity={entity} mode="create" onSubmit={handleSubmit} />
      </Card>
    </div>
  );
}

export default function EntityCreatePage() {
  const params = useParams();
  const entityName = params.entity as string;

  const [entity, setEntity] = useState<EntityInfo | null>(null);

  useEffect(() => {
    getEntity(entityName).then(setEntity);
  }, [entityName]);

  if (!entity) {
    return <div className="space-y-3"><Skeleton className="h-4 w-24" /><Skeleton className="h-6 w-48" /><Skeleton className="h-20 w-full max-w-lg" /></div>;
  }

  return (
    <EntityCreateContent entity={entity} entityName={entityName} />
  );
}
