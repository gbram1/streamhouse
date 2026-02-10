import { MarketingLayout } from "@/components/layout/marketing-layout";

export default function DocsLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return <MarketingLayout>{children}</MarketingLayout>;
}
