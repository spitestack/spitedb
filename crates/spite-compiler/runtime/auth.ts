/**
 * SpiteStack Auth Module
 *
 * Provides JWT-based authentication middleware for Bun handlers.
 */

export type AuthConfig = {
  secret: string;
  issuer?: string;
  audience?: string;
  expiresIn?: number; // seconds, default 900 (15min)
  refreshExpiresIn?: number; // seconds, default 604800 (7d)
  maxSessionDuration?: number; // seconds, default 2592000 (30d) - Absolute max session life
};

export type AuthUser = {
  sub: string;
  sid?: string; // Session ID
  jti?: string; // JWT ID for token revocation
  firstIat?: number; // Timestamp of original login (for absolute timeout)
  tenant?: string;
  roles?: string[];
  permissions?: string[];

  orgs?: {
    [tenantId: string]: {
      roles: string[];
      permissions: string[];
    };
  };

  mfa_pending?: boolean;
  pwd_change?: boolean;

  [key: string]: unknown;
};

export type AuthResult =
  | { ok: true; user: AuthUser }
  | { ok: false; error: string };

/**
 * Resolves the effective tenant for a request.
 */
export function resolveTenant(req: Request, user: AuthUser): { ok: true; tenant: string } | { ok: false; error: string; status: number } {
  const headerTenant = req.headers.get('x-tenant-id');

  if (headerTenant) {
    if (user.orgs?.[headerTenant]) {
      return { ok: true, tenant: headerTenant };
    }
    if (user.tenant === headerTenant) {
      return { ok: true, tenant: headerTenant };
    }
    return { ok: false, error: 'Access denied to requested tenant', status: 403 };
  }

  if (user.tenant) {
    return { ok: true, tenant: user.tenant };
  }

  const orgIds = Object.keys(user.orgs || {});
  if (orgIds.length === 1) {
    return { ok: true, tenant: orgIds[0] };
  }

  return { ok: false, error: 'Tenant ID required (X-Tenant-ID header)', status: 400 };
}

/**
 * Creates an auth middleware that validates JWT tokens.
 */
export function createAuth(config: AuthConfig) {
  const encoder = new TextEncoder();
  const secretKey = encoder.encode(config.secret);

  let cachedKey: CryptoKey | null = null;

  async function getKey(): Promise<CryptoKey> {
    if (!cachedKey) {
      cachedKey = await crypto.subtle.importKey(
        'raw',
        secretKey,
        { name: 'HMAC', hash: 'SHA-256' },
        false,
        ['sign', 'verify']
      );
    }
    return cachedKey;
  }

  async function verify(token: string): Promise<AuthResult> {
    try {
      const parts = token.split('.');
      if (parts.length !== 3) return { ok: false, error: 'Invalid token format' };
      const [headerB64, payloadB64, signatureB64] = parts;

      const data = encoder.encode(`${headerB64}.${payloadB64}`);
      const signature = base64UrlDecode(signatureB64);
      const key = await getKey();
      
      const valid = await crypto.subtle.verify('HMAC', key, signature, data);
      if (!valid) return { ok: false, error: 'Invalid signature' };

      // Use proper base64url decoding for payload
      const payloadJson = new TextDecoder().decode(base64UrlDecode(payloadB64));
      const payload = JSON.parse(payloadJson) as AuthUser & { exp?: number; iss?: string; aud?: string; typ?: string };

      if (payload.exp && payload.exp < Date.now() / 1000) {
        return { ok: false, error: 'Token expired' };
      }

      if (config.issuer && payload.iss !== config.issuer) {
        return { ok: false, error: 'Invalid issuer' };
      }

      if (config.audience && payload.aud !== config.audience) {
        return { ok: false, error: 'Invalid audience' };
      }

      return { ok: true, user: payload };
    } catch {
      return { ok: false, error: 'Token verification failed' };
    }
  }

  async function sign(user: AuthUser, type: 'access' | 'refresh' = 'access'): Promise<string> {
    const now = Math.floor(Date.now() / 1000);
    const header = { alg: 'HS256', typ: 'JWT' };
    const exp = type === 'access'
      ? (config.expiresIn ?? 900)
      : (config.refreshExpiresIn ?? 604800);

    // Preserve original firstIat or set to now if this is a new session
    const firstIat = user.firstIat ?? now;

    // Generate unique token ID for revocation support
    const jti = crypto.randomUUID();

    const payload = {
      ...user,
      jti,
      firstIat,
      typ: type,
      iat: now,
      exp: now + exp,
      ...(config.issuer && { iss: config.issuer }),
      ...(config.audience && { aud: config.audience }),
    };

    const headerB64 = base64UrlEncode(JSON.stringify(header));
    const payloadB64 = base64UrlEncode(JSON.stringify(payload));
    const data = encoder.encode(`${headerB64}.${payloadB64}`);

    const key = await getKey();
    const signature = await crypto.subtle.sign('HMAC', key, data);
    const signatureB64 = base64UrlEncode(String.fromCharCode(...new Uint8Array(signature)));

    return `${headerB64}.${payloadB64}.${signatureB64}`;
  }

  async function verifyRequest(req: Request): Promise<AuthResult> {
    // Try Authorization header first
    const authHeader = req.headers.get('Authorization');
    let token: string | null = null;

    if (authHeader?.startsWith('Bearer ')) {
      token = authHeader.slice(7);
    } else {
      // Fall back to HttpOnly cookie
      const cookieHeader = req.headers.get('Cookie') || '';
      const match = cookieHeader.split(';').find(c => c.trim().startsWith('spite_token='));
      if (match) {
        token = match.split('=')[1]?.trim() || null;
      }
    }

    if (!token) {
      return { ok: false, error: 'Missing or invalid Authorization header' };
    }

    const result = await verify(token);
    if (result.ok) {
      const tokenType = (result.user as any).typ;
      if (tokenType === 'refresh') {
        return { ok: false, error: 'Cannot use refresh token as access token' };
      }
      if (result.user.mfa_pending) {
        return { ok: false, error: 'MFA session token cannot access protected routes' };
      }
      if (result.user.pwd_change) {
        return { ok: false, error: 'Password change required before accessing protected routes' };
      }
    }
    return result;
  }

  async function verifyRefresh(token: string): Promise<AuthResult> {
    const result = await verify(token);
    if (!result.ok) return result;
    
    const user = result.user;
    if ((user as any).typ !== 'refresh') {
      return { ok: false, error: 'Invalid refresh token type' };
    }

    // Check Absolute Session Timeout
    const maxDuration = config.maxSessionDuration ?? 2592000; // Default 30 days
    const now = Math.floor(Date.now() / 1000);
    if (user.firstIat && (now - user.firstIat > maxDuration)) {
      return { ok: false, error: 'Session absolute timeout exceeded' };
    }

    return result;
  }

  /**
   * Check if a user has a specific permission in a tenant.
   */
  function hasPermission(user: AuthUser, tenant: string, permission: string): boolean {
    // Check tenant-specific permissions
    if (user.orgs?.[tenant]?.permissions.includes(permission)) return true;
    // Check default tenant permissions
    if (user.tenant === tenant && user.permissions?.includes(permission)) return true;
    return false;
  }

  /**
   * Check if a user has a specific role in a tenant.
   */
  function hasRole(user: AuthUser, tenant: string, role: string): boolean {
    // Check tenant-specific roles
    if (user.orgs?.[tenant]?.roles.includes(role)) return true;
    // Check default tenant roles
    if (user.tenant === tenant && user.roles?.includes(role)) return true;
    return false;
  }

  return { verify, sign, verifyRequest, verifyRefresh, hasPermission, hasRole };
}

// Base64 URL encoding/decoding helpers
function base64UrlEncode(str: string): string {
  return btoa(str).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function base64UrlDecode(str: string): Uint8Array {
  const base64 = str.replace(/-/g, '+').replace(/_/g, '/');
  const padded = base64 + '='.repeat((4 - (base64.length % 4)) % 4);
  const binary = atob(padded);
  return Uint8Array.from(binary, c => c.charCodeAt(0));
}
