/**
 * SpiteStack Rate Limiting Module
 *
 * In-memory rate limiting with exponential backoff for security-sensitive endpoints.
 * Tracks attempts by key (IP, email, etc.) and enforces configurable limits.
 */

export interface RateLimitConfig {
  /** Maximum number of attempts allowed within the window */
  maxAttempts: number;
  /** Time window in milliseconds */
  windowMs: number;
  /** Lockout duration in milliseconds after max attempts exceeded */
  lockoutMs: number;
  /** Enable exponential backoff on lockout */
  exponentialBackoff?: boolean;
}

interface RateLimitEntry {
  attempts: number;
  firstAttempt: number;
  lockedUntil: number;
  lockoutCount: number;
}

/**
 * In-memory rate limiter.
 *
 * Note: This implementation resets on server restart. For distributed systems,
 * consider using Redis or a similar external store.
 */
export class RateLimiter {
  private entries = new Map<string, RateLimitEntry>();
  private config: Required<RateLimitConfig>;
  private cleanupInterval: ReturnType<typeof setInterval> | null = null;

  constructor(config: RateLimitConfig) {
    this.config = {
      maxAttempts: config.maxAttempts,
      windowMs: config.windowMs,
      lockoutMs: config.lockoutMs,
      exponentialBackoff: config.exponentialBackoff ?? true,
    };

    // Cleanup expired entries every minute
    this.cleanupInterval = setInterval(() => this.cleanup(), 60_000);
  }

  /**
   * Check if a key is currently rate limited.
   * Returns the number of milliseconds until the limit resets, or 0 if not limited.
   */
  check(key: string): { allowed: boolean; retryAfterMs: number; remaining: number } {
    const now = Date.now();
    const entry = this.entries.get(key);

    if (!entry) {
      return { allowed: true, retryAfterMs: 0, remaining: this.config.maxAttempts };
    }

    // Check if locked out
    if (entry.lockedUntil > now) {
      return {
        allowed: false,
        retryAfterMs: entry.lockedUntil - now,
        remaining: 0,
      };
    }

    // Check if window has expired
    if (now - entry.firstAttempt > this.config.windowMs) {
      // Reset the entry but preserve lockout count for exponential backoff
      entry.attempts = 0;
      entry.firstAttempt = now;
      return { allowed: true, retryAfterMs: 0, remaining: this.config.maxAttempts };
    }

    // Check if within limits
    const remaining = this.config.maxAttempts - entry.attempts;
    if (remaining > 0) {
      return { allowed: true, retryAfterMs: 0, remaining };
    }

    // Calculate lockout duration with exponential backoff
    const lockoutDuration = this.config.exponentialBackoff
      ? this.config.lockoutMs * Math.pow(2, entry.lockoutCount)
      : this.config.lockoutMs;

    return {
      allowed: false,
      retryAfterMs: lockoutDuration,
      remaining: 0,
    };
  }

  /**
   * Record an attempt for a key.
   * Call this after each login/verification attempt.
   */
  recordAttempt(key: string, success: boolean = false): void {
    const now = Date.now();
    let entry = this.entries.get(key);

    if (!entry) {
      entry = {
        attempts: 0,
        firstAttempt: now,
        lockedUntil: 0,
        lockoutCount: 0,
      };
      this.entries.set(key, entry);
    }

    // If window expired, reset
    if (now - entry.firstAttempt > this.config.windowMs) {
      entry.attempts = 0;
      entry.firstAttempt = now;
    }

    // On successful attempt, reset everything
    if (success) {
      this.entries.delete(key);
      return;
    }

    entry.attempts++;

    // Check if we need to trigger lockout
    if (entry.attempts >= this.config.maxAttempts) {
      const lockoutDuration = this.config.exponentialBackoff
        ? this.config.lockoutMs * Math.pow(2, entry.lockoutCount)
        : this.config.lockoutMs;

      entry.lockedUntil = now + lockoutDuration;
      entry.lockoutCount++;
    }
  }

  /**
   * Reset rate limit for a key.
   * Call this after successful authentication.
   */
  reset(key: string): void {
    this.entries.delete(key);
  }

  /**
   * Get composite key for IP + identifier rate limiting.
   */
  static compositeKey(ip: string, identifier: string): string {
    return `${ip}:${identifier}`;
  }

  /**
   * Extract client IP from request headers with proxy spoofing protection.
   *
   * SECURITY: Default mode is 'none' (secure by default). Proxy headers can be
   * spoofed by attackers to bypass rate limiting. Only trust proxy headers if
   * you are behind a properly configured reverse proxy.
   *
   * Configure TRUSTED_PROXY_MODE environment variable:
   * - 'none' (default): Don't trust any proxy headers - most secure
   * - 'cloudflare': Only trust cf-connecting-ip (Cloudflare strips client headers)
   * - 'proxy': Trust x-forwarded-for and x-real-ip (use behind nginx/etc)
   * - 'auto': Intelligent fallback (legacy, not recommended)
   */
  static getClientIp(req: Request): string {
    const trustedProxyMode = process.env.TRUSTED_PROXY_MODE || 'none';

    // Cloudflare mode: only trust Cloudflare headers
    if (trustedProxyMode === 'cloudflare') {
      const cfIp = req.headers.get('cf-connecting-ip');
      if (cfIp) return cfIp;
      return 'unknown';
    }

    // None mode: don't trust any proxy headers
    if (trustedProxyMode === 'none') {
      return 'unknown';
    }

    // Auto mode or proxy mode: use trust hierarchy
    // 1. Cloudflare header (most trusted - they strip client-sent headers)
    const cfIp = req.headers.get('cf-connecting-ip');
    if (cfIp) return cfIp;

    // 2. Check x-forwarded-for with multiple hops
    const forwarded = req.headers.get('x-forwarded-for');
    if (forwarded) {
      const ips = forwarded.split(',').map(ip => ip.trim());

      // If there are multiple IPs, the proxy chain added entries
      // The first IP is the original client (or first proxy they control)
      if (ips.length > 1) {
        return ips[0];
      }

      // Single IP in x-forwarded-for: could be spoofed
      // In auto mode, fall through to check other headers
      // In proxy mode (explicit trust), use it
      if (trustedProxyMode === 'proxy') {
        return ips[0];
      }
    }

    // 3. x-real-ip is typically set by reverse proxies like nginx
    const realIp = req.headers.get('x-real-ip');
    if (realIp) {
      // In auto mode, only trust if it looks like a private proxy setup
      // (i.e., we also see x-forwarded-for or cloudflare headers)
      if (trustedProxyMode === 'proxy' || forwarded || cfIp) {
        return realIp;
      }
    }

    // 4. Fallback to single x-forwarded-for if nothing else available
    if (forwarded) {
      const ips = forwarded.split(',').map(ip => ip.trim());
      // Log warning in development about potential spoofing
      if (process.env.NODE_ENV !== 'production') {
        console.warn(`[rate-limit] Using single x-forwarded-for IP (${ips[0]}) - may be spoofed. Configure TRUSTED_PROXY_MODE for production.`);
      }
      return ips[0];
    }

    // No proxy headers found - fallback
    return 'unknown';
  }

  /**
   * Clean up expired entries to prevent memory leaks.
   */
  private cleanup(): void {
    const now = Date.now();
    const maxAge = this.config.windowMs + this.config.lockoutMs * 8; // Account for exponential backoff

    for (const [key, entry] of this.entries) {
      if (now - entry.firstAttempt > maxAge && entry.lockedUntil < now) {
        this.entries.delete(key);
      }
    }
  }

  /**
   * Stop the cleanup interval (for graceful shutdown).
   */
  destroy(): void {
    if (this.cleanupInterval) {
      clearInterval(this.cleanupInterval);
      this.cleanupInterval = null;
    }
  }

  /**
   * Get current stats for monitoring.
   */
  getStats(): { totalKeys: number; lockedKeys: number } {
    const now = Date.now();
    let lockedKeys = 0;

    for (const entry of this.entries.values()) {
      if (entry.lockedUntil > now) {
        lockedKeys++;
      }
    }

    return {
      totalKeys: this.entries.size,
      lockedKeys,
    };
  }
}

// Pre-configured rate limiters for common security scenarios

/** Login rate limiter: 5 attempts per 15 minutes, 15 minute lockout */
export const loginRateLimiter = new RateLimiter({
  maxAttempts: 5,
  windowMs: 15 * 60 * 1000, // 15 minutes
  lockoutMs: 15 * 60 * 1000, // 15 minutes base lockout
  exponentialBackoff: true,
});

/** MFA verification rate limiter: 5 attempts per 5 minutes, 5 minute lockout */
export const mfaRateLimiter = new RateLimiter({
  maxAttempts: 5,
  windowMs: 5 * 60 * 1000, // 5 minutes
  lockoutMs: 5 * 60 * 1000, // 5 minutes base lockout
  exponentialBackoff: true,
});

/** Email verification rate limiter: 5 attempts per hour, 1 hour lockout */
export const emailVerificationRateLimiter = new RateLimiter({
  maxAttempts: 5,
  windowMs: 60 * 60 * 1000, // 1 hour
  lockoutMs: 60 * 60 * 1000, // 1 hour lockout
  exponentialBackoff: false,
});

/** Password recovery rate limiter: 3 attempts per hour, 1 hour lockout */
export const passwordRecoveryRateLimiter = new RateLimiter({
  maxAttempts: 3,
  windowMs: 60 * 60 * 1000, // 1 hour
  lockoutMs: 60 * 60 * 1000, // 1 hour lockout
  exponentialBackoff: false,
});

/** Registration rate limiter: 5 registrations per hour per IP */
export const registrationRateLimiter = new RateLimiter({
  maxAttempts: 5,
  windowMs: 60 * 60 * 1000, // 1 hour
  lockoutMs: 60 * 60 * 1000, // 1 hour lockout
  exponentialBackoff: false,
});

/**
 * Create a rate limit error response.
 */
export function rateLimitResponse(retryAfterMs: number): Response {
  const retryAfterSec = Math.ceil(retryAfterMs / 1000);
  return new Response(
    JSON.stringify({
      error: 'Too many attempts',
      retryAfter: retryAfterSec,
      message: `Please try again in ${formatDuration(retryAfterMs)}`,
    }),
    {
      status: 429,
      headers: {
        'Content-Type': 'application/json',
        'Retry-After': retryAfterSec.toString(),
      },
    }
  );
}

/**
 * Format milliseconds as human-readable duration.
 */
function formatDuration(ms: number): string {
  const seconds = Math.ceil(ms / 1000);
  if (seconds < 60) return `${seconds} seconds`;
  const minutes = Math.ceil(seconds / 60);
  if (minutes < 60) return `${minutes} minute${minutes > 1 ? 's' : ''}`;
  const hours = Math.ceil(minutes / 60);
  return `${hours} hour${hours > 1 ? 's' : ''}`;
}
