create table public.minlauncher_activity_atlas_loadout (
  user_id uuid not null references auth.users (id) on delete cascade,
  slot smallint not null check (slot between 1 and 4),
  reward_id text not null check (reward_id ~ '^atlas_[a-z0-9_]{1,58}$'),
  primary key (user_id, slot),
  unique (user_id, reward_id)
);

alter table public.minlauncher_activity_atlas_loadout enable row level security;
revoke all on public.minlauncher_activity_atlas_loadout from public, anon, authenticated;
grant select on public.minlauncher_activity_atlas_loadout to authenticated;

create policy "players read their own atlas loadout"
  on public.minlauncher_activity_atlas_loadout for select to authenticated
  using ((select auth.uid()) = user_id);

create or replace function public.minlauncher_activity_get_progress(p_user_id uuid)
returns jsonb
language plpgsql
security definer
set search_path = ''
as $$
declare
  v_progress public.minlauncher_activity_progress%rowtype;
  v_equipped_atlas_ids jsonb;
begin
  if p_user_id is null then
    raise exception 'invalid activity owner';
  end if;

  insert into public.minlauncher_activity_progress (user_id)
  values (p_user_id)
  on conflict (user_id) do nothing;

  select * into v_progress
  from public.minlauncher_activity_progress
  where user_id = p_user_id;

  select coalesce(
    pg_catalog.jsonb_agg(loadout.reward_id order by loadout.slot),
    '[]'::jsonb
  ) into v_equipped_atlas_ids
  from public.minlauncher_activity_atlas_loadout as loadout
  where loadout.user_id = p_user_id;

  return pg_catalog.jsonb_build_object(
    'total_milliseconds', v_progress.total_milliseconds::text,
    'curve_version', v_progress.curve_version,
    'equipped_atlas_ids', v_equipped_atlas_ids
  );
end;
$$;

create or replace function public.minlauncher_activity_set_atlas_loadout(
  p_user_id uuid,
  p_reward_ids text[]
)
returns jsonb
language plpgsql
security definer
set search_path = ''
as $$
declare
  v_equipped_atlas_ids jsonb;
begin
  if p_user_id is null
    or p_reward_ids is null
    or pg_catalog.cardinality(p_reward_ids) > 4
    or pg_catalog.array_position(p_reward_ids, null) is not null
    or exists (
      select 1
      from pg_catalog.unnest(p_reward_ids) as requested(reward_id)
      where requested.reward_id !~ '^atlas_[a-z0-9_]{1,58}$'
    )
    or pg_catalog.cardinality(p_reward_ids) <> (
      select count(distinct requested.reward_id)
      from pg_catalog.unnest(p_reward_ids) as requested(reward_id)
    ) then
    raise exception 'invalid activity atlas loadout';
  end if;

  insert into public.minlauncher_activity_progress (user_id)
  values (p_user_id)
  on conflict (user_id) do nothing;

  perform 1
  from public.minlauncher_activity_progress
  where user_id = p_user_id
  for update;

  delete from public.minlauncher_activity_atlas_loadout
  where user_id = p_user_id;

  insert into public.minlauncher_activity_atlas_loadout (
    user_id,
    slot,
    reward_id
  )
  select
    p_user_id,
    requested.ordinality::smallint,
    requested.reward_id
  from pg_catalog.unnest(p_reward_ids) with ordinality as requested(reward_id, ordinality);

  select coalesce(
    pg_catalog.jsonb_agg(loadout.reward_id order by loadout.slot),
    '[]'::jsonb
  ) into v_equipped_atlas_ids
  from public.minlauncher_activity_atlas_loadout as loadout
  where loadout.user_id = p_user_id;

  return pg_catalog.jsonb_build_object(
    'equipped_atlas_ids', v_equipped_atlas_ids
  );
end;
$$;

revoke all on function public.minlauncher_activity_get_progress(uuid)
  from public, anon, authenticated;
revoke all on function public.minlauncher_activity_set_atlas_loadout(uuid, text[])
  from public, anon, authenticated;
grant execute on function public.minlauncher_activity_get_progress(uuid)
  to service_role;
grant execute on function public.minlauncher_activity_set_atlas_loadout(uuid, text[])
  to service_role;
