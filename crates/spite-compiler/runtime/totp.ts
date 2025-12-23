
/**
 * SpiteStack Zero-Dependency TOTP
 * 
 * Implements RFC 6238 (TOTP) and RFC 4226 (HOTP) using native Web Crypto API.
 * Compatible with Google Authenticator, Authy, etc.
 */

// Base32 helpers for secret encoding/decoding (RFC 4648)
const BASE32_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

export function base32Decode(str: string): Uint8Array {
  let v = 0;
  let bits = 0;
  const chars = str.toUpperCase().replace(/=+$/, '');
  const out: number[] = [];

  for (let i = 0; i < chars.length; i++) {
    const val = BASE32_ALPHABET.indexOf(chars[i]);
    if (val === -1) throw new Error('Invalid base32 character');
    v = (v << 5) | val;
    bits += 5;
    if (bits >= 8) {
      out.push((v >>> (bits - 8)) & 255);
      bits -= 8;
    }
  }
  return new Uint8Array(out);
}

export function base32Encode(buffer: Uint8Array): string {
  let v = 0;
  let bits = 0;
  let out = '';
  for (let i = 0; i < buffer.length; i++) {
    v = (v << 8) | buffer[i];
    bits += 8;
    while (bits >= 5) {
      out += BASE32_ALPHABET[(v >>> (bits - 5)) & 31];
      bits -= 5;
    }
  }
  if (bits > 0) {
    out += BASE32_ALPHABET[(v << (5 - bits)) & 31];
  }
  return out;
}

export function generateSecret(length = 20): string {
  const bytes = crypto.getRandomValues(new Uint8Array(length));
  return base32Encode(bytes);
}

export async function generateTOTP(secret: string, window = 0): Promise<string> {
  const counter = Math.floor(Date.now() / 30000) + window;
  return generateHOTP(secret, counter);
}

export async function verifyTOTP(token: string, secret: string, window = 1): Promise<boolean> {
  // Check current, previous, and next windows to account for clock drift
  for (let i = -window; i <= window; i++) {
    const generated = await generateTOTP(secret, i);
    if (generated === token) return true;
  }
  return false;
}

async function generateHOTP(secret: string, counter: number): Promise<string> {
  const decodedSecret = base32Decode(secret);
  
  // Convert counter to 8-byte buffer
  const counterBuf = new Uint8Array(8);
  for (let i = 7; i >= 0; i--) {
    counterBuf[i] = counter & 0xff;
    counter = counter >>> 8;
  }

  // HMAC-SHA1
  const key = await crypto.subtle.importKey(
    'raw', 
    decodedSecret, 
    { name: 'HMAC', hash: 'SHA-1' }, 
    false, 
    ['sign']
  );
  
  const signature = await crypto.subtle.sign('HMAC', key, counterBuf);
  const hmac = new Uint8Array(signature);

  // Dynamic truncation
  const offset = hmac[hmac.length - 1] & 0xf;
  const binary =
    ((hmac[offset] & 0x7f) << 24) |
    ((hmac[offset + 1] & 0xff) << 16) |
    ((hmac[offset + 2] & 0xff) << 8) |
    (hmac[offset + 3] & 0xff);

  const otp = binary % 1000000;
  return otp.toString().padStart(6, '0');
}

export function generateTotpUri(secret: string, accountName: string, issuer: string): string {
  return `otpauth://totp/${encodeURIComponent(issuer)}:${encodeURIComponent(accountName)}?secret=${secret}&issuer=${encodeURIComponent(issuer)}&algorithm=SHA1&digits=6&period=30`;
}

// Aliases for compatibility
export const verify = verifyTOTP;
export const generateUri = generateTotpUri;
