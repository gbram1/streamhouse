/**
 * React Query hooks for Schema Registry
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient, API_ENDPOINTS } from '../api-client';
import type { Schema, SchemaSubject, SchemaRegistrationRequest } from '../types';

// Fetch all subjects
export function useSchemaSubjects() {
  return useQuery({
    queryKey: ['schema-subjects'],
    queryFn: () => apiClient.get<SchemaSubject[]>(API_ENDPOINTS.schemaSubjects),
  });
}

// Fetch schema by ID
export function useSchema(id: number) {
  return useQuery({
    queryKey: ['schema', id],
    queryFn: () => apiClient.get<Schema>(API_ENDPOINTS.schemaById(id)),
    enabled: id > 0,
  });
}

// Fetch subject versions
export function useSubjectVersions(subject: string) {
  return useQuery({
    queryKey: ['subject-versions', subject],
    queryFn: () => apiClient.get<number[]>(API_ENDPOINTS.schemaSubjectVersions(subject)),
    enabled: !!subject,
  });
}

// Fetch specific schema version
export function useSchemaVersion(subject: string, version: number) {
  return useQuery({
    queryKey: ['schema-version', subject, version],
    queryFn: () =>
      apiClient.get<Schema>(API_ENDPOINTS.schemaSubjectVersion(subject, version)),
    enabled: !!subject && version > 0,
  });
}

// Register new schema
export function useRegisterSchema() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ subject, data }: { subject: string; data: SchemaRegistrationRequest }) =>
      apiClient.post<{ id: number }>(
        API_ENDPOINTS.schemaSubjectVersions(subject),
        data
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['schema-subjects'] });
    },
  });
}

// Delete subject
export function useDeleteSubject() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (subject: string) =>
      apiClient.delete(API_ENDPOINTS.schemaSubject(subject)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['schema-subjects'] });
    },
  });
}
