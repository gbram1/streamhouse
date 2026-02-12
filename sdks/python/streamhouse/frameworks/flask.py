"""StreamHouse Flask Integration.

Provides Flask-specific utilities for integrating StreamHouse into your Flask
applications, including the extension pattern for client management, WSGI
middleware for automatic request event publishing, and a thread-based
background consumer.

Example usage:

    from flask import Flask
    from streamhouse.frameworks.flask import (
        StreamHouseExtension,
        StreamHouseMiddleware,
        BackgroundConsumer,
    )

    # Extension pattern (recommended)
    app = Flask(__name__)
    app.config["STREAMHOUSE_BASE_URL"] = "http://localhost:8080"
    app.config["STREAMHOUSE_API_KEY"] = "sk_live_xxx"

    sh = StreamHouseExtension(app)

    @app.route("/topics")
    def list_topics():
        client = sh.client
        return {"topics": [t.name for t in client.list_topics()]}

    # Or use the factory pattern with init_app
    sh = StreamHouseExtension()
    sh.init_app(app)

    # WSGI middleware for automatic request event publishing
    app.wsgi_app = StreamHouseMiddleware(
        app.wsgi_app,
        base_url="http://localhost:8080",
        topic="http-requests",
    )

    # Background consumer for processing events
    def handle_event(record):
        print(f"Got event: {record.value}")

    consumer = BackgroundConsumer(
        base_url="http://localhost:8080",
        topic="events",
        partition=0,
        callback=handle_event,
    )
    consumer.start()
"""

import json
import time
import logging
import threading
from typing import Any, Callable, Dict, Optional

from ..client import StreamHouseClient
from ._sync import SyncBackgroundConsumer as BackgroundConsumer

logger = logging.getLogger("streamhouse.flask")

__all__ = [
    "StreamHouseExtension",
    "StreamHouseMiddleware",
    "BackgroundConsumer",
]


class StreamHouseExtension:
    """Flask extension for StreamHouse client management.

    Follows the standard Flask extension pattern, supporting both direct
    initialization and the application factory pattern via ``init_app()``.

    The extension reads configuration from ``app.config`` using these keys:

    - ``STREAMHOUSE_BASE_URL``: StreamHouse server URL (required unless
      passed to ``init_app`` directly).
    - ``STREAMHOUSE_API_KEY``: Optional API key for authentication.
    - ``STREAMHOUSE_TIMEOUT``: Request timeout in seconds (default: 30.0).

    Example::

        from flask import Flask
        from streamhouse.frameworks.flask import StreamHouseExtension

        # Direct initialization
        app = Flask(__name__)
        app.config["STREAMHOUSE_BASE_URL"] = "http://localhost:8080"
        sh = StreamHouseExtension(app)

        # Factory pattern
        sh = StreamHouseExtension()

        def create_app():
            app = Flask(__name__)
            app.config["STREAMHOUSE_BASE_URL"] = "http://localhost:8080"
            sh.init_app(app)
            return app

        # Access the client
        @app.route("/topics")
        def list_topics():
            return {"topics": [t.name for t in sh.client.list_topics()]}
    """

    def __init__(self, app: Any = None, **kwargs: Any):
        """Initialize the extension.

        Args:
            app: Optional Flask application instance. If provided, calls
                ``init_app(app, **kwargs)`` immediately.
            **kwargs: Additional keyword arguments passed to ``init_app()``.
        """
        self._client: Optional[StreamHouseClient] = None
        if app is not None:
            self.init_app(app, **kwargs)

    def init_app(
        self,
        app: Any,
        base_url: Optional[str] = None,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
    ) -> None:
        """Initialize the extension with a Flask application.

        Configuration is resolved in this order:
        1. Explicit keyword arguments to this method.
        2. Values from ``app.config``.

        Args:
            app: Flask application instance.
            base_url: StreamHouse server URL. Falls back to
                ``app.config["STREAMHOUSE_BASE_URL"]``.
            api_key: Optional API key. Falls back to
                ``app.config["STREAMHOUSE_API_KEY"]``.
            timeout: Request timeout in seconds. Falls back to
                ``app.config["STREAMHOUSE_TIMEOUT"]`` (default: 30.0).

        Raises:
            ValueError: If no ``base_url`` is provided either as argument or
                in app config.
        """
        resolved_base_url = base_url or app.config.get("STREAMHOUSE_BASE_URL")
        resolved_api_key = api_key or app.config.get("STREAMHOUSE_API_KEY")
        resolved_timeout = app.config.get("STREAMHOUSE_TIMEOUT", timeout)

        if not resolved_base_url:
            raise ValueError(
                "StreamHouse base_url is required. Set STREAMHOUSE_BASE_URL "
                "in app.config or pass base_url to init_app()."
            )

        self._client = StreamHouseClient(
            base_url=resolved_base_url,
            api_key=resolved_api_key,
            timeout=resolved_timeout,
        )

        # Register the extension on the app
        if not hasattr(app, "extensions"):
            app.extensions = {}
        app.extensions["streamhouse"] = self

    @property
    def client(self) -> StreamHouseClient:
        """Return the configured StreamHouseClient instance.

        Returns:
            The StreamHouseClient.

        Raises:
            RuntimeError: If the extension has not been initialized with an
                application yet.
        """
        if self._client is None:
            raise RuntimeError(
                "StreamHouseExtension has not been initialized. "
                "Call init_app(app) or pass app to the constructor first."
            )
        return self._client


class StreamHouseMiddleware:
    """WSGI middleware that automatically publishes HTTP request events to StreamHouse.

    For every incoming request, this middleware publishes an event containing
    the request method, path, response status code, and duration to a
    configurable StreamHouse topic. This is useful for request logging,
    analytics, and audit trails.

    Event publishing is fire-and-forget via a daemon thread so it does not
    block the response. Failures are logged but never affect the
    request/response cycle.

    Example::

        from flask import Flask
        from streamhouse.frameworks.flask import StreamHouseMiddleware

        app = Flask(__name__)
        app.wsgi_app = StreamHouseMiddleware(
            app.wsgi_app,
            base_url="http://localhost:8080",
            topic="http-requests",
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
        """Initialize the WSGI middleware.

        Args:
            app: The WSGI application to wrap.
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

    def __call__(self, environ: Dict[str, Any], start_response: Callable) -> Any:
        """WSGI interface.

        Wraps the inner application, captures the response status code via
        a ``start_response`` wrapper, measures request duration, and publishes
        the event asynchronously in a daemon thread.
        """
        start_time = time.monotonic()
        status_code = 500  # Default in case of unhandled error

        def start_response_wrapper(status: str, response_headers: list, exc_info: Any = None) -> Any:
            nonlocal status_code
            # WSGI status is a string like "200 OK"
            try:
                status_code = int(status.split(" ", 1)[0])
            except (ValueError, IndexError):
                status_code = 500
            return start_response(status, response_headers, exc_info)

        try:
            response = self.app(environ, start_response_wrapper)
            return response
        finally:
            duration_ms = (time.monotonic() - start_time) * 1000

            # Build the event payload
            event: Dict[str, Any] = {
                "method": environ.get("REQUEST_METHOD", "UNKNOWN"),
                "path": environ.get("PATH_INFO", "/"),
                "status_code": status_code,
                "duration_ms": round(duration_ms, 2),
                "timestamp": int(time.time() * 1000),
            }

            if self.include_headers:
                headers: Dict[str, str] = {}
                for key, value in environ.items():
                    if key.startswith("HTTP_"):
                        # Convert HTTP_ACCEPT_LANGUAGE to Accept-Language
                        header_name = key[5:].replace("_", "-").title()
                        headers[header_name] = value
                if "CONTENT_TYPE" in environ:
                    headers["Content-Type"] = environ["CONTENT_TYPE"]
                if "CONTENT_LENGTH" in environ:
                    headers["Content-Length"] = environ["CONTENT_LENGTH"]
                event["headers"] = headers

            # Fire-and-forget: publish in a daemon thread
            thread = threading.Thread(
                target=self._publish_event,
                args=(event,),
                daemon=True,
            )
            thread.start()

    def _publish_event(self, event: Dict[str, Any]) -> None:
        """Publish a request event to StreamHouse.

        Runs in a daemon thread. Errors are logged but never propagated.

        Args:
            event: The event dictionary to publish as JSON.
        """
        try:
            client = StreamHouseClient(
                base_url=self.base_url,
                api_key=self.api_key,
            )
            client.produce(self.topic, json.dumps(event))
        except Exception as e:
            logger.warning("Failed to publish request event: %s", e)
