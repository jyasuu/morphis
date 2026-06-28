export interface EntityInfo {
  name: string
  fields: FieldInfo[]
  primaryKey: string
  autoIncrementFields: string[]
  hasSearch: boolean
  searchFilterFields?: SearchFilterFieldInfo[]
  capabilities: EntityCapabilities
}

export interface EntityCapabilities {
  list: boolean
  detail: boolean
  create: boolean
  update: boolean
  delete: boolean
}

export interface SearchFilterFieldInfo {
  name: string
  scalarType: string
}

export interface FieldInfo {
  name: string
  kind: "scalar" | "has_many" | "belongs_to"
  scalarType?: string
  nullable: boolean
  relatedEntity?: string
  autoIncrement?: boolean
  isPk?: boolean
}
