"use client";

import { useState, useEffect } from "react";
import { useQuery } from "urql";
import { SearchFilter } from "@/components/search-filter";
import { RelationFilter } from "@/components/relation-filter";
import type { RelationFilterMeta } from "@/lib/metadata";

interface Props {
  entityName: string;
  filterFields: { name: string; scalarType: string }[];
  relationFilters: RelationFilterMeta[];
  onFilterChange: (params: {
    query: string;
    filter: Record<string, string>;
    logic?: "and" | "or";
    terms?: string[];
  }) => void;
}

function buildOptionsQuery(entityName: string, field: string): string {
  return `query ${entityName}Options {
    ${entityName}List(limit: 200) {
      ${field}
    }
  }`;
}

export function LegacyFilters({
  entityName,
  filterFields,
  relationFilters,
  onFilterChange,
}: Props) {
  const [values, setValues] = useState<Record<string, string>>({});
  const [relationVals, setRelationVals] = useState<string[][]>(
    relationFilters.map(() => [])
  );

  // Emit on every value change
  useEffect(() => {
    const directFilter: Record<string, string> = {};
    for (const f of filterFields) {
      if (values[f.name]) directFilter[f.name] = values[f.name];
    }
    const searchTerms = relationVals.flat().filter(Boolean);
    onFilterChange({
      query: searchTerms.join(" "),
      filter: directFilter,
    });
  }, [values, relationVals, filterFields, onFilterChange]);

  return (
    <div className="space-y-3">
      {filterFields.length > 0 && (
        <SearchFilter
          entityName={entityName}
          fields={filterFields}
          onFilter={(f) => setValues((prev) => ({ ...prev, ...f }))}
        />
      )}
      {relationFilters.length > 0 && (
        <div className="flex flex-wrap gap-3">
          {relationFilters.map((rf, i) => (
            <RelationFilter
              key={`${rf.relationEntity}-${rf.field}`}
              filter={rf}
              onSelect={(vals) => {
                const next = [...relationVals];
                next[i] = vals;
                setRelationVals(next);
              }}
            />
          ))}
        </div>
      )}
    </div>
  );
}
