import NextAuth from "next-auth";
import Credentials from "next-auth/providers/credentials";
import Google from "next-auth/providers/google";
import GitHub from "next-auth/providers/github";

function oidcProvider() {
  const issuer = process.env.AUTH_OIDC_ISSUER;
  const clientId = process.env.AUTH_OIDC_CLIENT_ID;
  const clientSecret = process.env.AUTH_OIDC_CLIENT_SECRET;
  if (!issuer || !clientId || !clientSecret) return null;
  return {
    id: "oidc",
    name: process.env.AUTH_OIDC_NAME || "SSO",
    type: "oidc" as const,
    issuer,
    clientId,
    clientSecret,
    allowDangerousEmailAccountLinking: true,
    profile(profile: any) {
      const email = profile.email || profile.preferred_username || "";
      return {
        id: profile.sub,
        name: profile.name || email,
        email,
        image: profile.picture,
        role: "user",
        provider: "oidc",
      };
    },
  };
}

declare module "next-auth" {
  interface Session {
    accessToken?: string;
    user: {
      id?: string;
      name?: string | null;
      email?: string | null;
      image?: string | null;
      role: string;
      provider: string;
    };
  }
}

declare module "@auth/core/jwt" {
  interface JWT {
    role: string;
    provider: string;
    accessToken?: string;
  }
}

export const { handlers, auth, signIn, signOut } = NextAuth({
  providers: [
    Credentials({
      name: "credentials",
      credentials: {
        username: { label: "Username", type: "text", placeholder: "admin" },
        password: { label: "Password", type: "password" },
      },
      async authorize(credentials) {
        const username = credentials?.username as string;
        const password = credentials?.password as string;
        const adminUser = process.env.AUTH_ADMIN_USERNAME || "admin";
        const adminPass = process.env.AUTH_ADMIN_PASSWORD;
        if (!adminPass) return null;
        if (username === adminUser && password === adminPass) {
          return { id: "admin", name: adminUser, role: "admin", provider: "credentials" };
        }
        return null;
      },
    }),
    Google({ allowDangerousEmailAccountLinking: true }),
    GitHub({ allowDangerousEmailAccountLinking: true }),
    oidcProvider(),
  ].filter((p): p is NonNullable<typeof p> => p != null),
  callbacks: {
    async jwt({ token, user, account }) {
      if (user) {
        token.role = (user as any).role || "user";
        token.provider = account?.provider || "credentials";
        if (account?.provider === "oidc" && account?.access_token) {
          token.accessToken = account.access_token as string;
        }
      }
      return token;
    },
    async session({ session, token }) {
      session.user.role = token.role as string;
      session.user.provider = token.provider as string;
      session.accessToken = token.accessToken as string;
      return session;
    },
  },
  session: { strategy: "jwt" },
  pages: {
    signIn: "/login",
  },
});
