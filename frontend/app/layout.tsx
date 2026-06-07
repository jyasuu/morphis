import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";
import { GraphQLProvider } from "@/lib/client";
import { ToastContainer } from "@/components/toast";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "Morphis Admin",
  description: "Generic GraphQL admin UI",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      className={`${geistSans.variable} ${geistMono.variable} h-full antialiased`}
    >
      <body className="min-h-full flex flex-col">
        <GraphQLProvider>
          <header className="border-b px-6 py-3">
            <a href="/" className="text-lg font-semibold text-zinc-800">
              Morphis Admin
            </a>
          </header>
          <main className="flex-1 px-6 py-6">{children}</main>
          <ToastContainer />
        </GraphQLProvider>
      </body>
    </html>
  );
}
