import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  listAccounts: vi.fn(),
  removeAccount: vi.fn(),
  upsertSyncedAccount: vi.fn(),
  encryptAccountSyncPayload: vi.fn(),
  restoreAccountSyncPayload: vi.fn(),
}));

vi.mock("../ipc/client", () => mocks);

import type { SupabaseClient } from "@supabase/supabase-js";
import type { Account } from "../ipc/types";
import { syncGameAccounts } from "./account-sync";

const account: Account = {
  id: "offline-uuid-1",
  uuid: "12345678-1234-4234-8234-123456789abc",
  username: "Steve",
  kind: "offline",
  uuid_prefix: "12345678123442348234123456789abc",
  selected: true,
  created_at: "2026-10-27T00:00:00Z",
};

function cloud(rows: unknown[] = []) {
  const writes: Record<string, unknown>[] = [];
  const records = [...rows] as Record<string, unknown>[];
  const client = {
    from: () => ({
      select: () => ({ order: async () => ({ data: records, error: null }) }),
      upsert: async (value: Record<string, unknown>) => {
        writes.push(value);
        const index = records.findIndex((row) => row.account_id === value.account_id);
        if (index < 0) records.push(value);
        else records[index] = { ...records[index], ...value };
        return { error: null };
      },
    }),
  } as unknown as SupabaseClient;
  return { client, writes };
}

describe("cloud account sync", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listAccounts.mockResolvedValue({
      accounts: [account],
      selected_id: account.id,
      nickname_claims: [],
    });
    mocks.removeAccount.mockResolvedValue(undefined);
    mocks.upsertSyncedAccount.mockResolvedValue(undefined);
    mocks.encryptAccountSyncPayload.mockResolvedValue([9, 8, 7]);
    mocks.restoreAccountSyncPayload.mockResolvedValue({ ...account, selected: false });
  });

  it("uploads only encrypted account data for a new cloud backup", async () => {
    const { client, writes } = cloud();
    await syncGameAccounts(client, "user-1", "separate sync password");
    expect(mocks.encryptAccountSyncPayload).toHaveBeenCalledWith(
      account.id,
      "separate sync password",
    );
    expect(writes).toEqual([
      { user_id: "user-1", account_id: account.id, payload: [9, 8, 7], deleted: false },
    ]);
    expect(JSON.stringify(writes)).not.toContain("separate sync password");
    expect(JSON.stringify(writes)).not.toContain("Steve");
  });

  it("restores cloud accounts and applies deletion tombstones", async () => {
    const { client } = cloud([
      { account_id: account.id, payload: [1, 2], deleted: false, updated_at: "2026-10-27" },
      { account_id: "offline-deleted", payload: null, deleted: true, updated_at: "2026-10-27" },
    ]);
    mocks.listAccounts.mockResolvedValue({ accounts: [], selected_id: null, nickname_claims: [] });
    await syncGameAccounts(client, "user-1", "separate sync password");
    expect(mocks.restoreAccountSyncPayload).toHaveBeenCalledWith(
      account.id,
      [1, 2],
      "separate sync password",
    );
    expect(mocks.upsertSyncedAccount).toHaveBeenCalledWith({ ...account, selected: false });
  });

  it("publishes deletions as tombstones so another device does not restore them", async () => {
    const { client, writes } = cloud();
    mocks.listAccounts.mockResolvedValue({ accounts: [], selected_id: null, nickname_claims: [] });
    await syncGameAccounts(client, "user-1", "separate sync password", [account.id]);
    expect(writes[0]).toEqual({
      user_id: "user-1",
      account_id: account.id,
      payload: null,
      deleted: true,
    });
  });
});
