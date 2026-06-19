#!/bin/sh
# frontend/entrypoint.sh — generate runtime /config.json before nginx starts.
#
# Environment variables consumed:
#   MEMORYOPS_WORKSPACE_ID   Workspace UUID to inject at runtime.
#                            Overrides the value baked at build time via
#                            VITE_MEMORYOPS_WORKSPACE_ID (dev fallback).
#
# Why runtime config?
#   Baking VITE_MEMORYOPS_WORKSPACE_ID at image build time makes the image
#   workspace-specific.  The runtime approach lets a single image serve any
#   workspace by setting the env var at container start.

set -eu

TARGET_DIR=/tmp/memoryops-runtime
TARGET=${TARGET_DIR}/config.json

WORKSPACE_ID="${MEMORYOPS_WORKSPACE_ID:-}"

# Workspace IDs are UUIDs; validate the format if non-empty so we never emit
# malformed JSON due to unexpected characters.
if [ -n "${WORKSPACE_ID}" ]; then
  # UUID regex: 8-4-4-4-12 hex digits (case-insensitive)
  case "${WORKSPACE_ID}" in
    [0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]-[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]-[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]-[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]-[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F])
      ;;
    *)
      echo "WARNING: MEMORYOPS_WORKSPACE_ID '${WORKSPACE_ID}' is not a valid UUID; ignoring" >&2
      WORKSPACE_ID=""
      ;;
  esac
fi

mkdir -p "${TARGET_DIR}"
printf '{"workspaceId":"%s"}\n' "${WORKSPACE_ID}" > "${TARGET}"
echo "Generated ${TARGET} (workspaceId=${WORKSPACE_ID:-<empty>})"
# Do NOT exec nginx here — this script is invoked by nginx's entrypoint.d
# mechanism, which starts nginx after all scripts in the directory complete.
