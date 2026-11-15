// Test harness: verifies web receiver decryption against testvectors/drop-v1.json

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import {
    initSodium,
    deriveContentKey,
    decryptDrop,
    parseLink,
    unwrapKey,
    hexToBytes,
    bytesToHex
} from './crypto.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function main() {
    console.log('=== Web Receiver Test Vector Verification ===');
    const sodium = await initSodium();
    console.log('libsodium initialized successfully.');

    const vectorPath = path.resolve(__dirname, '../testvectors/drop-v1.json');
    const vectorJson = JSON.parse(fs.readFileSync(vectorPath, 'utf8'));

    let passed = 0;
    let failed = 0;

    // 1. Verify decrypt cases
    console.log(`\nRunning ${vectorJson.decrypt_cases.length} decrypt test cases...`);
    for (const testCase of vectorJson.decrypt_cases) {
        try {
            const masterKey = hexToBytes(testCase.K_hex);
            const contentKey = deriveContentKey(masterKey);
            const ciphertext = hexToBytes(testCase.ciphertext_hex);

            const { metadata, content } = decryptDrop(contentKey, ciphertext);

            if (metadata.kind !== testCase.expect.kind) {
                throw new Error(`kind mismatch: expected ${testCase.expect.kind}, got ${metadata.kind}`);
            }
            if (testCase.expect.name && metadata.name !== testCase.expect.name) {
                throw new Error(`name mismatch: expected ${testCase.expect.name}, got ${metadata.name}`);
            }
            if (metadata.size !== testCase.expect.size) {
                throw new Error(`size mismatch: expected ${testCase.expect.size}, got ${metadata.size}`);
            }

            const contentHex = bytesToHex(content);
            if (contentHex !== testCase.expect.content_hex) {
                throw new Error(`content mismatch on case ${testCase.name}`);
            }

            console.log(`  [OK] Decrypt: ${testCase.name} (${metadata.size} bytes)`);
            passed++;
        } catch (e) {
            console.error(`  [FAIL] Decrypt: ${testCase.name}: ${e.message}`);
            failed++;
        }
    }

    // 2. Verify wrap cases
    console.log(`\nRunning ${vectorJson.wrap_cases.length} passphrase wrap test cases...`);
    for (const testCase of vectorJson.wrap_cases) {
        try {
            // Test 1: parse base64url payload
            const parsed = parseLink(testCase.payload_b64url);
            if (!parsed.isProtected) {
                throw new Error('Expected isProtected=true');
            }

            // Test 2: unwrap key
            const unwrappedKey = unwrapKey(parsed.wrapParams, testCase.passphrase_utf8);
            const unwrappedHex = bytesToHex(unwrappedKey);

            if (unwrappedHex !== testCase.K_hex) {
                throw new Error(`Unwrapped key mismatch: expected ${testCase.K_hex}, got ${unwrappedHex}`);
            }

            // Test 3: wrong passphrase must fail
            let wrongFailed = false;
            try {
                unwrapKey(parsed.wrapParams, testCase.passphrase_utf8 + '_wrong');
            } catch {
                wrongFailed = true;
            }
            if (!wrongFailed) {
                throw new Error('Wrong passphrase did not throw an error');
            }

            console.log(`  [OK] Wrap: ${testCase.name}`);
            passed++;
        } catch (e) {
            console.error(`  [FAIL] Wrap: ${testCase.name}: ${e.message}`);
            failed++;
        }
    }

    // 3. Verify negative cases from testvectors
    console.log(`\nRunning ${vectorJson.negative_cases.length} negative test cases from testvectors...`);
    for (const negCase of vectorJson.negative_cases) {
        try {
            if (negCase.ciphertext_hex) {
                const masterKey = hexToBytes(negCase.K_hex);
                const contentKey = deriveContentKey(masterKey);
                const ciphertext = hexToBytes(negCase.ciphertext_hex);
                let rejected = false;
                try {
                    decryptDrop(contentKey, ciphertext);
                } catch {
                    rejected = true;
                }
                if (!rejected) {
                    throw new Error(`Negative case ${negCase.name} unexpectedly succeeded`);
                }
            } else if (negCase.payload_b64url) {
                let rejected = false;
                try {
                    parseLink(negCase.payload_b64url);
                } catch {
                    rejected = true;
                }
                if (!rejected) {
                    throw new Error(`Negative case ${negCase.name} unexpectedly succeeded`);
                }
            }
            console.log(`  [OK] Negative: ${negCase.name} rejected correctly`);
            passed++;
        } catch (e) {
            console.error(`  [FAIL] Negative: ${negCase.name}: ${e.message}`);
            failed++;
        }
    }

    console.log(`\nResults: ${passed} passed, ${failed} failed.`);
    if (failed > 0) {
        process.exit(1);
    }
}

main().catch(err => {
    console.error('Fatal error:', err);
    process.exit(1);
});
