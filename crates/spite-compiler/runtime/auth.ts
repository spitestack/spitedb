/**
 * SpiteStack Auth Module
 *
 * Provides JWT-based authentication middleware for Bun handlers.
 */

export type AuthConfig = {
  secret: string;
  issuer?: string;
  audience?: string;
  expiresIn?: number; // seconds, default 3600
};

export type AuthUser = {
  sub: string;
  tenant: string;
  [key: string]: unknown;
};

export type AuthResult =
  | { ok: true; user: AuthUser }
  | { ok: false; error: string };

/**
 * Creates an auth middleware that validates JWT tokens.
 */
export function createAuth(config: AuthConfig) {
  const encoder = new TextEncoder();
  const secretKey = encoder.encode(config.secret);

  async function verify(token: string): Promise<AuthResult> {
    try {
      const [headerB64, payloadB64, signatureB64] = token.split('.');
      if (!headerB64 || !payloadB64 || !signatureB64) {
        return { ok: false, error: 'Invalid token format' };
      }

      // Verify signature
      const data = encoder.encode(`${headerB64}.${payloadB64}`);
      const signature = base64UrlDecode(signatureB64);
      const key = await crypto.subtle.importKey(
        'raw',
        secretKey,
        { name: 'HMAC', hash: 'SHA-256' },
        false,
        ['verify']
      );
      const valid = await crypto.subtle.verify('HMAC', key, signature as BufferSource, data as BufferSource);
      if (!valid) {
        return { ok: false, error: 'Invalid signature' };
      }

      // Decode payload
      const payload = JSON.parse(atob(payloadB64)) as AuthUser & { exp?: number; iss?: string; aud?: string };

      // Check expiration
      if (payload.exp && payload.exp < Date.now() / 1000) {
        return { ok: false, error: 'Token expired' };
      }

      // Check issuer
      if (config.issuer && payload.iss !== config.issuer) {
        return { ok: false, error: 'Invalid issuer' };
      }

      // Check audience
      if (config.audience && payload.aud !== config.audience) {
        return { ok: false, error: 'Invalid audience' };
      }

      return { ok: true, user: payload };
    } catch {
      return { ok: false, error: 'Token verification failed' };
    }
  }

  async function sign(user: AuthUser): Promise<string> {
    const header = { alg: 'HS256', typ: 'JWT' };
    const payload = {
      ...user,
      iat: Math.floor(Date.now() / 1000),
      exp: Math.floor(Date.now() / 1000) + (config.expiresIn ?? 3600),
      ...(config.issuer && { iss: config.issuer }),
      ...(config.audience && { aud: config.audience }),
    };

    const headerB64 = base64UrlEncode(JSON.stringify(header));
    const payloadB64 = base64UrlEncode(JSON.stringify(payload));
    const data = encoder.encode(`${headerB64}.${payloadB64}`);

    const key = await crypto.subtle.importKey(
      'raw',
      secretKey,
      { name: 'HMAC', hash: 'SHA-256' },
      false,
      ['sign']
    );
    const signature = await crypto.subtle.sign('HMAC', key, data);
    const signatureB64 = base64UrlEncode(String.fromCharCode(...new Uint8Array(signature)));

    return `${headerB64}.${payloadB64}.${signatureB64}`;
  }

  function middleware(req: Request): AuthResult {
    const authHeader = req.headers.get('Authorization');
    if (!authHeader?.startsWith('Bearer ')) {
      return { ok: false, error: 'Missing or invalid Authorization header' };
    }
    // Note: For sync middleware, we return a pending verification
    // The actual verify is async - use verifyRequest for full async flow
    return { ok: false, error: 'Use verifyRequest for async verification' };
  }

  async function verifyRequest(req: Request): Promise<AuthResult> {
    const authHeader = req.headers.get('Authorization');
    if (!authHeader?.startsWith('Bearer ')) {
      return { ok: false, error: 'Missing or invalid Authorization header' };
    }
    const token = authHeader.slice(7);
    return verify(token);
  }

  return { verify, sign, middleware, verifyRequest };
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
