//! Bun.serve router code generation for TypeScript.

use crate::ir::{AccessLevel, DomainIR};
use super::ts_types::{to_snake_case, to_pascal_case};

/// Generates the main router that wires up all handlers.
pub fn generate_router(domain: &DomainIR) -> String {
    let mut output = String::new();

    // Imports
    output.push_str("import type { SpiteDbNapi, TelemetryDbNapi } from '@spitestack/db';\n");
    output.push_str("import type { AuthConfig } from './runtime/auth';\n");
    output.push_str("import { createAuth, resolveTenant } from './runtime/auth';\n");
    output.push_str("import { createEmailProvider } from './runtime/email';\n");
    output.push_str("import { createSmsProvider } from './runtime/sms';\n");
    output.push_str("import { handleAuthRegister, handleAuthLogin, handleAuthRefresh, handleAuthVerifyEmail, handleAuthRequestRecovery, handleAuthResetPassword, handleAuthChangePassword, handleAuthMfaChallenge, handleAuthMfaVerify, handleAuthMfaEnroll, handleAuthMfaEnrollChallenge, handleAuthSocialLogin, handleAuthSocialCallback, handleAuthSocialLink, handleAuthSocialMergeVerify } from './runtime/identity';\n");
    output.push_str("import type { SocialProvider } from './runtime/social';\n");
    output.push_str("import { handleTenantCreate, handleTenantInvite, handleTenantAcceptInvite, handleTenantUpdateSettings, handleTenantGetSettings, SYSTEM_TENANT_ID } from './runtime/tenant';\n");
    output.push_str(
        "import { emitTelemetry, finishSpan, logError, metricCounter, metricHistogram, startSpan } from './runtime/telemetry';\n",
    );
    output.push_str("import { getSecurityHeaders } from './runtime/security-headers';\n");

    // Import handlers for each aggregate
    for aggregate in &domain.aggregates {
        let snake_name = to_snake_case(&aggregate.name);

        let handler_names: Vec<String> = aggregate
            .commands
            .iter()
            .map(|cmd| format!("handle{}{}", aggregate.name, to_pascal_case(&cmd.name)))
            .chain(std::iter::once(format!("handle{}Get", aggregate.name)))
            .collect();

        output.push_str(&format!(
            "import {{ {} }} from './handlers/{}.handlers';\n",
            handler_names.join(", "),
            snake_name
        ));
    }

    output.push('\n');

    // Router context type
    output.push_str("export type RouterContext = {\n");
    output.push_str("  db: SpiteDbNapi;\n");
    output.push_str("  telemetry: TelemetryDbNapi;\n");
    output.push_str("  authConfig: AuthConfig;\n");
    output.push_str("};\n\n");

    // Router function
    output.push_str("export function createRouter(ctx: RouterContext) {\n");
    output.push_str("  const auth = createAuth(ctx.authConfig);\n");
    output.push_str("  const emailProvider = createEmailProvider();\n");
    output.push_str("  const smsProvider = createSmsProvider();\n");
    output.push_str("  const securityHeaders = getSecurityHeaders();\n\n");
    output.push_str("  // Helper to apply security headers to all responses\n");
    output.push_str("  const finalize = (response: Response): Response => {\n");
    output.push_str("    const headers = new Headers(response.headers);\n");
    output.push_str("    for (const [key, value] of Object.entries(securityHeaders)) {\n");
    output.push_str("      if (!headers.has(key)) headers.set(key, value);\n");
    output.push_str("    }\n");
    output.push_str("    return new Response(response.body, { status: response.status, statusText: response.statusText, headers });\n");
    output.push_str("  };\n\n");
    output.push_str("  return async (req: Request): Promise<Response> => {\n");
    output.push_str("    const url = new URL(req.url);\n");
    output.push_str("    const path = url.pathname;\n");
    output.push_str("    const method = req.method;\n");
    output.push_str("    const isProd = process.env.NODE_ENV === 'production';\n\n");
    
    // Auth check
    output.push_str("    // Authenticate request\n");
    output.push_str("    const authResult = await auth.verifyRequest(req);\n\n");

    // Public Auth Routes (Login, Register, MFA Challenge/Verify)
    output.push_str("    const isAuthRoute = path.startsWith('/auth/');\n");

    output.push_str("    if (isAuthRoute) {\n");
    output.push_str("       const body = method === 'POST' ? await req.json().catch(() => ({})) : {};\n");
    output.push_str("       // Public endpoints\n");
    output.push_str("       if (method === 'POST' && path === '/auth/refresh') {\n");
    output.push_str("         const response = await handleAuthRefresh(ctx.db, ctx.telemetry, ctx.authConfig, emailProvider, smsProvider, body);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n");
    output.push_str("       if (method === 'POST' && path === '/auth/login') {\n");
    output.push_str("         const response = await handleAuthLogin(ctx.db, ctx.telemetry, ctx.authConfig, emailProvider, smsProvider, body, req);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n");
    output.push_str("       if (method === 'POST' && path === '/auth/register') {\n");
    output.push_str("         const response = await handleAuthRegister(ctx.db, ctx.telemetry, ctx.authConfig, emailProvider, smsProvider, body, req);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n");
    output.push_str("       if (method === 'POST' && path === '/auth/verify-email') {\n");
    output.push_str("         const response = await handleAuthVerifyEmail(ctx.db, ctx.telemetry, ctx.authConfig, emailProvider, smsProvider, body, req);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n");
    output.push_str("       if (method === 'POST' && path === '/auth/recover') {\n");
    output.push_str("         const response = await handleAuthRequestRecovery(ctx.db, ctx.telemetry, ctx.authConfig, emailProvider, smsProvider, body, req);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n");
    output.push_str("       if (method === 'POST' && path === '/auth/reset') {\n");
    output.push_str("         const response = await handleAuthResetPassword(ctx.db, ctx.telemetry, ctx.authConfig, emailProvider, smsProvider, body, req);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n");
    output.push_str("       if (method === 'POST' && path === '/auth/change-password') {\n");
    output.push_str("         const response = await handleAuthChangePassword(ctx.db, ctx.telemetry, ctx.authConfig, emailProvider, smsProvider, body, req);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n");
    output.push_str("       if (method === 'POST' && path === '/auth/mfa/challenge') {\n");
    output.push_str("         const response = await handleAuthMfaChallenge(ctx.db, ctx.telemetry, ctx.authConfig, emailProvider, smsProvider, body, req);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n");
    output.push_str("       if (method === 'POST' && path === '/auth/mfa/verify') {\n");
    output.push_str("         const response = await handleAuthMfaVerify(ctx.db, ctx.telemetry, ctx.authConfig, emailProvider, smsProvider, body, req);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n");
    
    output.push_str("       // Protected endpoints (Enroll) - requires fully authenticated session, not mfa_pending\n");
    output.push_str("       if (method === 'POST' && path === '/auth/mfa/enroll/challenge') {\n");
    output.push_str("         if (!authResult.ok) return finalize(new Response('Unauthorized', { status: 401 }));\n");
    output.push_str("         if (authResult.user.mfa_pending) {\n");
    output.push_str("           return finalize(new Response(JSON.stringify({ error: 'Complete MFA verification before enrolling new authenticators' }), { status: 403, headers: { 'Content-Type': 'application/json' } }));\n");
    output.push_str("         }\n");
    output.push_str("         const response = await handleAuthMfaEnrollChallenge(ctx.db, ctx.telemetry, ctx.authConfig, emailProvider, smsProvider, body, authResult.user);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n");
    output.push_str("       if (method === 'POST' && path === '/auth/mfa/enroll') {\n");
    output.push_str("         if (!authResult.ok) return finalize(new Response('Unauthorized', { status: 401 }));\n");
    output.push_str("         if (authResult.user.mfa_pending) {\n");
    output.push_str("           return finalize(new Response(JSON.stringify({ error: 'Complete MFA verification before enrolling new authenticators' }), { status: 403, headers: { 'Content-Type': 'application/json' } }));\n");
    output.push_str("         }\n");
    output.push_str("         const response = await handleAuthMfaEnroll(ctx.db, ctx.telemetry, ctx.authConfig, emailProvider, smsProvider, body, authResult.user);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n\n");

    // Social auth routes
    output.push_str("       // Social auth routes\n");
    output.push_str("       const socialMatch = path.match(/^\\/auth\\/social\\/(google|github|apple|microsoft|facebook)(\\/callback|\\/link)?$/);\n");
    output.push_str("       if (socialMatch) {\n");
    output.push_str("         const provider = socialMatch[1] as SocialProvider;\n");
    output.push_str("         const action = socialMatch[2];\n\n");
    output.push_str("         // Initiate social login (public)\n");
    output.push_str("         if (method === 'GET' && !action) {\n");
    output.push_str("           const response = await handleAuthSocialLogin(provider);\n");
    output.push_str("           return response;\n");
    output.push_str("         }\n\n");
    output.push_str("         // OAuth callback (public)\n");
    output.push_str("         if (method === 'GET' && action === '/callback') {\n");
    output.push_str("           const response = await handleAuthSocialCallback(ctx.db, ctx.telemetry, ctx.authConfig, emailProvider, req, provider);\n");
    output.push_str("           return response;\n");
    output.push_str("         }\n\n");
    output.push_str("         // Link social account (protected)\n");
    output.push_str("         if (method === 'GET' && action === '/link') {\n");
    output.push_str("           if (!authResult.ok) return new Response('Unauthorized', { status: 401 });\n");
    output.push_str("           const response = await handleAuthSocialLink(ctx.db, ctx.telemetry, ctx.authConfig, req, provider, authResult.user);\n");
    output.push_str("           return response;\n");
    output.push_str("         }\n");
    output.push_str("       }\n\n");

    // Social merge verify route
    output.push_str("       // Social merge verify (public)\n");
    output.push_str("       if (method === 'POST' && path === '/auth/social/merge/verify') {\n");
    output.push_str("         const response = await handleAuthSocialMergeVerify(ctx.db, ctx.telemetry, body);\n");
    output.push_str("         return response;\n");
    output.push_str("       }\n");
    output.push_str("    }\n\n");

    // Tenant Routes
    output.push_str("    if (path.startsWith('/tenant')) {\n");
    output.push_str("       const body = method === 'POST' ? await req.json().catch(() => ({})) : {};\n");
    output.push_str("       if (method === 'POST' && path === '/tenant') {\n");
    output.push_str("         if (!authResult.ok) return finalize(new Response('Unauthorized', { status: 401 }));\n");
    output.push_str("         const response = await handleTenantCreate(ctx.db, ctx.telemetry, ctx.authConfig, body, authResult.user);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n");
    output.push_str("       if (method === 'POST' && path === '/tenant/invite') {\n");
    output.push_str("         if (!authResult.ok) return finalize(new Response('Unauthorized', { status: 401 }));\n");
    output.push_str("         const response = await handleTenantInvite(ctx.db, ctx.telemetry, ctx.authConfig, emailProvider, body, authResult.user);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n");
    output.push_str("       if (method === 'POST' && path === '/tenant/invite/accept') {\n");
    output.push_str("         if (!authResult.ok) return finalize(new Response('Unauthorized', { status: 401 }));\n");
    output.push_str("         const response = await handleTenantAcceptInvite(ctx.db, ctx.telemetry, ctx.authConfig, body, authResult.user);\n");
    output.push_str("         return finalize(response);\n");
    output.push_str("       }\n\n");

    // Tenant settings routes
    output.push_str("       // Tenant settings routes\n");
    output.push_str("       const tenantSettingsMatch = path.match(/^\\/tenant\\/([^/]+)\\/settings$/);\n");
    output.push_str("       if (tenantSettingsMatch) {\n");
    output.push_str("         const tenantId = tenantSettingsMatch[1];\n");
    output.push_str("         if (!authResult.ok) return new Response('Unauthorized', { status: 401 });\n\n");
    output.push_str("         // GET tenant settings\n");
    output.push_str("         if (method === 'GET') {\n");
    output.push_str("           const response = await handleTenantGetSettings(ctx.db, ctx.telemetry, ctx.authConfig, tenantId, authResult.user);\n");
    output.push_str("           return response;\n");
    output.push_str("         }\n\n");
    output.push_str("         // UPDATE tenant settings (POST or PATCH)\n");
    output.push_str("         if (method === 'POST' || method === 'PATCH') {\n");
    output.push_str("           const settingsBody = await req.json().catch(() => ({}));\n");
    output.push_str("           const response = await handleTenantUpdateSettings(ctx.db, ctx.telemetry, ctx.authConfig, { tenantId, settings: settingsBody }, authResult.user);\n");
    output.push_str("           return response;\n");
    output.push_str("         }\n");
    output.push_str("       }\n");
    output.push_str("    }\n\n");

    // Helper functions for access control
    output.push_str("    // Access control helpers\n");
    output.push_str("    const checkAuth = (): Response | null => {\n");
    output.push_str("      if (!authResult.ok) {\n");
    output.push_str("        return new Response(JSON.stringify({ error: authResult.error }), {\n");
    output.push_str("          status: 401,\n");
    output.push_str("          headers: { 'Content-Type': 'application/json' }\n");
    output.push_str("        });\n");
    output.push_str("      }\n");
    output.push_str("      return null;\n");
    output.push_str("    };\n\n");

    output.push_str("    const checkInternal = (): Response | null => {\n");
    output.push_str("      const authErr = checkAuth();\n");
    output.push_str("      if (authErr) return authErr;\n");
    output.push_str("      const systemMembership = authResult.user.orgs?.[SYSTEM_TENANT_ID];\n");
    output.push_str("      if (!systemMembership) {\n");
    output.push_str("        return new Response(JSON.stringify({ error: 'Forbidden: internal endpoint' }), {\n");
    output.push_str("          status: 403,\n");
    output.push_str("          headers: { 'Content-Type': 'application/json' }\n");
    output.push_str("        });\n");
    output.push_str("      }\n");
    output.push_str("      return null;\n");
    output.push_str("    };\n\n");

    output.push_str("    const checkPrivate = (): { error: Response } | { user: typeof authResult.user; tenant: string } => {\n");
    output.push_str("      const authErr = checkAuth();\n");
    output.push_str("      if (authErr) return { error: authErr };\n");
    output.push_str("      const tenantResult = resolveTenant(req, authResult.user);\n");
    output.push_str("      if (!tenantResult.ok) {\n");
    output.push_str("        return { error: new Response(JSON.stringify({ error: tenantResult.error }), {\n");
    output.push_str("          status: tenantResult.status,\n");
    output.push_str("          headers: { 'Content-Type': 'application/json' }\n");
    output.push_str("        }) };\n");
    output.push_str("      }\n");
    output.push_str("      return { user: authResult.user, tenant: tenantResult.tenant };\n");
    output.push_str("    };\n\n");

    output.push_str("    const checkRoles = (user: typeof authResult.user, tenant: string | undefined, roles: string[]): Response | null => {\n");
    output.push_str("      if (roles.length === 0) return null;\n");
    output.push_str("      const userRoles = tenant && user.orgs?.[tenant]?.roles ? user.orgs[tenant].roles : (user.roles || []);\n");
    output.push_str("      const hasRole = roles.some(r => userRoles.includes(r));\n");
    output.push_str("      if (!hasRole) {\n");
    output.push_str("        return new Response(JSON.stringify({ error: 'Forbidden: insufficient role' }), {\n");
    output.push_str("          status: 403,\n");
    output.push_str("          headers: { 'Content-Type': 'application/json' }\n");
    output.push_str("        });\n");
    output.push_str("      }\n");
    output.push_str("      return null;\n");
    output.push_str("    };\n\n");

    // Telemetry helper
    output.push_str("    const createFinalize = (tenant: string, user?: { sub?: string }) => {\n");
    output.push_str("      const traceId = crypto.randomUUID();\n");
    output.push_str("      const requestSpan = startSpan(tenant, traceId, 'http.request', undefined, { method, path, tenant, user: user?.sub || 'anonymous' });\n");
    output.push_str("      return {\n");
    output.push_str("        traceId,\n");
    output.push_str("        spanId: requestSpan.spanId,\n");
    output.push_str("        finalize: (response: Response, err?: unknown): Response => {\n");
    output.push_str("          const endMs = Date.now();\n");
    output.push_str("          const durationMs = Math.max(0, endMs - requestSpan.startMs);\n");
    output.push_str("          const attrs = { method, path, status: response.status, tenant };\n");
    output.push_str("          const records = [\n");
    output.push_str("            finishSpan(requestSpan, response.status >= 500 ? 'Error' : 'Ok', endMs, attrs),\n");
    output.push_str("            metricCounter(tenant, 'http.request.count', 1, attrs, traceId, requestSpan.spanId),\n");
    output.push_str("            metricHistogram(tenant, 'http.request.duration_ms', durationMs, attrs, traceId, requestSpan.spanId),\n");
    output.push_str("          ];\n");
    output.push_str("          if (err || response.status >= 500) {\n");
    output.push_str("            const message = err instanceof Error ? err.message : 'request failed';\n");
    output.push_str("            records.push(logError(tenant, message, attrs, traceId, requestSpan.spanId));\n");
    output.push_str("            if (isProd && response.status >= 500) {\n");
    output.push_str("              response = new Response(JSON.stringify({ error: 'Internal Server Error' }), {\n");
    output.push_str("                status: response.status,\n");
    output.push_str("                headers: { 'Content-Type': 'application/json' }\n");
    output.push_str("              });\n");
    output.push_str("            }\n");
    output.push_str("          }\n");
    output.push_str("          emitTelemetry(ctx.telemetry, records);\n");
    output.push_str("          return response;\n");
    output.push_str("        }\n");
    output.push_str("      };\n");
    output.push_str("    };\n\n");

    // Generate route matching for each aggregate
    output.push_str("    try {\n");
    for aggregate in &domain.aggregates {
        let snake_name = to_snake_case(&aggregate.name);

        output.push_str(&format!(
            "      // {} routes\n",
            aggregate.name
        ));
        output.push_str(&format!(
            "      const {}Match = path.match(/^\\/{}\\/([^/]+)(?:\\/([^/]+))?$/);\n",
            snake_name, snake_name
        ));
        output.push_str(&format!("      if ({}Match) {{\n", snake_name));
        output.push_str(&format!("        const streamId = {}Match[1];\n", snake_name));
        output.push_str(&format!("        const action = {}Match[2];\n\n", snake_name));

        // GET handler - default to Internal access
        output.push_str("        if (method === 'GET' && !action) {\n");
        output.push_str("          // GET is Internal by default\n");
        output.push_str("          const accessErr = checkInternal();\n");
        output.push_str("          if (accessErr) return accessErr;\n");
        output.push_str("          const { traceId, spanId, finalize } = createFinalize('system', authResult.user);\n");
        output.push_str("          const handlerCtx = { ...ctx, tenant: 'system' };\n");
        output.push_str(&format!(
            "          const response = await handle{}Get(handlerCtx, streamId, traceId, spanId);\n",
            aggregate.name
        ));
        output.push_str("          return finalize(response);\n");
        output.push_str("        }\n");

        // Command handlers with access control
        for cmd in &aggregate.commands {
            output.push_str(&format!(
                "        if (method === 'POST' && action === '{}') {{\n",
                cmd.name
            ));

            // Generate access check based on access level
            match cmd.access {
                AccessLevel::Public => {
                    output.push_str("          // Public endpoint - no auth required\n");
                    output.push_str("          const { traceId, spanId, finalize } = createFinalize('public', authResult.ok ? authResult.user : undefined);\n");
                    output.push_str("          const handlerCtx = { ...ctx, tenant: 'public' };\n");
                }
                AccessLevel::Internal => {
                    output.push_str("          // Internal endpoint - requires system tenant membership\n");
                    output.push_str("          const accessErr = checkInternal();\n");
                    output.push_str("          if (accessErr) return accessErr;\n");
                    if !cmd.roles.is_empty() {
                        let roles_str = cmd.roles.iter().map(|r| format!("'{}'", r)).collect::<Vec<_>>().join(", ");
                        output.push_str(&format!("          const roleErr = checkRoles(authResult.user, SYSTEM_TENANT_ID, [{}]);\n", roles_str));
                        output.push_str("          if (roleErr) return roleErr;\n");
                    }
                    output.push_str("          const { traceId, spanId, finalize } = createFinalize(SYSTEM_TENANT_ID, authResult.user);\n");
                    output.push_str("          const handlerCtx = { ...ctx, tenant: SYSTEM_TENANT_ID };\n");
                }
                AccessLevel::Private => {
                    output.push_str("          // Private endpoint - requires auth + tenant\n");
                    output.push_str("          const privateResult = checkPrivate();\n");
                    output.push_str("          if ('error' in privateResult) return privateResult.error;\n");
                    output.push_str("          const user = privateResult.user;\n");
                    output.push_str("          const tenant = privateResult.tenant;\n");
                    if !cmd.roles.is_empty() {
                        let roles_str = cmd.roles.iter().map(|r| format!("'{}'", r)).collect::<Vec<_>>().join(", ");
                        output.push_str(&format!("          const roleErr = checkRoles(user, tenant, [{}]);\n", roles_str));
                        output.push_str("          if (roleErr) return roleErr;\n");
                    }
                    output.push_str("          const { traceId, spanId, finalize } = createFinalize(tenant, user);\n");
                    output.push_str("          const handlerCtx = { ...ctx, tenant };\n");
                }
            }

            output.push_str("          const body = await req.json();\n");
            output.push_str(&format!(
                "          const response = await handle{}{}(handlerCtx, streamId, body, traceId, spanId);\n",
                aggregate.name,
                to_pascal_case(&cmd.name)
            ));
            output.push_str("          return finalize(response);\n");
            output.push_str("        }\n");
        }

        output.push_str("      }\n\n");
    }

    // 404 fallback
    output.push_str("      return finalize(new Response(JSON.stringify({ error: 'Not Found' }), { status: 404, headers: { 'Content-Type': 'application/json' } }));\n");
    output.push_str("    } catch (err) {\n");
    output.push_str("      console.error('Router error:', err);\n");
    output.push_str("      const message = isProd ? 'Internal Server Error' : (err instanceof Error ? err.message : 'Unknown error');\n");
    output.push_str("      return finalize(new Response(JSON.stringify({ error: message }), { status: 500, headers: { 'Content-Type': 'application/json' } }));\n");
    output.push_str("    }\n");
    output.push_str("  };\n");
    output.push_str("}\n");

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AggregateIR, CommandIR, DomainIR, EventTypeIR, ObjectType};
    use std::path::PathBuf;

    fn make_test_aggregate(name: &str, commands: Vec<CommandIR>) -> AggregateIR {
        AggregateIR {
            name: name.to_string(),
            source_path: PathBuf::new(),
            state: ObjectType { fields: vec![] },
            initial_state: vec![],
            events: EventTypeIR {
                name: format!("{}Event", name),
                variants: vec![],
            },
            commands,
            raw_apply_body: None,
        }
    }

    #[test]
    fn emits_telemetry_without_flush() {
        let mut domain = DomainIR::new(PathBuf::new());
        domain.aggregates.push(make_test_aggregate("Todo", vec![]));

        let code = generate_router(&domain);

        assert!(code.contains("emitTelemetry(ctx.telemetry, records);"));
        assert!(code.contains("const createFinalize = (tenant: string, user?: { sub?: string }) => {"));
        assert!(code.contains("finalize: (response: Response, err?: unknown): Response => {"));
        assert!(!code.contains("flushTelemetry"));
    }
}
