#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")"
aidoku package .
aidoku verify package.aix
