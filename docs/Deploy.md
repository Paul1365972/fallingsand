# Deploy

The dedicated binary (`fallingsand_server`) serves WebTransport over UDP (QUIC), default `0.0.0.0:4433`. On start it prints a web client URL (from `FALLINGSAND_WEB_CLIENT_URL`) pre-filling the direct-connect host. Saves live at `saves/dedicated/world.redb`; keep the working directory stable so cached certs and saves are reused.

## TLS modes (picked at startup, in order)

1. **Explicit cert** — `--cert <pem> --key <pem>`, used verbatim.
2. **Cloudflare-managed** — `--domain` + `--cloudflare-token` (or env `FALLINGSAND_DOMAIN` / `CLOUDFLARE_API_TOKEN`). Sets an `A` record to the public IP, obtains a Let's Encrypt cert via ACME dns-01, caches it under `saves/certs/<domain>/`; a background DDNS task refreshes the record.
3. **Self-signed fallback** — 13-day validity (under WebTransport's cap on pinned certs); prints its SHA-256 hash to pin in the direct-connect menu.

ACME certs are obtained or re-issued (when older than ~60 days) **only at startup** — no renewal loop, so restart long-lived servers before expiry. Cloudflare/ACME calls have bounded timeouts and can't hang startup.
