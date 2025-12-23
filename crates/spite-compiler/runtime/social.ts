
/**
 * SpiteStack Social Auth Module
 *
 * Wraps 'arctic' for OAuth with Google, GitHub, Apple, Microsoft, and Facebook.
 */

import {
  Google,
  GitHub,
  Apple,
  MicrosoftEntraId,
  Facebook,
  generateState,
  generateCodeVerifier
} from 'arctic';

export type SocialProvider = 'google' | 'github' | 'apple' | 'microsoft' | 'facebook';

// =============================================================================
// JWT Signature Validation for Apple ID Tokens
// =============================================================================

interface JWK {
  kty: string;
  kid: string;
  use: string;
  alg: string;
  n: string;
  e: string;
}

interface JWKSCache {
  keys: JWK[];
  fetchedAt: number;
}

// Cache Apple's public keys for 1 hour
const JWKS_CACHE_TTL = 60 * 60 * 1000;
let appleJwksCache: JWKSCache | null = null;

async function fetchAppleJWKS(): Promise<JWK[]> {
  // Return cached keys if still valid
  if (appleJwksCache && Date.now() - appleJwksCache.fetchedAt < JWKS_CACHE_TTL) {
    return appleJwksCache.keys;
  }

  const response = await fetch('https://appleid.apple.com/auth/keys');
  if (!response.ok) {
    throw new Error('Failed to fetch Apple JWKS');
  }
  const jwks = await response.json();

  appleJwksCache = {
    keys: jwks.keys,
    fetchedAt: Date.now()
  };

  return jwks.keys;
}

function base64UrlDecode(str: string): Uint8Array {
  // Add padding if needed
  const padding = '='.repeat((4 - (str.length % 4)) % 4);
  const base64 = (str + padding).replace(/-/g, '+').replace(/_/g, '/');
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

async function importRSAPublicKey(jwk: JWK): Promise<CryptoKey> {
  return crypto.subtle.importKey(
    'jwk',
    {
      kty: jwk.kty,
      n: jwk.n,
      e: jwk.e,
      alg: jwk.alg,
      use: jwk.use
    },
    {
      name: 'RSASSA-PKCS1-v1_5',
      hash: 'SHA-256'
    },
    false,
    ['verify']
  );
}

async function verifyAppleIdToken(
  idToken: string,
  expectedAudience: string
): Promise<{ payload: any; verified: boolean }> {
  const parts = idToken.split('.');
  if (parts.length !== 3) {
    throw new Error('Invalid ID token format');
  }

  const [headerB64, payloadB64, signatureB64] = parts;

  // Decode header to get key ID
  const headerJson = new TextDecoder().decode(base64UrlDecode(headerB64));
  const header = JSON.parse(headerJson);

  // Decode payload
  const payloadJson = new TextDecoder().decode(base64UrlDecode(payloadB64));
  const payload = JSON.parse(payloadJson);

  // Fetch Apple's public keys
  const jwks = await fetchAppleJWKS();
  const jwk = jwks.find(k => k.kid === header.kid);

  if (!jwk) {
    throw new Error(`No matching key found for kid: ${header.kid}`);
  }

  // Verify signature
  const publicKey = await importRSAPublicKey(jwk);
  const signatureBytes = base64UrlDecode(signatureB64);
  const dataToVerify = new TextEncoder().encode(`${headerB64}.${payloadB64}`);

  const isValid = await crypto.subtle.verify(
    'RSASSA-PKCS1-v1_5',
    publicKey,
    signatureBytes,
    dataToVerify
  );

  if (!isValid) {
    throw new Error('Invalid ID token signature');
  }

  // Validate claims
  const now = Math.floor(Date.now() / 1000);

  // Check issuer
  if (payload.iss !== 'https://appleid.apple.com') {
    throw new Error(`Invalid issuer: ${payload.iss}`);
  }

  // Check audience
  if (payload.aud !== expectedAudience) {
    throw new Error(`Invalid audience: ${payload.aud}, expected: ${expectedAudience}`);
  }

  // Check expiration (with 5 minute clock skew tolerance)
  if (payload.exp && payload.exp + 300 < now) {
    throw new Error('ID token has expired');
  }

  // Check issued at (not more than 10 minutes in the future)
  if (payload.iat && payload.iat > now + 600) {
    throw new Error('ID token issued in the future');
  }

  return { payload, verified: true };
}

export interface SocialConfig {
  // Google
  googleClientId?: string;
  googleClientSecret?: string;

  // GitHub
  githubClientId?: string;
  githubClientSecret?: string;

  // Apple
  appleClientId?: string;
  appleTeamId?: string;
  appleKeyId?: string;
  applePrivateKey?: string;

  // Microsoft
  microsoftClientId?: string;
  microsoftClientSecret?: string;
  microsoftTenantId?: string; // 'common' for multi-tenant, or specific tenant ID

  // Facebook
  facebookClientId?: string;
  facebookClientSecret?: string;

  redirectBaseUrl?: string; // e.g. http://localhost:3000
}

export class SocialProviders {
  google?: Google;
  github?: GitHub;
  apple?: Apple;
  microsoft?: MicrosoftEntraId;
  facebook?: Facebook;
  redirectBase: string;
  appleClientId?: string;

  constructor(config: SocialConfig) {
    this.redirectBase = config.redirectBaseUrl || 'http://localhost:3000';
    this.appleClientId = config.appleClientId;

    // Google
    if (config.googleClientId && config.googleClientSecret) {
      this.google = new Google(
        config.googleClientId,
        config.googleClientSecret,
        `${this.redirectBase}/auth/social/google/callback`
      );
    }

    // GitHub
    if (config.githubClientId && config.githubClientSecret) {
      this.github = new GitHub(
        config.githubClientId,
        config.githubClientSecret,
        `${this.redirectBase}/auth/social/github/callback`
      );
    }

    // Apple
    if (config.appleClientId && config.appleTeamId && config.appleKeyId && config.applePrivateKey) {
      this.apple = new Apple(
        config.appleClientId,
        config.appleTeamId,
        config.appleKeyId,
        config.applePrivateKey,
        `${this.redirectBase}/auth/social/apple/callback`
      );
    }

    // Microsoft
    if (config.microsoftClientId && config.microsoftClientSecret) {
      this.microsoft = new MicrosoftEntraId(
        config.microsoftTenantId || 'common',
        config.microsoftClientId,
        config.microsoftClientSecret,
        `${this.redirectBase}/auth/social/microsoft/callback`
      );
    }

    // Facebook
    if (config.facebookClientId && config.facebookClientSecret) {
      this.facebook = new Facebook(
        config.facebookClientId,
        config.facebookClientSecret,
        `${this.redirectBase}/auth/social/facebook/callback`
      );
    }
  }

  async createAuthorizationURL(provider: SocialProvider): Promise<{ url: URL, state: string, codeVerifier?: string }> {
    const state = generateState();
    let url: URL;
    let codeVerifier: string | undefined;

    switch (provider) {
      case 'google':
        if (!this.google) throw new Error('Google auth not configured');
        codeVerifier = generateCodeVerifier();
        url = await this.google.createAuthorizationURL(state, codeVerifier, { scopes: ['profile', 'email'] });
        break;

      case 'github':
        if (!this.github) throw new Error('GitHub auth not configured');
        url = await this.github.createAuthorizationURL(state, { scopes: ['user:email'] });
        break;

      case 'apple':
        if (!this.apple) throw new Error('Apple auth not configured');
        codeVerifier = generateCodeVerifier();
        url = await this.apple.createAuthorizationURL(state, codeVerifier, { scopes: ['name', 'email'] });
        break;

      case 'microsoft':
        if (!this.microsoft) throw new Error('Microsoft auth not configured');
        codeVerifier = generateCodeVerifier();
        url = await this.microsoft.createAuthorizationURL(state, codeVerifier, { scopes: ['openid', 'profile', 'email'] });
        break;

      case 'facebook':
        if (!this.facebook) throw new Error('Facebook auth not configured');
        url = await this.facebook.createAuthorizationURL(state, { scopes: ['email', 'public_profile'] });
        break;

      default:
        throw new Error(`Invalid provider: ${provider}`);
    }

    return { url, state, codeVerifier };
  }

  async validateAuthorizationCode(
    provider: SocialProvider,
    code: string,
    codeVerifier?: string
  ): Promise<{ accessToken: string, idToken?: string }> {
    switch (provider) {
      case 'google':
        if (!this.google) throw new Error('Google auth not configured');
        if (!codeVerifier) throw new Error('Google requires codeVerifier');
        const googleTokens = await this.google.validateAuthorizationCode(code, codeVerifier);
        return { accessToken: googleTokens.accessToken, idToken: googleTokens.idToken };

      case 'github':
        if (!this.github) throw new Error('GitHub auth not configured');
        const githubTokens = await this.github.validateAuthorizationCode(code);
        return { accessToken: githubTokens.accessToken };

      case 'apple':
        if (!this.apple) throw new Error('Apple auth not configured');
        if (!codeVerifier) throw new Error('Apple requires codeVerifier');
        const appleTokens = await this.apple.validateAuthorizationCode(code, codeVerifier);
        return { accessToken: appleTokens.accessToken, idToken: appleTokens.idToken };

      case 'microsoft':
        if (!this.microsoft) throw new Error('Microsoft auth not configured');
        if (!codeVerifier) throw new Error('Microsoft requires codeVerifier');
        const msTokens = await this.microsoft.validateAuthorizationCode(code, codeVerifier);
        return { accessToken: msTokens.accessToken, idToken: msTokens.idToken };

      case 'facebook':
        if (!this.facebook) throw new Error('Facebook auth not configured');
        const fbTokens = await this.facebook.validateAuthorizationCode(code);
        return { accessToken: fbTokens.accessToken };

      default:
        throw new Error(`Invalid provider: ${provider}`);
    }
  }

  async getUserProfile(
    provider: SocialProvider,
    tokens: { accessToken: string, idToken?: string }
  ): Promise<{ id: string, email: string, name?: string, email_verified?: boolean }> {
    switch (provider) {
      case 'google': {
        const res = await fetch('https://openidconnect.googleapis.com/v1/userinfo', {
          headers: { Authorization: `Bearer ${tokens.accessToken}` }
        });
        const user = await res.json();
        // Google provides email_verified claim in userinfo response
        return { id: user.sub, email: user.email, name: user.name, email_verified: user.email_verified === true };
      }

      case 'github': {
        const userRes = await fetch('https://api.github.com/user', {
          headers: { Authorization: `Bearer ${tokens.accessToken}` }
        });
        const user = await userRes.json();

        let email = user.email;
        let email_verified = false;
        if (!email) {
          // Fetch emails if private
          const emailRes = await fetch('https://api.github.com/user/emails', {
            headers: { Authorization: `Bearer ${tokens.accessToken}` }
          });
          const emails = await emailRes.json();
          const primary = emails.find((e: any) => e.primary && e.verified);
          email = primary?.email;
          email_verified = primary?.verified === true;
        } else {
          // GitHub only shows public email if verified
          email_verified = true;
        }
        return { id: user.id.toString(), email, name: user.name, email_verified };
      }

      case 'apple': {
        // Apple sends user info in the ID token
        if (!tokens.idToken) throw new Error('Apple requires idToken');
        if (!this.appleClientId) throw new Error('Apple client ID not configured');

        // Verify the ID token signature and claims
        const { payload } = await verifyAppleIdToken(tokens.idToken, this.appleClientId);

        // Apple includes email_verified claim in the ID token
        return {
          id: payload.sub,
          email: payload.email,
          // Apple only sends name on first auth, we may not have it
          name: undefined,
          email_verified: payload.email_verified === 'true' || payload.email_verified === true
        };
      }

      case 'microsoft': {
        const res = await fetch('https://graph.microsoft.com/v1.0/me', {
          headers: { Authorization: `Bearer ${tokens.accessToken}` }
        });
        const user = await res.json();
        // Microsoft Graph doesn't directly provide email_verified, but mail field
        // indicates a verified email, userPrincipalName may be organizational
        return {
          id: user.id,
          email: user.mail || user.userPrincipalName,
          name: user.displayName,
          // If mail is present, it's typically verified; userPrincipalName is always org-verified
          email_verified: !!(user.mail || user.userPrincipalName)
        };
      }

      case 'facebook': {
        const res = await fetch(
          `https://graph.facebook.com/me?fields=id,email,name&access_token=${tokens.accessToken}`
        );
        const user = await res.json();
        // Facebook only returns email if it's verified
        return { id: user.id, email: user.email, name: user.name, email_verified: !!user.email };
      }

      default:
        throw new Error(`Invalid provider: ${provider}`);
    }
  }

  private decodeIdToken(idToken: string): any {
    const parts = idToken.split('.');
    if (parts.length !== 3) throw new Error('Invalid ID token format');
    const payload = parts[1];
    const decoded = Buffer.from(payload, 'base64').toString('utf-8');
    return JSON.parse(decoded);
  }

  getConfiguredProviders(): SocialProvider[] {
    const providers: SocialProvider[] = [];
    if (this.google) providers.push('google');
    if (this.github) providers.push('github');
    if (this.apple) providers.push('apple');
    if (this.microsoft) providers.push('microsoft');
    if (this.facebook) providers.push('facebook');
    return providers;
  }
}

export function createSocialProviders(): SocialProviders {
  return new SocialProviders({
    // Google
    googleClientId: process.env.GOOGLE_CLIENT_ID,
    googleClientSecret: process.env.GOOGLE_CLIENT_SECRET,

    // GitHub
    githubClientId: process.env.GITHUB_CLIENT_ID,
    githubClientSecret: process.env.GITHUB_CLIENT_SECRET,

    // Apple
    appleClientId: process.env.APPLE_CLIENT_ID,
    appleTeamId: process.env.APPLE_TEAM_ID,
    appleKeyId: process.env.APPLE_KEY_ID,
    applePrivateKey: process.env.APPLE_PRIVATE_KEY,

    // Microsoft
    microsoftClientId: process.env.MICROSOFT_CLIENT_ID,
    microsoftClientSecret: process.env.MICROSOFT_CLIENT_SECRET,
    microsoftTenantId: process.env.MICROSOFT_TENANT_ID,

    // Facebook
    facebookClientId: process.env.FACEBOOK_CLIENT_ID,
    facebookClientSecret: process.env.FACEBOOK_CLIENT_SECRET,

    redirectBaseUrl: process.env.BASE_URL,
  });
}
