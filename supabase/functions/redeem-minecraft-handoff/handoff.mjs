/**
 * @param {import("npm:@supabase/supabase-js@2").SupabaseClient} admin
 * @param {string} codeHash
 * @param {Date} [now]
 */
export async function consumeMinecraftHandoff(
  admin,
  codeHash,
  now = new Date(),
) {
  const consumedAt = now.toISOString();
  const { data, error } = await admin
    .from("minecraft_auth_handoffs")
    .update({ consumed_at: consumedAt })
    .eq("code_hash", codeHash)
    .is("consumed_at", null)
    .gt("expires_at", consumedAt)
    .select("encrypted_payload")
    .maybeSingle();

  if (error) return { kind: "error" };
  if (!data) return { kind: "expired_or_used" };
  return { kind: "ok", encryptedPayload: data.encrypted_payload };
}
