"use client";

import { createClient, fetchExchange, Provider } from "urql";
import type { ReactNode } from "react";

const apiUrl = "/api/graphql";

export const client = createClient({
  url: apiUrl,
  exchanges: [fetchExchange],
  requestPolicy: "network-only",
});

export function GraphQLProvider({ children }: { children: ReactNode }) {
  return <Provider value={client}>{children}</Provider>;
}
