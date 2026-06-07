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
import { Card } from "@/components/card";
import { getFilterComponent } from "@/components/filters/registry";
import { getRelationFilters, getFilterComponentName, getPermissions } from "@/lib/metadata";
import { showToast } from "@/components/toast";
import { EmptyState } from "@/components/empty-state";
import { Skeleton } from "@/components/skeleton";
import { Icon } from "@/components/icon";
import { useConfirm } from "@/components/confirm-dialog";
import { Breadcrumbs } from "@/components/breadcrumbs";

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
  const [sortField, setSortField] = useState<string | undefined>();
  const [sortDir, setSortDir] = useState<"asc" | "desc" | undefined>();
  const [andedData, setAndedData] = useState<any[] | null>(null);
  const [andedLoading, setAndedLoading] = useState(false);
  const client = useClient();
  const confirm = useConfirm();

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
      : buildListQuery(entity, { limit: PAGE_SIZE, sortField, sortDir });
  const listVars = isAndSearch
    ? {}
    : isSearching
      ? entity.searchFilterFields?.length
        ? { query: activeSearchQuery, filter: activeFilter }
        : { query: activeSearchQuery }
      : { limit: PAGE_SIZE, offset: page * PAGE_SIZE, ...(sortField ? { order_by: `${sortDir === "desc" ? "-" : ""}${sortField}` } : {}) };

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
    const ok = await confirm.confirm("Delete record", "Are you sure you want to delete this record?");
    if (!ok) return;
    const res = await deleteMut({ id: pk });
    if (res.error) {
      showToast(`Delete failed: ${res.error.message}`, "error");
    } else {
      showToast("Deleted successfully");
      reexecute({ requestPolicy: "network-only" });
    }
  }

  function handleSort(field: string) {
    setSortField(field);
    setSortDir((prev) => (prev === "asc" ? "desc" : "asc"));
    setPage(0);
  }

  const perms = getPermissions(entityName);
  const hasMore = data.length >= PAGE_SIZE;

  return (
    <div>
      <Breadcrumbs segments={[{ label: "Entities", href: "/" }, { label: entity.name }]} />
      <div className="flex items-center justify-between mb-4">
        <h1 className="text-2xl font-semibold">{entity.name}</h1>
        {perms.create && (
          <button
            onClick={() => router.push(`/${entityName}/new`)}
            className="bg-blue-600 text-white px-4 py-2 rounded-lg text-sm hover:bg-blue-700"
          >
            + New
          </button>
        )}
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

      <Card className="p-0 overflow-hidden">
        <DynamicTable
          entity={entity}
          data={data}
          pkValue={pkValue}
          onSort={handleSort}
          sortField={sortField}
          sortDir={sortDir}
          onRowClick={(pk) =>
            router.push(`/${entityName}/${encodeURIComponent(pk)}`)
          }
          onView={(pk) =>
            router.push(`/${entityName}/${encodeURIComponent(pk)}`)
          }
          onEdit={perms.update ? (pk) =>
            router.push(`/${entityName}/${encodeURIComponent(pk)}`) : undefined}
          onDelete={perms.delete ? handleDelete : undefined}
          perm={{ update: perms.update, delete: perms.delete }}
          loading={result.fetching || andedLoading}
        />

        {!isSearching && (
          <div className="flex items-center justify-center gap-2 px-4 py-3 border-t border-zinc-100">
            <button
              onClick={() => setPage((p) => Math.max(0, p - 1))}
              disabled={page === 0}
              className="px-3 py-1.5 text-sm rounded-lg border border-zinc-200 bg-white text-zinc-600 disabled:opacity-30 hover:bg-zinc-50 hover:border-zinc-300 transition-colors"
            >
              <Icon name="chevron-left" className="w-4 h-4" /> Previous
            </button>
            <span className="inline-flex items-center justify-center min-w-[80px] px-3 py-1.5 text-sm font-medium text-zinc-700 bg-zinc-100 rounded-lg">
              Page {page + 1}
            </span>
            <button
              onClick={() => setPage((p) => p + 1)}
              disabled={!hasMore}
              className="px-3 py-1.5 text-sm rounded-lg border border-zinc-200 bg-white text-zinc-600 disabled:opacity-30 hover:bg-zinc-50 hover:border-zinc-300 transition-colors"
            >
              Next <Icon name="chevron-right" className="w-4 h-4" />
            </button>
          </div>
        )}
      </Card>
      {confirm.dialog}
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
        <Breadcrumbs segments={[{ label: "Entities", href: "/" }, { label: entityName }]} />
        <EmptyState icon="search" title="Not Found" description={`Entity "${entityName}" does not exist`} />
      </div>
    );
  }

  if (!entity) {
    return (
      <div className="space-y-3">
        <Skeleton className="h-4 w-24" />
        <Skeleton className="h-6 w-48" />
        <Skeleton className="h-32 w-full" />
      </div>
    );
  }

  return <EntityListContent entity={entity} entityName={entityName} />;
}
