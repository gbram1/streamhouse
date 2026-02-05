package io.streamhouse.client.model;

import java.util.List;

public class ConsumerGroupDetail {
    private String groupId;
    private List<ConsumerOffset> offsets;

    public String getGroupId() { return groupId; }
    public void setGroupId(String groupId) { this.groupId = groupId; }

    public List<ConsumerOffset> getOffsets() { return offsets; }
    public void setOffsets(List<ConsumerOffset> offsets) { this.offsets = offsets; }

    public static class ConsumerOffset {
        private String topic;
        private int partitionId;
        private long committedOffset;
        private long highWatermark;
        private long lag;

        public String getTopic() { return topic; }
        public void setTopic(String topic) { this.topic = topic; }

        public int getPartitionId() { return partitionId; }
        public void setPartitionId(int partitionId) { this.partitionId = partitionId; }

        public long getCommittedOffset() { return committedOffset; }
        public void setCommittedOffset(long committedOffset) { this.committedOffset = committedOffset; }

        public long getHighWatermark() { return highWatermark; }
        public void setHighWatermark(long highWatermark) { this.highWatermark = highWatermark; }

        public long getLag() { return lag; }
        public void setLag(long lag) { this.lag = lag; }
    }
}
