# MinLauncher account authentication

The website now uses Supabase Auth email/password accounts. New users choose an email, a unique Minecraft nickname, and a password; Supabase sends the confirmation link to the email. Passwords are never stored in the MinLauncher application database or logged by the nickname-login function.

## Sign-in behavior

- Users sign in with their Minecraft nickname and MinLauncher password.
- `launcher-nickname` resolves registered nicknames and delegates password verification to Supabase Auth. Unknown nicknames use the same generic failure response as a wrong password.
- Supabase Auth's per-IP sign-up/sign-in rate limit applies to the password-grant request, including a synthetic address for unknown nicks.
- Password recovery uses the user's email and redirects to the existing `/auth/callback` allowlist entry. A `PASSWORD_RECOVERY` event routes the user to `/auth/reset-password`.
- New passwords require at least 12 Unicode characters and at most 72 UTF-8 bytes.

## Existing Google and Discord accounts

The website no longer offers Google or Discord sign-in. Keep those Supabase providers enabled until existing users have set a password through the email recovery flow and their accounts can sign in by nickname. Then disable the provider settings in the Supabase dashboard. Disabling them before migration would lock out users whose accounts do not yet have a password.

## Verified Supabase settings

The production project currently has email sign-up enabled, email confirmation enabled, the Email provider enabled, and the default sign-up/sign-in limit of 30 requests per 5 minutes per IP. Google and Discord remain enabled in Supabase pending migration of existing accounts; they are not exposed in the website UI.

### Email delivery (SMTP)
Supabase's built-in mailer has strict rate limits (2–3 emails/hour) and does not reliably deliver to major providers like Gmail due to SPF/DKIM policies. For email confirmations and password recovery to arrive in Gmail, configure **Custom SMTP** in Supabase Dashboard (e.g. Resend, Brevo, SendGrid, or Gmail SMTP) or disable email confirmation ("Confirm email" toggle under Authentication -> Providers -> Email). If a user with the given email already exists, Supabase returns an empty `identities` array without sending an email; the site detects this and prompts the user to log in or reset password.

## Microsoft linking

Minecraft account linking remains separate from MinLauncher account credentials. The site never asks for a Microsoft password or checks whether an email belongs to Microsoft. Linking opens the official Microsoft OAuth flow. Users without a Microsoft account must create one through Microsoft themselves before linking.
