## Running E2E tests

Prerequisites: `docker compose up -d` (full stack) + `npm run dev` (frontend on :5173)

Run: `npm run e2e`

In CI: set E2E_BASE_URL to the running frontend URL.
The GitHub Actions workflow should add an `e2e` job that:
1. Runs `docker compose -f docker-compose.yml -f docker-compose.test.yml up -d`
2. Waits for health checks
3. Runs `npm run dev &` then `npm run e2e`
