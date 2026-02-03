import Link from "next/link";
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
} from "lucide-react";

const sections = [
  {
    title: "Getting Started",
    description: "Quick guides to get up and running",
    icon: Play,
    color: "blue",
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
    color: "cyan",
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
    color: "teal",
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
    color: "pink",
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
    color: "orange",
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

export default function DocsPage() {
  return (
    <div className="relative">
      {/* Background gradient */}
      <div className="absolute inset-0 bg-gradient-to-b from-cyan-600/5 via-transparent to-transparent" />
      <div className="absolute left-1/2 top-0 h-[400px] w-[600px] -translate-x-1/2 bg-gradient-to-b from-cyan-500/10 via-blue-500/5 to-transparent blur-3xl" />

      <div className="relative mx-auto max-w-7xl px-6 py-16 lg:px-8 lg:py-24">
        {/* Header */}
        <div className="mx-auto max-w-2xl text-center">
          <div className="mx-auto mb-6 flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-cyan-500/20 to-blue-600/10">
            <BookOpen className="h-7 w-7 text-cyan-400" />
          </div>
          <h1 className="text-4xl font-bold tracking-tight text-white sm:text-5xl">
            Documentation
          </h1>
          <p className="mt-4 text-lg text-[#8b90a3]">
            Everything you need to build with StreamHouse. From quick starts to
            deep dives into architecture.
          </p>
        </div>

        {/* Search (placeholder) */}
        <div className="mx-auto mt-10 max-w-xl">
          <div className="relative">
            <input
              type="text"
              placeholder="Search documentation..."
              className="h-12 w-full rounded-xl border border-white/10 bg-white/5 pl-12 pr-4 text-white placeholder:text-[#6b7086] focus:border-cyan-500/50 focus:outline-none focus:ring-1 focus:ring-cyan-500/50"
            />
            <svg
              className="absolute left-4 top-1/2 h-5 w-5 -translate-y-1/2 text-[#6b7086]"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
              />
            </svg>
          </div>
        </div>

        {/* Quick Links */}
        <div className="mx-auto mt-12 grid max-w-4xl gap-4 sm:grid-cols-3">
          {quickLinks.map((link) => (
            <Link
              key={link.title}
              href={link.href}
              className="group flex items-center gap-4 rounded-xl border border-white/5 bg-[#0e0e18] p-4 transition-all hover:border-white/10 hover:bg-[#121220]"
            >
              <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg bg-cyan-500/10">
                <link.icon className="h-5 w-5 text-cyan-400" />
              </div>
              <div>
                <h3 className="font-medium text-white group-hover:text-cyan-400">
                  {link.title}
                </h3>
                <p className="mt-0.5 text-xs text-[#6b7086]">
                  {link.description}
                </p>
              </div>
            </Link>
          ))}
        </div>

        {/* Documentation Sections */}
        <div className="mx-auto mt-16 max-w-5xl">
          <div className="grid gap-8 md:grid-cols-2 lg:grid-cols-3">
            {sections.map((section) => (
              <div
                key={section.title}
                className="rounded-xl border border-white/5 bg-[#0e0e18] p-6"
              >
                <div className="flex items-center gap-3">
                  <div
                    className={`flex h-10 w-10 items-center justify-center rounded-lg bg-${section.color}-500/10`}
                  >
                    <section.icon
                      className={`h-5 w-5 text-${section.color}-400`}
                    />
                  </div>
                  <div>
                    <h3 className="font-semibold text-white">{section.title}</h3>
                    <p className="text-xs text-[#6b7086]">
                      {section.description}
                    </p>
                  </div>
                </div>

                <ul className="mt-5 space-y-2">
                  {section.links.map((link) => (
                    <li key={link.title}>
                      <Link
                        href={link.href}
                        className="group flex items-center justify-between rounded-lg px-2 py-1.5 text-sm text-[#a0a5b8] transition-colors hover:bg-white/5 hover:text-white"
                      >
                        <span>{link.title}</span>
                        <span className="text-xs text-[#6b7086]">
                          {link.time}
                        </span>
                      </Link>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        </div>

        {/* CTA */}
        <div className="mx-auto mt-16 max-w-2xl text-center">
          <div className="rounded-2xl border border-cyan-500/20 bg-gradient-to-br from-cyan-500/10 via-blue-500/5 to-transparent p-8">
            <Zap className="mx-auto h-8 w-8 text-cyan-400" />
            <h3 className="mt-4 text-xl font-semibold text-white">
              Ready to start building?
            </h3>
            <p className="mt-2 text-[#8b90a3]">
              Follow our quick start guide to deploy StreamHouse in minutes.
            </p>
            <Link
              href="/docs/quick-start"
              className="mt-6 inline-flex items-center gap-2 rounded-lg bg-gradient-to-r from-cyan-600 to-blue-500 px-6 py-2.5 text-sm font-medium text-white shadow-lg shadow-cyan-500/20 transition-all hover:shadow-cyan-500/30"
            >
              Quick Start Guide
              <ArrowRight className="h-4 w-4" />
            </Link>
          </div>
        </div>
      </div>
    </div>
  );
}
