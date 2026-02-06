"use client";

import { useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { Search, ChevronRight, X, Menu } from "lucide-react";
import { navigation } from "@/lib/docs-content";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";

export function DocsSidebar() {
  const pathname = usePathname();
  const [search, setSearch] = useState("");
  const [mobileOpen, setMobileOpen] = useState(false);

  const filteredNav = search
    ? navigation
        .map((section) => ({
          ...section,
          links: section.links.filter((link) =>
            link.title.toLowerCase().includes(search.toLowerCase())
          ),
        }))
        .filter((section) => section.links.length > 0)
    : navigation;

  const sidebar = (
    <div className="flex h-full flex-col">
      {/* Search */}
      <div className="relative px-4 pb-4 pt-2">
        <Search className="absolute left-7 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-400" />
        <Input
          type="text"
          placeholder="Search docs..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="h-9 border-slate-200 bg-white pl-9 text-sm text-slate-900 placeholder:text-slate-400 focus-visible:border-teal-500 focus-visible:ring-teal-500/20"
        />
        {search && (
          <button
            onClick={() => setSearch("")}
            className="absolute right-7 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-600"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        )}
      </div>

      <Separator className="bg-slate-200" />

      {/* Navigation */}
      <nav className="flex-1 overflow-y-auto px-3 py-4">
        <div className="space-y-6">
          {filteredNav.map((section) => (
            <div key={section.title}>
              <h4 className="mb-1.5 px-2 text-xs font-semibold uppercase tracking-wider text-slate-500">
                {section.title}
              </h4>
              <ul className="space-y-0.5">
                {section.links.map((link) => {
                  const isActive = pathname === link.href;
                  return (
                    <li key={link.href}>
                      <Link
                        href={link.href}
                        onClick={() => setMobileOpen(false)}
                        className={`flex items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors ${
                          isActive
                            ? "bg-teal-50 font-medium text-teal-700"
                            : "text-slate-600 hover:bg-slate-100 hover:text-slate-900"
                        }`}
                      >
                        {isActive && (
                          <ChevronRight className="h-3 w-3 flex-shrink-0" />
                        )}
                        <span className={isActive ? "" : "pl-5"}>
                          {link.title}
                        </span>
                      </Link>
                    </li>
                  );
                })}
              </ul>
            </div>
          ))}
        </div>

        {search && filteredNav.length === 0 && (
          <p className="px-2 py-8 text-center text-sm text-slate-400">
            No results for &ldquo;{search}&rdquo;
          </p>
        )}
      </nav>

      {/* Back to docs */}
      <Separator className="bg-slate-200" />
      <div className="px-4 py-3">
        <Link
          href="/docs"
          className="text-xs text-slate-500 transition-colors hover:text-teal-600"
        >
          &larr; Back to Documentation
        </Link>
      </div>
    </div>
  );

  return (
    <>
      {/* Mobile toggle */}
      <Button
        variant="outline"
        size="sm"
        className="fixed bottom-4 right-4 z-50 border-slate-300 bg-white shadow-lg lg:hidden"
        onClick={() => setMobileOpen(!mobileOpen)}
      >
        {mobileOpen ? (
          <X className="mr-1.5 h-4 w-4" />
        ) : (
          <Menu className="mr-1.5 h-4 w-4" />
        )}
        {mobileOpen ? "Close" : "Menu"}
      </Button>

      {/* Mobile overlay */}
      {mobileOpen && (
        <div
          className="fixed inset-0 z-40 bg-black/20 backdrop-blur-sm lg:hidden"
          onClick={() => setMobileOpen(false)}
        />
      )}

      {/* Mobile sidebar */}
      <aside
        className={`fixed bottom-0 left-0 top-16 z-40 w-72 border-r border-slate-200 bg-[#faf8f5] transition-transform lg:hidden ${
          mobileOpen ? "translate-x-0" : "-translate-x-full"
        }`}
      >
        {sidebar}
      </aside>

      {/* Desktop sidebar */}
      <aside className="hidden w-64 flex-shrink-0 border-r border-slate-200 lg:block">
        <div className="sticky top-16 h-[calc(100vh-4rem)] overflow-hidden">
          {sidebar}
        </div>
      </aside>
    </>
  );
}
