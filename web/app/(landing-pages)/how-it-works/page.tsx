"use client";

import Link from "next/link";
import { Button } from "@/components/ui/button";
import {
  ArrowRight,
  ArrowDown,
  ArrowUpRight,
  Cloud,
  Database,
  HardDrive,
  Layers,
  Server,
  Shield,
  Zap,
  Check,
  Clock,
  Cpu,
  FileText,
  Lock,
  RefreshCw,
  Search,
  Archive,
  Activity,
  GitBranch,
} from "lucide-react";
import { useEffect, useState } from "react";

function GithubIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
    </svg>
  );
}

// Animated flow arrow component
function FlowArrow({ direction = "down", className = "" }: { direction?: "down" | "right"; className?: string }) {
  return (
    <div className={`flex items-center justify-center ${className}`}>
      {direction === "down" ? (
        <div className="flex flex-col items-center">
          <div className="h-8 w-px bg-gradient-to-b from-teal-300 to-teal-500" />
          <ArrowDown className="h-4 w-4 text-teal-500 -mt-1" />
        </div>
      ) : (
        <div className="flex items-center">
          <div className="w-8 h-px bg-gradient-to-r from-teal-300 to-teal-500" />
          <ArrowRight className="h-4 w-4 text-teal-500 -ml-1" />
        </div>
      )}
    </div>
  );
}

// Step component for data flow
function FlowStep({
  number,
  title,
  description,
  icon: Icon,
  color = "teal"
}: {
  number: number;
  title: string;
  description: string;
  icon: React.ComponentType<{ className?: string }>;
  color?: "teal" | "blue" | "amber" | "rose";
}) {
  const colorClasses = {
    teal: "from-teal-500 to-cyan-500 bg-teal-50",
    blue: "from-blue-500 to-indigo-500 bg-blue-50",
    amber: "from-amber-500 to-orange-500 bg-amber-50",
    rose: "from-rose-500 to-pink-500 bg-rose-50",
  };

  return (
    <div className="flex gap-4">
      <div className="flex flex-col items-center">
        <div className={`flex h-10 w-10 items-center justify-center rounded-full bg-gradient-to-br ${colorClasses[color].split(" ")[0]} ${colorClasses[color].split(" ")[1]} text-white text-sm font-bold`}>
          {number}
        </div>
      </div>
      <div className="flex-1 pb-8">
        <div className="flex items-center gap-3">
          <div className={`flex h-8 w-8 items-center justify-center rounded-lg ${colorClasses[color].split(" ")[2]}`}>
            <Icon className={`h-4 w-4 text-${color}-600`} />
          </div>
          <h4 className="font-semibold text-slate-900">{title}</h4>
        </div>
        <p className="mt-2 text-sm text-slate-600">{description}</p>
      </div>
    </div>
  );
}

export default function HowItWorks() {
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);

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
                    item.name === "How it Works" ? "text-teal-600 font-medium" : "text-slate-600"
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

      <main>
        {/* Hero Section */}
        <section className="relative overflow-hidden pt-16">
          {/* Gradient orbs */}
          <div className="pointer-events-none absolute inset-0">
            <div className="absolute left-1/4 top-32 h-[500px] w-[500px] rounded-full bg-gradient-to-br from-teal-200/50 via-cyan-200/30 to-transparent blur-3xl" />
            <div className="absolute right-1/4 top-48 h-[400px] w-[400px] rounded-full bg-gradient-to-bl from-blue-200/40 via-indigo-100/20 to-transparent blur-3xl" />
          </div>

          <div className="relative mx-auto max-w-7xl px-6 pb-16 pt-24 text-center lg:px-8 lg:pt-32">
            <span className="inline-block rounded-full bg-teal-100 px-4 py-1.5 text-sm font-semibold text-teal-700">
              Deep Dive
            </span>

            <h1 className="mx-auto mt-6 max-w-4xl text-4xl font-light tracking-tight text-slate-900 sm:text-5xl lg:text-6xl">
              <span className="font-serif italic">How</span>
              <span className="bg-gradient-to-r from-teal-600 to-cyan-600 bg-clip-text font-serif italic text-transparent"> StreamHouse </span>
              <span className="font-serif italic">works</span>
            </h1>

            <p className="mx-auto mt-6 max-w-2xl text-lg text-slate-600">
              A complete guide to how StreamHouse replaces Kafka + Flink with a single,
              cloud-native streaming platform. Understand the architecture, data flow,
              and why your data is always safe.
            </p>
          </div>
        </section>

        {/* The Problem Section */}
        <section className="border-y border-slate-200 bg-white py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="text-center">
              <span className="text-sm font-semibold uppercase tracking-wider text-slate-500">
                The Challenge
              </span>
              <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                Why we built something new
              </h2>
            </div>

            <div className="mt-16 grid gap-8 lg:grid-cols-2">
              {/* Traditional Architecture */}
              <div className="rounded-2xl border border-red-200 bg-red-50/50 p-8">
                <h3 className="flex items-center gap-2 text-lg font-semibold text-slate-900">
                  <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-red-100">
                    <Server className="h-4 w-4 text-red-600" />
                  </div>
                  Traditional: Kafka + Flink
                </h3>

                <div className="mt-6 space-y-4">
                  <div className="rounded-xl bg-white p-4 shadow-sm">
                    <div className="flex items-center justify-between">
                      <span className="text-sm font-medium text-slate-700">Kafka Brokers</span>
                      <span className="text-xs text-slate-500">Stateful, local disks</span>
                    </div>
                    <div className="mt-3 flex gap-2">
                      {[1, 2, 3].map((i) => (
                        <div key={i} className="flex-1 rounded-lg bg-slate-100 p-3 text-center">
                          <HardDrive className="mx-auto h-5 w-5 text-slate-400" />
                          <span className="mt-1 block text-xs text-slate-500">Broker {i}</span>
                        </div>
                      ))}
                    </div>
                  </div>

                  <FlowArrow direction="down" />

                  <div className="rounded-xl bg-white p-4 shadow-sm">
                    <div className="flex items-center justify-between">
                      <span className="text-sm font-medium text-slate-700">Flink Cluster</span>
                      <span className="text-xs text-slate-500">Separate system</span>
                    </div>
                    <div className="mt-3 flex gap-2">
                      {[1, 2].map((i) => (
                        <div key={i} className="flex-1 rounded-lg bg-slate-100 p-3 text-center">
                          <Cpu className="mx-auto h-5 w-5 text-slate-400" />
                          <span className="mt-1 block text-xs text-slate-500">TaskManager</span>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>

                <div className="mt-6 space-y-2 text-sm text-slate-600">
                  <p>• 3x storage for replication</p>
                  <p>• Two systems to operate</p>
                  <p>• Complex failover</p>
                  <p>• Stateful brokers = hard scaling</p>
                </div>
              </div>

              {/* StreamHouse Architecture */}
              <div className="rounded-2xl border border-teal-200 bg-gradient-to-br from-teal-50 to-cyan-50 p-8">
                <h3 className="flex items-center gap-2 text-lg font-semibold text-slate-900">
                  <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-teal-100">
                    <Zap className="h-4 w-4 text-teal-600" />
                  </div>
                  StreamHouse: Unified & Stateless
                </h3>

                <div className="mt-6 space-y-4">
                  <div className="rounded-xl bg-white/80 p-4 shadow-sm backdrop-blur">
                    <div className="flex items-center justify-between">
                      <span className="text-sm font-medium text-slate-700">Stateless Agents</span>
                      <span className="text-xs text-teal-600">Ephemeral compute</span>
                    </div>
                    <div className="mt-3 flex gap-2">
                      {[1, 2, 3].map((i) => (
                        <div key={i} className="flex-1 rounded-lg bg-teal-50 p-3 text-center">
                          <Layers className="mx-auto h-5 w-5 text-teal-500" />
                          <span className="mt-1 block text-xs text-teal-600">Agent {i}</span>
                        </div>
                      ))}
                    </div>
                  </div>

                  <FlowArrow direction="down" />

                  <div className="grid grid-cols-2 gap-3">
                    <div className="rounded-xl bg-white/80 p-4 shadow-sm backdrop-blur">
                      <Database className="h-5 w-5 text-blue-500" />
                      <span className="mt-2 block text-sm font-medium text-slate-700">PostgreSQL</span>
                      <span className="text-xs text-slate-500">Metadata only</span>
                    </div>
                    <div className="rounded-xl bg-white/80 p-4 shadow-sm backdrop-blur">
                      <Cloud className="h-5 w-5 text-cyan-500" />
                      <span className="mt-2 block text-sm font-medium text-slate-700">S3 Storage</span>
                      <span className="text-xs text-slate-500">All data</span>
                    </div>
                  </div>
                </div>

                <div className="mt-6 space-y-2 text-sm text-slate-700">
                  <p className="flex items-center gap-2">
                    <Check className="h-4 w-4 text-teal-500" />
                    11 nines durability via S3
                  </p>
                  <p className="flex items-center gap-2">
                    <Check className="h-4 w-4 text-teal-500" />
                    One system, SQL built-in
                  </p>
                  <p className="flex items-center gap-2">
                    <Check className="h-4 w-4 text-teal-500" />
                    Instant failover (lease-based)
                  </p>
                  <p className="flex items-center gap-2">
                    <Check className="h-4 w-4 text-teal-500" />
                    Scale agents freely
                  </p>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Architecture Overview */}
        <section className="py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="text-center">
              <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                Architecture
              </span>
              <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                The complete picture
              </h2>
              <p className="mx-auto mt-4 max-w-2xl text-slate-600">
                StreamHouse separates compute from storage. Agents are stateless and can be
                killed or restarted instantly. All durability comes from S3 and PostgreSQL.
              </p>
            </div>

            {/* Architecture Diagram */}
            <div className="mt-16">
              <div
                className="relative rounded-3xl border border-slate-200 bg-white p-8 lg:p-12"
                style={{
                  opacity: mounted ? 1 : 0,
                  transform: mounted ? "translateY(0)" : "translateY(20px)",
                  transition: "all 0.6s ease",
                }}
              >
                {/* Applications Layer */}
                <div className="text-center">
                  <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
                    Your Applications
                  </span>
                  <div className="mt-4 flex flex-wrap items-center justify-center gap-3">
                    {["Rust SDK", "Python SDK", "Node SDK", "Java SDK", "gRPC", "REST API"].map((sdk) => (
                      <div
                        key={sdk}
                        className="rounded-full border border-slate-200 bg-slate-50 px-4 py-2 text-sm text-slate-600"
                      >
                        {sdk}
                      </div>
                    ))}
                  </div>
                </div>

                <div className="my-8 flex justify-center">
                  <div className="flex flex-col items-center">
                    <div className="h-12 w-px bg-gradient-to-b from-slate-200 to-teal-400" />
                    <span className="my-2 rounded-full bg-teal-100 px-3 py-1 text-xs font-medium text-teal-700">
                      gRPC / HTTP
                    </span>
                    <div className="h-12 w-px bg-gradient-to-b from-teal-400 to-slate-200" />
                  </div>
                </div>

                {/* Agents Layer */}
                <div className="rounded-2xl border-2 border-dashed border-teal-200 bg-teal-50/50 p-6">
                  <div className="mb-4 text-center">
                    <span className="text-xs font-semibold uppercase tracking-wider text-teal-600">
                      Stateless Agent Pool
                    </span>
                  </div>
                  <div className="flex flex-wrap items-center justify-center gap-4">
                    {[1, 2, 3, 4].map((i) => (
                      <div
                        key={i}
                        className="flex items-center gap-3 rounded-xl border border-teal-200 bg-white px-4 py-3 shadow-sm"
                      >
                        <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-gradient-to-br from-teal-500 to-cyan-500">
                          <Layers className="h-5 w-5 text-white" />
                        </div>
                        <div>
                          <p className="text-sm font-medium text-slate-900">Agent {i}</p>
                          <p className="text-xs text-slate-500">Ephemeral</p>
                        </div>
                      </div>
                    ))}
                    <div className="flex items-center gap-2 rounded-xl border-2 border-dashed border-slate-300 px-4 py-3">
                      <span className="text-2xl text-slate-400">+</span>
                      <span className="text-sm text-slate-500">Scale freely</span>
                    </div>
                  </div>
                </div>

                <div className="my-8 flex items-center justify-center gap-16">
                  <div className="flex flex-col items-center">
                    <div className="h-12 w-px bg-gradient-to-b from-slate-200 to-blue-400" />
                    <span className="my-2 rounded-full bg-blue-100 px-3 py-1 text-xs font-medium text-blue-700">
                      Metadata
                    </span>
                    <div className="h-12 w-px bg-gradient-to-b from-blue-400 to-slate-200" />
                  </div>
                  <div className="flex flex-col items-center">
                    <div className="h-12 w-px bg-gradient-to-b from-slate-200 to-cyan-400" />
                    <span className="my-2 rounded-full bg-cyan-100 px-3 py-1 text-xs font-medium text-cyan-700">
                      Segments
                    </span>
                    <div className="h-12 w-px bg-gradient-to-b from-cyan-400 to-slate-200" />
                  </div>
                </div>

                {/* Storage Layer */}
                <div className="grid gap-6 lg:grid-cols-2">
                  <div className="rounded-2xl border border-blue-200 bg-blue-50/50 p-6">
                    <div className="flex items-center gap-3">
                      <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-blue-100">
                        <Database className="h-6 w-6 text-blue-600" />
                      </div>
                      <div>
                        <h4 className="font-semibold text-slate-900">PostgreSQL</h4>
                        <p className="text-sm text-slate-600">Metadata Store</p>
                      </div>
                    </div>
                    <div className="mt-4 space-y-2 text-sm text-slate-600">
                      <p>• Topic & partition info</p>
                      <p>• Consumer offsets</p>
                      <p>• Segment locations</p>
                      <p>• Partition leases</p>
                    </div>
                  </div>

                  <div className="rounded-2xl border border-cyan-200 bg-cyan-50/50 p-6">
                    <div className="flex items-center gap-3">
                      <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-cyan-100">
                        <Cloud className="h-6 w-6 text-cyan-600" />
                      </div>
                      <div>
                        <h4 className="font-semibold text-slate-900">S3 / Object Storage</h4>
                        <p className="text-sm text-slate-600">Durable Data Store</p>
                      </div>
                    </div>
                    <div className="mt-4 space-y-2 text-sm text-slate-600">
                      <p>• Compressed segment files</p>
                      <p>• 11 nines durability</p>
                      <p>• Infinite capacity</p>
                      <p>• $0.023/GB/month</p>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Data Flow Section */}
        <section className="border-y border-slate-200 bg-gradient-to-b from-slate-50 to-white py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="text-center">
              <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                Data Flow
              </span>
              <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                From producer to consumer
              </h2>
              <p className="mx-auto mt-4 max-w-2xl text-slate-600">
                Follow a message through the system and understand exactly how
                durability and performance are achieved at each step.
              </p>
            </div>

            <div className="mt-16 grid gap-8 lg:grid-cols-2">
              {/* Write Path */}
              <div className="rounded-2xl border border-slate-200 bg-white p-8">
                <div className="mb-8 flex items-center gap-3">
                  <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-gradient-to-br from-teal-500 to-cyan-500">
                    <ArrowDown className="h-5 w-5 text-white" />
                  </div>
                  <div>
                    <h3 className="text-lg font-semibold text-slate-900">Write Path</h3>
                    <p className="text-sm text-slate-500">Producer → S3</p>
                  </div>
                </div>

                <div className="space-y-1">
                  <FlowStep
                    number={1}
                    title="Producer sends records"
                    description="Your application batches records and sends them to an agent via gRPC."
                    icon={Server}
                    color="teal"
                  />
                  <FlowStep
                    number={2}
                    title="Write-Ahead Log (WAL)"
                    description="Records are immediately written to local disk with CRC32 checksums. This is the durability checkpoint."
                    icon={FileText}
                    color="amber"
                  />
                  <FlowStep
                    number={3}
                    title="In-memory buffer"
                    description="Records accumulate in memory with delta encoding and varint compression."
                    icon={Cpu}
                    color="blue"
                  />
                  <FlowStep
                    number={4}
                    title="Segment finalization"
                    description="When buffer reaches ~64-256MB, it's compressed with LZ4 and indexed for fast seeks."
                    icon={Archive}
                    color="teal"
                  />
                  <FlowStep
                    number={5}
                    title="S3 upload"
                    description="Immutable segment file uploaded to S3. Metadata registered in PostgreSQL."
                    icon={Cloud}
                    color="teal"
                  />
                  <FlowStep
                    number={6}
                    title="WAL truncated"
                    description="After S3 confirms upload, WAL is safely cleared for that segment."
                    icon={Check}
                    color="teal"
                  />
                </div>
              </div>

              {/* Read Path */}
              <div className="rounded-2xl border border-slate-200 bg-white p-8">
                <div className="mb-8 flex items-center gap-3">
                  <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-gradient-to-br from-blue-500 to-indigo-500">
                    <ArrowUpRight className="h-5 w-5 text-white" />
                  </div>
                  <div>
                    <h3 className="text-lg font-semibold text-slate-900">Read Path</h3>
                    <p className="text-sm text-slate-500">S3 → Consumer</p>
                  </div>
                </div>

                <div className="space-y-1">
                  <FlowStep
                    number={1}
                    title="Consumer requests offset"
                    description="Consumer asks for records starting from a specific offset via gRPC."
                    icon={Search}
                    color="blue"
                  />
                  <FlowStep
                    number={2}
                    title="Segment index lookup"
                    description="O(log n) BTreeMap lookup finds which segment contains the offset. Sub-microsecond."
                    icon={GitBranch}
                    color="blue"
                  />
                  <FlowStep
                    number={3}
                    title="Check local cache"
                    description="LRU disk cache checked first. Cache hit = &lt;1ms latency."
                    icon={HardDrive}
                    color="amber"
                  />
                  <FlowStep
                    number={4}
                    title="S3 download (if miss)"
                    description="On cache miss, segment downloaded from S3 (50-200ms) and cached locally."
                    icon={Cloud}
                    color="blue"
                  />
                  <FlowStep
                    number={5}
                    title="Decompress & decode"
                    description="LZ4 decompression at 3.1M records/sec. Delta decoding restores original values."
                    icon={Cpu}
                    color="blue"
                  />
                  <FlowStep
                    number={6}
                    title="Return records"
                    description="Ordered records streamed back to consumer. Offset committed to PostgreSQL."
                    icon={Check}
                    color="blue"
                  />
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* WAL Section */}
        <section className="py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="grid items-center gap-12 lg:grid-cols-2">
              <div>
                <span className="text-sm font-semibold uppercase tracking-wider text-amber-600">
                  Durability Layer
                </span>
                <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                  Write-Ahead Log (WAL)
                </h2>
                <p className="mt-4 text-slate-600">
                  Before any record is acknowledged, it&apos;s written to a local Write-Ahead Log.
                  This ensures zero data loss even if an agent crashes before uploading to S3.
                </p>

                <div className="mt-8 space-y-4">
                  <div className="flex items-start gap-4">
                    <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg bg-amber-100">
                      <Shield className="h-5 w-5 text-amber-600" />
                    </div>
                    <div>
                      <h4 className="font-semibold text-slate-900">CRC32 Checksums</h4>
                      <p className="mt-1 text-sm text-slate-600">
                        Every record includes a checksum. Corrupted records are detected and skipped during recovery.
                      </p>
                    </div>
                  </div>

                  <div className="flex items-start gap-4">
                    <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg bg-amber-100">
                      <Clock className="h-5 w-5 text-amber-600" />
                    </div>
                    <div>
                      <h4 className="font-semibold text-slate-900">Configurable Fsync</h4>
                      <p className="mt-1 text-sm text-slate-600">
                        Choose between Always (safest), Interval (100ms default), or Never (testing only).
                      </p>
                    </div>
                  </div>

                  <div className="flex items-start gap-4">
                    <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg bg-amber-100">
                      <RefreshCw className="h-5 w-5 text-amber-600" />
                    </div>
                    <div>
                      <h4 className="font-semibold text-slate-900">Automatic Recovery</h4>
                      <p className="mt-1 text-sm text-slate-600">
                        On agent restart, WAL is replayed to recover any unflushed records. Zero manual intervention.
                      </p>
                    </div>
                  </div>
                </div>
              </div>

              {/* WAL Format Visualization */}
              <div className="rounded-2xl border border-slate-200 bg-slate-900 p-6 shadow-2xl">
                <div className="mb-4 flex items-center gap-2">
                  <div className="h-3 w-3 rounded-full bg-red-500" />
                  <div className="h-3 w-3 rounded-full bg-yellow-500" />
                  <div className="h-3 w-3 rounded-full bg-green-500" />
                  <span className="ml-3 text-xs text-slate-500">WAL Record Format</span>
                </div>

                <div className="font-mono text-sm">
                  <div className="mb-4 text-slate-400">// Each record in the WAL file</div>

                  <div className="space-y-2">
                    <div className="flex items-center gap-2">
                      <div className="w-24 rounded bg-teal-900/50 px-2 py-1 text-center text-teal-400">
                        4 bytes
                      </div>
                      <span className="text-slate-300">Record Size</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="w-24 rounded bg-amber-900/50 px-2 py-1 text-center text-amber-400">
                        4 bytes
                      </div>
                      <span className="text-slate-300">CRC32 Checksum</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="w-24 rounded bg-blue-900/50 px-2 py-1 text-center text-blue-400">
                        8 bytes
                      </div>
                      <span className="text-slate-300">Timestamp (ms)</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="w-24 rounded bg-purple-900/50 px-2 py-1 text-center text-purple-400">
                        4 bytes
                      </div>
                      <span className="text-slate-300">Key Size</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="w-24 rounded bg-purple-900/50 px-2 py-1 text-center text-purple-400">
                        N bytes
                      </div>
                      <span className="text-slate-300">Key Data</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="w-24 rounded bg-rose-900/50 px-2 py-1 text-center text-rose-400">
                        4 bytes
                      </div>
                      <span className="text-slate-300">Value Size</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="w-24 rounded bg-rose-900/50 px-2 py-1 text-center text-rose-400">
                        M bytes
                      </div>
                      <span className="text-slate-300">Value Data</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Performance Section */}
        <section className="border-y border-slate-200 bg-white py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="text-center">
              <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                Performance
              </span>
              <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                Why it&apos;s still fast
              </h2>
              <p className="mx-auto mt-4 max-w-2xl text-slate-600">
                Despite storing data in S3, StreamHouse achieves high performance through
                smart optimizations at every layer.
              </p>
            </div>

            <div className="mt-16 grid gap-8 lg:grid-cols-2">
              {/* Delta Encoding */}
              <div className="rounded-2xl border border-slate-200 bg-slate-50 p-8">
                <div className="flex items-center gap-3">
                  <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-teal-100">
                    <Activity className="h-6 w-6 text-teal-600" />
                  </div>
                  <div>
                    <h3 className="text-lg font-semibold text-slate-900">Delta Encoding + Varints</h3>
                    <p className="text-sm text-slate-500">5-7 bytes saved per record</p>
                  </div>
                </div>

                <p className="mt-4 text-slate-600">
                  Instead of storing absolute values, we store differences. Sequential offsets
                  become tiny deltas that compress to 1 byte.
                </p>

                <div className="mt-6 rounded-xl bg-white p-4 shadow-sm">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="border-b border-slate-200">
                        <th className="py-2 text-left font-medium text-slate-600">Field</th>
                        <th className="py-2 text-right font-medium text-slate-600">Traditional</th>
                        <th className="py-2 text-right font-medium text-teal-600">StreamHouse</th>
                      </tr>
                    </thead>
                    <tbody className="text-slate-700">
                      <tr className="border-b border-slate-100">
                        <td className="py-2">Offset 1000</td>
                        <td className="py-2 text-right">8 bytes</td>
                        <td className="py-2 text-right text-teal-600">2 bytes</td>
                      </tr>
                      <tr className="border-b border-slate-100">
                        <td className="py-2">Offset 1001</td>
                        <td className="py-2 text-right">8 bytes</td>
                        <td className="py-2 text-right text-teal-600">1 byte (δ=1)</td>
                      </tr>
                      <tr className="border-b border-slate-100">
                        <td className="py-2">Timestamp</td>
                        <td className="py-2 text-right">8 bytes</td>
                        <td className="py-2 text-right text-teal-600">1-3 bytes</td>
                      </tr>
                      <tr>
                        <td className="py-2 font-medium">Total saved</td>
                        <td className="py-2 text-right">—</td>
                        <td className="py-2 text-right font-medium text-teal-600">~70%</td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </div>

              {/* LZ4 Compression */}
              <div className="rounded-2xl border border-slate-200 bg-slate-50 p-8">
                <div className="flex items-center gap-3">
                  <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-blue-100">
                    <Archive className="h-6 w-6 text-blue-600" />
                  </div>
                  <div>
                    <h3 className="text-lg font-semibold text-slate-900">LZ4 Block Compression</h3>
                    <p className="text-sm text-slate-500">80%+ space savings</p>
                  </div>
                </div>

                <p className="mt-4 text-slate-600">
                  Each ~1MB block is compressed independently. Fast enough to not be a bottleneck,
                  effective enough to dramatically reduce storage and transfer costs.
                </p>

                <div className="mt-6 grid grid-cols-2 gap-4">
                  <div className="rounded-xl bg-white p-4 text-center shadow-sm">
                    <div className="text-3xl font-bold text-teal-600">2.26M</div>
                    <div className="mt-1 text-sm text-slate-500">records/sec write</div>
                  </div>
                  <div className="rounded-xl bg-white p-4 text-center shadow-sm">
                    <div className="text-3xl font-bold text-blue-600">3.10M</div>
                    <div className="mt-1 text-sm text-slate-500">records/sec read</div>
                  </div>
                  <div className="rounded-xl bg-white p-4 text-center shadow-sm">
                    <div className="text-3xl font-bold text-amber-600">80%+</div>
                    <div className="mt-1 text-sm text-slate-500">compression ratio</div>
                  </div>
                  <div className="rounded-xl bg-white p-4 text-center shadow-sm">
                    <div className="text-3xl font-bold text-purple-600">640µs</div>
                    <div className="mt-1 text-sm text-slate-500">seek to offset</div>
                  </div>
                </div>
              </div>

              {/* Segment Index */}
              <div className="rounded-2xl border border-slate-200 bg-slate-50 p-8">
                <div className="flex items-center gap-3">
                  <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-purple-100">
                    <Search className="h-6 w-6 text-purple-600" />
                  </div>
                  <div>
                    <h3 className="text-lg font-semibold text-slate-900">Segment Index</h3>
                    <p className="text-sm text-slate-500">O(log n) offset lookup</p>
                  </div>
                </div>

                <p className="mt-4 text-slate-600">
                  Binary search over block offsets without decompressing. Find any offset
                  in microseconds, even in multi-GB segments.
                </p>

                <div className="mt-6 rounded-xl bg-white p-4 font-mono text-xs shadow-sm">
                  <div className="text-slate-500">// Segment index structure</div>
                  <div className="mt-2 space-y-1">
                    <div className="flex justify-between">
                      <span className="text-slate-600">Block 0:</span>
                      <span className="text-teal-600">offset=0, pos=64</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-slate-600">Block 1:</span>
                      <span className="text-teal-600">offset=10000, pos=1048640</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-slate-600">Block 2:</span>
                      <span className="text-teal-600">offset=20000, pos=2097216</span>
                    </div>
                  </div>
                  <div className="mt-3 border-t border-slate-200 pt-3">
                    <div className="text-slate-500">// Find offset 15000:</div>
                    <div className="text-blue-600">binary_search → Block 1</div>
                    <div className="text-blue-600">seek(1048640) → decompress</div>
                  </div>
                </div>
              </div>

              {/* LRU Cache */}
              <div className="rounded-2xl border border-slate-200 bg-slate-50 p-8">
                <div className="flex items-center gap-3">
                  <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-amber-100">
                    <HardDrive className="h-6 w-6 text-amber-600" />
                  </div>
                  <div>
                    <h3 className="text-lg font-semibold text-slate-900">LRU Segment Cache</h3>
                    <p className="text-sm text-slate-500">80%+ cache hit rate</p>
                  </div>
                </div>

                <p className="mt-4 text-slate-600">
                  Frequently accessed segments are cached on local disk. Sequential reads
                  achieve excellent cache hit rates, avoiding S3 latency.
                </p>

                <div className="mt-6 space-y-3">
                  <div className="flex items-center justify-between rounded-lg bg-white p-3 shadow-sm">
                    <span className="text-sm text-slate-600">Cache hit latency</span>
                    <span className="font-mono font-medium text-teal-600">&lt;1ms</span>
                  </div>
                  <div className="flex items-center justify-between rounded-lg bg-white p-3 shadow-sm">
                    <span className="text-sm text-slate-600">Cache miss latency</span>
                    <span className="font-mono font-medium text-amber-600">50-200ms</span>
                  </div>
                  <div className="flex items-center justify-between rounded-lg bg-white p-3 shadow-sm">
                    <span className="text-sm text-slate-600">Sequential read hit rate</span>
                    <span className="font-mono font-medium text-teal-600">&gt;80%</span>
                  </div>
                  <div className="flex items-center justify-between rounded-lg bg-white p-3 shadow-sm">
                    <span className="text-sm text-slate-600">Automatic prefetching</span>
                    <span className="font-medium text-teal-600">✓ Enabled</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Durability Guarantees */}
        <section className="py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="text-center">
              <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                Reliability
              </span>
              <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                How you won&apos;t lose data
              </h2>
              <p className="mx-auto mt-4 max-w-2xl text-slate-600">
                Multiple layers of protection ensure your data is always safe,
                from agent crashes to S3 outages.
              </p>
            </div>

            <div className="mt-16 grid gap-6 lg:grid-cols-3">
              {/* Layer 1: WAL */}
              <div className="rounded-2xl border border-amber-200 bg-gradient-to-br from-amber-50 to-orange-50 p-6">
                <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-amber-100">
                  <span className="text-lg font-bold text-amber-600">1</span>
                </div>
                <h3 className="mt-4 text-lg font-semibold text-slate-900">Write-Ahead Log</h3>
                <p className="mt-2 text-sm text-slate-600">
                  Every record hits local disk before acknowledgment. CRC32 checksums detect corruption.
                  On crash, WAL replays automatically.
                </p>
                <div className="mt-4 rounded-lg bg-white/80 p-3">
                  <div className="text-xs font-medium text-amber-700">Recovery scenario:</div>
                  <div className="mt-1 text-xs text-slate-600">
                    Agent crashes → Restart → WAL replay → Zero data loss
                  </div>
                </div>
              </div>

              {/* Layer 2: S3 */}
              <div className="rounded-2xl border border-cyan-200 bg-gradient-to-br from-cyan-50 to-teal-50 p-6">
                <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-cyan-100">
                  <span className="text-lg font-bold text-cyan-600">2</span>
                </div>
                <h3 className="mt-4 text-lg font-semibold text-slate-900">S3 Durability</h3>
                <p className="mt-2 text-sm text-slate-600">
                  Once segments upload to S3, you get 11 nines (99.999999999%) durability.
                  Immutable files, no partial writes, optional multi-region.
                </p>
                <div className="mt-4 rounded-lg bg-white/80 p-3">
                  <div className="text-xs font-medium text-cyan-700">What this means:</div>
                  <div className="mt-1 text-xs text-slate-600">
                    Lose 1 object per 10 million every 10,000 years
                  </div>
                </div>
              </div>

              {/* Layer 3: PostgreSQL */}
              <div className="rounded-2xl border border-blue-200 bg-gradient-to-br from-blue-50 to-indigo-50 p-6">
                <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-blue-100">
                  <span className="text-lg font-bold text-blue-600">3</span>
                </div>
                <h3 className="mt-4 text-lg font-semibold text-slate-900">PostgreSQL Metadata</h3>
                <p className="mt-2 text-sm text-slate-600">
                  All metadata durably stored: segment locations, consumer offsets, partition leases.
                  Standard PostgreSQL replication for HA.
                </p>
                <div className="mt-4 rounded-lg bg-white/80 p-3">
                  <div className="text-xs font-medium text-blue-700">Tracked data:</div>
                  <div className="mt-1 text-xs text-slate-600">
                    Offsets, watermarks, leases, segment registry
                  </div>
                </div>
              </div>
            </div>

            {/* Failure Scenarios */}
            <div className="mt-16">
              <h3 className="text-center text-lg font-semibold text-slate-900">
                Failure Recovery Scenarios
              </h3>

              <div className="mt-8 grid gap-6 lg:grid-cols-3">
                <div className="rounded-xl border border-slate-200 bg-white p-6">
                  <div className="flex items-center gap-2">
                    <div className="flex h-8 w-8 items-center justify-center rounded-full bg-red-100">
                      <Server className="h-4 w-4 text-red-600" />
                    </div>
                    <h4 className="font-medium text-slate-900">Agent Crash Mid-Write</h4>
                  </div>
                  <div className="mt-4 space-y-2 text-sm">
                    <div className="flex items-center gap-2">
                      <div className="h-2 w-2 rounded-full bg-amber-400" />
                      <span className="text-slate-600">Records in WAL</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <ArrowDown className="h-3 w-3 text-slate-400" />
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="h-2 w-2 rounded-full bg-blue-400" />
                      <span className="text-slate-600">Agent restarts</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <ArrowDown className="h-3 w-3 text-slate-400" />
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="h-2 w-2 rounded-full bg-teal-400" />
                      <span className="text-slate-600">WAL replayed</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <ArrowDown className="h-3 w-3 text-slate-400" />
                    </div>
                    <div className="flex items-center gap-2">
                      <Check className="h-4 w-4 text-teal-500" />
                      <span className="font-medium text-teal-600">Zero data loss</span>
                    </div>
                  </div>
                </div>

                <div className="rounded-xl border border-slate-200 bg-white p-6">
                  <div className="flex items-center gap-2">
                    <div className="flex h-8 w-8 items-center justify-center rounded-full bg-amber-100">
                      <RefreshCw className="h-4 w-4 text-amber-600" />
                    </div>
                    <h4 className="font-medium text-slate-900">Partition Rebalancing</h4>
                  </div>
                  <div className="mt-4 space-y-2 text-sm">
                    <div className="flex items-center gap-2">
                      <div className="h-2 w-2 rounded-full bg-red-400" />
                      <span className="text-slate-600">Agent 1 dies</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <ArrowDown className="h-3 w-3 text-slate-400" />
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="h-2 w-2 rounded-full bg-amber-400" />
                      <span className="text-slate-600">Lease expires (30s)</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <ArrowDown className="h-3 w-3 text-slate-400" />
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="h-2 w-2 rounded-full bg-blue-400" />
                      <span className="text-slate-600">Agent 2 acquires lease</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <ArrowDown className="h-3 w-3 text-slate-400" />
                    </div>
                    <div className="flex items-center gap-2">
                      <Check className="h-4 w-4 text-teal-500" />
                      <span className="font-medium text-teal-600">Seamless failover</span>
                    </div>
                  </div>
                </div>

                <div className="rounded-xl border border-slate-200 bg-white p-6">
                  <div className="flex items-center gap-2">
                    <div className="flex h-8 w-8 items-center justify-center rounded-full bg-cyan-100">
                      <Cloud className="h-4 w-4 text-cyan-600" />
                    </div>
                    <h4 className="font-medium text-slate-900">S3 Temporarily Down</h4>
                  </div>
                  <div className="mt-4 space-y-2 text-sm">
                    <div className="flex items-center gap-2">
                      <div className="h-2 w-2 rounded-full bg-red-400" />
                      <span className="text-slate-600">S3 unavailable</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <ArrowDown className="h-3 w-3 text-slate-400" />
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="h-2 w-2 rounded-full bg-amber-400" />
                      <span className="text-slate-600">Circuit breaker activates</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <ArrowDown className="h-3 w-3 text-slate-400" />
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="h-2 w-2 rounded-full bg-blue-400" />
                      <span className="text-slate-600">Records buffer in WAL</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <ArrowDown className="h-3 w-3 text-slate-400" />
                    </div>
                    <div className="flex items-center gap-2">
                      <Check className="h-4 w-4 text-teal-500" />
                      <span className="font-medium text-teal-600">S3 recovers, WAL drains</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Lease-Based Coordination */}
        <section className="border-y border-slate-200 bg-gradient-to-b from-slate-50 to-white py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="grid items-center gap-12 lg:grid-cols-2">
              <div>
                <span className="text-sm font-semibold uppercase tracking-wider text-purple-600">
                  Coordination
                </span>
                <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                  Lease-based leadership
                </h2>
                <p className="mt-4 text-slate-600">
                  Instead of complex consensus protocols like Raft or ZooKeeper, StreamHouse uses
                  simple time-based leases with epoch fencing to prevent split-brain scenarios.
                </p>

                <div className="mt-8 space-y-4">
                  <div className="flex items-start gap-4">
                    <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg bg-purple-100">
                      <Lock className="h-5 w-5 text-purple-600" />
                    </div>
                    <div>
                      <h4 className="font-semibold text-slate-900">30-Second Leases</h4>
                      <p className="mt-1 text-sm text-slate-600">
                        Each partition has exactly one leader. Leases renewed every 10 seconds.
                      </p>
                    </div>
                  </div>

                  <div className="flex items-start gap-4">
                    <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg bg-purple-100">
                      <GitBranch className="h-5 w-5 text-purple-600" />
                    </div>
                    <div>
                      <h4 className="font-semibold text-slate-900">Epoch Fencing</h4>
                      <p className="mt-1 text-sm text-slate-600">
                        Every write includes epoch number. Stale epochs are rejected, preventing duplicate writes.
                      </p>
                    </div>
                  </div>

                  <div className="flex items-start gap-4">
                    <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg bg-purple-100">
                      <RefreshCw className="h-5 w-5 text-purple-600" />
                    </div>
                    <div>
                      <h4 className="font-semibold text-slate-900">Automatic Failover</h4>
                      <p className="mt-1 text-sm text-slate-600">
                        If leader fails, lease expires and any healthy agent can take over within 30 seconds.
                      </p>
                    </div>
                  </div>
                </div>
              </div>

              {/* Lease Visualization */}
              <div className="rounded-2xl border border-slate-200 bg-white p-6 shadow-lg">
                <h4 className="mb-4 font-semibold text-slate-900">Partition Lease Table</h4>

                <div className="overflow-hidden rounded-lg border border-slate-200">
                  <table className="w-full text-sm">
                    <thead className="bg-slate-50">
                      <tr>
                        <th className="px-4 py-2 text-left font-medium text-slate-600">Partition</th>
                        <th className="px-4 py-2 text-left font-medium text-slate-600">Agent</th>
                        <th className="px-4 py-2 text-left font-medium text-slate-600">Epoch</th>
                        <th className="px-4 py-2 text-left font-medium text-slate-600">Status</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-100">
                      <tr>
                        <td className="px-4 py-3 font-mono text-slate-700">orders:0</td>
                        <td className="px-4 py-3 text-slate-600">agent-1</td>
                        <td className="px-4 py-3 font-mono text-purple-600">42</td>
                        <td className="px-4 py-3">
                          <span className="inline-flex items-center gap-1 rounded-full bg-teal-100 px-2 py-0.5 text-xs font-medium text-teal-700">
                            <span className="h-1.5 w-1.5 rounded-full bg-teal-500" />
                            Active
                          </span>
                        </td>
                      </tr>
                      <tr>
                        <td className="px-4 py-3 font-mono text-slate-700">orders:1</td>
                        <td className="px-4 py-3 text-slate-600">agent-2</td>
                        <td className="px-4 py-3 font-mono text-purple-600">38</td>
                        <td className="px-4 py-3">
                          <span className="inline-flex items-center gap-1 rounded-full bg-teal-100 px-2 py-0.5 text-xs font-medium text-teal-700">
                            <span className="h-1.5 w-1.5 rounded-full bg-teal-500" />
                            Active
                          </span>
                        </td>
                      </tr>
                      <tr>
                        <td className="px-4 py-3 font-mono text-slate-700">orders:2</td>
                        <td className="px-4 py-3 text-slate-600">agent-1</td>
                        <td className="px-4 py-3 font-mono text-purple-600">55</td>
                        <td className="px-4 py-3">
                          <span className="inline-flex items-center gap-1 rounded-full bg-teal-100 px-2 py-0.5 text-xs font-medium text-teal-700">
                            <span className="h-1.5 w-1.5 rounded-full bg-teal-500" />
                            Active
                          </span>
                        </td>
                      </tr>
                    </tbody>
                  </table>
                </div>

                <div className="mt-4 rounded-lg bg-purple-50 p-4">
                  <div className="text-xs font-medium text-purple-700">How epoch fencing works:</div>
                  <div className="mt-2 space-y-1 font-mono text-xs text-slate-600">
                    <div>Agent-1 (epoch=5) tries to write → <span className="text-red-600">REJECTED</span></div>
                    <div>Agent-2 (epoch=6) is current leader</div>
                    <div>Agent-2 (epoch=6) writes → <span className="text-teal-600">ACCEPTED</span></div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Storage Format */}
        <section className="py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="text-center">
              <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                Storage Format
              </span>
              <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                Inside a segment file
              </h2>
              <p className="mx-auto mt-4 max-w-2xl text-slate-600">
                Segment files are the immutable storage unit. Each contains thousands of records,
                compressed and indexed for fast random access.
              </p>
            </div>

            <div className="mt-16">
              <div className="rounded-2xl border border-slate-200 bg-slate-900 p-8 shadow-2xl">
                <div className="mb-6 flex items-center gap-2">
                  <div className="h-3 w-3 rounded-full bg-red-500" />
                  <div className="h-3 w-3 rounded-full bg-yellow-500" />
                  <div className="h-3 w-3 rounded-full bg-green-500" />
                  <span className="ml-3 text-xs text-slate-500">segment_00000000000000000000.seg</span>
                </div>

                <div className="space-y-4 font-mono text-sm">
                  {/* Header */}
                  <div className="rounded-lg bg-teal-900/30 p-4">
                    <div className="mb-2 text-xs font-semibold uppercase tracking-wider text-teal-400">
                      Header (64 bytes)
                    </div>
                    <div className="grid grid-cols-2 gap-4 text-slate-300 lg:grid-cols-4">
                      <div>
                        <span className="text-slate-500">Magic:</span> <span className="text-teal-400">&quot;STRM&quot;</span>
                      </div>
                      <div>
                        <span className="text-slate-500">Version:</span> <span className="text-teal-400">1</span>
                      </div>
                      <div>
                        <span className="text-slate-500">Compression:</span> <span className="text-teal-400">LZ4</span>
                      </div>
                      <div>
                        <span className="text-slate-500">Records:</span> <span className="text-teal-400">100,000</span>
                      </div>
                    </div>
                  </div>

                  {/* Blocks */}
                  <div className="space-y-2">
                    {[0, 1, 2].map((i) => (
                      <div key={i} className="rounded-lg bg-blue-900/30 p-4">
                        <div className="flex items-center justify-between">
                          <div>
                            <span className="text-xs font-semibold uppercase tracking-wider text-blue-400">
                              Block {i}
                            </span>
                            <span className="ml-3 text-xs text-slate-500">
                              (~1MB compressed)
                            </span>
                          </div>
                          <div className="text-xs text-slate-400">
                            offsets {i * 33333}-{(i + 1) * 33333 - 1}
                          </div>
                        </div>
                        <div className="mt-2 h-8 rounded bg-blue-800/50" />
                      </div>
                    ))}
                  </div>

                  {/* Index */}
                  <div className="rounded-lg bg-purple-900/30 p-4">
                    <div className="mb-2 text-xs font-semibold uppercase tracking-wider text-purple-400">
                      Index
                    </div>
                    <div className="grid gap-2 text-xs text-slate-300 lg:grid-cols-3">
                      <div className="rounded bg-purple-800/30 p-2">
                        Block 0 → offset=0, pos=64
                      </div>
                      <div className="rounded bg-purple-800/30 p-2">
                        Block 1 → offset=33333, pos=1048640
                      </div>
                      <div className="rounded bg-purple-800/30 p-2">
                        Block 2 → offset=66666, pos=2097216
                      </div>
                    </div>
                  </div>

                  {/* Footer */}
                  <div className="rounded-lg bg-amber-900/30 p-4">
                    <div className="mb-2 text-xs font-semibold uppercase tracking-wider text-amber-400">
                      Footer (32 bytes)
                    </div>
                    <div className="flex gap-6 text-slate-300">
                      <div>
                        <span className="text-slate-500">Index pos:</span> <span className="text-amber-400">3145728</span>
                      </div>
                      <div>
                        <span className="text-slate-500">CRC32:</span> <span className="text-amber-400">0xA1B2C3D4</span>
                      </div>
                      <div>
                        <span className="text-slate-500">Magic:</span> <span className="text-amber-400">&quot;STRM&quot;</span>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>

            {/* S3 Structure */}
            <div className="mt-12 grid gap-8 lg:grid-cols-2">
              <div className="rounded-2xl border border-slate-200 bg-white p-6">
                <h3 className="flex items-center gap-2 text-lg font-semibold text-slate-900">
                  <Cloud className="h-5 w-5 text-cyan-600" />
                  S3 Object Hierarchy
                </h3>
                <div className="mt-4 rounded-lg bg-slate-50 p-4 font-mono text-sm">
                  <div className="text-slate-600">s3://streamhouse-bucket/</div>
                  <div className="ml-4 text-slate-600">└── org-{`{org_id}`}/</div>
                  <div className="ml-8 text-slate-600">└── data/</div>
                  <div className="ml-12 text-teal-600">├── orders/</div>
                  <div className="ml-16 text-slate-500">├── 0/</div>
                  <div className="ml-20 text-blue-600">├── 00000000000000000000.seg</div>
                  <div className="ml-20 text-blue-600">├── 00000000000000100000.seg</div>
                  <div className="ml-20 text-slate-400">└── ...</div>
                  <div className="ml-16 text-slate-500">├── 1/</div>
                  <div className="ml-16 text-slate-400">└── ...</div>
                  <div className="ml-12 text-teal-600">└── users/</div>
                </div>
              </div>

              <div className="rounded-2xl border border-slate-200 bg-white p-6">
                <h3 className="flex items-center gap-2 text-lg font-semibold text-slate-900">
                  <Database className="h-5 w-5 text-blue-600" />
                  PostgreSQL Schema
                </h3>
                <div className="mt-4 space-y-3 text-sm">
                  <div className="rounded-lg bg-slate-50 p-3">
                    <div className="font-medium text-slate-700">segments</div>
                    <div className="mt-1 text-xs text-slate-500">
                      topic, partition, base_offset, end_offset, s3_key, size_bytes
                    </div>
                  </div>
                  <div className="rounded-lg bg-slate-50 p-3">
                    <div className="font-medium text-slate-700">consumer_offsets</div>
                    <div className="mt-1 text-xs text-slate-500">
                      group_id, topic, partition, committed_offset, committed_at
                    </div>
                  </div>
                  <div className="rounded-lg bg-slate-50 p-3">
                    <div className="font-medium text-slate-700">partition_leases</div>
                    <div className="mt-1 text-xs text-slate-500">
                      topic, partition, agent_id, lease_epoch, expires_at
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Comparison Table */}
        <section className="border-y border-slate-200 bg-white py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="text-center">
              <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                Comparison
              </span>
              <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                StreamHouse vs. Kafka + Flink
              </h2>
            </div>

            <div className="mt-12 overflow-hidden rounded-2xl border border-slate-200">
              <table className="w-full">
                <thead className="bg-slate-50">
                  <tr>
                    <th className="px-6 py-4 text-left text-sm font-semibold text-slate-900">
                      Aspect
                    </th>
                    <th className="px-6 py-4 text-left text-sm font-semibold text-slate-500">
                      Kafka + Flink
                    </th>
                    <th className="px-6 py-4 text-left text-sm font-semibold text-teal-600">
                      StreamHouse
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-100">
                  {[
                    {
                      aspect: "Storage",
                      kafka: "Local SSDs per broker",
                      streamhouse: "S3 (11 nines durability)",
                    },
                    {
                      aspect: "Compute model",
                      kafka: "Stateful brokers + tasks",
                      streamhouse: "Stateless agents",
                    },
                    {
                      aspect: "Coordination",
                      kafka: "ZooKeeper + Flink JobManager",
                      streamhouse: "PostgreSQL + leases",
                    },
                    {
                      aspect: "Scaling",
                      kafka: "Add brokers, rebalance",
                      streamhouse: "Spin up more agents instantly",
                    },
                    {
                      aspect: "Failover time",
                      kafka: "Minutes (leader election)",
                      streamhouse: "30 seconds (lease expiration)",
                    },
                    {
                      aspect: "Recovery",
                      kafka: "Log replication + checkpoints",
                      streamhouse: "WAL replay + S3",
                    },
                    {
                      aspect: "SQL processing",
                      kafka: "Separate Flink cluster",
                      streamhouse: "Built-in",
                    },
                    {
                      aspect: "Systems to operate",
                      kafka: "2+ (Kafka, Flink, ZK)",
                      streamhouse: "1",
                    },
                    {
                      aspect: "Storage cost",
                      kafka: "3x for replication",
                      streamhouse: "1x (S3 handles durability)",
                    },
                  ].map((row, i) => (
                    <tr key={row.aspect} className={i % 2 === 1 ? "bg-slate-50/50" : ""}>
                      <td className="px-6 py-4 text-sm font-medium text-slate-900">
                        {row.aspect}
                      </td>
                      <td className="px-6 py-4 text-sm text-slate-500">{row.kafka}</td>
                      <td className="px-6 py-4 text-sm font-medium text-teal-600">
                        {row.streamhouse}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </section>

        {/* CTA */}
        <section className="py-24">
          <div className="mx-auto max-w-4xl px-6 text-center lg:px-8">
            <h2 className="font-serif text-4xl italic text-slate-900 sm:text-5xl">
              Ready to simplify
              <br />
              your streaming?
            </h2>
            <p className="mx-auto mt-4 max-w-2xl text-lg text-slate-600">
              Get started in minutes. No complex configuration, no 50-page runbooks.
            </p>
            <div className="mt-10 flex flex-col items-center justify-center gap-4 sm:flex-row">
              <Link href="/console">
                <Button
                  size="lg"
                  className="h-12 bg-slate-900 px-8 text-base font-medium text-white hover:bg-slate-800"
                >
                  Get started free
                  <ArrowRight className="ml-2 h-4 w-4" />
                </Button>
              </Link>
              <Link href="/docs">
                <Button
                  size="lg"
                  variant="outline"
                  className="h-12 border-slate-300 px-8 text-base font-medium text-slate-700 hover:bg-slate-50"
                >
                  Read the docs
                </Button>
              </Link>
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
