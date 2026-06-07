import { auth } from "@/auth";
import { NextRequest, NextResponse } from "next/server";

const BACKEND_URL = process.env.GRAPHQL_URL || "http://localhost:4000/graphql";
const ADMIN_TENANT_ID = process.env.ADMIN_TENANT_ID || "default";

function getTenantId(user: { role: string; email?: string | null }): string {
  if (user.role === "admin") return ADMIN_TENANT_ID;
  const mappings = process.env.USER_TENANT_MAPPINGS;
  if (mappings && user.email) {
    try {
      const map: Record<string, string> = JSON.parse(mappings);
      if (map[user.email]) return map[user.email];
    } catch {}
  }
  return ADMIN_TENANT_ID;
}

async function proxyToBackend(body: unknown) {
  const session = await auth();
  if (process.env.AUTH_DISABLED !== "true" && !session?.user) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }
  const user = session?.user ?? { role: "admin", email: null, name: null };
  const tenantId = getTenantId(user);
  const res = await fetch(BACKEND_URL, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Tenant-ID": tenantId,
      "X-Auth-Role": user.role,
      "X-Auth-User": user.email || user.name || "unknown",
    },
    body: JSON.stringify(body),
  });
  const data = await res.json();
  return NextResponse.json(data);
}

export async function POST(req: NextRequest) {
  const body = await req.json();
  return proxyToBackend(body);
}

export async function GET(req: NextRequest) {
  const { searchParams } = new URL(req.url);
  const query = searchParams.get("query");
  const variables = searchParams.get("variables");
  const body: Record<string, unknown> = {};
  if (query) body.query = query;
  if (variables) {
    try { body.variables = JSON.parse(variables); } catch {}
  }
  return proxyToBackend(body);
}
