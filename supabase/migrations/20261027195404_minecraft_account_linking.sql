create table if not exists public.minecraft_oauth_states (
  state_hash text primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  code_verifier text not null,
  created_at timestamptz not null default now(),
  expires_at timestamptz not null,
  consumed_at timestamptz
);

create index if not exists minecraft_oauth_states_expiry_idx
  on public.minecraft_oauth_states (expires_at);
create index if not exists minecraft_oauth_states_user_expiry_idx
  on public.minecraft_oauth_states (user_id, expires_at);

create table if not exists public.minecraft_account_links (
  user_id uuid primary key references auth.users (id) on delete cascade,
  xuid text not null unique,
  minecraft_uuid uuid not null,
  minecraft_username text not null,
  linked_at timestamptz not null default now()
);

create table if not exists public.minecraft_auth_handoffs (
  code_hash text primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  encrypted_payload text not null,
  created_at timestamptz not null default now(),
  expires_at timestamptz not null,
  consumed_at timestamptz
);

create index if not exists minecraft_auth_handoffs_expiry_idx
  on public.minecraft_auth_handoffs (expires_at);
create index if not exists minecraft_auth_handoffs_user_idx
  on public.minecraft_auth_handoffs (user_id);

alter table public.minecraft_oauth_states enable row level security;
alter table public.minecraft_account_links enable row level security;
alter table public.minecraft_auth_handoffs enable row level security;

revoke all on public.minecraft_oauth_states from anon, authenticated;
revoke all on public.minecraft_account_links from anon, authenticated;
revoke all on public.minecraft_auth_handoffs from anon, authenticated;
grant all on public.minecraft_oauth_states to service_role;
grant all on public.minecraft_account_links to service_role;
grant all on public.minecraft_auth_handoffs to service_role;
