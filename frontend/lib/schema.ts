import type { EntityInfo, FieldInfo } from "./types";

const introspectionQuery = `
  query IntrospectionQuery {
    __schema {
      types {
        name
        kind
        fields {
          name
          type {
            name
            kind
            ofType {
              name
              kind
              ofType {
                name
                kind
                ofType {
                  name
                  kind
                }
              }
            }
          }
        }
        inputFields {
          name
          type {
            name
            kind
            ofType {
              name
              kind
              ofType {
                name
                kind
              }
            }
          }
        }
      }
      queryType {
        name
        fields {
          name
          type {
            name
            kind
            ofType {
              name
              kind
              ofType {
                name
                kind
                ofType {
                  name
                  kind
                }
              }
            }
          }
        }
      }
      mutationType {
        name
        fields {
          name
          args {
            name
            type {
              name
              kind
              ofType {
                name
                kind
              }
            }
          }
        }
      }
    }
  }
`;

type SchemaCache = Record<string, EntityInfo>;

let cache: SchemaCache | null = null;

interface IntroTypeRef {
  name: string | null;
  kind: string;
  ofType?: IntroTypeRef | null;
}

function unwrapType(t: IntroTypeRef | null | undefined): {
  namedType: string | null;
  kind: string;
  nonNull: boolean;
  isList: boolean;
} {
  if (!t) return { namedType: null, kind: "UNKNOWN", nonNull: false, isList: false };
  if (t.kind === "NON_NULL") {
    const inner = unwrapType(t.ofType);
    return { ...inner, nonNull: true };
  }
  if (t.kind === "LIST") {
    const inner = unwrapType(t.ofType);
    return { ...inner, isList: true };
  }
  return { namedType: t.name, kind: t.kind, nonNull: false, isList: false };
}

function isScalarKind(kind: string): boolean {
  return ["SCALAR", "ENUM"].includes(kind);
}

export async function loadSchema(): Promise<SchemaCache> {
  if (cache) return cache;

  const url =
    process.env.NEXT_PUBLIC_GRAPHQL_URL || "http://localhost:4000/graphql";
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ query: introspectionQuery }),
  });
  const json = await res.json();
  const schema = json.data.__schema;

  const typeMap: Record<string, any> = {};
  for (const t of schema.types) {
    typeMap[t.name] = t;
  }

  const queryFields: Record<string, any> = {};
  for (const f of schema.queryType?.fields || []) {
    queryFields[f.name] = f;
  }

  const entityNames = new Set<string>();
  for (const name of Object.keys(queryFields)) {
    const m = name.match(/^(.+?)List$/);
    if (m) entityNames.add(m[1]);
  }

  const searchNames = new Set<string>();
  for (const name of Object.keys(queryFields)) {
    const m = name.match(/^search(.+)$/);
    if (m) searchNames.add(m[1]);
  }

  const entities: SchemaCache = {};

  for (const name of entityNames) {
    const type = typeMap[name];
    if (!type || !type.fields) continue;

    const cap = name.charAt(0).toUpperCase() + name.slice(1);
    const createInputName = `Create${cap}Input`;
    const createInputType = typeMap[createInputName];

    const createInputFields = new Set<string>();
    if (createInputType?.inputFields) {
      for (const f of createInputType.inputFields) {
        createInputFields.add(f.name);
      }
    }

    const fields: FieldInfo[] = [];
    let primaryKey = "id";
    const autoIncrementFields: string[] = [];

    for (const f of type.fields) {
      const { namedType: fName, kind: fKind, nonNull, isList } = unwrapType(f.type);
      const innerType = fName ? typeMap[fName] : null;

      const isCreateable = createInputFields.has(f.name);

      if (innerType && innerType.fields) {
        if (isList) {
          fields.push({
            name: f.name,
            kind: "has_many",
            nullable: !nonNull,
            relatedEntity: fName!,
          });
        } else {
          fields.push({
            name: f.name,
            kind: "belongs_to",
            nullable: !nonNull,
            relatedEntity: fName!,
          });
        }
      } else if (isScalarKind(fKind)) {
        if (f.name === "id") {
          primaryKey = f.name;
        }
        if (!isCreateable) {
          autoIncrementFields.push(f.name);
        }
        fields.push({
          name: f.name,
          kind: "scalar",
          scalarType: fName ?? undefined,
          nullable: !nonNull,
          autoIncrement: !isCreateable,
          isPk: f.name === "id",
        });
      }
    }

    // Guess primary key for natural keys (mat_no, etc.)
    if (!fields.find((f) => f.name === primaryKey)) {
      primaryKey =
        fields.find((f) => f.kind === "scalar" && !f.autoIncrement && f.name.endsWith("_no"))
          ?.name ||
        fields.find((f) => f.kind === "scalar" && !f.autoIncrement)
          ?.name ||
        fields[0]?.name ||
        "id";
    }

    entities[name] = {
      name,
      fields,
      primaryKey,
      autoIncrementFields,
      hasSearch: searchNames.has(cap) || searchNames.has(name),
    };
  }

  cache = entities;
  return entities;
}

export async function getEntity(name: string): Promise<EntityInfo | null> {
  const schema = await loadSchema();
  return schema[name] ?? null;
}

export async function getEntityNames(): Promise<string[]> {
  const schema = await loadSchema();
  return Object.keys(schema).sort();
}

export function getCachedEntity(name: string): EntityInfo | null {
  return cache?.[name] ?? null;
}
