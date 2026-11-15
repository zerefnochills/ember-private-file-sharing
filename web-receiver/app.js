// Ember Web Receiver Application (ESM)
// Client-side drop decryption adhering to docs/protocol-spec.md

import {
    initSodium,
    parseLink,
    unwrapKey,
    deriveContentKey,
    decryptDrop
} from './crypto.js';

let sodiumInstance = null;
let parsedLinkData = null;
let remoteBlobMetadata = null;

// DOM Elements
const views = {
    input: document.getElementById('view-input'),
    loading: document.getElementById('view-loading'),
    details: document.getElementById('view-details'),
    progress: document.getElementById('view-progress'),
    result: document.getElementById('view-result'),
    error: document.getElementById('view-error')
};

const el = {
    loadingText: document.getElementById('loading-text'),
    formLink: document.getElementById('form-link'),
    inputLink: document.getElementById('input-link'),
    metaBlobId: document.getElementById('meta-blob-id'),
    metaSize: document.getElementById('meta-size'),
    metaParts: document.getElementById('meta-parts'),
    metaExpires: document.getElementById('meta-expires'),
    burnNotice: document.getElementById('burn-notice'),
    passphraseContainer: document.getElementById('passphrase-container'),
    inputPassphrase: document.getElementById('input-passphrase'),
    btnDownload: document.getElementById('btn-download'),
    progressStatus: document.getElementById('progress-status'),
    progressFill: document.getElementById('progress-fill'),
    progressText: document.getElementById('progress-text'),
    resFilename: document.getElementById('res-filename'),
    resMeta: document.getElementById('res-meta'),
    btnSave: document.getElementById('btn-save'),
    textPreviewContainer: document.getElementById('text-preview-container'),
    textPreview: document.getElementById('text-preview'),
    btnCopyText: document.getElementById('btn-copy-text'),
    resultBurnStatus: document.getElementById('result-burn-status'),
    errorTitle: document.getElementById('error-title'),
    errorMessage: document.getElementById('error-message'),
    btnRetry: document.getElementById('btn-retry')
};

function switchView(viewName) {
    Object.values(views).forEach(v => v.classList.add('hidden'));
    if (views[viewName]) {
        views[viewName].classList.remove('hidden');
    }
}

function showError(title, message) {
    el.errorTitle.textContent = title;
    el.errorMessage.textContent = message;
    switchView('error');
}

function formatBytes(bytes) {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

function formatRemainingTime(expiresTimestamp) {
    const now = Math.floor(Date.now() / 1000);
    const diff = expiresTimestamp - now;
    if (diff <= 0) return 'Expired';
    const hours = Math.floor(diff / 3600);
    const minutes = Math.floor((diff % 3600) / 60);
    if (hours > 0) {
        return `${hours}h ${minutes}m`;
    }
    return `${minutes}m`;
}

async function loadDrop(linkString) {
    switchView('loading');
    el.loadingText.textContent = 'Parsing link payload...';

    try {
        parsedLinkData = parseLink(linkString);
    } catch (err) {
        showError('Invalid Drop Link', err.message);
        return;
    }

    // If blobId was not embedded in the URL path, try window.location pathname
    if (!parsedLinkData.blobId) {
        const pathParts = window.location.pathname.split('/');
        const lastPart = pathParts[pathParts.length - 1];
        if (lastPart && lastPart !== 'd' && lastPart !== '') {
            parsedLinkData.blobId = lastPart;
        }
    }

    if (!parsedLinkData.blobId) {
        showError('Missing Blob ID', 'The drop link does not contain a valid relay blob ID.');
        return;
    }

    el.loadingText.textContent = 'Querying relay metadata...';

    try {
        // Safe GET request: never contains hash fragment, never burns the blob
        const res = await fetch(`/v1/blobs/${parsedLinkData.blobId}`);
        if (!res.ok) {
            if (res.status === 404) {
                throw new Error('Drop not found on relay server (never existed or deleted).');
            } else if (res.status === 410) {
                throw new Error('Drop is gone: already burned or expired.');
            } else {
                throw new Error(`Server returned HTTP ${res.status}: ${res.statusText}`);
            }
        }

        remoteBlobMetadata = await res.json();
    } catch (err) {
        showError('Relay Query Failed', err.message);
        return;
    }

    // Populate details view
    el.metaBlobId.textContent = parsedLinkData.blobId;
    el.metaSize.textContent = formatBytes(remoteBlobMetadata.size);
    el.metaParts.textContent = `${remoteBlobMetadata.parts} part(s)`;
    el.metaExpires.textContent = formatRemainingTime(remoteBlobMetadata.expires_at);

    if (remoteBlobMetadata.burn) {
        el.burnNotice.classList.remove('hidden');
    } else {
        el.burnNotice.classList.add('hidden');
    }

    if (parsedLinkData.isProtected) {
        el.passphraseContainer.classList.remove('hidden');
        el.inputPassphrase.value = '';
    } else {
        el.passphraseContainer.classList.add('hidden');
    }

    switchView('details');
}

async function handleDownloadAndDecrypt() {
    let masterKey;

    if (parsedLinkData.isProtected) {
        const passphrase = el.inputPassphrase.value;
        if (!passphrase) {
            alert('Please enter the passphrase for this drop.');
            el.inputPassphrase.focus();
            return;
        }

        switchView('loading');
        el.loadingText.textContent = 'Deriving key with Argon2id (memory-hard)...';
        await new Promise(r => setTimeout(r, 20)); // allow UI update

        try {
            masterKey = unwrapKey(parsedLinkData.wrapParams, passphrase);
        } catch (err) {
            showError('Passphrase Error', 'Failed to unwrap key. Incorrect passphrase or corrupted payload.');
            return;
        }
    } else {
        masterKey = parsedLinkData.masterKey;
    }

    switchView('progress');
    el.progressStatus.textContent = 'Preparing retrieval...';
    el.progressFill.style.width = '0%';
    el.progressText.textContent = '0%';

    let claimToken = null;
    if (remoteBlobMetadata.burn) {
        el.progressStatus.textContent = 'Claiming burn-after-view drop...';
        try {
            const claimRes = await fetch(`/v1/blobs/${parsedLinkData.blobId}/claim`, {
                method: 'POST'
            });
            if (!claimRes.ok) {
                if (claimRes.status === 409) {
                    throw new Error('Drop is already being claimed by another recipient.');
                } else if (claimRes.status === 410) {
                    throw new Error('Drop has already been burned.');
                } else {
                    throw new Error(`Failed to claim drop (HTTP ${claimRes.status})`);
                }
            }
            const claimData = await claimRes.json();
            claimToken = claimData.claim;
        } catch (err) {
            showError('Claim Failed', err.message);
            return;
        }
    }

    // Download parts sequentially
    const totalParts = remoteBlobMetadata.parts;
    const downloadedParts = [];
    let totalDownloadedBytes = 0;

    for (let partNum = 0; partNum < totalParts; partNum++) {
        el.progressStatus.textContent = `Downloading part ${partNum + 1} of ${totalParts}...`;
        const percent = Math.round((partNum / totalParts) * 80);
        el.progressFill.style.width = `${percent}%`;
        el.progressText.textContent = `${percent}%`;

        try {
            const partRes = await fetch(`/v1/blobs/${parsedLinkData.blobId}/parts/${partNum}`);
            if (!partRes.ok) {
                throw new Error(`Failed to download part ${partNum} (HTTP ${partRes.status})`);
            }
            const partBuf = await partRes.arrayBuffer();
            const partBytes = new Uint8Array(partBuf);
            downloadedParts.push(partBytes);
            totalDownloadedBytes += partBytes.length;
        } catch (err) {
            showError('Download Interrupted', err.message);
            return;
        }
    }

    el.progressStatus.textContent = 'Assembling ciphertext stream...';
    el.progressFill.style.width = '85%';
    el.progressText.textContent = '85%';
    await new Promise(r => setTimeout(r, 10));

    // Concatenate all parts
    const fullCiphertext = new Uint8Array(totalDownloadedBytes);
    let offset = 0;
    for (const part of downloadedParts) {
        fullCiphertext.set(part, offset);
        offset += part.length;
    }

    el.progressStatus.textContent = 'Decrypting ciphertext stream (libsodium)...';
    el.progressFill.style.width = '95%';
    el.progressText.textContent = '95%';
    await new Promise(r => setTimeout(r, 10));

    let decrypted;
    try {
        const contentKey = deriveContentKey(masterKey);
        decrypted = decryptDrop(contentKey, fullCiphertext);
    } catch (err) {
        showError('Decryption Failed', `Tampering or corruption detected: ${err.message}`);
        return;
    }

    // If claimed, finish to trigger server-side burn
    if (claimToken) {
        el.progressStatus.textContent = 'Confirming burn-after-view completion...';
        try {
            await fetch(`/v1/blobs/${parsedLinkData.blobId}/finish`, {
                method: 'POST',
                headers: { 'X-Claim': claimToken }
            });
        } catch {
            // Non-fatal if network drop occurs at finish; sweeper cleans up
        }
    }

    el.progressFill.style.width = '100%';
    el.progressText.textContent = '100%';

    // Populate result view
    const meta = decrypted.metadata;
    const content = decrypted.content;

    const displayFilename = meta.name || (meta.kind === 'text' ? 'message.txt' : 'download.bin');
    el.resFilename.textContent = displayFilename;
    el.resMeta.textContent = `${formatBytes(meta.size)} • ${meta.mime || 'application/octet-stream'}`;

    // Setup download button
    const blob = new Blob([content], { type: meta.mime || 'application/octet-stream' });
    const blobUrl = URL.createObjectURL(blob);
    el.btnSave.href = blobUrl;
    el.btnSave.download = displayFilename;

    if (remoteBlobMetadata.burn) {
        el.resultBurnStatus.textContent = 'Drop burned from relay server. Decrypted in local session.';
    } else {
        el.resultBurnStatus.textContent = 'Decrypted in browser session using XChaCha20-Poly1305.';
    }

    // Text preview if kind == text
    if (meta.kind === 'text') {
        try {
            const decodedText = new TextDecoder('utf-8', { fatal: true }).decode(content);
            el.textPreview.value = decodedText;
            el.textPreviewContainer.classList.remove('hidden');
        } catch {
            el.textPreviewContainer.classList.add('hidden');
        }
    } else {
        el.textPreviewContainer.classList.add('hidden');
    }

    switchView('result');
}

// Event Listeners
el.btnDownload.addEventListener('click', handleDownloadAndDecrypt);

el.btnRetry.addEventListener('click', () => {
    if (window.location.hash) {
        loadDrop(window.location.href);
    } else {
        switchView('input');
    }
});

el.formLink.addEventListener('submit', (e) => {
    e.preventDefault();
    const val = el.inputLink.value.trim();
    if (val) {
        loadDrop(val);
    }
});

el.btnCopyText.addEventListener('click', () => {
    navigator.clipboard.writeText(el.textPreview.value).then(() => {
        el.btnCopyText.textContent = 'Copied!';
        setTimeout(() => {
            el.btnCopyText.textContent = 'Copy Message';
        }, 2000);
    });
});

// App Initialization
async function initApp() {
    try {
        sodiumInstance = await initSodium();
    } catch (err) {
        showError('Engine Initialization Failed', 'Could not initialize libsodium WebAssembly engine: ' + err.message);
        return;
    }

    if (window.location.hash) {
        loadDrop(window.location.href);
    } else {
        switchView('input');
    }
}

initApp();
