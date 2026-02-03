import Link from "next/link";
import { ArrowRight, Calendar, Clock, User } from "lucide-react";
import { getAllBlogPosts, getFeaturedPost } from "@/lib/blog-data";

export default function BlogPage() {
  const featuredPost = getFeaturedPost();
  const allPosts = getAllBlogPosts();
  const regularPosts = allPosts.filter((post) => !post.featured);

  return (
    <div className="relative">
      {/* Background gradient */}
      <div className="absolute inset-0 bg-gradient-to-b from-blue-600/5 via-transparent to-transparent" />
      <div className="absolute left-1/2 top-0 h-[400px] w-[600px] -translate-x-1/2 bg-gradient-to-b from-blue-500/10 via-cyan-500/5 to-transparent blur-3xl" />

      <div className="relative mx-auto max-w-7xl px-6 py-16 lg:px-8 lg:py-24">
        {/* Header */}
        <div className="mx-auto max-w-2xl text-center">
          <h1 className="text-4xl font-bold tracking-tight text-white sm:text-5xl">
            Blog
          </h1>
          <p className="mt-4 text-lg text-[#8b90a3]">
            Engineering insights, product updates, and tutorials from the
            StreamHouse team.
          </p>
        </div>

        {/* Featured Post */}
        {featuredPost && (
          <div className="mx-auto mt-16 max-w-4xl">
            <Link href={`/blog/${featuredPost.slug}`} className="group block">
              <article className="relative overflow-hidden rounded-2xl border border-blue-500/20 bg-gradient-to-br from-blue-500/10 via-cyan-500/5 to-transparent p-8 transition-all hover:border-blue-500/30 lg:p-12">
                <div className="absolute right-4 top-4">
                  <span className="inline-flex items-center rounded-full bg-blue-500/20 px-3 py-1 text-xs font-medium text-blue-400">
                    Featured
                  </span>
                </div>

                <div className="flex items-center gap-4 text-sm text-[#6b7086]">
                  <span className="inline-flex items-center gap-1.5 rounded-full bg-white/5 px-3 py-1 text-xs font-medium text-[#a0a5b8]">
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

                <h2 className="mt-4 text-2xl font-bold text-white transition-colors group-hover:text-blue-400 lg:text-3xl">
                  {featuredPost.title}
                </h2>

                <p className="mt-4 text-[#8b90a3] lg:text-lg">
                  {featuredPost.excerpt}
                </p>

                <div className="mt-6 flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <div className="flex h-10 w-10 items-center justify-center rounded-full bg-gradient-to-br from-blue-500 to-cyan-400 text-sm font-medium text-white">
                      {featuredPost.author.avatar}
                    </div>
                    <div>
                      <p className="text-sm font-medium text-white">
                        {featuredPost.author.name}
                      </p>
                      <p className="text-xs text-[#6b7086]">
                        {featuredPost.author.role}
                      </p>
                    </div>
                  </div>

                  <span className="flex items-center gap-2 text-sm font-medium text-blue-400 transition-colors group-hover:text-blue-300">
                    Read more
                    <ArrowRight className="h-4 w-4 transition-transform group-hover:translate-x-1" />
                  </span>
                </div>
              </article>
            </Link>
          </div>
        )}

        {/* All Posts */}
        <div className="mx-auto mt-16 max-w-5xl">
          <h2 className="text-xl font-semibold text-white">All Posts</h2>

          <div className="mt-8 grid gap-6 md:grid-cols-2">
            {regularPosts.map((post) => (
              <Link
                key={post.slug}
                href={`/blog/${post.slug}`}
                className="group block"
              >
                <article className="h-full rounded-xl border border-white/5 bg-[#0e0e18] p-6 transition-all hover:border-white/10 hover:bg-[#121220]">
                  <div className="flex items-center gap-3 text-xs text-[#6b7086]">
                    <span className="inline-flex items-center rounded-full bg-white/5 px-2.5 py-0.5 font-medium text-[#a0a5b8]">
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

                  <h3 className="mt-3 text-lg font-semibold text-white transition-colors group-hover:text-blue-400">
                    {post.title}
                  </h3>

                  <p className="mt-2 text-sm text-[#6b7086] line-clamp-2">
                    {post.excerpt}
                  </p>

                  <div className="mt-4 flex items-center gap-2">
                    <div className="flex h-7 w-7 items-center justify-center rounded-full bg-gradient-to-br from-blue-500/50 to-cyan-400/50 text-xs font-medium text-white">
                      {post.author.avatar}
                    </div>
                    <span className="text-sm text-[#8b90a3]">
                      {post.author.name}
                    </span>
                  </div>
                </article>
              </Link>
            ))}
          </div>
        </div>

        {/* Newsletter CTA */}
        <div className="mx-auto mt-20 max-w-2xl">
          <div className="rounded-2xl border border-white/5 bg-[#0e0e18] p-8 text-center">
            <h3 className="text-xl font-semibold text-white">
              Subscribe to our newsletter
            </h3>
            <p className="mt-2 text-[#6b7086]">
              Get the latest StreamHouse updates delivered to your inbox.
            </p>
            <form className="mt-6 flex flex-col gap-3 sm:flex-row sm:justify-center">
              <input
                type="email"
                placeholder="Enter your email"
                className="h-11 rounded-lg border border-white/10 bg-white/5 px-4 text-sm text-white placeholder:text-[#6b7086] focus:border-blue-500/50 focus:outline-none focus:ring-1 focus:ring-blue-500/50 sm:w-64"
              />
              <button
                type="submit"
                className="h-11 rounded-lg bg-gradient-to-r from-blue-600 to-blue-500 px-6 text-sm font-medium text-white shadow-lg shadow-blue-500/20 transition-all hover:shadow-blue-500/30"
              >
                Subscribe
              </button>
            </form>
          </div>
        </div>
      </div>
    </div>
  );
}
