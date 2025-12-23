/**
 * SpiteStack Client SDK Runtime
 *
 * Provides a base client for interacting with the SpiteStack API.
 * Uses HttpOnly cookies for secure token storage - tokens are never accessible to JavaScript.
 * Includes auth management and Svelte integration.
 */

// Svelte store contract (minimal to avoid peer dependency)
export interface Writable<T> {
  set: (value: T) => void;
  update: (updater: (value: T) => T) => void;
  subscribe: (run: (value: T) => void) => () => void;
}

// Minimal Svelte store implementation
function writable<T>(value: T): Writable<T> {
  const subscribers = new Set<(value: T) => void>();
  function set(newValue: T) {
    value = newValue;
    for (const sub of subscribers) sub(value);
  }
  function update(fn: (value: T) => T) { set(fn(value)); }
  function subscribe(run: (value: T) => void) {
    subscribers.add(run);
    run(value);
    return () => { subscribers.delete(run); };
  }
  return { set, update, subscribe };
}

export type AuthState = {
  user: any | null;
  isAuthenticated: boolean;
};

export interface ClientConfig {
  baseUrl: string;
  defaultTenant?: string;
}

export class SpiteClient {
  private baseUrl: string;
  public defaultTenant?: string;
  public auth: Writable<AuthState>;
  private refreshPromise: Promise<void> | null = null;

  constructor(config: ClientConfig) {
    this.baseUrl = config.baseUrl.replace(/\/$/, '');

    // Initialize auth state as unauthenticated
    // Will be updated after checking session with server
    this.auth = writable({
      user: null,
      isAuthenticated: false,
    });
  }

  /**
   * Check the current session status with the server.
   * Call this on app initialization to restore session state.
   */
  async checkSession(): Promise<boolean> {
    try {
      const res = await fetch(`${this.baseUrl}/auth/session`, {
        method: 'GET',
        credentials: 'include', // Include cookies
        headers: { 'Content-Type': 'application/json' },
      });

      if (res.ok) {
        const { user } = await res.json();
        this.auth.set({
          user,
          isAuthenticated: true,
        });
        return true;
      }
    } catch (e) {
      // Session check failed, user is not authenticated
    }

    this.auth.set({
      user: null,
      isAuthenticated: false,
    });
    return false;
  }

  /**
   * Set the authenticated state after successful login.
   * The server sets HttpOnly cookies - we just update local state.
   * @param user - User payload from server response
   */
  setAuthenticated(user: any) {
    this.auth.set({
      user,
      isAuthenticated: true,
    });
  }

  /**
   * Clear the authenticated state.
   * Call /auth/logout to clear server-side cookies.
   */
  async logout() {
    try {
      await fetch(`${this.baseUrl}/auth/logout`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
      });
    } catch (e) {
      // Logout request failed, but clear local state anyway
    }

    this.auth.set({
      user: null,
      isAuthenticated: false,
    });
  }

  private async performRefresh(): Promise<void> {
    const res = await fetch(`${this.baseUrl}/auth/refresh`, {
      method: 'POST',
      credentials: 'include', // Include cookies
      headers: { 'Content-Type': 'application/json' },
    });

    if (!res.ok) {
      await this.logout();
      throw new Error('Session expired');
    }

    const { user } = await res.json();
    this.auth.set({
      user,
      isAuthenticated: true,
    });
  }

  private async ensureFreshSession(): Promise<boolean> {
    let current: AuthState | null = null;
    this.auth.subscribe(s => current = s)();

    if (!current?.isAuthenticated) {
      // Try to refresh the session
      if (!this.refreshPromise) {
        this.refreshPromise = this.performRefresh()
          .catch(() => {
            // Refresh failed, user is not authenticated
          })
          .finally(() => { this.refreshPromise = null; });
      }
      await this.refreshPromise;
      this.auth.subscribe(s => current = s)();
    }

    return current?.isAuthenticated || false;
  }

  protected async fetch(path: string, options: RequestInit = {}, tenant?: string) {
    const headers = new Headers(options.headers);
    if (!headers.has('Content-Type')) headers.set('Content-Type', 'application/json');

    const targetTenant = tenant || this.defaultTenant;
    if (targetTenant) headers.set('X-Tenant-ID', targetTenant);

    let response = await fetch(`${this.baseUrl}${path}`, {
      ...options,
      headers,
      credentials: 'include', // Always include cookies
    });

    // Handle expired token during request
    if (response.status === 401 && !path.startsWith('/auth/')) {
      try {
        const refreshed = await this.ensureFreshSession();
        if (refreshed) {
          response = await fetch(`${this.baseUrl}${path}`, {
            ...options,
            headers,
            credentials: 'include',
          });
        }
      } catch (e) {
        await this.logout();
        throw new Error('Unauthorized');
      }
    }

    return response;
  }

  async query<T>(aggregate: string, streamId: string, tenant?: string): Promise<{ streamId: string; state: T }> {
    const res = await this.fetch(`/${aggregate}/${streamId}`, {}, tenant);
    if (!res.ok) throw new Error(`Query failed: ${res.statusText}`);
    return res.json();
  }

  async command<T>(
    aggregate: string,
    streamId: string,
    command: string,
    payload: unknown,
    tenant?: string
  ): Promise<{ streamId: string; events: any[]; state: T }> {
    const res = await this.fetch(`/${aggregate}/${streamId}/${command}`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }, tenant);

    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error(err.error || res.statusText);
    }
    return res.json();
  }
}
