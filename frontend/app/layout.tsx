import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";
import { GraphQLProvider } from "@/lib/client";
import { ToastContainer } from "@/components/toast";
import { NavBar } from "@/components/nav-bar";
import { ThemeProvider } from "@/components/theme-provider";
import Script from "next/script";

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
      suppressHydrationWarning
    >
      <head>
        <Script id="theme-init" strategy="beforeInteractive">
          {`(function(){try{var e=localStorage.getItem("morphis-theme");if(e==="dark"||(!e&&matchMedia("(prefers-color-scheme:dark)").matches))document.documentElement.classList.add("dark")}catch(e){}})()`}
        </Script>
      </head>
      <body className="min-h-full flex flex-col">
        <ThemeProvider>
          <GraphQLProvider>
            <NavBar />
            <main className="flex-1 px-6 py-6 max-w-6xl w-full mx-auto">{children}</main>
            <ToastContainer />
          </GraphQLProvider>
        </ThemeProvider>
      </body>
    </html>
  );
}
