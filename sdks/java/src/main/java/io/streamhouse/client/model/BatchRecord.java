package io.streamhouse.client.model;

public class BatchRecord {
    private String value;
    private String key;
    private Integer partition;

    public BatchRecord() {}

    public BatchRecord(String value) {
        this.value = value;
    }

    public BatchRecord(String value, String key) {
        this.value = value;
        this.key = key;
    }

    public static BatchRecord of(String value) {
        return new BatchRecord(value);
    }

    public static BatchRecord of(String value, String key) {
        return new BatchRecord(value, key);
    }

    public String getValue() { return value; }
    public void setValue(String value) { this.value = value; }

    public String getKey() { return key; }
    public void setKey(String key) { this.key = key; }

    public Integer getPartition() { return partition; }
    public void setPartition(Integer partition) { this.partition = partition; }
}
