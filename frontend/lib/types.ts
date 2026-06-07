export interface EntityInfo {
  name: string
  fields: FieldInfo[]
  primaryKey: string
  autoIncrementFields: string[]
  hasSearch: boolean
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
