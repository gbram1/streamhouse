"use client";

import Link from "next/link";
import { Button } from "@/components/ui/button";
import { ArrowRight, Calendar, Clock, Zap } from "lucide-react";
import { getAllBlogPosts, getFeaturedPost } from "@/lib/blog-data";

function GithubIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
    </svg>
  );
}

export default function BlogPage() {
  const featuredPost = getFeaturedPost();
  const allPosts = getAllBlogPosts();
  const regularPosts = allPosts.filter((post) => !post.featured);

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
                    item.name === "Blog" ? "text-teal-600 font-medium" : "text-slate-600"
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
            <div className="absolute left-1/4 top-32 h-[400px] w-[400px] rounded-full bg-gradient-to-br from-blue-200/40 via-indigo-200/20 to-transparent blur-3xl" />
            <div className="absolute right-1/4 top-48 h-[300px] w-[300px] rounded-full bg-gradient-to-bl from-teal-200/30 via-cyan-100/20 to-transparent blur-3xl" />
          </div>

          <div className="relative mx-auto max-w-7xl px-6 py-16 lg:px-8 lg:py-24">
            <div className="mx-auto max-w-2xl text-center">
              <span className="inline-block rounded-full bg-blue-100 px-4 py-1.5 text-sm font-semibold text-blue-700">
                Latest Updates
              </span>
              <h1 className="mt-6 font-serif text-4xl italic tracking-tight text-slate-900 sm:text-5xl">
                Blog
              </h1>
              <p className="mt-4 text-lg text-slate-600">
                Engineering insights, product updates, and tutorials from the
                StreamHouse team.
              </p>
            </div>
          </div>
        </section>

        {/* Featured Post */}
        {featuredPost && (
          <section className="pb-16">
            <div className="mx-auto max-w-4xl px-6 lg:px-8">
              <Link href={`/blog/${featuredPost.slug}`} className="group block">
                <article className="relative overflow-hidden rounded-2xl border border-teal-200 bg-gradient-to-br from-teal-50 to-cyan-50 p-8 transition-all hover:shadow-lg lg:p-12">
                  <div className="absolute right-4 top-4">
                    <span className="inline-flex items-center rounded-full bg-teal-100 px-3 py-1 text-xs font-medium text-teal-700">
                      Featured
                    </span>
                  </div>

                  <div className="flex items-center gap-4 text-sm text-slate-500">
                    <span className="inline-flex items-center gap-1.5 rounded-full bg-white px-3 py-1 text-xs font-medium text-slate-600">
                      {featuredPost.category}
                    </span>
                    <span className="flex items-center gap-1">
                      <Calendar className="h-3.5 w-3.5" />
                      {new Date(featuredPost.publishedAt).toLocaleDateString(
                        "en-US",
                        {
                          month: "short",
                          day: "numeric",
                          year: "numeric",
                        }
                      )}
                    </span>
                    <span className="flex items-center gap-1">
                      <Clock className="h-3.5 w-3.5" />
                      {featuredPost.readingTime}
                    </span>
                  </div>

                  <h2 className="mt-4 text-2xl font-bold text-slate-900 transition-colors group-hover:text-teal-600 lg:text-3xl">
                    {featuredPost.title}
                  </h2>

                  <p className="mt-4 text-slate-600 lg:text-lg">
                    {featuredPost.excerpt}
                  </p>

                  <div className="mt-6 flex items-center justify-between">
                    <div className="flex items-center gap-3">
                      <div className="flex h-10 w-10 items-center justify-center rounded-full bg-gradient-to-br from-teal-500 to-cyan-500 text-sm font-medium text-white">
                        {featuredPost.author.avatar}
                      </div>
                      <div>
                        <p className="text-sm font-medium text-slate-900">
                          {featuredPost.author.name}
                        </p>
                        <p className="text-xs text-slate-500">
                          {featuredPost.author.role}
                        </p>
                      </div>
                    </div>

                    <span className="flex items-center gap-2 text-sm font-medium text-teal-600 transition-colors group-hover:text-teal-700">
                      Read more
                      <ArrowRight className="h-4 w-4 transition-transform group-hover:translate-x-1" />
                    </span>
                  </div>
                </article>
              </Link>
            </div>
          </section>
        )}

        {/* All Posts */}
        <section className="border-y border-slate-200 bg-white py-16">
          <div className="mx-auto max-w-5xl px-6 lg:px-8">
            <div className="text-center">
              <span className="text-sm font-semibold uppercase tracking-wider text-teal-600">
                All Articles
              </span>
              <h2 className="mt-3 font-serif text-3xl italic text-slate-900">
                Recent posts
              </h2>
            </div>

            <div className="mt-12 grid gap-6 md:grid-cols-2">
              {regularPosts.map((post) => (
                <Link
                  key={post.slug}
                  href={`/blog/${post.slug}`}
                  className="group block"
                >
                  <article className="h-full rounded-2xl border border-slate-200 bg-slate-50 p-6 transition-all hover:border-teal-200 hover:shadow-md">
                    <div className="flex items-center gap-3 text-xs text-slate-500">
                      <span className="inline-flex items-center rounded-full bg-white px-2.5 py-0.5 font-medium text-slate-600">
                        {post.category}
                      </span>
                      <span className="flex items-center gap-1">
                        <Calendar className="h-3 w-3" />
                        {new Date(post.publishedAt).toLocaleDateString("en-US", {
                          month: "short",
                          day: "numeric",
                        })}
                      </span>
                      <span className="flex items-center gap-1">
                        <Clock className="h-3 w-3" />
                        {post.readingTime}
                      </span>
                    </div>

                    <h3 className="mt-3 text-lg font-semibold text-slate-900 transition-colors group-hover:text-teal-600">
                      {post.title}
                    </h3>

                    <p className="mt-2 text-sm text-slate-600 line-clamp-2">
                      {post.excerpt}
                    </p>

                    <div className="mt-4 flex items-center gap-2">
                      <div className="flex h-7 w-7 items-center justify-center rounded-full bg-gradient-to-br from-teal-500 to-cyan-500 text-xs font-medium text-white">
                        {post.author.avatar}
                      </div>
                      <span className="text-sm text-slate-600">
                        {post.author.name}
                      </span>
                    </div>
                  </article>
                </Link>
              ))}
            </div>
          </div>
        </section>

        {/* Newsletter CTA */}
        <section className="py-24">
          <div className="mx-auto max-w-2xl px-6 lg:px-8">
            <div className="rounded-3xl border border-teal-200 bg-gradient-to-br from-teal-50 to-cyan-50 p-8 text-center">
              <Zap className="mx-auto h-10 w-10 text-teal-600" />
              <h3 className="mt-4 font-serif text-2xl italic text-slate-900">
                Subscribe to our newsletter
              </h3>
              <p className="mt-2 text-slate-600">
                Get the latest StreamHouse updates delivered to your inbox.
              </p>
              <form className="mt-6 flex flex-col gap-3 sm:flex-row sm:justify-center">
                <input
                  type="email"
                  placeholder="Enter your email"
                  className="h-11 rounded-lg border border-slate-200 bg-white px-4 text-sm text-slate-900 placeholder:text-slate-400 shadow-sm focus:border-teal-500 focus:outline-none focus:ring-2 focus:ring-teal-500/20 sm:w-64"
                />
                <Button className="h-11 bg-slate-900 px-6 text-white hover:bg-slate-800">
                  Subscribe
                </Button>
              </form>
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
