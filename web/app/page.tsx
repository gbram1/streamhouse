"use client";

import Link from "next/link";
import { Button } from "@/components/ui/button";
import {
  ArrowRight,
  ArrowUpRight,
  Cloud,
  Code2,
  Database,
  Layers,
  Server,
  Zap,
  Check,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";

function GithubIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
    </svg>
  );
}

// Integration icons
function AwsIcon() {
  return (
    <svg className="h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
      <path d="M6.763 10.036c0 .296.032.535.088.71.064.176.144.368.256.576.04.063.056.127.056.183 0 .08-.048.16-.152.24l-.503.335a.383.383 0 0 1-.208.072c-.08 0-.16-.04-.239-.112a2.47 2.47 0 0 1-.287-.375 6.18 6.18 0 0 1-.248-.471c-.622.734-1.405 1.101-2.347 1.101-.67 0-1.205-.191-1.596-.574-.391-.384-.59-.894-.59-1.533 0-.678.239-1.23.726-1.644.487-.415 1.133-.623 1.955-.623.272 0 .551.024.846.064.296.04.6.104.918.176v-.583c0-.607-.127-1.03-.375-1.277-.255-.248-.686-.367-1.3-.367-.28 0-.568.031-.863.103-.296.072-.583.16-.862.272a2.1 2.1 0 0 1-.255.104.488.488 0 0 1-.128.024c-.112 0-.168-.08-.168-.247v-.391c0-.128.016-.224.056-.28a.597.597 0 0 1 .224-.167c.279-.144.614-.264 1.005-.36a4.84 4.84 0 0 1 1.246-.151c.95 0 1.644.216 2.091.647.439.43.662 1.085.662 1.963v2.586zm-3.24 1.214c.263 0 .534-.048.822-.144.287-.096.543-.271.758-.51.128-.152.224-.32.272-.512.047-.191.08-.423.08-.694v-.335a6.66 6.66 0 0 0-.735-.136 6.02 6.02 0 0 0-.75-.048c-.535 0-.926.104-1.19.32-.263.215-.39.518-.39.917 0 .375.095.655.295.846.191.2.47.296.838.296zm6.41.862c-.144 0-.24-.024-.304-.08-.064-.048-.12-.16-.168-.311L7.586 5.55a1.398 1.398 0 0 1-.072-.32c0-.128.064-.2.191-.2h.783c.151 0 .255.025.31.08.065.048.113.16.16.312l1.342 5.284 1.245-5.284c.04-.16.088-.264.151-.312a.549.549 0 0 1 .32-.08h.638c.152 0 .256.025.32.08.063.048.12.16.151.312l1.261 5.348 1.381-5.348c.048-.16.104-.264.16-.312a.52.52 0 0 1 .311-.08h.743c.127 0 .2.065.2.2 0 .04-.009.08-.017.128a1.137 1.137 0 0 1-.056.2l-1.923 6.17c-.048.16-.104.263-.168.311a.51.51 0 0 1-.303.08h-.687c-.151 0-.255-.024-.32-.08-.063-.056-.119-.16-.15-.32l-1.238-5.148-1.23 5.14c-.04.16-.087.264-.15.32-.065.056-.177.08-.32.08zm10.256.215c-.415 0-.83-.048-1.229-.143-.399-.096-.71-.2-.918-.32-.128-.071-.215-.151-.247-.223a.563.563 0 0 1-.048-.224v-.407c0-.167.064-.247.183-.247.048 0 .096.008.144.024.048.016.12.048.2.08.271.12.566.215.878.279.319.064.63.096.95.096.502 0 .894-.088 1.165-.264a.86.86 0 0 0 .415-.758.777.777 0 0 0-.215-.559c-.144-.151-.415-.287-.806-.407l-1.157-.36c-.583-.183-1.014-.454-1.277-.813a1.902 1.902 0 0 1-.4-1.158c0-.335.073-.63.216-.886.144-.255.335-.479.575-.654.24-.184.51-.32.83-.415.32-.096.655-.136 1.006-.136.175 0 .359.008.535.032.183.024.35.056.518.088.16.04.312.08.455.127.144.048.256.096.336.144a.69.69 0 0 1 .24.2.43.43 0 0 1 .071.263v.375c0 .168-.064.256-.184.256a.83.83 0 0 1-.303-.096 3.652 3.652 0 0 0-1.532-.311c-.455 0-.815.071-1.062.223-.248.152-.375.383-.375.71 0 .224.08.416.24.567.159.152.454.304.877.44l1.134.358c.574.184.99.44 1.237.767.247.327.367.702.367 1.117 0 .343-.072.655-.207.926-.144.272-.336.511-.583.703-.248.2-.543.343-.886.447-.36.111-.734.167-1.142.167z" />
    </svg>
  );
}

function PostgresIcon() {
  return (
    <svg className="h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
      <path d="M17.128 0a10.134 10.134 0 0 0-2.755.403l-.063.02a10.922 10.922 0 0 0-1.612-.144c-.91-.004-1.752.168-2.483.485a8.136 8.136 0 0 0-2.148-.533C4.132.067 1.146 2.46.066 6.427c-.738 2.71-.073 6.463 1.376 10.324 1.453 3.874 3.488 7.134 5.49 7.134.393 0 .787-.125 1.146-.39l.12-.093c.326.284.738.47 1.226.535l.136.015c-.124.262-.19.537-.21.814a2.28 2.28 0 0 0 .06.69c-.053.066-.099.138-.139.214a.976.976 0 0 0-.073.325c-.013.213.053.435.203.623.29.36.748.478 1.293.338.554-.142 1.158-.505 1.677-1.045.456-.474.84-1.049 1.12-1.663l.036.003a3.685 3.685 0 0 0 .54-.027c1.633-.17 2.722-.924 3.237-2.134a4.77 4.77 0 0 0 .347-1.713c.009-.233.002-.47-.018-.707a14.22 14.22 0 0 0 1.035-.676c1.104-.813 1.922-1.725 2.431-2.708a6.47 6.47 0 0 0 .567-1.506c.28-1.094.193-2.197-.238-3.139-.222-.485-.524-.918-.9-1.29a4.762 4.762 0 0 0-.036-.044 7.098 7.098 0 0 0-.503-3.23c-.32-.856-.79-1.636-1.39-2.318a7.549 7.549 0 0 0-1.655-1.38A7.384 7.384 0 0 0 17.128 0z" />
    </svg>
  );
}

function KafkaIcon() {
  return (
    <svg className="h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0C5.373 0 0 5.373 0 12s5.373 12 12 12 12-5.373 12-12S18.627 0 12 0zm0 2.182a9.818 9.818 0 1 1 0 19.636 9.818 9.818 0 0 1 0-19.636zm0 3.273a2.182 2.182 0 1 0 0 4.363 2.182 2.182 0 0 0 0-4.363zm-4.364 6.545a2.182 2.182 0 1 0 0 4.364 2.182 2.182 0 0 0 0-4.364zm8.728 0a2.182 2.182 0 1 0 0 4.364 2.182 2.182 0 0 0 0-4.364z" />
    </svg>
  );
}

function ClickhouseIcon() {
  return (
    <svg className="h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
      <path d="M21.333 10H24v4h-2.667zM16 1.333h2.667v21.334H16zM10.667 4h2.666v16h-2.666zM5.333 6.667H8v10.666H5.333zM0 9.333h2.667v5.334H0z" />
    </svg>
  );
}

function FlinkIcon() {
  return (
    <svg className="h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0L2 6v12l10 6 10-6V6L12 0zm0 2.18l7.5 4.5v9l-7.5 4.5-7.5-4.5v-9l7.5-4.5z" />
    </svg>
  );
}

function SparkIcon() {
  return (
    <svg className="h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0L1 6v12l11 6 11-6V6L12 0zm0 3.6l7.2 4v8.8l-7.2 4-7.2-4V7.6l7.2-4z" />
    </svg>
  );
}

export default function Home() {
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
                  className="text-sm text-slate-600 transition-colors hover:text-slate-900"
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
          {/* Warm gradient orbs */}
          <div className="pointer-events-none absolute inset-0">
            <div className="absolute left-1/4 top-32 h-[500px] w-[500px] rounded-full bg-gradient-to-br from-amber-200/60 via-orange-200/40 to-transparent blur-3xl" />
            <div className="absolute right-1/4 top-48 h-[400px] w-[400px] rounded-full bg-gradient-to-bl from-rose-200/50 via-pink-100/30 to-transparent blur-3xl" />
            <div className="absolute bottom-0 left-1/2 h-[300px] w-[600px] -translate-x-1/2 rounded-full bg-gradient-to-t from-teal-100/40 to-transparent blur-3xl" />
          </div>

          <div className="relative mx-auto max-w-7xl px-6 pb-12 pt-24 text-center lg:px-8 lg:pt-32">
            {/* Badge */}
            <Link
              href="/blog/phase-7-release"
              className="group mb-8 inline-flex items-center gap-2 rounded-full border border-slate-200 bg-white/80 px-4 py-1.5 text-sm text-slate-600 shadow-sm backdrop-blur transition-all hover:shadow-md"
            >
              <span className="flex h-2 w-2 animate-pulse rounded-full bg-teal-500" />
              Phase 7 Released — Exactly-once semantics
              <ArrowRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-0.5" />
            </Link>

            {/* Headline - Serif style */}
            <h1 className="mx-auto max-w-4xl text-5xl font-light tracking-tight text-slate-900 sm:text-6xl lg:text-7xl">
              <span className="font-serif italic">Event streaming</span>
              <br />
              <span className="font-serif italic">for the </span>
              <span className="bg-gradient-to-r from-teal-600 to-cyan-600 bg-clip-text font-serif italic text-transparent">
                cloud era
              </span>
            </h1>

            {/* Subheadline */}
            <p className="mx-auto mt-6 max-w-2xl text-lg text-slate-600">
              Legacy streaming infrastructure is too heavy, slow, and complicated.
              StreamHouse is S3-native, serverless, and ready.
            </p>

            {/* CTAs */}
            <div className="mt-10 flex flex-col items-center justify-center gap-4 sm:flex-row">
              <Link href="/console">
                <Button
                  size="lg"
                  className="group h-12 bg-slate-900 px-8 text-base font-medium text-white hover:bg-slate-800"
                >
                  Start building free
                  <ArrowRight className="ml-2 h-4 w-4 transition-transform group-hover:translate-x-0.5" />
                </Button>
              </Link>
              <Link href="/docs">
                <Button
                  size="lg"
                  variant="outline"
                  className="h-12 border-slate-300 bg-white/80 px-8 text-base font-medium text-slate-700 backdrop-blur hover:bg-white"
                >
                  Documentation
                </Button>
              </Link>
            </div>
          </div>

          {/* Architecture Diagram */}
          <div className="relative mx-auto max-w-6xl px-6 pb-32 lg:px-8">
            <div className="relative">
              {/* Center node - StreamHouse */}
              <div className="mx-auto flex w-fit flex-col items-center">
                <div className="relative z-10 rounded-2xl border border-slate-200 bg-white p-6 shadow-xl">
                  <div className="flex h-16 w-16 items-center justify-center rounded-xl bg-gradient-to-br from-teal-500 to-cyan-500">
                    <Zap className="h-8 w-8 text-white" />
                  </div>
                  <p className="mt-3 text-center font-semibold text-slate-900">
                    StreamHouse
                  </p>
                </div>
              </div>

              {/* Connection lines and nodes */}
              <div className="absolute inset-0 flex items-center justify-center">
                {/* Left side - Sources */}
                <div className="absolute left-0 top-1/2 flex -translate-y-1/2 flex-col gap-6 lg:left-12">
                  <p className="mb-2 text-xs font-semibold uppercase tracking-wider text-slate-400">
                    Sources
                  </p>
                  {[
                    { icon: KafkaIcon, name: "Kafka", color: "text-slate-600" },
                    { icon: PostgresIcon, name: "Postgres", color: "text-blue-600" },
                    { icon: AwsIcon, name: "AWS", color: "text-orange-500" },
                  ].map((source, i) => (
                    <div
                      key={source.name}
                      className="flex items-center gap-3"
                      style={{
                        opacity: mounted ? 1 : 0,
                        transform: mounted ? "translateX(0)" : "translateX(-20px)",
                        transition: `all 0.5s ease ${i * 0.1}s`,
                      }}
                    >
                      <div className="flex h-10 w-10 items-center justify-center rounded-lg border border-slate-200 bg-white shadow-sm">
                        <source.icon />
                      </div>
                      <span className="text-sm text-slate-600">{source.name}</span>
                      <div className="h-px w-16 bg-gradient-to-r from-slate-300 to-transparent lg:w-24" />
                    </div>
                  ))}
                </div>

                {/* Right side - Destinations */}
                <div className="absolute right-0 top-1/2 flex -translate-y-1/2 flex-col gap-6 lg:right-12">
                  <p className="mb-2 text-right text-xs font-semibold uppercase tracking-wider text-slate-400">
                    Destinations
                  </p>
                  {[
                    { icon: ClickhouseIcon, name: "ClickHouse", color: "text-yellow-500" },
                    { icon: SparkIcon, name: "Spark", color: "text-orange-500" },
                    { icon: FlinkIcon, name: "Flink", color: "text-pink-500" },
                  ].map((dest, i) => (
                    <div
                      key={dest.name}
                      className="flex items-center gap-3"
                      style={{
                        opacity: mounted ? 1 : 0,
                        transform: mounted ? "translateX(0)" : "translateX(20px)",
                        transition: `all 0.5s ease ${i * 0.1}s`,
                      }}
                    >
                      <div className="h-px w-16 bg-gradient-to-l from-slate-300 to-transparent lg:w-24" />
                      <span className="text-sm text-slate-600">{dest.name}</span>
                      <div className="flex h-10 w-10 items-center justify-center rounded-lg border border-slate-200 bg-white shadow-sm">
                        <dest.icon />
                      </div>
                    </div>
                  ))}
                </div>
              </div>

              {/* Deployment options */}
              <div className="mt-20 flex flex-col items-center">
                <p className="mb-4 text-xs font-semibold uppercase tracking-wider text-slate-400">
                  Deployment Options
                </p>
                <div className="flex gap-3">
                  {["Self Hosted", "BYOC", "Cloud"].map((option) => (
                    <div
                      key={option}
                      className="rounded-full border border-slate-200 bg-white px-4 py-1.5 text-sm text-slate-600 shadow-sm"
                    >
                      {option}
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Why Section */}
        <section className="border-y border-slate-200 bg-white py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="grid gap-16 lg:grid-cols-2">
              {/* Problem */}
              <div>
                <span className="text-sm font-semibold uppercase tracking-wider text-red-500">
                  The Problem
                </span>
                <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                  Kafka wasn&apos;t built for the cloud
                </h2>
                <p className="mt-4 text-slate-600">
                  Traditional streaming requires expensive disks, complex replication,
                  and constant operational overhead.
                </p>
                <div className="mt-8 space-y-3">
                  {[
                    "3x storage cost for replication",
                    "ZooKeeper/KRaft dependencies",
                    "Manual capacity planning",
                    "Expensive cross-AZ traffic",
                  ].map((item) => (
                    <div
                      key={item}
                      className="flex items-center gap-3 text-slate-600"
                    >
                      <X className="h-4 w-4 flex-shrink-0 text-red-400" />
                      {item}
                    </div>
                  ))}
                </div>
              </div>

              {/* Solution */}
              <div className="rounded-3xl bg-gradient-to-br from-teal-50 to-cyan-50 p-8 lg:p-10">
                <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                  The Solution
                </span>
                <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                  S3-native from day one
                </h2>
                <p className="mt-4 text-slate-600">
                  StreamHouse uses object storage as the source of truth. Stateless
                  agents. Zero replication. Infinite scale.
                </p>
                <div className="mt-8 space-y-3">
                  {[
                    "11 nines durability via S3",
                    "80% lower infrastructure costs",
                    "Instant, automatic scaling",
                    "No disks to manage",
                  ].map((item) => (
                    <div
                      key={item}
                      className="flex items-center gap-3 text-slate-700"
                    >
                      <Check className="h-4 w-4 flex-shrink-0 text-teal-500" />
                      {item}
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Architecture Section */}
        <section className="py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="text-center">
              <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                Architecture
              </span>
              <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                Disaggregated by design
              </h2>
              <p className="mx-auto mt-4 max-w-2xl text-slate-600">
                Compute and storage scale independently. Agents are stateless.
                S3 provides durability.
              </p>
            </div>

            <div className="mt-16 grid gap-8 lg:grid-cols-3">
              {[
                {
                  icon: Layers,
                  title: "Stateless Agents",
                  description:
                    "Ephemeral compute that can be killed or restarted without data loss. No persistent state on the broker.",
                },
                {
                  icon: Cloud,
                  title: "S3 Storage",
                  description:
                    "Events written directly to object storage with LZ4 compression. 11 nines durability, infinite capacity.",
                },
                {
                  icon: Database,
                  title: "Metadata Store",
                  description:
                    "Lightweight coordination via PostgreSQL. Only offsets, leases, and watermarks—no event data.",
                },
              ].map((item) => (
                <div
                  key={item.title}
                  className="rounded-2xl border border-slate-200 bg-white p-8"
                >
                  <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-slate-100">
                    <item.icon className="h-6 w-6 text-slate-600" />
                  </div>
                  <h3 className="mt-5 text-lg font-semibold text-slate-900">
                    {item.title}
                  </h3>
                  <p className="mt-2 text-slate-600">{item.description}</p>
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* Features Grid */}
        <section className="border-y border-slate-200 bg-gradient-to-b from-slate-50 to-white py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="text-center">
              <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                Features
              </span>
              <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                Everything you need
              </h2>
            </div>

            <div className="mt-16 grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
              {[
                {
                  icon: Cloud,
                  title: "S3-Native Storage",
                  description: "Direct writes to object storage with LZ4 compression.",
                },
                {
                  icon: Layers,
                  title: "Zero Replication",
                  description: "S3 handles durability. No inter-broker traffic costs.",
                },
                {
                  icon: Code2,
                  title: "SQL Processing",
                  description: "Built-in stream processing. No separate Flink cluster.",
                },
                {
                  icon: Server,
                  title: "Stateless Agents",
                  description: "Kill, restart, or scale freely. All state in S3.",
                },
                {
                  icon: Database,
                  title: "Smart Caching",
                  description: "LRU cache reduces S3 reads by 80-95%.",
                },
                {
                  icon: Zap,
                  title: "Full Observability",
                  description: "Prometheus metrics and Grafana dashboards.",
                },
              ].map((feature) => (
                <div
                  key={feature.title}
                  className="group rounded-2xl border border-slate-200 bg-white p-6 transition-all hover:border-teal-200 hover:shadow-lg"
                >
                  <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-slate-100 transition-colors group-hover:bg-teal-100">
                    <feature.icon className="h-5 w-5 text-slate-600 transition-colors group-hover:text-teal-600" />
                  </div>
                  <h3 className="mt-4 font-semibold text-slate-900">
                    {feature.title}
                  </h3>
                  <p className="mt-2 text-sm text-slate-600">
                    {feature.description}
                  </p>
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* Code Section */}
        <section className="py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="grid items-center gap-12 lg:grid-cols-2">
              <div>
                <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                  Developer Experience
                </span>
                <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                  Up and running in minutes
                </h2>
                <p className="mt-4 text-slate-600">
                  Start streaming with a few commands. No complex configuration,
                  no 50-page runbooks.
                </p>
                <div className="mt-8 flex flex-wrap gap-3">
                  <Link href="/docs">
                    <Button className="bg-slate-900 text-white hover:bg-slate-800">
                      Quick start guide
                      <ArrowUpRight className="ml-2 h-4 w-4" />
                    </Button>
                  </Link>
                  <Link href="https://github.com/streamhouse/streamhouse">
                    <Button variant="outline" className="border-slate-300">
                      <GithubIcon className="mr-2 h-4 w-4" />
                      View on GitHub
                    </Button>
                  </Link>
                </div>
              </div>

              <div className="overflow-hidden rounded-2xl border border-slate-200 bg-slate-900 shadow-2xl">
                <div className="flex items-center gap-2 border-b border-slate-700 bg-slate-800 px-4 py-3">
                  <div className="h-3 w-3 rounded-full bg-red-500" />
                  <div className="h-3 w-3 rounded-full bg-yellow-500" />
                  <div className="h-3 w-3 rounded-full bg-green-500" />
                  <span className="ml-3 text-xs text-slate-500">terminal</span>
                </div>
                <div className="p-6 font-mono text-sm">
                  <div className="space-y-3 text-slate-300">
                    <div className="text-slate-500"># Start infrastructure</div>
                    <div>
                      <span className="text-teal-400">$</span>{" "}
                      <span className="text-white">docker compose up -d</span>
                    </div>
                    <div className="pl-4 text-green-400">✓ Services ready</div>
                    <div className="pt-2 text-slate-500"># Launch agent</div>
                    <div>
                      <span className="text-teal-400">$</span>{" "}
                      <span className="text-white">cargo run --bin agent</span>
                    </div>
                    <div className="pl-4 text-green-400">✓ Listening on :9090</div>
                    <div className="pt-2 text-slate-500"># Produce events</div>
                    <div>
                      <span className="text-teal-400">$</span>{" "}
                      <span className="text-white">
                        streamctl produce events -v &apos;{`{"id":1}`}&apos;
                      </span>
                    </div>
                    <div className="pl-4 text-cyan-400">→ offset=0</div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Use Cases */}
        <section className="border-y border-slate-200 bg-white py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="text-center">
              <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                Use Cases
              </span>
              <h2 className="mt-3 font-serif text-3xl italic text-slate-900 sm:text-4xl">
                Built for modern data teams
              </h2>
            </div>

            <div className="mt-12 grid gap-6 md:grid-cols-2">
              {[
                {
                  title: "Real-time analytics",
                  description:
                    "Replace Kafka → Flink → ClickHouse with integrated SQL processing.",
                  tag: "Analytics",
                },
                {
                  title: "Log aggregation",
                  description:
                    "Store logs in S3 at $0.023/GB. Infinite retention.",
                  tag: "Observability",
                },
                {
                  title: "ML feature pipelines",
                  description:
                    "Compute real-time features with SQL. Direct S3 output.",
                  tag: "ML/AI",
                },
                {
                  title: "Change data capture",
                  description:
                    "Simplified CDC with built-in transformations.",
                  tag: "Data Sync",
                },
              ].map((useCase) => (
                <div
                  key={useCase.title}
                  className="group rounded-2xl border border-slate-200 bg-slate-50 p-8 transition-all hover:bg-white hover:shadow-lg"
                >
                  <span className="inline-block rounded-full bg-teal-100 px-3 py-1 text-xs font-semibold text-teal-700">
                    {useCase.tag}
                  </span>
                  <h3 className="mt-4 text-lg font-semibold text-slate-900">
                    {useCase.title}
                  </h3>
                  <p className="mt-2 text-slate-600">{useCase.description}</p>
                  <Link
                    href="/use-cases"
                    className="mt-4 inline-flex items-center text-sm font-medium text-teal-600 hover:text-teal-700"
                  >
                    Learn more
                    <ArrowRight className="ml-1 h-3.5 w-3.5" />
                  </Link>
                </div>
              ))}
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
              Join teams cutting infrastructure costs by 80% while eliminating
              operational complexity.
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
