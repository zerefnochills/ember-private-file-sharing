// Ember Drop Format v1 Decryption Primitives (JavaScript / libsodium.js)
// Implements client-side decryption for Ember web receiver.

import sodium from 'libsodium-wrappers-sumo';

export async function initSodium() {
    await sodium.ready;
    return sodium;
}

export const KDF_CONTEXT = 'EMBERv1_';
export const SUBKEY_ID_CONTENT = 1;
export const CHUNK_SIZE = 1048576; // 1 MiB
export const PART0_CIPHERTEXT_SIZE = 4137; // 24-byte header + 4096 plaintext + 17 auth tag
export const METADATA_RECORD_SIZE = 4096;

export const AD_DROP = 'ember-drop-v1';

/**
 * Parses an Ember share link URL or hash fragment.
 */
export function parseLink(urlOrFragment) {
    let fragment = '';
    let blobId = '';

    if (urlOrFragment.includes('#')) {
        const parts = urlOrFragment.split('#');
        fragment = parts[1];
        const pathParts = parts[0].split('/');
        blobId = pathParts[pathParts.length - 1];
    } else if (urlOrFragment.startsWith('#')) {
        fragment = urlOrFragment.slice(1);
    } else {
        fragment = urlOrFragment;
    }

    // Decode base64url payload
    const raw = fromBase64Url(fragment);
    if (raw.length < 2) {
        throw new Error('Link payload too short');
    }

    const version = raw[0];
    if (version !== 1) {
        throw new Error(`Unsupported protocol version: ${version} (upgrade required)`);
    }

    const flags = raw[1];
    if ((flags & ~0x01) !== 0) {
        throw new Error(`Unsupported flag bits: 0x${flags.toString(16)}`);
    }

    const isProtected = (flags & 0x01) !== 0;

    if (!isProtected) {
        if (raw.length !== 34) {
            throw new Error(`Invalid unprotected payload length: expected 34 bytes, got ${raw.length}`);
        }
        const masterKey = raw.slice(2, 34);
        return {
            version,
            isProtected: false,
            blobId,
            masterKey
        };
    } else {
        if (raw.length !== 95) {
            throw new Error(`Invalid protected payload length: expected 95 bytes, got ${raw.length}`);
        }
        const ops = raw[2];
        const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
        const memKib = view.getUint32(3, false); // big-endian
        const salt = raw.slice(7, 23); // 16 bytes
        const nonce = raw.slice(23, 47); // 24 bytes
        const ciphertext = raw.slice(47, 95); // 48 bytes (32 key + 16 poly1305)
        const ad = raw.slice(0, 23); // 23 bytes prefix (version, flags, ops, mem_kib, salt)

        if (ops < 1 || ops > 10) {
            throw new Error(`Passphrase ops parameter ${ops} outside allowed range [1, 10]`);
        }
        if (memKib < 8192 || memKib > 262144) {
            throw new Error(`Passphrase mem_kib parameter ${memKib} outside allowed range [8192, 262144]`);
        }

        return {
            version,
            isProtected: true,
            blobId,
            wrapParams: {
                ops,
                memKib,
                salt,
                nonce,
                ciphertext,
                ad
            }
        };
    }
}

/**
 * Unwraps a passphrase-protected master key using Argon2id and XChaCha20-Poly1305.
 */
export function unwrapKey(wrapParams, passphrase) {
    const { ops, memKib, salt, nonce, ciphertext, ad } = wrapParams;
    const memBytes = memKib * 1024;

    // Derive K_wrap with Argon2id
    const kWrap = sodium.crypto_pwhash(
        32,
        passphrase,
        salt,
        ops,
        memBytes,
        sodium.crypto_pwhash_ALG_ARGON2ID13
    );

    // Decrypt master key K with XChaCha20-Poly1305-IETF using authenticated prefix AD
    try {
        const masterKey = sodium.crypto_aead_xchacha20poly1305_ietf_decrypt(
            null,
            ciphertext,
            ad,
            nonce,
            kWrap
        );
        return masterKey;
    } catch {
        throw new Error('Authentication failed: incorrect passphrase or corrupted key wrap');
    }
}

/**
 * Derives the 32-byte drop content subkey (id 1) using BLAKE2b KDF.
 */
export function deriveContentKey(masterKey) {
    return sodium.crypto_kdf_derive_from_key(
        32,
        SUBKEY_ID_CONTENT,
        KDF_CONTEXT,
        masterKey
    );
}

/**
 * Decrypts a full drop ciphertext stream or concatenated parts.
 */
export function decryptDrop(contentKey, ciphertext) {
    if (ciphertext.length < 24 + 17) {
        throw new Error('Ciphertext too short for header and tag');
    }

    const header = ciphertext.slice(0, 24);
    const state = sodium.crypto_secretstream_xchacha20poly1305_init_pull(header, contentKey);

    // Part 0 Message 0 metadata chunk: exactly 4096 plaintext + 17 auth tag = 4113 bytes
    const part0End = 24 + 4096 + 17;
    if (ciphertext.length < part0End) {
        throw new Error('Ciphertext too short for metadata record');
    }

    const part0Chunk = ciphertext.slice(24, part0End);
    const pull0 = sodium.crypto_secretstream_xchacha20poly1305_pull(state, part0Chunk, AD_DROP);
    if (!pull0) {
        throw new Error('Authentication failed on metadata chunk');
    }

    // Message 0 MUST NOT be final unless content size is 0 and empty file
    const metaRecord = pull0.message;
    if (metaRecord.length !== 4096) {
        throw new Error(`Unexpected metadata record size: ${metaRecord.length}`);
    }

    // Layout: u32 big-endian json_len || json (UTF-8) || zero padding up to 4096
    const view = new DataView(metaRecord.buffer, metaRecord.byteOffset, metaRecord.byteLength);
    const jsonLen = view.getUint32(0, false);
    if (jsonLen > 4096 - 4) {
        throw new Error(`Declared JSON length ${jsonLen} exceeds limit 4092`);
    }

    const jsonBytes = metaRecord.slice(4, 4 + jsonLen);
    let metadata;
    try {
        const jsonText = new TextDecoder('utf-8', { fatal: true }).decode(jsonBytes);
        metadata = JSON.parse(jsonText);
    } catch (e) {
        throw new Error(`Invalid metadata record JSON: ${e.message}`);
    }

    // Verify metadata padding (bytes 4 + jsonLen .. 4096 must be 0)
    for (let i = 4 + jsonLen; i < 4096; i++) {
        if (metaRecord[i] !== 0) {
            throw new Error(`Invalid padding in metadata record at offset ${i}`);
        }
    }

    const declaredSize = metadata.size;
    let offset = part0End;
    const contentChunks = [];
    let receivedContentBytes = 0;
    let sawFinal = (pull0.tag === sodium.crypto_secretstream_xchacha20poly1305_TAG_FINAL);

    while (offset < ciphertext.length) {
        if (sawFinal) {
            throw new Error('Unexpected trailing bytes after FINAL chunk');
        }

        // Remaining ciphertext chunk: chunk size is up to 1 MiB + 17 bytes, or whatever remains
        const remaining = ciphertext.length - offset;
        const chunkLen = Math.min(remaining, CHUNK_SIZE + 17);
        const chunk = ciphertext.slice(offset, offset + chunkLen);
        offset += chunkLen;

        const pull = sodium.crypto_secretstream_xchacha20poly1305_pull(state, chunk, AD_DROP);
        if (!pull) {
            throw new Error('Authentication failed on content chunk');
        }

        contentChunks.push(pull.message);
        receivedContentBytes += pull.message.length;

        if (pull.tag === sodium.crypto_secretstream_xchacha20poly1305_TAG_FINAL) {
            sawFinal = true;
        }
    }

    if (!sawFinal) {
        throw new Error('Cipherstream ended prematurely without FINAL tag');
    }

    // Flatten content chunks
    const totalPlaintext = new Uint8Array(receivedContentBytes);
    let pOffset = 0;
    for (const c of contentChunks) {
        totalPlaintext.set(c, pOffset);
        pOffset += c.length;
    }

    if (declaredSize > totalPlaintext.length) {
        throw new Error(`Size mismatch: stated content size (${declaredSize}) exceeds decrypted length (${totalPlaintext.length})`);
    }

    // Verify trailing padding bytes are all 0
    for (let i = declaredSize; i < totalPlaintext.length; i++) {
        if (totalPlaintext[i] !== 0) {
            throw new Error('Invalid padding: non-zero byte in content padding region');
        }
    }

    const content = totalPlaintext.slice(0, declaredSize);

    return {
        metadata,
        content
    };
}

/**
 * Base64url decode helper.
 */
export function fromBase64Url(str) {
    let base64 = str.replace(/-/g, '+').replace(/_/g, '/');
    while (base64.length % 4 !== 0) {
        base64 += '=';
    }
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
}

/**
 * Hex to Uint8Array helper.
 */
export function hexToBytes(hex) {
    const bytes = new Uint8Array(hex.length / 2);
    for (let i = 0; i < hex.length; i += 2) {
        bytes[i / 2] = parseInt(hex.substring(i, i + 2), 16);
    }
    return bytes;
}

/**
 * Uint8Array to hex helper.
 */
export function bytesToHex(bytes) {
    return Array.from(bytes)
        .map(b => b.toString(16).padStart(2, '0'))
        .join('');
}
