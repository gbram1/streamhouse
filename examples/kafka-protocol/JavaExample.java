/**
 * StreamHouse Kafka Protocol - Java Examples
 *
 * Requirements (Maven):
 *   <dependency>
 *     <groupId>org.apache.kafka</groupId>
 *     <artifactId>kafka-clients</artifactId>
 *     <version>3.6.0</version>
 *   </dependency>
 *
 * Usage:
 *   # Start StreamHouse first:
 *   USE_LOCAL_STORAGE=1 ./target/release/unified-server
 *
 *   # Compile and run:
 *   javac -cp kafka-clients-3.6.0.jar JavaExample.java
 *   java -cp .:kafka-clients-3.6.0.jar JavaExample <example>
 */

import org.apache.kafka.clients.admin.*;
import org.apache.kafka.clients.consumer.*;
import org.apache.kafka.clients.producer.*;
import org.apache.kafka.common.serialization.*;
import org.apache.kafka.common.TopicPartition;
import org.apache.kafka.common.header.Header;
import org.apache.kafka.common.header.internals.RecordHeader;

import java.time.Duration;
import java.util.*;
import java.util.concurrent.ExecutionException;

public class JavaExample {

    private static final String BOOTSTRAP_SERVERS = "localhost:9092";

    // =========================================================================
    // BASIC PRODUCER
    // =========================================================================

    public static void basicProducer() throws Exception {
        Properties props = new Properties();
        props.put(ProducerConfig.BOOTSTRAP_SERVERS_CONFIG, BOOTSTRAP_SERVERS);
        props.put(ProducerConfig.KEY_SERIALIZER_CLASS_CONFIG, StringSerializer.class.getName());
        props.put(ProducerConfig.VALUE_SERIALIZER_CLASS_CONFIG, StringSerializer.class.getName());

        try (Producer<String, String> producer = new KafkaProducer<>(props)) {
            ProducerRecord<String, String> record = new ProducerRecord<>(
                "java-topic",
                "key1",
                "Hello from Java!"
            );

            RecordMetadata metadata = producer.send(record).get();
            System.out.printf("Sent to partition %d at offset %d%n",
                metadata.partition(), metadata.offset());
        }
    }

    // =========================================================================
    // PRODUCER WITH HEADERS
    // =========================================================================

    public static void producerWithHeaders() throws Exception {
        Properties props = new Properties();
        props.put(ProducerConfig.BOOTSTRAP_SERVERS_CONFIG, BOOTSTRAP_SERVERS);
        props.put(ProducerConfig.KEY_SERIALIZER_CLASS_CONFIG, StringSerializer.class.getName());
        props.put(ProducerConfig.VALUE_SERIALIZER_CLASS_CONFIG, StringSerializer.class.getName());

        try (Producer<String, String> producer = new KafkaProducer<>(props)) {
            List<Header> headers = Arrays.asList(
                new RecordHeader("content-type", "application/json".getBytes()),
                new RecordHeader("source", "java-app".getBytes())
            );

            ProducerRecord<String, String> record = new ProducerRecord<>(
                "headers-topic",
                null,  // partition
                "user-123",  // key
                "{\"action\": \"login\", \"user_id\": 123}",  // value
                headers
            );

            producer.send(record).get();
            System.out.println("Sent message with headers");
        }
    }

    // =========================================================================
    // BATCH PRODUCER
    // =========================================================================

    public static void batchProducer() throws Exception {
        Properties props = new Properties();
        props.put(ProducerConfig.BOOTSTRAP_SERVERS_CONFIG, BOOTSTRAP_SERVERS);
        props.put(ProducerConfig.KEY_SERIALIZER_CLASS_CONFIG, StringSerializer.class.getName());
        props.put(ProducerConfig.VALUE_SERIALIZER_CLASS_CONFIG, StringSerializer.class.getName());
        props.put(ProducerConfig.BATCH_SIZE_CONFIG, 32768);
        props.put(ProducerConfig.LINGER_MS_CONFIG, 100);
        props.put(ProducerConfig.COMPRESSION_TYPE_CONFIG, "lz4");

        try (Producer<String, String> producer = new KafkaProducer<>(props)) {
            for (int i = 0; i < 100; i++) {
                producer.send(new ProducerRecord<>(
                    "batch-java-topic",
                    "key-" + i,
                    "Message " + i
                ));
            }
            producer.flush();
            System.out.println("Sent 100 messages in batch");
        }
    }

    // =========================================================================
    // ASYNC PRODUCER WITH CALLBACKS
    // =========================================================================

    public static void asyncProducer() throws Exception {
        Properties props = new Properties();
        props.put(ProducerConfig.BOOTSTRAP_SERVERS_CONFIG, BOOTSTRAP_SERVERS);
        props.put(ProducerConfig.KEY_SERIALIZER_CLASS_CONFIG, StringSerializer.class.getName());
        props.put(ProducerConfig.VALUE_SERIALIZER_CLASS_CONFIG, StringSerializer.class.getName());

        try (Producer<String, String> producer = new KafkaProducer<>(props)) {
            for (int i = 0; i < 5; i++) {
                final int index = i;
                producer.send(
                    new ProducerRecord<>("callback-topic", "key-" + i, "Message " + i),
                    (metadata, exception) -> {
                        if (exception != null) {
                            System.err.println("Error sending message " + index + ": " + exception.getMessage());
                        } else {
                            System.out.printf("Message %d sent to partition %d @ offset %d%n",
                                index, metadata.partition(), metadata.offset());
                        }
                    }
                );
            }
            producer.flush();
        }
    }

    // =========================================================================
    // BASIC CONSUMER
    // =========================================================================

    public static void basicConsumer() {
        Properties props = new Properties();
        props.put(ConsumerConfig.BOOTSTRAP_SERVERS_CONFIG, BOOTSTRAP_SERVERS);
        props.put(ConsumerConfig.GROUP_ID_CONFIG, "java-consumer-group");
        props.put(ConsumerConfig.KEY_DESERIALIZER_CLASS_CONFIG, StringDeserializer.class.getName());
        props.put(ConsumerConfig.VALUE_DESERIALIZER_CLASS_CONFIG, StringDeserializer.class.getName());
        props.put(ConsumerConfig.AUTO_OFFSET_RESET_CONFIG, "earliest");

        try (Consumer<String, String> consumer = new KafkaConsumer<>(props)) {
            consumer.subscribe(Collections.singletonList("java-topic"));

            System.out.println("Consuming messages (10s timeout)...");
            long endTime = System.currentTimeMillis() + 10000;

            while (System.currentTimeMillis() < endTime) {
                ConsumerRecords<String, String> records = consumer.poll(Duration.ofMillis(1000));
                for (ConsumerRecord<String, String> record : records) {
                    System.out.printf("  Offset %d: key=%s, value=%s%n",
                        record.offset(), record.key(), record.value());
                }
            }
        }
    }

    // =========================================================================
    // CONSUMER WITH MANUAL COMMITS
    // =========================================================================

    public static void manualCommitConsumer() {
        Properties props = new Properties();
        props.put(ConsumerConfig.BOOTSTRAP_SERVERS_CONFIG, BOOTSTRAP_SERVERS);
        props.put(ConsumerConfig.GROUP_ID_CONFIG, "manual-commit-java-group");
        props.put(ConsumerConfig.KEY_DESERIALIZER_CLASS_CONFIG, StringDeserializer.class.getName());
        props.put(ConsumerConfig.VALUE_DESERIALIZER_CLASS_CONFIG, StringDeserializer.class.getName());
        props.put(ConsumerConfig.AUTO_OFFSET_RESET_CONFIG, "earliest");
        props.put(ConsumerConfig.ENABLE_AUTO_COMMIT_CONFIG, false);

        try (Consumer<String, String> consumer = new KafkaConsumer<>(props)) {
            consumer.subscribe(Collections.singletonList("important-java-topic"));

            System.out.println("Consuming with manual commits (10s timeout)...");
            long endTime = System.currentTimeMillis() + 10000;

            while (System.currentTimeMillis() < endTime) {
                ConsumerRecords<String, String> records = consumer.poll(Duration.ofMillis(1000));
                for (ConsumerRecord<String, String> record : records) {
                    try {
                        // Process the message
                        System.out.printf("Processing: %s%n", record.value());

                        // Commit after successful processing
                        consumer.commitSync(Collections.singletonMap(
                            new TopicPartition(record.topic(), record.partition()),
                            new OffsetAndMetadata(record.offset() + 1)
                        ));
                    } catch (Exception e) {
                        System.err.println("Processing failed: " + e.getMessage());
                        // Don't commit - message will be reprocessed
                    }
                }
            }
        }
    }

    // =========================================================================
    // SEEK TO OFFSET
    // =========================================================================

    public static void seekConsumer() {
        Properties props = new Properties();
        props.put(ConsumerConfig.BOOTSTRAP_SERVERS_CONFIG, BOOTSTRAP_SERVERS);
        props.put(ConsumerConfig.GROUP_ID_CONFIG, "seek-java-group-" + System.currentTimeMillis());
        props.put(ConsumerConfig.KEY_DESERIALIZER_CLASS_CONFIG, StringDeserializer.class.getName());
        props.put(ConsumerConfig.VALUE_DESERIALIZER_CLASS_CONFIG, StringDeserializer.class.getName());

        try (Consumer<String, String> consumer = new KafkaConsumer<>(props)) {
            TopicPartition tp = new TopicPartition("java-topic", 0);
            consumer.assign(Collections.singletonList(tp));

            // Seek to beginning
            consumer.seekToBeginning(Collections.singletonList(tp));
            System.out.println("Position after seekToBeginning: " + consumer.position(tp));

            // Seek to specific offset
            consumer.seek(tp, 5);
            System.out.println("Position after seek(5): " + consumer.position(tp));

            // Seek to end
            consumer.seekToEnd(Collections.singletonList(tp));
            System.out.println("Position after seekToEnd: " + consumer.position(tp));
        }
    }

    // =========================================================================
    // ROUND-TRIP TEST
    // =========================================================================

    public static void roundtrip() throws Exception {
        String topic = "roundtrip-java";

        // 1. Produce messages
        System.out.println("1. Producing messages...");
        Properties producerProps = new Properties();
        producerProps.put(ProducerConfig.BOOTSTRAP_SERVERS_CONFIG, BOOTSTRAP_SERVERS);
        producerProps.put(ProducerConfig.KEY_SERIALIZER_CLASS_CONFIG, StringSerializer.class.getName());
        producerProps.put(ProducerConfig.VALUE_SERIALIZER_CLASS_CONFIG, StringSerializer.class.getName());

        try (Producer<String, String> producer = new KafkaProducer<>(producerProps)) {
            for (int i = 0; i < 5; i++) {
                producer.send(new ProducerRecord<>(topic, "key-" + i, "Message " + i));
            }
            producer.flush();
        }
        System.out.println("   Produced 5 messages");

        // 2. Wait for flush
        System.out.println("2. Waiting for flush to storage (8s)...");
        Thread.sleep(8000);

        // 3. Consume messages
        System.out.println("3. Consuming messages...");
        Properties consumerProps = new Properties();
        consumerProps.put(ConsumerConfig.BOOTSTRAP_SERVERS_CONFIG, BOOTSTRAP_SERVERS);
        consumerProps.put(ConsumerConfig.GROUP_ID_CONFIG, "roundtrip-java-" + System.currentTimeMillis());
        consumerProps.put(ConsumerConfig.KEY_DESERIALIZER_CLASS_CONFIG, StringDeserializer.class.getName());
        consumerProps.put(ConsumerConfig.VALUE_DESERIALIZER_CLASS_CONFIG, StringDeserializer.class.getName());
        consumerProps.put(ConsumerConfig.AUTO_OFFSET_RESET_CONFIG, "earliest");

        int count = 0;
        try (Consumer<String, String> consumer = new KafkaConsumer<>(consumerProps)) {
            consumer.subscribe(Collections.singletonList(topic));

            long endTime = System.currentTimeMillis() + 10000;
            while (System.currentTimeMillis() < endTime) {
                ConsumerRecords<String, String> records = consumer.poll(Duration.ofMillis(1000));
                for (ConsumerRecord<String, String> record : records) {
                    count++;
                    System.out.printf("   Received: key=%s, value=%s, offset=%d%n",
                        record.key(), record.value(), record.offset());
                }
            }
        }
        System.out.printf("   Consumed %d messages%n", count);
    }

    // =========================================================================
    // ADMIN OPERATIONS
    // =========================================================================

    public static void adminOperations() throws Exception {
        Properties props = new Properties();
        props.put(AdminClientConfig.BOOTSTRAP_SERVERS_CONFIG, BOOTSTRAP_SERVERS);

        try (AdminClient admin = AdminClient.create(props)) {
            // List topics
            Set<String> topics = admin.listTopics().names().get();
            System.out.println("Existing topics: " + topics);

            // Create a topic
            NewTopic newTopic = new NewTopic("admin-java-topic", 3, (short) 1);
            try {
                admin.createTopics(Collections.singletonList(newTopic)).all().get();
                System.out.println("Created topic: admin-java-topic");
            } catch (ExecutionException e) {
                System.out.println("Topic already exists or error: " + e.getCause().getMessage());
            }

            // Describe topic
            Map<String, TopicDescription> descriptions = admin.describeTopics(
                Collections.singletonList("admin-java-topic")
            ).allTopicNames().get();

            for (TopicDescription desc : descriptions.values()) {
                System.out.printf("Topic %s: %d partitions%n",
                    desc.name(), desc.partitions().size());
            }

            // List consumer groups
            Collection<ConsumerGroupListing> groups = admin.listConsumerGroups().all().get();
            System.out.println("Consumer groups:");
            for (ConsumerGroupListing group : groups) {
                System.out.println("  - " + group.groupId());
            }
        }
    }

    // =========================================================================
    // MAIN
    // =========================================================================

    public static void main(String[] args) throws Exception {
        Map<String, Runnable> examples = new LinkedHashMap<>();
        examples.put("basic-producer", () -> { try { basicProducer(); } catch (Exception e) { throw new RuntimeException(e); } });
        examples.put("producer-headers", () -> { try { producerWithHeaders(); } catch (Exception e) { throw new RuntimeException(e); } });
        examples.put("batch-producer", () -> { try { batchProducer(); } catch (Exception e) { throw new RuntimeException(e); } });
        examples.put("async-producer", () -> { try { asyncProducer(); } catch (Exception e) { throw new RuntimeException(e); } });
        examples.put("basic-consumer", JavaExample::basicConsumer);
        examples.put("manual-commit", JavaExample::manualCommitConsumer);
        examples.put("seek", JavaExample::seekConsumer);
        examples.put("roundtrip", () -> { try { roundtrip(); } catch (Exception e) { throw new RuntimeException(e); } });
        examples.put("admin", () -> { try { adminOperations(); } catch (Exception e) { throw new RuntimeException(e); } });

        if (args.length < 1) {
            System.out.println("StreamHouse Kafka Protocol - Java Examples");
            System.out.println("==================================================");
            System.out.println("\nUsage: java JavaExample <example>");
            System.out.println("\nAvailable examples:");
            for (String name : examples.keySet()) {
                System.out.println("  - " + name);
            }
            System.out.println("\nExample: java JavaExample roundtrip");
            return;
        }

        String example = args[0];
        Runnable fn = examples.get(example);
        if (fn == null) {
            System.out.println("Unknown example: " + example);
            System.out.println("Available: " + String.join(", ", examples.keySet()));
            System.exit(1);
        }

        System.out.println("\nRunning example: " + example);
        System.out.println("--------------------------------------------------");

        try {
            fn.run();
        } catch (Exception e) {
            System.err.println("Error: " + e.getMessage());
            e.printStackTrace();
            System.exit(1);
        }
    }
}
