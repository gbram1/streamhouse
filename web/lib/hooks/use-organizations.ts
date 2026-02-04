/**
 * React Query hooks for Organizations
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient, API_ENDPOINTS } from '../api-client';
import type {
  Organization,
  CreateOrganizationRequest,
  UpdateOrganizationRequest,
  OrganizationQuota,
  OrganizationUsage,
} from '../types';

// Fetch all organizations
export function useOrganizations() {
  return useQuery({
    queryKey: ['organizations'],
    queryFn: () => apiClient.get<Organization[]>(API_ENDPOINTS.organizations),
  });
}

// Fetch single organization
export function useOrganization(id: string) {
  return useQuery({
    queryKey: ['organization', id],
    queryFn: () => apiClient.get<Organization>(API_ENDPOINTS.organization(id)),
    enabled: !!id,
  });
}

// Fetch organization quota
export function useOrganizationQuota(id: string) {
  return useQuery({
    queryKey: ['organization-quota', id],
    queryFn: () => apiClient.get<OrganizationQuota>(API_ENDPOINTS.organizationQuota(id)),
    enabled: !!id,
  });
}

// Fetch organization usage
export function useOrganizationUsage(id: string) {
  return useQuery({
    queryKey: ['organization-usage', id],
    queryFn: () => apiClient.get<OrganizationUsage>(API_ENDPOINTS.organizationUsage(id)),
    enabled: !!id,
  });
}

// Create organization mutation
export function useCreateOrganization() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: CreateOrganizationRequest) =>
      apiClient.post<Organization>(API_ENDPOINTS.organizations, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['organizations'] });
    },
  });
}

// Update organization mutation
export function useUpdateOrganization() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: UpdateOrganizationRequest }) =>
      apiClient.patch<Organization>(API_ENDPOINTS.organization(id), data),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['organizations'] });
      queryClient.invalidateQueries({ queryKey: ['organization', variables.id] });
    },
  });
}

// Delete organization mutation
export function useDeleteOrganization() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => apiClient.delete(API_ENDPOINTS.organization(id)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['organizations'] });
    },
  });
}
