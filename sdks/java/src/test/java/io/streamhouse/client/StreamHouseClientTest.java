package io.streamhouse.client;

import static org.junit.jupiter.api.Assertions.*;

import java.io.IOException;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Nested;
import org.junit.jupiter.api.Test;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.PropertyNamingStrategies;

import io.streamhouse.client.exception.*;
import io.streamhouse.client.model.*;

import okhttp3.mockwebserver.MockResponse;
import okhttp3.mockwebserver.MockWebServer;
import okhttp3.mockwebserver.RecordedRequest;

class StreamHouseClientTest {

    private MockWebServer mockServer;
    private StreamHouseClient client;
    private ObjectMapper objectMapper;

    @BeforeEach
    void setUp() throws IOException {
        mockServer = new MockWebServer();
        mockServer.start();

        client = StreamHouseClient.builder()
            .baseUrl(mockServer.url("/").toString())
            .build();

        objectMapper = new ObjectMapper()
            .setPropertyNamingStrategy(PropertyNamingStrategies.SNAKE_CASE);
    }

    @AfterEach
    void tearDown() throws IOException {
        mockServer.shutdown();
    }

    private void enqueueJson(int statusCode, Object body) throws Exception {
        String json = body != null ? objectMapper.writeValueAsString(body) : "";
        mockServer.enqueue(new MockResponse()
            .setResponseCode(statusCode)
            .setHeader("Content-Type", "application/json")
            .setBody(json));
    }

    @Nested
    @DisplayName("Client Initialization")
    class ClientInitializationTests {

        @Test
        void shouldCreateClientWithBaseUrl() {
            StreamHouseClient c = StreamHouseClient.builder()
                .baseUrl("http://localhost:8080")
                .build();
            assertNotNull(c);
        }

        @Test
        void shouldStripTrailingSlash() throws Exception {
            enqueueJson(200, List.of());

            StreamHouseClient c = StreamHouseClient.builder()
                .baseUrl(mockServer.url("/").toString() + "/")
                .build();
            c.listTopics();

            RecordedRequest request = mockServer.takeRequest();
            assertTrue(request.getPath().startsWith("/api/v1"));
        }

        @Test
        void shouldThrowWhenBaseUrlMissing() {
            assertThrows(IllegalArgumentException.class, () ->
                StreamHouseClient.builder().build());
        }
    }

    @Nested
    @DisplayName("Topic Operations")
    class TopicOperationTests {

        @Test
        void shouldListTopics() throws Exception {
            List<Map<String, Object>> topics = List.of(
                Map.of("name", "topic1", "partitions", 3, "replication_factor", 1),
                Map.of("name", "topic2", "partitions", 6, "replication_factor", 2)
            );
            enqueueJson(200, topics);

            List<Topic> result = client.listTopics();

            assertEquals(2, result.size());
            assertEquals("topic1", result.get(0).getName());
            assertEquals(3, result.get(0).getPartitions());
            assertEquals("topic2", result.get(1).getName());
        }

        @Test
        void shouldCreateTopic() throws Exception {
            Map<String, Object> topic = Map.of(
                "name", "my-topic",
                "partitions", 3,
                "replication_factor", 1,
                "created_at", "2024-01-01"
            );
            enqueueJson(201, topic);

            Topic result = client.createTopic("my-topic", 3);

            assertEquals("my-topic", result.getName());
            assertEquals(3, result.getPartitions());

            RecordedRequest request = mockServer.takeRequest();
            assertEquals("POST", request.getMethod());
            assertTrue(request.getPath().endsWith("/topics"));
        }

        @Test
        void shouldGetTopic() throws Exception {
            Map<String, Object> topic = Map.of("name", "my-topic", "partitions", 3);
            enqueueJson(200, topic);

            Topic result = client.getTopic("my-topic");

            assertEquals("my-topic", result.getName());

            RecordedRequest request = mockServer.takeRequest();
            assertEquals("GET", request.getMethod());
            assertTrue(request.getPath().contains("/topics/my-topic"));
        }

        @Test
        void shouldDeleteTopic() throws Exception {
            enqueueJson(204, null);

            client.deleteTopic("my-topic");

            RecordedRequest request = mockServer.takeRequest();
            assertEquals("DELETE", request.getMethod());
            assertTrue(request.getPath().contains("/topics/my-topic"));
        }

        @Test
        void shouldListPartitions() throws Exception {
            List<Map<String, Object>> partitions = List.of(
                Map.of("topic", "events", "partition_id", 0, "high_watermark", 100),
                Map.of("topic", "events", "partition_id", 1, "high_watermark", 200)
            );
            enqueueJson(200, partitions);

            List<Partition> result = client.listPartitions("events");

            assertEquals(2, result.size());
        }
    }

    @Nested
    @DisplayName("Producer Operations")
    class ProducerOperationTests {

        @Test
        void shouldProduceMessage() throws Exception {
            Map<String, Object> result = Map.of("offset", 42, "partition", 0);
            enqueueJson(200, result);

            ProduceResult produceResult = client.produce("events", "{\"event\": \"click\"}");

            assertEquals(42, produceResult.getOffset());
            assertEquals(0, produceResult.getPartition());
        }

        @Test
        void shouldProduceMessageWithKey() throws Exception {
            Map<String, Object> result = Map.of("offset", 42, "partition", 0);
            enqueueJson(200, result);

            client.produce("events", "{\"event\": \"click\"}", "user-123");

            RecordedRequest request = mockServer.takeRequest();
            String body = request.getBody().readUtf8();
            assertTrue(body.contains("user-123"));
        }

        @Test
        void shouldProduceBatch() throws Exception {
            Map<String, Object> result = Map.of(
                "count", 2,
                "offsets", List.of(
                    Map.of("partition", 0, "offset", 10),
                    Map.of("partition", 0, "offset", 11)
                )
            );
            enqueueJson(200, result);

            List<BatchRecord> records = List.of(
                new BatchRecord("{\"event\": \"a\"}", null, null),
                new BatchRecord("{\"event\": \"b\"}", "user-1", null)
            );
            BatchProduceResult batchResult = client.produceBatch("events", records);

            assertEquals(2, batchResult.getCount());
            assertEquals(2, batchResult.getOffsets().size());
        }
    }

    @Nested
    @DisplayName("Consumer Operations")
    class ConsumerOperationTests {

        @Test
        void shouldConsumeMessages() throws Exception {
            Map<String, Object> result = Map.of(
                "records", List.of(
                    Map.of("partition", 0, "offset", 0, "key", null, "value", "{\"event\": \"a\"}", "timestamp", 1000),
                    Map.of("partition", 0, "offset", 1, "key", "k1", "value", "{\"event\": \"b\"}", "timestamp", 1001)
                ),
                "next_offset", 2
            );
            enqueueJson(200, result);

            ConsumeResult consumeResult = client.consume("events", 0);

            assertEquals(2, consumeResult.getRecords().size());
            assertEquals(2, consumeResult.getNextOffset());
            assertEquals("{\"event\": \"a\"}", consumeResult.getRecords().get(0).getValue());
        }

        @Test
        void shouldConsumeWithOffsetAndLimit() throws Exception {
            Map<String, Object> result = Map.of("records", List.of(), "next_offset", 0);
            enqueueJson(200, result);

            client.consume("events", 0, 10, 100);

            RecordedRequest request = mockServer.takeRequest();
            assertTrue(request.getPath().contains("offset=10"));
            assertTrue(request.getPath().contains("maxRecords=100"));
        }
    }

    @Nested
    @DisplayName("Consumer Group Operations")
    class ConsumerGroupOperationTests {

        @Test
        void shouldListConsumerGroups() throws Exception {
            List<Map<String, Object>> groups = List.of(
                Map.of("group_id", "group1", "topics", List.of("events"), "total_lag", 0),
                Map.of("group_id", "group2", "topics", List.of("logs"), "total_lag", 10)
            );
            enqueueJson(200, groups);

            List<ConsumerGroup> result = client.listConsumerGroups();

            assertEquals(2, result.size());
        }

        @Test
        void shouldGetConsumerGroup() throws Exception {
            Map<String, Object> group = Map.of(
                "group_id", "my-group",
                "offsets", List.of(
                    Map.of("topic", "events", "partition_id", 0, "committed_offset", 50)
                )
            );
            enqueueJson(200, group);

            ConsumerGroupDetail result = client.getConsumerGroup("my-group");

            assertEquals("my-group", result.getGroupId());
            assertEquals(1, result.getOffsets().size());
        }

        @Test
        void shouldCommitOffset() throws Exception {
            enqueueJson(200, Map.of("success", true));

            boolean result = client.commitOffset("my-group", "events", 0, 42);

            assertTrue(result);
        }

        @Test
        void shouldDeleteConsumerGroup() throws Exception {
            enqueueJson(204, null);

            client.deleteConsumerGroup("my-group");

            RecordedRequest request = mockServer.takeRequest();
            assertEquals("DELETE", request.getMethod());
            assertTrue(request.getPath().contains("/consumer-groups/my-group"));
        }
    }

    @Nested
    @DisplayName("SQL Operations")
    class SqlOperationTests {

        @Test
        void shouldExecuteSqlQuery() throws Exception {
            Map<String, Object> result = Map.of(
                "columns", List.of(
                    Map.of("name", "offset", "data_type", "bigint"),
                    Map.of("name", "value", "data_type", "string")
                ),
                "rows", List.of(List.of(0, "test")),
                "row_count", 1,
                "execution_time_ms", 50,
                "truncated", false
            );
            enqueueJson(200, result);

            SqlResult queryResult = client.query("SELECT * FROM events");

            assertEquals(1, queryResult.getRowCount());
            assertEquals(2, queryResult.getColumns().size());
            assertEquals("offset", queryResult.getColumns().get(0).getName());
        }

        @Test
        void shouldPassTimeoutToQuery() throws Exception {
            enqueueJson(200, Map.of("columns", List.of(), "rows", List.of(), "row_count", 0));

            client.query("SELECT * FROM events", 5000L);

            RecordedRequest request = mockServer.takeRequest();
            String body = request.getBody().readUtf8();
            assertTrue(body.contains("5000"));
        }
    }

    @Nested
    @DisplayName("Error Handling")
    class ErrorHandlingTests {

        @Test
        void shouldThrowNotFoundExceptionFor404() throws Exception {
            enqueueJson(404, Map.of("error", "topic not found"));

            NotFoundException ex = assertThrows(NotFoundException.class,
                () -> client.getTopic("nonexistent"));

            assertEquals(404, ex.getStatusCode());
            assertTrue(ex.getMessage().contains("topic not found"));
        }

        @Test
        void shouldThrowValidationExceptionFor400() throws Exception {
            enqueueJson(400, Map.of("error", "invalid partitions"));

            ValidationException ex = assertThrows(ValidationException.class,
                () -> client.createTopic("test", 0));

            assertEquals(400, ex.getStatusCode());
        }

        @Test
        void shouldThrowConflictExceptionFor409() throws Exception {
            enqueueJson(409, Map.of("error", "topic already exists"));

            ConflictException ex = assertThrows(ConflictException.class,
                () -> client.createTopic("existing", 3));

            assertEquals(409, ex.getStatusCode());
        }

        @Test
        void shouldThrowAuthenticationExceptionFor401() throws Exception {
            enqueueJson(401, Map.of("error", "unauthorized"));

            assertThrows(AuthenticationException.class, () -> client.listTopics());
        }

        @Test
        void shouldThrowServerExceptionFor500() throws Exception {
            enqueueJson(500, Map.of("error", "internal server error"));

            ServerException ex = assertThrows(ServerException.class,
                () -> client.listTopics());

            assertEquals(500, ex.getStatusCode());
        }
    }

    @Nested
    @DisplayName("Authentication")
    class AuthenticationTests {

        @Test
        void shouldIncludeApiKeyInHeader() throws Exception {
            client = StreamHouseClient.builder()
                .baseUrl(mockServer.url("/").toString())
                .apiKey("test-api-key")
                .build();

            enqueueJson(200, List.of());

            client.listTopics();

            RecordedRequest request = mockServer.takeRequest();
            assertEquals("Bearer test-api-key", request.getHeader("Authorization"));
        }

        @Test
        void shouldNotIncludeAuthorizationHeaderWithoutApiKey() throws Exception {
            enqueueJson(200, List.of());

            client.listTopics();

            RecordedRequest request = mockServer.takeRequest();
            assertNull(request.getHeader("Authorization"));
        }
    }

    @Nested
    @DisplayName("Health Check")
    class HealthCheckTests {

        @Test
        void shouldReturnTrueWhenHealthy() throws Exception {
            mockServer.enqueue(new MockResponse().setResponseCode(200));

            assertTrue(client.healthCheck());
        }

        @Test
        void shouldReturnFalseWhenUnhealthy() throws Exception {
            mockServer.enqueue(new MockResponse().setResponseCode(503));

            assertFalse(client.healthCheck());
        }
    }

    @Nested
    @DisplayName("Agent Operations")
    class AgentOperationTests {

        @Test
        void shouldListAgents() throws Exception {
            List<Map<String, Object>> agents = List.of(
                Map.of("agent_id", "agent-1", "address", "10.0.0.1:8080"),
                Map.of("agent_id", "agent-2", "address", "10.0.0.2:8080")
            );
            enqueueJson(200, agents);

            List<Agent> result = client.listAgents();

            assertEquals(2, result.size());
        }

        @Test
        void shouldGetMetrics() throws Exception {
            Map<String, Object> metrics = Map.of(
                "topics_count", 5,
                "agents_count", 3,
                "partitions_count", 15,
                "total_messages", 10000
            );
            enqueueJson(200, metrics);

            MetricsSnapshot result = client.getMetrics();

            assertEquals(5, result.getTopicsCount());
            assertEquals(10000, result.getTotalMessages());
        }
    }
}
