package io.streamhouse.client.model;

public class Partition {
    private String topic;
    private int partitionId;
    private String leaderAgentId;
    private long highWatermark;
    private long lowWatermark;

    public String getTopic() { return topic; }
    public void setTopic(String topic) { this.topic = topic; }

    public int getPartitionId() { return partitionId; }
    public void setPartitionId(int partitionId) { this.partitionId = partitionId; }

    public String getLeaderAgentId() { return leaderAgentId; }
    public void setLeaderAgentId(String leaderAgentId) { this.leaderAgentId = leaderAgentId; }

    public long getHighWatermark() { return highWatermark; }
    public void setHighWatermark(long highWatermark) { this.highWatermark = highWatermark; }

    public long getLowWatermark() { return lowWatermark; }
    public void setLowWatermark(long lowWatermark) { this.lowWatermark = lowWatermark; }
}
