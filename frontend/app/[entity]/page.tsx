"use client";

import { useEffect, useState, useCallback, useMemo, useRef } from "react";
import { useQuery, useMutation, useClient } from "urql";
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
import { getFilterComponent } from "@/components/filters/registry";
import { getRelationFilters, getFilterComponentName } from "@/lib/metadata";
import { showToast } from "@/components/toast";

const PAGE_SIZE = 10;

function intersectByPk<T>(arrays: T[][], pk: string): T[] {
  if (arrays.length === 0) return [];
  const counts = new Map<string, { record: T; count: number }>();
  for (const arr of arrays) {
    const seen = new Set<string>();
    for (const r of arr) {
      const key = String((r as any)[pk]);
      if (!seen.has(key)) {
        seen.add(key);
        const entry = counts.get(key) || { record: r, count: 0 };
        entry.count++;
        counts.set(key, entry);
      }
    }
  }
  return [...counts.values()]
    .filter((e) => e.count === arrays.length)
    .map((e) => e.record);
}

function EntityListContent({
  entity,
  entityName,
}: {
  entity: EntityInfo;
  entityName: string;
}) {
  const router = useRouter();
  const [searchQuery, setSearchQuery] = useState("");
  const [advFilter, setAdvFilter] = useState<{
    query: string;
    filter: Record<string, string>;
    logic?: "and" | "or";
    terms?: string[];
  }>({ query: "", filter: {} });
  const [page, setPage] = useState(0);
  const [andedData, setAndedData] = useState<any[] | null>(null);
  const [andedLoading, setAndedLoading] = useState(false);
  const client = useClient();

  const relFilters = getRelationFilters(entityName);
  const FilterComponent = getFilterComponent(
    getFilterComponentName(entityName)
  );
  const activeSearchQuery = advFilter.query || searchQuery;
  const activeFilter = advFilter.filter;
  const hasActiveFilter = Object.values(activeFilter).some(Boolean);
  const isSearching =
    entity.hasSearch &&
    (activeSearchQuery.length > 0 || hasActiveFilter);
  const isAndSearch = isSearching && advFilter.logic === "and" && (advFilter.terms?.length ?? 0) > 1;

  const listQuery = isAndSearch
    ? null
    : isSearching
      ? buildSearchQuery(entity, !!entity.searchFilterFields?.length)
      : buildListQuery(entity, { limit: PAGE_SIZE });
  const listVars = isAndSearch
    ? {}
    : isSearching
      ? entity.searchFilterFields?.length
        ? { query: activeSearchQuery, filter: activeFilter }
        : { query: activeSearchQuery }
      : { limit: PAGE_SIZE, offset: page * PAGE_SIZE };

  const [result, reexecute] = useQuery({
    query: listQuery ?? "query _ { __typename }",
    variables: listVars as any,
    pause: listQuery === null,
  });

  const [, deleteMut] = useMutation(buildDeleteMutation(entity));

  // Perform AND multi-search with client-side intersection
  const searchQueryStr = useMemo(
    () =>
      entity.hasSearch
        ? buildSearchQuery(entity, !!entity.searchFilterFields?.length)
        : "",
    [entity]
  );
  const prevAndKey = useRef("");
  useEffect(() => {
    if (!isAndSearch || !searchQueryStr) {
      setAndedData(null);
      setAndedLoading(false);
      return;
    }
    const terms = advFilter.terms ?? [];
    const andKey = terms.sort().join(",") + "|" + JSON.stringify(activeFilter);
    if (andKey === prevAndKey.current) return;
    prevAndKey.current = andKey;

    setAndedLoading(true);
    const queries = terms.map((term) =>
      client
        .query(
          searchQueryStr,
          entity.searchFilterFields?.length
            ? { query: term, filter: activeFilter }
            : { query: term }
        )
        .toPromise()
    );
    Promise.all(queries)
      .then((responses) => {
        const key = `search${entityName.charAt(0).toUpperCase() + entityName.slice(1)}`;
        const results = responses.map(
          (r) => (r.data as any)?.[key] || []
        );
        const intersected = intersectByPk(results, entity.primaryKey);
        setAndedData(intersected);
        setAndedLoading(false);
      })
      .catch(() => setAndedLoading(false));
  }, [isAndSearch, advFilter, entity, client, searchQueryStr, entityName, activeFilter]);

  // Reset page when search or filter changes
  useEffect(() => {
    setPage(0);
  }, [searchQuery, advFilter]);

  const data = useMemo(() => {
    if (isAndSearch) return andedData ?? [];
    if (!result.data) return [];
    if (isSearching) {
      const key = `search${entityName.charAt(0).toUpperCase() + entityName.slice(1)}`;
      return (result.data as any)?.[key] || [];
    }
    return (result.data as any)?.[`${entityName}List`] || [];
  }, [result.data, isSearching, isAndSearch, andedData, entityName]);

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
        <div className="mb-4 space-y-3">
          <SearchBar
            onSearch={setSearchQuery}
            placeholder={`Search ${entity.name}...`}
          />
          <FilterComponent
            entityName={entityName}
            filterFields={entity.searchFilterFields ?? []}
            relationFilters={relFilters}
            onFilterChange={(f) => {
              setAdvFilter(f);
              setPage(0);
            }}
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
        loading={result.fetching || andedLoading}
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
