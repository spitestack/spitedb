/**
 * SpiteStack SMS Module
 * 
 * Interface and providers for SMS notifications.
 */

export interface SmsProvider {
  send(to: string, message: string): Promise<void>;
}

export class ConsoleSmsProvider implements SmsProvider {
  async send(to: string, message: string): Promise<void> {
    console.log('\n========= 📱 SMS SENT =========');
    console.log(`To: ${to}`);
    console.log(`Message: ${message}`);
    console.log('===============================\n');
  }
}

// Example Twilio implementation (commented out to avoid assuming deps, but structure is here)
/*
export class TwilioSmsProvider implements SmsProvider {
  constructor(private sid: string, private token: string, private from: string) {}
  async send(to: string, message: string) {
    // fetch('https://api.twilio.com/...')
  }
}
*/

export function createSmsProvider(): SmsProvider {
  // Can extend to check process.env.SMS_PROVIDER
  return new ConsoleSmsProvider();
}
