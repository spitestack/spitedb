#!/bin/bash
#
# Find Maximum Sustained Throughput
#
# Uses binary search to find the highest request rate where p99 latency
# stays under the target (default: 60ms) for sustained periods.
#
# Usage:
#   ./bench/find-max-throughput.sh
#   ./bench/find-max-throughput.sh --min 500 --max 10000 --target-p99 60
#   ./bench/find-max-throughput.sh --url http://localhost:3000 --duration 1m
#

set -e

# Default values
MIN_RPS=500
MAX_RPS=10000
TARGET_P99=60
DURATION="1m"
BASE_URL="http://localhost:3000"
PRECISION=100  # Stop when range is within this

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --min)
      MIN_RPS="$2"
      shift 2
      ;;
    --max)
      MAX_RPS="$2"
      shift 2
      ;;
    --target-p99)
      TARGET_P99="$2"
      shift 2
      ;;
    --duration)
      DURATION="$2"
      shift 2
      ;;
    --url)
      BASE_URL="$2"
      shift 2
      ;;
    --precision)
      PRECISION="$2"
      shift 2
      ;;
    -h|--help)
      echo "Usage: $0 [options]"
      echo ""
      echo "Options:"
      echo "  --min <rps>        Minimum RPS to test (default: 500)"
      echo "  --max <rps>        Maximum RPS to test (default: 10000)"
      echo "  --target-p99 <ms>  Target p99 latency in ms (default: 60)"
      echo "  --duration <dur>   Duration per test (default: 1m)"
      echo "  --url <url>        Server URL (default: http://localhost:3000)"
      echo "  --precision <rps>  Stop when range is within this (default: 100)"
      echo ""
      echo "Example:"
      echo "  $0 --min 1000 --max 5000 --target-p99 60 --duration 30s"
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Check if k6 is installed
if ! command -v k6 &> /dev/null; then
  echo "Error: k6 is not installed."
  echo "Install with: brew install k6"
  exit 1
fi

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOAD_TEST_SCRIPT="$SCRIPT_DIR/http-load-test.js"

if [ ! -f "$LOAD_TEST_SCRIPT" ]; then
  echo "Error: Load test script not found: $LOAD_TEST_SCRIPT"
  exit 1
fi

# Create results directory
mkdir -p "$SCRIPT_DIR/results"

echo "================================================================================"
echo "                   SpiteStack Max Throughput Finder"
echo "================================================================================"
echo ""
echo "Configuration:"
echo "  Search range: $MIN_RPS - $MAX_RPS req/sec"
echo "  Target p99:   <${TARGET_P99}ms"
echo "  Duration:     $DURATION per test"
echo "  Server:       $BASE_URL"
echo "  Precision:    $PRECISION req/sec"
echo ""

# Binary search
LAST_PASSING_RPS=0
ITERATION=0

while [ $((MAX_RPS - MIN_RPS)) -gt $PRECISION ]; do
  ITERATION=$((ITERATION + 1))
  MID_RPS=$(( (MIN_RPS + MAX_RPS) / 2 ))

  echo "--------------------------------------------------------------------------------"
  echo "Iteration $ITERATION: Testing at $MID_RPS req/sec..."
  echo "  Range: [$MIN_RPS, $MAX_RPS]"
  echo ""

  # Run k6 and capture output
  RESULT_FILE="$SCRIPT_DIR/results/iteration-$ITERATION.json"

  k6 run "$LOAD_TEST_SCRIPT" \
    -e "RPS=$MID_RPS" \
    -e "DURATION=$DURATION" \
    -e "BASE_URL=$BASE_URL" \
    -e "TARGET_P99=$TARGET_P99" \
    --quiet 2>&1 || true

  # Parse the result
  if [ -f "$SCRIPT_DIR/results/latest.json" ]; then
    cp "$SCRIPT_DIR/results/latest.json" "$RESULT_FILE"
    P99=$(cat "$RESULT_FILE" | grep -o '"p99":[^,]*' | cut -d: -f2 | tr -d ' ')
    PASS=$(cat "$RESULT_FILE" | grep -o '"pass":[^,}]*' | cut -d: -f2 | tr -d ' ')

    echo "  Result: p99 = ${P99}ms"

    if [ "$PASS" = "true" ]; then
      echo "  Status: PASS (p99 < ${TARGET_P99}ms) -> increase load"
      MIN_RPS=$MID_RPS
      LAST_PASSING_RPS=$MID_RPS
    else
      echo "  Status: FAIL (p99 >= ${TARGET_P99}ms) -> decrease load"
      MAX_RPS=$MID_RPS
    fi
  else
    echo "  Warning: Could not read results, decreasing load"
    MAX_RPS=$MID_RPS
  fi

  echo ""
done

echo "================================================================================"
echo "                              FINAL RESULT"
echo "================================================================================"
echo ""
if [ $LAST_PASSING_RPS -gt 0 ]; then
  echo "  Max sustained throughput: $LAST_PASSING_RPS req/sec"
  echo "  At p99 latency:           <${TARGET_P99}ms"
else
  echo "  Could not find a passing rate in the range [$MIN_RPS, $MAX_RPS]"
  echo "  Try lowering --min or increasing --target-p99"
fi
echo ""
echo "  Results saved to: $SCRIPT_DIR/results/"
echo ""
echo "================================================================================"
