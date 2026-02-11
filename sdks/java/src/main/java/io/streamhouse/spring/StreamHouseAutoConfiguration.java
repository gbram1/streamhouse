package io.streamhouse.spring;

import io.streamhouse.client.StreamHouseClient;
import io.streamhouse.client.exception.StreamHouseException;
import io.streamhouse.client.model.ProduceResult;
import io.streamhouse.client.model.ConsumeResult;
import io.streamhouse.client.model.BatchRecord;
import io.streamhouse.client.model.BatchProduceResult;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;
import java.lang.reflect.Method;
import java.time.Duration;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;
import java.util.logging.Level;
import java.util.logging.Logger;

/**
 * Spring Boot auto-configuration for StreamHouse.
 *
 * <p>Provides automatic bean creation and configuration for StreamHouse client
 * integration in Spring Boot applications. Includes:</p>
 * <ul>
 *   <li>{@link StreamHouseProperties} - Configuration binding from application.yml</li>
 *   <li>{@link StreamHouseClient} bean - Auto-configured client</li>
 *   <li>{@link StreamHouseTemplate} - Convenience wrapper for common operations</li>
 *   <li>{@link StreamHouseListener} - Annotation for consumer methods</li>
 *   <li>{@link StreamHouseHealthIndicator} - Spring Boot health indicator</li>
 * </ul>
 *
 * <h3>Quick Start</h3>
 *
 * <p>Add StreamHouse Spring Boot starter to your project and configure in application.yml:</p>
 * <pre>{@code
 * streamhouse:
 *   base-url: http://localhost:8080
 *   api-key: sk_live_xxx
 *   timeout: 30s
 * }</pre>
 *
 * <p>Then inject and use the client:</p>
 * <pre>{@code
 * @Service
 * public class EventService {
 *     @Autowired
 *     private StreamHouseTemplate streamhouse;
 *
 *     public void publishEvent(String event) {
 *         streamhouse.produce("events", event);
 *     }
 * }
 * }</pre>
 *
 * <p>Or use annotation-based consumers:</p>
 * <pre>{@code
 * @Component
 * public class EventConsumer {
 *     @StreamHouseListener(topic = "events", partition = 0)
 *     public void onEvent(String value) {
 *         System.out.println("Received: " + value);
 *     }
 * }
 * }</pre>
 *
 * <p><b>Note:</b> This class is designed as a Spring {@code @Configuration} class.
 * In a full Spring Boot starter, annotate with {@code @Configuration} and
 * {@code @EnableConfigurationProperties(StreamHouseProperties.class)}.
 * The annotations are omitted here to avoid requiring Spring dependencies at
 * compile time.</p>
 */
// @Configuration
// @EnableConfigurationProperties(StreamHouseProperties.class)
public class StreamHouseAutoConfiguration {

    private static final Logger logger = Logger.getLogger(StreamHouseAutoConfiguration.class.getName());

    // =========================================================================
    // Bean definitions (use @Bean annotation in Spring context)
    // =========================================================================

    /**
     * Create the StreamHouseClient bean.
     *
     * <p>In Spring Boot, annotate with {@code @Bean} and
     * {@code @ConditionalOnMissingBean}:</p>
     * <pre>{@code
     * @Bean
     * @ConditionalOnMissingBean
     * public StreamHouseClient streamHouseClient(StreamHouseProperties properties) {
     *     return createClient(properties);
     * }
     * }</pre>
     *
     * @param properties Configuration properties.
     * @return A configured StreamHouseClient.
     */
    public StreamHouseClient createClient(StreamHouseProperties properties) {
        StreamHouseClient.Builder builder = StreamHouseClient.builder()
                .baseUrl(properties.getBaseUrl())
                .timeout(properties.getTimeout());

        if (properties.getApiKey() != null && !properties.getApiKey().isEmpty()) {
            builder.apiKey(properties.getApiKey());
        }

        return builder.build();
    }

    /**
     * Create the StreamHouseTemplate bean.
     *
     * @param client The StreamHouseClient.
     * @return A StreamHouseTemplate wrapping the client.
     */
    public StreamHouseTemplate createTemplate(StreamHouseClient client) {
        return new StreamHouseTemplate(client);
    }

    /**
     * Create the health indicator bean.
     *
     * @param client The StreamHouseClient.
     * @return A StreamHouseHealthIndicator.
     */
    public StreamHouseHealthIndicator createHealthIndicator(StreamHouseClient client) {
        return new StreamHouseHealthIndicator(client);
    }

    /**
     * Create the listener bean post processor.
     *
     * @param client The StreamHouseClient.
     * @return A StreamHouseListenerBeanPostProcessor.
     */
    public StreamHouseListenerBeanPostProcessor createListenerProcessor(StreamHouseClient client) {
        return new StreamHouseListenerBeanPostProcessor(client);
    }

    // =========================================================================
    // StreamHouseProperties
    // =========================================================================

    /**
     * Configuration properties for StreamHouse, bindable from application.yml.
     *
     * <p>In Spring Boot, annotate with
     * {@code @ConfigurationProperties(prefix = "streamhouse")}:</p>
     *
     * <pre>{@code
     * streamhouse:
     *   base-url: http://localhost:8080
     *   api-key: sk_live_xxx
     *   timeout: 30s
     * }</pre>
     */
    // @ConfigurationProperties(prefix = "streamhouse")
    public static class StreamHouseProperties {

        /** Base URL of the StreamHouse server. */
        private String baseUrl = "http://localhost:8080";

        /** API key for authentication (optional). */
        private String apiKey;

        /** Request timeout. */
        private Duration timeout = Duration.ofSeconds(30);

        /** Whether to enable the health indicator. */
        private boolean healthEnabled = true;

        /** Whether to enable annotation-based listeners. */
        private boolean listenersEnabled = true;

        public String getBaseUrl() {
            return baseUrl;
        }

        public void setBaseUrl(String baseUrl) {
            this.baseUrl = baseUrl;
        }

        public String getApiKey() {
            return apiKey;
        }

        public void setApiKey(String apiKey) {
            this.apiKey = apiKey;
        }

        public Duration getTimeout() {
            return timeout;
        }

        public void setTimeout(Duration timeout) {
            this.timeout = timeout;
        }

        public boolean isHealthEnabled() {
            return healthEnabled;
        }

        public void setHealthEnabled(boolean healthEnabled) {
            this.healthEnabled = healthEnabled;
        }

        public boolean isListenersEnabled() {
            return listenersEnabled;
        }

        public void setListenersEnabled(boolean listenersEnabled) {
            this.listenersEnabled = listenersEnabled;
        }
    }

    // =========================================================================
    // @StreamHouseListener annotation
    // =========================================================================

    /**
     * Annotation for methods that should consume messages from StreamHouse topics.
     *
     * <p>Annotated methods are automatically discovered and registered as
     * background consumers. The method receives the message value as a String
     * parameter.</p>
     *
     * <pre>{@code
     * @Component
     * public class OrderConsumer {
     *
     *     @StreamHouseListener(topic = "orders", partition = 0, pollIntervalMs = 1000)
     *     public void onOrder(String value) {
     *         Order order = objectMapper.readValue(value, Order.class);
     *         processOrder(order);
     *     }
     * }
     * }</pre>
     */
    @Target(ElementType.METHOD)
    @Retention(RetentionPolicy.RUNTIME)
    public @interface StreamHouseListener {

        /** Topic to consume from. */
        String topic();

        /** Partition to consume from. Default: 0. */
        int partition() default 0;

        /** Consumer group ID for offset tracking (optional). */
        String groupId() default "";

        /** Polling interval in milliseconds. Default: 1000. */
        long pollIntervalMs() default 1000;

        /** Maximum records per poll. Default: 100. */
        int maxRecords() default 100;

        /** Starting offset. Default: 0 (beginning). */
        long startOffset() default 0;
    }

    // =========================================================================
    // StreamHouseListenerBeanPostProcessor
    // =========================================================================

    /**
     * Processes beans to find {@link StreamHouseListener} annotations and start
     * background consumer threads for each annotated method.
     *
     * <p>In a full Spring Boot starter, this would implement
     * {@code BeanPostProcessor} and be registered as a {@code @Bean}.</p>
     */
    public static class StreamHouseListenerBeanPostProcessor {

        private final StreamHouseClient client;
        private final ScheduledExecutorService executor;
        private final Map<String, ConsumerTask> activeTasks = new ConcurrentHashMap<>();

        public StreamHouseListenerBeanPostProcessor(StreamHouseClient client) {
            this.client = client;
            this.executor = Executors.newScheduledThreadPool(
                    Runtime.getRuntime().availableProcessors()
            );
        }

        /**
         * Process a bean to discover and register listener methods.
         *
         * <p>Call this for each Spring bean during post-processing, or invoke
         * manually for non-Spring usage.</p>
         *
         * @param bean The bean instance.
         * @param beanName The bean name.
         * @return The (unmodified) bean.
         */
        public Object postProcessAfterInitialization(Object bean, String beanName) {
            Class<?> targetClass = bean.getClass();

            for (Method method : targetClass.getDeclaredMethods()) {
                StreamHouseListener annotation = method.getAnnotation(StreamHouseListener.class);
                if (annotation == null) continue;

                // Validate method signature: must accept a single String parameter
                if (method.getParameterCount() != 1 || !method.getParameterTypes()[0].equals(String.class)) {
                    logger.warning(String.format(
                            "Method %s.%s annotated with @StreamHouseListener must accept a single String parameter. Skipping.",
                            targetClass.getSimpleName(), method.getName()
                    ));
                    continue;
                }

                method.setAccessible(true);

                String taskId = String.format("%s.%s[%s/%d]",
                        beanName, method.getName(), annotation.topic(), annotation.partition());

                ConsumerTask task = new ConsumerTask(
                        client, bean, method, annotation
                );

                activeTasks.put(taskId, task);

                executor.scheduleWithFixedDelay(
                        task,
                        0,
                        annotation.pollIntervalMs(),
                        TimeUnit.MILLISECONDS
                );

                logger.info(String.format(
                        "Registered StreamHouse listener: %s -> %s/%d",
                        taskId, annotation.topic(), annotation.partition()
                ));
            }

            return bean;
        }

        /**
         * Shut down all consumer tasks.
         */
        public void destroy() {
            executor.shutdown();
            try {
                if (!executor.awaitTermination(10, TimeUnit.SECONDS)) {
                    executor.shutdownNow();
                }
            } catch (InterruptedException e) {
                executor.shutdownNow();
                Thread.currentThread().interrupt();
            }
            logger.info("StreamHouse listener processor shut down.");
        }

        /**
         * Get the number of active listener tasks.
         */
        public int getActiveTaskCount() {
            return activeTasks.size();
        }

        /** Internal consumer task for a single annotated method. */
        private static class ConsumerTask implements Runnable {
            private final StreamHouseClient client;
            private final Object bean;
            private final Method method;
            private final StreamHouseListener config;
            private long currentOffset;

            ConsumerTask(StreamHouseClient client, Object bean, Method method, StreamHouseListener config) {
                this.client = client;
                this.bean = bean;
                this.method = method;
                this.config = config;
                this.currentOffset = config.startOffset();
            }

            @Override
            public void run() {
                try {
                    ConsumeResult result = client.consume(
                            config.topic(),
                            config.partition(),
                            currentOffset,
                            config.maxRecords()
                    );

                    for (var record : result.getRecords()) {
                        try {
                            method.invoke(bean, record.getValue());
                        } catch (Exception e) {
                            logger.log(Level.WARNING, String.format(
                                    "Error invoking listener %s.%s at offset %d: %s",
                                    bean.getClass().getSimpleName(),
                                    method.getName(),
                                    record.getOffset(),
                                    e.getMessage()
                            ), e);
                        }
                        currentOffset = record.getOffset() + 1;
                    }

                    // Commit offset if group ID is set
                    if (!config.groupId().isEmpty()) {
                        try {
                            client.commitOffset(
                                    config.groupId(),
                                    config.topic(),
                                    config.partition(),
                                    currentOffset
                            );
                        } catch (StreamHouseException e) {
                            logger.warning("Failed to commit offset: " + e.getMessage());
                        }
                    }
                } catch (Exception e) {
                    logger.log(Level.WARNING, String.format(
                            "Error polling %s/%d: %s",
                            config.topic(), config.partition(), e.getMessage()
                    ), e);
                }
            }
        }
    }

    // =========================================================================
    // StreamHouseHealthIndicator
    // =========================================================================

    /**
     * Spring Boot health indicator for StreamHouse connectivity.
     *
     * <p>Reports the health of the StreamHouse connection as part of the
     * application's {@code /actuator/health} endpoint. Implements the
     * Spring Boot {@code HealthIndicator} interface pattern.</p>
     *
     * <pre>{@code
     * // In application.yml:
     * management:
     *   health:
     *     streamhouse:
     *       enabled: true
     * }</pre>
     *
     * <p>Health response example:</p>
     * <pre>{@code
     * {
     *   "status": "UP",
     *   "details": {
     *     "streamhouse": {
     *       "status": "UP"
     *     }
     *   }
     * }
     * }</pre>
     */
    public static class StreamHouseHealthIndicator {

        private final StreamHouseClient client;

        public StreamHouseHealthIndicator(StreamHouseClient client) {
            this.client = client;
        }

        /**
         * Perform the health check.
         *
         * <p>In Spring Boot, this would implement
         * {@code HealthIndicator.health()} returning a {@code Health} object.</p>
         *
         * @return A map containing "status" ("UP" or "DOWN") and optional details.
         */
        public Map<String, Object> health() {
            try {
                boolean healthy = client.healthCheck();
                if (healthy) {
                    return Map.of(
                            "status", "UP",
                            "details", Map.of("connection", "ok")
                    );
                } else {
                    return Map.of(
                            "status", "DOWN",
                            "details", Map.of("connection", "unhealthy")
                    );
                }
            } catch (Exception e) {
                return Map.of(
                        "status", "DOWN",
                        "details", Map.of("error", e.getMessage())
                );
            }
        }

        /**
         * Check if StreamHouse is reachable.
         *
         * @return true if health check passes.
         */
        public boolean isHealthy() {
            return client.healthCheck();
        }
    }

    // =========================================================================
    // StreamHouseTemplate
    // =========================================================================

    /**
     * Convenience wrapper around {@link StreamHouseClient} for common operations.
     *
     * <p>Provides simplified methods that handle exceptions and reduce boilerplate.
     * Designed to follow the Spring {@code *Template} pattern (similar to
     * {@code JdbcTemplate}, {@code RedisTemplate}, etc.).</p>
     *
     * <pre>{@code
     * @Service
     * public class EventService {
     *
     *     @Autowired
     *     private StreamHouseTemplate streamhouse;
     *
     *     public long publishEvent(String topic, String event) {
     *         ProduceResult result = streamhouse.produce(topic, event);
     *         return result.getOffset();
     *     }
     *
     *     public List<ConsumedRecord> getLatest(String topic, int partition) {
     *         ConsumeResult result = streamhouse.consume(topic, partition, 0, 10);
     *         return result.getRecords();
     *     }
     * }
     * }</pre>
     */
    public static class StreamHouseTemplate {

        private final StreamHouseClient client;

        public StreamHouseTemplate(StreamHouseClient client) {
            this.client = client;
        }

        /**
         * Get the underlying client for advanced operations.
         *
         * @return The StreamHouseClient instance.
         */
        public StreamHouseClient getClient() {
            return client;
        }

        /**
         * Produce a message to a topic.
         *
         * @param topic Topic name.
         * @param value Message value.
         * @return The produce result with offset and partition.
         * @throws StreamHouseException if the produce fails.
         */
        public ProduceResult produce(String topic, String value) throws StreamHouseException {
            return client.produce(topic, value);
        }

        /**
         * Produce a message with a key.
         *
         * @param topic Topic name.
         * @param key Message key.
         * @param value Message value.
         * @return The produce result.
         * @throws StreamHouseException if the produce fails.
         */
        public ProduceResult produce(String topic, String key, String value) throws StreamHouseException {
            return client.produce(topic, value, key);
        }

        /**
         * Produce a batch of messages.
         *
         * @param topic Topic name.
         * @param records List of batch records.
         * @return The batch produce result.
         * @throws StreamHouseException if the produce fails.
         */
        public BatchProduceResult produceBatch(String topic, List<BatchRecord> records)
                throws StreamHouseException {
            return client.produceBatch(topic, records);
        }

        /**
         * Consume messages from a partition.
         *
         * @param topic Topic name.
         * @param partition Partition ID.
         * @param offset Starting offset.
         * @param maxRecords Maximum records to return.
         * @return The consume result.
         * @throws StreamHouseException if the consume fails.
         */
        public ConsumeResult consume(String topic, int partition, long offset, int maxRecords)
                throws StreamHouseException {
            return client.consume(topic, partition, offset, maxRecords);
        }

        /**
         * Commit a consumer group offset.
         *
         * @param groupId Consumer group ID.
         * @param topic Topic name.
         * @param partition Partition ID.
         * @param offset Offset to commit.
         * @return true if successful.
         * @throws StreamHouseException if the commit fails.
         */
        public boolean commitOffset(String groupId, String topic, int partition, long offset)
                throws StreamHouseException {
            return client.commitOffset(groupId, topic, partition, offset);
        }

        /**
         * Check if StreamHouse is healthy.
         *
         * @return true if health check passes.
         */
        public boolean isHealthy() {
            return client.healthCheck();
        }
    }
}
