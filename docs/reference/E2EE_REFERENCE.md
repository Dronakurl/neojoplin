# Joplin E2EE Implementation Reference

This document provides a comprehensive reference for Joplin's End-to-End Encryption (E2EE) implementation, based on analysis of the Joplin TypeScript codebase.

## Overview

Joplin's E2EE system uses AES-256-GCM encryption with PBKDF2 key derivation, providing secure encryption for notes and resources while maintaining sync compatibility across devices.

## Core Components

### EncryptionService (`EncryptionService.ts`)

The central service that handles all encryption/decryption operations.

**Key Features:**
- Master key management (load, unload, check passwords)
- Multiple encryption methods (SJCL, SJCL1a/b, SJCL2-4, KeyV1, FileV1, StringV1)
- String and file encryption/decryption
- JED format encoding/decoding
- Chunked encryption for large data

### Encryption Methods

```typescript
enum EncryptionMethod {
    SJCL = 1,      // Deprecated - OCB2 mode
    SJCL2 = 2,     // Deprecated - Master key encryption
    SJCL3 = 3,     // CCM mode
    SJCL4 = 4,     // CCM mode with 10k iterations
    SJCL1a = 5,    // CCM mode, AES-128, 101 iterations
    SJCL1b = 7,    // CCM mode, AES-256, 101 iterations (recommended)
    Custom = 6,    // Custom encryption handler
    KeyV1 = 8,     // AES-256-GCM, 220k PBKDF2 iterations
    FileV1 = 9,    // AES-256-GCM, 3 PBKDF2 iterations, 128KB chunks
    StringV1 = 10, // AES-256-GCM, 3 PBKDF2 iterations, 64KB chunks
}
```

### Chunk Sizes

- **StringV1**: 64KB (65,536 bytes)
- **FileV1**: 128KB (131,072 bytes)
- **KeyV1**: 5KB (5,000 bytes)
- **SJCL methods**: 5KB (for mobile performance)

### Iteration Counts

- **KeyV1** (master keys): 220,000 iterations (OWASP recommended)
- **StringV1/FileV1** (data): 3 iterations (since master key is already secure)
- **SJCL methods**: 101-10,000 iterations depending on method

## JED Format (Joplin Encrypted Data)

### Structure

```
JED01 + [metadata_size(6 hex)] + [metadata]
[metadata] = [encryption_method(2 hex)] + [master_key_id(64 hex)]
```

### Example

```
JED010A[32-char-hex-key-id][6-char-length][encrypted-chunk-1]...
```

### Fields

- **JED01**: Version identifier (5 bytes)
- **Metadata size**: 6 hex digits indicating metadata length
- **Encryption method**: 2 hex digits (e.g., "0A" for StringV1)
- **Master key ID**: 64 hex characters (32 bytes)
- **Encrypted chunks**: Variable number of chunks

### Chunk Format

Each encrypted chunk:
```
[length(6 hex)][encrypted_content]
```

Where `encrypted_content` is JSON:
```json
{
  "iv": "hex-encoded-iv",
  "ct": "hex-encoded-ciphertext"
}
```

## Master Key Management

### Master Key Entity

```typescript
interface MasterKeyEntity {
    id: string;                    // UUID
    content: string;               // Encrypted key data
    encryption_method: number;     // EncryptionMethod enum
    checksum: string;              // SHA256 checksum (SJCL2 only)
    created_time: number;          // Milliseconds since epoch
    updated_time: number;          // Milliseconds since epoch
    source_application: string;    // App ID
    hasBeenUsed: boolean;          // Track usage
}
```

### Master Key Generation

1. Generate 256 random bytes (2048 bits)
2. Convert to hex string
3. Encrypt with password using default encryption method
4. Create master key entity with timestamps
5. Save to database

### Master Key Loading

```typescript
async loadMasterKey(
    model: MasterKeyEntity,
    getPassword: string | (() => string),
    makeActive = false
)
```

**Process:**
1. Get password (string or callback)
2. Decrypt master key content with password
3. Cache decrypted key with timestamp
4. Optionally set as active master key
5. Handle invalid passwords gracefully

### Active Master Key

- Stored in sync info (`activeMasterKeyId`)
- Retrieved via `getActiveMasterKeyId()`
- Used for all encryption/decryption operations
- Error thrown if no active key

## Encryption Process

### String Encryption

```typescript
async encryptString(plainText, options): Promise<string>
```

**Steps:**
1. Get active master key (or use specified key ID)
2. Load decrypted master key
3. Generate JED header with method and key ID
4. Chunk data according to method
5. For each chunk:
   - Encrypt with master key
   - Generate random IV/salt
   - Format as JSON with IV and ciphertext
   - Prepend chunk length (6 hex digits)
6. Concatenate all chunks

### File Encryption

```typescript
async encryptFile(srcPath, destPath, options)
```

**Steps:**
1. Read file as base64
2. Use FileV1 method (128KB chunks)
3. Write to destination as ASCII
4. Handle large files efficiently with streaming

## Decryption Process

### String Decryption

```typescript
async decryptString(cipherText, options): Promise<string>
```

**Steps:**
1. Parse JED header
2. Extract encryption method and master key ID
3. Load corresponding master key
4. For each chunk:
   - Read chunk length (6 hex digits)
   - Read encrypted content
   - Parse JSON to get IV and ciphertext
   - Decrypt with master key
   - Append to result
5. Return concatenated plaintext

### File Decryption

```typescript
async decryptFile(srcPath, destPath, options)
```

**Steps:**
1. Read file as ASCII
2. Parse JED format
3. Decrypt to base64
4. Detect file type
5. Write to temp file with extension

## CLI Commands

### e2ee enable

```bash
joplin e2ee enable
```

**Process:**
1. Prompt for master password
2. Confirm password
3. Generate 256-byte master key
4. Encrypt with password (KeyV1, 220k iterations)
5. Save to database
6. Set as active master key
7. Enable encryption in settings

### e2ee disable

```bash
joplin e2ee disable
```

**Process:**
1. Unload all master keys
2. Clear active master key
3. Disable encryption in settings
4. Optionally decrypt all data

### e2ee decrypt [path]

```bash
# Decrypt a string
joplin e2ee decrypt "JED01..."

# Start decryption worker (decrypt all items)
joplin e2ee decrypt
```

**Decryption Worker:**
- Processes all encrypted items in database
- Handles missing master keys with password prompt
- Supports retry for failed items
- Shows progress and statistics

### e2ee status

```bash
joplin e2ee status
```

**Output:**
```
Encryption is: Enabled
```

### e2ee decrypt-file <path>

```bash
joplin e2ee decrypt-file <encrypted-file> [--output <dir>]
```

**Process:**
1. Read encrypted file
2. Detect file type
3. Decrypt to temp directory
4. Output decrypted file path

### e2ee target-status <path>

```bash
joplin e2ee target-status <sync-target-path> [--verbose]
```

**Process:**
1. Scan sync target directory
2. Check each item/resource for encryption
3. Count encrypted vs decrypted
4. Optionally list all paths

**Output:**
```
Encrypted items: 45/100
Encrypted resources: 12/50
Other items (never encrypted): 20
```

## Password Management

### Password Cache

Passwords are cached in settings after successful decryption:

```typescript
Setting.setObjectValue('encryption.passwordCache', masterKeyId, password);
```

Cached passwords are automatically loaded on startup.

### Password Validation

```typescript
async checkMasterKeyPassword(model: MasterKeyEntity, password: string): Promise<boolean>
```

**Process:**
1. Attempt to decrypt master key with password
2. Return true if successful, false otherwise
3. Used for password confirmation and validation

## Error Handling

### Common Error Codes

- **`noActiveMasterKey`**: No active master key set
- **`masterKeyNotLoaded`**: Master key not loaded in memory
- **`invalidIdentifier`**: Invalid JED format identifier

### Error Recovery

```typescript
try {
    await EncryptionService.instance().decryptString(data);
} catch (error) {
    if (error.code === 'masterKeyNotLoaded') {
        // Prompt for password
        await askForMasterKey(error);
        // Retry operation
    }
}
```

## Performance Considerations

### Mobile Optimization

- Small chunk sizes (5KB) for SJCL methods
- Frame waiting to prevent UI freezing
- Async operations for large files

### Desktop Optimization

- Larger chunks for modern methods (64KB/128KB)
- Native crypto libraries (node:crypto)
- Efficient streaming for file operations

### Progress Reporting

```typescript
{
    onProgress: ({ doneSize }) => {
        console.log(`Processed: ${doneSize} bytes`);
    }
}
```

## Security Features

### Key Derivation

- **PBKDF2-HMAC-SHA512** for all modern methods
- **220,000 iterations** for master keys (OWASP compliant)
- **3 iterations** for data encryption (master key already secure)

### Nonce Management

- Random 256-bit nonce for encryption
- Nonce increment between chunks
- Prevents nonce reuse attacks

### Data Integrity

- **AES-GCM** authentication tag (16 bytes)
- **Checksums** for older methods (SHA256)
- **Associated data** support (currently empty)

## Compatibility Notes

### Cross-Platform

- Same encryption parameters across all platforms
- Native crypto libraries for performance
- UTF-16LE encoding for text (StringV1)
- Base64 encoding for files (FileV1)

### Backward Compatibility

- All historical encryption methods supported
- Automatic master key upgrading
- Graceful fallback for deprecated methods

### Migration Path

```
SJCL (OCB2) → SJCL1a (CCM, AES-128) → SJCL1b (CCM, AES-256) → StringV1/FileV1 (AES-256-GCM)
```

## Implementation Recommendations

### For NeoJoplin

1. **CLI Commands** - Implement core e2ee commands:
   - `neojoplin e2ee enable` - Setup encryption
   - `neojoplin e2ee disable` - Disable encryption
   - `neojoplin e2ee status` - Show encryption status
   - `neojoplin e2ee decrypt` - Decrypt strings/data

2. **Master Key Management**:
   - Store master keys in database
   - Password prompts using `dialoguer`
   - Active key tracking in config

3. **Integration with Note Commands**:
   - `--encrypt` flag for mk-note
   - Auto-decryption for cat/edit
   - Encryption status indicators in ls

4. **Sync Integration**:
   - Encrypt notes before sync upload
   - Decrypt after sync download
   - Handle encrypted items from Joplin clients

5. **Security Best Practices**:
   - Never log passwords or decrypted content
   - Use secure password prompts
   - Clear sensitive data from memory
   - Validate all encryption parameters

## Testing

### Unit Tests

- Encryption method parameters
- Master key generation/loading
- JED format encoding/decoding
- Password validation
- Error handling

### Integration Tests

- Cross-client encryption compatibility
- Sync with encrypted data
- Large file encryption
- Multiple master keys

### Compatibility Tests

- Decrypt Joplin-encrypted data
- Encrypt data readable by Joplin
- Master key migration
- All encryption methods

## References

- **Source**: `/home/konrad/gallery/kjoplin/joplin/packages/lib/services/e2ee/EncryptionService.ts`
- **CLI**: `/home/konrad/gallery/kjoplin/joplin/packages/app-cli/app/command-e2ee.ts`
- **Tests**: `/home/konrad/gallery/kjoplin/joplin/packages/lib/services/e2ee/EncryptionService.test.ts`
- **OWASP**: https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html#pbkdf2
