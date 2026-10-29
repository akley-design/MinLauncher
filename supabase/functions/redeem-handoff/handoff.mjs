const alreadyUsed =
  "This launcher link was already used or has expired. Create a new link on the website.";

/**
 * @param {import("npm:@supabase/supabase-js@2").SupabaseClient} admin
 * @param {string} codeHash
 */
export async function completeHandoff(admin, codeHash) {
  const now = new Date().toISOString();
  const { data: handoff, error: handoffError } = await admin
    .from("minlauncher_auth_handoffs")
    .select("user_id")
    .eq("code_hash", codeHash)
    .is("consumed_at", null)
    .gt("expires_at", now)
    .maybeSingle();
  if (handoffError || !handoff) return { status: 401, error: alreadyUsed };

  const { data: userData, error: userError } =
    await admin.auth.admin.getUserById(handoff.user_id);
  const email = userData?.user?.email;
  if (userError || !email) {
    return {
      status: 400,
      error: "This account has no email available for secure sign-in",
    };
  }

  const { data: linkData, error: linkError } =
    await admin.auth.admin.generateLink({ type: "magiclink", email });
  const tokenHash = linkData?.properties?.hashed_token;
  if (linkError || !tokenHash) {
    return { status: 500, error: "Could not complete secure sign-in" };
  }

  const consumedAt = new Date().toISOString();
  const { data: consumed, error: consumeError } = await admin
    .from("minlauncher_auth_handoffs")
    .update({ consumed_at: consumedAt })
    .eq("code_hash", codeHash)
    .is("consumed_at", null)
    .gt("expires_at", consumedAt)
    .select("code_hash")
    .maybeSingle();
  if (consumeError || !consumed) return { status: 401, error: alreadyUsed };

  return { status: 200, tokenHash };
}
