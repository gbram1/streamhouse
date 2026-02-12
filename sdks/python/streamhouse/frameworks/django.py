"""StreamHouse Django Integration.

Provides Django-specific utilities for integrating StreamHouse into your
Django applications, including a singleton client configured from Django
settings, WSGI middleware for automatic request event publishing, and a
base management command for running background consumers.

Configuration in ``settings.py``::

    STREAMHOUSE = {
        "BASE_URL": "http://localhost:8080",
        "API_KEY": "sk_live_xxx",  # optional
        "TIMEOUT": 30.0,

        # Middleware-specific settings
        "MIDDLEWARE_TOPIC": "http-requests",
        "MIDDLEWARE_INCLUDE_HEADERS": False,
    }

    MIDDLEWARE = [
        # ...
        "streamhouse.frameworks.django.StreamHouseMiddleware",
    ]

Usage examples::

    # In a view — use the singleton client
    from streamhouse.frameworks.django import get_client

    def my_view(request):
        client = get_client()
        result = client.produce("events", '{"action": "page_view"}')
        return JsonResponse({"offset": result.offset})

    # Management command for background consumers
    # myapp/management/commands/streamhouse_consume.py
    from django.core.management.base import BaseCommand
    from streamhouse.frameworks.django import StreamHouseConsumerCommand

    class Command(StreamHouseConsumerCommand, BaseCommand):
        consumers = [
            {
                "topic": "events",
                "partition": 0,
                "handler": "myapp.consumers.handle_event",
            },
            {
                "topic": "orders",
                "partition": 0,
                "handler": "myapp.consumers.handle_order",
                "group_id": "order-processor",
                "poll_interval": 0.5,
            },
        ]
"""

import json
import time
import logging
import threading
from typing import Any, Callable, Dict, List, Optional

from ..client import StreamHouseClient
from ._sync import SyncBackgroundConsumer as BackgroundConsumer

logger = logging.getLogger("streamhouse.django")

__all__ = [
    "get_client",
    "StreamHouseMiddleware",
    "StreamHouseConsumerCommand",
    "BackgroundConsumer",
]

_client_instance: Optional[StreamHouseClient] = None
_client_lock = threading.Lock()


def get_client() -> StreamHouseClient:
    """Get a StreamHouseClient configured from Django settings.

    Reads configuration from ``django.conf.settings.STREAMHOUSE`` dict.
    Returns a thread-safe singleton instance that is created on first call
    and reused for subsequent calls.

    The following keys are read from the ``STREAMHOUSE`` settings dict:

    - ``BASE_URL``: StreamHouse server URL (default: ``"http://localhost:8080"``)
    - ``API_KEY``: Optional API key for authentication (default: ``None``)
    - ``TIMEOUT``: Request timeout in seconds (default: ``30.0``)

    Returns:
        A configured ``StreamHouseClient`` instance.

    Raises:
        django.core.exceptions.ImproperlyConfigured: If Django settings are
            not configured.

    Example::

        from streamhouse.frameworks.django import get_client

        def my_view(request):
            client = get_client()
            topics = client.list_topics()
            return JsonResponse({"topics": [t.name for t in topics]})
    """
    global _client_instance
    if _client_instance is None:
        with _client_lock:
            if _client_instance is None:
                from django.conf import settings

                config = getattr(settings, "STREAMHOUSE", {})
                _client_instance = StreamHouseClient(
                    base_url=config.get("BASE_URL", "http://localhost:8080"),
                    api_key=config.get("API_KEY"),
                    timeout=config.get("TIMEOUT", 30.0),
                )
    return _client_instance


class StreamHouseMiddleware:
    """Django middleware that publishes HTTP request events to StreamHouse.

    Follows Django's standard middleware pattern with a ``get_response``
    callable. For every incoming request, publishes an event containing
    the request method, path, status code, and duration to a configurable
    StreamHouse topic. This is useful for request logging, analytics, and
    audit trails.

    Event publishing is fire-and-forget via a daemon thread so that it
    does not block the request/response cycle. Publishing failures are
    logged as warnings but never affect the response.

    Configuration via ``settings.py`` ``STREAMHOUSE`` dict:

    - ``MIDDLEWARE_TOPIC``: Topic name to publish to (default: ``"http-requests"``)
    - ``MIDDLEWARE_INCLUDE_HEADERS``: Include request headers in events (default: ``False``)
    - ``BASE_URL``: StreamHouse server URL (default: ``"http://localhost:8080"``)
    - ``API_KEY``: Optional API key (default: ``None``)
    - ``TIMEOUT``: Request timeout in seconds (default: ``30.0``)

    Example::

        # settings.py
        STREAMHOUSE = {
            "BASE_URL": "http://localhost:8080",
            "MIDDLEWARE_TOPIC": "http-requests",
            "MIDDLEWARE_INCLUDE_HEADERS": True,
        }

        MIDDLEWARE = [
            # ...
            "streamhouse.frameworks.django.StreamHouseMiddleware",
        ]
    """

    def __init__(self, get_response: Callable) -> None:
        """Initialize the middleware.

        Args:
            get_response: The next middleware or view callable in the chain.
        """
        self.get_response = get_response

        from django.conf import settings

        config = getattr(settings, "STREAMHOUSE", {})
        self.topic: str = config.get("MIDDLEWARE_TOPIC", "http-requests")
        self.include_headers: bool = config.get("MIDDLEWARE_INCLUDE_HEADERS", False)
        self._client = StreamHouseClient(
            base_url=config.get("BASE_URL", "http://localhost:8080"),
            api_key=config.get("API_KEY"),
            timeout=config.get("TIMEOUT", 30.0),
        )

    def __call__(self, request: Any) -> Any:
        """Process the request, publish an event, and return the response.

        Args:
            request: The Django ``HttpRequest`` object.

        Returns:
            The Django ``HttpResponse`` object from downstream middleware/view.
        """
        start_time = time.monotonic()
        response = self.get_response(request)
        duration_ms = (time.monotonic() - start_time) * 1000

        event: Dict[str, Any] = {
            "method": request.method,
            "path": request.path,
            "status_code": response.status_code,
            "duration_ms": round(duration_ms, 2),
            "timestamp": int(time.time() * 1000),
        }

        if self.include_headers:
            event["headers"] = dict(request.headers)

        # Fire-and-forget in daemon thread to avoid blocking the response
        threading.Thread(
            target=self._publish_event,
            args=(event,),
            daemon=True,
        ).start()

        return response

    def _publish_event(self, event: Dict[str, Any]) -> None:
        """Publish a request event to StreamHouse.

        Called in a daemon thread. Exceptions are caught and logged as
        warnings so they never propagate or affect request handling.

        Args:
            event: The event dict to publish as JSON.
        """
        try:
            self._client.produce(self.topic, json.dumps(event))
        except Exception as e:
            logger.warning("Failed to publish request event: %s", e)


class StreamHouseConsumerCommand:
    """Base class for Django management commands that run StreamHouse consumers.

    Subclass this alongside ``django.core.management.base.BaseCommand`` and
    define a ``consumers`` list to create a management command that runs
    background consumers with proper signal handling.

    Each entry in the ``consumers`` list is a dict with the following keys:

    - ``topic`` (required): Topic to consume from.
    - ``partition`` (optional, default ``0``): Partition to consume from.
    - ``handler`` (required): Dotted Python path to a synchronous callback
      function (e.g., ``"myapp.consumers.handle_event"``).
    - ``group_id`` (optional): Consumer group ID for offset tracking.
    - ``poll_interval`` (optional, default ``1.0``): Seconds between polls.
    - ``max_records`` (optional, default ``100``): Max records per poll.
    - ``start_offset`` (optional, default ``0``): Initial offset.

    Example::

        # myapp/management/commands/streamhouse_consume.py
        from django.core.management.base import BaseCommand
        from streamhouse.frameworks.django import StreamHouseConsumerCommand

        class Command(StreamHouseConsumerCommand, BaseCommand):
            consumers = [
                {
                    "topic": "events",
                    "partition": 0,
                    "handler": "myapp.consumers.handle_event",
                },
                {
                    "topic": "orders",
                    "partition": 0,
                    "handler": "myapp.consumers.handle_order",
                    "group_id": "order-processor",
                    "poll_interval": 0.5,
                },
            ]

    Run with::

        python manage.py streamhouse_consume
    """

    help = "Run StreamHouse background consumers"
    consumers: List[Dict[str, Any]] = []

    def handle(self, *args: Any, **options: Any) -> None:
        """Start all configured consumers and block until interrupted.

        Reads client configuration from ``django.conf.settings.STREAMHOUSE``,
        imports each consumer's handler from its dotted path, starts a
        ``SyncBackgroundConsumer`` for each entry, and then blocks on a
        ``threading.Event`` until ``SIGINT`` or ``SIGTERM`` is received.

        Args:
            *args: Positional arguments from Django management command framework.
            **options: Keyword arguments from Django management command framework.
        """
        import signal

        from django.conf import settings

        config = getattr(settings, "STREAMHOUSE", {})
        client_config: Dict[str, Any] = {
            "base_url": config.get("BASE_URL", "http://localhost:8080"),
            "api_key": config.get("API_KEY"),
        }

        running: List[SyncBackgroundConsumer] = []
        for consumer_config in self.consumers:
            handler = self._import_handler(consumer_config["handler"])
            consumer = SyncBackgroundConsumer(
                base_url=client_config.get("base_url", "http://localhost:8080"),
                topic=consumer_config["topic"],
                partition=consumer_config.get("partition", 0),
                callback=handler,
                group_id=consumer_config.get("group_id"),
                api_key=client_config.get("api_key"),
                poll_interval=consumer_config.get("poll_interval", 1.0),
                max_records=consumer_config.get("max_records", 100),
                start_offset=consumer_config.get("start_offset", 0),
            )
            consumer.start()
            running.append(consumer)
            self._write_output(
                f"Started consumer for "
                f"{consumer_config['topic']}/{consumer_config.get('partition', 0)}"
            )

        self._write_output(
            f"Running {len(running)} consumer(s). Press Ctrl+C to stop."
        )

        # Block until SIGINT/SIGTERM
        stop_event = threading.Event()

        def _signal_handler(*_: Any) -> None:
            stop_event.set()

        signal.signal(signal.SIGINT, _signal_handler)
        signal.signal(signal.SIGTERM, _signal_handler)

        stop_event.wait()

        self._write_output("Shutting down consumers...")
        for consumer in running:
            consumer.stop()
        self._write_output("All consumers stopped.")

    def _write_output(self, message: str) -> None:
        """Write a message to stdout, falling back to print.

        When mixed with ``BaseCommand``, ``self.stdout`` is available and
        provides Django's styled output writer. If ``stdout`` is not
        available (e.g., when used standalone), falls back to ``print()``.

        Args:
            message: The message string to write.
        """
        stdout = getattr(self, "stdout", None)
        if stdout is not None:
            stdout.write(message)
        else:
            print(message)

    @staticmethod
    def _import_handler(dotted_path: str) -> Callable:
        """Import a handler function from a dotted Python path.

        Args:
            dotted_path: A dotted path like ``"myapp.consumers.handle_event"``.
                The last component is the attribute name, and everything before
                it is the module path.

        Returns:
            The imported callable.

        Raises:
            ImportError: If the module cannot be imported.
            AttributeError: If the function does not exist in the module.
        """
        module_path, func_name = dotted_path.rsplit(".", 1)
        import importlib

        module = importlib.import_module(module_path)
        return getattr(module, func_name)
