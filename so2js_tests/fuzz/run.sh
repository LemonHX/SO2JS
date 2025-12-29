#!/bin/bash

# This script is used to run fuzzilli on so2js

set -e

CURRENT_DIR=$(cd "$(dirname "$0")" && pwd)
ROOT_DIR="$CURRENT_DIR/../.."

FUZZILLI_DIR="$CURRENT_DIR/fuzzilli"
RESULTS_DIR="$CURRENT_DIR/results"

# Build so2js for fuzzing and install dependencies
"$CURRENT_DIR/build.sh"

# Build and run fuzzilli
cd "$FUZZILLI_DIR"

swift run \
  -c release FuzzilliCli \
  --profile=so2js \
  --logLevel=verbose \
  --diagnostics \
  --storagePath="$RESULTS_DIR" \
  --jobs=4 \
  "$@" \
  "$ROOT_DIR/target/debug/so2js_fuzz"