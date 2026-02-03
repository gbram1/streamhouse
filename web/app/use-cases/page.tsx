import Link from "next/link";
import {
  ArrowRight,
  BarChart3,
  Bell,
  Check,
  Cloud,
  Code2,
  CreditCard,
  Database,
  FileText,
  Globe,
  LineChart,
  MessageSquare,
  MonitorDot,
  RefreshCw,
  Server,
  Shield,
  Truck,
  Users,
  Zap,
} from "lucide-react";
import { Button } from "@/components/ui/button";

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
    color: "orange",
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
    color: "pink",
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

export default function UseCasesPage() {
  return (
    <div className="relative">
      {/* Hero */}
      <section className="relative overflow-hidden">
        <div className="absolute inset-0 bg-gradient-to-b from-teal-600/5 via-transparent to-transparent" />
        <div className="absolute left-1/2 top-0 h-[400px] w-[600px] -translate-x-1/2 bg-gradient-to-b from-teal-500/10 via-cyan-500/5 to-transparent blur-3xl" />

        <div className="relative mx-auto max-w-7xl px-6 py-16 lg:px-8 lg:py-24">
          <div className="mx-auto max-w-2xl text-center">
            <h1 className="text-4xl font-bold tracking-tight text-white sm:text-5xl">
              Use Cases
            </h1>
            <p className="mt-4 text-lg text-[#8b90a3]">
              See how teams use StreamHouse to power real-time applications,
              reduce costs, and eliminate operational complexity.
            </p>
          </div>

          {/* Quick nav */}
          <div className="mx-auto mt-10 flex max-w-4xl flex-wrap justify-center gap-3">
            {useCases.map((uc) => (
              <a
                key={uc.id}
                href={`#${uc.id}`}
                className={`flex items-center gap-2 rounded-full border border-white/5 bg-[#0e0e18] px-4 py-2 text-sm transition-all hover:border-${uc.color}-500/30 hover:bg-${uc.color}-500/10`}
              >
                <uc.icon className={`h-4 w-4 text-${uc.color}-400`} />
                <span className="text-[#a0a5b8]">{uc.title}</span>
              </a>
            ))}
          </div>
        </div>
      </section>

      {/* Use Cases */}
      <section className="py-16">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="space-y-24">
            {useCases.map((useCase, index) => (
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
                      <div
                        className={`flex h-12 w-12 items-center justify-center rounded-xl bg-${useCase.color}-500/10`}
                      >
                        <useCase.icon
                          className={`h-6 w-6 text-${useCase.color}-400`}
                        />
                      </div>
                      <div>
                        <h2 className="text-2xl font-bold text-white">
                          {useCase.title}
                        </h2>
                        <p className={`text-sm text-${useCase.color}-400`}>
                          {useCase.tagline}
                        </p>
                      </div>
                    </div>

                    <p className="mt-6 text-lg text-[#8b90a3]">
                      {useCase.description}
                    </p>

                    <ul className="mt-6 space-y-3">
                      {useCase.benefits.map((benefit) => (
                        <li
                          key={benefit}
                          className="flex items-start gap-3 text-[#c8cde4]"
                        >
                          <Check
                            className={`mt-0.5 h-5 w-5 flex-shrink-0 text-${useCase.color}-400`}
                          />
                          {benefit}
                        </li>
                      ))}
                    </ul>

                    <div className="mt-6 flex flex-wrap gap-2">
                      {useCase.industries.map((industry) => (
                        <span
                          key={industry}
                          className="rounded-full bg-white/5 px-3 py-1 text-xs text-[#6b7086]"
                        >
                          {industry}
                        </span>
                      ))}
                    </div>

                    <div className="mt-8">
                      <Link
                        href="/docs"
                        className={`inline-flex items-center gap-2 text-sm font-medium text-${useCase.color}-400 transition-colors hover:text-${useCase.color}-300`}
                      >
                        Learn how to implement
                        <ArrowRight className="h-4 w-4" />
                      </Link>
                    </div>
                  </div>

                  {/* Example Card */}
                  <div className="flex-1">
                    <div
                      className={`rounded-2xl border border-${useCase.color}-500/20 bg-gradient-to-br from-${useCase.color}-500/10 via-transparent to-transparent p-8`}
                    >
                      <div className="flex items-center gap-2 text-sm font-medium text-[#6b7086]">
                        <Globe className="h-4 w-4" />
                        Example
                      </div>
                      <h3 className="mt-4 text-xl font-semibold text-white">
                        {useCase.example.title}
                      </h3>
                      <p className="mt-3 text-[#8b90a3]">
                        {useCase.example.description}
                      </p>

                      {/* Visual representation */}
                      <div className="mt-6 rounded-xl border border-white/5 bg-[#0a0a12]/50 p-4">
                        <div className="flex items-center justify-between gap-4">
                          <div className="flex flex-col items-center">
                            <div className="flex h-10 w-16 items-center justify-center rounded-lg border border-white/10 bg-white/5 text-xs text-[#a0a5b8]">
                              Source
                            </div>
                          </div>
                          <div className="flex-1 border-t border-dashed border-white/10" />
                          <div className="flex flex-col items-center">
                            <div
                              className={`flex h-10 w-16 items-center justify-center rounded-lg border border-${useCase.color}-500/30 bg-${useCase.color}-500/10 text-xs text-${useCase.color}-400`}
                            >
                              Process
                            </div>
                          </div>
                          <div className="flex-1 border-t border-dashed border-white/10" />
                          <div className="flex flex-col items-center">
                            <div className="flex h-10 w-16 items-center justify-center rounded-lg border border-white/10 bg-white/5 text-xs text-[#a0a5b8]">
                              Output
                            </div>
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Testimonials */}
      <section className="border-y border-white/5 bg-[#080810] py-24">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="mx-auto max-w-2xl text-center">
            <h2 className="text-3xl font-bold tracking-tight text-white">
              Trusted by engineering teams
            </h2>
            <p className="mt-4 text-[#8b90a3]">
              See what teams are saying about StreamHouse.
            </p>
          </div>

          <div className="mx-auto mt-12 grid max-w-5xl gap-8 md:grid-cols-3">
            {testimonials.map((testimonial) => (
              <div
                key={testimonial.author}
                className="rounded-xl border border-white/5 bg-[#0e0e18] p-6"
              >
                <p className="text-[#c8cde4]">&ldquo;{testimonial.quote}&rdquo;</p>
                <div className="mt-6 flex items-center gap-3">
                  <div className="flex h-10 w-10 items-center justify-center rounded-full bg-gradient-to-br from-blue-500/50 to-cyan-400/50 text-sm font-medium text-white">
                    {testimonial.author
                      .split(" ")
                      .map((n) => n[0])
                      .join("")}
                  </div>
                  <div>
                    <p className="text-sm font-medium text-white">
                      {testimonial.author}
                    </p>
                    <p className="text-xs text-[#6b7086]">
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
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="relative overflow-hidden rounded-3xl bg-gradient-to-br from-teal-600/20 via-cyan-600/10 to-blue-600/20 px-8 py-16 sm:px-16 lg:py-24">
            <div className="absolute inset-0 bg-[#0a0a12]/80" />
            <div className="absolute left-1/4 top-0 h-64 w-64 bg-teal-500/20 blur-3xl" />
            <div className="absolute bottom-0 right-1/4 h-64 w-64 bg-blue-500/20 blur-3xl" />

            <div className="relative mx-auto max-w-2xl text-center">
              <Zap className="mx-auto h-10 w-10 text-teal-400" />
              <h2 className="mt-6 text-3xl font-bold tracking-tight text-white sm:text-4xl">
                Have a different use case?
              </h2>
              <p className="mx-auto mt-4 max-w-xl text-lg text-[#8b90a3]">
                StreamHouse is flexible enough for any streaming workload. Talk
                to us about your specific requirements.
              </p>
              <div className="mt-10 flex flex-col items-center justify-center gap-4 sm:flex-row">
                <Link href="/console">
                  <Button
                    size="lg"
                    className="h-12 bg-white px-8 text-base font-medium text-[#0a0a12] hover:bg-white/90"
                  >
                    Start building free
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
    </div>
  );
}
