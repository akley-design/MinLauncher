create table if not exists public.minlauncher_game_accounts (
  user_id uuid not null references auth.users (id) on delete cascade,
  account_id text not null check (length(account_id) between 1 and 128),
  payload jsonb,
  deleted boolean not null default false,
  updated_at timestamptz not null default now(),
  primary key (user_id, account_id),
  check ((deleted and payload is null) or (not deleted and payload is not null))
);

alter table public.minlauncher_game_accounts enable row level security;
revoke all on public.minlauncher_game_accounts from anon, authenticated;
grant select, insert, update, delete on public.minlauncher_game_accounts to authenticated;

create policy "users manage their own game account backups"
  on public.minlauncher_game_accounts
  for all
  to authenticated
  using ((select auth.uid()) = user_id)
  with check ((select auth.uid()) = user_id);

create index if not exists minlauncher_game_accounts_updated_idx
  on public.minlauncher_game_accounts (user_id, updated_at desc);

create or replace function public.minlauncher_touch_game_account()
returns trigger language plpgsql set search_path = '' as $$
begin
  new.updated_at = now();
  return new;
end;
$$;

create trigger minlauncher_game_accounts_touch_updated_at
  before update on public.minlauncher_game_accounts
  for each row execute function public.minlauncher_touch_game_account();
