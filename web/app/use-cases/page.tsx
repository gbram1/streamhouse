"use client";

import Link from "next/link";
import { Button } from "@/components/ui/button";
import {
  ArrowRight,
  BarChart3,
  Check,
  Code2,
  FileText,
  Globe,
  MonitorDot,
  RefreshCw,
  Server,
  Zap,
} from "lucide-react";

function GithubIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
    </svg>
  );
}

const useCases = [
  {
    id: "real-time-analytics",
    icon: BarChart3,
    title: "Real-Time Analytics",
    tagline: "From events to insights in milliseconds",
    description:
      "Replace complex Kafka → Flink → ClickHouse pipelines with integrated SQL stream processing. Compute aggregations, detect patterns, and power dashboards in real-time.",
    color: "blue",
    benefits: [
      "50% cost reduction vs. traditional pipelines",
      "Sub-second latency for aggregations",
      "SQL-based transformations",
      "Direct Grafana integration",
    ],
    industries: ["SaaS", "E-commerce", "FinTech"],
    example: {
      title: "Product Analytics",
      description:
        "Track user behavior, compute session metrics, and detect engagement patterns in real-time across millions of events.",
    },
  },
  {
    id: "log-aggregation",
    icon: FileText,
    title: "Log Aggregation",
    tagline: "Store logs at S3 prices, not logging prices",
    description:
      "Aggregate logs from across your infrastructure at a fraction of cloud logging costs. Store indefinitely in S3 with intelligent indexing for fast search.",
    color: "cyan",
    benefits: [
      "$0.023/GB vs. $0.50/GB cloud logging",
      "Infinite retention without cost explosion",
      "Structured log processing with SQL",
      "Integration with existing log shippers",
    ],
    industries: ["DevOps", "Security", "Compliance"],
    example: {
      title: "Centralized Logging",
      description:
        "Collect logs from Kubernetes pods, Lambda functions, and EC2 instances into a unified, searchable data lake.",
    },
  },
  {
    id: "ml-feature-pipelines",
    icon: Code2,
    title: "ML Feature Pipelines",
    tagline: "Real-time features without the infrastructure",
    description:
      "Compute ML features in real-time with SQL. Stream to feature stores or directly to S3 for training pipelines. No separate feature engineering infrastructure.",
    color: "purple",
    benefits: [
      "Real-time feature computation",
      "Direct S3 output for training",
      "Windowed aggregations for time-series features",
      "Feature versioning and backfill support",
    ],
    industries: ["ML/AI", "AdTech", "Fraud Detection"],
    example: {
      title: "Fraud Detection Features",
      description:
        "Compute transaction velocity, device fingerprints, and behavioral anomaly scores in real-time for ML models.",
    },
  },
  {
    id: "change-data-capture",
    icon: RefreshCw,
    title: "Change Data Capture",
    tagline: "Sync databases without the complexity",
    description:
      "Capture database changes and replicate to downstream systems. Built-in transformations mean fewer moving parts and easier debugging.",
    color: "teal",
    benefits: [
      "Database change streaming",
      "Schema evolution handling",
      "Transformation without Debezium + Flink",
      "Multi-destination replication",
    ],
    industries: ["E-commerce", "FinTech", "Healthcare"],
    example: {
      title: "Order Sync",
      description:
        "Stream order changes from PostgreSQL to Elasticsearch for search and to a data warehouse for analytics.",
    },
  },
  {
    id: "event-driven-microservices",
    icon: Server,
    title: "Event-Driven Microservices",
    tagline: "Decouple services with reliable messaging",
    description:
      "Build loosely coupled microservices with guaranteed message delivery. Event sourcing, CQRS, and saga patterns made simple.",
    color: "amber",
    benefits: [
      "At-least-once delivery guarantees",
      "Consumer group coordination",
      "Dead letter queue support",
      "Event replay for debugging",
    ],
    industries: ["SaaS", "Marketplaces", "Travel"],
    example: {
      title: "Order Processing",
      description:
        "Orchestrate inventory, payment, shipping, and notification services with reliable event-driven communication.",
    },
  },
  {
    id: "iot-telemetry",
    icon: MonitorDot,
    title: "IoT Telemetry",
    tagline: "Handle millions of device messages",
    description:
      "Ingest, process, and store telemetry data from IoT devices at scale. Real-time alerting and long-term storage in one platform.",
    color: "rose",
    benefits: [
      "High-throughput ingestion (50K+ msg/sec)",
      "Real-time anomaly detection",
      "Long-term storage at S3 prices",
      "Time-series aggregations with SQL",
    ],
    industries: ["Manufacturing", "Smart Cities", "Energy"],
    example: {
      title: "Fleet Monitoring",
      description:
        "Track vehicle location, engine diagnostics, and driver behavior for thousands of vehicles in real-time.",
    },
  },
];

const testimonials = [
  {
    quote:
      "We cut our streaming infrastructure costs by 75% and eliminated two full-time positions worth of Kafka maintenance.",
    author: "Sarah Chen",
    role: "VP Engineering",
    company: "DataFlow Inc",
  },
  {
    quote:
      "The SQL processing alone saved us from deploying a separate Flink cluster. Our pipeline went from 5 services to 1.",
    author: "Marcus Johnson",
    role: "Staff Engineer",
    company: "AnalyticsCo",
  },
  {
    quote:
      "Finally, a streaming platform that doesn't require a dedicated team to operate. It just works.",
    author: "Emily Zhang",
    role: "CTO",
    company: "StartupXYZ",
  },
];

const colorClasses: Record<string, { bg: string; text: string; border: string; light: string }> = {
  blue: { bg: "bg-blue-100", text: "text-blue-600", border: "border-blue-200", light: "bg-blue-50" },
  cyan: { bg: "bg-cyan-100", text: "text-cyan-600", border: "border-cyan-200", light: "bg-cyan-50" },
  purple: { bg: "bg-purple-100", text: "text-purple-600", border: "border-purple-200", light: "bg-purple-50" },
  teal: { bg: "bg-teal-100", text: "text-teal-600", border: "border-teal-200", light: "bg-teal-50" },
  amber: { bg: "bg-amber-100", text: "text-amber-600", border: "border-amber-200", light: "bg-amber-50" },
  rose: { bg: "bg-rose-100", text: "text-rose-600", border: "border-rose-200", light: "bg-rose-50" },
};

export default function UseCasesPage() {
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
                    item.name === "Use Cases" ? "text-teal-600 font-medium" : "text-slate-600"
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
            <div className="absolute right-1/4 top-48 h-[300px] w-[300px] rounded-full bg-gradient-to-bl from-amber-200/30 via-orange-100/20 to-transparent blur-3xl" />
          </div>

          <div className="relative mx-auto max-w-7xl px-6 py-16 lg:px-8 lg:py-24">
            <div className="mx-auto max-w-2xl text-center">
              <span className="inline-block rounded-full bg-teal-100 px-4 py-1.5 text-sm font-semibold text-teal-700">
                Built for Modern Teams
              </span>
              <h1 className="mt-6 font-serif text-4xl italic tracking-tight text-slate-900 sm:text-5xl">
                Use Cases
              </h1>
              <p className="mt-4 text-lg text-slate-600">
                See how teams use StreamHouse to power real-time applications,
                reduce costs, and eliminate operational complexity.
              </p>
            </div>

            {/* Quick nav */}
            <div className="mx-auto mt-10 flex max-w-4xl flex-wrap justify-center gap-3">
              {useCases.map((uc) => {
                const colors = colorClasses[uc.color] || colorClasses.teal;
                return (
                  <a
                    key={uc.id}
                    href={`#${uc.id}`}
                    className={`flex items-center gap-2 rounded-full border ${colors.border} ${colors.light} px-4 py-2 text-sm transition-all hover:shadow-md`}
                  >
                    <uc.icon className={`h-4 w-4 ${colors.text}`} />
                    <span className="text-slate-700">{uc.title}</span>
                  </a>
                );
              })}
            </div>
          </div>
        </section>

        {/* Use Cases */}
        <section className="py-16">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="space-y-24">
              {useCases.map((useCase, index) => {
                const colors = colorClasses[useCase.color] || colorClasses.teal;
                return (
                  <div
                    key={useCase.id}
                    id={useCase.id}
                    className="scroll-mt-24"
                  >
                    <div
                      className={`flex flex-col gap-12 ${
                        index % 2 === 1 ? "lg:flex-row-reverse" : "lg:flex-row"
                      }`}
                    >
                      {/* Content */}
                      <div className="flex-1">
                        <div className="flex items-center gap-3">
                          <div className={`flex h-12 w-12 items-center justify-center rounded-xl ${colors.bg}`}>
                            <useCase.icon className={`h-6 w-6 ${colors.text}`} />
                          </div>
                          <div>
                            <h2 className="text-2xl font-bold text-slate-900">
                              {useCase.title}
                            </h2>
                            <p className={`text-sm ${colors.text}`}>
                              {useCase.tagline}
                            </p>
                          </div>
                        </div>

                        <p className="mt-6 text-lg text-slate-600">
                          {useCase.description}
                        </p>

                        <ul className="mt-6 space-y-3">
                          {useCase.benefits.map((benefit) => (
                            <li
                              key={benefit}
                              className="flex items-start gap-3 text-slate-700"
                            >
                              <Check className={`mt-0.5 h-5 w-5 flex-shrink-0 ${colors.text}`} />
                              {benefit}
                            </li>
                          ))}
                        </ul>

                        <div className="mt-6 flex flex-wrap gap-2">
                          {useCase.industries.map((industry) => (
                            <span
                              key={industry}
                              className="rounded-full bg-slate-100 px-3 py-1 text-xs text-slate-600"
                            >
                              {industry}
                            </span>
                          ))}
                        </div>

                        <div className="mt-8">
                          <Link
                            href="/docs"
                            className={`inline-flex items-center gap-2 text-sm font-medium ${colors.text} transition-colors hover:opacity-80`}
                          >
                            Learn how to implement
                            <ArrowRight className="h-4 w-4" />
                          </Link>
                        </div>
                      </div>

                      {/* Example Card */}
                      <div className="flex-1">
                        <div className={`rounded-2xl border ${colors.border} bg-gradient-to-br from-white to-${useCase.color}-50/30 p-8`}>
                          <div className="flex items-center gap-2 text-sm font-medium text-slate-500">
                            <Globe className="h-4 w-4" />
                            Example
                          </div>
                          <h3 className="mt-4 text-xl font-semibold text-slate-900">
                            {useCase.example.title}
                          </h3>
                          <p className="mt-3 text-slate-600">
                            {useCase.example.description}
                          </p>

                          {/* Visual representation */}
                          <div className="mt-6 rounded-xl border border-slate-200 bg-white p-4">
                            <div className="flex items-center justify-between gap-4">
                              <div className="flex flex-col items-center">
                                <div className="flex h-10 w-16 items-center justify-center rounded-lg border border-slate-200 bg-slate-50 text-xs text-slate-600">
                                  Source
                                </div>
                              </div>
                              <div className="flex-1 border-t border-dashed border-slate-300" />
                              <div className="flex flex-col items-center">
                                <div className={`flex h-10 w-16 items-center justify-center rounded-lg border ${colors.border} ${colors.light} text-xs ${colors.text}`}>
                                  Process
                                </div>
                              </div>
                              <div className="flex-1 border-t border-dashed border-slate-300" />
                              <div className="flex flex-col items-center">
                                <div className="flex h-10 w-16 items-center justify-center rounded-lg border border-slate-200 bg-slate-50 text-xs text-slate-600">
                                  Output
                                </div>
                              </div>
                            </div>
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        </section>

        {/* Testimonials */}
        <section className="border-y border-slate-200 bg-white py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="mx-auto max-w-2xl text-center">
              <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                Testimonials
              </span>
              <h2 className="mt-3 font-serif text-3xl italic text-slate-900">
                Trusted by engineering teams
              </h2>
              <p className="mt-4 text-slate-600">
                See what teams are saying about StreamHouse.
              </p>
            </div>

            <div className="mx-auto mt-12 grid max-w-5xl gap-8 md:grid-cols-3">
              {testimonials.map((testimonial) => (
                <div
                  key={testimonial.author}
                  className="rounded-2xl border border-slate-200 bg-slate-50 p-6"
                >
                  <p className="text-slate-700">&ldquo;{testimonial.quote}&rdquo;</p>
                  <div className="mt-6 flex items-center gap-3">
                    <div className="flex h-10 w-10 items-center justify-center rounded-full bg-gradient-to-br from-teal-500 to-cyan-500 text-sm font-medium text-white">
                      {testimonial.author
                        .split(" ")
                        .map((n) => n[0])
                        .join("")}
                    </div>
                    <div>
                      <p className="text-sm font-medium text-slate-900">
                        {testimonial.author}
                      </p>
                      <p className="text-xs text-slate-500">
                        {testimonial.role}, {testimonial.company}
                      </p>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* CTA */}
        <section className="py-24">
          <div className="mx-auto max-w-4xl px-6 text-center lg:px-8">
            <div className="rounded-3xl border border-teal-200 bg-gradient-to-br from-teal-50 to-cyan-50 p-8 lg:p-12">
              <Zap className="mx-auto h-10 w-10 text-teal-600" />
              <h2 className="mt-4 font-serif text-2xl italic text-slate-900">
                Have a different use case?
              </h2>
              <p className="mx-auto mt-2 max-w-xl text-slate-600">
                StreamHouse is flexible enough for any streaming workload. Talk
                to us about your specific requirements.
              </p>
              <div className="mt-10 flex flex-col items-center justify-center gap-4 sm:flex-row">
                <Link href="/console">
                  <Button className="h-12 bg-slate-900 px-8 text-base font-medium text-white hover:bg-slate-800">
                    Start building free
                    <ArrowRight className="ml-2 h-4 w-4" />
                  </Button>
                </Link>
                <Link href="/docs">
                  <Button variant="outline" className="h-12 border-slate-300 px-8 text-base font-medium text-slate-700 hover:bg-slate-50">
                    Read documentation
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
