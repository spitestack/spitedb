/**
 * SpiteStack Security Headers Module
 *
 * Provides standard security headers for HTTP responses.
 * Follows OWASP best practices and supports SOC 2/HIPAA compliance.
 */

export interface SecurityHeadersConfig {
  /** Enable Strict-Transport-Security (HSTS) - only set in production */
  enableHSTS?: boolean;
  /** HSTS max-age in seconds (default: 1 year) */
  hstsMaxAge?: number;
  /** Enable X-Frame-Options */
  enableFrameOptions?: boolean;
  /** X-Frame-Options value: 'DENY' or 'SAMEORIGIN' */
  frameOptions?: 'DENY' | 'SAMEORIGIN';
  /** Enable X-Content-Type-Options */
  enableContentTypeOptions?: boolean;
  /** Enable X-XSS-Protection (legacy, but still useful for older browsers) */
  enableXSSProtection?: boolean;
  /** Enable Referrer-Policy */
  enableReferrerPolicy?: boolean;
  /** Referrer-Policy value */
  referrerPolicy?: string;
  /** Custom Content-Security-Policy */
  contentSecurityPolicy?: string;
  /** Enable Permissions-Policy */
  enablePermissionsPolicy?: boolean;
  /** Permissions-Policy value */
  permissionsPolicy?: string;
}

const DEFAULT_CONFIG: Required<SecurityHeadersConfig> = {
  enableHSTS: true,
  hstsMaxAge: 31536000, // 1 year
  enableFrameOptions: true,
  frameOptions: 'DENY',
  enableContentTypeOptions: true,
  enableXSSProtection: true,
  enableReferrerPolicy: true,
  referrerPolicy: 'strict-origin-when-cross-origin',
  contentSecurityPolicy: "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self'; connect-src 'self'; frame-ancestors 'none';",
  enablePermissionsPolicy: true,
  permissionsPolicy: 'camera=(), microphone=(), geolocation=(), payment=()',
};

/**
 * Generate security headers based on configuration.
 */
export function getSecurityHeaders(
  config: SecurityHeadersConfig = {}
): Record<string, string> {
  const cfg = { ...DEFAULT_CONFIG, ...config };
  const isProd = process.env.NODE_ENV === 'production';
  const headers: Record<string, string> = {};

  // HSTS - only in production (requires HTTPS)
  if (cfg.enableHSTS && isProd) {
    headers['Strict-Transport-Security'] = `max-age=${cfg.hstsMaxAge}; includeSubDomains; preload`;
  }

  // Prevent clickjacking
  if (cfg.enableFrameOptions) {
    headers['X-Frame-Options'] = cfg.frameOptions;
  }

  // Prevent MIME type sniffing
  if (cfg.enableContentTypeOptions) {
    headers['X-Content-Type-Options'] = 'nosniff';
  }

  // XSS Protection (legacy, but still useful)
  if (cfg.enableXSSProtection) {
    headers['X-XSS-Protection'] = '1; mode=block';
  }

  // Referrer Policy
  if (cfg.enableReferrerPolicy) {
    headers['Referrer-Policy'] = cfg.referrerPolicy;
  }

  // Content Security Policy
  if (cfg.contentSecurityPolicy) {
    headers['Content-Security-Policy'] = cfg.contentSecurityPolicy;
  }

  // Permissions Policy (formerly Feature-Policy)
  if (cfg.enablePermissionsPolicy && cfg.permissionsPolicy) {
    headers['Permissions-Policy'] = cfg.permissionsPolicy;
  }

  // Additional security headers
  headers['X-DNS-Prefetch-Control'] = 'off';
  headers['X-Download-Options'] = 'noopen';
  headers['X-Permitted-Cross-Domain-Policies'] = 'none';

  return headers;
}

/**
 * Apply security headers to a Response object.
 */
export function applySecurityHeaders(
  response: Response,
  config?: SecurityHeadersConfig
): Response {
  const securityHeaders = getSecurityHeaders(config);
  const newHeaders = new Headers(response.headers);

  for (const [key, value] of Object.entries(securityHeaders)) {
    // Don't override existing headers
    if (!newHeaders.has(key)) {
      newHeaders.set(key, value);
    }
  }

  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers: newHeaders,
  });
}

/**
 * Create a middleware function that applies security headers.
 */
export function createSecurityHeadersMiddleware(config?: SecurityHeadersConfig) {
  return (response: Response): Response => {
    return applySecurityHeaders(response, config);
  };
}

/**
 * CSRF Token management for cookie-based authentication.
 */
export class CsrfProtection {
  private static readonly CSRF_COOKIE_NAME = 'spite_csrf_token';
  private static readonly CSRF_HEADER_NAME = 'X-CSRF-Token';
  private static readonly TOKEN_LENGTH = 32;

  /**
   * Generate a cryptographically secure CSRF token.
   */
  static generateToken(): string {
    const bytes = crypto.getRandomValues(new Uint8Array(this.TOKEN_LENGTH));
    return Array.from(bytes)
      .map(b => b.toString(16).padStart(2, '0'))
      .join('');
  }

  /**
   * Create a Set-Cookie header for the CSRF token.
   */
  static createCsrfCookie(token: string): string {
    const isProd = process.env.NODE_ENV === 'production';
    const secureCookieFlag = isProd ? '; Secure' : '';
    return `${this.CSRF_COOKIE_NAME}=${token}; Path=/; SameSite=Strict; Max-Age=86400${secureCookieFlag}`;
  }

  /**
   * Validate CSRF token from request.
   * Token must be present in both cookie and header, and they must match.
   */
  static validateRequest(req: Request): { valid: boolean; error?: string } {
    // GET, HEAD, OPTIONS don't need CSRF protection
    const method = req.method.toUpperCase();
    if (['GET', 'HEAD', 'OPTIONS'].includes(method)) {
      return { valid: true };
    }

    const cookieHeader = req.headers.get('Cookie') || '';
    const getCookie = (name: string) =>
      cookieHeader.split(';').find(c => c.trim().startsWith(name + '='))?.split('=')[1];

    const cookieToken = getCookie(this.CSRF_COOKIE_NAME);
    const headerToken = req.headers.get(this.CSRF_HEADER_NAME);

    if (!cookieToken) {
      return { valid: false, error: 'Missing CSRF cookie' };
    }

    if (!headerToken) {
      return { valid: false, error: 'Missing CSRF header' };
    }

    // Constant-time comparison to prevent timing attacks
    if (!this.constantTimeCompare(cookieToken, headerToken)) {
      return { valid: false, error: 'CSRF token mismatch' };
    }

    return { valid: true };
  }

  /**
   * Constant-time string comparison to prevent timing attacks.
   */
  private static constantTimeCompare(a: string, b: string): boolean {
    if (a.length !== b.length) {
      return false;
    }

    let result = 0;
    for (let i = 0; i < a.length; i++) {
      result |= a.charCodeAt(i) ^ b.charCodeAt(i);
    }
    return result === 0;
  }

  /**
   * Create a CSRF error response.
   */
  static errorResponse(error: string): Response {
    return new Response(
      JSON.stringify({
        error: 'CSRF validation failed',
        message: error,
      }),
      {
        status: 403,
        headers: { 'Content-Type': 'application/json' },
      }
    );
  }
}

/**
 * API-specific Content Security Policy (more permissive for API endpoints).
 */
export const API_CSP = "default-src 'none'; frame-ancestors 'none';";

/**
 * Web app Content Security Policy template.
 */
export function createWebAppCSP(options: {
  scriptSrc?: string[];
  styleSrc?: string[];
  imgSrc?: string[];
  connectSrc?: string[];
  fontSrc?: string[];
  nonce?: string;
}): string {
  const parts: string[] = ["default-src 'self'"];

  // Script sources
  const scriptSources = ["'self'", ...(options.scriptSrc || [])];
  if (options.nonce) {
    scriptSources.push(`'nonce-${options.nonce}'`);
  }
  parts.push(`script-src ${scriptSources.join(' ')}`);

  // Style sources
  const styleSources = ["'self'", "'unsafe-inline'", ...(options.styleSrc || [])];
  parts.push(`style-src ${styleSources.join(' ')}`);

  // Image sources
  const imgSources = ["'self'", 'data:', ...(options.imgSrc || [])];
  parts.push(`img-src ${imgSources.join(' ')}`);

  // Connect sources (for fetch/XHR)
  const connectSources = ["'self'", ...(options.connectSrc || [])];
  parts.push(`connect-src ${connectSources.join(' ')}`);

  // Font sources
  const fontSources = ["'self'", ...(options.fontSrc || [])];
  parts.push(`font-src ${fontSources.join(' ')}`);

  // Prevent framing
  parts.push("frame-ancestors 'none'");

  // Upgrade insecure requests in production
  if (process.env.NODE_ENV === 'production') {
    parts.push('upgrade-insecure-requests');
  }

  return parts.join('; ');
}
