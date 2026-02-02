'use client';

import { DashboardLayout } from '@/components/layout/dashboard-layout';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useCreateTopic } from '@/lib/hooks/use-topics';
import { useRouter } from 'next/navigation';
import { useState } from 'react';

export default function NewTopicPage() {
  const router = useRouter();
  const createTopic = useCreateTopic();
  const [name, setName] = useState('');
  const [partitionCount, setPartitionCount] = useState(4);
  const [retentionMs, setRetentionMs] = useState('');

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    try {
      await createTopic.mutateAsync({
        name,
        partitions: partitionCount,  // API expects 'partitions' field
        replication_factor: 1,  // Default replication factor
        retentionMs: retentionMs ? parseInt(retentionMs) : undefined,
      });

      router.push('/topics');
    } catch (error) {
      console.error('Failed to create topic:', error);
    }
  };

  return (
    <DashboardLayout title="Create Topic" description="Create a new event stream">
      <Card className="max-w-2xl p-6">
        <form onSubmit={handleSubmit} className="space-y-6">
          <div>
            <Label htmlFor="name">Topic Name</Label>
            <Input
              id="name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="orders"
              required
            />
          </div>

          <div>
            <Label htmlFor="partitions">Partition Count</Label>
            <Input
              id="partitions"
              type="number"
              min={1}
              value={partitionCount}
              onChange={(e) => setPartitionCount(parseInt(e.target.value))}
              required
            />
          </div>

          <div>
            <Label htmlFor="retention">Retention (ms, optional)</Label>
            <Input
              id="retention"
              type="number"
              value={retentionMs}
              onChange={(e) => setRetentionMs(e.target.value)}
              placeholder="604800000"
            />
          </div>

          <div className="flex gap-4">
            <Button type="submit" disabled={createTopic.isPending}>
              {createTopic.isPending ? 'Creating...' : 'Create Topic'}
            </Button>
            <Button
              type="button"
              variant="outline"
              onClick={() => router.push('/topics')}
            >
              Cancel
            </Button>
          </div>
        </form>
      </Card>
    </DashboardLayout>
  );
}
