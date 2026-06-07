"use client";

import { createClient, fetchExchange, Provider } from "urql";
import type { ReactNode } from "react";

const apiUrl =
  process.env.NEXT_PUBLIC_GRAPHQL_URL || "http://localhost:4000/graphql";

export const client = createClient({
  url: apiUrl,
  exchanges: [fetchExchange],
  requestPolicy: "network-only",
});

export function GraphQLProvider({ children }: { children: ReactNode }) {
  return <Provider value={client}>{children}</Provider>;
}
