# F-001 — GitHub repo listing

**Phase:** MVP  
**API:** `GET /api/repos`

## Intent

Show the operator which repos the configured PAT can see for `GITHUB_USER`, so they can pick one to sync.

## Behavior

- Call GitHub REST with `GITHUB_TOKEN`.
- Cache results in memory for a few minutes (rate limits).
- Map to `Repo { name, full_name, private, default_branch, clone_url }`.
- Never echo the token.

## Acceptance

- [ ] Returns repos for the configured user
- [ ] Second call within TTL does not hit GitHub
- [ ] Failures surface as `502` with safe error message
