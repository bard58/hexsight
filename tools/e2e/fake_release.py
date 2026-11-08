#!/usr/bin/env python3
"""A fake GitHub release server for the update button test (Python stdlib only).

Usage:
  python fake_release.py <port> <mode> <exe-file> <log-file> [tag]

  mode   good     the sha256 file holds the real hash of <exe-file>
         badhash  the sha256 file holds a wrong hash
         dead     same as good; give an exe that exits at once (for example whoami.exe)
  tag    release tag, default v0.1.9

Serves, on 127.0.0.1 only:
  GET /repos/bard58/hexsight/releases/latest         release JSON like GitHub
  GET /bard58/hexsight/releases/download/<tag>/<f>   302 to /dl/<f>
  GET /dl/<f>                                        the bytes
Every request line goes to <log-file>.
"""
import hashlib
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, HTTPServer

REPO = "bard58/hexsight"


def main():
    if len(sys.argv) < 5:
        sys.exit(__doc__)
    port = int(sys.argv[1])
    mode = sys.argv[2]
    exe_path = sys.argv[3]
    log_path = sys.argv[4]
    tag = sys.argv[5] if len(sys.argv) > 5 else "v0.1.9"
    if mode not in ("good", "badhash", "dead"):
        sys.exit("mode must be good, badhash or dead")

    with open(exe_path, "rb") as f:
        exe_bytes = f.read()
    name = f"hexsight-{tag}-windows-x64.exe"
    sha_name = name + ".sha256"
    real = hashlib.sha256(exe_bytes).hexdigest()
    shown = real if mode != "badhash" else ("0" * 63 + "1" if real[-1] != "1" else "0" * 63 + "2")
    sha_bytes = f"{shown}  {name}\n".encode("ascii")
    files = {name: exe_bytes, sha_name: sha_bytes}
    base = f"http://127.0.0.1:{port}"

    def release():
        def asset(n, size):
            return {
                "name": n,
                "state": "uploaded",
                "size": size,
                "browser_download_url": f"{base}/{REPO}/releases/download/{tag}/{n}",
            }
        return {
            "tag_name": tag,
            "name": tag,
            "draft": False,
            "prerelease": False,
            "assets": [asset(name, len(exe_bytes)), asset(sha_name, len(sha_bytes))],
        }

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, fmt, *args):  # replaces the stderr log
            pass

        def note(self, status):
            line = f"{time.strftime('%Y-%m-%dT%H:%M:%S')} {self.command} {self.path} -> {status}\n"
            with open(log_path, "a", encoding="utf-8") as f:
                f.write(line)

        def send(self, status, body=b"", ctype="application/octet-stream", extra=None):
            self.note(status)
            self.send_response(status)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            for k, v in (extra or {}).items():
                self.send_header(k, v)
            self.send_header("Connection", "close")
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(body)

        def do_GET(self):
            path = self.path.split("?", 1)[0]
            if path == f"/repos/{REPO}/releases/latest":
                body = json.dumps(release()).encode()
                return self.send(200, body, "application/json")
            prefix = f"/{REPO}/releases/download/{tag}/"
            if path.startswith(prefix) and path[len(prefix):] in files:
                return self.send(302, b"", extra={"Location": "/dl/" + path[len(prefix):]})
            if path.startswith("/dl/") and path[4:] in files:
                return self.send(200, files[path[4:]])
            self.send(404, b"not found", "text/plain")

    with open(log_path, "a", encoding="utf-8") as f:
        f.write(f"{time.strftime('%Y-%m-%dT%H:%M:%S')} server start port={port} mode={mode} exe={exe_path} sha256={shown}\n")
    srv = HTTPServer(("127.0.0.1", port), Handler)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
