import Link from "next/link";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Activity,
  ArrowRight,
  CheckCircle2,
  Cloud,
  Code2,
  Database,
  DollarSign,
  Gauge,
  GitBranch,
  HardDrive,
  Layers,
  Server,
  Zap,
} from "lucide-react";

export default function Home() {
  return (
    <div className="min-h-screen bg-gradient-to-b from-gray-950 via-gray-900 to-gray-950 text-white">
      {/* Header */}
      <header className="fixed top-0 left-0 right-0 z-50 border-b border-gray-800 bg-gray-950/80 backdrop-blur-md">
        <div className="container mx-auto px-4 py-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-2">
              <div className="relative">
                <Zap className="h-8 w-8 text-blue-500" />
                <div className="absolute inset-0 blur-lg bg-blue-500/30" />
              </div>
              <h1 className="text-2xl font-bold">StreamHouse</h1>
            </div>
            <nav className="hidden md:flex items-center space-x-6">
              <Link
                href="/docs"
                className="text-gray-400 hover:text-white transition-colors"
              >
                Docs
              </Link>
              <Link
                href="/pricing"
                className="text-gray-400 hover:text-white transition-colors"
              >
                Pricing
              </Link>
              <Link
                href="https://github.com/streamhouse/streamhouse"
                className="text-gray-400 hover:text-white transition-colors"
              >
                GitHub
              </Link>
              <Link href="/dashboard">
                <Button
                  variant="outline"
                  className="border-gray-700 text-gray-300 hover:bg-gray-800"
                >
                  Dashboard
                </Button>
              </Link>
              <Link href="/console">
                <Button className="bg-blue-600 hover:bg-blue-700">
                  Get Started
                </Button>
              </Link>
            </nav>
          </div>
        </div>
      </header>

      {/* Hero Section */}
      <main className="pt-24">
        <section className="container mx-auto px-4 py-24 text-center">
          {/* Badge */}
          <div className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-blue-500/10 border border-blue-500/20 text-blue-400 text-sm mb-8">
            <span className="relative flex h-2 w-2">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75"></span>
              <span className="relative inline-flex rounded-full h-2 w-2 bg-blue-500"></span>
            </span>
            Now in Production - Phase 7 Complete
          </div>

          {/* Main Headline */}
          <h2 className="text-5xl md:text-7xl font-bold mb-6 leading-tight">
            <span className="bg-gradient-to-r from-blue-400 via-purple-400 to-pink-400 bg-clip-text text-transparent">
              80% Cheaper
            </span>
            <br />
            Than Kafka
          </h2>

          <p className="text-xl md:text-2xl text-gray-400 max-w-3xl mx-auto mb-12 leading-relaxed">
            S3-native event streaming platform. Zero disks. Zero replication.
            Zero cluster management. Just stream.
          </p>

          {/* CTA Buttons */}
          <div className="flex flex-col sm:flex-row gap-4 justify-center mb-16">
            <Link href="/console">
              <Button
                size="lg"
                className="bg-blue-600 hover:bg-blue-700 text-lg px-8 py-6"
              >
                Get Started Free
                <ArrowRight className="ml-2 h-5 w-5" />
              </Button>
            </Link>
            <Link href="/docs">
              <Button
                size="lg"
                variant="outline"
                className="border-gray-700 text-gray-300 hover:bg-gray-800 text-lg px-8 py-6"
              >
                View Documentation
              </Button>
            </Link>
          </div>

          {/* Key Stats */}
          <div className="grid grid-cols-2 md:grid-cols-4 gap-8 max-w-4xl mx-auto">
            <div className="text-center">
              <div className="text-4xl md:text-5xl font-bold text-blue-400">
                80%
              </div>
              <div className="text-gray-500 mt-1">Cost Savings</div>
            </div>
            <div className="text-center">
              <div className="text-4xl md:text-5xl font-bold text-purple-400">
                0
              </div>
              <div className="text-gray-500 mt-1">Disks to Manage</div>
            </div>
            <div className="text-center">
              <div className="text-4xl md:text-5xl font-bold text-pink-400">
                50K
              </div>
              <div className="text-gray-500 mt-1">Messages/sec</div>
            </div>
            <div className="text-center">
              <div className="text-4xl md:text-5xl font-bold text-green-400">
                11 9s
              </div>
              <div className="text-gray-500 mt-1">S3 Durability</div>
            </div>
          </div>
        </section>

        {/* Architecture Comparison */}
        <section className="py-24 bg-gradient-to-b from-gray-900 to-gray-950">
          <div className="container mx-auto px-4">
            <div className="text-center mb-16">
              <h3 className="text-3xl md:text-4xl font-bold mb-4">
                Kafka Was Built for{" "}
                <span className="text-gray-500 line-through">2011</span>
              </h3>
              <p className="text-xl text-gray-400 max-w-2xl mx-auto">
                StreamHouse is built for the cloud era. No local disks. No
                replication lag. No operational headaches.
              </p>
            </div>

            <div className="grid md:grid-cols-2 gap-8 max-w-5xl mx-auto">
              {/* Kafka Side */}
              <Card className="bg-gray-900/50 border-gray-800">
                <CardHeader>
                  <div className="flex items-center gap-3">
                    <div className="p-2 bg-red-500/10 rounded-lg">
                      <Server className="h-6 w-6 text-red-400" />
                    </div>
                    <CardTitle className="text-xl text-gray-300">
                      Traditional Kafka
                    </CardTitle>
                  </div>
                </CardHeader>
                <CardContent className="space-y-4">
                  <div className="flex items-center gap-3 text-gray-400">
                    <HardDrive className="h-5 w-5 text-red-400" />
                    <span>Local disk replication across brokers</span>
                  </div>
                  <div className="flex items-center gap-3 text-gray-400">
                    <Server className="h-5 w-5 text-red-400" />
                    <span>ZooKeeper/KRaft coordination overhead</span>
                  </div>
                  <div className="flex items-center gap-3 text-gray-400">
                    <GitBranch className="h-5 w-5 text-red-400" />
                    <span>Complex partition rebalancing</span>
                  </div>
                  <div className="flex items-center gap-3 text-gray-400">
                    <DollarSign className="h-5 w-5 text-red-400" />
                    <span>$0.10-0.30/GB for EBS storage</span>
                  </div>
                  <div className="flex items-center gap-3 text-gray-400">
                    <Activity className="h-5 w-5 text-red-400" />
                    <span>Inter-AZ traffic costs add up fast</span>
                  </div>
                </CardContent>
              </Card>

              {/* StreamHouse Side */}
              <Card className="bg-gradient-to-br from-blue-500/10 to-purple-500/10 border-blue-500/30">
                <CardHeader>
                  <div className="flex items-center gap-3">
                    <div className="p-2 bg-blue-500/20 rounded-lg">
                      <Zap className="h-6 w-6 text-blue-400" />
                    </div>
                    <CardTitle className="text-xl text-white">
                      StreamHouse
                    </CardTitle>
                  </div>
                </CardHeader>
                <CardContent className="space-y-4">
                  <div className="flex items-center gap-3 text-gray-300">
                    <CheckCircle2 className="h-5 w-5 text-green-400" />
                    <span>Direct writes to S3 - no local disks</span>
                  </div>
                  <div className="flex items-center gap-3 text-gray-300">
                    <CheckCircle2 className="h-5 w-5 text-green-400" />
                    <span>Stateless agents - kill and restart freely</span>
                  </div>
                  <div className="flex items-center gap-3 text-gray-300">
                    <CheckCircle2 className="h-5 w-5 text-green-400" />
                    <span>Lease-based coordination - no ZooKeeper</span>
                  </div>
                  <div className="flex items-center gap-3 text-gray-300">
                    <CheckCircle2 className="h-5 w-5 text-green-400" />
                    <span>$0.023/GB S3 storage - 80% savings</span>
                  </div>
                  <div className="flex items-center gap-3 text-gray-300">
                    <CheckCircle2 className="h-5 w-5 text-green-400" />
                    <span>Zero inter-AZ replication traffic</span>
                  </div>
                </CardContent>
              </Card>
            </div>
          </div>
        </section>

        {/* Core Features */}
        <section className="py-24">
          <div className="container mx-auto px-4">
            <div className="text-center mb-16">
              <h3 className="text-3xl md:text-4xl font-bold mb-4">
                Everything You Need.{" "}
                <span className="text-gray-500">Nothing You Don&apos;t.</span>
              </h3>
              <p className="text-xl text-gray-400 max-w-2xl mx-auto">
                Purpose-built for cloud-native event streaming with built-in SQL
                processing.
              </p>
            </div>

            <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-6 max-w-6xl mx-auto">
              <Card className="bg-gray-900/50 border-gray-800 hover:border-blue-500/50 transition-colors">
                <CardHeader>
                  <Cloud className="h-10 w-10 text-blue-400 mb-4" />
                  <CardTitle className="text-white">S3-Native Storage</CardTitle>
                </CardHeader>
                <CardContent>
                  <CardDescription className="text-gray-400">
                    Events stored directly in S3 with 11 nines of durability.
                    LZ4 compression achieves 4-8x reduction. No disk capacity
                    planning ever again.
                  </CardDescription>
                </CardContent>
              </Card>

              <Card className="bg-gray-900/50 border-gray-800 hover:border-purple-500/50 transition-colors">
                <CardHeader>
                  <Layers className="h-10 w-10 text-purple-400 mb-4" />
                  <CardTitle className="text-white">Stateless Agents</CardTitle>
                </CardHeader>
                <CardContent>
                  <CardDescription className="text-gray-400">
                    All state lives in S3 + metadata store. Agents are ephemeral
                    compute. Scale horizontally, fail gracefully, restart
                    instantly.
                  </CardDescription>
                </CardContent>
              </Card>

              <Card className="bg-gray-900/50 border-gray-800 hover:border-pink-500/50 transition-colors">
                <CardHeader>
                  <Database className="h-10 w-10 text-pink-400 mb-4" />
                  <CardTitle className="text-white">
                    No Data Replication
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <CardDescription className="text-gray-400">
                    Only metadata is replicated - offsets, leases, watermarks.
                    S3 handles durability. Zero inter-broker traffic. Zero
                    replica lag.
                  </CardDescription>
                </CardContent>
              </Card>

              <Card className="bg-gray-900/50 border-gray-800 hover:border-green-500/50 transition-colors">
                <CardHeader>
                  <Code2 className="h-10 w-10 text-green-400 mb-4" />
                  <CardTitle className="text-white">
                    Built-in SQL Processing
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <CardDescription className="text-gray-400">
                    Stream processing with SQL - no separate Flink cluster.
                    Transformations, aggregations, windowing, and joins built
                    right in.
                  </CardDescription>
                </CardContent>
              </Card>

              <Card className="bg-gray-900/50 border-gray-800 hover:border-orange-500/50 transition-colors">
                <CardHeader>
                  <Gauge className="h-10 w-10 text-orange-400 mb-4" />
                  <CardTitle className="text-white">
                    Aggressive Caching
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <CardDescription className="text-gray-400">
                    LRU caching reduces S3 reads by 80-95%. Hot data served from
                    memory. Cold data fetched on demand. Best of both worlds.
                  </CardDescription>
                </CardContent>
              </Card>

              <Card className="bg-gray-900/50 border-gray-800 hover:border-cyan-500/50 transition-colors">
                <CardHeader>
                  <Activity className="h-10 w-10 text-cyan-400 mb-4" />
                  <CardTitle className="text-white">Full Observability</CardTitle>
                </CardHeader>
                <CardContent>
                  <CardDescription className="text-gray-400">
                    Prometheus metrics, health checks, consumer lag monitoring,
                    and Grafana dashboards. Production-ready from day one.
                  </CardDescription>
                </CardContent>
              </Card>
            </div>
          </div>
        </section>

        {/* How It Works */}
        <section className="py-24 bg-gradient-to-b from-gray-950 to-gray-900">
          <div className="container mx-auto px-4">
            <div className="text-center mb-16">
              <h3 className="text-3xl md:text-4xl font-bold mb-4">
                Disaggregated by Design
              </h3>
              <p className="text-xl text-gray-400 max-w-2xl mx-auto">
                Compute and storage scaled independently. The way cloud was
                meant to work.
              </p>
            </div>

            <div className="max-w-4xl mx-auto">
              <div className="grid md:grid-cols-3 gap-8">
                <div className="text-center">
                  <div className="w-16 h-16 bg-blue-500/20 rounded-2xl flex items-center justify-center mx-auto mb-4">
                    <Server className="h-8 w-8 text-blue-400" />
                  </div>
                  <h4 className="text-lg font-semibold mb-2">
                    Stateless Agents
                  </h4>
                  <p className="text-gray-400 text-sm">
                    Ephemeral compute nodes. Auto-scaling. No persistent state.
                    Any agent can handle any partition.
                  </p>
                </div>
                <div className="text-center">
                  <div className="w-16 h-16 bg-purple-500/20 rounded-2xl flex items-center justify-center mx-auto mb-4">
                    <Cloud className="h-8 w-8 text-purple-400" />
                  </div>
                  <h4 className="text-lg font-semibold mb-2">S3 Storage</h4>
                  <p className="text-gray-400 text-sm">
                    Infinite capacity. 11 nines durability. $0.023/GB. Managed
                    by AWS. You focus on your app.
                  </p>
                </div>
                <div className="text-center">
                  <div className="w-16 h-16 bg-pink-500/20 rounded-2xl flex items-center justify-center mx-auto mb-4">
                    <Database className="h-8 w-8 text-pink-400" />
                  </div>
                  <h4 className="text-lg font-semibold mb-2">
                    Metadata Store
                  </h4>
                  <p className="text-gray-400 text-sm">
                    PostgreSQL or SQLite. Topics, partitions, offsets, leases.
                    Small, fast, cached locally.
                  </p>
                </div>
              </div>

              {/* Architecture Diagram Placeholder */}
              <div className="mt-16 p-8 bg-gray-900/50 rounded-2xl border border-gray-800">
                <div className="text-center text-gray-500 py-12">
                  <div className="flex items-center justify-center gap-4 mb-8">
                    <div className="flex flex-col items-center">
                      <div className="w-24 h-16 bg-blue-500/20 rounded-lg flex items-center justify-center border border-blue-500/30">
                        <span className="text-blue-400 text-sm font-mono">
                          Producer
                        </span>
                      </div>
                    </div>
                    <ArrowRight className="h-6 w-6 text-gray-600" />
                    <div className="flex flex-col items-center">
                      <div className="w-24 h-16 bg-purple-500/20 rounded-lg flex items-center justify-center border border-purple-500/30">
                        <span className="text-purple-400 text-sm font-mono">
                          Agent
                        </span>
                      </div>
                    </div>
                    <ArrowRight className="h-6 w-6 text-gray-600" />
                    <div className="flex flex-col items-center">
                      <div className="w-24 h-16 bg-green-500/20 rounded-lg flex items-center justify-center border border-green-500/30">
                        <span className="text-green-400 text-sm font-mono">
                          S3
                        </span>
                      </div>
                    </div>
                    <ArrowRight className="h-6 w-6 text-gray-600" />
                    <div className="flex flex-col items-center">
                      <div className="w-24 h-16 bg-orange-500/20 rounded-lg flex items-center justify-center border border-orange-500/30">
                        <span className="text-orange-400 text-sm font-mono">
                          Consumer
                        </span>
                      </div>
                    </div>
                  </div>
                  <p className="text-sm">
                    Producer → Agent (gRPC) → S3 Segment → Consumer
                  </p>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Terminal Demo */}
        <section className="py-24">
          <div className="container mx-auto px-4">
            <div className="text-center mb-16">
              <h3 className="text-3xl md:text-4xl font-bold mb-4">
                Up and Running in Minutes
              </h3>
              <p className="text-xl text-gray-400 max-w-2xl mx-auto">
                Start streaming with a few commands. No complex setup required.
              </p>
            </div>

            <div className="max-w-3xl mx-auto">
              <div className="bg-gray-900 rounded-xl border border-gray-800 overflow-hidden shadow-2xl">
                {/* Terminal Header */}
                <div className="flex items-center gap-2 px-4 py-3 bg-gray-800/50 border-b border-gray-700">
                  <div className="w-3 h-3 rounded-full bg-red-500" />
                  <div className="w-3 h-3 rounded-full bg-yellow-500" />
                  <div className="w-3 h-3 rounded-full bg-green-500" />
                  <span className="ml-4 text-gray-500 text-sm font-mono">
                    terminal
                  </span>
                </div>

                {/* Terminal Content */}
                <div className="p-6 font-mono text-sm">
                  <div className="space-y-4">
                    <div>
                      <span className="text-green-400">$</span>{" "}
                      <span className="text-gray-300">
                        # Start the development stack
                      </span>
                    </div>
                    <div>
                      <span className="text-green-400">$</span>{" "}
                      <span className="text-white">docker compose up -d</span>
                    </div>
                    <div className="text-gray-500">
                      ✓ MinIO (S3) started on :9000
                      <br />✓ PostgreSQL started on :5432
                    </div>

                    <div className="pt-4">
                      <span className="text-green-400">$</span>{" "}
                      <span className="text-gray-300"># Start an agent</span>
                    </div>
                    <div>
                      <span className="text-green-400">$</span>{" "}
                      <span className="text-white">
                        cargo run --bin agent --features metrics
                      </span>
                    </div>
                    <div className="text-gray-500">
                      ✓ Agent started on :9090
                      <br />✓ Metrics available on :8080
                    </div>

                    <div className="pt-4">
                      <span className="text-green-400">$</span>{" "}
                      <span className="text-gray-300">
                        # Create a topic and produce
                      </span>
                    </div>
                    <div>
                      <span className="text-green-400">$</span>{" "}
                      <span className="text-white">
                        streamctl topics create events --partitions 4
                      </span>
                    </div>
                    <div>
                      <span className="text-green-400">$</span>{" "}
                      <span className="text-white">
                        streamctl produce events --key user123 --value
                        &apos;{`{"action":"click"}`}&apos;
                      </span>
                    </div>
                    <div className="text-blue-400">
                      → Produced to partition 2, offset 0
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Use Cases */}
        <section className="py-24 bg-gradient-to-b from-gray-900 to-gray-950">
          <div className="container mx-auto px-4">
            <div className="text-center mb-16">
              <h3 className="text-3xl md:text-4xl font-bold mb-4">
                Built for Modern Data Pipelines
              </h3>
              <p className="text-xl text-gray-400 max-w-2xl mx-auto">
                Replace complex infrastructure with a single, unified platform.
              </p>
            </div>

            <div className="grid md:grid-cols-2 gap-8 max-w-5xl mx-auto">
              <Card className="bg-gray-900/50 border-gray-800">
                <CardHeader>
                  <CardTitle className="text-white flex items-center gap-3">
                    <span className="text-2xl">📊</span>
                    Real-Time Analytics
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <p className="text-gray-400 mb-4">
                    Replace Kafka → Flink → ClickHouse with StreamHouse&apos;s
                    integrated SQL processing.
                  </p>
                  <div className="text-sm text-green-400">
                    50% cost reduction • 60% faster deployment
                  </div>
                </CardContent>
              </Card>

              <Card className="bg-gray-900/50 border-gray-800">
                <CardHeader>
                  <CardTitle className="text-white flex items-center gap-3">
                    <span className="text-2xl">📝</span>
                    Log Aggregation
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <p className="text-gray-400 mb-4">
                    Store logs directly in S3 at $0.023/GB instead of expensive
                    cloud logging services.
                  </p>
                  <div className="text-sm text-green-400">
                    90% cheaper than Datadog • Infinite retention
                  </div>
                </CardContent>
              </Card>

              <Card className="bg-gray-900/50 border-gray-800">
                <CardHeader>
                  <CardTitle className="text-white flex items-center gap-3">
                    <span className="text-2xl">🤖</span>
                    ML Feature Pipelines
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <p className="text-gray-400 mb-4">
                    Compute real-time features with SQL streaming. Direct S3
                    output for training.
                  </p>
                  <div className="text-sm text-green-400">
                    Real-time features • Eliminate Flink complexity
                  </div>
                </CardContent>
              </Card>

              <Card className="bg-gray-900/50 border-gray-800">
                <CardHeader>
                  <CardTitle className="text-white flex items-center gap-3">
                    <span className="text-2xl">🔄</span>
                    CDC & Data Sync
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <p className="text-gray-400 mb-4">
                    Simplified CDC pipelines with built-in SQL transforms. Fewer
                    moving parts.
                  </p>
                  <div className="text-sm text-green-400">
                    Simpler debugging • Reduced latency
                  </div>
                </CardContent>
              </Card>
            </div>
          </div>
        </section>

        {/* Tech Stack */}
        <section className="py-24">
          <div className="container mx-auto px-4">
            <div className="text-center mb-16">
              <h3 className="text-3xl md:text-4xl font-bold mb-4">
                Built with Modern Technology
              </h3>
              <p className="text-xl text-gray-400 max-w-2xl mx-auto">
                Performance and reliability at the core.
              </p>
            </div>

            <div className="flex flex-wrap justify-center gap-8 max-w-4xl mx-auto">
              <div className="flex items-center gap-3 px-6 py-3 bg-gray-900/50 rounded-lg border border-gray-800">
                <span className="text-2xl">🦀</span>
                <span className="text-gray-300">Rust</span>
              </div>
              <div className="flex items-center gap-3 px-6 py-3 bg-gray-900/50 rounded-lg border border-gray-800">
                <span className="text-2xl">📡</span>
                <span className="text-gray-300">gRPC</span>
              </div>
              <div className="flex items-center gap-3 px-6 py-3 bg-gray-900/50 rounded-lg border border-gray-800">
                <span className="text-2xl">☁️</span>
                <span className="text-gray-300">Amazon S3</span>
              </div>
              <div className="flex items-center gap-3 px-6 py-3 bg-gray-900/50 rounded-lg border border-gray-800">
                <span className="text-2xl">🐘</span>
                <span className="text-gray-300">PostgreSQL</span>
              </div>
              <div className="flex items-center gap-3 px-6 py-3 bg-gray-900/50 rounded-lg border border-gray-800">
                <span className="text-2xl">📊</span>
                <span className="text-gray-300">Prometheus</span>
              </div>
              <div className="flex items-center gap-3 px-6 py-3 bg-gray-900/50 rounded-lg border border-gray-800">
                <span className="text-2xl">🗜️</span>
                <span className="text-gray-300">LZ4</span>
              </div>
            </div>
          </div>
        </section>

        {/* CTA Section */}
        <section className="py-24 bg-gradient-to-b from-gray-950 to-blue-950/30">
          <div className="container mx-auto px-4 text-center">
            <h3 className="text-3xl md:text-5xl font-bold mb-6">
              Ready to Simplify Your Streaming?
            </h3>
            <p className="text-xl text-gray-400 max-w-2xl mx-auto mb-12">
              Join the teams saving 80% on streaming infrastructure while
              eliminating operational complexity.
            </p>
            <div className="flex flex-col sm:flex-row gap-4 justify-center">
              <Link href="/console">
                <Button
                  size="lg"
                  className="bg-blue-600 hover:bg-blue-700 text-lg px-8 py-6"
                >
                  Start Free
                  <ArrowRight className="ml-2 h-5 w-5" />
                </Button>
              </Link>
              <Link href="https://github.com/streamhouse/streamhouse">
                <Button
                  size="lg"
                  variant="outline"
                  className="border-gray-700 text-gray-300 hover:bg-gray-800 text-lg px-8 py-6"
                >
                  View on GitHub
                </Button>
              </Link>
            </div>
          </div>
        </section>
      </main>

      {/* Footer */}
      <footer className="border-t border-gray-800 py-16 bg-gray-950">
        <div className="container mx-auto px-4">
          <div className="grid md:grid-cols-4 gap-8 mb-12">
            <div>
              <div className="flex items-center space-x-2 mb-4">
                <Zap className="h-6 w-6 text-blue-500" />
                <span className="text-xl font-bold">StreamHouse</span>
              </div>
              <p className="text-gray-500 text-sm">
                S3-native event streaming. Built with Rust. Ready for
                production.
              </p>
            </div>
            <div>
              <h4 className="font-semibold mb-4 text-gray-300">Product</h4>
              <ul className="space-y-2 text-gray-500">
                <li>
                  <Link href="/docs" className="hover:text-white">
                    Documentation
                  </Link>
                </li>
                <li>
                  <Link href="/pricing" className="hover:text-white">
                    Pricing
                  </Link>
                </li>
                <li>
                  <Link href="/dashboard" className="hover:text-white">
                    Dashboard
                  </Link>
                </li>
                <li>
                  <Link href="/console" className="hover:text-white">
                    Console
                  </Link>
                </li>
              </ul>
            </div>
            <div>
              <h4 className="font-semibold mb-4 text-gray-300">Resources</h4>
              <ul className="space-y-2 text-gray-500">
                <li>
                  <Link href="/docs/quickstart" className="hover:text-white">
                    Quick Start
                  </Link>
                </li>
                <li>
                  <Link href="/docs/architecture" className="hover:text-white">
                    Architecture
                  </Link>
                </li>
                <li>
                  <Link href="/docs/api" className="hover:text-white">
                    API Reference
                  </Link>
                </li>
                <li>
                  <Link href="/blog" className="hover:text-white">
                    Blog
                  </Link>
                </li>
              </ul>
            </div>
            <div>
              <h4 className="font-semibold mb-4 text-gray-300">Community</h4>
              <ul className="space-y-2 text-gray-500">
                <li>
                  <Link
                    href="https://github.com/streamhouse/streamhouse"
                    className="hover:text-white"
                  >
                    GitHub
                  </Link>
                </li>
                <li>
                  <Link href="/discord" className="hover:text-white">
                    Discord
                  </Link>
                </li>
                <li>
                  <Link href="/twitter" className="hover:text-white">
                    Twitter
                  </Link>
                </li>
              </ul>
            </div>
          </div>
          <div className="border-t border-gray-800 pt-8 text-center text-gray-500 text-sm">
            <p>
              © {new Date().getFullYear()} StreamHouse. Built with Rust,
              powered by S3.
            </p>
          </div>
        </div>
      </footer>
    </div>
  );
}
