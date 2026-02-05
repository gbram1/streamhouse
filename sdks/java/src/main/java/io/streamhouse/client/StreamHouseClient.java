package io.streamhouse.client;

import java.io.IOException;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.time.Duration;
import java.util.List;
import java.util.Map;
import java.util.Optional;

import com.fasterxml.jackson.core.type.TypeReference;
import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.PropertyNamingStrategies;

import io.streamhouse.client.model.*;
import io.streamhouse.client.exception.*;

/**
 * Client for interacting with StreamHouse streaming data platform.
 *
 * <p>Example usage:</p>
 * <pre>{@code
 * StreamHouseClient client = StreamHouseClient.builder()
 *     .baseUrl("http://localhost:8080")
 *     .build();
 *
 * // Create a topic
 * Topic topic = client.createTopic("events", 3);
 *
 * // Produce a message
 * ProduceResult result = client.produce("events", "{\"event\": \"click\"}");
 *
 * // Consume messages
 * ConsumeResult result = client.consume("events", 0);
 * }</pre>
 */
public class StreamHouseClient implements AutoCloseable {
    private final String baseUrl;
    private final String apiUrl;
    private final String apiKey;
    private final HttpClient httpClient;
    private final ObjectMapper objectMapper;
    private final Duration timeout;

    private StreamHouseClient(Builder builder) {
        this.baseUrl = builder.baseUrl.replaceAll("/$", "");
        this.apiUrl = this.baseUrl + "/api/v1";
        this.apiKey = builder.apiKey;
        this.timeout = builder.timeout;

        this.httpClient = HttpClient.newBuilder()
            .connectTimeout(timeout)
            .build();

        this.objectMapper = new ObjectMapper()
            .setPropertyNamingStrategy(PropertyNamingStrategies.SNAKE_CASE)
            .configure(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES, false);
    }

    /**
     * Create a new builder for StreamHouseClient.
     */
    public static Builder builder() {
        return new Builder();
    }

    /**
     * Builder for StreamHouseClient.
     */
    public static class Builder {
        private String baseUrl;
        private String apiKey;
        private Duration timeout = Duration.ofSeconds(30);

        /**
         * Set the base URL of the StreamHouse server.
         */
        public Builder baseUrl(String baseUrl) {
            this.baseUrl = baseUrl;
            return this;
        }

        /**
         * Set the API key for authentication.
         */
        public Builder apiKey(String apiKey) {
            this.apiKey = apiKey;
            return this;
        }

        /**
         * Set the request timeout.
         */
        public Builder timeout(Duration timeout) {
            this.timeout = timeout;
            return this;
        }

        /**
         * Build the client.
         */
        public StreamHouseClient build() {
            if (baseUrl == null || baseUrl.isEmpty()) {
                throw new IllegalArgumentException("baseUrl is required");
            }
            return new StreamHouseClient(this);
        }
    }

    private HttpRequest.Builder requestBuilder(String path) {
        HttpRequest.Builder builder = HttpRequest.newBuilder()
            .uri(URI.create(apiUrl + path))
            .timeout(timeout)
            .header("Content-Type", "application/json");

        if (apiKey != null && !apiKey.isEmpty()) {
            builder.header("Authorization", "Bearer " + apiKey);
        }

        return builder;
    }

    private <T> T request(String method, String path, Object body, TypeReference<T> typeRef)
            throws StreamHouseException {
        try {
            HttpRequest.Builder builder = requestBuilder(path);

            if (body != null) {
                String jsonBody = objectMapper.writeValueAsString(body);
                builder.method(method, HttpRequest.BodyPublishers.ofString(jsonBody));
            } else {
                builder.method(method, HttpRequest.BodyPublishers.noBody());
            }

            HttpResponse<String> response = httpClient.send(
                builder.build(),
                HttpResponse.BodyHandlers.ofString()
            );

            if (response.statusCode() >= 400) {
                handleError(response.statusCode(), response.body());
            }

            if (response.statusCode() == 204 || response.body().isEmpty()) {
                return null;
            }

            return objectMapper.readValue(response.body(), typeRef);
        } catch (IOException | InterruptedException e) {
            throw new ConnectionException("Connection failed: " + e.getMessage(), e);
        }
    }

    private void handleError(int statusCode, String body) throws StreamHouseException {
        String message = body;
        try {
            Map<String, Object> errorData = objectMapper.readValue(body, new TypeReference<>() {});
            if (errorData.containsKey("error")) {
                message = (String) errorData.get("error");
            }
        } catch (Exception ignored) {}

        switch (statusCode) {
            case 400:
                throw new ValidationException(message);
            case 401:
                throw new AuthenticationException(message);
            case 404:
                throw new NotFoundException(message);
            case 408:
                throw new TimeoutException(message);
            case 409:
                throw new ConflictException(message);
            default:
                if (statusCode >= 500) {
                    throw new ServerException(message, statusCode);
                }
                throw new StreamHouseException(message, statusCode);
        }
    }

    // =========================================================================
    // Topic Operations
    // =========================================================================

    /**
     * List all topics.
     */
    public List<Topic> listTopics() throws StreamHouseException {
        return request("GET", "/topics", null, new TypeReference<>() {});
    }

    /**
     * Create a new topic.
     */
    public Topic createTopic(String name, int partitions) throws StreamHouseException {
        return createTopic(name, partitions, 1);
    }

    /**
     * Create a new topic with replication factor.
     */
    public Topic createTopic(String name, int partitions, int replicationFactor)
            throws StreamHouseException {
        Map<String, Object> body = Map.of(
            "name", name,
            "partitions", partitions,
            "replication_factor", replicationFactor
        );
        return request("POST", "/topics", body, new TypeReference<>() {});
    }

    /**
     * Get topic details.
     */
    public Topic getTopic(String name) throws StreamHouseException {
        return request("GET", "/topics/" + name, null, new TypeReference<>() {});
    }

    /**
     * Delete a topic.
     */
    public void deleteTopic(String name) throws StreamHouseException {
        request("DELETE", "/topics/" + name, null, new TypeReference<Void>() {});
    }

    /**
     * List partitions for a topic.
     */
    public List<Partition> listPartitions(String topic) throws StreamHouseException {
        return request("GET", "/topics/" + topic + "/partitions", null, new TypeReference<>() {});
    }

    // =========================================================================
    // Producer Operations
    // =========================================================================

    /**
     * Produce a single message.
     */
    public ProduceResult produce(String topic, String value) throws StreamHouseException {
        return produce(topic, value, null, null);
    }

    /**
     * Produce a single message with key.
     */
    public ProduceResult produce(String topic, String value, String key) throws StreamHouseException {
        return produce(topic, value, key, null);
    }

    /**
     * Produce a single message with key and partition.
     */
    public ProduceResult produce(String topic, String value, String key, Integer partition)
            throws StreamHouseException {
        Map<String, Object> body = new java.util.HashMap<>();
        body.put("topic", topic);
        body.put("value", value);
        if (key != null) body.put("key", key);
        if (partition != null) body.put("partition", partition);

        return request("POST", "/produce", body, new TypeReference<>() {});
    }

    /**
     * Produce a batch of messages.
     */
    public BatchProduceResult produceBatch(String topic, List<BatchRecord> records)
            throws StreamHouseException {
        Map<String, Object> body = Map.of(
            "topic", topic,
            "records", records
        );
        return request("POST", "/produce/batch", body, new TypeReference<>() {});
    }

    // =========================================================================
    // Consumer Operations
    // =========================================================================

    /**
     * Consume messages from a partition.
     */
    public ConsumeResult consume(String topic, int partition) throws StreamHouseException {
        return consume(topic, partition, 0, 100);
    }

    /**
     * Consume messages from a partition with offset and limit.
     */
    public ConsumeResult consume(String topic, int partition, long offset, int maxRecords)
            throws StreamHouseException {
        String path = String.format("/consume?topic=%s&partition=%d&offset=%d&maxRecords=%d",
            topic, partition, offset, maxRecords);
        return request("GET", path, null, new TypeReference<>() {});
    }

    // =========================================================================
    // Consumer Group Operations
    // =========================================================================

    /**
     * List all consumer groups.
     */
    public List<ConsumerGroup> listConsumerGroups() throws StreamHouseException {
        return request("GET", "/consumer-groups", null, new TypeReference<>() {});
    }

    /**
     * Get consumer group details.
     */
    public ConsumerGroupDetail getConsumerGroup(String groupId) throws StreamHouseException {
        return request("GET", "/consumer-groups/" + groupId, null, new TypeReference<>() {});
    }

    /**
     * Commit a consumer offset.
     */
    public boolean commitOffset(String groupId, String topic, int partition, long offset)
            throws StreamHouseException {
        Map<String, Object> body = Map.of(
            "group_id", groupId,
            "topic", topic,
            "partition", partition,
            "offset", offset
        );
        Map<String, Object> result = request("POST", "/consumer-groups/commit", body,
            new TypeReference<>() {});
        return result != null && Boolean.TRUE.equals(result.get("success"));
    }

    /**
     * Delete a consumer group.
     */
    public void deleteConsumerGroup(String groupId) throws StreamHouseException {
        request("DELETE", "/consumer-groups/" + groupId, null, new TypeReference<Void>() {});
    }

    // =========================================================================
    // SQL Operations
    // =========================================================================

    /**
     * Execute a SQL query.
     */
    public SqlResult query(String sql) throws StreamHouseException {
        return query(sql, null);
    }

    /**
     * Execute a SQL query with timeout.
     */
    public SqlResult query(String sql, Long timeoutMs) throws StreamHouseException {
        Map<String, Object> body = new java.util.HashMap<>();
        body.put("query", sql);
        if (timeoutMs != null) body.put("timeout_ms", timeoutMs);

        return request("POST", "/sql", body, new TypeReference<>() {});
    }

    // =========================================================================
    // Agent/Cluster Operations
    // =========================================================================

    /**
     * List all agents.
     */
    public List<Agent> listAgents() throws StreamHouseException {
        return request("GET", "/agents", null, new TypeReference<>() {});
    }

    /**
     * Get cluster metrics.
     */
    public MetricsSnapshot getMetrics() throws StreamHouseException {
        return request("GET", "/metrics", null, new TypeReference<>() {});
    }

    // =========================================================================
    // Health Check
    // =========================================================================

    /**
     * Check if the server is healthy.
     */
    public boolean healthCheck() {
        try {
            HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(baseUrl + "/health"))
                .timeout(timeout)
                .GET()
                .build();

            HttpResponse<String> response = httpClient.send(
                request,
                HttpResponse.BodyHandlers.ofString()
            );

            return response.statusCode() == 200;
        } catch (Exception e) {
            return false;
        }
    }

    @Override
    public void close() {
        // HttpClient doesn't need explicit closing in Java 11+
    }
}
