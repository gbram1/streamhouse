"""StreamHouse FastAPI Integration.

Provides FastAPI-specific utilities for integrating StreamHouse into your
FastAPI applications, including dependency injection, ASGI middleware for
automatic request event publishing, and background consumer management.

Example usage:

    from fastapi import FastAPI, Depends
    from streamhouse.frameworks.fastapi import (
        StreamHouseDependency,
        StreamHouseMiddleware,
        BackgroundConsumer,
        streamhouse_lifespan,
        get_streamhouse,
    )

    # Basic dependency injection
    sh = StreamHouseDependency(base_url="http://localhost:8080")

    app = FastAPI()

    @app.get("/topics")
    async def list_topics(client = Depends(sh)):
        return await client.list_topics_async()

    # With middleware for automatic request event publishing
    app.add_middleware(
        StreamHouseMiddleware,
        base_url="http://localhost:8080",
        topic="http-requests",
    )

    # With lifespan for background consumer
    consumer = BackgroundConsumer(
        base_url="http://localhost:8080",
        topic="events",
        partition=0,
        callback=my_handler,
    )

    app = FastAPI(lifespan=streamhouse_lifespan(consumers=[consumer]))
"""

import asyncio
import json
import time
import logging
from contextlib import asynccontextmanager
from typing import Any, Callable, Awaitable, Dict, List, Optional, Sequence

from ..client import StreamHouseClient

logger = logging.getLogger("streamhouse.fastapi")


class StreamHouseDependency:
    """FastAPI dependency that provides a StreamHouseClient instance.

    Creates and reuses a single async client session across requests. Use with
    FastAPI's ``Depends()`` to inject the client into route handlers.

    Example::

        from fastapi import FastAPI, Depends
        from streamhouse.frameworks.fastapi import StreamHouseDependency

        sh = StreamHouseDependency(base_url="http://localhost:8080")

        app = FastAPI()

        @app.post("/publish")
        async def publish(event: dict, client = Depends(sh)):
            result = await client.produce_async("events", json.dumps(event))
            return {"offset": result.offset}
    """

    def __init__(
        self,
        base_url: str,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
    ):
        """Initialize the dependency.

        Args:
            base_url: StreamHouse server URL.
            api_key: Optional API key for authentication.
            timeout: Request timeout in seconds.
        """
        self.base_url = base_url
        self.api_key = api_key
        self.timeout = timeout
        self._client: Optional[StreamHouseClient] = None

    async def _get_client(self) -> StreamHouseClient:
        """Get or create the shared client instance."""
        if self._client is None:
            self._client = StreamHouseClient(
                base_url=self.base_url,
                api_key=self.api_key,
                timeout=self.timeout,
            )
            await self._client.__aenter__()
        return self._client

    async def __call__(self) -> StreamHouseClient:
        """FastAPI dependency callable. Returns the shared client."""
        return await self._get_client()

    async def close(self) -> None:
        """Close the underlying client session."""
        if self._client is not None:
            await self._client.__aexit__(None, None, None)
            self._client = None


def get_streamhouse(
    base_url: str = "http://localhost:8080",
    api_key: Optional[str] = None,
) -> StreamHouseDependency:
    """Create a StreamHouse dependency for FastAPI.

    Convenience factory for creating a ``StreamHouseDependency`` with common
    defaults. Returns a callable suitable for use with ``Depends()``.

    Args:
        base_url: StreamHouse server URL.
        api_key: Optional API key for authentication.

    Returns:
        A ``StreamHouseDependency`` instance.

    Example::

        from fastapi import FastAPI, Depends
        from streamhouse.frameworks.fastapi import get_streamhouse

        sh = get_streamhouse("http://localhost:8080", api_key="sk_live_xxx")

        @app.get("/topics")
        async def topics(client = Depends(sh)):
            return await client.list_topics_async()
    """
    return StreamHouseDependency(base_url=base_url, api_key=api_key)


class StreamHouseMiddleware:
    """ASGI middleware that automatically publishes HTTP request events to StreamHouse.

    For every incoming request, this middleware publishes an event containing
    the request method, path, status code, and duration to a configurable
    StreamHouse topic. This is useful for request logging, analytics, and
    audit trails.

    The middleware is non-blocking: event publishing failures are logged but
    do not affect the request/response cycle.

    Example::

        from fastapi import FastAPI
        from streamhouse.frameworks.fastapi import StreamHouseMiddleware

        app = FastAPI()
        app.add_middleware(
            StreamHouseMiddleware,
            base_url="http://localhost:8080",
            topic="http-requests",
            api_key="sk_live_xxx",
        )
    """

    def __init__(
        self,
        app: Any,
        base_url: str,
        topic: str = "http-requests",
        api_key: Optional[str] = None,
        include_headers: bool = False,
    ):
        """Initialize the middleware.

        Args:
            app: The ASGI application.
            base_url: StreamHouse server URL.
            topic: Topic to publish request events to.
            api_key: Optional API key for authentication.
            include_headers: Whether to include request headers in events.
        """
        self.app = app
        self.base_url = base_url
        self.topic = topic
        self.api_key = api_key
        self.include_headers = include_headers
        self._client: Optional[StreamHouseClient] = None

    async def _ensure_client(self) -> StreamHouseClient:
        """Lazily initialize the client on first request."""
        if self._client is None:
            self._client = StreamHouseClient(
                base_url=self.base_url,
                api_key=self.api_key,
            )
            await self._client.__aenter__()
        return self._client

    async def __call__(self, scope: Dict, receive: Callable, send: Callable) -> None:
        """ASGI interface."""
        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return

        start_time = time.monotonic()
        status_code = 500  # Default in case of unhandled error

        # Capture the response status code
        async def send_wrapper(message: Dict) -> None:
            nonlocal status_code
            if message["type"] == "http.response.start":
                status_code = message.get("status", 500)
            await send(message)

        try:
            await self.app(scope, receive, send_wrapper)
        finally:
            duration_ms = (time.monotonic() - start_time) * 1000

            # Build the event
            event: Dict[str, Any] = {
                "method": scope.get("method", "UNKNOWN"),
                "path": scope.get("path", "/"),
                "status_code": status_code,
                "duration_ms": round(duration_ms, 2),
                "timestamp": int(time.time() * 1000),
            }

            if self.include_headers:
                headers = {
                    k.decode(): v.decode()
                    for k, v in scope.get("headers", [])
                }
                event["headers"] = headers

            # Publish asynchronously, don't block the response
            try:
                client = await self._ensure_client()
                await client.produce_async(self.topic, json.dumps(event))
            except Exception as e:
                logger.warning("Failed to publish request event: %s", e)


class BackgroundConsumer:
    """Runs a StreamHouse consumer in a background asyncio task.

    Continuously polls a topic partition and invokes a callback for each
    consumed message. Designed to be started during application startup
    and stopped during shutdown, typically via ``streamhouse_lifespan()``.

    The consumer automatically tracks its offset and resumes from where
    it left off after restarts (within the same process lifetime).

    Example::

        from streamhouse.frameworks.fastapi import BackgroundConsumer

        async def handle_event(record):
            print(f"Received: {record.value} at offset {record.offset}")

        consumer = BackgroundConsumer(
            base_url="http://localhost:8080",
            topic="events",
            partition=0,
            callback=handle_event,
            poll_interval=1.0,
        )

        # Start/stop manually
        await consumer.start()
        # ... later ...
        await consumer.stop()
    """

    def __init__(
        self,
        base_url: str,
        topic: str,
        partition: int = 0,
        callback: Optional[Callable[..., Awaitable[None]]] = None,
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
            callback: Async function called for each consumed record.
            group_id: Optional consumer group ID for offset tracking.
            api_key: Optional API key for authentication.
            poll_interval: Seconds between poll attempts.
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
        self._task: Optional[asyncio.Task] = None
        self._running = False
        self._client: Optional[StreamHouseClient] = None

    async def start(self) -> None:
        """Start consuming in a background task."""
        if self._running:
            return

        self._client = StreamHouseClient(
            base_url=self.base_url,
            api_key=self.api_key,
        )
        await self._client.__aenter__()
        self._running = True
        self._task = asyncio.create_task(self._consume_loop())
        logger.info(
            "Background consumer started for %s/%d at offset %d",
            self.topic,
            self.partition,
            self._current_offset,
        )

    async def stop(self) -> None:
        """Stop the background consumer gracefully."""
        self._running = False
        if self._task is not None:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass
            self._task = None

        if self._client is not None:
            await self._client.__aexit__(None, None, None)
            self._client = None

        logger.info(
            "Background consumer stopped for %s/%d at offset %d",
            self.topic,
            self.partition,
            self._current_offset,
        )

    async def _consume_loop(self) -> None:
        """Internal consume loop."""
        while self._running:
            try:
                result = await self._client.consume_async(
                    topic=self.topic,
                    partition=self.partition,
                    offset=self._current_offset,
                    max_records=self.max_records,
                )

                for record in result.records:
                    if self.callback is not None:
                        try:
                            await self.callback(record)
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
                            await self._client.commit_offset_async(
                                self.group_id,
                                self.topic,
                                self.partition,
                                self._current_offset,
                            )
                        except Exception as e:
                            logger.warning("Failed to commit offset: %s", e)

                if not result.records:
                    await asyncio.sleep(self.poll_interval)

            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error("Consumer poll error: %s", e)
                await asyncio.sleep(self.poll_interval)

    @property
    def current_offset(self) -> int:
        """Return the current consumer offset."""
        return self._current_offset

    @property
    def is_running(self) -> bool:
        """Return whether the consumer is running."""
        return self._running


def streamhouse_lifespan(
    consumers: Optional[Sequence[BackgroundConsumer]] = None,
    dependencies: Optional[Sequence[StreamHouseDependency]] = None,
):
    """Create a FastAPI lifespan context manager for StreamHouse resources.

    Manages the lifecycle of background consumers and dependency clients,
    starting them on application startup and stopping them on shutdown.

    Args:
        consumers: Background consumers to start/stop with the application.
        dependencies: Dependencies whose clients should be closed on shutdown.

    Returns:
        An async context manager suitable for FastAPI's ``lifespan`` parameter.

    Example::

        from fastapi import FastAPI
        from streamhouse.frameworks.fastapi import (
            StreamHouseDependency,
            BackgroundConsumer,
            streamhouse_lifespan,
        )

        sh = StreamHouseDependency(base_url="http://localhost:8080")

        async def handle_event(record):
            print(f"Got event: {record.value}")

        consumer = BackgroundConsumer(
            base_url="http://localhost:8080",
            topic="events",
            partition=0,
            callback=handle_event,
        )

        app = FastAPI(lifespan=streamhouse_lifespan(
            consumers=[consumer],
            dependencies=[sh],
        ))
    """

    @asynccontextmanager
    async def lifespan(app: Any):
        # Startup: start all background consumers
        if consumers:
            for consumer in consumers:
                await consumer.start()

        yield

        # Shutdown: stop consumers and close dependencies
        if consumers:
            for consumer in consumers:
                await consumer.stop()

        if dependencies:
            for dep in dependencies:
                await dep.close()

    return lifespan
