#!/usr/bin/env python3
"""Minimal HTTP server that logs POST request bodies to a file.

Usage: python3 mock_webhook.py <port> <log_file>
"""
import sys
import json
from http.server import HTTPServer, BaseHTTPRequestHandler


class WebhookHandler(BaseHTTPRequestHandler):
    log_file = None

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length).decode("utf-8") if length else ""
        auth = self.headers.get("Authorization", "")
        entry = {"auth": auth, "body": json.loads(body) if body else None}
        with open(self.log_file, "a") as f:
            f.write(json.dumps(entry) + "\n")
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"ok")

    def log_message(self, format, *args):
        pass  # suppress console output


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 9999
    log_file = sys.argv[2] if len(sys.argv) > 2 else "/tmp/webhook.log"
    WebhookHandler.log_file = log_file
    server = HTTPServer(("0.0.0.0", port), WebhookHandler)
    print(f"Mock webhook listening on :{port}, logging to {log_file}")
    sys.stdout.flush()
    server.serve_forever()
