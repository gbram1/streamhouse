package io.streamhouse.client.model;

public class MetricsSnapshot {
    private long topicsCount;
    private long agentsCount;
    private long partitionsCount;
    private long totalMessages;

    public long getTopicsCount() { return topicsCount; }
    public void setTopicsCount(long topicsCount) { this.topicsCount = topicsCount; }

    public long getAgentsCount() { return agentsCount; }
    public void setAgentsCount(long agentsCount) { this.agentsCount = agentsCount; }

    public long getPartitionsCount() { return partitionsCount; }
    public void setPartitionsCount(long partitionsCount) { this.partitionsCount = partitionsCount; }

    public long getTotalMessages() { return totalMessages; }
    public void setTotalMessages(long totalMessages) { this.totalMessages = totalMessages; }
}
