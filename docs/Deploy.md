# Deploy

The dedicated binary (`fallingsand_server`) serves **WebTransport over UDP** (QUIC), default
`0.0.0.0:4433`. On start it prints a web client URL (from `FALLINGSAND_WEB_CLIENT_URL`) that pre-fills
the direct-connect menu's host field. Saves live at `saves/dedicated/world.redb`; keep the working
directory stable so cached certs and saves are reused.

## TLS modes (picked at startup, in order)

1. **Explicit cert** — `--cert <pem> --key <pem>` (must be given together). Used verbatim.
2. **Cloudflare-managed** — `--domain <name>` + `--cloudflare-token <tok>` (or env
   `FALLINGSAND_DOMAIN` / `CLOUDFLARE_API_TOKEN`). The server sets an `A` record to its public IP,
   then obtains a Let's Encrypt certificate via ACME **dns-01** (a `_acme-challenge` TXT record),
   caching it under `saves/certs/<domain>/`. A background **DDNS** task refreshes the `A` record every
   5 minutes.
3. **Self-signed fallback** — no cert/domain given. Generates a self-signed cert (SANs
   `localhost`/`127.0.0.1`/bind IP, **13-day** validity) and prints its SHA-256 hash; pin it via the
   cert field in the direct-connect menu.

The 13-day self-signed validity stays under WebTransport's ~14-day cap on
`serverCertificateHashes`-pinned certs.

## Caveat: no in-process cert renewal

ACME certs are obtained (or re-issued when older than `RENEW_AFTER` ≈ 60 days) **only at startup** —
there is no renewal loop. A server that runs continuously past its certificate's expiry will serve an
expired cert until restarted. For long-lived deployments, restart periodically (or before expiry).
The reqwest client used for Cloudflare/ACME has bounded connect/request timeouts so these calls can't
hang the startup path.
