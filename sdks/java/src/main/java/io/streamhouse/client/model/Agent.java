package io.streamhouse.client.model;

public class Agent {
    private String agentId;
    private String address;
    private String availabilityZone;
    private String agentGroup;
    private long lastHeartbeat;
    private long startedAt;
    private int activeLeases;

    public String getAgentId() { return agentId; }
    public void setAgentId(String agentId) { this.agentId = agentId; }

    public String getAddress() { return address; }
    public void setAddress(String address) { this.address = address; }

    public String getAvailabilityZone() { return availabilityZone; }
    public void setAvailabilityZone(String availabilityZone) { this.availabilityZone = availabilityZone; }

    public String getAgentGroup() { return agentGroup; }
    public void setAgentGroup(String agentGroup) { this.agentGroup = agentGroup; }

    public long getLastHeartbeat() { return lastHeartbeat; }
    public void setLastHeartbeat(long lastHeartbeat) { this.lastHeartbeat = lastHeartbeat; }

    public long getStartedAt() { return startedAt; }
    public void setStartedAt(long startedAt) { this.startedAt = startedAt; }

    public int getActiveLeases() { return activeLeases; }
    public void setActiveLeases(int activeLeases) { this.activeLeases = activeLeases; }
}
