#!/usr/bin/env bash
# Make the throwaway TLS certificate for the mock League Client (tools/mock/mock.mjs).
# The mock listens on 127.0.0.1 only. The key is for local tests. Never reuse it.
# Needs openssl (Git for Windows ships it). Run from any folder.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -subj "/CN=127.0.0.1" \
  -addext "subjectAltName=IP:127.0.0.1" \
  -keyout "$here/key.pem" -out "$here/cert.pem"
echo "wrote $here/key.pem and $here/cert.pem"
