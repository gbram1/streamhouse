'use client';

import { useEffect } from 'react';
import { useRouter } from 'next/navigation';

export default function ConsolePage() {
  const router = useRouter();

  useEffect(() => {
    router.replace('/dashboard');
  }, [router]);

  return (
    <div className="flex h-screen items-center justify-center">
      <p>Redirecting to dashboard...</p>
    </div>
  );
}
