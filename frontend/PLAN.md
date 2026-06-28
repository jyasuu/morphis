# Generic GraphQL Admin UI — Minimum Demo Plan

## Goal

A Next.js app that connects to the Morphis GraphQL endpoint, introspects the schema at runtime, and renders a generic CRUD admin UI for **any entity** defined in `config.yaml` — no entity-specific code.

---

## Milestone 1: Scaffold & Connect

- [ ] `npx create-next-app@latest frontend/` with App Router + TypeScript + Tailwind
- [ ] Add `urql` GraphQL client
- [ ] Create a reusable client initialization (pointed at `http://localhost:4000/graphql`)
- [ ] Verify connection: hardcode a query against `materialsList` and render JSON

**Deliverable:** Next.js project boots, fetches data from the API.

---

## Milestone 2: Introspection Engine

- [ ] Fetch full GraphQL introspection schema on startup (cache in-memory or as a module-level singleton)
- [ ] Extract a clean type list: for each type, produce `{ name, fields[], primaryKey, isAutoIncrement[], relations[] }`
- [ ] Helper: `guessPrimaryKey(type)` → pick `id` or `*_no` or first unique field
- [ ] Helper: `classifyField(field)` → scalar, relation-has-many, relation-belongs-to, auto-increment
- [ ] Helper: `buildListQuery(type)` → auto-generate a GraphQL query for that type (scalar fields + relation names)
- [ ] Helper: `buildDetailQuery(type)` → same but fetches into nested relation data

**Deliverable:** Passing the string `"materials"` into a function returns a ready-to-execute GraphQL query.

---

## Milestone 3: Dynamic Entity List Page

- [ ] Route: `/[entity]` → generic list page
- [ ] Render a table: columns = all scalar fields, rows = records
- [ ] Each row has an "Edit" link → `/[entity]/[id]`
- [ ] A "New" button → `/[entity]/new` (or inline modal)
- [ ] Handle different types: string fields as text, int fields right-aligned, auto-increment PKs hidden

**Deliverable:** `/materials`, `/sizes`, `/colorways`, `/material_features`, `/feature_attributes` all render a read-only list.

---

## Milestone 4: Dynamic Entity Create/Edit

- [ ] Route: `/[entity]/[id]` → detail + edit form
- [ ] Auto-generate a form from the type's scalar fields (label + input by type)
- [ ] Mutation: `update{Entity}` on submit — auto-build the mutation from form fields
- [ ] Route: `/[entity]/new` → same form, empty, calls `create{Entity}` on submit
- [ ] Relation display: show has_many children as sub-tables (read-only, no nested CRUD yet)
- [ ] Relation display: show belongs_to parent as a link

**Deliverable:** Full CRUD on any entity — create, read, update, delete.

---

## Milestone 5: Search & Filter

- [ ] Check introspection: if a `search{Entity}` query exists, show a search bar
- [ ] Debounced search input → calls `search{Entity}` → renders same table
- [ ] Fall back to `{entity}List(filter: ...)` for simple field filters if no search index
- [ ] Add a basic filter: e.g., `status` dropdown for materials (from known enum-like values)

**Deliverable:** Search works for entities with an ES index; basic filter works for all entities.

---

## Milestone 6: Polish

- [ ] Loading states (skeleton rows while fetching)
- [ ] Error state (toast on mutation failure)
- [ ] Empty state ("No records found")
- [ ] Confirm before delete
- [ ] Pagination: pass `limit`/`offset` to list queries with next/prev buttons

**Deliverable:** A functional, usable generic admin — not ugly, not pretty, just works.

---

## Out of Scope (for minimum demo)

- Nested CRUD (create a material + its sizes in one transaction)
- Color picker for `hex`
- Authentication / JWT handling (assumes public API or simple header)
- SSR for list pages (keep it client-side for simplicity)
- File uploads
- Sorting by column
- Dark mode
- Responsive / mobile layout
- Real-time updates

---

## Tech Stack

| Concern | Choice |
|---|---|
| Framework | Next.js 14+ (App Router) |
| Language | TypeScript |
| GraphQL Client | urql (with `@urql/core` + `graphql-ws`) |
| Styling | Tailwind CSS |
| UI Components | shadcn/ui (optional, for nicer forms) |
| Introspection | native `fetch` + `buildClientSchema` from `graphql` package |

## File Structure (target)

```
frontend/
├── app/
│   ├── layout.tsx            # root layout, urql provider
│   ├── page.tsx              # redirect to /materials or entity picker
│   └── [entity]/
│       ├── page.tsx          # list page
│       └── [id]/
│           ├── page.tsx      # detail/edit page
│           └── new/page.tsx  # create page
├── lib/
│   ├── client.ts             # urql client
│   ├── schema.ts             # introspection fetch + type helpers
│   ├── query-builder.ts      # auto-build list/detail/mutation queries
│   └── types.ts              # inferred type definitions
├── components/
│   ├── dynamic-table.tsx     # generic table renderer
│   ├── dynamic-form.tsx      # generic create/edit form
│   └── search-bar.tsx        # debounced search
├── tailwind.config.ts
├── next.config.js
└── package.json
```
