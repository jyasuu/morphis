import type { ComponentType } from "react";
import type { RelationFilterMeta } from "@/lib/metadata";
import { AdvancedFilter } from "@/components/advanced-filter";
import { LegacyFilters } from "@/components/filters/legacy-filters";

export interface FilterComponentProps {
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

const registry: Record<string, ComponentType<FilterComponentProps>> = {
  advanced: AdvancedFilter,
  legacy: LegacyFilters,
};

export function getFilterComponent(
  name: string
): ComponentType<FilterComponentProps> {
  return registry[name] ?? AdvancedFilter;
}
