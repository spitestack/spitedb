/**
 * SpiteStack Email Module
 * 
 * Provides an abstraction for sending transactional emails.
 * Includes a Console provider for dev joy and Resend for production.
 */

export interface EmailProvider {
  send(to: string, subject: string, html: string): Promise<void>;
}

export class ConsoleEmailProvider implements EmailProvider {
  async send(to: string, subject: string, html: string): Promise<void> {
    console.log('\n========= 📧 EMAIL SENT =========');
    console.log(`To: ${to}`);
    console.log(`Subject: ${subject}`);
    console.log('---------------------------------');
    console.log(html); // In a real terminal app, we might strip tags or use a CLI markdown renderer
    console.log('=================================\n');
  }
}

export class ResendEmailProvider implements EmailProvider {
  private apiKey: string;
  private from: string;

  constructor(apiKey: string, from: string) {
    this.apiKey = apiKey;
    this.from = from;
  }

  async send(to: string, subject: string, html: string): Promise<void> {
    const res = await fetch('https://api.resend.com/emails', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${this.apiKey}`,
      },
      body: JSON.stringify({
        from: this.from,
        to,
        subject,
        html,
      }),
    });

    if (!res.ok) {
      const err = await res.text();
      throw new Error(`Failed to send email via Resend: ${err}`);
    }
  }
}

export function createEmailProvider(): EmailProvider {
  if (process.env.EMAIL_PROVIDER === 'resend' || (process.env.RESEND_API_KEY && process.env.NODE_ENV === 'production')) {
    const apiKey = process.env.RESEND_API_KEY;
    const from = process.env.EMAIL_FROM || 'SpiteStack <noreply@spitestack.dev>';
    
    if (!apiKey) {
      console.warn('⚠️ RESEND_API_KEY missing. Falling back to ConsoleEmailProvider.');
      return new ConsoleEmailProvider();
    }
    return new ResendEmailProvider(apiKey, from);
  }

  return new ConsoleEmailProvider();
}
