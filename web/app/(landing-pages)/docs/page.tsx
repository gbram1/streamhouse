"use client";

import Link from "next/link";
import { Button } from "@/components/ui/button";
import {
  ArrowRight,
  BookOpen,
  Code2,
  Database,
  FileText,
  Layers,
  Play,
  Server,
  Settings,
  Terminal,
  Zap,
  Search,
} from "lucide-react";

function GithubIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
    </svg>
  );
}

const sections = [
  {
    title: "Getting Started",
    description: "Quick guides to get up and running",
    icon: Play,
    color: "teal",
    links: [
      { title: "Quick Start", href: "/docs/quick-start", time: "5 min" },
      { title: "Installation", href: "/docs/installation", time: "3 min" },
      { title: "First Topic", href: "/docs/first-topic", time: "5 min" },
      { title: "Docker Setup", href: "/docs/docker", time: "10 min" },
    ],
  },
  {
    title: "Core Concepts",
    description: "Understanding StreamHouse architecture",
    icon: Layers,
    color: "blue",
    links: [
      { title: "Architecture Overview", href: "/docs/architecture", time: "10 min" },
      { title: "Topics & Partitions", href: "/docs/topics", time: "8 min" },
      { title: "Producers", href: "/docs/producers", time: "6 min" },
      { title: "Consumers", href: "/docs/consumers", time: "8 min" },
    ],
  },
  {
    title: "Agents",
    description: "Stateless compute nodes",
    icon: Server,
    color: "cyan",
    links: [
      { title: "Agent Configuration", href: "/docs/agents/config", time: "5 min" },
      { title: "Scaling Agents", href: "/docs/agents/scaling", time: "6 min" },
      { title: "Health & Metrics", href: "/docs/agents/metrics", time: "5 min" },
      { title: "Lease Management", href: "/docs/agents/leases", time: "7 min" },
    ],
  },
  {
    title: "Storage",
    description: "S3-native persistence layer",
    icon: Database,
    color: "purple",
    links: [
      { title: "S3 Configuration", href: "/docs/storage/s3", time: "5 min" },
      { title: "Segment Format", href: "/docs/storage/segments", time: "8 min" },
      { title: "Compression", href: "/docs/storage/compression", time: "4 min" },
      { title: "Retention Policies", href: "/docs/storage/retention", time: "5 min" },
    ],
  },
  {
    title: "SQL Processing",
    description: "Stream transformations with SQL",
    icon: Code2,
    color: "rose",
    links: [
      { title: "SQL Overview", href: "/docs/sql/overview", time: "6 min" },
      { title: "Creating Streams", href: "/docs/sql/streams", time: "8 min" },
      { title: "Windowed Aggregations", href: "/docs/sql/windows", time: "10 min" },
      { title: "Joins", href: "/docs/sql/joins", time: "8 min" },
    ],
  },
  {
    title: "Operations",
    description: "Production deployment guides",
    icon: Settings,
    color: "amber",
    links: [
      { title: "Monitoring", href: "/docs/ops/monitoring", time: "8 min" },
      { title: "Alerting", href: "/docs/ops/alerting", time: "6 min" },
      { title: "Backup & Recovery", href: "/docs/ops/backup", time: "7 min" },
      { title: "Security", href: "/docs/ops/security", time: "10 min" },
    ],
  },
];

const quickLinks = [
  {
    title: "API Reference",
    description: "Complete gRPC and REST API documentation",
    icon: FileText,
    href: "/docs/api",
  },
  {
    title: "CLI Reference",
    description: "streamctl command-line tool documentation",
    icon: Terminal,
    href: "/docs/cli",
  },
  {
    title: "Examples",
    description: "Sample applications and code snippets",
    icon: Code2,
    href: "/docs/examples",
  },
];

const colorClasses: Record<string, { bg: string; text: string; border: string }> = {
  teal: { bg: "bg-teal-100", text: "text-teal-600", border: "border-teal-200" },
  blue: { bg: "bg-blue-100", text: "text-blue-600", border: "border-blue-200" },
  cyan: { bg: "bg-cyan-100", text: "text-cyan-600", border: "border-cyan-200" },
  purple: { bg: "bg-purple-100", text: "text-purple-600", border: "border-purple-200" },
  rose: { bg: "bg-rose-100", text: "text-rose-600", border: "border-rose-200" },
  amber: { bg: "bg-amber-100", text: "text-amber-600", border: "border-amber-200" },
};

export default function DocsPage() {
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
                    item.name === "Docs" ? "text-teal-600 font-medium" : "text-slate-600"
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

      <main className="pt-16">
        {/* Hero */}
        <section className="relative overflow-hidden">
          <div className="pointer-events-none absolute inset-0">
            <div className="absolute left-1/4 top-32 h-[400px] w-[400px] rounded-full bg-gradient-to-br from-teal-200/40 via-cyan-200/20 to-transparent blur-3xl" />
            <div className="absolute right-1/4 top-48 h-[300px] w-[300px] rounded-full bg-gradient-to-bl from-blue-200/30 via-indigo-100/20 to-transparent blur-3xl" />
          </div>

          <div className="relative mx-auto max-w-7xl px-6 py-16 lg:px-8 lg:py-24">
            <div className="mx-auto max-w-2xl text-center">
              <div className="mx-auto mb-6 flex h-14 w-14 items-center justify-center rounded-2xl bg-teal-100">
                <BookOpen className="h-7 w-7 text-teal-600" />
              </div>
              <h1 className="font-serif text-4xl italic tracking-tight text-slate-900 sm:text-5xl">
                Documentation
              </h1>
              <p className="mt-4 text-lg text-slate-600">
                Everything you need to build with StreamHouse. From quick starts to
                deep dives into architecture.
              </p>
            </div>

            {/* Search */}
            <div className="mx-auto mt-10 max-w-xl">
              <div className="relative">
                <input
                  type="text"
                  placeholder="Search documentation..."
                  className="h-12 w-full rounded-xl border border-slate-200 bg-white pl-12 pr-4 text-slate-900 placeholder:text-slate-400 shadow-sm focus:border-teal-500 focus:outline-none focus:ring-2 focus:ring-teal-500/20"
                />
                <Search className="absolute left-4 top-1/2 h-5 w-5 -translate-y-1/2 text-slate-400" />
              </div>
            </div>

            {/* Quick Links */}
            <div className="mx-auto mt-12 grid max-w-4xl gap-4 sm:grid-cols-3">
              {quickLinks.map((link) => (
                <Link
                  key={link.title}
                  href={link.href}
                  className="group flex items-center gap-4 rounded-xl border border-slate-200 bg-white p-4 shadow-sm transition-all hover:border-teal-200 hover:shadow-md"
                >
                  <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg bg-teal-100">
                    <link.icon className="h-5 w-5 text-teal-600" />
                  </div>
                  <div>
                    <h3 className="font-medium text-slate-900 group-hover:text-teal-600">
                      {link.title}
                    </h3>
                    <p className="mt-0.5 text-xs text-slate-500">
                      {link.description}
                    </p>
                  </div>
                </Link>
              ))}
            </div>
          </div>
        </section>

        {/* Documentation Sections */}
        <section className="border-y border-slate-200 bg-white py-16">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="text-center">
              <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                Browse by Topic
              </span>
              <h2 className="mt-3 font-serif text-3xl italic text-slate-900">
                Documentation sections
              </h2>
            </div>

            <div className="mx-auto mt-12 grid max-w-5xl gap-6 md:grid-cols-2 lg:grid-cols-3">
              {sections.map((section) => {
                const colors = colorClasses[section.color] || colorClasses.teal;
                return (
                  <div
                    key={section.title}
                    className={`rounded-2xl border ${colors.border} bg-gradient-to-br from-${section.color}-50/50 to-white p-6`}
                  >
                    <div className="flex items-center gap-3">
                      <div className={`flex h-10 w-10 items-center justify-center rounded-lg ${colors.bg}`}>
                        <section.icon className={`h-5 w-5 ${colors.text}`} />
                      </div>
                      <div>
                        <h3 className="font-semibold text-slate-900">{section.title}</h3>
                        <p className="text-xs text-slate-500">{section.description}</p>
                      </div>
                    </div>

                    <ul className="mt-5 space-y-2">
                      {section.links.map((link) => (
                        <li key={link.title}>
                          <Link
                            href={link.href}
                            className="group flex items-center justify-between rounded-lg px-2 py-1.5 text-sm text-slate-600 transition-colors hover:bg-slate-100 hover:text-slate-900"
                          >
                            <span>{link.title}</span>
                            <span className="text-xs text-slate-400">{link.time}</span>
                          </Link>
                        </li>
                      ))}
                    </ul>
                  </div>
                );
              })}
            </div>
          </div>
        </section>

        {/* CTA */}
        <section className="py-24">
          <div className="mx-auto max-w-4xl px-6 text-center lg:px-8">
            <div className="rounded-3xl border border-teal-200 bg-gradient-to-br from-teal-50 to-cyan-50 p-8 lg:p-12">
              <Zap className="mx-auto h-10 w-10 text-teal-600" />
              <h2 className="mt-4 font-serif text-2xl italic text-slate-900">
                Ready to start building?
              </h2>
              <p className="mt-2 text-slate-600">
                Follow our quick start guide to deploy StreamHouse in minutes.
              </p>
              <div className="mt-8 flex flex-col items-center justify-center gap-4 sm:flex-row">
                <Link href="/docs/quick-start">
                  <Button className="h-11 bg-slate-900 px-6 text-white hover:bg-slate-800">
                    Quick Start Guide
                    <ArrowRight className="ml-2 h-4 w-4" />
                  </Button>
                </Link>
                <Link href="/how-it-works">
                  <Button variant="outline" className="h-11 border-slate-300 px-6">
                    How it Works
                  </Button>
                </Link>
              </div>
            </div>
          </div>
        </section>
      </main>

      {/* Footer */}
      <footer className="border-t border-slate-200 bg-white">
        <div className="mx-auto max-w-7xl px-6 py-12 lg:px-8">
          <div className="flex flex-col items-center justify-between gap-6 md:flex-row">
            <Link href="/" className="flex items-center gap-2">
              <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-gradient-to-br from-teal-500 to-cyan-500">
                <Zap className="h-3.5 w-3.5 text-white" />
              </div>
              <span className="font-semibold text-slate-900">StreamHouse</span>
            </Link>

            <nav className="flex flex-wrap items-center justify-center gap-6">
              {["Documentation", "GitHub", "Blog", "Dashboard"].map((item) => (
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
              ))}
            </nav>

            <p className="text-sm text-slate-400">
              © {new Date().getFullYear()} StreamHouse · MIT License
            </p>
          </div>
        </div>
      </footer>
    </div>
  );
}
