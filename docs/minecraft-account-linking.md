# Microsoft Minecraft account linking

The OAuth implementation is deployed to Supabase project `dbgcezpwqxhlttptdlph`, and the website build is published to Cloudflare Pages at `https://minlauncher.pages.dev/`.

## Microsoft Entra app registration

- Name: `MinLauncher Website`
- Application (client) ID: `11ecdc7a-a6b7-4a22-9c82-8394bb03b263`
- Supported account type: personal Microsoft accounts
- Platform: Web
- Redirect URI: `https://dbgcezpwqxhlttptdlph.supabase.co/functions/v1/minecraft-oauth-complete`
- Client secret: configured in Supabase as `MICROSOFT_CLIENT_SECRET`; never put it in client code or this file. Entra currently shows the secret expiring 25 April 2027.

The Microsoft app requests `XboxLive.signin`. Xbox Live, XSTS, and Minecraft Services must still be verified with a real Microsoft account that owns Minecraft Java Edition.

## Supabase Edge Function secrets

The following values are configured in the Supabase project. The client secret and encryption key are server-only:

```text
MICROSOFT_CLIENT_ID=11ecdc7a-a6b7-4a22-9c82-8394bb03b263
MICROSOFT_CLIENT_SECRET=<stored in Supabase; do not copy here>
MICROSOFT_REDIRECT_URI=https://dbgcezpwqxhlttptdlph.supabase.co/functions/v1/minecraft-oauth-complete
MICROSOFT_RETURN_URL=https://minlauncher.pages.dev/account/settings
MINECRAFT_HANDOFF_ENCRYPTION_KEY=<stored in Supabase; do not copy here>
```

The Supabase migration and five Edge Functions are deployed: `minecraft-oauth-start`, `minecraft-oauth-complete`, `minecraft-account-status`, `minecraft-account-unlink`, and `redeem-minecraft-handoff`. OAuth uses state plus PKCE. Game credentials are encrypted in a short-lived, single-use handoff row; only the launcher redeemer returns the decrypted payload, and the launcher stores tokens in its encrypted local credential store.

## User flow and remaining verification

The signed-in user opens Account settings, starts Microsoft sign-in, and approves the Xbox scopes. The callback checks the Xbox/XSTS chain, verifies that a Minecraft Java profile exists, links the XUID uniquely to the current MinLauncher user, then returns to the site. The browser opens `minlauncher://auth/minecraft`; the running launcher redeems the one-time code and adds the Microsoft profile to its account list. If Microsoft is linked to another MinLauncher user, linking stops without moving it.

When the Minecraft access token expires, the player must start Microsoft sign-in again from Account settings with **Update Microsoft sign-in**. No Microsoft or Minecraft refresh token is stored by the launcher. Still required: a real end-to-end login with a Microsoft account that owns Minecraft Java Edition, followed by launcher redemption, launching the game, and joining an `online-mode=true` server. Also verify cancellation, expired/reused handoffs, missing Xbox/Minecraft entitlement, existing links, and network errors. Therefore this integration is configured and deployed, but not yet verified as working end to end.
