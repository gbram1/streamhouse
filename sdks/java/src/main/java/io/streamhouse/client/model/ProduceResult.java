package io.streamhouse.client.model;

public class ProduceResult {
    private long offset;
    private int partition;

    public long getOffset() { return offset; }
    public void setOffset(long offset) { this.offset = offset; }

    public int getPartition() { return partition; }
    public void setPartition(int partition) { this.partition = partition; }
}
