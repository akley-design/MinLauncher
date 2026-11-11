create table public.minlauncher_activity_progress (
  user_id uuid primary key references auth.users (id) on delete cascade,
  total_milliseconds bigint not null default 0 check (total_milliseconds >= 0),
  curve_version smallint not null default 1 check (curve_version = 1),
  last_credited_at timestamptz,
  updated_at timestamptz not null default pg_catalog.now()
);

create table public.minlauncher_activity_sessions (
  session_id uuid primary key default pg_catalog.gen_random_uuid(),
  user_id uuid not null references auth.users (id) on delete cascade,
  source text not null check (source in ('site', 'launcher')),
  created_at timestamptz not null,
  last_heartbeat_at timestamptz not null,
  expires_at timestamptz not null,
  closed_at timestamptz,
  last_seq bigint not null default 0 check (last_seq >= 0),
  last_response jsonb,
  check (expires_at > created_at),
  check (closed_at is null or closed_at >= created_at)
);

create index minlauncher_activity_sessions_user_created_idx
  on public.minlauncher_activity_sessions (user_id, created_at desc);

create index minlauncher_activity_sessions_expiry_idx
  on public.minlauncher_activity_sessions (expires_at);

alter table public.minlauncher_activity_progress enable row level security;
alter table public.minlauncher_activity_sessions enable row level security;

revoke all on public.minlauncher_activity_progress from public, anon, authenticated;
revoke all on public.minlauncher_activity_sessions from public, anon, authenticated;
grant select on public.minlauncher_activity_progress to authenticated;

create policy "players read their own activity progress"
  on public.minlauncher_activity_progress for select to authenticated
  using ((select auth.uid()) = user_id);

create or replace function public.minlauncher_activity_get_progress(p_user_id uuid)
returns jsonb
language plpgsql
security definer
set search_path = ''
as $$
declare
  v_progress public.minlauncher_activity_progress%rowtype;
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

  return pg_catalog.jsonb_build_object(
    'total_milliseconds', v_progress.total_milliseconds::text,
    'curve_version', v_progress.curve_version
  );
end;
$$;

create or replace function public.minlauncher_activity_start(
  p_user_id uuid,
  p_source text
)
returns jsonb
language plpgsql
security definer
set search_path = ''
as $$
declare
  v_progress public.minlauncher_activity_progress%rowtype;
  v_session_id uuid;
  v_now timestamptz;
  v_live_count integer;
  v_hourly_count integer;
begin
  if p_user_id is null or p_source is null or p_source not in ('site', 'launcher') then
    raise exception 'invalid activity session';
  end if;

  insert into public.minlauncher_activity_progress (user_id)
  values (p_user_id)
  on conflict (user_id) do nothing;

  select * into v_progress
  from public.minlauncher_activity_progress
  where user_id = p_user_id
  for update;

  v_now := pg_catalog.clock_timestamp();

  select count(*) into v_live_count
  from public.minlauncher_activity_sessions
  where user_id = p_user_id
    and closed_at is null
    and expires_at > v_now
    and last_heartbeat_at > v_now - interval '45 seconds';
  if v_live_count >= 3 then
    return pg_catalog.jsonb_build_object('error_code', 'start_rate_limited');
  end if;

  select count(*) into v_hourly_count
  from public.minlauncher_activity_sessions
  where user_id = p_user_id
    and created_at > v_now - interval '1 hour';
  if v_hourly_count >= 120 then
    return pg_catalog.jsonb_build_object('error_code', 'start_rate_limited');
  end if;

  insert into public.minlauncher_activity_sessions (
    user_id,
    source,
    created_at,
    last_heartbeat_at,
    expires_at
  ) values (
    p_user_id,
    p_source,
    v_now,
    v_now,
    v_now + interval '12 hours'
  ) returning session_id into v_session_id;

  update public.minlauncher_activity_progress
  set updated_at = v_now
  where user_id = p_user_id;

  return pg_catalog.jsonb_build_object(
    'session_id', v_session_id::text,
    'expires_at', (v_now + interval '12 hours')
  );
end;
$$;

create or replace function public.minlauncher_activity_heartbeat(
  p_user_id uuid,
  p_session_id uuid,
  p_seq bigint
)
returns jsonb
language plpgsql
security definer
set search_path = ''
as $$
declare
  v_progress public.minlauncher_activity_progress%rowtype;
  v_session public.minlauncher_activity_sessions%rowtype;
  v_now timestamptz;
  v_elapsed_ms bigint;
  v_credit_start timestamptz;
  v_credited_ms bigint := 0;
  v_response jsonb;
  v_interval_status text;
begin
  if p_user_id is null
    or p_session_id is null
    or p_seq is null
    or p_seq < 1
    or p_seq > 2147483647 then
    raise exception 'invalid activity heartbeat';
  end if;

  select * into v_progress
  from public.minlauncher_activity_progress
  where user_id = p_user_id
  for update;
  if not found then
    return pg_catalog.jsonb_build_object('error_code', 'session_unavailable');
  end if;

  select * into v_session
  from public.minlauncher_activity_sessions
  where session_id = p_session_id
    and user_id = p_user_id
  for update;
  if not found then
    return pg_catalog.jsonb_build_object('error_code', 'session_unavailable');
  end if;

  v_now := greatest(pg_catalog.clock_timestamp(), v_session.last_heartbeat_at);
  if v_session.closed_at is not null or v_session.expires_at <= v_now then
    return pg_catalog.jsonb_build_object('error_code', 'session_unavailable');
  end if;

  if p_seq = v_session.last_seq and v_session.last_response is not null then
    return v_session.last_response || pg_catalog.jsonb_build_object('duplicate', true);
  end if;
  if p_seq < v_session.last_seq then
    return pg_catalog.jsonb_build_object('error_code', 'stale_sequence');
  end if;

  if p_seq > v_session.last_seq + 1 then
    v_interval_status := 'sequence_gap';
  else
    v_elapsed_ms := pg_catalog.floor(
      extract(epoch from (v_now - v_session.last_heartbeat_at)) * 1000
    )::bigint;
    if v_elapsed_ms < 10000 then
      v_interval_status := 'too_soon';
    elsif v_elapsed_ms > 45000 then
      v_interval_status := 'expired_interval';
    else
      v_interval_status := 'accepted';
      v_credit_start := greatest(
        v_session.last_heartbeat_at,
        coalesce(v_progress.last_credited_at, v_session.last_heartbeat_at)
      );
      v_credited_ms := greatest(
        0::bigint,
        pg_catalog.floor(
          extract(epoch from (v_now - v_credit_start)) * 1000
        )::bigint
      );

      if v_credited_ms > 0 then
        update public.minlauncher_activity_progress
        set total_milliseconds = total_milliseconds + v_credited_ms,
            last_credited_at = v_now,
            updated_at = v_now
        where user_id = p_user_id
        returning * into v_progress;
      end if;
    end if;
  end if;

  v_response := pg_catalog.jsonb_build_object(
    'total_milliseconds', v_progress.total_milliseconds::text,
    'curve_version', v_progress.curve_version,
    'credited_milliseconds', v_credited_ms,
    'interval_status', v_interval_status,
    'duplicate', false
  );

  update public.minlauncher_activity_sessions
  set last_seq = p_seq,
      last_heartbeat_at = case
        when v_interval_status in ('accepted', 'sequence_gap', 'expired_interval') then v_now
        else last_heartbeat_at
      end,
      expires_at = v_now + interval '12 hours',
      last_response = v_response
  where session_id = p_session_id;

  return v_response;
end;
$$;

create or replace function public.minlauncher_activity_pause(
  p_user_id uuid,
  p_session_id uuid
)
returns jsonb
language plpgsql
security definer
set search_path = ''
as $$
declare
  v_progress public.minlauncher_activity_progress%rowtype;
  v_session public.minlauncher_activity_sessions%rowtype;
begin
  if p_user_id is null or p_session_id is null then
    raise exception 'invalid activity session';
  end if;

  select * into v_progress
  from public.minlauncher_activity_progress
  where user_id = p_user_id
  for update;
  if not found then
    return pg_catalog.jsonb_build_object('paused', true);
  end if;

  select * into v_session
  from public.minlauncher_activity_sessions
  where session_id = p_session_id
    and user_id = p_user_id
  for update;
  if not found then
    return pg_catalog.jsonb_build_object('paused', true);
  end if;

  update public.minlauncher_activity_sessions
  set closed_at = pg_catalog.clock_timestamp(),
      expires_at = pg_catalog.clock_timestamp() + interval '7 days'
  where session_id = p_session_id
    and closed_at is null;

  return pg_catalog.jsonb_build_object('paused', true);
end;
$$;

revoke all on function public.minlauncher_activity_get_progress(uuid) from public, anon, authenticated;
revoke all on function public.minlauncher_activity_start(uuid, text) from public, anon, authenticated;
revoke all on function public.minlauncher_activity_heartbeat(uuid, uuid, bigint) from public, anon, authenticated;
revoke all on function public.minlauncher_activity_pause(uuid, uuid) from public, anon, authenticated;
grant execute on function public.minlauncher_activity_get_progress(uuid) to service_role;
grant execute on function public.minlauncher_activity_start(uuid, text) to service_role;
grant execute on function public.minlauncher_activity_heartbeat(uuid, uuid, bigint) to service_role;
grant execute on function public.minlauncher_activity_pause(uuid, uuid) to service_role;

create extension if not exists pg_cron;
do $$
declare
  v_job_id bigint;
begin
  select jobid into v_job_id
  from cron.job
  where jobname = 'minlauncher-cleanup-activity-sessions';

  if v_job_id is not null then
    perform cron.unschedule(v_job_id);
  end if;

  perform cron.schedule(
    'minlauncher-cleanup-activity-sessions',
    '40 3 * * *',
    $cleanup$
      delete from public.minlauncher_activity_sessions
      where expires_at < clock_timestamp()
        or (closed_at is not null and closed_at < clock_timestamp() - interval '7 days');
    $cleanup$
  );
end;
$$;
