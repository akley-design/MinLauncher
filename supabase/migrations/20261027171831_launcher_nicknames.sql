create table if not exists public.minlauncher_nicknames (
  nickname_normalized text primary key check (nickname_normalized ~ '^[a-z0-9_]{3,16}$'),
  nickname text not null check (nickname ~ '^[A-Za-z0-9_]{3,16}$'),
  owner_kind text not null check (owner_kind in ('registered', 'offline')),
  user_id uuid unique references auth.users(id) on delete cascade,
  claim_token_hash text unique,
  player_uuid uuid not null unique,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  check ((owner_kind = 'registered' and user_id is not null and claim_token_hash is null)
      or (owner_kind = 'offline' and user_id is null and claim_token_hash is not null))
);

alter table public.minlauncher_nicknames enable row level security;
revoke all on public.minlauncher_nicknames from anon, authenticated;
grant all on public.minlauncher_nicknames to service_role;

create or replace function public.minlauncher_check_nickname(p_nickname text)
returns boolean language sql security definer set search_path = '' as $$
  select not exists (
    select 1 from public.minlauncher_nicknames
    where nickname_normalized = lower(p_nickname)
  );
$$;
revoke all on function public.minlauncher_check_nickname(text) from public, anon, authenticated;
grant execute on function public.minlauncher_check_nickname(text) to service_role;

create or replace function public.minlauncher_claim_offline(
  p_nickname text, p_nickname_normalized text, p_claim_token_hash text, p_player_uuid uuid
) returns table (nickname text, player_uuid uuid)
language plpgsql security definer set search_path = '' as $$
begin
  if exists (select 1 from public.minlauncher_nicknames
    where nickname_normalized = p_nickname_normalized and claim_token_hash = p_claim_token_hash
      and owner_kind = 'offline' and player_uuid = p_player_uuid) then
    return query select n.nickname, n.player_uuid from public.minlauncher_nicknames n
      where n.nickname_normalized = p_nickname_normalized;
    return;
  end if;
  insert into public.minlauncher_nicknames
    (nickname_normalized, nickname, owner_kind, claim_token_hash, player_uuid)
  values (p_nickname_normalized, p_nickname, 'offline', p_claim_token_hash, p_player_uuid);
  return query select p_nickname, p_player_uuid;
exception when unique_violation then
  return;
end;
$$;
revoke all on function public.minlauncher_claim_offline(text, text, text, uuid) from public, anon, authenticated;
grant execute on function public.minlauncher_claim_offline(text, text, text, uuid) to service_role;

create or replace function public.minlauncher_release_offline(p_claim_token_hash text)
returns boolean language plpgsql security definer set search_path = '' as $$
declare affected integer;
begin
  delete from public.minlauncher_nicknames
   where owner_kind = 'offline' and claim_token_hash = p_claim_token_hash;
  get diagnostics affected = row_count;
  return affected > 0;
end;
$$;
revoke all on function public.minlauncher_release_offline(text) from public, anon, authenticated;
grant execute on function public.minlauncher_release_offline(text) to service_role;

create or replace function public.minlauncher_claim_registered(
  p_user_id uuid, p_nickname text, p_nickname_normalized text,
  p_claim_token_hash text default null
) returns table (nickname text, player_uuid uuid)
language plpgsql security definer set search_path = '' as $$
declare current_row public.minlauncher_nicknames%rowtype;
begin
  select * into current_row from public.minlauncher_nicknames where user_id = p_user_id for update;
  if found then
    if current_row.nickname_normalized = p_nickname_normalized then
      return query select current_row.nickname, current_row.player_uuid;
      return;
    end if;
    if exists (select 1 from public.minlauncher_nicknames where nickname_normalized = p_nickname_normalized) then
      return;
    end if;
    delete from public.minlauncher_nicknames where user_id = p_user_id;
    insert into public.minlauncher_nicknames (nickname_normalized, nickname, owner_kind, user_id, player_uuid)
      values (p_nickname_normalized, p_nickname, 'registered', p_user_id, current_row.player_uuid);
    return query select p_nickname, current_row.player_uuid;
    return;
  end if;

  select * into current_row from public.minlauncher_nicknames
   where nickname_normalized = p_nickname_normalized for update;
  if found then
    if current_row.owner_kind <> 'offline' or p_claim_token_hash is null
      or current_row.claim_token_hash <> p_claim_token_hash then
      return;
    end if;
    update public.minlauncher_nicknames set owner_kind = 'registered', user_id = p_user_id,
      claim_token_hash = null, nickname = p_nickname, updated_at = now()
      where nickname_normalized = p_nickname_normalized;
    return query select p_nickname, current_row.player_uuid;
    return;
  end if;
  insert into public.minlauncher_nicknames (nickname_normalized, nickname, owner_kind, user_id, player_uuid)
    values (p_nickname_normalized, p_nickname, 'registered', p_user_id, gen_random_uuid());
  return query select p_nickname, n.player_uuid from public.minlauncher_nicknames n where n.user_id = p_user_id;
exception when unique_violation then
  return;
end;
$$;
revoke all on function public.minlauncher_claim_registered(uuid, text, text, text) from public, anon, authenticated;
grant execute on function public.minlauncher_claim_registered(uuid, text, text, text) to service_role;
