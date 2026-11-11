create table public.minlauncher_telegram_links (
  user_id uuid primary key references auth.users (id) on delete cascade,
  telegram_user_id bigint not null unique check (telegram_user_id > 0),
  linked_at timestamptz not null default pg_catalog.clock_timestamp()
);

create table public.minlauncher_telegram_link_challenges (
  user_id uuid primary key references auth.users (id) on delete cascade,
  token_hash text not null unique check (token_hash ~ '^[0-9a-f]{64}$'),
  created_at timestamptz not null default pg_catalog.clock_timestamp(),
  expires_at timestamptz not null,
  consumed_at timestamptz,
  check (expires_at > created_at),
  check (consumed_at is null or consumed_at >= created_at)
);

alter table public.minlauncher_telegram_links enable row level security;
alter table public.minlauncher_telegram_link_challenges enable row level security;

revoke all on table public.minlauncher_telegram_links from public, anon, authenticated;
revoke all on table public.minlauncher_telegram_link_challenges from public, anon, authenticated;
grant all on table public.minlauncher_telegram_links to service_role;
grant all on table public.minlauncher_telegram_link_challenges to service_role;

create or replace function public.minlauncher_redeem_telegram_link(
  p_token_hash text,
  p_telegram_user_id bigint
)
returns text
language plpgsql
security definer
set search_path = ''
as $$
declare
  v_challenge public.minlauncher_telegram_link_challenges%rowtype;
begin
  if p_token_hash is null
    or p_token_hash !~ '^[0-9a-f]{64}$'
    or p_telegram_user_id is null
    or p_telegram_user_id <= 0 then
    return 'invalid';
  end if;

  select * into v_challenge
  from public.minlauncher_telegram_link_challenges
  where token_hash = p_token_hash
  for update;

  if not found then
    return 'invalid';
  end if;
  if v_challenge.consumed_at is not null then
    return 'used';
  end if;
  if v_challenge.expires_at <= pg_catalog.clock_timestamp() then
    return 'expired';
  end if;
  if exists (
    select 1
    from public.minlauncher_telegram_links
    where user_id = v_challenge.user_id
       or telegram_user_id = p_telegram_user_id
  ) then
    return 'already_linked';
  end if;

  begin
    update public.minlauncher_telegram_link_challenges
    set consumed_at = pg_catalog.clock_timestamp()
    where user_id = v_challenge.user_id
      and token_hash = p_token_hash
      and consumed_at is null;

    if not found then
      return 'used';
    end if;

    insert into public.minlauncher_telegram_links (user_id, telegram_user_id)
    values (v_challenge.user_id, p_telegram_user_id);
  exception
    when unique_violation then
      return 'already_linked';
  end;

  return 'linked';
end;
$$;

create or replace function public.minlauncher_unlink_telegram(
  p_user_id uuid default null,
  p_telegram_user_id bigint default null
)
returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
  v_user_id uuid;
begin
  if (p_user_id is null) = (p_telegram_user_id is null)
    or (p_telegram_user_id is not null and p_telegram_user_id <= 0) then
    raise exception 'invalid telegram unlink request';
  end if;

  if p_user_id is not null then
    v_user_id := p_user_id;
  else
    select user_id into v_user_id
    from public.minlauncher_telegram_links
    where telegram_user_id = p_telegram_user_id
    for update;
    if not found then
      return;
    end if;
  end if;

  delete from public.minlauncher_telegram_links where user_id = v_user_id;
  update public.minlauncher_telegram_link_challenges
  set consumed_at = pg_catalog.clock_timestamp()
  where user_id = v_user_id and consumed_at is null;
end;
$$;

revoke all on function public.minlauncher_redeem_telegram_link(text, bigint)
  from public, anon, authenticated;
revoke all on function public.minlauncher_unlink_telegram(uuid, bigint)
  from public, anon, authenticated;
grant execute on function public.minlauncher_redeem_telegram_link(text, bigint) to service_role;
grant execute on function public.minlauncher_unlink_telegram(uuid, bigint) to service_role;