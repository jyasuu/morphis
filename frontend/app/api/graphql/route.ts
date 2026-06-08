import { auth } from "@/auth";
import { NextRequest, NextResponse } from "next/server";
import { createHmac } from "crypto";

function backendUrl(): string {
  const u = process.env.GRAPHQL_URL || "http://localhost:4000/graphql";
  return u.endsWith("/graphql") ? u : `${u.replace(/\/+$/, "")}/graphql`;
}

const BACKEND_URL = backendUrl();
const ADMIN_TENANT_ID = process.env.ADMIN_TENANT_ID || "default";
const JWT_SECRET = process.env.AUTH_PROXY_JWT_SECRET;

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

function b64url(s: string): string {
  return Buffer.from(s).toString("base64url");
}

function signJwt(payload: Record<string, unknown>, secret: string): string {
  const header = b64url(JSON.stringify({ alg: "HS256", typ: "JWT" }));
  const body = b64url(JSON.stringify(payload));
  const sig = createHmac("sha256", secret).update(`${header}.${body}`).digest("base64url");
  return `${header}.${body}.${sig}`;
}

async function proxyToBackend(body: unknown) {
  let session;
  try {
    session = await auth();
  } catch (e) {
    console.error("auth() failed:", e);
  }
  if (process.env.AUTH_DISABLED !== "true" && !session?.user) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }
  const user = session?.user ?? { role: "admin", email: null, name: null };
  const tenantId = getTenantId(user);
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (JWT_SECRET) {
    headers["Authorization"] = `Bearer ${signJwt({ sub: user.email || user.name || "admin", tenant_id: tenantId, role: user.role }, JWT_SECRET)}`;
  } else {
    headers["X-Tenant-ID"] = tenantId;
    headers["X-Auth-Role"] = user.role;
    headers["X-Auth-User"] = user.email || user.name || "unknown";
  }
  let res;
  try {
    res = await fetch(BACKEND_URL, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
    });
  } catch (e) {
    console.error("fetch to backend failed:", e);
    return NextResponse.json({ error: "Backend unreachable" }, { status: 502 });
  }
  let data;
  const bodyText = await res.text();
  try {
    data = JSON.parse(bodyText);
  } catch (e) {
    console.error("Failed to parse backend response:", e, "status:", res.status);
    console.error("Backend response body:", bodyText.slice(0, 500));
    return NextResponse.json({ error: "Invalid backend response" }, { status: 502 });
  }
  return NextResponse.json(data);
}

export async function POST(req: NextRequest) {
  try {
    const body = await req.json();
    return proxyToBackend(body);
  } catch (e) {
    console.error("POST handler error:", e);
    return NextResponse.json({ error: String(e) }, { status: 500 });
  }
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
