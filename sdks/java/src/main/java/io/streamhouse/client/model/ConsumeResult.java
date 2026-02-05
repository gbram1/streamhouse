package io.streamhouse.client.model;

import java.util.List;

public class ConsumeResult {
    private List<ConsumedRecord> records;
    private long nextOffset;

    public List<ConsumedRecord> getRecords() { return records; }
    public void setRecords(List<ConsumedRecord> records) { this.records = records; }

    public long getNextOffset() { return nextOffset; }
    public void setNextOffset(long nextOffset) { this.nextOffset = nextOffset; }
}
