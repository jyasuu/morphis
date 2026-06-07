import config from "@/config/entity-metadata.json";

export interface FieldControl {
  control: "text" | "select";
  options?: { label: string; value: string }[];
}

export interface RelationFilterMeta {
  label: string;
  relationEntity: string;
  field: string;
  displayField: string;
  defaultLogic?: "and" | "or";
}

interface ConfigFile {
  defaultFilterComponent?: string;
  entityOverrides?: Record<
    string,
    { filterComponent?: string }
  >;
  controls: Record<string, Record<string, FieldControl>>;
  relationFilters: Record<string, RelationFilterMeta[]>;
}

const cfg = config as ConfigFile;

export function getFieldControl(entityName: string, fieldName: string): FieldControl {
  return cfg.controls?.[entityName]?.[fieldName] ?? { control: "text" };
}

export function getRelationFilters(entityName: string): RelationFilterMeta[] {
  return cfg.relationFilters?.[entityName] ?? [];
}

export function getFilterComponentName(entityName: string): string {
  return (
    cfg.entityOverrides?.[entityName]?.filterComponent ??
    cfg.defaultFilterComponent ??
    "advanced"
  );
}
