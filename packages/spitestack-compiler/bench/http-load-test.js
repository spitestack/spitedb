/**
 * k6 Load Test Script for SpiteStack
 *
 * Tests sustained HTTP throughput while maintaining p99 latency target.
 *
 * Usage:
 *   k6 run bench/http-load-test.js -e RPS=1000 -e DURATION=1m
 *   k6 run bench/http-load-test.js -e RPS=2000 -e DURATION=2m -e BASE_URL=http://localhost:3000
 *
 * The script sends TODO create commands at a constant rate and measures latency.
 */

import http from "k6/http";
import { check } from "k6";
import { Counter, Trend } from "k6/metrics";

// Custom metrics
const successfulRequests = new Counter("successful_requests");
const failedRequests = new Counter("failed_requests");
const requestLatency = new Trend("request_latency", true);

// Configuration from environment variables
const RPS = parseInt(__ENV.RPS || "1000");
const DURATION = __ENV.DURATION || "1m";
const BASE_URL = __ENV.BASE_URL || "http://localhost:3000";
const TARGET_P99 = parseInt(__ENV.TARGET_P99 || "60");

export const options = {
  scenarios: {
    constant_load: {
      executor: "constant-arrival-rate",
      rate: RPS,
      timeUnit: "1s",
      duration: DURATION,
      preAllocatedVUs: Math.min(RPS, 100),
      maxVUs: Math.max(RPS * 2, 500),
    },
  },
  thresholds: {
    http_req_failed: ["rate<0.01"], // Error rate < 1%
    http_req_duration: [`p(99)<${TARGET_P99}`], // p99 latency target
  },
};

// Generate UUIDv7-like IDs (timestamp-based UUIDs)
let requestCounter = 0;
function generateUUIDv7() {
  const timestamp = Date.now();
  const counter = requestCounter++;

  // UUIDv7 format: timestamp (48 bits) + version (4 bits) + random (12 bits) + variant (2 bits) + random (62 bits)
  const timestampHex = timestamp.toString(16).padStart(12, '0');
  const randomPart1 = (Math.random() * 0xfff | 0).toString(16).padStart(3, '0');
  const randomPart2 = ((Math.random() * 0x3fff | 0) | 0x8000).toString(16).padStart(4, '0'); // variant bits
  const randomPart3 = Math.random().toString(16).substring(2, 14).padStart(12, '0');

  // Format: xxxxxxxx-xxxx-7xxx-yxxx-xxxxxxxxxxxx
  return `${timestampHex.substring(0, 8)}-${timestampHex.substring(8, 12)}-7${randomPart1}-${randomPart2}-${randomPart3}`;
}

function generateId() {
  return generateUUIDv7();
}

export default function () {
  const todoId = generateId();
  const sessionId = `bench-session-${__VU}`;
  const commandId = generateUUIDv7();

  const payload = JSON.stringify({
    id: todoId,
    title: `Load test todo ${Date.now()}`,
  });

  const params = {
    headers: {
      "Content-Type": "application/json",
      "x-session-id": sessionId,
      "x-command-id": commandId,
    },
  };

  const startTime = Date.now();
  const res = http.post(`${BASE_URL}/api/todo/create`, payload, params);
  const duration = Date.now() - startTime;

  requestLatency.add(duration);

  const success = check(res, {
    "status is 200": (r) => r.status === 200,
    "response has aggregateId": (r) => {
      try {
        const body = JSON.parse(r.body);
        return body && body.aggregateId !== undefined;
      } catch {
        return false;
      }
    },
  });

  if (success) {
    successfulRequests.add(1);
  } else {
    failedRequests.add(1);
  }
}

export function handleSummary(data) {
  const p99 = data.metrics.http_req_duration?.values?.["p(99)"] ?? 0;
  const p95 = data.metrics.http_req_duration?.values?.["p(95)"] ?? 0;
  const p50 = data.metrics.http_req_duration?.values?.["p(50)"] ?? 0;
  const avgLatency = data.metrics.http_req_duration?.values?.avg ?? 0;
  const totalRequests = data.metrics.http_reqs?.values?.count ?? 0;
  const failedCount = data.metrics.http_req_failed?.values?.passes ?? 0;
  const errorRate = totalRequests > 0 ? (failedCount / totalRequests) * 100 : 0;
  const duration = data.state?.testRunDurationMs ?? 0;
  const actualRps = duration > 0 ? (totalRequests / (duration / 1000)).toFixed(1) : 0;

  const status = p99 < TARGET_P99 ? "PASS" : "FAIL";
  const statusSymbol = p99 < TARGET_P99 ? "\u2713" : "\u2717";

  console.log(`
================================================================================
                        SpiteStack Load Test Results
================================================================================

Target: ${RPS} req/sec for ${DURATION}
Actual: ${actualRps} req/sec

Latency:
  p50:  ${p50.toFixed(2)}ms
  p95:  ${p95.toFixed(2)}ms
  p99:  ${p99.toFixed(2)}ms  (target: <${TARGET_P99}ms)
  avg:  ${avgLatency.toFixed(2)}ms

Requests:
  Total:   ${totalRequests}
  Success: ${totalRequests - failedCount}
  Failed:  ${failedCount} (${errorRate.toFixed(2)}%)

Status: ${statusSymbol} ${status} (p99 ${p99.toFixed(2)}ms ${p99 < TARGET_P99 ? "<" : ">="} ${TARGET_P99}ms target)

================================================================================
`);

  // Return JSON for scripting
  return {
    stdout: "", // Suppress default k6 output
    "bench/results/latest.json": JSON.stringify(
      {
        rps: RPS,
        actualRps: parseFloat(actualRps),
        duration: DURATION,
        p50: p50,
        p95: p95,
        p99: p99,
        avgLatency: avgLatency,
        totalRequests: totalRequests,
        failedRequests: failedCount,
        errorRate: errorRate,
        pass: p99 < TARGET_P99,
        timestamp: new Date().toISOString(),
      },
      null,
      2
    ),
  };
}
