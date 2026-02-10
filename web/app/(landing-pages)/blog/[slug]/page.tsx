import Link from "next/link";
import { notFound } from "next/navigation";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import {
  ArrowLeft,
  ArrowRight,
  Calendar,
  Clock,
  Share2,
  Twitter,
  Zap,
} from "lucide-react";
import { getBlogPost, getAllBlogPosts } from "@/lib/blog-data";

function GithubIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
    </svg>
  );
}

interface BlogPostPageProps {
  params: Promise<{
    slug: string;
  }>;
}

export async function generateStaticParams() {
  const posts = getAllBlogPosts();
  return posts.map((post) => ({
    slug: post.slug,
  }));
}

export default async function BlogPostPage({ params }: BlogPostPageProps) {
  const { slug } = await params;
  const post = getBlogPost(slug);

  if (!post) {
    notFound();
  }

  const allPosts = getAllBlogPosts();
  const currentIndex = allPosts.findIndex((p) => p.slug === post.slug);
  const relatedPosts = allPosts
    .filter((p) => p.slug !== post.slug)
    .slice(0, 2);

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
                    item.name === "Blog"
                      ? "font-medium text-teal-600"
                      : "text-slate-600"
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
        {/* Article header */}
        <div className="relative overflow-hidden">
          <div className="pointer-events-none absolute inset-0">
            <div className="absolute left-1/4 top-16 h-[300px] w-[400px] rounded-full bg-gradient-to-br from-blue-200/30 via-indigo-100/15 to-transparent blur-3xl" />
            <div className="absolute right-1/3 top-32 h-[250px] w-[300px] rounded-full bg-gradient-to-bl from-teal-200/20 via-cyan-100/10 to-transparent blur-3xl" />
          </div>

          <div className="relative mx-auto max-w-3xl px-6 pb-8 pt-10 lg:px-8 lg:pt-14">
            {/* Back link */}
            <Link
              href="/blog"
              className="inline-flex items-center gap-2 text-sm text-slate-500 transition-colors hover:text-teal-600"
            >
              <ArrowLeft className="h-3.5 w-3.5" />
              Back to blog
            </Link>

            {/* Meta */}
            <div className="mt-6 flex flex-wrap items-center gap-3">
              <Badge
                variant="secondary"
                className="bg-teal-50 text-xs font-medium text-teal-700"
              >
                {post.category}
              </Badge>
              <span className="flex items-center gap-1.5 text-sm text-slate-500">
                <Calendar className="h-3.5 w-3.5" />
                {new Date(post.publishedAt).toLocaleDateString("en-US", {
                  month: "long",
                  day: "numeric",
                  year: "numeric",
                })}
              </span>
              <span className="flex items-center gap-1.5 text-sm text-slate-500">
                <Clock className="h-3.5 w-3.5" />
                {post.readingTime}
              </span>
            </div>

            {/* Title */}
            <h1 className="mt-5 font-serif text-3xl italic tracking-tight text-slate-900 sm:text-4xl lg:text-[2.75rem] lg:leading-tight">
              {post.title}
            </h1>

            {/* Excerpt */}
            <p className="mt-5 text-lg leading-relaxed text-slate-600">
              {post.excerpt}
            </p>

            {/* Author & share */}
            <div className="mt-8 flex items-center justify-between">
              <div className="flex items-center gap-3.5">
                <div className="flex h-11 w-11 items-center justify-center rounded-full bg-gradient-to-br from-teal-500 to-cyan-500 text-sm font-semibold text-white">
                  {post.author.avatar}
                </div>
                <div>
                  <p className="text-sm font-medium text-slate-900">
                    {post.author.name}
                  </p>
                  <p className="text-xs text-slate-500">{post.author.role}</p>
                </div>
              </div>

              <div className="flex items-center gap-2">
                <button className="flex h-8 w-8 items-center justify-center rounded-lg border border-slate-200 bg-white text-slate-400 transition-colors hover:border-slate-300 hover:text-slate-600">
                  <Twitter className="h-3.5 w-3.5" />
                </button>
                <button className="flex h-8 w-8 items-center justify-center rounded-lg border border-slate-200 bg-white text-slate-400 transition-colors hover:border-slate-300 hover:text-slate-600">
                  <Share2 className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>
          </div>
        </div>

        <Separator className="bg-slate-200" />

        {/* Article body */}
        <article className="mx-auto max-w-3xl px-6 py-10 lg:px-8">
          <div
            className="prose prose-slate max-w-none prose-headings:font-semibold prose-headings:tracking-tight prose-h2:mt-10 prose-h2:text-2xl prose-h3:mt-8 prose-h3:text-lg prose-p:leading-relaxed prose-p:text-slate-600 prose-a:font-medium prose-a:text-teal-600 prose-a:no-underline hover:prose-a:text-teal-700 hover:prose-a:underline prose-strong:text-slate-900 prose-code:rounded-md prose-code:bg-slate-100 prose-code:px-1.5 prose-code:py-0.5 prose-code:text-sm prose-code:font-medium prose-code:text-slate-800 prose-code:before:content-none prose-code:after:content-none prose-pre:rounded-xl prose-pre:border prose-pre:border-slate-200 prose-pre:bg-slate-950 prose-pre:text-sm prose-ul:text-slate-600 prose-ol:text-slate-600 prose-li:marker:text-teal-500 prose-hr:border-slate-200"
            dangerouslySetInnerHTML={{
              __html: formatContent(post.content),
            }}
          />
        </article>

        {/* Tags */}
        <div className="mx-auto max-w-3xl px-6 lg:px-8">
          <Separator className="bg-slate-200" />
          <div className="flex flex-wrap items-center gap-2 py-6">
            <span className="text-sm text-slate-500">Tags:</span>
            {post.tags.map((tag) => (
              <Badge
                key={tag}
                variant="outline"
                className="border-slate-200 bg-white text-xs text-slate-600"
              >
                {tag}
              </Badge>
            ))}
          </div>
        </div>

        {/* Related posts */}
        {relatedPosts.length > 0 && (
          <section className="border-y border-slate-200 bg-white py-14">
            <div className="mx-auto max-w-4xl px-6 lg:px-8">
              <h3 className="text-center text-sm font-semibold uppercase tracking-wider text-teal-600">
                Keep Reading
              </h3>
              <h2 className="mt-2 text-center font-serif text-2xl italic text-slate-900">
                More from the blog
              </h2>

              <div className="mt-8 grid gap-6 sm:grid-cols-2">
                {relatedPosts.map((related) => (
                  <Link
                    key={related.slug}
                    href={`/blog/${related.slug}`}
                    className="group block"
                  >
                    <article className="h-full rounded-2xl border border-slate-200 bg-slate-50 p-6 transition-all hover:border-teal-200 hover:shadow-md">
                      <div className="flex items-center gap-3 text-xs text-slate-500">
                        <Badge
                          variant="secondary"
                          className="bg-white text-xs text-slate-600"
                        >
                          {related.category}
                        </Badge>
                        <span className="flex items-center gap-1">
                          <Clock className="h-3 w-3" />
                          {related.readingTime}
                        </span>
                      </div>
                      <h4 className="mt-3 text-base font-semibold text-slate-900 transition-colors group-hover:text-teal-600">
                        {related.title}
                      </h4>
                      <p className="mt-2 line-clamp-2 text-sm text-slate-600">
                        {related.excerpt}
                      </p>
                      <div className="mt-4 flex items-center gap-2">
                        <div className="flex h-6 w-6 items-center justify-center rounded-full bg-gradient-to-br from-teal-500 to-cyan-500 text-[10px] font-medium text-white">
                          {related.author.avatar}
                        </div>
                        <span className="text-xs text-slate-500">
                          {related.author.name}
                        </span>
                      </div>
                    </article>
                  </Link>
                ))}
              </div>
            </div>
          </section>
        )}

        {/* CTA */}
        <section className="py-16">
          <div className="mx-auto max-w-3xl px-6 lg:px-8">
            <div className="rounded-3xl border border-teal-200 bg-gradient-to-br from-teal-50 to-cyan-50 p-8 text-center lg:p-10">
              <Zap className="mx-auto h-9 w-9 text-teal-600" />
              <h3 className="mt-3 font-serif text-xl italic text-slate-900">
                Ready to try StreamHouse?
              </h3>
              <p className="mt-2 text-sm text-slate-600">
                Get started with S3-native event streaming in minutes.
              </p>
              <div className="mt-6 flex flex-col items-center justify-center gap-3 sm:flex-row">
                <Link href="/console">
                  <Button className="h-10 bg-slate-900 px-5 text-sm text-white hover:bg-slate-800">
                    Start building free
                    <ArrowRight className="ml-2 h-4 w-4" />
                  </Button>
                </Link>
                <Link href="/docs">
                  <Button
                    variant="outline"
                    className="h-10 border-slate-300 px-5 text-sm"
                  >
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
              &copy; {new Date().getFullYear()} StreamHouse &middot; MIT License
            </p>
          </div>
        </div>
      </footer>
    </div>
  );
}

function formatContent(content: string): string {
  let html = content
    .replace(/^### (.*$)/gm, "<h3>$1</h3>")
    .replace(/^## (.*$)/gm, "<h2>$1</h2>")
    .replace(/\*\*(.*?)\*\*/g, "<strong>$1</strong>")
    .replace(/```(\w+)?\n([\s\S]*?)```/g, (_, lang, code) => {
      return `<pre><code class="language-${lang || "text"}">${escapeHtml(code.trim())}</code></pre>`;
    })
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>')
    .replace(/^---$/gm, "<hr />");

  html = html
    .split("\n\n")
    .map((block) => {
      block = block.trim();
      if (!block) return "";
      if (
        block.startsWith("<h") ||
        block.startsWith("<pre") ||
        block.startsWith("<hr") ||
        block.startsWith("<ul") ||
        block.startsWith("<ol")
      ) {
        return block;
      }
      if (block.match(/^[-*] /m)) {
        const items = block
          .split("\n")
          .map((line) => {
            const match = line.match(/^[-*] (.+)/);
            return match ? `<li>${match[1]}</li>` : "";
          })
          .filter(Boolean);
        return `<ul>${items.join("")}</ul>`;
      }
      if (block.match(/^\d+\. /m)) {
        const items = block
          .split("\n")
          .map((line) => {
            const match = line.match(/^\d+\. (.+)/);
            return match ? `<li>${match[1]}</li>` : "";
          })
          .filter(Boolean);
        return `<ol>${items.join("")}</ol>`;
      }
      return `<p>${block.replace(/\n/g, "<br />")}</p>`;
    })
    .join("\n");

  return html;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}
