# F-005 — Health endpoint

**Phase:** MVP  
**API:** `GET /healthz`

## Intent

Container / Coolify health checks without touching GitHub or git.

## Acceptance

- [ ] Returns `200` quickly even if GitHub is down
- [ ] Does not require auth
