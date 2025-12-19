import type { GeneratedFile } from "../types";

export function generateAuthFile(appImportPath?: string | null): GeneratedFile {
  const appImport = appImportPath ? `import app from "${appImportPath}";` : "";
  const appAuthInit = appImportPath
    ? "const appConfig = app?.config ?? app;\nconst appAuth: SpiteStackAuthOptions = appConfig?.auth ?? {};"
    : "const appAuth: SpiteStackAuthOptions = {};";
  const content = `/**
 * Auto-generated SpiteStack auth integration
 * DO NOT EDIT - regenerate with \`spitestack compile\`
 */

import { organization } from "better-auth/plugins";
import type { SpiteDbNapi } from "@spitestack/db";
import { executeCommand, type Command, type CommandContext } from "./wiring";
${appImport}

export interface SpiteStackAuthOptions {
  organizationLimit?: number | ((user: unknown) => Promise<boolean> | boolean);
  allowUserToCreateOrganization?: boolean | ((user: unknown) => Promise<boolean> | boolean);
  emitInvitations?: boolean;
  emitUserDelete?: boolean;
  internalOrgId?: string;
  emailVerification?: {
    sendVerificationEmail?: (
      data: {
        user: unknown;
        url: string;
        token: string;
      },
      request?: Request
    ) => Promise<void>;
    sendOnSignUp?: boolean;
    sendOnSignIn?: boolean;
    autoSignInAfterVerification?: boolean;
    expiresIn?: number;
    onEmailVerification?: (user: unknown, request?: Request) => Promise<void>;
    afterEmailVerification?: (user: unknown, request?: Request) => Promise<void>;
  };
  tenantPrefix?: {
    org?: string;
    user?: string;
  };
}

${appAuthInit}

type EventEnvelope<T> = {
  data: T;
  __meta: {
    tenantId: string;
    actorId?: string | null;
  };
};

function wrapEvent<T>(event: T, tenantId: string, actorId?: string | null): EventEnvelope<T> {
  return {
    data: event,
    __meta: {
      tenantId,
      actorId: actorId ?? null,
    },
  };
}

function randomId(prefix: string): string {
  const uuid = globalThis.crypto?.randomUUID?.() ?? \`\${Date.now()}-\${Math.random()}\`;
  return \`\${prefix}:\${uuid}\`;
}

function resolveTenantId(kind: "org" | "user", id: string, prefix?: SpiteStackAuthOptions["tenantPrefix"]): string {
  const effectivePrefix = kind === "org" ? prefix?.org ?? "org:" : prefix?.user ?? "user:";
  return \`\${effectivePrefix}\${id}\`;
}

function mergeOptions(
  base: SpiteStackAuthOptions,
  override: SpiteStackAuthOptions
): SpiteStackAuthOptions {
  return {
    ...base,
    ...override,
    tenantPrefix: {
      ...(base.tenantPrefix ?? {}),
      ...(override.tenantPrefix ?? {}),
    },
  };
}

async function appendEvent<T>(
  db: SpiteDbNapi,
  input: {
    streamId: string;
    tenantId: string;
    event: T;
    actorId?: string | null;
  }
): Promise<void> {
  const payload = wrapEvent(input.event, input.tenantId, input.actorId ?? null);
  await db.append(
    input.streamId,
    randomId("spitestack"),
    -1,
    [Buffer.from(JSON.stringify(payload))],
    input.tenantId
  );
}

export function createSpiteStackAuth(db: SpiteDbNapi, options: SpiteStackAuthOptions = {}) {
  const resolvedOptions = mergeOptions(appAuth, options);
  const organizationHooks = {
    afterCreateOrganization: async ({ organization, user }: any) => {
      const tenantId = resolveTenantId("org", organization.id, resolvedOptions.tenantPrefix);
      const streamId = \`org:\${organization.id}\`;
      await appendEvent(db, {
        streamId,
        tenantId,
        actorId: user?.id ?? null,
        event: {
          type: "OrganizationCreated",
          organizationId: organization.id,
          name: organization.name ?? null,
          slug: organization.slug ?? null,
          logo: organization.logo ?? null,
          metadata: organization.metadata ?? null,
          createdBy: user?.id ?? null,
        },
      });
    },
    afterUpdateOrganization: async ({ organization, user }: any) => {
      const tenantId = resolveTenantId("org", organization.id, resolvedOptions.tenantPrefix);
      const streamId = \`org:\${organization.id}\`;
      await appendEvent(db, {
        streamId,
        tenantId,
        actorId: user?.id ?? null,
        event: {
          type: "OrganizationUpdated",
          organizationId: organization.id,
          name: organization.name ?? null,
          slug: organization.slug ?? null,
          logo: organization.logo ?? null,
          metadata: organization.metadata ?? null,
          updatedBy: user?.id ?? null,
        },
      });
    },
    afterAddMember: async ({ member, user, organization }: any) => {
      if (!organization) return;
      const tenantId = resolveTenantId("org", organization.id, resolvedOptions.tenantPrefix);
      const streamId = \`org:\${organization.id}\`;
      await appendEvent(db, {
        streamId,
        tenantId,
        actorId: user?.id ?? null,
        event: {
          type: "MemberAdded",
          organizationId: member.organizationId,
          userId: member.userId,
          role: member.role,
          addedBy: user?.id ?? null,
        },
      });
    },
    afterRemoveMember: async ({ member, user, organization }: any) => {
      if (!organization) return;
      const tenantId = resolveTenantId("org", organization.id, resolvedOptions.tenantPrefix);
      const streamId = \`org:\${organization.id}\`;
      await appendEvent(db, {
        streamId,
        tenantId,
        actorId: user?.id ?? null,
        event: {
          type: "MemberRemoved",
          organizationId: member.organizationId,
          userId: member.userId,
          removedBy: user?.id ?? null,
        },
      });
    },
    afterUpdateMemberRole: async ({ member, previousRole, user, organization }: any) => {
      if (!organization) return;
      const tenantId = resolveTenantId("org", organization.id, resolvedOptions.tenantPrefix);
      const streamId = \`org:\${organization.id}\`;
      await appendEvent(db, {
        streamId,
        tenantId,
        actorId: user?.id ?? null,
        event: {
          type: "MemberRoleChanged",
          organizationId: member.organizationId,
          userId: member.userId,
          role: member.role,
          previousRole: previousRole ?? null,
          updatedBy: user?.id ?? null,
        },
      });
    },
    afterCreateInvitation: async ({ invitation, inviter, organization }: any) => {
      if (!resolvedOptions.emitInvitations || !organization) return;
      const tenantId = resolveTenantId("org", organization.id, resolvedOptions.tenantPrefix);
      const streamId = \`org:\${organization.id}\`;
      await appendEvent(db, {
        streamId,
        tenantId,
        actorId: inviter?.id ?? null,
        event: {
          type: "InvitationCreated",
          organizationId: invitation.organizationId,
          invitationId: invitation.id,
          email: invitation.email,
          role: invitation.role,
          inviterId: invitation.inviterId ?? inviter?.id ?? null,
        },
      });
    },
    afterAcceptInvitation: async ({ invitation, member, user, organization }: any) => {
      if (!organization) return;
      const tenantId = resolveTenantId("org", organization.id, resolvedOptions.tenantPrefix);
      const streamId = \`org:\${organization.id}\`;
      await appendEvent(db, {
        streamId,
        tenantId,
        actorId: user?.id ?? null,
        event: {
          type: "InvitationAccepted",
          organizationId: invitation.organizationId,
          invitationId: invitation.id,
          userId: member.userId,
          role: member.role,
        },
      });
    },
  };

  const databaseHooks = {
    user: {
      create: {
        after: async (user: any) => {
          const tenantId = resolveTenantId("user", user.id, resolvedOptions.tenantPrefix);
          const streamId = \`user:\${user.id}\`;
          await appendEvent(db, {
            streamId,
            tenantId,
            actorId: user.id,
            event: {
              type: "UserCreated",
              userId: user.id,
              email: user.email ?? null,
              name: user.name ?? null,
            },
          });
        },
      },
      update: {
        after: async (user: any) => {
          const tenantId = resolveTenantId("user", user.id, resolvedOptions.tenantPrefix);
          const streamId = \`user:\${user.id}\`;
          await appendEvent(db, {
            streamId,
            tenantId,
            actorId: user.id,
            event: {
              type: "UserUpdated",
              userId: user.id,
              email: user.email ?? null,
              name: user.name ?? null,
            },
          });
        },
      },
      delete: resolvedOptions.emitUserDelete
        ? {
            after: async (user: any) => {
              const tenantId = resolveTenantId("user", user.id, resolvedOptions.tenantPrefix);
              const streamId = \`user:\${user.id}\`;
              await appendEvent(db, {
                streamId,
                tenantId,
                actorId: user.id,
                event: {
                  type: "UserDeleted",
                  userId: user.id,
                },
              });
            },
          }
        : undefined,
    },
  };

  return {
    plugins: [
      organization({
        organizationHooks,
        organizationLimit: resolvedOptions.organizationLimit ?? 1,
        allowUserToCreateOrganization: resolvedOptions.allowUserToCreateOrganization,
      }),
    ],
    databaseHooks,
    emailVerification: resolvedOptions.emailVerification,
  };
}

export function createSpiteStackApp(options: { db: SpiteDbNapi; auth?: SpiteStackAuthOptions }) {
  const auth = createSpiteStackAuth(options.db, options.auth);
  return {
    db: options.db,
    auth,
    executeCommand: (ctx: CommandContext, command: Command) => executeCommand(ctx, command),
  };
}
`;

  return {
    path: "auth.ts",
    content,
  };
}
