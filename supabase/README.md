# Supabase launcher identity and handoff

The migrations provide a private, case-insensitive nickname registry and a short-lived, one-use bridge from the signed-in website to the desktop app. The database stores only a SHA-256 hash of offline claim tokens and handoff codes. The service-role key is used only inside Edge Functions.

Deploy from the repository root with the Supabase CLI:

```powershell
supabase link --project-ref <project-ref>
supabase db push
supabase functions deploy launcher-nickname
supabase functions deploy create-handoff
supabase functions deploy redeem-handoff
```

The project must have `SUPABASE_URL`, `SUPABASE_ANON_KEY`, and `SUPABASE_SERVICE_ROLE_KEY` available to Edge Functions. Keep the service-role key in Supabase's server-side function secrets only. The OAuth providers must return an email address because the server turns the one-use ticket into a Supabase magic-link verification token. Deploy in this order: database migrations, all three functions, website, then launcher.

`launcher-nickname` checks availability, atomically claims and releases offline nicknames, and manages each signed-in user's registered profile. It returns only whether a nickname is available; direct table access is revoked from browser roles. Registered usernames are selected before `create-handoff` will issue a launcher link.

The `minlauncher_game_accounts` table stores per-user account backup rows. Its RLS policy allows only the authenticated owner to access their rows; anonymous users have no table privileges. Account payloads are encrypted by the desktop app with the user's separate sync password before upload. The password and clear payload are never sent to Supabase. Apply the migration with `supabase db push` before deploying a launcher build that uses cloud account sync.

In Supabase Auth URL Configuration, allow the production website callback (`https://<site-domain>/auth/callback`) and `minlauncher://auth/callback`. Keep the existing Google and Discord provider configuration. Build the launcher with the same public `VITE_SUPABASE_URL` and `VITE_SUPABASE_ANON_KEY` used by the website.

For GitHub release builds, set repository Actions variables named `VITE_SUPABASE_URL` and `VITE_SUPABASE_ANON_KEY`; the release workflow checks them before building. These are public client values, not the service-role secret.
