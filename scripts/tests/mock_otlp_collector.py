#!/usr/bin/env python3
"""Minimal OTLP HTTP collector for CI smoke tests."""

from __future__ import annotations

import argparse
import base64
import http.server
import json
import threading
import signal


class _CollectorHandler(http.server.BaseHTTPRequestHandler):
    storage = None  # type: ignore[var-annotated]

    def do_POST(self) -> None:  # noqa: N802 (BaseHTTPRequestHandler API)
        length = int(self.headers.get("Content-Length", 0))
        payload = self.rfile.read(length) if length else b""
        record = {
            "path": self.path,
            "headers": dict(self.headers.items()),
            "body_b64": base64.b64encode(payload).decode("ascii"),
        }
        try:
            record["body"] = payload.decode("utf-8")
        except UnicodeDecodeError:
            record["body"] = ""
        _CollectorHandler.storage.append(record)
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b"{}")

    def log_message(self, format: str, *args) -> None:  # noqa: A003 (matches base signature)
        return  # suppress noisy stdout in CI


def serve(port: int, sink: str) -> None:
    records: list[dict[str, str]] = []
    _CollectorHandler.storage = records
    server = http.server.HTTPServer(("127.0.0.1", port), _CollectorHandler)

    stop_event = threading.Event()

    def _run() -> None:
        with server:
            server.serve_forever()
        stop_event.set()

    thread = threading.Thread(target=_run, daemon=True)
    thread.start()

    def _handle_signal(_signum, _frame) -> None:
        stop_event.set()
        server.shutdown()

    signal.signal(signal.SIGINT, _handle_signal)
    signal.signal(signal.SIGTERM, _handle_signal)

    try:
        while not stop_event.is_set():
            thread.join(timeout=0.5)
    finally:
        server.shutdown()
        thread.join()
        with open(sink, "w", encoding="utf-8") as handle:
            json.dump(records, handle, indent=2, sort_keys=True)


def main() -> None:
    parser = argparse.ArgumentParser(description="CI OTLP collector")
    parser.add_argument("--port", type=int, default=4318)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    serve(args.port, args.output)


if __name__ == "__main__":
    main()
