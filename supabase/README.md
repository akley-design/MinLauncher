# Supabase launcher handoff

The migration and two Edge Functions provide a short-lived, one-use bridge from the signed-in website to the desktop app. The database stores only a SHA-256 hash of the 256-bit handoff code; the service-role key is used only inside Edge Functions.

Deploy from the repository root with the Supabase CLI:

```powershell
supabase link --project-ref <project-ref>
supabase db push
supabase functions deploy create-handoff
supabase functions deploy redeem-handoff
```

The project must have `SUPABASE_URL`, `SUPABASE_ANON_KEY`, and `SUPABASE_SERVICE_ROLE_KEY` available to Edge Functions. Keep the service-role key in Supabase's server-side function secrets only. The OAuth providers must return an email address because the server turns the one-use ticket into a Supabase magic-link verification token.

In Supabase Auth URL Configuration, allow the production website callback (`https://<site-domain>/auth/callback`) and `minlauncher://auth/callback`. Keep the existing Google and Discord provider configuration. Build the launcher with the same public `VITE_SUPABASE_URL` and `VITE_SUPABASE_ANON_KEY` used by the website.

For GitHub release builds, set repository Actions variables named `VITE_SUPABASE_URL` and `VITE_SUPABASE_ANON_KEY`; the release workflow checks them before building. These are public client values, not the service-role secret.
