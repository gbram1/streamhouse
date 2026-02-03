import Link from "next/link";
import {
  ArrowRight,
  BarChart3,
  Check,
  Cloud,
  Code2,
  Database,
  Gauge,
  Globe,
  Layers,
  Lock,
  Server,
  Sparkles,
  Zap,
} from "lucide-react";
import { Button } from "@/components/ui/button";

const features = [
  {
    icon: Cloud,
    title: "S3-Native Storage",
    description:
      "Events are written directly to object storage with built-in compression. No local disks to manage, no replication to configure.",
    highlights: [
      "11 nines durability",
      "LZ4 compression",
      "Pay only for storage used",
    ],
  },
  {
    icon: Layers,
    title: "Stateless Agents",
    description:
      "Compute nodes that can be killed, restarted, or scaled without data loss. All state lives in S3 and the metadata store.",
    highlights: [
      "Instant scaling",
      "Zero data migration",
      "Lease-based coordination",
    ],
  },
  {
    icon: Code2,
    title: "SQL Stream Processing",
    description:
      "Transform your streaming data with familiar SQL syntax. No separate Flink or Spark cluster required.",
    highlights: [
      "Windowed aggregations",
      "Stream joins",
      "Materialized views",
    ],
  },
  {
    icon: Gauge,
    title: "Smart Caching",
    description:
      "LRU cache reduces S3 reads by 80-95%. Hot data served from memory, cold data fetched on demand.",
    highlights: [
      "Configurable cache size",
      "Automatic eviction",
      "Cache hit metrics",
    ],
  },
  {
    icon: BarChart3,
    title: "Full Observability",
    description:
      "Prometheus metrics, Grafana dashboards, and health endpoints out of the box. Know exactly what's happening.",
    highlights: [
      "Consumer lag monitoring",
      "Throughput metrics",
      "S3 operation tracking",
    ],
  },
  {
    icon: Lock,
    title: "Enterprise Security",
    description:
      "TLS encryption, authentication, and authorization. Secure your streams without complexity.",
    highlights: [
      "mTLS support",
      "ACL-based access",
      "Audit logging",
    ],
  },
];

const comparisons = [
  {
    metric: "Infrastructure Cost",
    streamhouse: "$500/mo",
    kafka: "$2,500/mo",
    savings: "80%",
  },
  {
    metric: "Operational Overhead",
    streamhouse: "Minimal",
    kafka: "Significant",
    savings: "90%",
  },
  {
    metric: "Scaling Time",
    streamhouse: "Seconds",
    kafka: "Hours",
    savings: "99%",
  },
  {
    metric: "Data Durability",
    streamhouse: "11 nines (S3)",
    kafka: "3x replication",
    savings: "Higher",
  },
];

const integrations = [
  { name: "Rust", icon: "🦀" },
  { name: "gRPC", icon: "📡" },
  { name: "Amazon S3", icon: "☁️" },
  { name: "PostgreSQL", icon: "🐘" },
  { name: "Prometheus", icon: "📊" },
  { name: "Grafana", icon: "📈" },
  { name: "Docker", icon: "🐳" },
  { name: "Kubernetes", icon: "⎈" },
];

export default function ProductPage() {
  return (
    <div className="relative">
      {/* Hero Section */}
      <section className="relative overflow-hidden">
        <div className="absolute inset-0 bg-gradient-to-b from-blue-600/5 via-transparent to-transparent" />
        <div className="absolute left-1/2 top-0 h-[500px] w-[700px] -translate-x-1/2 bg-gradient-to-b from-blue-500/10 via-cyan-500/5 to-transparent blur-3xl" />

        <div className="relative mx-auto max-w-7xl px-6 py-20 lg:px-8 lg:py-28">
          <div className="mx-auto max-w-3xl text-center">
            <div className="mb-6 inline-flex items-center gap-2 rounded-full border border-blue-500/20 bg-blue-500/5 px-4 py-1.5 text-sm text-blue-400">
              <Sparkles className="h-4 w-4" />
              <span>Reimagining Event Streaming</span>
            </div>

            <h1 className="text-4xl font-bold tracking-tight text-white sm:text-5xl lg:text-6xl">
              The streaming platform
              <span className="mt-2 block bg-gradient-to-r from-blue-400 via-cyan-400 to-teal-400 bg-clip-text text-transparent">
                built for the cloud
              </span>
            </h1>

            <p className="mx-auto mt-6 max-w-2xl text-lg text-[#8b90a3]">
              StreamHouse is an S3-native streaming platform that eliminates
              cluster management, cuts costs by 80%, and scales instantly. No
              disks. No replication. No complexity.
            </p>

            <div className="mt-10 flex flex-col items-center justify-center gap-4 sm:flex-row">
              <Link href="/console">
                <Button
                  size="lg"
                  className="h-12 bg-gradient-to-r from-blue-600 to-blue-500 px-8 text-base font-medium shadow-lg shadow-blue-500/25 transition-all hover:shadow-blue-500/40"
                >
                  Start building free
                  <ArrowRight className="ml-2 h-4 w-4" />
                </Button>
              </Link>
              <Link href="/docs">
                <Button
                  size="lg"
                  variant="outline"
                  className="h-12 border-white/10 bg-white/5 px-8 text-base font-medium text-white hover:bg-white/10"
                >
                  View documentation
                </Button>
              </Link>
            </div>
          </div>
        </div>
      </section>

      {/* Features Grid */}
      <section className="border-y border-white/5 bg-[#080810] py-24">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="mx-auto max-w-2xl text-center">
            <h2 className="text-3xl font-bold tracking-tight text-white sm:text-4xl">
              Everything you need to stream
            </h2>
            <p className="mt-4 text-lg text-[#8b90a3]">
              Production-ready features without the operational burden.
            </p>
          </div>

          <div className="mx-auto mt-16 grid max-w-5xl gap-8 md:grid-cols-2 lg:grid-cols-3">
            {features.map((feature) => (
              <div
                key={feature.title}
                className="group rounded-xl border border-white/5 bg-[#0e0e18] p-6 transition-all hover:border-white/10"
              >
                <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-gradient-to-br from-blue-500/20 to-cyan-500/10">
                  <feature.icon className="h-6 w-6 text-blue-400" />
                </div>
                <h3 className="mt-4 text-lg font-semibold text-white">
                  {feature.title}
                </h3>
                <p className="mt-2 text-sm text-[#6b7086]">
                  {feature.description}
                </p>
                <ul className="mt-4 space-y-2">
                  {feature.highlights.map((highlight) => (
                    <li
                      key={highlight}
                      className="flex items-center gap-2 text-sm text-[#a0a5b8]"
                    >
                      <Check className="h-4 w-4 flex-shrink-0 text-cyan-400" />
                      {highlight}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Comparison Section */}
      <section className="py-24">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="mx-auto max-w-2xl text-center">
            <h2 className="text-3xl font-bold tracking-tight text-white sm:text-4xl">
              StreamHouse vs. Traditional Kafka
            </h2>
            <p className="mt-4 text-lg text-[#8b90a3]">
              See how StreamHouse compares to traditional Kafka deployments.
            </p>
          </div>

          <div className="mx-auto mt-12 max-w-3xl overflow-hidden rounded-2xl border border-white/5">
            <div className="grid grid-cols-4 border-b border-white/5 bg-[#0e0e18] px-6 py-4 text-sm font-medium">
              <div className="text-[#6b7086]">Metric</div>
              <div className="text-center text-blue-400">StreamHouse</div>
              <div className="text-center text-[#6b7086]">Kafka</div>
              <div className="text-center text-cyan-400">Savings</div>
            </div>
            {comparisons.map((row, i) => (
              <div
                key={row.metric}
                className={`grid grid-cols-4 px-6 py-4 text-sm ${
                  i % 2 === 0 ? "bg-[#0a0a12]" : "bg-[#0e0e18]"
                }`}
              >
                <div className="text-white">{row.metric}</div>
                <div className="text-center text-[#c8cde4]">
                  {row.streamhouse}
                </div>
                <div className="text-center text-[#6b7086]">{row.kafka}</div>
                <div className="text-center font-medium text-cyan-400">
                  {row.savings}
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Architecture Section */}
      <section className="border-y border-white/5 bg-[#080810] py-24">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="mx-auto max-w-2xl text-center">
            <h2 className="text-3xl font-bold tracking-tight text-white sm:text-4xl">
              Disaggregated architecture
            </h2>
            <p className="mt-4 text-lg text-[#8b90a3]">
              Compute and storage scale independently. Agents are stateless. S3
              provides durability.
            </p>
          </div>

          <div className="mx-auto mt-12 max-w-4xl">
            <div className="rounded-2xl border border-white/5 bg-[#0a0a12] p-8">
              <div className="flex flex-col items-center gap-6 md:flex-row md:justify-between">
                <div className="flex flex-col items-center">
                  <div className="flex h-16 w-24 items-center justify-center rounded-xl border border-blue-500/30 bg-blue-500/10">
                    <span className="text-sm font-medium text-blue-400">
                      Producers
                    </span>
                  </div>
                  <span className="mt-2 text-xs text-[#6b7086]">gRPC</span>
                </div>

                <div className="h-px w-16 bg-gradient-to-r from-blue-500/50 to-cyan-500/50 md:h-px md:w-12" />

                <div className="flex flex-col items-center">
                  <div className="flex h-16 w-24 items-center justify-center rounded-xl border border-cyan-500/30 bg-cyan-500/10">
                    <span className="text-sm font-medium text-cyan-400">
                      Agents
                    </span>
                  </div>
                  <span className="mt-2 text-xs text-[#6b7086]">Stateless</span>
                </div>

                <div className="h-px w-16 bg-gradient-to-r from-cyan-500/50 to-teal-500/50 md:h-px md:w-12" />

                <div className="flex flex-col items-center">
                  <div className="flex h-16 w-24 items-center justify-center rounded-xl border border-teal-500/30 bg-teal-500/10">
                    <span className="text-sm font-medium text-teal-400">
                      S3
                    </span>
                  </div>
                  <span className="mt-2 text-xs text-[#6b7086]">Durable</span>
                </div>

                <div className="h-px w-16 bg-gradient-to-r from-teal-500/50 to-purple-500/50 md:h-px md:w-12" />

                <div className="flex flex-col items-center">
                  <div className="flex h-16 w-24 items-center justify-center rounded-xl border border-purple-500/30 bg-purple-500/10">
                    <span className="text-sm font-medium text-purple-400">
                      Consumers
                    </span>
                  </div>
                  <span className="mt-2 text-xs text-[#6b7086]">Cached</span>
                </div>
              </div>

              <div className="mt-8 flex justify-center">
                <div className="flex items-center gap-4 rounded-lg border border-white/5 bg-white/[0.02] px-6 py-3">
                  <Database className="h-5 w-5 text-[#6b7086]" />
                  <div>
                    <p className="text-sm font-medium text-white">
                      Metadata Store (PostgreSQL)
                    </p>
                    <p className="text-xs text-[#6b7086]">
                      Offsets, leases, watermarks only
                    </p>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      {/* Integrations */}
      <section className="py-24">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="mx-auto max-w-2xl text-center">
            <h2 className="text-3xl font-bold tracking-tight text-white sm:text-4xl">
              Built on proven technology
            </h2>
            <p className="mt-4 text-lg text-[#8b90a3]">
              StreamHouse integrates with your existing infrastructure.
            </p>
          </div>

          <div className="mx-auto mt-12 flex max-w-3xl flex-wrap items-center justify-center gap-6">
            {integrations.map((tech) => (
              <div
                key={tech.name}
                className="flex items-center gap-2 rounded-lg border border-white/5 bg-[#0e0e18] px-4 py-2.5"
              >
                <span className="text-lg">{tech.icon}</span>
                <span className="text-sm text-[#a0a5b8]">{tech.name}</span>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* CTA */}
      <section className="py-24">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="relative overflow-hidden rounded-3xl bg-gradient-to-br from-blue-600/20 via-cyan-600/10 to-teal-600/20 px-8 py-16 sm:px-16 lg:py-24">
            <div className="absolute inset-0 bg-[#0a0a12]/80" />
            <div className="absolute left-1/4 top-0 h-64 w-64 bg-blue-500/20 blur-3xl" />
            <div className="absolute bottom-0 right-1/4 h-64 w-64 bg-cyan-500/20 blur-3xl" />

            <div className="relative mx-auto max-w-2xl text-center">
              <Zap className="mx-auto h-10 w-10 text-blue-400" />
              <h2 className="mt-6 text-3xl font-bold tracking-tight text-white sm:text-4xl">
                Ready to simplify your streaming?
              </h2>
              <p className="mx-auto mt-4 max-w-xl text-lg text-[#8b90a3]">
                Join teams cutting infrastructure costs by 80% while eliminating
                operational complexity.
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
                <Link href="/use-cases">
                  <Button
                    size="lg"
                    variant="outline"
                    className="h-12 border-white/20 bg-transparent px-8 text-base font-medium text-white hover:bg-white/10"
                  >
                    Explore use cases
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
