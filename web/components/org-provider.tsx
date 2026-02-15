'use client';

import { useEffect } from 'react';
import { useOrganization } from '@clerk/nextjs';
import { apiClient } from '@/lib/api-client';

const DEFAULT_ORGANIZATION_ID = '00000000-0000-0000-0000-000000000000';

export function OrgProvider({ children }: { children: React.ReactNode }) {
  const { organization, isLoaded } = useOrganization();

  useEffect(() => {
    if (!isLoaded) return;

    if (organization) {
      apiClient.setOrganizationId(organization.id);
    } else {
      // Personal account — use default org
      apiClient.setOrganizationId(DEFAULT_ORGANIZATION_ID);
    }
  }, [organization, isLoaded]);

  return <>{children}</>;
}
