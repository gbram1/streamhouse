package io.streamhouse.client.model;

import java.util.List;

public class ConsumerGroup {
    private String groupId;
    private List<String> topics;
    private long totalLag;
    private int partitionCount;

    public String getGroupId() { return groupId; }
    public void setGroupId(String groupId) { this.groupId = groupId; }

    public List<String> getTopics() { return topics; }
    public void setTopics(List<String> topics) { this.topics = topics; }

    public long getTotalLag() { return totalLag; }
    public void setTotalLag(long totalLag) { this.totalLag = totalLag; }

    public int getPartitionCount() { return partitionCount; }
    public void setPartitionCount(int partitionCount) { this.partitionCount = partitionCount; }
}
