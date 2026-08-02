# F-012 — Dashboard auth

Session-cookie login for the operator dashboard and API.

## Behavior

- Required env: `ANCHOR_PASSWORD`. Optional: `ANCHOR_USER` (default `admin`), `ANCHOR_SESSION_SECRET`, `ANCHOR_COOKIE_SECURE`.
- Public: `GET /healthz`, `GET|POST /login`, `GET /static/*`.
- Everything else (API, dashboard, settings, terminal WebSocket) requires a valid `anchor_session` cookie.
- Browser HTML requests without a session redirect to `/login`; API/WS get `401` JSON `{ "error", "code": "UNAUTHORIZED" }`.
- `POST /logout` clears the cookie.

## Security notes

- Password compare is length-checked and constant-time on padded bytes.
- Session token is `user|expiry|hmac` (HMAC-SHA256). Prefer setting `ANCHOR_SESSION_SECRET`; otherwise a key is derived from the password.
- Still keep Anchor off the public internet (Tailscale). Auth is defense in depth, not exposure license.
- Never log `ANCHOR_PASSWORD` or session secrets.
