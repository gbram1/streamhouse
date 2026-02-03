import Link from "next/link";
import { notFound } from "next/navigation";
import { ArrowLeft, Calendar, Clock, Share2, Twitter } from "lucide-react";
import { getBlogPost, getAllBlogPosts } from "@/lib/blog-data";

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

  return (
    <div className="relative">
      {/* Background gradient */}
      <div className="absolute inset-0 bg-gradient-to-b from-blue-600/5 via-transparent to-transparent" />
      <div className="absolute left-1/2 top-0 h-[300px] w-[500px] -translate-x-1/2 bg-gradient-to-b from-blue-500/10 via-cyan-500/5 to-transparent blur-3xl" />

      <div className="relative mx-auto max-w-4xl px-6 py-12 lg:px-8 lg:py-16">
        {/* Back link */}
        <Link
          href="/blog"
          className="inline-flex items-center gap-2 text-sm text-[#6b7086] transition-colors hover:text-white"
        >
          <ArrowLeft className="h-4 w-4" />
          Back to blog
        </Link>

        {/* Article header */}
        <header className="mt-8">
          <div className="flex flex-wrap items-center gap-3 text-sm text-[#6b7086]">
            <span className="inline-flex items-center rounded-full bg-blue-500/10 px-3 py-1 text-xs font-medium text-blue-400">
              {post.category}
            </span>
            <span className="flex items-center gap-1.5">
              <Calendar className="h-4 w-4" />
              {new Date(post.publishedAt).toLocaleDateString("en-US", {
                month: "long",
                day: "numeric",
                year: "numeric",
              })}
            </span>
            <span className="flex items-center gap-1.5">
              <Clock className="h-4 w-4" />
              {post.readingTime}
            </span>
          </div>

          <h1 className="mt-6 text-3xl font-bold tracking-tight text-white sm:text-4xl lg:text-5xl">
            {post.title}
          </h1>

          <p className="mt-6 text-lg text-[#8b90a3]">{post.excerpt}</p>

          {/* Author and share */}
          <div className="mt-8 flex items-center justify-between border-b border-white/5 pb-8">
            <div className="flex items-center gap-4">
              <div className="flex h-12 w-12 items-center justify-center rounded-full bg-gradient-to-br from-blue-500 to-cyan-400 text-sm font-semibold text-white">
                {post.author.avatar}
              </div>
              <div>
                <p className="font-medium text-white">{post.author.name}</p>
                <p className="text-sm text-[#6b7086]">{post.author.role}</p>
              </div>
            </div>

            <div className="flex items-center gap-2">
              <button className="flex h-9 w-9 items-center justify-center rounded-lg border border-white/10 bg-white/5 text-[#6b7086] transition-colors hover:bg-white/10 hover:text-white">
                <Twitter className="h-4 w-4" />
              </button>
              <button className="flex h-9 w-9 items-center justify-center rounded-lg border border-white/10 bg-white/5 text-[#6b7086] transition-colors hover:bg-white/10 hover:text-white">
                <Share2 className="h-4 w-4" />
              </button>
            </div>
          </div>
        </header>

        {/* Article content */}
        <article className="prose prose-invert mt-10 max-w-none prose-headings:font-semibold prose-headings:tracking-tight prose-h2:mt-12 prose-h2:text-2xl prose-h3:mt-8 prose-h3:text-xl prose-p:text-[#c8cde4] prose-p:leading-relaxed prose-a:text-blue-400 prose-a:no-underline hover:prose-a:text-blue-300 prose-strong:text-white prose-code:rounded prose-code:bg-white/10 prose-code:px-1.5 prose-code:py-0.5 prose-code:text-sm prose-code:text-cyan-400 prose-code:before:content-none prose-code:after:content-none prose-pre:border prose-pre:border-white/10 prose-pre:bg-[#0a0a12] prose-ul:text-[#c8cde4] prose-li:marker:text-blue-400">
          <div
            dangerouslySetInnerHTML={{
              __html: formatContent(post.content),
            }}
          />
        </article>

        {/* Tags */}
        <div className="mt-12 border-t border-white/5 pt-8">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-sm text-[#6b7086]">Tags:</span>
            {post.tags.map((tag) => (
              <span
                key={tag}
                className="rounded-full bg-white/5 px-3 py-1 text-xs text-[#a0a5b8]"
              >
                {tag}
              </span>
            ))}
          </div>
        </div>

        {/* CTA */}
        <div className="mt-12 rounded-2xl border border-blue-500/20 bg-gradient-to-br from-blue-500/10 via-cyan-500/5 to-transparent p-8 text-center">
          <h3 className="text-xl font-semibold text-white">
            Ready to try StreamHouse?
          </h3>
          <p className="mt-2 text-[#8b90a3]">
            Get started with S3-native event streaming in minutes.
          </p>
          <div className="mt-6 flex flex-col items-center justify-center gap-3 sm:flex-row">
            <Link
              href="/console"
              className="inline-flex h-11 items-center justify-center rounded-lg bg-gradient-to-r from-blue-600 to-blue-500 px-6 text-sm font-medium text-white shadow-lg shadow-blue-500/20 transition-all hover:shadow-blue-500/30"
            >
              Start building free
            </Link>
            <Link
              href="/docs"
              className="inline-flex h-11 items-center justify-center rounded-lg border border-white/10 bg-white/5 px-6 text-sm font-medium text-white transition-colors hover:bg-white/10"
            >
              Read documentation
            </Link>
          </div>
        </div>
      </div>
    </div>
  );
}

function formatContent(content: string): string {
  // Convert markdown-like content to HTML
  let html = content
    // Headers
    .replace(/^### (.*$)/gm, '<h3>$1</h3>')
    .replace(/^## (.*$)/gm, '<h2>$1</h2>')
    // Bold
    .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
    // Code blocks
    .replace(/```(\w+)?\n([\s\S]*?)```/g, (_, lang, code) => {
      return `<pre><code class="language-${lang || 'text'}">${escapeHtml(code.trim())}</code></pre>`;
    })
    // Inline code
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    // Links
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>')
    // Horizontal rules
    .replace(/^---$/gm, '<hr />')
    // Paragraphs
    .split('\n\n')
    .map((block) => {
      block = block.trim();
      if (!block) return '';
      if (block.startsWith('<h') || block.startsWith('<pre') || block.startsWith('<hr') || block.startsWith('<ul') || block.startsWith('<ol')) {
        return block;
      }
      // Handle lists
      if (block.match(/^[-*] /m)) {
        const items = block.split('\n').map((line) => {
          const match = line.match(/^[-*] (.+)/);
          return match ? `<li>${match[1]}</li>` : '';
        }).filter(Boolean);
        return `<ul>${items.join('')}</ul>`;
      }
      // Handle numbered lists
      if (block.match(/^\d+\. /m)) {
        const items = block.split('\n').map((line) => {
          const match = line.match(/^\d+\. (.+)/);
          return match ? `<li>${match[1]}</li>` : '';
        }).filter(Boolean);
        return `<ol>${items.join('')}</ol>`;
      }
      return `<p>${block.replace(/\n/g, '<br />')}</p>`;
    })
    .join('\n');

  return html;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
