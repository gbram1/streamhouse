import Link from "next/link";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Activity, Database, Gauge, Zap } from "lucide-react";

export default function Home() {
  return (
    <div className="min-h-screen bg-gradient-to-b from-gray-50 to-gray-100 dark:from-gray-950 dark:to-gray-900">
      {/* Header */}
      <header className="border-b bg-white/50 backdrop-blur-sm dark:bg-gray-950/50">
        <div className="container mx-auto px-4 py-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-2">
              <Zap className="h-8 w-8 text-blue-600" />
              <h1 className="text-2xl font-bold">StreamHouse</h1>
            </div>
            <nav className="flex items-center space-x-4">
              <Link href="/dashboard">
                <Button variant="ghost">Dashboard</Button>
              </Link>
              <Link href="/topics">
                <Button variant="ghost">Topics</Button>
              </Link>
              <Link href="/agents">
                <Button variant="ghost">Agents</Button>
              </Link>
              <Link href="/console">
                <Button variant="ghost">Console</Button>
              </Link>
              <Button>Sign In</Button>
            </nav>
          </div>
        </div>
      </header>

      {/* Hero Section */}
      <main className="container mx-auto px-4 py-16">
        <div className="text-center mb-16">
          <h2 className="text-5xl font-bold mb-4 bg-gradient-to-r from-blue-600 to-purple-600 bg-clip-text text-transparent">
            S3-Native Event Streaming
          </h2>
          <p className="text-xl text-gray-600 dark:text-gray-400 max-w-2xl mx-auto mb-8">
            The modern alternative to Kafka. Built for the cloud, powered by S3.
            Zero cluster management, infinite scale.
          </p>
          <div className="flex gap-4 justify-center">
            <Link href="/dashboard">
              <Button size="lg">Get Started</Button>
            </Link>
            <Link href="/docs">
              <Button size="lg" variant="outline">
                Documentation
              </Button>
            </Link>
          </div>
        </div>

        {/* Features Grid */}
        <div className="grid md:grid-cols-2 lg:grid-cols-4 gap-6 mb-16">
          <Card>
            <CardHeader>
              <Database className="h-8 w-8 text-blue-600 mb-2" />
              <CardTitle>S3-Native Storage</CardTitle>
            </CardHeader>
            <CardContent>
              <CardDescription>
                Store events directly in S3 with intelligent caching. No local
                disk limits.
              </CardDescription>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <Zap className="h-8 w-8 text-purple-600 mb-2" />
              <CardTitle>Zero Management</CardTitle>
            </CardHeader>
            <CardContent>
              <CardDescription>
                Stateless agents mean no rebalancing, no ZooKeeper, no cluster
                management.
              </CardDescription>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <Gauge className="h-8 w-8 text-green-600 mb-2" />
              <CardTitle>SQL Processing</CardTitle>
            </CardHeader>
            <CardContent>
              <CardDescription>
                Built-in stream processing with SQL. No separate Flink cluster
                required.
              </CardDescription>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <Activity className="h-8 w-8 text-orange-600 mb-2" />
              <CardTitle>Real-time Metrics</CardTitle>
            </CardHeader>
            <CardContent>
              <CardDescription>
                Prometheus metrics, health checks, and consumer lag monitoring
                out of the box.
              </CardDescription>
            </CardContent>
          </Card>
        </div>

        {/* Quick Stats */}
        <Card className="mb-16">
          <CardHeader>
            <CardTitle>Quick Stats</CardTitle>
            <CardDescription>
              Real-time overview of your StreamHouse cluster
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-6">
              <div>
                <div className="text-3xl font-bold text-blue-600">0</div>
                <div className="text-sm text-gray-600 dark:text-gray-400">
                  Active Topics
                </div>
              </div>
              <div>
                <div className="text-3xl font-bold text-green-600">0</div>
                <div className="text-sm text-gray-600 dark:text-gray-400">
                  Agents Running
                </div>
              </div>
              <div>
                <div className="text-3xl font-bold text-purple-600">0</div>
                <div className="text-sm text-gray-600 dark:text-gray-400">
                  Messages/sec
                </div>
              </div>
              <div>
                <div className="text-3xl font-bold text-orange-600">0 B</div>
                <div className="text-sm text-gray-600 dark:text-gray-400">
                  Storage Used
                </div>
              </div>
            </div>
          </CardContent>
        </Card>

        {/* Getting Started */}
        <Card>
          <CardHeader>
            <CardTitle>Quick Start</CardTitle>
            <CardDescription>
              Get up and running with StreamHouse in minutes
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              <div className="flex items-start space-x-4">
                <div className="flex-shrink-0 w-8 h-8 rounded-full bg-blue-600 text-white flex items-center justify-center font-bold">
                  1
                </div>
                <div>
                  <h4 className="font-semibold mb-1">Start an Agent</h4>
                  <code className="text-sm bg-gray-100 dark:bg-gray-800 px-2 py-1 rounded">
                    cargo run --bin agent --features metrics
                  </code>
                </div>
              </div>
              <div className="flex items-start space-x-4">
                <div className="flex-shrink-0 w-8 h-8 rounded-full bg-blue-600 text-white flex items-center justify-center font-bold">
                  2
                </div>
                <div>
                  <h4 className="font-semibold mb-1">Create a Topic</h4>
                  <code className="text-sm bg-gray-100 dark:bg-gray-800 px-2 py-1 rounded">
                    streamctl topics create orders --partitions 3
                  </code>
                </div>
              </div>
              <div className="flex items-start space-x-4">
                <div className="flex-shrink-0 w-8 h-8 rounded-full bg-blue-600 text-white flex items-center justify-center font-bold">
                  3
                </div>
                <div>
                  <h4 className="font-semibold mb-1">Start Producing</h4>
                  <code className="text-sm bg-gray-100 dark:bg-gray-800 px-2 py-1 rounded">
                    streamctl produce orders --key user123 --value
                    &apos;&#123;&quot;event&quot;:&quot;purchase&quot;&#125;&apos;
                  </code>
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      </main>

      {/* Footer */}
      <footer className="border-t mt-16 py-8 bg-white/50 backdrop-blur-sm dark:bg-gray-950/50">
        <div className="container mx-auto px-4 text-center text-gray-600 dark:text-gray-400">
          <p>
            StreamHouse - Built with Rust, powered by S3, ready for production.
          </p>
        </div>
      </footer>
    </div>
  );
}
