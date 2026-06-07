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
import { Icon } from "@/components/icon";
import { showToast } from "@/components/toast";
import { Breadcrumbs } from "@/components/breadcrumbs";
import { useT } from "@/lib/i18n";

function EntityCreateContent({
  entity,
  entityName,
}: {
  entity: EntityInfo;
  entityName: string;
}) {
  const router = useRouter();
  const t = useT();
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
    showToast(t("create.created"));
    router.push(`/${entityName}`);
  }

  return (
    <div>
      <Breadcrumbs segments={[{ label: t("breadcrumbs.entities"), href: "/" }, { label: t.entity(entityName), href: `/${entityName}` }, { label: t("breadcrumbs.new") }]} />
      <Card>
        <h1 className="text-xl font-semibold mb-1">{t("create.title", { name: t.entity(entityName) })}</h1>
        <p className="text-xs text-[var(--text-muted)] mb-4">{t("create.subtitle")}</p>
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
