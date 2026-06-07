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

export interface EntityPermissions {
  list: boolean;
  create: boolean;
  read: boolean;
  update: boolean;
  delete: boolean;
}

interface EntityOverride {
  filterComponent?: string;
  permissions?: Partial<EntityPermissions>;
}

interface ConfigFile {
  defaultFilterComponent?: string;
  defaultPermissions?: Partial<EntityPermissions>;
  entityOverrides?: Record<string, EntityOverride>;
  controls: Record<string, Record<string, FieldControl>>;
  relationFilters: Record<string, RelationFilterMeta[]>;
}

const cfg = config as ConfigFile;

const defaultPerms: EntityPermissions = {
  list: true,
  create: true,
  read: true,
  update: true,
  delete: true,
};

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

export function getPermissions(entityName: string): EntityPermissions {
  const defaults = { ...defaultPerms, ...cfg.defaultPermissions };
  const overrides = cfg.entityOverrides?.[entityName]?.permissions;
  return overrides ? { ...defaults, ...overrides } : defaults;
}
