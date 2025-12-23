
import { type SpiteDbNapi, type TelemetryDbNapi } from '@spitestack/db';
import { type AuthConfig, createAuth, type AuthUser } from './auth';
import type { EmailProvider } from './email';
import type { SmsProvider } from './sms';
import { createSocialProviders, type SocialProvider } from './social';
import { SYSTEM_ADMIN_ROLE, SYSTEM_TENANT_ID, SYSTEM_TENANT_NAME, TenantAggregate } from './tenant';
import * as TOTP from './totp';
import {
  loginRateLimiter,
  mfaRateLimiter,
  emailVerificationRateLimiter,
  passwordRecoveryRateLimiter,
  registrationRateLimiter,
  rateLimitResponse,
  RateLimiter,
} from './rate-limit';
import {
  validatePassword,
  generateSecureCode,
  generateSecureNumericCode,
  passwordPolicyErrorResponse,
} from './password-policy';

// =============================================================================
// Utilities
// =============================================================================

/**
 * Hash a string case-insensitively (for emails and user-entered codes).
 * Lowercases to ensure consistent lookup regardless of case.
 */
async function hashString(value: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(value.toLowerCase().trim());
  const hash = await crypto.subtle.digest('SHA-256', data);
  return Buffer.from(hash).toString('hex');
}

/**
 * Hash a token case-sensitively (for refresh tokens).
 * Preserves full entropy of cryptographically generated tokens.
 */
async function hashToken(value: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(value);
  const hash = await crypto.subtle.digest('SHA-256', data);
  return Buffer.from(hash).toString('hex');
}

/**
 * Generate a cryptographically secure alphanumeric code.
 * Uses crypto.getRandomValues() for security.
 */
function generateCode(): string {
  return generateSecureCode(8);
}

/**
 * Generate a cryptographically secure numeric code for SMS/MFA.
 */
function generateNumericCode(): string {
  return generateSecureNumericCode(6);
}

function generateSystemPassword(length: number = 20): string {
  const uppercase = 'ABCDEFGHJKLMNPQRSTUVWXYZ';
  const lowercase = 'abcdefghijkmnopqrstuvwxyz';
  const digits = '23456789';
  const special = '!@#$%^&*()-_=+';
  const all = uppercase + lowercase + digits + special;

  const pick = (charset: string): string => {
    const index = crypto.getRandomValues(new Uint32Array(1))[0] % charset.length;
    return charset[index];
  };

  const chars = [
    pick(uppercase),
    pick(lowercase),
    pick(digits),
    pick(special),
  ];

  while (chars.length < length) {
    chars.push(pick(all));
  }

  for (let i = chars.length - 1; i > 0; i--) {
    const j = crypto.getRandomValues(new Uint32Array(1))[0] % (i + 1);
    [chars[i], chars[j]] = [chars[j], chars[i]];
  }

  return chars.join('');
}

/**
 * Perform a dummy password hash to prevent timing attacks.
 * Takes approximately the same time as a real password verification.
 */
async function dummyPasswordHash(): Promise<void> {
  await Bun.password.verify('dummy', '$argon2id$v=19$m=65536,t=2,p=1$dummysaltdummysalt$dummyhashdummyhashdummyhashdummyhash');
}

// =============================================================================
// Cookie Helpers for HttpOnly Token Storage
// =============================================================================

const COOKIE_OPTIONS = {
  httpOnly: true,
  secure: process.env.NODE_ENV === 'production',
  sameSite: 'Strict' as const,
  path: '/',
};

function setAuthCookies(headers: Headers, token: string, refreshToken: string, tokenMaxAge: number = 900, refreshMaxAge: number = 604800) {
  const isProd = process.env.NODE_ENV === 'production';
  const secureCookieFlag = isProd ? '; Secure' : '';

  headers.append('Set-Cookie', `spite_token=${token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=${tokenMaxAge}${secureCookieFlag}`);
  headers.append('Set-Cookie', `spite_refresh=${refreshToken}; Path=/; HttpOnly; SameSite=Strict; Max-Age=${refreshMaxAge}${secureCookieFlag}`);
}

function clearAuthCookies(headers: Headers) {
  const isProd = process.env.NODE_ENV === 'production';
  const secureCookieFlag = isProd ? '; Secure' : '';

  headers.append('Set-Cookie', `spite_token=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0${secureCookieFlag}`);
  headers.append('Set-Cookie', `spite_refresh=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0${secureCookieFlag}`);
}

function getCookie(req: Request, name: string): string | null {
  const cookieHeader = req.headers.get('Cookie') || '';
  const match = cookieHeader.split(';').find(c => c.trim().startsWith(name + '='));
  return match ? match.split('=')[1]?.trim() || null : null;
}

// =============================================================================
// Types
// =============================================================================

export type AuthenticatorType = 'totp' | 'sms' | 'passkey';

export interface AuthenticatorConfig {
  id: string;
  type: AuthenticatorType;
  verified: boolean;
  name?: string;
  secret?: string;
  phoneNumber?: string;
  credentialId?: string;
  publicKey?: string;
  counter?: number;
}

export interface MfaChallenge {
  id: string;
  authenticatorId: string;
  type: AuthenticatorType;
  hash?: string;
  challenge?: string;
  expiresAt: number;
}

export interface PendingMerge {
  provider: SocialProvider;
  providerId: string;
  email: string;
  codeHash: string;
  expiresAt: number;
}

export interface PendingLinkIntent {
  provider: SocialProvider;
  nonceHash: string;
  expiresAt: number;
}

// =============================================================================
// Events
// =============================================================================

export type IdentityEvent =
  | { type: 'UserRegistered'; userId: string; email: string; passwordHash: string; mustChangePassword?: boolean; ts: number }
  | { type: 'UserLoggedIn'; userId: string; deviceId: string; ip?: string; userAgent?: string; ts: number }
  | { type: 'TokenRefreshed'; userId: string; oldHash: string; newHash: string; ts: number }
  | { type: 'RoleAssigned'; userId: string; tenantId: string; role: string; ts: number }
  | { type: 'SessionRevoked'; userId: string; sessionId: string; reason: string; ts: number }
  | { type: 'EmailVerificationRequested'; userId: string; codeHash: string; expiresAt: number; ts: number }
  | { type: 'EmailVerified'; userId: string; ts: number }
  | { type: 'PasswordRecoveryRequested'; userId: string; codeHash: string; expiresAt: number; ts: number }
  | { type: 'PasswordRecovered'; userId: string; newPasswordHash: string; ts: number }
  | { type: 'PasswordChanged'; userId: string; newPasswordHash: string; ts: number }
  | { type: 'AuthenticatorRegistered'; userId: string; config: AuthenticatorConfig; ts: number }
  | { type: 'AuthenticatorVerified'; userId: string; authenticatorId: string; ts: number }
  | { type: 'AuthenticatorRemoved'; userId: string; authenticatorId: string; ts: number }
  | { type: 'MfaChallengeCreated'; userId: string; challenge: MfaChallenge; ts: number }
  | { type: 'MfaChallengeVerified'; userId: string; challengeId: string; ts: number }
  // Social Events
  | { type: 'SocialIdentityLinked'; userId: string; provider: string; providerId: string; email: string; ts: number }
  | { type: 'SocialLoginAttempted'; userId: string; provider: string; success: boolean; ts: number }
  // Merge Flow Events
  | { type: 'SocialMergeRequested'; userId: string; provider: SocialProvider; providerId: string; email: string; codeHash: string; expiresAt: number; ts: number }
  | { type: 'SocialMergeCompleted'; userId: string; provider: SocialProvider; providerId: string; ts: number }
  // First Login Tracking
  | { type: 'FirstPasswordLoginCompleted'; userId: string; ts: number }
  // Social Link Intent (secure account linking)
  | { type: 'SocialLinkIntentCreated'; userId: string; provider: SocialProvider; nonceHash: string; expiresAt: number; ts: number }
  | { type: 'SocialLinkIntentConsumed'; userId: string; provider: SocialProvider; ts: number }
  // Security Audit Events (SOC 2/HIPAA compliance)
  // Note: IP addresses logged under GDPR Legitimate Interest (Recital 49) for security purposes only.
  // Not used for marketing, profiling, or behavioral tracking. See privacy policy for retention period.
  | { type: 'LoginFailed'; userId: string; reason: string; ip?: string; userAgent?: string; ts: number }
  | { type: 'AccountLocked'; userId: string; reason: string; lockedUntil: number; ts: number }
  | { type: 'AccountUnlocked'; userId: string; ts: number }
  | { type: 'MfaEnrolled'; userId: string; authenticatorType: AuthenticatorType; ts: number }
  | { type: 'MfaRemoved'; userId: string; authenticatorType: AuthenticatorType; ts: number }
  // Session Management Events
  | { type: 'SessionCreated'; userId: string; sessionId: string; refreshTokenHash: string; ts: number }
  | { type: 'RefreshTokenRotated'; userId: string; sessionId: string; oldHash: string; newHash: string; ts: number }
  | { type: 'SessionRevokedByReuse'; userId: string; sessionId: string; ts: number }
  | { type: 'AllSessionsRevoked'; userId: string; reason: string; ts: number };

export type LookupEvent =
  | { type: 'UserReferenceCreated'; userId: string; email: string; ts: number }
  | { type: 'SocialReferenceCreated'; userId: string; provider: string; providerId: string; ts: number };

// =============================================================================
// Aggregate
// =============================================================================

export class IdentityAggregate {
  userId: string = '';
  email: string = '';
  passwordHash: string = '';
  emailVerified: boolean = false;
  mustChangePassword: boolean = false;
  orgs: Record<string, { roles: string[] }> = {};
  authenticators: Record<string, AuthenticatorConfig> = {};
  activeChallenges: Record<string, MfaChallenge> = {};
  socialAccounts: Record<string, string> = {}; // provider -> providerId

  verificationCodeHash?: string;
  verificationExpiresAt?: number;
  recoveryCodeHash?: string;
  recoveryExpiresAt?: number;
  exists: boolean = false;
  revokedSessions: Set<string> = new Set();
  // Active sessions with their current refresh token hashes
  activeSessions: Map<string, { refreshTokenHash: string; createdAt: number }> = new Map();

  // New fields for social merge and invite flow
  hasCompletedFirstLogin: boolean = false;
  pendingMerge?: PendingMerge;
  pendingLinkIntent?: PendingLinkIntent;

  // Account lockout fields
  lockedUntil: number = 0;
  failedLoginAttempts: number = 0;
  lastFailedLoginAt: number = 0;

  apply(event: IdentityEvent) {
    switch (event.type) {
      case 'UserRegistered':
        this.userId = event.userId;
        this.email = event.email;
        this.passwordHash = event.passwordHash;
        this.mustChangePassword = event.mustChangePassword ?? false;
        this.exists = true;
        break;
      case 'SocialIdentityLinked':
        this.socialAccounts[event.provider] = event.providerId;
        if (!this.exists) {
          // Auto-create if first event (registration via social)
          this.userId = event.userId;
          this.email = event.email;
          this.emailVerified = true; // Trusted provider
          this.exists = true;
          this.hasCompletedFirstLogin = true; // Social signup counts as first login
        }
        break;
      case 'AuthenticatorRegistered':
        this.authenticators[event.config.id] = event.config;
        break;
      case 'AuthenticatorVerified':
        if (this.authenticators[event.authenticatorId]) {
          this.authenticators[event.authenticatorId].verified = true;
        }
        break;
      case 'AuthenticatorRemoved':
        delete this.authenticators[event.authenticatorId];
        break;
      case 'MfaChallengeCreated':
        this.activeChallenges[event.challenge.id] = event.challenge;
        break;
      case 'MfaChallengeVerified':
        delete this.activeChallenges[event.challengeId];
        break;
      case 'EmailVerificationRequested':
        this.verificationCodeHash = event.codeHash;
        this.verificationExpiresAt = event.expiresAt;
        break;
      case 'EmailVerified':
        this.emailVerified = true;
        this.verificationCodeHash = undefined;
        break;
      case 'PasswordRecoveryRequested':
        this.recoveryCodeHash = event.codeHash;
        this.recoveryExpiresAt = event.expiresAt;
        break;
      case 'PasswordRecovered':
        this.passwordHash = event.newPasswordHash;
        this.recoveryCodeHash = undefined;
        this.mustChangePassword = false;
        this.failedLoginAttempts = 0;
        this.lockedUntil = 0;
        break;
      case 'RoleAssigned':
        if (!this.orgs[event.tenantId]) this.orgs[event.tenantId] = { roles: [] };
        if (!this.orgs[event.tenantId].roles.includes(event.role)) {
          this.orgs[event.tenantId].roles.push(event.role);
        }
        break;
      case 'SessionRevoked':
        this.revokedSessions.add(event.sessionId);
        break;
      case 'FirstPasswordLoginCompleted':
        this.hasCompletedFirstLogin = true;
        break;
      case 'SocialMergeRequested':
        this.pendingMerge = {
          provider: event.provider,
          providerId: event.providerId,
          email: event.email,
          codeHash: event.codeHash,
          expiresAt: event.expiresAt,
        };
        break;
      case 'SocialMergeCompleted':
        this.socialAccounts[event.provider] = event.providerId;
        this.pendingMerge = undefined;
        break;
      case 'SocialLinkIntentCreated':
        this.pendingLinkIntent = {
          provider: event.provider,
          nonceHash: event.nonceHash,
          expiresAt: event.expiresAt,
        };
        break;
      case 'SocialLinkIntentConsumed':
        this.pendingLinkIntent = undefined;
        break;
      case 'LoginFailed':
        this.failedLoginAttempts++;
        this.lastFailedLoginAt = event.ts;
        break;
      case 'AccountLocked':
        this.lockedUntil = event.lockedUntil;
        break;
      case 'AccountUnlocked':
        this.lockedUntil = 0;
        this.failedLoginAttempts = 0;
        break;
      case 'PasswordChanged':
        this.passwordHash = event.newPasswordHash;
        this.failedLoginAttempts = 0;
        this.lockedUntil = 0;
        this.mustChangePassword = false;
        break;
      case 'SessionCreated':
        this.activeSessions.set(event.sessionId, {
          refreshTokenHash: event.refreshTokenHash,
          createdAt: event.ts,
        });
        break;
      case 'RefreshTokenRotated':
        const session = this.activeSessions.get(event.sessionId);
        if (session) {
          session.refreshTokenHash = event.newHash;
        }
        break;
      case 'SessionRevokedByReuse':
        this.activeSessions.delete(event.sessionId);
        this.revokedSessions.add(event.sessionId);
        break;
      case 'AllSessionsRevoked':
        // Move all active sessions to revoked
        for (const sessionId of this.activeSessions.keys()) {
          this.revokedSessions.add(sessionId);
        }
        this.activeSessions.clear();
        break;
    }
  }

  /**
   * Check if the account is currently locked.
   */
  isLocked(): boolean {
    return this.lockedUntil > Date.now();
  }

  /**
   * Get the remaining lockout time in milliseconds.
   */
  getLockoutRemaining(): number {
    const remaining = this.lockedUntil - Date.now();
    return remaining > 0 ? remaining : 0;
  }

  get events(): IdentityEvent[] { return this._events; }
  private _events: IdentityEvent[] = [];

  async register(
    userId: string,
    email: string,
    password: string,
    options?: { mustChangePassword?: boolean; emailVerified?: boolean }
  ) {
    if (this.exists) throw new Error('User already exists');
    this.userId = userId;
    this.email = email;
    const passwordHash = await Bun.password.hash(password);
    this._events.push({
      type: 'UserRegistered',
      userId,
      email,
      passwordHash,
      mustChangePassword: options?.mustChangePassword ?? false,
      ts: Date.now()
    });
    if (options?.emailVerified) {
      this._events.push({ type: 'EmailVerified', userId, ts: Date.now() });
    }
  }

  markEmailVerified() {
    this.emailVerified = true;
    this._events.push({ type: 'EmailVerified', userId: this.userId, ts: Date.now() });
  }

  assignRole(tenantId: string, role: string) {
    if (!this.orgs[tenantId]) this.orgs[tenantId] = { roles: [] };
    if (!this.orgs[tenantId].roles.includes(role)) {
      this.orgs[tenantId].roles.push(role);
    }
    this._events.push({ type: 'RoleAssigned', userId: this.userId, tenantId, role, ts: Date.now() });
  }

  async changePassword(newPassword: string, revokeOtherSessions: boolean = true) {
    const newPasswordHash = await Bun.password.hash(newPassword);
    this.passwordHash = newPasswordHash;
    this.mustChangePassword = false;
    this._events.push({ type: 'PasswordChanged', userId: this.userId, newPasswordHash, ts: Date.now() });
    // Revoke all sessions on password change for security
    if (revokeOtherSessions) {
      this.revokeAllSessions('password_change');
    }
  }

  async login(password: string): Promise<'success' | 'mfa_required'> {
    if (!this.exists) throw new Error('Invalid credentials');
    const valid = await Bun.password.verify(password, this.passwordHash);
    if (!valid) throw new Error('Invalid credentials');
    const hasMfa = Object.values(this.authenticators).some(a => a.verified);
    if (hasMfa) return 'mfa_required';
    return 'success';
  }

  linkSocial(provider: string, providerId: string, email: string) {
    this._events.push({ type: 'SocialIdentityLinked', userId: this.userId, provider, providerId, email, ts: Date.now() });
  }

  async registerTotp(name = 'Authenticator App') {
    const secret = TOTP.generateSecret();
    const id = crypto.randomUUID();
    const config: AuthenticatorConfig = {
      id,
      type: 'totp',
      verified: false,
      name,
      secret,
    };
    this._events.push({ type: 'AuthenticatorRegistered', userId: this.userId, config, ts: Date.now() });
    const uri = TOTP.generateUri(secret, this.email, 'SpiteStack');
    return { secret, uri, id };
  }

  async verifyMfa(challengeId: string | null, code: string, authenticatorId?: string) {
    // If challengeId is null, it's a TOTP direct verification
    if (!challengeId && authenticatorId) {
      const auth = this.authenticators[authenticatorId];
      if (!auth) throw new Error('Authenticator not found');
      if (auth.type !== 'totp') throw new Error('Direct verification only for TOTP');
      const valid = await TOTP.verify(auth.secret!, code);
      if (!valid) throw new Error('Invalid code');
      if (!auth.verified) {
        this._events.push({ type: 'AuthenticatorVerified', userId: this.userId, authenticatorId, ts: Date.now() });
      }
      return;
    }

    // Otherwise verify via challenge
    if (!challengeId) throw new Error('Challenge ID required');
    const challenge = this.activeChallenges[challengeId];
    if (!challenge) throw new Error('Challenge not found');
    if (Date.now() > challenge.expiresAt) throw new Error('Challenge expired');

    if (challenge.type === 'totp') {
      const auth = this.authenticators[challenge.authenticatorId];
      if (!auth?.secret) throw new Error('Authenticator config error');
      const valid = await TOTP.verify(auth.secret, code);
      if (!valid) throw new Error('Invalid code');
    } else if (challenge.type === 'sms') {
      const inputHash = await hashString(code);
      if (inputHash !== challenge.hash) throw new Error('Invalid code');
    }

    this._events.push({ type: 'MfaChallengeVerified', userId: this.userId, challengeId, ts: Date.now() });
  }

  async requestMfaChallenge(authenticatorId: string) {
    const auth = this.authenticators[authenticatorId];
    if (!auth) throw new Error('Authenticator not found');
    if (!auth.verified) throw new Error('Authenticator not verified');

    const challengeId = crypto.randomUUID();
    let code: string | undefined;
    let hash: string | undefined;

    if (auth.type === 'sms') {
      code = generateCode();
      hash = await hashString(code);
    }

    const challenge: MfaChallenge = {
      id: challengeId,
      authenticatorId,
      type: auth.type,
      hash,
      expiresAt: Date.now() + 5 * 60 * 1000, // 5 min
    };

    this._events.push({ type: 'MfaChallengeCreated', userId: this.userId, challenge, ts: Date.now() });
    return { challenge, code };
  }

  async requestEmailVerification() {
    if (this.emailVerified) throw new Error('Email already verified');
    const code = generateCode();
    const codeHash = await hashString(code);
    const expiresAt = Date.now() + 24 * 60 * 60 * 1000; // 24h
    this._events.push({ type: 'EmailVerificationRequested', userId: this.userId, codeHash, expiresAt, ts: Date.now() });
    return code;
  }

  async verifyEmail(code: string) {
    if (!this.verificationCodeHash) throw new Error('No verification pending');
    if (Date.now() > (this.verificationExpiresAt || 0)) throw new Error('Verification expired');
    const inputHash = await hashString(code);
    if (inputHash !== this.verificationCodeHash) throw new Error('Invalid code');
    this._events.push({ type: 'EmailVerified', userId: this.userId, ts: Date.now() });
  }

  async requestPasswordRecovery() {
    const code = generateCode();
    const codeHash = await hashString(code);
    const expiresAt = Date.now() + 1 * 60 * 60 * 1000; // 1h
    this._events.push({ type: 'PasswordRecoveryRequested', userId: this.userId, codeHash, expiresAt, ts: Date.now() });
    return code;
  }

  async recoverPassword(code: string, newPassword: string) {
    if (!this.recoveryCodeHash) throw new Error('No recovery pending');
    if (Date.now() > (this.recoveryExpiresAt || 0)) throw new Error('Recovery expired');
    const inputHash = await hashString(code);
    if (inputHash !== this.recoveryCodeHash) throw new Error('Invalid code');
    const newPasswordHash = await Bun.password.hash(newPassword);
    this.passwordHash = newPasswordHash;
    this.mustChangePassword = false;
    this._events.push({ type: 'PasswordRecovered', userId: this.userId, newPasswordHash, ts: Date.now() });
  }

  refresh(oldHash: string, newHash: string) {
    this._events.push({ type: 'TokenRefreshed', userId: this.userId, oldHash, newHash, ts: Date.now() });
  }

  /**
   * Create a new session with refresh token tracking.
   */
  async createSession(sessionId: string, refreshToken: string) {
    const refreshTokenHash = await hashToken(refreshToken);
    this._events.push({
      type: 'SessionCreated',
      userId: this.userId,
      sessionId,
      refreshTokenHash,
      ts: Date.now()
    });
  }

  /**
   * Validate and rotate a refresh token.
   * Returns 'valid' if rotation succeeds, 'reuse_detected' if token was already rotated (attack),
   * or 'invalid' if the session doesn't exist or token doesn't match.
   */
  async rotateRefreshToken(
    sessionId: string,
    currentRefreshToken: string,
    newRefreshToken: string
  ): Promise<'valid' | 'reuse_detected' | 'invalid'> {
    // Check if session is revoked
    if (this.revokedSessions.has(sessionId)) {
      return 'invalid';
    }

    const session = this.activeSessions.get(sessionId);
    if (!session) {
      return 'invalid';
    }

    const currentHash = await hashToken(currentRefreshToken);
    const newHash = await hashToken(newRefreshToken);

    // Check if the current token hash matches
    if (session.refreshTokenHash !== currentHash) {
      // Token reuse detected! This could be an attack.
      // Revoke the entire session family.
      this._events.push({
        type: 'SessionRevokedByReuse',
        userId: this.userId,
        sessionId,
        ts: Date.now()
      });
      return 'reuse_detected';
    }

    // Valid rotation
    this._events.push({
      type: 'RefreshTokenRotated',
      userId: this.userId,
      sessionId,
      oldHash: currentHash,
      newHash,
      ts: Date.now()
    });

    return 'valid';
  }

  /**
   * Revoke all sessions (e.g., on password change).
   */
  revokeAllSessions(reason: string) {
    if (this.activeSessions.size > 0) {
      this._events.push({
        type: 'AllSessionsRevoked',
        userId: this.userId,
        reason,
        ts: Date.now()
      });
    }
  }

  logLogin(deviceId: string, ip?: string, userAgent?: string) {
    this._events.push({ type: 'UserLoggedIn', userId: this.userId, deviceId, ip, userAgent, ts: Date.now() });
  }

  markFirstPasswordLogin() {
    if (!this.hasCompletedFirstLogin) {
      this._events.push({ type: 'FirstPasswordLoginCompleted', userId: this.userId, ts: Date.now() });
    }
  }

  async requestSocialMerge(provider: SocialProvider, providerId: string, email: string) {
    if (this.socialAccounts[provider]) {
      throw new Error(`${provider} is already linked to this account`);
    }
    const code = generateCode();
    const codeHash = await hashString(code);
    const expiresAt = Date.now() + 10 * 60 * 1000; // 10 minutes
    this._events.push({
      type: 'SocialMergeRequested',
      userId: this.userId,
      provider,
      providerId,
      email,
      codeHash,
      expiresAt,
      ts: Date.now()
    });
    return code;
  }

  async verifySocialMerge(code: string) {
    if (!this.pendingMerge) throw new Error('No merge pending');
    if (Date.now() > this.pendingMerge.expiresAt) throw new Error('Merge code expired');
    const inputHash = await hashString(code);
    if (inputHash !== this.pendingMerge.codeHash) throw new Error('Invalid merge code');

    this._events.push({
      type: 'SocialMergeCompleted',
      userId: this.userId,
      provider: this.pendingMerge.provider,
      providerId: this.pendingMerge.providerId,
      ts: Date.now()
    });

    return { provider: this.pendingMerge.provider, providerId: this.pendingMerge.providerId };
  }

  /**
   * Create a secure link intent for OAuth account linking.
   * Returns a nonce that should be included in the OAuth state parameter.
   */
  async createLinkIntent(provider: SocialProvider): Promise<string> {
    if (this.socialAccounts[provider]) {
      throw new Error(`${provider} is already linked to this account`);
    }
    const nonce = generateCode();
    const nonceHash = await hashString(nonce);
    const expiresAt = Date.now() + 5 * 60 * 1000; // 5 minutes

    this._events.push({
      type: 'SocialLinkIntentCreated',
      userId: this.userId,
      provider,
      nonceHash,
      expiresAt,
      ts: Date.now()
    });

    return nonce;
  }

  /**
   * Verify and consume a link intent.
   * Returns true if the intent is valid for the given provider and nonce.
   */
  async verifyLinkIntent(provider: SocialProvider, nonce: string): Promise<boolean> {
    if (!this.pendingLinkIntent) return false;
    if (this.pendingLinkIntent.provider !== provider) return false;
    if (Date.now() > this.pendingLinkIntent.expiresAt) return false;

    const inputHash = await hashString(nonce);
    if (inputHash !== this.pendingLinkIntent.nonceHash) return false;

    this._events.push({
      type: 'SocialLinkIntentConsumed',
      userId: this.userId,
      provider,
      ts: Date.now()
    });

    return true;
  }
}

// =============================================================================
// Lookup Helpers
// =============================================================================

async function resolveUserId(db: SpiteDbNapi, email: string): Promise<string | null> {
  const hash = await hashString(email);
  const events = await db.readStream(`lookup-email-${hash}`, 0, 1, 'system').catch(() => []);
  if (events.length === 0) return null;
  return (JSON.parse(events[0].data.toString()) as LookupEvent).userId;
}

async function resolveUserIdBySocial(db: SpiteDbNapi, provider: string, providerId: string): Promise<string | null> {
  const streamId = `lookup-social-${provider}-${providerId}`;
  const events = await db.readStream(streamId, 0, 1, 'system').catch(() => []);
  if (events.length === 0) return null;
  return (JSON.parse(events[0].data.toString()) as LookupEvent).userId;
}

async function loadAggregate(db: SpiteDbNapi, userId: string): Promise<{ agg: IdentityAggregate, rev: number }> {
  const streamId = `identity-${userId}`;
  const events = await db.readStream(streamId, 0, 1000, 'system');
  const agg = new IdentityAggregate();
  for (const e of events) agg.apply(JSON.parse(e.data.toString()) as IdentityEvent);
  return { agg, rev: events.length > 0 ? events[events.length - 1].streamRev : 0 };
}

async function persist(db: SpiteDbNapi, userId: string, agg: IdentityAggregate, rev: number) {
  const streamId = `identity-${userId}`;
  const cmdId = crypto.randomUUID();
  const eventBuffers = agg.events.map(e => Buffer.from(JSON.stringify(e)));
  await db.append(streamId, cmdId, rev, eventBuffers, 'system');
}

// =============================================================================
// Bootstrap
// =============================================================================

export async function ensureSystemAdmin(
  db: SpiteDbNapi,
  options: { adminEmail: string }
): Promise<{ created: boolean; password?: string; userId: string; systemTenantCreated: boolean }> {
  const adminEmail = options.adminEmail.trim().toLowerCase();
  if (!adminEmail) {
    throw new Error('SYSTEM_ADMIN_EMAIL is required to bootstrap the system admin.');
  }

  const existingUserId = await resolveUserId(db, adminEmail);
  const userId = existingUserId ?? crypto.randomUUID();
  let created = false;
  let password: string | undefined;

  let agg: IdentityAggregate;
  let rev = 0;
  if (existingUserId) {
    const loaded = await loadAggregate(db, existingUserId);
    agg = loaded.agg;
    rev = loaded.rev;
  } else {
    created = true;
    for (let i = 0; i < 5; i++) {
      const candidate = generateSystemPassword();
      if (validatePassword(candidate).valid) {
        password = candidate;
        break;
      }
    }
    password = password || generateSystemPassword();
    agg = new IdentityAggregate();
    await agg.register(userId, adminEmail, password, { mustChangePassword: true, emailVerified: true });
  }

  const hasSystemRole = agg.orgs?.[SYSTEM_TENANT_ID]?.roles?.includes(SYSTEM_ADMIN_ROLE);
  if (!hasSystemRole) {
    agg.assignRole(SYSTEM_TENANT_ID, SYSTEM_ADMIN_ROLE);
  }

  const tenantStreamId = `tenant-${SYSTEM_TENANT_ID}`;
  const tenantHead = await db.readStream(tenantStreamId, 0, 1, 'system').catch(() => []);
  const systemTenantCreated = tenantHead.length === 0;

  const batch: Array<{ streamId: string; commandId: string; expectedRev: number; events: Buffer[] }> = [];

  const identityEvents = agg.events.map(e => Buffer.from(JSON.stringify(e)));
  if (identityEvents.length > 0) {
    batch.push({
      streamId: `identity-${userId}`,
      commandId: crypto.randomUUID(),
      expectedRev: created ? 0 : rev,
      events: identityEvents
    });
  }

  if (created) {
    const emailHash = await hashString(adminEmail);
    const lookupEvent: LookupEvent = { type: 'UserReferenceCreated', userId, email: adminEmail, ts: Date.now() };
    batch.push({
      streamId: `lookup-email-${emailHash}`,
      commandId: crypto.randomUUID(),
      expectedRev: 0,
      events: [Buffer.from(JSON.stringify(lookupEvent))]
    });
  }

  if (systemTenantCreated) {
    const tenantAgg = new TenantAggregate();
    tenantAgg.create(SYSTEM_TENANT_ID, SYSTEM_TENANT_NAME, userId);
    const tenantEvents = tenantAgg.events.map(e => Buffer.from(JSON.stringify(e)));
    batch.push({
      streamId: tenantStreamId,
      commandId: crypto.randomUUID(),
      expectedRev: 0,
      events: tenantEvents
    });
  }

  if (batch.length > 0) {
    await db.appendBatch(batch, 'system');
  }

  return { created, password, userId, systemTenantCreated };
}

// =============================================================================
// Handlers
// =============================================================================

export async function handleAuthRegister(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  emailProvider: EmailProvider,
  smsProvider: SmsProvider,
  body: any,
  req?: Request
): Promise<Response> {
  const { email, password } = body;
  if (!email || !password) return new Response('Missing fields', { status: 400 });

  // Rate limiting by IP
  const clientIp = req ? RateLimiter.getClientIp(req) : 'unknown';
  const rateLimitCheck = registrationRateLimiter.check(clientIp);
  if (!rateLimitCheck.allowed) {
    return rateLimitResponse(rateLimitCheck.retryAfterMs);
  }

  // Validate password against policy
  const passwordValidation = validatePassword(password);
  if (!passwordValidation.valid) {
    return passwordPolicyErrorResponse(passwordValidation);
  }

  // Check if email already exists
  if (await resolveUserId(db, email)) {
    // Record attempt even on failure to prevent enumeration via timing
    registrationRateLimiter.recordAttempt(clientIp);

    // SECURITY: Return the same response as success to prevent email enumeration
    // Send an email to the existing user notifying them of the registration attempt
    await emailProvider.send(
      email,
      'Registration Attempt',
      'Someone tried to register with your email address. If this was you, please log in or reset your password. If not, you can safely ignore this email.'
    );

    // Return generic success to prevent enumeration
    return new Response(JSON.stringify({
      message: 'Check your email to continue registration'
    }), { status: 200 });
  }

  registrationRateLimiter.recordAttempt(clientIp, true); // Success resets limiter

  const userId = crypto.randomUUID();
  const agg = new IdentityAggregate();
  await agg.register(userId, email, password);
  const code = await agg.requestEmailVerification();

  const emailHash = await hashString(email);
  const identityEvents = agg.events.map(ev => Buffer.from(JSON.stringify(ev)));
  const lookupEvent: LookupEvent = { type: 'UserReferenceCreated', userId, email, ts: Date.now() };

  await db.appendBatch([
    { streamId: `identity-${userId}`, commandId: crypto.randomUUID(), expectedRev: 0, events: identityEvents },
    { streamId: `lookup-email-${emailHash}`, commandId: crypto.randomUUID(), expectedRev: 0, events: [Buffer.from(JSON.stringify(lookupEvent))] }
  ], 'system');

  if (code) await emailProvider.send(email, 'Verify your email', `Your verification code is: ${code}`);

  // Return same message format as existing user case to prevent enumeration
  return new Response(JSON.stringify({
    message: 'Check your email to continue registration'
  }), { status: 200 });
}

export async function handleAuthLogin(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  emailProvider: EmailProvider,
  smsProvider: SmsProvider,
  body: any,
  req?: Request
): Promise<Response> {
  const { email, password, deviceId } = body;
  if (!email || !password) return new Response('Missing credentials', { status: 400 });

  // Rate limiting by IP + email combination
  const clientIp = req ? RateLimiter.getClientIp(req) : 'unknown';
  const userAgent = req?.headers.get('user-agent') || 'unknown';
  const rateLimitKey = RateLimiter.compositeKey(clientIp, email.toLowerCase());
  const rateLimitCheck = loginRateLimiter.check(rateLimitKey);
  if (!rateLimitCheck.allowed) {
    return rateLimitResponse(rateLimitCheck.retryAfterMs);
  }

  const userId = await resolveUserId(db, email);

  // Timing attack protection: perform dummy hash even if user doesn't exist
  if (!userId) {
    await dummyPasswordHash();
    loginRateLimiter.recordAttempt(rateLimitKey);
    return new Response(JSON.stringify({ error: 'Invalid credentials' }), {
      status: 401,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  const { agg, rev } = await loadAggregate(db, userId);

  // Check if account is locked
  if (agg.isLocked()) {
    const remaining = agg.getLockoutRemaining();
    return new Response(JSON.stringify({
      error: 'Account temporarily locked',
      retryAfter: Math.ceil(remaining / 1000),
      message: `Too many failed attempts. Please try again later.`
    }), {
      status: 423,
      headers: {
        'Content-Type': 'application/json',
        'Retry-After': Math.ceil(remaining / 1000).toString()
      }
    });
  }

  let status;
  try {
    status = await agg.login(password);
  } catch {
    // Record failed login attempt
    loginRateLimiter.recordAttempt(rateLimitKey);

    // Emit LoginFailed event for audit trail
    agg._events.push({
      type: 'LoginFailed',
      userId,
      reason: 'Invalid password',
      ip: clientIp,
      userAgent,
      ts: Date.now()
    });

    // Check if we should lock the account (5 failed attempts)
    const maxFailedAttempts = 5;
    const lockoutDurationMs = 15 * 60 * 1000; // 15 minutes base lockout

    if (agg.failedLoginAttempts + 1 >= maxFailedAttempts) {
      const lockoutMultiplier = Math.min(Math.pow(2, Math.floor(agg.failedLoginAttempts / maxFailedAttempts)), 8);
      const lockedUntil = Date.now() + (lockoutDurationMs * lockoutMultiplier);

      agg._events.push({
        type: 'AccountLocked',
        userId,
        reason: 'Too many failed login attempts',
        lockedUntil,
        ts: Date.now()
      });
    }

    await persist(db, userId, agg, rev);
    return new Response(JSON.stringify({ error: 'Invalid credentials' }), {
      status: 401,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  // Successful authentication - reset rate limiter
  loginRateLimiter.reset(rateLimitKey);

  // SECURITY: Don't reveal that credentials are valid if email is unverified
  // This prevents attackers from confirming credential validity
  if (!agg.emailVerified) {
    // Silently send a verification reminder (legitimate users will see it)
    // But return a generic error that doesn't confirm credential validity
    try {
      const code = await agg.requestEmailVerification();
      await persist(db, userId, agg, rev);
      await emailProvider.send(
        agg.email,
        'Email verification required',
        `Please verify your email to sign in. Your verification code is: ${code}`
      );
    } catch {
      // Verification already pending or error - ignore silently
    }

    // Return same error as invalid credentials to prevent enumeration
    return new Response(JSON.stringify({
      error: 'Invalid credentials or account not verified',
      message: 'Check your credentials. If you recently registered, check your email for verification instructions.'
    }), {
      status: 401,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  if (agg.mustChangePassword) {
    const auth = createAuth(authConfig);
    const passwordChangeToken = await auth.sign({ sub: userId, pwd_change: true }, 'access');
    return new Response(JSON.stringify({
      status: 'password_change_required',
      passwordChangeToken
    }), { status: 200 });
  }

  // If account was locked but lockout expired, emit unlock event
  if (agg.lockedUntil > 0 && agg.lockedUntil <= Date.now()) {
    agg._events.push({ type: 'AccountUnlocked', userId, ts: Date.now() });
  }

  if (status === 'mfa_required') {
    const auth = createAuth(authConfig);
    // MFA pending token - include sid for tracking but mfa_pending flag blocks protected routes
    const mfaSessionId = crypto.randomUUID();
    const mfaToken = await auth.sign({ sub: userId, sid: mfaSessionId, mfa_pending: true }, 'access');
    await persist(db, userId, agg, rev);
    return new Response(JSON.stringify({
      status: 'mfa_required',
      mfaToken,
      authenticators: Object.values(agg.authenticators).filter(a => a.verified).map(a => ({ id: a.id, type: a.type, name: a.name }))
    }), { status: 200 });
  }

  // Track first password login for invited users
  if (!agg.hasCompletedFirstLogin && agg.emailVerified) {
    agg.markFirstPasswordLogin();
  }

  agg.logLogin(deviceId || 'unknown', clientIp, userAgent);

  const auth = createAuth(authConfig);
  const sessionId = crypto.randomUUID();
  const token = await auth.sign({ sub: userId, sid: sessionId, orgs: agg.orgs, firstIat: Date.now() / 1000 }, 'access');
  const refreshToken = await auth.sign({ sub: userId, sid: sessionId, orgs: agg.orgs, firstIat: Date.now() / 1000 }, 'refresh');

  // Track the session with refresh token hash for rotation
  await agg.createSession(sessionId, refreshToken);
  await persist(db, userId, agg, rev);

  // Set HttpOnly cookies for token storage
  const headers = new Headers({ 'Content-Type': 'application/json' });
  setAuthCookies(headers, token, refreshToken);

  // Return user info but NOT tokens (they're in cookies)
  return new Response(JSON.stringify({
    status: 'success',
    userId,
    user: { sub: userId, orgs: agg.orgs }
  }), { status: 200, headers });
}

export async function handleAuthMfaChallenge(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  emailProvider: EmailProvider,
  smsProvider: SmsProvider,
  body: any,
  req?: Request
): Promise<Response> {
  const { mfaToken, authenticatorId } = body;
  const auth = createAuth(authConfig);
  const result = await auth.verify(mfaToken);
  if (!result.ok || !result.user.mfa_pending) {
    return new Response(JSON.stringify({ error: 'Invalid MFA session' }), {
      status: 401,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  const userId = result.user.sub;

  // Rate limiting by userId to prevent SMS spam attacks
  // Use a separate limiter for challenge requests (more restrictive than verification)
  const rateLimitKey = `mfa-challenge:${userId}`;
  const rateLimitCheck = mfaRateLimiter.check(rateLimitKey);
  if (!rateLimitCheck.allowed) {
    return rateLimitResponse(rateLimitCheck.retryAfterMs);
  }

  const { agg, rev } = await loadAggregate(db, userId);

  try {
    const { challenge, code } = await agg.requestMfaChallenge(authenticatorId);
    if (agg.authenticators[authenticatorId].type === 'sms' && code) {
      const phone = agg.authenticators[authenticatorId].phoneNumber;
      if (phone) await smsProvider.send(phone, `Your verification code is: ${code}`);
    }
    // Record successful challenge request (still counts towards rate limit)
    mfaRateLimiter.recordAttempt(rateLimitKey, true);
    await persist(db, userId, agg, rev);
    return new Response(JSON.stringify({ challengeId: challenge.id, type: challenge.type }), { status: 200 });
  } catch (err) {
    mfaRateLimiter.recordAttempt(rateLimitKey);
    return new Response(JSON.stringify({ error: (err as Error).message }), {
      status: 400,
      headers: { 'Content-Type': 'application/json' }
    });
  }
}

export async function handleAuthMfaVerify(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  emailProvider: EmailProvider,
  smsProvider: SmsProvider,
  body: any,
  req?: Request
): Promise<Response> {
  const { mfaToken, challengeId, code, authenticatorId } = body;
  const auth = createAuth(authConfig);
  const result = await auth.verify(mfaToken);
  if (!result.ok || !result.user.mfa_pending) {
    return new Response('Invalid MFA session', { status: 401 });
  }

  const userId = result.user.sub;

  // Rate limiting by userId for MFA attempts
  const rateLimitKey = `mfa:${userId}`;
  const rateLimitCheck = mfaRateLimiter.check(rateLimitKey);
  if (!rateLimitCheck.allowed) {
    return rateLimitResponse(rateLimitCheck.retryAfterMs);
  }

  const { agg, rev } = await loadAggregate(db, userId);
  const clientIp = req ? RateLimiter.getClientIp(req) : 'unknown';
  const userAgent = req?.headers.get('user-agent') || 'unknown';

  try {
    await agg.verifyMfa(challengeId, code, authenticatorId);
  } catch (err) {
    // Record failed MFA attempt
    mfaRateLimiter.recordAttempt(rateLimitKey);
    return new Response(JSON.stringify({ error: (err as Error).message }), {
      status: 400,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  // Successful MFA - reset rate limiter
  mfaRateLimiter.reset(rateLimitKey);

  // Track first password login after MFA
  if (!agg.hasCompletedFirstLogin && agg.emailVerified) {
    agg.markFirstPasswordLogin();
  }

  agg.logLogin('mfa', clientIp, userAgent);

  const sessionId = crypto.randomUUID();
  const token = await auth.sign({ sub: userId, sid: sessionId, orgs: agg.orgs, firstIat: Date.now() / 1000 }, 'access');
  const refreshToken = await auth.sign({ sub: userId, sid: sessionId, orgs: agg.orgs, firstIat: Date.now() / 1000 }, 'refresh');

  // Track the session with refresh token hash for rotation
  await agg.createSession(sessionId, refreshToken);
  await persist(db, userId, agg, rev);

  // Set HttpOnly cookies for token storage
  const headers = new Headers({ 'Content-Type': 'application/json' });
  setAuthCookies(headers, token, refreshToken);

  return new Response(JSON.stringify({
    status: 'success',
    user: { sub: userId, orgs: agg.orgs }
  }), { status: 200, headers });
}

export async function handleAuthMfaEnroll(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  emailProvider: EmailProvider,
  smsProvider: SmsProvider,
  body: any,
  user: AuthUser
): Promise<Response> {
  const { type, name } = body;
  const { agg, rev } = await loadAggregate(db, user.sub);

  if (type === 'totp') {
    const { secret, uri, id } = await agg.registerTotp(name);
    await persist(db, user.sub, agg, rev);
    return new Response(JSON.stringify({ secret, uri, id }), { status: 200 });
  }

  return new Response('Unsupported authenticator type', { status: 400 });
}

export async function handleAuthMfaEnrollChallenge(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  emailProvider: EmailProvider,
  smsProvider: SmsProvider,
  body: any,
  user: AuthUser
): Promise<Response> {
  const { authenticatorId, code } = body;
  if (!authenticatorId || !code) {
    return new Response(JSON.stringify({ error: 'Missing authenticatorId or code' }), {
      status: 400,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  const { agg, rev } = await loadAggregate(db, user.sub);

  // Check if the authenticator exists and belongs to this user
  const authenticator = agg.authenticators[authenticatorId];
  if (!authenticator) {
    return new Response(JSON.stringify({ error: 'Authenticator not found' }), {
      status: 404,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  // If already verified, return success
  if (authenticator.verified) {
    return new Response(JSON.stringify({ ok: true, message: 'Authenticator already verified' }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  try {
    // Use verifyMfa with null challengeId for direct TOTP verification
    // This will verify the code and emit AuthenticatorVerified event if successful
    await agg.verifyMfa(null, code, authenticatorId);
    await persist(db, user.sub, agg, rev);

    return new Response(JSON.stringify({ ok: true, message: 'Authenticator verified successfully' }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' }
    });
  } catch (err) {
    return new Response(JSON.stringify({ error: (err as Error).message }), {
      status: 400,
      headers: { 'Content-Type': 'application/json' }
    });
  }
}

export async function handleAuthRefresh(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  emailProvider: EmailProvider,
  smsProvider: SmsProvider,
  body: any,
  req?: Request
): Promise<Response> {
  // Read refresh token from cookie (preferred) or body (fallback)
  const refreshToken = (req ? getCookie(req, 'spite_refresh') : null) || body?.refreshToken;

  if (!refreshToken) {
    return new Response(JSON.stringify({ error: 'No refresh token' }), {
      status: 401,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  const auth = createAuth(authConfig);
  const r = await auth.verifyRefresh(refreshToken);
  if (!r.ok) {
    const headers = new Headers({ 'Content-Type': 'application/json' });
    clearAuthCookies(headers);
    return new Response(JSON.stringify({ error: r.error }), { status: 401, headers });
  }

  const u = r.user;
  const sessionId = u.sid;
  if (!sessionId) {
    const headers = new Headers({ 'Content-Type': 'application/json' });
    clearAuthCookies(headers);
    return new Response(JSON.stringify({ error: 'Invalid session' }), { status: 401, headers });
  }

  const { agg, rev } = await loadAggregate(db, u.sub);

  // Check if session was explicitly revoked
  if (agg.revokedSessions.has(sessionId)) {
    const headers = new Headers({ 'Content-Type': 'application/json' });
    clearAuthCookies(headers);
    return new Response(JSON.stringify({ error: 'Session revoked' }), { status: 401, headers });
  }

  // Generate new tokens
  const token = await auth.sign(u, 'access');
  const newRefreshToken = await auth.sign(u, 'refresh');

  // Attempt to rotate the refresh token with reuse detection
  const rotationResult = await agg.rotateRefreshToken(sessionId, refreshToken, newRefreshToken);

  if (rotationResult === 'reuse_detected') {
    // Token reuse detected - this is a potential attack
    // The session has been revoked, persist the change
    await persist(db, u.sub, agg, rev);
    const headers = new Headers({ 'Content-Type': 'application/json' });
    clearAuthCookies(headers);
    return new Response(JSON.stringify({
      error: 'Token reuse detected',
      message: 'Your session has been terminated for security reasons. Please log in again.'
    }), { status: 401, headers });
  }

  if (rotationResult === 'invalid') {
    const headers = new Headers({ 'Content-Type': 'application/json' });
    clearAuthCookies(headers);
    return new Response(JSON.stringify({ error: 'Invalid session' }), { status: 401, headers });
  }

  // Successful rotation
  await persist(db, u.sub, agg, rev);

  // Set HttpOnly cookies with new tokens
  const headers = new Headers({ 'Content-Type': 'application/json' });
  setAuthCookies(headers, token, newRefreshToken);

  return new Response(JSON.stringify({
    status: 'success',
    user: { sub: u.sub, orgs: u.orgs }
  }), { status: 200, headers });
}

export async function handleAuthVerifyEmail(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  emailProvider: EmailProvider,
  smsProvider: SmsProvider,
  body: any,
  req?: Request
): Promise<Response> {
  const { userId, code } = body;
  if (!userId || !code) {
    return new Response(JSON.stringify({ error: 'Missing required fields' }), { status: 400 });
  }

  // Rate limiting by userId to prevent brute force attacks on verification codes
  const rateLimitKey = `verify:${userId}`;
  const rateLimitCheck = emailVerificationRateLimiter.check(rateLimitKey);
  if (!rateLimitCheck.allowed) {
    return rateLimitResponse(rateLimitCheck.retryAfterMs);
  }

  const { agg, rev } = await loadAggregate(db, userId);

  try {
    await agg.verifyEmail(code);
  } catch (err) {
    // Record failed attempt
    emailVerificationRateLimiter.recordAttempt(rateLimitKey);
    return new Response(JSON.stringify({ error: (err as Error).message }), {
      status: 400,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  // Successful verification - reset rate limiter
  emailVerificationRateLimiter.reset(rateLimitKey);

  await persist(db, userId, agg, rev);
  return new Response(JSON.stringify({ ok: true }), { status: 200 });
}

export async function handleAuthRequestRecovery(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  emailProvider: EmailProvider,
  smsProvider: SmsProvider,
  body: any,
  req?: Request
): Promise<Response> {
  const { email } = body;
  if (!email) return new Response(JSON.stringify({ error: 'Email required' }), { status: 400 });

  // Rate limiting by IP
  const clientIp = req ? RateLimiter.getClientIp(req) : 'unknown';
  const rateLimitKey = `recovery:${clientIp}`;
  const rateLimitCheck = passwordRecoveryRateLimiter.check(rateLimitKey);
  if (!rateLimitCheck.allowed) {
    return rateLimitResponse(rateLimitCheck.retryAfterMs);
  }

  passwordRecoveryRateLimiter.recordAttempt(rateLimitKey);

  const userId = await resolveUserId(db, email);

  if (userId) {
    const { agg, rev } = await loadAggregate(db, userId);
    const code = await agg.requestPasswordRecovery();
    if (code) {
      await emailProvider.send(email, 'Password Reset', `Your reset code is: ${code}`);
      await persist(db, userId, agg, rev);
    }
  }

  // Always return success to prevent email enumeration
  return new Response(JSON.stringify({ ok: true, message: 'If an account exists with this email, a reset code has been sent.' }), { status: 200 });
}

export async function handleAuthResetPassword(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  emailProvider: EmailProvider,
  smsProvider: SmsProvider,
  body: any,
  req?: Request
): Promise<Response> {
  const { email, code, newPassword } = body;
  if (!email || !code || !newPassword) {
    return new Response(JSON.stringify({ error: 'Missing required fields' }), { status: 400 });
  }

  // Validate new password against policy
  const passwordValidation = validatePassword(newPassword);
  if (!passwordValidation.valid) {
    return passwordPolicyErrorResponse(passwordValidation);
  }

  // Rate limiting by IP
  const clientIp = req ? RateLimiter.getClientIp(req) : 'unknown';
  const rateLimitKey = `reset:${clientIp}`;
  const rateLimitCheck = passwordRecoveryRateLimiter.check(rateLimitKey);
  if (!rateLimitCheck.allowed) {
    return rateLimitResponse(rateLimitCheck.retryAfterMs);
  }

  const userId = await resolveUserId(db, email);

  // Timing attack protection: perform dummy hash even if user doesn't exist
  if (!userId) {
    await dummyPasswordHash();
    passwordRecoveryRateLimiter.recordAttempt(rateLimitKey);
    // Return generic error that doesn't reveal if user exists
    return new Response(JSON.stringify({ error: 'Invalid or expired reset code' }), { status: 400 });
  }

  const { agg, rev } = await loadAggregate(db, userId);

  try {
    await agg.recoverPassword(code, newPassword);
  } catch (err) {
    passwordRecoveryRateLimiter.recordAttempt(rateLimitKey);
    // Generic error message to prevent enumeration
    return new Response(JSON.stringify({ error: 'Invalid or expired reset code' }), { status: 400 });
  }

  // Revoke all existing sessions for security (PasswordRecovered event already emitted by recoverPassword)
  agg.revokeAllSessions('password_reset');

  await persist(db, userId, agg, rev);
  return new Response(JSON.stringify({ ok: true }), { status: 200 });
}

export async function handleAuthChangePassword(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  emailProvider: EmailProvider,
  smsProvider: SmsProvider,
  body: any,
  req?: Request
): Promise<Response> {
  const { passwordChangeToken, newPassword } = body;
  if (!passwordChangeToken || !newPassword) {
    return new Response(JSON.stringify({ error: 'Missing required fields' }), { status: 400 });
  }

  const passwordValidation = validatePassword(newPassword);
  if (!passwordValidation.valid) {
    return passwordPolicyErrorResponse(passwordValidation);
  }

  const auth = createAuth(authConfig);
  const result = await auth.verify(passwordChangeToken);
  if (!result.ok || !result.user.pwd_change) {
    return new Response(JSON.stringify({ error: 'Invalid password change session' }), { status: 401 });
  }

  const userId = result.user.sub;
  const { agg, rev } = await loadAggregate(db, userId);

  await agg.changePassword(newPassword);

  if (!agg.hasCompletedFirstLogin && agg.emailVerified) {
    agg.markFirstPasswordLogin();
  }

  const clientIp = req ? RateLimiter.getClientIp(req) : 'unknown';
  const userAgent = req?.headers.get('user-agent') || 'unknown';
  agg.logLogin('password-change', clientIp, userAgent);

  const sessionId = crypto.randomUUID();
  const token = await auth.sign({ sub: userId, sid: sessionId, orgs: agg.orgs, firstIat: Date.now() / 1000 }, 'access');
  const refreshToken = await auth.sign({ sub: userId, sid: sessionId, orgs: agg.orgs, firstIat: Date.now() / 1000 }, 'refresh');

  // Track the new session with refresh token hash
  await agg.createSession(sessionId, refreshToken);
  await persist(db, userId, agg, rev);

  // Set HttpOnly cookies with new tokens
  const headers = new Headers({ 'Content-Type': 'application/json' });
  setAuthCookies(headers, token, refreshToken);

  return new Response(JSON.stringify({
    status: 'success',
    user: { sub: userId, orgs: agg.orgs }
  }), { status: 200, headers });
}

// =============================================================================
// Session Management Handlers
// =============================================================================

export async function handleAuthSession(
  db: SpiteDbNapi,
  authConfig: AuthConfig,
  req: Request
): Promise<Response> {
  const token = getCookie(req, 'spite_token');
  if (!token) {
    return new Response(JSON.stringify({ error: 'Not authenticated' }), {
      status: 401,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  const auth = createAuth(authConfig);
  const result = await auth.verify(token);
  if (!result.ok) {
    const headers = new Headers({ 'Content-Type': 'application/json' });
    clearAuthCookies(headers);
    return new Response(JSON.stringify({ error: 'Invalid session' }), { status: 401, headers });
  }

  return new Response(JSON.stringify({
    user: { sub: result.user.sub, orgs: result.user.orgs }
  }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' }
  });
}

export async function handleAuthLogout(
  req: Request
): Promise<Response> {
  const headers = new Headers({ 'Content-Type': 'application/json' });
  clearAuthCookies(headers);
  return new Response(JSON.stringify({ ok: true }), { status: 200, headers });
}

// =============================================================================
// Social Auth Handlers
// =============================================================================

export async function handleAuthSocialLogin(
  provider: SocialProvider
): Promise<Response> {
  const social = createSocialProviders();
  const { url, state, codeVerifier } = await social.createAuthorizationURL(provider);

  const isProd = process.env.NODE_ENV === 'production';
  const secureCookieFlag = isProd ? '; Secure' : '';

  const headers = new Headers();
  headers.set('Location', url.toString());
  headers.set('Set-Cookie', `spite_oauth_state=${state}; Path=/; HttpOnly; SameSite=Lax; Max-Age=300${secureCookieFlag}`);
  if (codeVerifier) {
    headers.append('Set-Cookie', `spite_code_verifier=${codeVerifier}; Path=/; HttpOnly; SameSite=Lax; Max-Age=300${secureCookieFlag}`);
  }

  return new Response(null, { status: 302, headers });
}

export async function handleAuthSocialCallback(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  emailProvider: EmailProvider,
  req: Request,
  provider: SocialProvider
): Promise<Response> {
  const url = new URL(req.url);
  const code = url.searchParams.get('code');
  const state = url.searchParams.get('state');

  // Extract cookies
  const cookieHeader = req.headers.get('Cookie') || '';
  const getCookie = (n: string) => cookieHeader.split(';').find(c => c.trim().startsWith(n + '='))?.split('=')[1];
  const storedState = getCookie('spite_oauth_state');
  const codeVerifier = getCookie('spite_code_verifier');
  const linkToken = getCookie('spite_link_token');

  if (!code || !state || !storedState || state !== storedState) {
    return new Response(JSON.stringify({ error: 'InvalidState', message: 'Invalid OAuth state' }), { status: 400 });
  }

  // Verify link token if present (secure account linking)
  let verifiedLinkUserId: string | null = null;
  let verifiedLinkNonce: string | null = null;
  if (linkToken) {
    const auth = createAuth(authConfig);
    const linkResult = await auth.verify(linkToken);
    if (linkResult.ok && linkResult.user.link_nonce && linkResult.user.link_provider === provider) {
      verifiedLinkUserId = linkResult.user.sub;
      verifiedLinkNonce = linkResult.user.link_nonce as string;
    }
    // If link token is invalid or doesn't match provider, ignore it (treat as normal login)
  }

  const social = createSocialProviders();
  let tokens, profile;
  try {
    tokens = await social.validateAuthorizationCode(provider, code, codeVerifier);
    profile = await social.getUserProfile(provider, tokens);
  } catch (e) {
    return new Response(JSON.stringify({
      error: 'AuthFailed',
      message: `Social auth failed: ${(e as Error).message}`
    }), { status: 400 });
  }

  // If this is a link operation (verified link token present)
  if (verifiedLinkUserId && verifiedLinkNonce) {
    const { agg, rev } = await loadAggregate(db, verifiedLinkUserId);
    if (!agg.exists) {
      return new Response(JSON.stringify({ error: 'UserNotFound' }), { status: 404 });
    }

    // Verify the link intent (nonce) stored server-side
    const intentValid = await agg.verifyLinkIntent(provider, verifiedLinkNonce);
    if (!intentValid) {
      return new Response(JSON.stringify({
        error: 'InvalidLinkIntent',
        message: 'Link intent expired or invalid. Please try again.'
      }), { status: 400 });
    }

    if (agg.socialAccounts[provider]) {
      return new Response(JSON.stringify({
        error: 'AlreadyLinked',
        message: `${provider} is already linked to your account`
      }), { status: 409 });
    }

    // Create social lookup
    const socialLookupStream = `lookup-social-${provider}-${profile.id}`;
    const socialLookupEvent: LookupEvent = {
      type: 'SocialReferenceCreated',
      userId: verifiedLinkUserId,
      provider,
      providerId: profile.id,
      ts: Date.now()
    };

    agg.linkSocial(provider, profile.id, profile.email);

    await db.appendBatch([
      { streamId: `identity-${verifiedLinkUserId}`, commandId: crypto.randomUUID(), expectedRev: rev, events: agg.events.map(e => Buffer.from(JSON.stringify(e))) },
      { streamId: socialLookupStream, commandId: crypto.randomUUID(), expectedRev: 0, events: [Buffer.from(JSON.stringify(socialLookupEvent))] }
    ], 'system');

    // Clear link token cookie and return success
    const headers = new Headers();
    headers.set('Set-Cookie', 'spite_link_token=; Path=/; HttpOnly; Max-Age=0');
    headers.set('Content-Type', 'application/json');
    return new Response(JSON.stringify({ ok: true, linked: provider }), { status: 200, headers });
  }

  // 1. Check if Social ID already exists -> Login
  const existingUserId = await resolveUserIdBySocial(db, provider, profile.id);

  if (existingUserId) {
    const { agg, rev } = await loadAggregate(db, existingUserId);

    // Enforce invite flow restrictions:
    // If user was invited (has orgs, has password, but hasn't completed first login), block social login
    const wasInvited = Object.keys(agg.orgs).length > 0 && agg.passwordHash && !agg.hasCompletedFirstLogin;
    if (wasInvited) {
      return new Response(JSON.stringify({
        error: 'FirstLoginRequired',
        message: 'Invited users must complete email verification and password-based login before using social sign-in.',
        email: agg.email
      }), { status: 403 });
    }

    agg.logLogin(provider);

    // Check MFA
    const hasMfa = Object.values(agg.authenticators).some(a => a.verified);
    if (hasMfa) {
      const auth = createAuth(authConfig);
      // MFA pending token - include sid for tracking but mfa_pending flag blocks protected routes
      const mfaSessionId = crypto.randomUUID();
      const mfaToken = await auth.sign({ sub: existingUserId, sid: mfaSessionId, mfa_pending: true }, 'access');
      await persist(db, existingUserId, agg, rev);
      return new Response(JSON.stringify({
        status: 'mfa_required',
        mfaToken,
        authenticators: Object.values(agg.authenticators).filter(a => a.verified).map(a => ({ id: a.id, type: a.type, name: a.name }))
      }), { status: 200 });
    }

    const auth = createAuth(authConfig);
    const sessionId = crypto.randomUUID();
    const token = await auth.sign({ sub: existingUserId, sid: sessionId, orgs: agg.orgs, firstIat: Date.now() / 1000 }, 'access');
    const refreshToken = await auth.sign({ sub: existingUserId, sid: sessionId, orgs: agg.orgs, firstIat: Date.now() / 1000 }, 'refresh');

    // Track the session with refresh token hash for rotation
    await agg.createSession(sessionId, refreshToken);
    await persist(db, existingUserId, agg, rev);

    // Set HttpOnly cookies for token storage
    const headers = new Headers({ 'Content-Type': 'application/json' });
    setAuthCookies(headers, token, refreshToken);

    return new Response(JSON.stringify({
      status: 'success',
      userId: existingUserId,
      user: { sub: existingUserId, orgs: agg.orgs }
    }), { status: 200, headers });
  }

  // 2. Check if email matches existing account -> Initiate Merge Flow
  const emailUserId = await resolveUserId(db, profile.email);

  if (emailUserId) {
    const { agg, rev } = await loadAggregate(db, emailUserId);

    // Send merge OTP
    const mergeCode = await agg.requestSocialMerge(provider, profile.id, profile.email);
    await persist(db, emailUserId, agg, rev);

    await emailProvider.send(
      profile.email,
      `Link your ${provider} account`,
      `Enter this code to link your ${provider} account: <strong>${mergeCode}</strong><br/>This code expires in 10 minutes.`
    );

    return new Response(JSON.stringify({
      error: 'MergeRequired',
      message: 'An account with this email already exists. Check your email for a verification code to link your accounts.',
      email: profile.email,
      provider
    }), { status: 409 });
  }

  // 3. New User - Check self-service signup settings
  const allowSelfService = process.env.ALLOW_SELF_SERVICE_SIGNUP === 'true';
  const allowedProviders = (process.env.ALLOWED_SOCIAL_PROVIDERS || 'google,github,apple,microsoft,facebook').split(',') as SocialProvider[];

  if (!allowSelfService) {
    return new Response(JSON.stringify({
      error: 'SignupDisabled',
      message: 'Self-service signup is disabled. Please request an invite.'
    }), { status: 403 });
  }

  if (!allowedProviders.includes(provider)) {
    return new Response(JSON.stringify({
      error: 'ProviderNotAllowed',
      message: `Sign-in with ${provider} is not enabled.`
    }), { status: 403 });
  }

  // Check if the provider has verified this email
  // If email_verified is false, require email verification before allowing signup
  if (profile.email_verified === false) {
    return new Response(JSON.stringify({
      error: 'EmailNotVerified',
      message: `Your ${provider} email is not verified. Please verify your email with ${provider} before signing up.`
    }), { status: 403 });
  }

  // Create new user via social
  const userId = crypto.randomUUID();
  const agg = new IdentityAggregate();
  agg.userId = userId;
  agg.email = profile.email;
  // Only mark as verified if provider confirmed email verification
  agg.emailVerified = profile.email_verified !== false;
  agg.exists = true;
  agg.hasCompletedFirstLogin = true;
  agg.linkSocial(provider, profile.id, profile.email);

  const identityStream = `identity-${userId}`;
  const socialLookupStream = `lookup-social-${provider}-${profile.id}`;
  const emailLookupStream = `lookup-email-${await hashString(profile.email)}`;

  const socialLookupEvent: LookupEvent = { type: 'SocialReferenceCreated', userId, provider, providerId: profile.id, ts: Date.now() };
  const emailLookupEvent: LookupEvent = { type: 'UserReferenceCreated', userId, email: profile.email, ts: Date.now() };

  const auth = createAuth(authConfig);
  const sessionId = crypto.randomUUID();
  const token = await auth.sign({ sub: userId, sid: sessionId, orgs: {}, firstIat: Date.now() / 1000 }, 'access');
  const refreshToken = await auth.sign({ sub: userId, sid: sessionId, orgs: {}, firstIat: Date.now() / 1000 }, 'refresh');

  // Track the session with refresh token hash for rotation
  await agg.createSession(sessionId, refreshToken);

  // Now batch persist identity, social lookup, email lookup, and session
  const identityEvents = agg.events.map(e => Buffer.from(JSON.stringify(e)));

  await db.appendBatch([
    { streamId: identityStream, commandId: crypto.randomUUID(), expectedRev: 0, events: identityEvents },
    { streamId: socialLookupStream, commandId: crypto.randomUUID(), expectedRev: 0, events: [Buffer.from(JSON.stringify(socialLookupEvent))] },
    { streamId: emailLookupStream, commandId: crypto.randomUUID(), expectedRev: 0, events: [Buffer.from(JSON.stringify(emailLookupEvent))] }
  ], 'system');

  // Set HttpOnly cookies for token storage
  const headers = new Headers({ 'Content-Type': 'application/json' });
  setAuthCookies(headers, token, refreshToken);

  return new Response(JSON.stringify({
    status: 'success',
    userId,
    created: true,
    user: { sub: userId, orgs: {} }
  }), { status: 201, headers });
}

export async function handleAuthSocialLink(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  req: Request,
  provider: SocialProvider,
  user: AuthUser
): Promise<Response> {
  // Check if user can link (must have completed first login for invited users)
  const { agg, rev } = await loadAggregate(db, user.sub);
  if (!agg.hasCompletedFirstLogin && agg.passwordHash && Object.keys(agg.orgs).length > 0) {
    return new Response(JSON.stringify({
      error: 'FirstLoginRequired',
      message: 'Complete your first password-based login before linking social accounts.'
    }), { status: 403 });
  }

  // Check if already linked
  if (agg.socialAccounts[provider]) {
    return new Response(JSON.stringify({
      error: 'AlreadyLinked',
      message: `${provider} is already linked to your account`
    }), { status: 409 });
  }

  // Create a secure link intent with a nonce stored server-side
  let linkNonce: string;
  try {
    linkNonce = await agg.createLinkIntent(provider);
    await persist(db, user.sub, agg, rev);
  } catch (err) {
    return new Response(JSON.stringify({
      error: 'LinkFailed',
      message: (err as Error).message
    }), { status: 400 });
  }

  const social = createSocialProviders();
  const { url, state, codeVerifier } = await social.createAuthorizationURL(provider);

  // Create a signed link token containing userId and nonce
  const auth = createAuth(authConfig);
  const linkToken = await auth.sign({
    sub: user.sub,
    link_nonce: linkNonce,
    link_provider: provider,
  }, 'access');

  const isProd = process.env.NODE_ENV === 'production';
  const secureCookieFlag = isProd ? '; Secure' : '';

  const headers = new Headers();
  headers.set('Location', url.toString());
  headers.set('Set-Cookie', `spite_oauth_state=${state}; Path=/; HttpOnly; SameSite=Lax; Max-Age=300${secureCookieFlag}`);
  // Use signed token instead of raw userId
  headers.append('Set-Cookie', `spite_link_token=${linkToken}; Path=/; HttpOnly; SameSite=Lax; Max-Age=300${secureCookieFlag}`);
  if (codeVerifier) {
    headers.append('Set-Cookie', `spite_code_verifier=${codeVerifier}; Path=/; HttpOnly; SameSite=Lax; Max-Age=300${secureCookieFlag}`);
  }

  return new Response(null, { status: 302, headers });
}

export async function handleAuthSocialMergeVerify(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  body: { email: string; code: string }
): Promise<Response> {
  const userId = await resolveUserId(db, body.email);
  if (!userId) {
    return new Response(JSON.stringify({ error: 'Invalid', message: 'Invalid request' }), { status: 400 });
  }

  const { agg, rev } = await loadAggregate(db, userId);

  if (!agg.pendingMerge) {
    return new Response(JSON.stringify({ error: 'NoPendingMerge', message: 'No merge request pending' }), { status: 400 });
  }

  let mergeResult;
  try {
    mergeResult = await agg.verifySocialMerge(body.code);
  } catch (err) {
    return new Response(JSON.stringify({ error: 'InvalidCode', message: (err as Error).message }), { status: 400 });
  }

  // Create social lookup
  const socialLookupStream = `lookup-social-${mergeResult.provider}-${mergeResult.providerId}`;
  const socialLookupEvent: LookupEvent = {
    type: 'SocialReferenceCreated',
    userId,
    provider: mergeResult.provider,
    providerId: mergeResult.providerId,
    ts: Date.now()
  };

  await db.appendBatch([
    { streamId: `identity-${userId}`, commandId: crypto.randomUUID(), expectedRev: rev, events: agg.events.map(e => Buffer.from(JSON.stringify(e))) },
    { streamId: socialLookupStream, commandId: crypto.randomUUID(), expectedRev: 0, events: [Buffer.from(JSON.stringify(socialLookupEvent))] }
  ], 'system');

  return new Response(JSON.stringify({
    ok: true,
    message: `${mergeResult.provider} account linked successfully. You can now sign in with ${mergeResult.provider}.`
  }), { status: 200 });
}
