import type { EntityInfo } from "./types";

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

export function buildListQuery(
  entity: EntityInfo,
  paginate?: { limit: number; sortField?: string; sortDir?: "asc" | "desc" }
): string {
  const scalarFields = entity.fields.filter((f) => f.kind === "scalar");
  const cols = scalarFields.map((f) => f.name).join("\n      ");
  const pagArgs = paginate ? "limit: $limit, offset: $offset" : "";
  const sortArgs =
    paginate?.sortField ? `order_by: $order_by` : "";
  const args = [pagArgs, sortArgs].filter(Boolean).join(", ");
  const argsStr = args ? `(${args})` : "";
  const vars = paginate
    ? `($limit: Int, $offset: Int${paginate.sortField ? ", $order_by: String" : ""})`
    : "";
  return `query ${capitalize(entity.name)}ListQuery${vars} {
    ${entity.name}List${argsStr} {
      ${cols}
    }
  }`;
}

export function buildDetailQuery(
  entity: EntityInfo,
  entityLookup?: (name: string) => EntityInfo | null
): string {
  const { fields } = entity;
  const parts: string[] = [];

  for (const f of fields) {
    if (f.kind === "scalar") {
      parts.push(f.name);
    } else if (f.kind === "has_many" && f.relatedEntity && entityLookup) {
      const rel = entityLookup(f.relatedEntity);
      if (rel) {
        const relFields = rel.fields
          .filter((rf) => rf.kind === "scalar" && !rf.autoIncrement)
          .slice(0, 3)
          .map((rf) => rf.name)
          .join("\n        ");
        parts.push(`${f.name} {\n        ${relFields}\n      }`);
      } else {
        parts.push(`${f.name} { id }`);
      }
    } else if (f.kind === "belongs_to" && f.relatedEntity && entityLookup) {
      const rel = entityLookup(f.relatedEntity);
      if (rel) {
        const relFields = rel.fields
          .filter((rf) => rf.kind === "scalar" && !rf.autoIncrement && !rf.isPk)
          .slice(0, 2)
          .map((rf) => rf.name)
          .join("\n        ");
        parts.push(`${f.name} {\n        ${relFields}\n      }`);
      } else {
        parts.push(`${f.name} { id }`);
      }
    } else {
      parts.push(`${f.name} { id }`);
    }
  }

  const selection = parts.join("\n      ");
  const cap = capitalize(entity.name);

  return `query ${cap}DetailQuery($id: String!) {
    ${entity.name}(id: $id) {
      ${selection}
    }
  }`;
}

export function buildCreateMutation(entity: EntityInfo): string {
  const scalarFields = entity.fields.filter(
    (f) => f.kind === "scalar" && !f.autoIncrement
  );
  const returnFields = scalarFields.map((f) => f.name).join("\n      ");
  const cap = capitalize(entity.name);

  return `mutation ${cap}Create($input: Create${cap}Input!) {
    create${cap}(input: $input) {
      ${returnFields}
    }
  }`;
}

export function buildUpdateMutation(entity: EntityInfo): string {
  const scalarFields = entity.fields.filter(
    (f) => f.kind === "scalar" && !f.autoIncrement
  );
  const returnFields = scalarFields.map((f) => f.name).join("\n      ");
  const cap = capitalize(entity.name);

  return `mutation ${cap}Update($id: String!, $input: Update${cap}Input!) {
    update${cap}(id: $id, input: $input) {
      ${returnFields}
    }
  }`;
}

export function buildDeleteMutation(entity: EntityInfo): string {
  const scalarFields = entity.fields.filter(
    (f) => f.kind === "scalar"
  );
  const returnFields = scalarFields.map((f) => f.name).join("\n      ");
  const cap = capitalize(entity.name);
  return `mutation ${cap}Delete($id: String!) {
    delete${cap}(id: $id) {
      ${returnFields}
    }
  }`;
}

export function buildSearchQuery(
  entity: EntityInfo,
  includeFilter?: boolean
): string {
  const scalarFields = entity.fields.filter((f) => f.kind === "scalar");
  const cols = scalarFields.map((f) => f.name).join("\n      ");
  const cap = capitalize(entity.name);

  if (includeFilter) {
    const filterTypeName = `${cap}SearchFilter`;
    const filterFields =
      entity.searchFilterFields?.map((f) => f.name).join(", ") ?? "";
    return `query ${cap}Search($query: String!, $filter: ${filterTypeName}) {
    search${cap}(query: $query, filter: $filter) {
      ${cols}
    }
  }`;
  }

  return `query ${cap}Search($query: String!) {
    search${cap}(query: $query) {
      ${cols}
    }
  }`;
}
