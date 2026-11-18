#!/usr/bin/env python3
"""HTTP CONNECT logging/allow-list proxy used by the Android privacy check."""

import argparse
import json
import selectors
import socket
import socketserver
import threading


def normalize(authority):
    if authority.startswith("["):
        host, _, port = authority[1:].partition("]:")
        return host.lower(), int(port or "443")
    host, separator, port = authority.rpartition(":")
    if not separator:
        host, port = authority, "443"
    return host.lower(), int(port)


class ProxyHandler(socketserver.BaseRequestHandler):
    def handle(self):
        request = bytearray()
        while b"\r\n\r\n" not in request and len(request) < 16384:
            chunk = self.request.recv(4096)
            if not chunk:
                return
            request.extend(chunk)
        first_line = bytes(request).split(b"\r\n", 1)[0].decode("ascii", "replace")
        parts = first_line.split()
        if len(parts) < 2:
            return
        method = parts[0].upper()
        if method != "CONNECT":
            self.request.sendall(b"HTTP/1.1 501 CONNECT required\r\nConnection: close\r\n\r\n")
            return
        authority = parts[1]
        try:
            host, port = normalize(authority)
        except ValueError:
            self.request.sendall(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n")
            return
        expected_host, expected_port = normalize(self.server.expected)
        allowed = host == expected_host and port == expected_port
        self.server.record({"host": host, "port": port, "allowed": allowed})
        if not allowed:
            self.request.sendall(b"HTTP/1.1 403 Host blocked\r\nConnection: close\r\n\r\n")
            return
        try:
            upstream = socket.create_connection((host, port), timeout=10)
        except OSError:
            self.request.sendall(b"HTTP/1.1 502 Relay unavailable\r\nConnection: close\r\n\r\n")
            return
        with upstream:
            self.request.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
            relay(self.request, upstream)


def relay(client, upstream):
    client.setblocking(False)
    upstream.setblocking(False)
    selector = selectors.DefaultSelector()
    selector.register(client, selectors.EVENT_READ, upstream)
    selector.register(upstream, selectors.EVENT_READ, client)
    try:
        while True:
            for key, _ in selector.select(timeout=30):
                data = key.fileobj.recv(65536)
                if not data:
                    return
                key.data.sendall(data)
    except OSError:
        return
    finally:
        selector.close()


class ProxyServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True

    def __init__(self, address, expected, log_path):
        super().__init__(address, ProxyHandler)
        self.expected = expected
        self.log_path = log_path
        self.log_lock = threading.Lock()

    def record(self, event):
        with self.log_lock, open(self.log_path, "a", encoding="utf-8") as stream:
            stream.write(json.dumps(event, sort_keys=True) + "\n")
            stream.flush()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--expected", required=True)
    parser.add_argument("--listen", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=8899)
    parser.add_argument("--log", required=True)
    args = parser.parse_args()
    with ProxyServer((args.listen, args.port), args.expected, args.log) as server:
        print(f"Logging proxy listening on {args.listen}:{args.port}; allowing {args.expected}", flush=True)
        server.serve_forever()


if __name__ == "__main__":
    main()
