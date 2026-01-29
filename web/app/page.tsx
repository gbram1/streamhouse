import Link from "next/link";
import { Button } from "@/components/ui/button";
import {
  ArrowRight,
  Check,
  ChevronRight,
  Cloud,
  Code2,
  Database,
  Layers,
  Server,
  Zap,
} from "lucide-react";

export default function Home() {
  return (
    <div className="min-h-screen bg-[#0a0a12] text-[#e8eaf0] antialiased">
      {/* Header */}
      <header className="sticky top-0 z-50 border-b border-white/5 bg-[#0a0a12]/90 backdrop-blur-xl">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="flex h-16 items-center justify-between">
            <div className="flex items-center gap-8">
              <Link href="/" className="flex items-center gap-2.5">
                <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-gradient-to-br from-blue-500 to-cyan-400">
                  <Zap className="h-4 w-4 text-white" />
                </div>
                <span className="text-lg font-semibold tracking-tight">
                  StreamHouse
                </span>
              </Link>
              <nav className="hidden items-center gap-1 lg:flex">
                <Link
                  href="/product"
                  className="rounded-lg px-3 py-2 text-sm text-[#a0a5b8] transition-colors hover:text-white"
                >
                  Product
                </Link>
                <Link
                  href="/docs"
                  className="rounded-lg px-3 py-2 text-sm text-[#a0a5b8] transition-colors hover:text-white"
                >
                  Documentation
                </Link>
                <Link
                  href="/pricing"
                  className="rounded-lg px-3 py-2 text-sm text-[#a0a5b8] transition-colors hover:text-white"
                >
                  Pricing
                </Link>
                <Link
                  href="/blog"
                  className="rounded-lg px-3 py-2 text-sm text-[#a0a5b8] transition-colors hover:text-white"
                >
                  Blog
                </Link>
              </nav>
            </div>
            <div className="flex items-center gap-3">
              <Link
                href="https://github.com/streamhouse/streamhouse"
                className="hidden rounded-lg px-3 py-2 text-sm text-[#a0a5b8] transition-colors hover:text-white sm:block"
              >
                GitHub
              </Link>
              <Link href="/dashboard">
                <Button
                  variant="ghost"
                  className="text-sm text-[#a0a5b8] hover:bg-white/5 hover:text-white"
                >
                  Sign in
                </Button>
              </Link>
              <Link href="/console">
                <Button className="bg-gradient-to-r from-blue-600 to-blue-500 text-sm font-medium shadow-lg shadow-blue-500/20 transition-all hover:shadow-blue-500/30">
                  Get Started
                </Button>
              </Link>
            </div>
          </div>
        </div>
      </header>

      <main>
        {/* Hero Section */}
        <section className="relative overflow-hidden">
          {/* Background gradient */}
          <div className="absolute inset-0 bg-gradient-to-b from-blue-600/5 via-transparent to-transparent" />
          <div className="absolute left-1/2 top-0 h-[600px] w-[800px] -translate-x-1/2 bg-gradient-to-b from-blue-500/10 via-cyan-500/5 to-transparent blur-3xl" />

          <div className="relative mx-auto max-w-7xl px-6 pb-24 pt-20 lg:px-8 lg:pb-32 lg:pt-28">
            <div className="mx-auto max-w-3xl text-center">
              {/* Announcement badge */}
              <Link
                href="/blog/phase-7-release"
                className="mb-8 inline-flex items-center gap-2 rounded-full border border-blue-500/20 bg-blue-500/5 px-4 py-1.5 text-sm text-blue-400 transition-colors hover:bg-blue-500/10"
              >
                <span className="font-medium">Phase 7 Released</span>
                <ChevronRight className="h-3.5 w-3.5" />
              </Link>

              {/* Headline */}
              <h1 className="text-4xl font-bold tracking-tight sm:text-5xl lg:text-6xl">
                <span className="block text-white">Event streaming</span>
                <span className="mt-1 block bg-gradient-to-r from-blue-400 via-cyan-400 to-teal-400 bg-clip-text text-transparent">
                  built for the cloud
                </span>
              </h1>

              {/* Subheadline */}
              <p className="mx-auto mt-6 max-w-2xl text-lg leading-relaxed text-[#8b90a3]">
                StreamHouse is an S3-native streaming platform that eliminates
                cluster management while cutting costs by 80%. No disks. No
                replication. No complexity.
              </p>

              {/* CTAs */}
              <div className="mt-10 flex flex-col items-center justify-center gap-4 sm:flex-row">
                <Link href="/console">
                  <Button
                    size="lg"
                    className="h-12 bg-gradient-to-r from-blue-600 to-blue-500 px-6 text-base font-medium shadow-lg shadow-blue-500/25 transition-all hover:shadow-blue-500/40"
                  >
                    Start building free
                    <ArrowRight className="ml-2 h-4 w-4" />
                  </Button>
                </Link>
                <Link href="/docs">
                  <Button
                    size="lg"
                    variant="outline"
                    className="h-12 border-white/10 bg-white/5 px-6 text-base font-medium text-white hover:bg-white/10"
                  >
                    Read the docs
                  </Button>
                </Link>
              </div>

              {/* Social proof */}
              <p className="mt-10 text-sm text-[#6b7086]">
                Open source · Built with Rust · Production ready
              </p>
            </div>
          </div>
        </section>

        {/* Value Props */}
        <section className="border-y border-white/5 bg-[#080810]">
          <div className="mx-auto max-w-7xl px-6 py-16 lg:px-8">
            <div className="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
              <div className="text-center">
                <div className="text-3xl font-bold text-white lg:text-4xl">
                  80%
                </div>
                <div className="mt-1 text-sm text-[#6b7086]">
                  lower infrastructure costs
                </div>
              </div>
              <div className="text-center">
                <div className="text-3xl font-bold text-white lg:text-4xl">
                  Zero
                </div>
                <div className="mt-1 text-sm text-[#6b7086]">
                  disks to manage
                </div>
              </div>
              <div className="text-center">
                <div className="text-3xl font-bold text-white lg:text-4xl">
                  11 nines
                </div>
                <div className="mt-1 text-sm text-[#6b7086]">
                  durability via S3
                </div>
              </div>
              <div className="text-center">
                <div className="text-3xl font-bold text-white lg:text-4xl">
                  50K+
                </div>
                <div className="mt-1 text-sm text-[#6b7086]">
                  messages per second
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Problem/Solution */}
        <section className="py-24 lg:py-32">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="mx-auto max-w-2xl text-center">
              <h2 className="text-3xl font-bold tracking-tight text-white sm:text-4xl">
                Kafka wasn&apos;t built for the cloud
              </h2>
              <p className="mt-4 text-lg text-[#8b90a3]">
                Traditional brokers require expensive disks, complex
                replication, and constant operational overhead. StreamHouse
                takes a different approach.
              </p>
            </div>

            <div className="mx-auto mt-16 grid max-w-5xl gap-6 lg:grid-cols-2">
              {/* Traditional */}
              <div className="rounded-2xl border border-white/5 bg-[#0e0e18] p-8">
                <div className="flex items-center gap-3">
                  <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-red-500/10">
                    <Server className="h-5 w-5 text-red-400" />
                  </div>
                  <h3 className="text-lg font-semibold text-white">
                    Traditional Kafka
                  </h3>
                </div>
                <ul className="mt-6 space-y-4">
                  {[
                    "Local disk storage with 3x replication",
                    "ZooKeeper or KRaft coordination layer",
                    "Complex partition rebalancing operations",
                    "Expensive inter-AZ network traffic",
                    "Manual capacity planning and scaling",
                  ].map((item, i) => (
                    <li
                      key={i}
                      className="flex items-start gap-3 text-[#6b7086]"
                    >
                      <span className="mt-1.5 block h-1.5 w-1.5 flex-shrink-0 rounded-full bg-red-400/50" />
                      {item}
                    </li>
                  ))}
                </ul>
              </div>

              {/* StreamHouse */}
              <div className="rounded-2xl border border-blue-500/20 bg-gradient-to-br from-blue-500/5 to-cyan-500/5 p-8">
                <div className="flex items-center gap-3">
                  <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-500/10">
                    <Zap className="h-5 w-5 text-blue-400" />
                  </div>
                  <h3 className="text-lg font-semibold text-white">
                    StreamHouse
                  </h3>
                </div>
                <ul className="mt-6 space-y-4">
                  {[
                    "S3-native storage with built-in durability",
                    "Stateless agents with lease-based coordination",
                    "Automatic partition assignment",
                    "Zero inter-broker replication traffic",
                    "Infinite scale, pay only for what you use",
                  ].map((item, i) => (
                    <li
                      key={i}
                      className="flex items-start gap-3 text-[#c8cde4]"
                    >
                      <Check className="mt-0.5 h-4 w-4 flex-shrink-0 text-blue-400" />
                      {item}
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          </div>
        </section>

        {/* Architecture */}
        <section className="border-y border-white/5 bg-[#080810] py-24 lg:py-32">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="mx-auto max-w-2xl text-center">
              <h2 className="text-3xl font-bold tracking-tight text-white sm:text-4xl">
                Disaggregated architecture
              </h2>
              <p className="mt-4 text-lg text-[#8b90a3]">
                Compute and storage scale independently. Agents are stateless.
                S3 provides durability. Metadata is lightweight.
              </p>
            </div>

            <div className="mx-auto mt-16 grid max-w-4xl gap-8 lg:grid-cols-3">
              <div className="text-center">
                <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-blue-500/20 to-blue-600/10">
                  <Layers className="h-6 w-6 text-blue-400" />
                </div>
                <h3 className="mt-5 text-lg font-semibold text-white">
                  Stateless Agents
                </h3>
                <p className="mt-2 text-sm leading-relaxed text-[#6b7086]">
                  Ephemeral compute that can be killed, restarted, or scaled
                  without data loss. No persistent state on the broker.
                </p>
              </div>

              <div className="text-center">
                <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-cyan-500/20 to-cyan-600/10">
                  <Cloud className="h-6 w-6 text-cyan-400" />
                </div>
                <h3 className="mt-5 text-lg font-semibold text-white">
                  S3 Storage
                </h3>
                <p className="mt-2 text-sm leading-relaxed text-[#6b7086]">
                  Events written directly to object storage. 11 nines
                  durability, infinite capacity, fraction of the cost.
                </p>
              </div>

              <div className="text-center">
                <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-teal-500/20 to-teal-600/10">
                  <Database className="h-6 w-6 text-teal-400" />
                </div>
                <h3 className="mt-5 text-lg font-semibold text-white">
                  Metadata Store
                </h3>
                <p className="mt-2 text-sm leading-relaxed text-[#6b7086]">
                  Lightweight coordination via PostgreSQL. Only offsets, leases,
                  and watermarks—no event data.
                </p>
              </div>
            </div>

            {/* Flow diagram */}
            <div className="mx-auto mt-16 max-w-3xl">
              <div className="rounded-2xl border border-white/5 bg-[#0a0a12] p-8">
                <div className="flex items-center justify-between">
                  <div className="flex flex-col items-center">
                    <div className="flex h-12 w-20 items-center justify-center rounded-lg border border-blue-500/30 bg-blue-500/10">
                      <span className="text-xs font-medium text-blue-400">
                        Producer
                      </span>
                    </div>
                  </div>

                  <div className="flex-1 border-t border-dashed border-white/10" />

                  <div className="flex flex-col items-center">
                    <div className="flex h-12 w-20 items-center justify-center rounded-lg border border-cyan-500/30 bg-cyan-500/10">
                      <span className="text-xs font-medium text-cyan-400">
                        Agent
                      </span>
                    </div>
                  </div>

                  <div className="flex-1 border-t border-dashed border-white/10" />

                  <div className="flex flex-col items-center">
                    <div className="flex h-12 w-20 items-center justify-center rounded-lg border border-teal-500/30 bg-teal-500/10">
                      <span className="text-xs font-medium text-teal-400">
                        S3
                      </span>
                    </div>
                  </div>

                  <div className="flex-1 border-t border-dashed border-white/10" />

                  <div className="flex flex-col items-center">
                    <div className="flex h-12 w-20 items-center justify-center rounded-lg border border-purple-500/30 bg-purple-500/10">
                      <span className="text-xs font-medium text-purple-400">
                        Consumer
                      </span>
                    </div>
                  </div>
                </div>

                <p className="mt-6 text-center text-sm text-[#6b7086]">
                  gRPC ingestion → Segment buffering → S3 persistence → Cached
                  reads
                </p>
              </div>
            </div>
          </div>
        </section>

        {/* Features */}
        <section className="py-24 lg:py-32">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="mx-auto max-w-2xl text-center">
              <h2 className="text-3xl font-bold tracking-tight text-white sm:text-4xl">
                Everything you need to stream
              </h2>
              <p className="mt-4 text-lg text-[#8b90a3]">
                Production-ready features without the operational burden.
              </p>
            </div>

            <div className="mx-auto mt-16 grid max-w-5xl gap-8 sm:grid-cols-2 lg:grid-cols-3">
              {[
                {
                  icon: Cloud,
                  title: "S3-Native Storage",
                  description:
                    "Direct writes to object storage with LZ4 compression. No local disks to manage or replicate.",
                  color: "blue",
                },
                {
                  icon: Layers,
                  title: "Zero Replication",
                  description:
                    "Only metadata is replicated. S3 provides durability. No inter-broker traffic costs.",
                  color: "cyan",
                },
                {
                  icon: Code2,
                  title: "SQL Processing",
                  description:
                    "Built-in stream processing with SQL. No separate Flink cluster required.",
                  color: "teal",
                },
                {
                  icon: Server,
                  title: "Stateless Agents",
                  description:
                    "Kill, restart, or scale agents freely. All state lives in S3 and the metadata store.",
                  color: "purple",
                },
                {
                  icon: Database,
                  title: "Smart Caching",
                  description:
                    "LRU caching reduces S3 reads by 80-95%. Hot data from memory, cold data on demand.",
                  color: "pink",
                },
                {
                  icon: Zap,
                  title: "Full Observability",
                  description:
                    "Prometheus metrics, health endpoints, consumer lag monitoring, Grafana dashboards.",
                  color: "orange",
                },
              ].map((feature) => (
                <div
                  key={feature.title}
                  className="group rounded-xl border border-white/5 bg-[#0e0e18] p-6 transition-colors hover:border-white/10"
                >
                  <div
                    className={`flex h-10 w-10 items-center justify-center rounded-lg bg-${feature.color}-500/10`}
                  >
                    <feature.icon
                      className={`h-5 w-5 text-${feature.color}-400`}
                    />
                  </div>
                  <h3 className="mt-4 font-semibold text-white">
                    {feature.title}
                  </h3>
                  <p className="mt-2 text-sm leading-relaxed text-[#6b7086]">
                    {feature.description}
                  </p>
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* Code Example */}
        <section className="border-y border-white/5 bg-[#080810] py-24 lg:py-32">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="mx-auto max-w-2xl text-center">
              <h2 className="text-3xl font-bold tracking-tight text-white sm:text-4xl">
                Up and running in minutes
              </h2>
              <p className="mt-4 text-lg text-[#8b90a3]">
                Start streaming with a few commands. No complex configuration.
              </p>
            </div>

            <div className="mx-auto mt-12 max-w-3xl">
              <div className="overflow-hidden rounded-xl border border-white/10 bg-[#0a0a12]">
                {/* Terminal header */}
                <div className="flex items-center gap-2 border-b border-white/5 bg-white/[0.02] px-4 py-3">
                  <div className="h-2.5 w-2.5 rounded-full bg-[#ff5f57]" />
                  <div className="h-2.5 w-2.5 rounded-full bg-[#febc2e]" />
                  <div className="h-2.5 w-2.5 rounded-full bg-[#28c840]" />
                  <span className="ml-3 text-xs text-[#6b7086]">terminal</span>
                </div>

                {/* Terminal content */}
                <div className="p-6 font-mono text-sm">
                  <div className="space-y-4">
                    <div className="text-[#6b7086]"># Start infrastructure</div>
                    <div>
                      <span className="text-cyan-400">$</span>{" "}
                      <span className="text-white">docker compose up -d</span>
                    </div>
                    <div className="pl-4 text-[#4ade80]">
                      ✓ minio started
                      <br />✓ postgres started
                    </div>

                    <div className="pt-2 text-[#6b7086]"># Start an agent</div>
                    <div>
                      <span className="text-cyan-400">$</span>{" "}
                      <span className="text-white">
                        cargo run --bin agent --features metrics
                      </span>
                    </div>
                    <div className="pl-4 text-[#4ade80]">
                      ✓ Agent listening on :9090
                      <br />✓ Metrics on :8080
                    </div>

                    <div className="pt-2 text-[#6b7086]">
                      # Create topic and produce
                    </div>
                    <div>
                      <span className="text-cyan-400">$</span>{" "}
                      <span className="text-white">
                        streamctl topics create events -p 4
                      </span>
                    </div>
                    <div>
                      <span className="text-cyan-400">$</span>{" "}
                      <span className="text-white">
                        streamctl produce events -k user1 -v
                        &apos;{`{"action":"click"}`}&apos;
                      </span>
                    </div>
                    <div className="pl-4 text-blue-400">
                      → partition=2 offset=0
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Use Cases */}
        <section className="py-24 lg:py-32">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="mx-auto max-w-2xl text-center">
              <h2 className="text-3xl font-bold tracking-tight text-white sm:text-4xl">
                Built for modern data teams
              </h2>
              <p className="mt-4 text-lg text-[#8b90a3]">
                Replace complex infrastructure with a unified streaming
                platform.
              </p>
            </div>

            <div className="mx-auto mt-16 grid max-w-5xl gap-6 lg:grid-cols-2">
              {[
                {
                  title: "Real-time analytics",
                  description:
                    "Replace Kafka → Flink → ClickHouse with integrated SQL stream processing. 50% cost reduction.",
                  badge: "Analytics",
                },
                {
                  title: "Log aggregation",
                  description:
                    "Store logs in S3 at $0.023/GB instead of expensive cloud logging. Infinite retention.",
                  badge: "Observability",
                },
                {
                  title: "ML feature pipelines",
                  description:
                    "Compute real-time features with SQL. Direct S3 output for training pipelines.",
                  badge: "ML/AI",
                },
                {
                  title: "Change data capture",
                  description:
                    "Simplified CDC with built-in transformations. Fewer moving parts, easier debugging.",
                  badge: "Data Sync",
                },
              ].map((useCase) => (
                <div
                  key={useCase.title}
                  className="rounded-xl border border-white/5 bg-[#0e0e18] p-6"
                >
                  <span className="inline-block rounded-full bg-blue-500/10 px-3 py-1 text-xs font-medium text-blue-400">
                    {useCase.badge}
                  </span>
                  <h3 className="mt-4 text-lg font-semibold text-white">
                    {useCase.title}
                  </h3>
                  <p className="mt-2 text-sm leading-relaxed text-[#6b7086]">
                    {useCase.description}
                  </p>
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* Tech */}
        <section className="border-t border-white/5 bg-[#080810] py-16">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="flex flex-wrap items-center justify-center gap-x-12 gap-y-6 text-[#6b7086]">
              <div className="flex items-center gap-2">
                <span className="text-lg">🦀</span>
                <span className="text-sm">Rust</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-lg">📡</span>
                <span className="text-sm">gRPC</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-lg">☁️</span>
                <span className="text-sm">Amazon S3</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-lg">🐘</span>
                <span className="text-sm">PostgreSQL</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-lg">📊</span>
                <span className="text-sm">Prometheus</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-lg">🗜️</span>
                <span className="text-sm">LZ4</span>
              </div>
            </div>
          </div>
        </section>

        {/* CTA */}
        <section className="py-24 lg:py-32">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="relative overflow-hidden rounded-3xl bg-gradient-to-br from-blue-600/20 via-cyan-600/10 to-teal-600/20 px-8 py-16 sm:px-16 lg:py-24">
              {/* Background effects */}
              <div className="absolute inset-0 bg-[#0a0a12]/80" />
              <div className="absolute left-1/4 top-0 h-64 w-64 bg-blue-500/20 blur-3xl" />
              <div className="absolute bottom-0 right-1/4 h-64 w-64 bg-cyan-500/20 blur-3xl" />

              <div className="relative mx-auto max-w-2xl text-center">
                <h2 className="text-3xl font-bold tracking-tight text-white sm:text-4xl">
                  Ready to simplify your streaming?
                </h2>
                <p className="mx-auto mt-4 max-w-xl text-lg text-[#8b90a3]">
                  Join teams cutting infrastructure costs by 80% while
                  eliminating operational complexity.
                </p>
                <div className="mt-10 flex flex-col items-center justify-center gap-4 sm:flex-row">
                  <Link href="/console">
                    <Button
                      size="lg"
                      className="h-12 bg-white px-8 text-base font-medium text-[#0a0a12] hover:bg-white/90"
                    >
                      Get started free
                      <ArrowRight className="ml-2 h-4 w-4" />
                    </Button>
                  </Link>
                  <Link href="/docs">
                    <Button
                      size="lg"
                      variant="outline"
                      className="h-12 border-white/20 bg-transparent px-8 text-base font-medium text-white hover:bg-white/10"
                    >
                      Read documentation
                    </Button>
                  </Link>
                </div>
              </div>
            </div>
          </div>
        </section>
      </main>

      {/* Footer */}
      <footer className="border-t border-white/5 bg-[#080810]">
        <div className="mx-auto max-w-7xl px-6 py-16 lg:px-8">
          <div className="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
            <div>
              <Link href="/" className="flex items-center gap-2.5">
                <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-gradient-to-br from-blue-500 to-cyan-400">
                  <Zap className="h-3.5 w-3.5 text-white" />
                </div>
                <span className="text-base font-semibold">StreamHouse</span>
              </Link>
              <p className="mt-4 text-sm text-[#6b7086]">
                S3-native event streaming.
                <br />
                Built with Rust. Production ready.
              </p>
            </div>

            <div>
              <h4 className="text-sm font-semibold text-white">Product</h4>
              <ul className="mt-4 space-y-3">
                {["Documentation", "Pricing", "Dashboard", "Console"].map(
                  (item) => (
                    <li key={item}>
                      <Link
                        href={`/${item.toLowerCase()}`}
                        className="text-sm text-[#6b7086] transition-colors hover:text-white"
                      >
                        {item}
                      </Link>
                    </li>
                  )
                )}
              </ul>
            </div>

            <div>
              <h4 className="text-sm font-semibold text-white">Resources</h4>
              <ul className="mt-4 space-y-3">
                {["Quick Start", "Architecture", "API Reference", "Blog"].map(
                  (item) => (
                    <li key={item}>
                      <Link
                        href="/docs"
                        className="text-sm text-[#6b7086] transition-colors hover:text-white"
                      >
                        {item}
                      </Link>
                    </li>
                  )
                )}
              </ul>
            </div>

            <div>
              <h4 className="text-sm font-semibold text-white">Community</h4>
              <ul className="mt-4 space-y-3">
                {["GitHub", "Discord", "Twitter"].map((item) => (
                  <li key={item}>
                    <Link
                      href="#"
                      className="text-sm text-[#6b7086] transition-colors hover:text-white"
                    >
                      {item}
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
          </div>

          <div className="mt-12 border-t border-white/5 pt-8">
            <p className="text-center text-sm text-[#6b7086]">
              © {new Date().getFullYear()} StreamHouse. Open source under MIT
              license.
            </p>
          </div>
        </div>
      </footer>
    </div>
  );
}
