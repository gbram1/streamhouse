"""Shared synchronous utilities for WSGI framework integrations.

Provides a thread-based BackgroundConsumer that can be used by both Flask and
Django integrations.
"""

import json
import time
import logging
import threading
from typing import Any, Callable, Optional

from ..client import StreamHouseClient

logger = logging.getLogger("streamhouse.sync")


class SyncBackgroundConsumer:
    """Thread-based background consumer for synchronous (WSGI) frameworks.

    Continuously polls a StreamHouse topic partition in a daemon thread and
    invokes a callback for each consumed record. Designed for Flask and Django
    applications where asyncio is not available.

    The consumer automatically tracks its offset and resumes from where it
    left off within the same process lifetime.

    Example::

        from streamhouse.frameworks._sync import SyncBackgroundConsumer

        def handle_event(record):
            print(f"Received: {record.value} at offset {record.offset}")

        consumer = SyncBackgroundConsumer(
            base_url="http://localhost:8080",
            topic="events",
            partition=0,
            callback=handle_event,
        )
        consumer.start()
        # On shutdown:
        consumer.stop()
    """

    def __init__(
        self,
        base_url: str,
        topic: str,
        partition: int = 0,
        callback: Optional[Callable] = None,
        group_id: Optional[str] = None,
        api_key: Optional[str] = None,
        poll_interval: float = 1.0,
        max_records: int = 100,
        start_offset: int = 0,
    ):
        """Initialize the background consumer.

        Args:
            base_url: StreamHouse server URL.
            topic: Topic to consume from.
            partition: Partition to consume from.
            callback: Synchronous function called for each consumed record.
            group_id: Optional consumer group ID for offset tracking.
            api_key: Optional API key for authentication.
            poll_interval: Seconds between poll attempts when no records found.
            max_records: Maximum records per poll.
            start_offset: Initial offset to consume from.
        """
        self.base_url = base_url
        self.topic = topic
        self.partition = partition
        self.callback = callback
        self.group_id = group_id
        self.api_key = api_key
        self.poll_interval = poll_interval
        self.max_records = max_records
        self._current_offset = start_offset
        self._thread: Optional[threading.Thread] = None
        self._running = False
        self._client: Optional[StreamHouseClient] = None

    def start(self) -> None:
        """Start consuming in a background daemon thread."""
        if self._running:
            return

        self._client = StreamHouseClient(
            base_url=self.base_url,
            api_key=self.api_key,
        )
        self._running = True
        self._thread = threading.Thread(
            target=self._consume_loop,
            name=f"streamhouse-consumer-{self.topic}-{self.partition}",
            daemon=True,
        )
        self._thread.start()
        logger.info(
            "Background consumer started for %s/%d at offset %d",
            self.topic,
            self.partition,
            self._current_offset,
        )

    def stop(self) -> None:
        """Stop the consumer and wait for thread to finish."""
        self._running = False
        if self._thread is not None:
            self._thread.join(timeout=5.0)
            self._thread = None

        self._client = None
        logger.info(
            "Background consumer stopped for %s/%d at offset %d",
            self.topic,
            self.partition,
            self._current_offset,
        )

    def _consume_loop(self) -> None:
        """Synchronous consume loop running in a background thread.

        Continuously polls the configured topic partition. For each record
        received, the callback is invoked and the offset is advanced. When
        no records are available or an error occurs, the loop sleeps for
        ``poll_interval`` seconds before retrying.
        """
        while self._running:
            try:
                result = self._client.consume(
                    topic=self.topic,
                    partition=self.partition,
                    offset=self._current_offset,
                    max_records=self.max_records,
                )

                for record in result.records:
                    if self.callback is not None:
                        try:
                            self.callback(record)
                        except Exception as e:
                            logger.error(
                                "Consumer callback error at offset %d: %s",
                                record.offset,
                                e,
                            )

                    self._current_offset = record.offset + 1

                    # Commit offset if using consumer groups
                    if self.group_id and self._client:
                        try:
                            self._client.commit_offset(
                                self.group_id,
                                self.topic,
                                self.partition,
                                self._current_offset,
                            )
                        except Exception as e:
                            logger.warning("Failed to commit offset: %s", e)

                if not result.records:
                    time.sleep(self.poll_interval)

            except Exception as e:
                logger.error("Consumer poll error: %s", e)
                time.sleep(self.poll_interval)

    @property
    def current_offset(self) -> int:
        """Return the current consumer offset."""
        return self._current_offset

    @property
    def is_running(self) -> bool:
        """Return whether the consumer is running."""
        return self._running
