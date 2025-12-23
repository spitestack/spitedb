/**
 * SpiteStack Password Policy Module
 *
 * Strong password validation for SOC 2 and HIPAA compliance.
 * Enforces: minimum length, complexity, and common password checks.
 */

export interface PasswordPolicyConfig {
  /** Minimum password length (default: 12) */
  minLength: number;
  /** Require at least one uppercase letter */
  requireUppercase: boolean;
  /** Require at least one lowercase letter */
  requireLowercase: boolean;
  /** Require at least one digit */
  requireDigit: boolean;
  /** Require at least one special character */
  requireSpecial: boolean;
  /** Check against common passwords list */
  checkCommonPasswords: boolean;
  /** Maximum password length (default: 128) */
  maxLength: number;
}

export interface PasswordValidationResult {
  valid: boolean;
  errors: string[];
  strength: 'weak' | 'fair' | 'good' | 'strong';
}

// Default strong policy for SOC 2/HIPAA compliance
export const DEFAULT_PASSWORD_POLICY: PasswordPolicyConfig = {
  minLength: 12,
  requireUppercase: true,
  requireLowercase: true,
  requireDigit: true,
  requireSpecial: true,
  checkCommonPasswords: true,
  maxLength: 128,
};

// Top 1000 most common passwords (abbreviated - real implementation should use full list)
// This is a representative subset; in production, use a comprehensive list
const COMMON_PASSWORDS = new Set([
  'password', 'password1', 'password123', '123456', '12345678', '123456789',
  '1234567890', 'qwerty', 'abc123', 'monkey', 'master', 'dragon', 'letmein',
  'login', 'admin', 'welcome', 'solo', 'princess', 'starwars', 'passw0rd',
  'shadow', 'sunshine', 'iloveyou', 'trustno1', 'superman', 'batman', 'ninja',
  'football', 'baseball', 'soccer', 'hockey', 'jordan', 'michael', 'jennifer',
  'hunter', 'jessica', 'charlie', 'andrew', 'michelle', 'joshua', 'ashley',
  'thomas', 'daniel', 'matthew', 'whatever', 'fuckoff', 'fuckyou', 'pussy',
  'asshole', 'cheese', 'chicken', 'summer', 'winter', 'spring', 'autumn',
  'cookie', 'flower', 'secret', 'diamond', 'forever', 'angels', 'phoenix',
  'buster', 'pepper', 'sparky', 'ginger', 'prince', 'junior', 'killer',
  'creative', 'internet', 'extreme', 'digital', 'computer', 'access', 'thunder',
  'mustang', 'corvette', 'porsche', 'ferrari', 'camaro', 'mercedes', 'toyota',
  'honda', 'yamaha', 'harley', 'maverick', 'guitar', 'piano', 'music', 'purple',
  'orange', 'yellow', 'silver', 'golden', 'bronze', 'platinum', 'titanium',
  'freedom', 'america', 'patriots', 'eagles', 'steelers', 'cowboys', 'raiders',
  'packers', 'dolphins', 'giants', 'yankees', 'redsox', 'dodgers', 'braves',
  'abcdef', 'abcdefg', 'abcdefgh', 'aaaaaa', 'qwerty123', 'zxcvbn', 'asdfgh',
  '654321', '111111', '222222', '333333', '444444', '555555', '666666', '777777',
  '888888', '999999', '000000', 'qweasd', 'qweasdzxc', 'asdfghjkl', 'qwertyui',
  'zaq12wsx', 'password!', 'password1!', 'welcome1', 'welcome123', 'admin123',
  'root', 'toor', 'changeme', 'changeit', 'test', 'test123', 'testing', 'guest',
  'default', 'system', 'server', 'network', 'database', 'security', 'manager',
  'operator', 'monitor', 'backup', 'oracle', 'mysql', 'postgres', 'linux', 'unix',
]);

/**
 * Validates a password against the configured policy.
 */
export function validatePassword(
  password: string,
  policy: PasswordPolicyConfig = DEFAULT_PASSWORD_POLICY
): PasswordValidationResult {
  const errors: string[] = [];
  let strengthScore = 0;

  // Length checks
  if (password.length < policy.minLength) {
    errors.push(`Password must be at least ${policy.minLength} characters long`);
  } else {
    strengthScore += 1;
    if (password.length >= 16) strengthScore += 1;
    if (password.length >= 20) strengthScore += 1;
  }

  if (password.length > policy.maxLength) {
    errors.push(`Password must be no more than ${policy.maxLength} characters`);
  }

  // Complexity checks
  const hasUppercase = /[A-Z]/.test(password);
  const hasLowercase = /[a-z]/.test(password);
  const hasDigit = /[0-9]/.test(password);
  const hasSpecial = /[!@#$%^&*()_+\-=\[\]{};':"\\|,.<>\/?`~]/.test(password);

  if (policy.requireUppercase && !hasUppercase) {
    errors.push('Password must contain at least one uppercase letter');
  }
  if (policy.requireLowercase && !hasLowercase) {
    errors.push('Password must contain at least one lowercase letter');
  }
  if (policy.requireDigit && !hasDigit) {
    errors.push('Password must contain at least one digit');
  }
  if (policy.requireSpecial && !hasSpecial) {
    errors.push('Password must contain at least one special character');
  }

  // Calculate strength based on character diversity
  const typesUsed = [hasUppercase, hasLowercase, hasDigit, hasSpecial].filter(Boolean).length;
  strengthScore += typesUsed;

  // Common password check
  if (policy.checkCommonPasswords) {
    const lowercasePassword = password.toLowerCase();

    // Check exact match
    if (COMMON_PASSWORDS.has(lowercasePassword)) {
      errors.push('Password is too common and easily guessable');
    }

    // Check if password contains common words with simple substitutions
    const normalized = lowercasePassword
      .replace(/0/g, 'o')
      .replace(/1/g, 'l')
      .replace(/3/g, 'e')
      .replace(/4/g, 'a')
      .replace(/5/g, 's')
      .replace(/7/g, 't')
      .replace(/@/g, 'a')
      .replace(/\$/g, 's')
      .replace(/!/g, 'i');

    if (COMMON_PASSWORDS.has(normalized)) {
      errors.push('Password is too similar to a common password');
    }
  }

  // Check for sequential characters
  if (hasSequentialChars(password, 4)) {
    errors.push('Password contains sequential characters (e.g., "1234", "abcd")');
    strengthScore -= 1;
  }

  // Check for repeated characters
  if (hasRepeatedChars(password, 4)) {
    errors.push('Password contains too many repeated characters');
    strengthScore -= 1;
  }

  // Determine strength level
  let strength: 'weak' | 'fair' | 'good' | 'strong';
  if (strengthScore <= 2) strength = 'weak';
  else if (strengthScore <= 4) strength = 'fair';
  else if (strengthScore <= 6) strength = 'good';
  else strength = 'strong';

  return {
    valid: errors.length === 0,
    errors,
    strength,
  };
}

/**
 * Check if password contains sequential characters.
 */
function hasSequentialChars(password: string, minLength: number): boolean {
  const sequences = [
    'abcdefghijklmnopqrstuvwxyz',
    'ABCDEFGHIJKLMNOPQRSTUVWXYZ',
    '0123456789',
    'qwertyuiop',
    'asdfghjkl',
    'zxcvbnm',
    'QWERTYUIOP',
    'ASDFGHJKL',
    'ZXCVBNM',
  ];

  for (const seq of sequences) {
    for (let i = 0; i <= seq.length - minLength; i++) {
      const forward = seq.slice(i, i + minLength);
      const backward = forward.split('').reverse().join('');

      if (password.includes(forward) || password.includes(backward)) {
        return true;
      }
    }
  }

  return false;
}

/**
 * Check if password contains too many repeated characters.
 */
function hasRepeatedChars(password: string, maxRepeats: number): boolean {
  const regex = new RegExp(`(.)\\1{${maxRepeats - 1},}`, 'g');
  return regex.test(password);
}

/**
 * Generate a secure random code for verification purposes.
 * Uses crypto.getRandomValues() for cryptographic security.
 */
export function generateSecureCode(length: number = 8): string {
  const charset = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789'; // Exclude confusable chars: 0,O,I,1
  const array = new Uint8Array(length);
  crypto.getRandomValues(array);

  let code = '';
  for (let i = 0; i < length; i++) {
    code += charset[array[i] % charset.length];
  }

  return code;
}

/**
 * Generate a secure numeric code for SMS/MFA.
 * Uses crypto.getRandomValues() for cryptographic security.
 */
export function generateSecureNumericCode(length: number = 6): string {
  const array = new Uint8Array(length);
  crypto.getRandomValues(array);

  let code = '';
  for (let i = 0; i < length; i++) {
    code += (array[i] % 10).toString();
  }

  return code;
}

/**
 * Create a standardized password policy error response.
 */
export function passwordPolicyErrorResponse(result: PasswordValidationResult): Response {
  return new Response(
    JSON.stringify({
      error: 'Password does not meet requirements',
      requirements: result.errors,
      strength: result.strength,
    }),
    {
      status: 400,
      headers: { 'Content-Type': 'application/json' },
    }
  );
}

/**
 * Get password requirements as a human-readable list.
 */
export function getPasswordRequirements(
  policy: PasswordPolicyConfig = DEFAULT_PASSWORD_POLICY
): string[] {
  const requirements: string[] = [];

  requirements.push(`At least ${policy.minLength} characters`);
  if (policy.requireUppercase) requirements.push('At least one uppercase letter (A-Z)');
  if (policy.requireLowercase) requirements.push('At least one lowercase letter (a-z)');
  if (policy.requireDigit) requirements.push('At least one number (0-9)');
  if (policy.requireSpecial) requirements.push('At least one special character (!@#$%^&*...)');
  if (policy.checkCommonPasswords) requirements.push('Cannot be a commonly used password');

  return requirements;
}
