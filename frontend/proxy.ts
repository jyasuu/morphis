import { auth } from "@/auth";
import { NextResponse } from "next/server";

export default auth((req) => {
  if (process.env.AUTH_DISABLED === "true") return;

  const { pathname } = req.nextUrl;

  if (
    !req.auth &&
    pathname !== "/login" &&
    !pathname.startsWith("/api/auth") &&
    !pathname.startsWith("/_next") &&
    !pathname.startsWith("/locales") &&
    pathname !== "/icon.svg" &&
    pathname !== "/favicon.ico"
  ) {
    const url = new URL("/login", req.url);
    url.searchParams.set("callbackUrl", pathname);
    return NextResponse.redirect(url);
  }
});

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico|icon.svg|locales).*)"],
};
