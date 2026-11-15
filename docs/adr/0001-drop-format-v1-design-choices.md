# ADR 0001: Drop format v1 design choices

- Status: proposed (to be confirmed or revised in M2 and M3, reviewed by the auditor in M13)
- Date: 2026-11-15
- Deciders: Claude (M1), owner to confirm

## Context
M1 needed concrete answers on chunking, resumability, burn-after-view and passphrase handling so that the core, server and web receiver can be built independently.

## Decisions
1. **Stream cipher:** libsodium secretstream (XChaCha20-Poly1305), 1 MiB content chunks, 4096-byte encrypted metadata record first. Gives ordering and truncation protection through the `FINAL` tag.
2. **Transport parts = crypto chunks** (part 0 is header plus metadata). Makes resumable upload and per-part verification simple and keeps the server blind to structure.
3. **Burn-after-view uses claim then finish (POST), never GET.** Protects against link-preview bots and gives a 10-minute window with resumable download on weak networks. Cost: an interrupted, unfinished claim burns the drop when the window ends; senders are told.
4. **Passphrase wrapping stored inside the link payload** (Argon2id, 64 MiB, 3 passes by default, parameters carried in the link and bounded by receivers). Keeps the system accountless with no server involvement. Cost: anyone with the link can guess passphrases offline, so passphrase strength is the only barrier (threat T6a). Mitigation: generated passphrases offered in the UI.
5. **Padding to 64 KiB bands** (minimum 4096). Cheap, hides exact size only within the band.
6. **Server logs omit blob IDs** (route templates only), so logs cannot link an address to a specific drop. Needs lawyer confirmation.
7. **Single sub-key derivation context** `EMBERv1_` with reserved ids for the future thread key (M7).

## Alternatives rejected
- Storing a server-side passphrase check: gives the server a guessing oracle and breaks the no-knowledge property.
- Burning on first GET: breaks with preview bots and flaky networks.
- Variable-size random padding: harder to specify and test in v1; revisit with the auditor.

## Consequences
M2 and M3 must implement the spec exactly and produce test vectors. If an implementation proves a choice wrong, record a superseding ADR and bump the format version.
