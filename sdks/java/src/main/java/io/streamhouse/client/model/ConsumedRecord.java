package io.streamhouse.client.model;

public class ConsumedRecord {
    private int partition;
    private long offset;
    private String key;
    private String value;
    private long timestamp;

    public int getPartition() { return partition; }
    public void setPartition(int partition) { this.partition = partition; }

    public long getOffset() { return offset; }
    public void setOffset(long offset) { this.offset = offset; }

    public String getKey() { return key; }
    public void setKey(String key) { this.key = key; }

    public String getValue() { return value; }
    public void setValue(String value) { this.value = value; }

    public long getTimestamp() { return timestamp; }
    public void setTimestamp(long timestamp) { this.timestamp = timestamp; }
}
