import { MarketingLayout } from "@/components/layout/marketing-layout";

export default function ProductLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return <MarketingLayout>{children}</MarketingLayout>;
}
