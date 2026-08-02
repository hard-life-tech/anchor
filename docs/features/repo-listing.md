# F-001 — GitHub repo listing

**Phase:** MVP  
**API:** `GET /api/repos`

## Intent

Show the operator which repos the configured PAT can see for `GITHUB_USER`, so they can pick one to sync.

## Behavior

- Call authenticated GitHub REST `GET /user/repos` with `GITHUB_TOKEN` (includes **private** and org repos the token can access).
- Paginate (`per_page=100`) until exhausted.
- Use `GITHUB_API_URL` (github.com or GHES).
- Cache results in memory for a few minutes (rate limits).
- Map to `Repo { name, full_name, private, default_branch, clone_url }`.
- Never echo the token.

## Acceptance

- [x] Returns private and public repos for the token
- [ ] Second call within TTL does not hit GitHub
- [x] Failures surface as `502` with safe error message
- [x] Does not use public-only `GET /users/{user}/repos`
