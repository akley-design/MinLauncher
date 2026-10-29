create table if not exists public.minlauncher_auth_handoffs (
  user_id uuid not null references auth.users (id) on delete cascade,
  code_hash text primary key,
  created_at timestamptz not null default now(),
  expires_at timestamptz not null,
  consumed_at timestamptz
);

create index if not exists minlauncher_auth_handoffs_expiry_idx
  on public.minlauncher_auth_handoffs (expires_at);

alter table public.minlauncher_auth_handoffs enable row level security;
revoke all on public.minlauncher_auth_handoffs from anon, authenticated;
grant all on public.minlauncher_auth_handoffs to service_role;
