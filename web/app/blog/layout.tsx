import Link from "next/link";
import { Zap } from "lucide-react";
import { Button } from "@/components/ui/button";

export default function BlogLayout({
  children,
}: {
  children: React.ReactNode;
}) {
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
                  className="rounded-lg px-3 py-2 text-sm text-white transition-colors"
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

      <main>{children}</main>

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
                        href={item === "Blog" ? "/blog" : "/docs"}
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
