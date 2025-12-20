/**
 * SpiteStack Example Server
 *
 * Environment variables:
 *   DB_PATH - Path to SpiteDB database file (default: ./data/events.db)
 *   PORT    - Server port (default: 3000)
 */

import { mkdir } from "node:fs/promises";
import { dirname } from "node:path";
import { SpiteDbNapi } from "@spitestack/db";
import { createCommandHandler } from "./.spitestack/generated/routes";

const DB_PATH = process.env.DB_PATH || "./data/events.db";
const PORT = parseInt(process.env.PORT || "3000", 10);

// Ensure data directory exists
await mkdir(dirname(DB_PATH), { recursive: true });

// Initialize SpiteDB
console.log(`Opening database: ${DB_PATH}`);
const db = await SpiteDbNapi.open(DB_PATH);

const handler = createCommandHandler({ db });

const server = Bun.serve({
  port: PORT,
  fetch: handler,
});

console.log(`Server running at http://localhost:${server.port}`);

// Admission control metrics logging for profiling
setInterval(() => {
  const metrics = db.getAdmissionMetrics();
  console.log(`[metrics] ${JSON.stringify({
    ts: Date.now(),
    limit: metrics.currentLimit,
    p99: metrics.observedP99Ms.toFixed(2),
    target: metrics.targetP99Ms,
    accepted: metrics.requestsAccepted,
    rejected: metrics.requestsRejected,
    rejRate: metrics.rejectionRate.toFixed(4),
    adj: metrics.adjustments,
  })}`);
}, 2000);

// Graceful shutdown
process.on("SIGINT", () => {
  console.log("\nShutting down...");
  process.exit(0);
});

process.on("SIGTERM", () => {
  console.log("\nShutting down...");
  process.exit(0);
});
