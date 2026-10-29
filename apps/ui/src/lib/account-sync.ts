import type { SupabaseClient } from "@supabase/supabase-js";

import {
  encryptAccountSyncPayload,
  listAccounts,
  removeAccount,
  restoreAccountSyncPayload,
  upsertSyncedAccount,
} from "../ipc/client";
import type { Account } from "../ipc/types";

const TABLE = "minlauncher_game_accounts";

interface CloudAccountRow {
  account_id: string;
  payload: number[] | null;
  deleted: boolean;
  updated_at: string;
}

function sameAccount(a: Account, b: Account): boolean {
  return (
    a.id === b.id &&
    a.uuid === b.uuid &&
    a.username === b.username &&
    a.kind === b.kind &&
    a.uuid_prefix === b.uuid_prefix &&
    a.created_at === b.created_at
  );
}

/** Merge this device's account list with the current user's encrypted cloud records. */
export async function syncGameAccounts(
  client: SupabaseClient,
  userId: string,
  password: string,
  deletedAccountIds: string[] = [],
): Promise<string[]> {
  for (const accountId of deletedAccountIds) {
    const { error } = await client
      .from(TABLE)
      .upsert(
        { user_id: userId, account_id: accountId, payload: null, deleted: true },
        { onConflict: "user_id,account_id" },
      );
    if (error) throw error;
  }

  const { data, error } = await client
    .from(TABLE)
    .select("account_id,payload,deleted,updated_at")
    .order("updated_at", { ascending: true });
  if (error) throw error;
  const remoteRows = (data ?? []) as CloudAccountRow[];
  const remoteAccounts = new Map<string, Account>();
  for (const row of remoteRows) {
    if (row.deleted) continue;
    if (!Array.isArray(row.payload)) throw new Error("Облачная запись аккаунта повреждена.");
    const account = await restoreAccountSyncPayload(row.account_id, row.payload, password);
    if (account.id !== row.account_id)
      throw new Error("Идентификатор облачного аккаунта не совпадает.");
    remoteAccounts.set(row.account_id, account);
  }

  const local = await listAccounts();
  const localAccounts = new Map(local.accounts.map((account) => [account.id, account]));
  for (const row of remoteRows) {
    if (row.deleted) {
      if (localAccounts.has(row.account_id)) {
        await removeAccount(row.account_id);
        localAccounts.delete(row.account_id);
      }
      continue;
    }
    const cloudAccount = remoteAccounts.get(row.account_id);
    if (cloudAccount && !localAccounts.has(row.account_id)) {
      await upsertSyncedAccount(cloudAccount);
      localAccounts.set(row.account_id, cloudAccount);
    }
  }

  for (const account of localAccounts.values()) {
    const row = remoteRows.find((entry) => entry.account_id === account.id);
    if (row?.deleted) continue;
    const cloudAccount = remoteAccounts.get(account.id);
    if (cloudAccount && sameAccount(cloudAccount, account)) continue;
    const payload = await encryptAccountSyncPayload(account.id, password);
    const { error: saveError } = await client
      .from(TABLE)
      .upsert(
        { user_id: userId, account_id: account.id, payload, deleted: false },
        { onConflict: "user_id,account_id" },
      );
    if (saveError) throw saveError;
  }

  return [...localAccounts.keys()];
}
