
import { type SpiteDbNapi, type TelemetryDbNapi } from '@spitestack/db';
import { type AuthConfig, createAuth, type AuthUser } from './auth';
import type { EmailProvider } from './email';
import { emitTelemetry, metricCounter } from './telemetry';
import type { SocialProvider } from './social';

export const SYSTEM_TENANT_ID = 'system';
export const SYSTEM_TENANT_NAME = 'System';
export const SYSTEM_ADMIN_ROLE = 'admin';

// =============================================================================
// Utilities
// =============================================================================

function generateCode(): string {
  // 32-char hex string for invite links
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Buffer.from(bytes).toString('hex');
}

// =============================================================================
// Events
// =============================================================================

export type TenantEvent =
  | { type: 'TenantCreated'; tenantId: string; name: string; createdBy: string; ts: number }
  | { type: 'TenantUpdated'; tenantId: string; name?: string; ts: number }
  | { type: 'TenantSettingsUpdated'; tenantId: string; settings: Partial<TenantSettings>; ts: number }
  | { type: 'UserInvited'; inviteId: string; tenantId: string; email: string; role: string; codeHash: string; invitedBy: string; expiresAt: number; ts: number }
  | { type: 'InviteAccepted'; inviteId: string; tenantId: string; userId: string; email: string; ts: number }
  | { type: 'InviteRevoked'; inviteId: string; tenantId: string; ts: number }
  | { type: 'UserRemoved'; tenantId: string; userId: string; ts: number };

// Tenant settings for self-service signup and social providers
export interface TenantSettings {
  allowSelfServiceSignup: boolean;
  allowedSocialProviders: SocialProvider[];
}

// We also need to emit RoleAssigned to the Identity stream
// We'll import the IdentityEvent type definition conceptually, or redefine the minimal part needed
type IdentityRoleEvent = { type: 'RoleAssigned'; userId: string; tenantId: string; role: string; ts: number };

// =============================================================================
// Aggregate
// =============================================================================

export interface InviteState {
  email: string;
  role: string;
  codeHash: string;
  expiresAt: number;
}

export class TenantAggregate {
  tenantId: string = '';
  name: string = '';
  users: Record<string, string> = {}; // userId -> role
  invites: Record<string, InviteState> = {}; // inviteId -> state
  exists: boolean = false;
  settings: TenantSettings = {
    allowSelfServiceSignup: false,
    allowedSocialProviders: [],
  };

  apply(event: TenantEvent) {
    switch (event.type) {
      case 'TenantCreated':
        this.tenantId = event.tenantId;
        this.name = event.name;
        this.exists = true;
        // Creator is usually auto-added as owner, but we might track that via InviteAccepted or explicit add logic
        // For 'TenantCreated', we assume the handler handles the Identity side 'RoleAssigned'.
        // We can track it here too if we want a user list.
        this.users[event.createdBy] = 'owner';
        break;
      case 'TenantUpdated':
        if (event.name) this.name = event.name;
        break;
      case 'TenantSettingsUpdated':
        this.settings = { ...this.settings, ...event.settings };
        break;
      case 'UserInvited':
        this.invites[event.inviteId] = {
          email: event.email,
          role: event.role,
          codeHash: event.codeHash,
          expiresAt: event.expiresAt
        };
        break;
      case 'InviteAccepted':
        const invite = this.invites[event.inviteId];
        if (invite) {
          this.users[event.userId] = invite.role;
          delete this.invites[event.inviteId];
        }
        break;
      case 'InviteRevoked':
        delete this.invites[event.inviteId];
        break;
      case 'UserRemoved':
        delete this.users[event.userId];
        break;
    }
  }

  get events(): TenantEvent[] { return this._events; }
  private _events: TenantEvent[] = [];

  create(tenantId: string, name: string, createdBy: string) {
    if (this.exists) throw new Error('Tenant already exists');
    this._events.push({
      type: 'TenantCreated',
      tenantId,
      name,
      createdBy,
      ts: Date.now()
    });
  }

  async inviteUser(email: string, role: string, invitedBy: string) {
    if (!this.exists) throw new Error('Tenant does not exist');
    // Check permissions? Assuming caller (handler) checked or 'invitedBy' is in 'users' with sufficient role.
    // We'll enforce simple logic: inviter must be current user. 
    // Ideally we check if 'invitedBy' has 'admin' or 'owner' role in 'this.users'.
    const inviterRole = this.users[invitedBy];
    if (inviterRole !== 'owner' && inviterRole !== 'admin') {
       // Handler may implement additional admin controls, but aggregate logic is strict
       // We'll throw here; handler must ensure state is hydrated correctly
       throw new Error('Insufficient permissions to invite');
    }

    const code = generateCode();
    const codeHash = await Bun.password.hash(code);
    const inviteId = crypto.randomUUID();
    
    this._events.push({
      type: 'UserInvited',
      inviteId,
      tenantId: this.tenantId,
      email,
      role,
      codeHash,
      invitedBy,
      expiresAt: Date.now() + 1000 * 60 * 60 * 24 * 7, // 7 days
      ts: Date.now()
    });

    return { code, inviteId };
  }

  async acceptInvite(inviteId: string, code: string, userId: string, userEmail: string) {
    if (!this.exists) throw new Error('Tenant does not exist');
    const invite = this.invites[inviteId];
    if (!invite) throw new Error('Invite not found');
    
    if (Date.now() > invite.expiresAt) throw new Error('Invite expired');
    // Optional: Check if userEmail matches invite.email. 
    // Security trade-off: forcing match prevents link sharing, but annoying if user has multiple aliases.
    // Let's enforce it for security.
    if (invite.email.toLowerCase() !== userEmail.toLowerCase()) {
      throw new Error(`Invite is for ${invite.email}, not ${userEmail}`);
    }

    const valid = await Bun.password.verify(code, invite.codeHash);
    if (!valid) throw new Error('Invalid invite code');

    this._events.push({
      type: 'InviteAccepted',
      inviteId,
      tenantId: this.tenantId,
      userId,
      email: userEmail,
      ts: Date.now()
    });
    
    return invite.role;
  }

  updateSettings(settings: Partial<TenantSettings>) {
    if (!this.exists) throw new Error('Tenant does not exist');
    this._events.push({
      type: 'TenantSettingsUpdated',
      tenantId: this.tenantId,
      settings,
      ts: Date.now()
    });
  }
}

// =============================================================================
// Helpers
// =============================================================================

async function loadTenant(db: SpiteDbNapi, tenantId: string): Promise<{ agg: TenantAggregate, rev: number }> {
  const streamId = `tenant-${tenantId}`;
  const events = await db.readStream(streamId, 0, 1000, 'system'); // System tenant reads all
  const agg = new TenantAggregate();
  for (const e of events) agg.apply(JSON.parse(e.data.toString()) as TenantEvent);
  const rev = events.length > 0 ? events[events.length - 1].streamRev : 0;
  return { agg, rev };
}

// =============================================================================
// Handlers
// =============================================================================

/**
 * Creates a new tenant. 
 * Self-service: Any auth user can create.
 * System admin: Can create for others (not impl here, simple flow is current user becomes owner).
 */
export async function handleTenantCreate(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  body: any,
  user: AuthUser
): Promise<Response> {
  const { name } = body;
  if (!name) return new Response('Name required', { status: 400 });

  const tenantId = crypto.randomUUID();
  const agg = new TenantAggregate();
  agg.create(tenantId, name, user.sub);

  const tenantStream = `tenant-${tenantId}`;
  const userStream = `identity-${user.sub}`;
  
  const tenantEvents = agg.events.map(e => Buffer.from(JSON.stringify(e)));
  
  // Side effect: Assign Role 'owner' to User
  const roleEvent: IdentityRoleEvent = { 
    type: 'RoleAssigned', 
    userId: user.sub, 
    tenantId, 
    role: 'owner', 
    ts: Date.now() 
  };
  const userEvents = [Buffer.from(JSON.stringify(roleEvent))];

  const cmdId = crypto.randomUUID();
  
  // Atomic commit across tenant and user streams
  await db.appendBatch([
    { streamId: tenantStream, commandId: cmdId, expectedRev: 0, events: tenantEvents },
    { streamId: userStream, commandId: cmdId, expectedRev: -1, events: userEvents } // -1 (Any) rev for user as they might be active
  ], 'system');

  return new Response(JSON.stringify({ tenantId, name }), { status: 201 });
}

/**
 * Invite a user to the tenant.
 * Requires: 'owner' or 'admin' role in that tenant.
 */
export async function handleTenantInvite(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  emailProvider: EmailProvider,
  body: any,
  user: AuthUser
): Promise<Response> {
  const { tenantId, email, role } = body;
  if (!tenantId || !email || !role) return new Response('Missing fields', { status: 400 });

  // Verify permission
  const auth = createAuth(authConfig);
  if (!auth.hasRole(user, tenantId, 'owner') && !auth.hasRole(user, tenantId, 'admin')) {
    return new Response('Forbidden', { status: 403 });
  }

  const { agg, rev } = await loadTenant(db, tenantId);
  if (!agg.exists) return new Response('Tenant not found', { status: 404 });

  let code, inviteId;
  try {
    const res = await agg.inviteUser(email, role, user.sub);
    code = res.code;
    inviteId = res.inviteId;
  } catch (e) {
    return new Response((e as Error).message, { status: 400 });
  }

  const streamId = `tenant-${tenantId}`;
  const cmdId = crypto.randomUUID();
  const eventBuffers = agg.events.map(e => Buffer.from(JSON.stringify(e)));
  await db.append(streamId, cmdId, rev, eventBuffers, 'system');

  // Send Email
  // In real app, domain would be configurable
  const link = `http://localhost:3000/invite?tenant=${tenantId}&invite=${inviteId}&code=${code}`; 
  await emailProvider.send(email, `You've been invited to ${agg.name}`, 
    `You have been invited to join <b>${agg.name}</b> as <b>${role}</b>.<br/>
     Click here to accept: <a href="${link}">${link}</a>`
  );

  return new Response(JSON.stringify({ inviteId }), { status: 200 });
}

/**
 * Accept an invite.
 * User must be logged in. If not, frontend should redirect to login/register then call this.
 */
export async function handleTenantAcceptInvite(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  body: any,
  user: AuthUser, // Authenticated user accepting the invite
  userEmail?: string // Passed from context or looked up if not in token. For now we assume token has email or we rely on lookup.
): Promise<Response> {
  // NOTE: AuthUser in token doesn't standardly have 'email' in our minimal schema, only 'sub'.
  // We need to fetch the user's email from their identity stream to verify ownership.
  // Or we trust the authenticated user is the one accepting it, but we enforce "invite email matches user email" logic 
  // inside aggregate which requires passing it in.
  
  const { tenantId, inviteId, code } = body;
  
  // Lookup user email from IdentityAggregate
  const identityStream = `identity-${user.sub}`;
  const identityEvents = await db.readStream(identityStream, 0, 1000, 'system');
  let resolvedEmail = '';
  let emailVerified = false;
  for (const e of identityEvents) {
    const evt = JSON.parse(e.data.toString());
    if (evt.type === 'UserRegistered') resolvedEmail = evt.email;
    if (evt.type === 'EmailVerified') emailVerified = true;
  }
  
  if (!resolvedEmail) return new Response('User profile error', { status: 500 });

  const { agg, rev } = await loadTenant(db, tenantId);
  let assignedRole;
  try {
    assignedRole = await agg.acceptInvite(inviteId, code, user.sub, resolvedEmail);
  } catch (e) {
    return new Response((e as Error).message, { status: 400 });
  }

  const tenantStream = `tenant-${tenantId}`;
  const tenantEvents = agg.events.map(e => Buffer.from(JSON.stringify(e)));

  const roleEvent: IdentityRoleEvent = { 
    type: 'RoleAssigned', 
    userId: user.sub, 
    tenantId, 
    role: assignedRole, 
    ts: Date.now() 
  };
  
  const userEvents = [Buffer.from(JSON.stringify(roleEvent))];

  // Auto-verify email if not already verified
  if (!emailVerified) {
    const verifyEvent = { type: 'EmailVerified', userId: user.sub, ts: Date.now() };
    userEvents.push(Buffer.from(JSON.stringify(verifyEvent)));
  }

  const cmdId = crypto.randomUUID();
  await db.appendBatch([
    { streamId: tenantStream, commandId: cmdId, expectedRev: rev, events: tenantEvents },
    { streamId: identityStream, commandId: cmdId, expectedRev: identityEvents.length > 0 ? identityEvents[identityEvents.length-1].streamRev : -1, events: userEvents }
  ], 'system');

  return new Response(JSON.stringify({ ok: true, tenantId, role: assignedRole, emailVerified: !emailVerified }), { status: 200 });
}

/**
 * Update tenant settings.
 * Requires: 'owner' role in that tenant.
 */
export async function handleTenantUpdateSettings(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  body: any,
  user: AuthUser
): Promise<Response> {
  const { tenantId, settings } = body;
  if (!tenantId || !settings) return new Response('Missing fields', { status: 400 });

  // Only owners can update settings
  const auth = createAuth(authConfig);
  if (!auth.hasRole(user, tenantId, 'owner')) {
    return new Response('Forbidden - only owners can update settings', { status: 403 });
  }

  const { agg, rev } = await loadTenant(db, tenantId);
  if (!agg.exists) return new Response('Tenant not found', { status: 404 });

  // Validate settings
  const validSettings: Partial<TenantSettings> = {};
  if (typeof settings.allowSelfServiceSignup === 'boolean') {
    validSettings.allowSelfServiceSignup = settings.allowSelfServiceSignup;
  }
  if (Array.isArray(settings.allowedSocialProviders)) {
    const validProviders: SocialProvider[] = ['google', 'github', 'apple', 'microsoft', 'facebook'];
    validSettings.allowedSocialProviders = settings.allowedSocialProviders.filter(
      (p: string) => validProviders.includes(p as SocialProvider)
    ) as SocialProvider[];
  }

  if (Object.keys(validSettings).length === 0) {
    return new Response('No valid settings provided', { status: 400 });
  }

  agg.updateSettings(validSettings);

  const streamId = `tenant-${tenantId}`;
  const cmdId = crypto.randomUUID();
  const eventBuffers = agg.events.map(e => Buffer.from(JSON.stringify(e)));
  await db.append(streamId, cmdId, rev, eventBuffers, 'system');

  return new Response(JSON.stringify({ ok: true, settings: { ...agg.settings, ...validSettings } }), { status: 200 });
}

/**
 * Get tenant settings.
 * Requires: membership in that tenant.
 */
export async function handleTenantGetSettings(
  db: SpiteDbNapi,
  telemetry: TelemetryDbNapi,
  authConfig: AuthConfig,
  tenantId: string,
  user: AuthUser
): Promise<Response> {
  // Check if user has access to this tenant
  const auth = createAuth(authConfig);
  const hasAccess = user.orgs?.[tenantId] || user.tenant === tenantId;
  if (!hasAccess) {
    return new Response('Forbidden', { status: 403 });
  }

  const { agg } = await loadTenant(db, tenantId);
  if (!agg.exists) return new Response('Tenant not found', { status: 404 });

  return new Response(JSON.stringify({ settings: agg.settings }), { status: 200 });
}
