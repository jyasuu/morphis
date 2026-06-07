"use client";

import { useEffect, useState, useCallback, useMemo } from "react";
import { useQuery, useMutation } from "urql";
import { useParams, useRouter } from "next/navigation";
import type { EntityInfo } from "@/lib/types";
import { getEntity, getEntityNames } from "@/lib/schema";
import {
  buildListQuery,
  buildDeleteMutation,
  buildSearchQuery,
} from "@/lib/query-builder";
import { DynamicTable } from "@/components/dynamic-table";
import { SearchBar } from "@/components/search-bar";
import { showToast } from "@/components/toast";

const PAGE_SIZE = 10;

function EntityListContent({
  entity,
  entityName,
}: {
  entity: EntityInfo;
  entityName: string;
}) {
  const router = useRouter();
  const [searchQuery, setSearchQuery] = useState("");
  const [page, setPage] = useState(0);

  const isSearching = entity.hasSearch && searchQuery.length > 0;

  const listQuery = isSearching
    ? buildSearchQuery(entity)
    : buildListQuery(entity, { limit: PAGE_SIZE });
  const listVars = isSearching
    ? { query: searchQuery }
    : { limit: PAGE_SIZE, offset: page * PAGE_SIZE };

  const [result, reexecute] = useQuery({
    query: listQuery,
    variables: listVars as any,
  });

  const [, deleteMut] = useMutation(buildDeleteMutation(entity));

  // Reset page when search changes
  useEffect(() => {
    setPage(0);
  }, [searchQuery]);

  const data = useMemo(() => {
    if (!result.data) return [];
    if (isSearching) {
      const key = `search${entityName.charAt(0).toUpperCase() + entityName.slice(1)}`;
      return (result.data as any)?.[key] || [];
    }
    return (result.data as any)?.[`${entityName}List`] || [];
  }, [result.data, isSearching, entityName]);

  const pkValue = useCallback(
    (record: Record<string, unknown>): string => {
      return String(record[entity.primaryKey] ?? "");
    },
    [entity]
  );

  async function handleDelete(pk: string) {
    if (!window.confirm("Delete this record?")) return;
    const res = await deleteMut({ id: pk });
    if (res.error) {
      showToast(`Delete failed: ${res.error.message}`, "error");
    } else {
      showToast("Deleted successfully");
      reexecute({ requestPolicy: "network-only" });
    }
  }

  const hasMore = data.length >= PAGE_SIZE;

  return (
    <div>
      <div className="flex items-center justify-between mb-4">
        <h1 className="text-2xl font-semibold">{entity.name}</h1>
        <button
          onClick={() => router.push(`/${entityName}/new`)}
          className="bg-blue-600 text-white px-4 py-2 rounded-lg text-sm hover:bg-blue-700"
        >
          + New
        </button>
      </div>

      {entity.hasSearch && (
        <div className="mb-4">
          <SearchBar
            onSearch={setSearchQuery}
            placeholder={`Search ${entity.name}...`}
          />
        </div>
      )}

      <DynamicTable
        entity={entity}
        data={data}
        pkValue={pkValue}
        onEdit={(pk) =>
          router.push(`/${entityName}/${encodeURIComponent(pk)}`)
        }
        onDelete={handleDelete}
        loading={result.fetching}
      />

      {!isSearching && (
        <div className="flex items-center justify-center gap-3 mt-4">
          <button
            onClick={() => setPage((p) => Math.max(0, p - 1))}
            disabled={page === 0}
            className="px-3 py-1 text-sm border rounded-lg disabled:opacity-30 hover:bg-zinc-50"
          >
            &larr; Previous
          </button>
          <span className="text-sm text-zinc-500">Page {page + 1}</span>
          <button
            onClick={() => setPage((p) => p + 1)}
            disabled={!hasMore}
            className="px-3 py-1 text-sm border rounded-lg disabled:opacity-30 hover:bg-zinc-50"
          >
            Next &rarr;
          </button>
        </div>
      )}
    </div>
  );
}

export default function EntityListPage() {
  const params = useParams();
  const entityName = params.entity as string;

  const [entity, setEntity] = useState<EntityInfo | null>(null);
  const [notFound, setNotFound] = useState(false);

  useEffect(() => {
    getEntity(entityName).then((e) => {
      if (e) {
        setEntity(e);
      } else {
        getEntityNames().then((names) => {
          if (names.length > 0 && !names.includes(entityName)) {
            setNotFound(true);
          }
        });
      }
    });
  }, [entityName]);

  if (notFound) {
    return (
      <div>
        <h1 className="text-2xl font-semibold mb-2">Not Found</h1>
        <p className="text-zinc-500 text-sm">
          Entity &quot;{entityName}&quot; does not exist.
        </p>
        <a
          href="/"
          className="text-blue-600 hover:underline text-sm mt-2 inline-block"
        >
          &larr; Back to entities
        </a>
      </div>
    );
  }

  if (!entity) {
    return (
      <div className="text-zinc-500 text-sm">Loading entity info...</div>
    );
  }

  return <EntityListContent entity={entity} entityName={entityName} />;
}
