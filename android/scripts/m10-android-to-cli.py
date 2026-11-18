#!/usr/bin/env python3
"""Ephemeral host harness for Android-to-CLI M10 interoperability."""

from __future__ import annotations

import hashlib
import argparse
import os
import secrets
import socket
import socketserver
import subprocess
import sys
import tempfile
import time
import base64
import json
import struct
import threading
import urllib.request
from pathlib import Path
from urllib.parse import urlparse


ROOT = Path(__file__).resolve().parents[2]
RELAY_PORT = 8080
COLLECTOR_PORT = 8123
TEXT = b"Android-to-CLI M10 text interoperability"
WEB_TEXT = "Android link opened and decrypted by the web receiver"
FILE_SIZE = 256 * 1024


class CollectorHandler(socketserver.BaseRequestHandler):
    def handle(self) -> None:
        self.request.settimeout(10)
        buffer = bytearray()
        while b"\r\n\r\n" not in buffer:
            buffer.extend(self.request.recv(4096))
        head, body = bytes(buffer).split(b"\r\n\r\n", 1)
        lines = head.decode("ascii", "replace").split("\r\n")
        headers = {key.lower(): value.strip() for line in lines[1:] if ":" in line
                   for key, value in [line.split(":", 1)]}
        length = int(headers.get("content-length", "0"))
        while len(body) < length:
            body += self.request.recv(min(4096, length - len(body)))
        if not lines[0].startswith("POST /collect "):
            response = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        else:
            try:
                links = body[:length].decode("utf-8").splitlines()
                if len(links) != 3 or any(not link.startswith("https://") or "#" not in link for link in links):
                    raise ValueError
                self.server.received[:] = links
                response = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            except (UnicodeDecodeError, ValueError):
                response = b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        self.request.sendall(response)


class CollectorServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True

    def __init__(self, address: tuple[str, int], handler: type[socketserver.BaseRequestHandler]) -> None:
        super().__init__(address, handler)
        self.received: list[str] = []


def run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(command, cwd=ROOT, check=True, **kwargs)


def read_exact(sock: socket.socket, count: int) -> bytes:
    result = bytearray()
    while len(result) < count:
        part = sock.recv(count - len(result))
        if not part:
            raise RuntimeError("Chrome DevTools connection closed")
        result.extend(part)
    return bytes(result)


def cdp_connect(ws_url: str) -> socket.socket:
    parsed = urlparse(ws_url)
    sock = socket.create_connection((parsed.hostname or "127.0.0.1", parsed.port or 80), timeout=10)
    key = base64.b64encode(os.urandom(16)).decode("ascii")
    path = parsed.path + (f"?{parsed.query}" if parsed.query else "")
    request = (
        f"GET {path} HTTP/1.1\r\n"
        f"Host: {parsed.netloc}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    )
    sock.sendall(request.encode("ascii"))
    response = bytearray()
    while not response.endswith(b"\r\n\r\n"):
        response.extend(sock.recv(1))
        if len(response) > 8192:
            raise RuntimeError("Invalid Chrome DevTools handshake")
    if b" 101 " not in response:
        status = bytes(response).split(b"\r\n", 1)[0].decode("ascii", "replace")
        raise RuntimeError(f"Chrome DevTools local WebSocket handshake returned {status}")
    return sock


def cdp_send(sock: socket.socket, message: dict[str, object]) -> None:
    payload = json.dumps(message).encode("utf-8")
    mask = os.urandom(4)
    size = len(payload)
    header = bytearray([0x81])
    if size < 126:
        header.append(0x80 | size)
    elif size <= 0xFFFF:
        header.append(0x80 | 126)
        header.extend(struct.pack("!H", size))
    else:
        header.append(0x80 | 127)
        header.extend(struct.pack("!Q", size))
    sock.sendall(header + mask + bytes(value ^ mask[index % 4] for index, value in enumerate(payload)))


def cdp_recv(sock: socket.socket) -> dict[str, object]:
    while True:
        first, second = read_exact(sock, 2)
        opcode = first & 0x0F
        size = second & 0x7F
        if size == 126:
            size = struct.unpack("!H", read_exact(sock, 2))[0]
        elif size == 127:
            size = struct.unpack("!Q", read_exact(sock, 8))[0]
        mask = read_exact(sock, 4) if second & 0x80 else b""
        payload = read_exact(sock, size)
        if mask:
            payload = bytes(value ^ mask[index % 4] for index, value in enumerate(payload))
        if opcode == 8:
            raise RuntimeError("Chrome DevTools WebSocket closed")
        if opcode == 9:
            sock.sendall(bytes([0x8A, len(payload)]) + payload)
            continue
        if opcode == 1:
            return json.loads(payload)


def cdp_command(sock: socket.socket, command_id: int, method: str, params: dict[str, object] | None = None) -> dict[str, object]:
    cdp_send(sock, {"id": command_id, "method": method, "params": params or {}})
    while True:
        message = cdp_recv(sock)
        if message.get("id") == command_id:
            if "error" in message:
                raise RuntimeError("Chrome DevTools command failed")
            return message


def verify_web_receiver(chrome: Path, link: str, expected_text: str, temp_dir: str) -> None:
    profile = Path(temp_dir) / "chrome-profile"
    process = subprocess.Popen(
        [str(chrome), "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
         "--remote-debugging-port=9222", f"--user-data-dir={profile}"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    sock: socket.socket | None = None
    try:
        ready = False
        for _ in range(100):
            if process.poll() is not None:
                raise RuntimeError("Headless Chrome exited")
            try:
                with urllib.request.urlopen("http://127.0.0.1:9222/json/version", timeout=0.5):
                    ready = True
                    break
            except Exception:
                time.sleep(0.1)
        if not ready:
            raise RuntimeError("Headless Chrome DevTools did not become ready")
        print("Headless Chrome DevTools is ready.", flush=True)

        browser_url = link.replace("https://10.0.2.2:8080", "http://127.0.0.1:8080", 1)
        request = urllib.request.Request(
            "http://127.0.0.1:9222/json/new?about%3Ablank",
            method="PUT",
        )
        target = json.loads(urllib.request.urlopen(request, timeout=10).read())
        print("Chrome opened the web receiver URL.", flush=True)
        sock = cdp_connect(target["webSocketDebuggerUrl"])
        print("Connected to the local Chrome DevTools target.", flush=True)
        cdp_command(sock, 1, "Runtime.enable")
        cdp_command(sock, 2, "Page.enable")
        cdp_command(sock, 3, "Page.navigate", {"url": browser_url})
        print("Enabled Chrome page inspection.", flush=True)

        def evaluate(command_id: int, expression: str) -> object:
            message = cdp_command(sock, command_id, "Runtime.evaluate", {
                "expression": expression,
                "returnByValue": True,
                "awaitPromise": True,
            })
            return message.get("result", {}).get("result", {}).get("value")

        details_ready = False
        for _ in range(20):
            details_ready = evaluate(4, "!!document.querySelector('#btn-download') && !document.querySelector('#view-details').classList.contains('hidden')") is True
            if details_ready:
                break
            time.sleep(0.5)
        if not details_ready:
            state = evaluate(5, "({ready:document.readyState,hashPresent:location.hash.length>0,scripts:[...document.scripts].map(s=>({type:s.type,src:s.src.split('/').pop()})),loadingHidden:document.querySelector('#view-loading').classList.contains('hidden'),loadingText:document.querySelector('#loading-text').textContent,detailsHidden:document.querySelector('#view-details').classList.contains('hidden'),errorVisible:!document.querySelector('#view-error').classList.contains('hidden'),error:document.querySelector('#error-message').textContent,resources:performance.getEntriesByType('resource').length})")
            print(f"Web receiver page state: {state}", flush=True)
            raise RuntimeError("Web receiver did not load Android's link")
        print("Web receiver loaded the Android drop metadata.", flush=True)
        evaluate(5, "document.querySelector('#btn-download').click(); true")
        print("Web receiver started the decrypt flow.", flush=True)
        result = None
        for _ in range(20):
            result = evaluate(6, "!document.querySelector('#view-result').classList.contains('hidden') ? document.querySelector('#text-preview').value : null")
            if result is not None:
                break
            time.sleep(0.5)
        if result != expected_text:
            raise RuntimeError("Web receiver did not decrypt the Android text drop")
        print("Web receiver decrypted the Android text drop.", flush=True)
    finally:
        if sock is not None:
            sock.close()
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def main() -> int:
    parser = argparse.ArgumentParser(description="Exercise Android-to-CLI and optional web receiver M10 interoperability")
    parser.add_argument("--cli-only", action="store_true", help="verify Android file and text drops with the CLI, without starting Chrome")
    options = parser.parse_args()
    windows = os.name == "nt"
    relay_exe = ROOT / "target" / "debug" / ("ember-android-test-server.exe" if windows else "ember-android-test-server")
    cli_exe = ROOT / "target" / "debug" / ("ember.exe" if windows else "ember")
    gradle = ROOT / "android" / ("gradlew.bat" if windows else "gradlew")
    serial = os.environ.get("ANDROID_SERIAL")
    adb = os.environ.get("ADB", "adb")
    adb_args = [adb] + (["-s", serial] if serial else [])
    if subprocess.run(adb_args + ["get-state"], cwd=ROOT, stdout=subprocess.DEVNULL).returncode:
        raise RuntimeError("No Android emulator is connected")
    api = subprocess.check_output(adb_args + ["shell", "getprop", "ro.build.version.sdk"], cwd=ROOT).decode().strip()
    if api != "34":
        raise RuntimeError(f"M10 interop requires API 34; found API {api}")

    run(["cargo", "build", "--manifest-path", "android/test-relay-server/Cargo.toml", "--bin", "ember-android-test-server"], stdout=subprocess.DEVNULL)
    run(["cargo", "build", "-p", "ember-cli", "--bin", "ember"], stdout=subprocess.DEVNULL)
    if not relay_exe.exists() or not cli_exe.exists():
        raise RuntimeError("The relay or CLI executable was not built")

    expected_file = bytes(index % 239 for index in range(FILE_SIZE))
    expected_file_hash = hashlib.sha256(expected_file).hexdigest()
    passphrase = secrets.token_urlsafe(24)
    httpd = CollectorServer(("0.0.0.0", COLLECTOR_PORT), CollectorHandler)
    collector_thread = threading.Thread(target=httpd.serve_forever, daemon=True)
    collector_thread.start()
    relay_process = subprocess.Popen([str(relay_exe)], cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    try:
        ready = False
        for _ in range(100):
            if relay_process.poll() is not None:
                raise RuntimeError("The local relay test harness exited")
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{RELAY_PORT}/v1/pow/challenge", timeout=0.5) as response:
                    ready = response.status == 200
                    break
            except Exception:
                time.sleep(0.1)
        if not ready:
            raise RuntimeError("The local relay test harness did not become ready")

        gradle_args = [
            str(gradle),
            "--no-daemon",
            "-p",
            "android",
            "connectedDebugAndroidTest",
            "-Pandroid.testInstrumentationRunnerArguments.class=com.ember.mobile.AndroidToCliInteropInstrumentedTest",
            f"-Pandroid.testInstrumentationRunnerArguments.m10_callback=http://10.0.2.2:{COLLECTOR_PORT}/collect",
            f"-Pandroid.testInstrumentationRunnerArguments.m10_passphrase={passphrase}",
        ]
        gradle_process = subprocess.Popen(gradle_args, cwd=ROOT)
        gradle_process.wait()
        if gradle_process.returncode:
            raise subprocess.CalledProcessError(gradle_process.returncode, gradle_args)
        if len(httpd.received) != 3:
            raise RuntimeError("Android did not deliver all test links to the in-memory harness")
        print("Collected three Android-generated test links in process memory.", flush=True)

        with tempfile.TemporaryDirectory(prefix="ember-m10-cli-") as temp_dir:
            file_path = Path(temp_dir) / "received-file.bin"
            text_path = Path(temp_dir) / "received-text.txt"
            run([str(cli_exe), "receive", httpd.received[0], "--relay", f"http://127.0.0.1:{RELAY_PORT}", "--passphrase", passphrase, "--output", str(file_path)], stdout=subprocess.DEVNULL)
            run([str(cli_exe), "receive", httpd.received[1], "--relay", f"http://127.0.0.1:{RELAY_PORT}", "--output", str(text_path)], stdout=subprocess.DEVNULL)
            if hashlib.sha256(file_path.read_bytes()).hexdigest() != expected_file_hash:
                raise RuntimeError("CLI file output did not match the Android sender bytes")
            if text_path.read_bytes() != TEXT:
                raise RuntimeError("CLI text output did not match the Android sender text")
            print("CLI decrypted the Android file and text drops.", flush=True)

            if not options.cli_only:
                chrome = Path(os.environ.get("CHROME_BIN", r"C:\Program Files\Google\Chrome\Application\chrome.exe"))
                if not chrome.is_file():
                    raise RuntimeError("Set CHROME_BIN to a Chrome executable for web receiver interoperability")
                print("Opening the Android text link in the web receiver.", flush=True)
                verify_web_receiver(chrome, httpd.received[2], WEB_TEXT, temp_dir)

        if options.cli_only:
            print("Android file and text drops received by CLI.")
        else:
            print("Android file and text drops received by CLI; Android text link opened and decrypted by the web receiver.")
        return 0
    finally:
        httpd.shutdown()
        collector_thread.join(timeout=5)
        httpd.server_close()
        relay_process.terminate()
        try:
            relay_process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            relay_process.kill()
            relay_process.wait()
        httpd.received.clear()
        passphrase = ""


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"M10 Android-to-CLI interoperability failed: {error.__class__.__name__}", file=sys.stderr)
        raise SystemExit(1)
