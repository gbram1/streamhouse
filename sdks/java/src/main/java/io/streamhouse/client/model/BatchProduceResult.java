package io.streamhouse.client.model;

import java.util.List;

public class BatchProduceResult {
    private int count;
    private List<BatchRecordResult> offsets;

    public int getCount() { return count; }
    public void setCount(int count) { this.count = count; }

    public List<BatchRecordResult> getOffsets() { return offsets; }
    public void setOffsets(List<BatchRecordResult> offsets) { this.offsets = offsets; }

    public static class BatchRecordResult {
        private int partition;
        private long offset;

        public int getPartition() { return partition; }
        public void setPartition(int partition) { this.partition = partition; }

        public long getOffset() { return offset; }
        public void setOffset(long offset) { this.offset = offset; }
    }
}
