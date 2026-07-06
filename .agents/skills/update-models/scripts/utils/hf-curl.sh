#!/usr/bin/env bash
set -euo pipefail

if [ "${HF_TOKEN+set}" = set ]; then
    curl -s -H "Authorization: Bearer $HF_TOKEN" $@
else
    curl -s $@
fi
