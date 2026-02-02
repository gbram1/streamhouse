/**
 * React Query hooks for Schema Registry
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient, API_ENDPOINTS } from '../api-client';
import type { Schema, SchemaSubject, SchemaRegistrationRequest } from '../types';

// API response for schema version
interface SchemaVersionResponse {
  subject: string;
  version: number;
  id: number;
  schema: string;
  schemaType: 'AVRO' | 'PROTOBUF' | 'JSON';
  references?: unknown[];
}

// Fetch all subjects with details
export function useSchemaSubjects() {
  return useQuery({
    queryKey: ['schema-subjects'],
    queryFn: async () => {
      // First, get all subject names
      const subjects = await apiClient.get<string[]>(API_ENDPOINTS.schemaSubjects);

      // Then fetch details for each subject
      const subjectDetails = await Promise.all(
        subjects.map(async (subject): Promise<SchemaSubject> => {
          try {
            // Get latest version details
            const latest = await apiClient.get<SchemaVersionResponse>(
              `/schemas/subjects/${encodeURIComponent(subject)}/versions/latest`
            );

            // Get all versions to count them
            const versions = await apiClient.get<number[]>(
              API_ENDPOINTS.schemaSubjectVersions(subject)
            );

            return {
              subject: subject,
              latestVersion: latest.version,
              schemaType: latest.schemaType,
              compatibilityMode: 'BACKWARD', // Default, API doesn't return this yet
              versionCount: versions.length,
            };
          } catch (error) {
            // Return minimal info if fetching details fails
            return {
              subject: subject,
              latestVersion: 1,
              schemaType: 'AVRO',
              compatibilityMode: 'BACKWARD',
              versionCount: 1,
            };
          }
        })
      );

      return subjectDetails;
    },
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
