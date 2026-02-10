"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import {
  ArrowLeft,
  ArrowRight,
  BookOpen,
  Clock,
  Zap,
  Search,
  ChevronRight,
  Copy,
  Check,
} from "lucide-react";
import { docsContent, getPageNavigation } from "@/lib/docs-content";
import { DocsSidebar } from "@/components/docs/docs-sidebar";
import { useState } from "react";

function GithubIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
    </svg>
  );
}

function CodeBlock({ code, language }: { code: string; language?: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="group relative mt-3 rounded-xl border border-slate-200 bg-slate-950 text-sm">
      <div className="flex items-center justify-between border-b border-slate-800 px-4 py-2">
        <span className="text-xs text-slate-500">{language || "text"}</span>
        <button
          onClick={handleCopy}
          className="flex items-center gap-1.5 text-xs text-slate-500 transition-colors hover:text-slate-300"
        >
          {copied ? (
            <>
              <Check className="h-3.5 w-3.5" />
              Copied
            </>
          ) : (
            <>
              <Copy className="h-3.5 w-3.5" />
              Copy
            </>
          )}
        </button>
      </div>
      <pre className="overflow-x-auto p-4">
        <code className="text-sm leading-relaxed text-slate-300">{code}</code>
      </pre>
    </div>
  );
}

function NotFoundPage() {
  return (
    <div className="min-h-screen bg-[#faf8f5] text-slate-900 antialiased">
      <header className="fixed left-0 right-0 top-0 z-50 bg-[#faf8f5]/80 backdrop-blur-xl">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="flex h-16 items-center justify-between">
            <Link href="/" className="flex items-center gap-2.5">
              <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-gradient-to-br from-teal-500 to-cyan-500">
                <Zap className="h-4 w-4 text-white" />
              </div>
              <span className="text-lg font-semibold tracking-tight">
                StreamHouse
              </span>
            </Link>
          </div>
        </div>
      </header>
      <main className="flex min-h-screen items-center justify-center pt-16">
        <div className="text-center">
          <BookOpen className="mx-auto h-12 w-12 text-slate-300" />
          <h1 className="mt-4 text-2xl font-semibold text-slate-900">
            Page Not Found
          </h1>
          <p className="mt-2 text-slate-600">
            This documentation page doesn&apos;t exist yet.
          </p>
          <Link href="/docs" className="mt-6 inline-block">
            <Button className="bg-slate-900 text-white hover:bg-slate-800">
              <ArrowLeft className="mr-2 h-4 w-4" />
              Back to Documentation
            </Button>
          </Link>
        </div>
      </main>
    </div>
  );
}

export default function DocsArticlePage() {
  const params = useParams();
  const slugArray = params.slug as string[];
  const slug = slugArray.join("/");
  const page = docsContent[slug];

  if (!page) {
    return <NotFoundPage />;
  }

  const { prev, next } = getPageNavigation(slug);

  return (
    <div className="min-h-screen bg-[#faf8f5] text-slate-900 antialiased">
      {/* Header */}
      <header className="fixed left-0 right-0 top-0 z-50 bg-[#faf8f5]/80 backdrop-blur-xl">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="flex h-16 items-center justify-between">
            <Link href="/" className="flex items-center gap-2.5">
              <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-gradient-to-br from-teal-500 to-cyan-500">
                <Zap className="h-4 w-4 text-white" />
              </div>
              <span className="text-lg font-semibold tracking-tight">
                StreamHouse
              </span>
            </Link>

            <nav className="hidden items-center gap-8 lg:flex">
              {[
                { name: "How it Works", href: "/how-it-works" },
                { name: "Docs", href: "/docs" },
                { name: "Use Cases", href: "/use-cases" },
                { name: "Blog", href: "/blog" },
              ].map((item) => (
                <Link
                  key={item.name}
                  href={item.href}
                  className={`text-sm transition-colors hover:text-slate-900 ${
                    item.name === "Docs"
                      ? "font-medium text-teal-600"
                      : "text-slate-600"
                  }`}
                >
                  {item.name}
                </Link>
              ))}
            </nav>

            <div className="flex items-center gap-3">
              <Link
                href="https://github.com/streamhouse/streamhouse"
                className="text-slate-500 transition-colors hover:text-slate-900"
              >
                <GithubIcon className="h-5 w-5" />
              </Link>
              <Link href="/dashboard">
                <Button variant="ghost" className="text-sm text-slate-600">
                  Sign in
                </Button>
              </Link>
              <Link href="/console">
                <Button className="bg-slate-900 text-sm font-medium text-white hover:bg-slate-800">
                  Get Started
                </Button>
              </Link>
            </div>
          </div>
        </div>
      </header>

      {/* Body */}
      <div className="flex pt-16">
        <DocsSidebar />

        {/* Main content */}
        <main className="min-w-0 flex-1">
          <div className="mx-auto max-w-3xl px-6 py-10 lg:px-12">
            {/* Breadcrumb */}
            <nav className="mb-6 flex items-center gap-1.5 text-sm text-slate-500">
              <Link href="/docs" className="hover:text-teal-600">
                Docs
              </Link>
              <ChevronRight className="h-3.5 w-3.5" />
              <span className="text-slate-400">{page.sectionGroup}</span>
              <ChevronRight className="h-3.5 w-3.5" />
              <span className="font-medium text-slate-700">{page.title}</span>
            </nav>

            {/* Page header */}
            <div className="mb-8">
              <h1 className="font-serif text-3xl italic tracking-tight text-slate-900 sm:text-4xl">
                {page.title}
              </h1>
              <p className="mt-3 text-lg text-slate-600">{page.description}</p>
              <div className="mt-4 flex items-center gap-3">
                <Badge
                  variant="secondary"
                  className="bg-slate-100 text-slate-600"
                >
                  <Clock className="mr-1 h-3 w-3" />
                  {page.readTime} read
                </Badge>
                <Badge
                  variant="secondary"
                  className="bg-teal-50 text-teal-700"
                >
                  {page.sectionGroup}
                </Badge>
              </div>
            </div>

            <Separator className="mb-8 bg-slate-200" />

            {/* Table of Contents */}
            {page.sections.length > 2 && (
              <div className="mb-8 rounded-xl border border-slate-200 bg-white p-5">
                <h3 className="mb-3 text-sm font-semibold text-slate-900">
                  On this page
                </h3>
                <ul className="space-y-1.5">
                  {page.sections.map((section) => (
                    <li key={section.id}>
                      <a
                        href={`#${section.id}`}
                        className="text-sm text-slate-500 transition-colors hover:text-teal-600"
                      >
                        {section.title}
                      </a>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {/* Content sections */}
            <div className="space-y-10">
              {page.sections.map((section) => (
                <section key={section.id} id={section.id}>
                  <h2 className="text-xl font-semibold text-slate-900">
                    {section.title}
                  </h2>
                  <p className="mt-3 leading-relaxed text-slate-600">
                    {section.content}
                  </p>

                  {section.items && (
                    <ul className="mt-4 space-y-2">
                      {section.items.map((item, i) => (
                        <li
                          key={i}
                          className="flex gap-3 text-sm leading-relaxed text-slate-600"
                        >
                          <span className="mt-1.5 h-1.5 w-1.5 flex-shrink-0 rounded-full bg-teal-500" />
                          <span>{item}</span>
                        </li>
                      ))}
                    </ul>
                  )}

                  {section.code && (
                    <CodeBlock
                      code={section.code}
                      language={section.language}
                    />
                  )}
                </section>
              ))}
            </div>

            {/* Previous / Next navigation */}
            {(prev || next) && (
              <>
                <Separator className="mb-6 mt-12 bg-slate-200" />
                <div className="flex items-center justify-between gap-4">
                  {prev ? (
                    <Link
                      href={prev.href}
                      className="group flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-4 py-3 text-sm transition-all hover:border-teal-200 hover:shadow-sm"
                    >
                      <ArrowLeft className="h-4 w-4 text-slate-400 transition-transform group-hover:-translate-x-0.5" />
                      <div>
                        <div className="text-xs text-slate-400">Previous</div>
                        <div className="font-medium text-slate-700">
                          {prev.title}
                        </div>
                      </div>
                    </Link>
                  ) : (
                    <div />
                  )}
                  {next ? (
                    <Link
                      href={next.href}
                      className="group flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-4 py-3 text-sm text-right transition-all hover:border-teal-200 hover:shadow-sm"
                    >
                      <div>
                        <div className="text-xs text-slate-400">Next</div>
                        <div className="font-medium text-slate-700">
                          {next.title}
                        </div>
                      </div>
                      <ArrowRight className="h-4 w-4 text-slate-400 transition-transform group-hover:translate-x-0.5" />
                    </Link>
                  ) : (
                    <div />
                  )}
                </div>
              </>
            )}
          </div>

          {/* Footer */}
          <footer className="border-t border-slate-200 bg-white">
            <div className="mx-auto max-w-7xl px-6 py-12 lg:px-8">
              <div className="flex flex-col items-center justify-between gap-6 md:flex-row">
                <Link href="/" className="flex items-center gap-2">
                  <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-gradient-to-br from-teal-500 to-cyan-500">
                    <Zap className="h-3.5 w-3.5 text-white" />
                  </div>
                  <span className="font-semibold text-slate-900">
                    StreamHouse
                  </span>
                </Link>

                <nav className="flex flex-wrap items-center justify-center gap-6">
                  {["Documentation", "GitHub", "Blog", "Dashboard"].map(
                    (item) => (
                      <Link
                        key={item}
                        href={
                          item === "GitHub"
                            ? "https://github.com/streamhouse/streamhouse"
                            : `/${item.toLowerCase()}`
                        }
                        className="text-sm text-slate-500 hover:text-slate-900"
                      >
                        {item}
                      </Link>
                    )
                  )}
                </nav>

                <p className="text-sm text-slate-400">
                  &copy; {new Date().getFullYear()} StreamHouse &middot; MIT
                  License
                </p>
              </div>
            </div>
          </footer>
        </main>
      </div>
    </div>
  );
}
