/**
 * Load Generator for SpiteStack Example
 *
 * Generates realistic activity for the Todo domain.
 * Run: bun run scripts/load-gen.ts
 *
 * This script:
 * - Authenticates as admin
 * - Creates todos at configurable rates
 * - Completes some todos randomly
 * - Updates titles occasionally
 * - Generates realistic monitoring data
 */

const BASE_URL = process.env.API_URL || 'http://localhost:3000';
const ADMIN_EMAIL = process.env.ADMIN_EMAIL || 'admin@local';
const ADMIN_PASSWORD = process.env.ADMIN_PASSWORD; // Must be set!

// Configuration - adjust these for different load profiles
const CONFIG = {
  batchSize: 5,              // Create 5 todos per batch
  batchIntervalMs: 10000,    // Every 10 seconds
  completeChance: 0.3,       // 30% of todos get completed
  updateTitleChance: 0.1,    // 10% get title updates
  maxCompletionsPerBatch: 3, // Cap completions per batch
  maxUpdatesPerBatch: 2,     // Cap updates per batch
};

let todoIds: string[] = [];
let cookies = '';
let totalCreated = 0;
let totalCompleted = 0;

async function login(): Promise<void> {
  if (!ADMIN_PASSWORD) {
    console.error('ADMIN_PASSWORD environment variable is required');
    console.error('Example: ADMIN_PASSWORD=yourpassword bun run scripts/load-gen.ts');
    process.exit(1);
  }

  console.log(`Logging in as ${ADMIN_EMAIL}...`);

  const res = await fetch(`${BASE_URL}/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email: ADMIN_EMAIL, password: ADMIN_PASSWORD }),
  });

  const setCookie = res.headers.get('set-cookie');
  if (setCookie) {
    // Parse multiple cookies from set-cookie header
    cookies = setCookie.split(',').map(c => c.split(';')[0]).join('; ');
  }

  const data = await res.json();

  if (data.status === 'password_change_required') {
    console.error('Password change required. Please log in via the admin dashboard first.');
    process.exit(1);
  }

  if (data.status !== 'success') {
    console.error(`Login failed: ${data.error || data.message || 'unknown error'}`);
    process.exit(1);
  }

  console.log('Logged in successfully');
}

async function createTodo(title: string): Promise<string> {
  const id = crypto.randomUUID();

  const res = await fetch(`${BASE_URL}/api/Todo/${id}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Cookie': cookies,
      'X-Action': 'Create',
    },
    body: JSON.stringify({ id, title }),
  });

  if (!res.ok) {
    const text = await res.text();
    throw new Error(`Create failed (${res.status}): ${text}`);
  }

  return id;
}

async function completeTodo(id: string): Promise<void> {
  const res = await fetch(`${BASE_URL}/api/Todo/${id}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Cookie': cookies,
      'X-Action': 'Complete',
    },
    body: JSON.stringify({}),
  });

  if (!res.ok) {
    const text = await res.text();
    throw new Error(`Complete failed (${res.status}): ${text}`);
  }
}

async function updateTitle(id: string, newTitle: string): Promise<void> {
  const res = await fetch(`${BASE_URL}/api/Todo/${id}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Cookie': cookies,
      'X-Action': 'UpdateTitle',
    },
    body: JSON.stringify({ title: newTitle }),
  });

  if (!res.ok) {
    const text = await res.text();
    throw new Error(`UpdateTitle failed (${res.status}): ${text}`);
  }
}

async function runBatch(): Promise<void> {
  const batchStart = Date.now();
  let created = 0;
  let completed = 0;
  let updated = 0;

  // Create new todos
  for (let i = 0; i < CONFIG.batchSize; i++) {
    const title = `${randomTaskName()} #${totalCreated + i + 1}`;
    try {
      const id = await createTodo(title);
      todoIds.push(id);
      created++;
    } catch (err) {
      console.error(`  Failed to create: ${(err as Error).message}`);
    }
  }
  totalCreated += created;

  // Complete some existing todos
  const shuffled = [...todoIds].sort(() => Math.random() - 0.5);
  const toComplete = shuffled
    .filter(() => Math.random() < CONFIG.completeChance)
    .slice(0, CONFIG.maxCompletionsPerBatch);

  for (const id of toComplete) {
    try {
      await completeTodo(id);
      todoIds = todoIds.filter(tid => tid !== id);
      completed++;
      totalCompleted++;
    } catch (err) {
      console.error(`  Failed to complete: ${(err as Error).message}`);
    }
  }

  // Update some titles
  const remaining = [...todoIds].sort(() => Math.random() - 0.5);
  const toUpdate = remaining
    .filter(() => Math.random() < CONFIG.updateTitleChance)
    .slice(0, CONFIG.maxUpdatesPerBatch);

  for (const id of toUpdate) {
    try {
      await updateTitle(id, `Updated: ${randomTaskName()}`);
      updated++;
    } catch (err) {
      console.error(`  Failed to update: ${(err as Error).message}`);
    }
  }

  const elapsed = Date.now() - batchStart;
  console.log(
    `[${new Date().toLocaleTimeString()}] ` +
    `+${created} created, ${completed} completed, ${updated} updated ` +
    `(${todoIds.length} active, ${totalCreated} total) [${elapsed}ms]`
  );
}

const taskVerbs = [
  'Review', 'Fix', 'Implement', 'Test', 'Deploy',
  'Update', 'Refactor', 'Document', 'Debug', 'Optimize'
];

const taskNouns = [
  'login flow', 'database queries', 'API endpoints', 'UI components',
  'error handling', 'caching layer', 'metrics collection', 'auth system',
  'email service', 'notification system', 'search feature', 'billing module'
];

function randomTaskName(): string {
  const verb = taskVerbs[Math.floor(Math.random() * taskVerbs.length)];
  const noun = taskNouns[Math.floor(Math.random() * taskNouns.length)];
  return `${verb} ${noun}`;
}

async function main(): Promise<void> {
  console.log('');
  console.log('SpiteStack Load Generator');
  console.log('=========================');
  console.log(`Target: ${BASE_URL}`);
  console.log(`Batch size: ${CONFIG.batchSize} todos every ${CONFIG.batchIntervalMs / 1000}s`);
  console.log('');

  await login();

  console.log('');
  console.log('Starting load generation (Ctrl+C to stop)...');
  console.log('');

  // Run immediately, then on interval
  await runBatch();

  while (true) {
    await Bun.sleep(CONFIG.batchIntervalMs);
    await runBatch();
  }
}

main().catch((err) => {
  console.error('Fatal error:', err);
  process.exit(1);
});
