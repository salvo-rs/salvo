# Clerk + React Starter

This repository shows how to use [Clerk](https://clerk.dev?utm_source=github&utm_medium=starter_repos&utm_campaign=react_starter) with React.

## Running the starter locally

1. Sign up for a Clerk account at https://clerk.dev
2. Fork and/or clone this repository
3. Install dependencies: `npm install`
4. Add your "Publishable Key" (found on [API Keys](https://dashboard.clerk.dev/last-active?path=/api-keys)) to a file called `.env.local`:

```sh
echo "VITE_CLERK_PUBLISHABLE_KEY=CLERK_PUBLISHABLE_KEY" >> app/.env.local
```

5. Run `cargo run`
6. Run the app: `cd app && npm run dev`

