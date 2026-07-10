# Deploy

The dedicated binary (`fallingsand_server`) serves WebTransport over UDP (QUIC), default `0.0.0.0:4433`. On start it prints a web client URL (from `FALLINGSAND_WEB_CLIENT_URL`) that pre-fills the direct-connect host field. Saves live at `saves/dedicated/world.redb`; keep the working directory stable so cached certs and saves are reused.

## TLS modes (picked at startup, in order)

1. **Explicit cert** — `--cert <pem> --key <pem>` (together). Used verbatim.
2. **Cloudflare-managed** — `--domain` + `--cloudflare-token` (or env `FALLINGSAND_DOMAIN` / `CLOUDFLARE_API_TOKEN`). Sets an `A` record to the public IP, obtains a Let's Encrypt cert via ACME dns-01, caches it under `saves/certs/<domain>/`; a background DDNS task refreshes the `A` record every 5 minutes.
3. **Self-signed fallback** — generates a self-signed cert (SANs `localhost`/`127.0.0.1`/bind IP; 13-day validity, under WebTransport's ~14-day cap on pinned certs) and prints its SHA-256 hash; pin it in the direct-connect menu.

ACME certs are obtained or re-issued (when older than `RENEW_AFTER` ≈ 60 days) **only at startup** — there is no renewal loop, so restart long-lived servers before expiry. Cloudflare/ACME calls have bounded timeouts and can't hang startup.
