-- 创建云同步元数据表
create table public.books (
  user_id uuid not null references auth.users(id) on delete cascade,
  sha256 text not null,
  title text not null,
  author text,
  page_count int,
  file_size bigint not null default 0,
  cover_key text,
  file_key text,
  updated_at timestamptz not null,
  server_updated_at timestamptz not null default now(),
  deleted boolean not null default false,
  primary key (user_id, sha256)
);

create table public.reading_progress (
  user_id uuid not null references auth.users(id) on delete cascade,
  sha256 text not null,
  page int not null default 1,
  zoom_mode text,
  view_mode text,
  device_name text,
  updated_at timestamptz not null,
  server_updated_at timestamptz not null default now(),
  primary key (user_id, sha256)
);

create table public.user_quota (
  user_id uuid primary key references auth.users(id) on delete cascade,
  bytes_used bigint not null default 0,
  bytes_limit bigint not null default 536870912,
  plan text not null default 'free'
);

-- 创建服务端更新时间触发函数
create function public.touch_server_updated_at()
returns trigger
language plpgsql
as $$
begin
  new.server_updated_at = now();
  return new;
end;
$$;

-- 为同步表绑定服务端更新时间触发器
create trigger touch_books_server_updated_at
before insert or update on public.books
for each row
execute function public.touch_server_updated_at();

create trigger touch_reading_progress_server_updated_at
before insert or update on public.reading_progress
for each row
execute function public.touch_server_updated_at();

-- 创建增量拉取所需索引
create index books_user_id_server_updated_at_idx
on public.books (user_id, server_updated_at);

create index reading_progress_user_id_server_updated_at_idx
on public.reading_progress (user_id, server_updated_at);

-- 启用三张表的行级安全
alter table public.books enable row level security;
alter table public.reading_progress enable row level security;
alter table public.user_quota enable row level security;

-- 创建 books 表的用户隔离策略
create policy "Users can select own books"
on public.books
for select
using (auth.uid() = user_id);

create policy "Users can insert own books"
on public.books
for insert
with check (auth.uid() = user_id);

create policy "Users can update own books"
on public.books
for update
using (auth.uid() = user_id)
with check (auth.uid() = user_id);

create policy "Users can delete own books"
on public.books
for delete
using (auth.uid() = user_id);

-- 创建 reading_progress 表的用户隔离策略
create policy "Users can select own reading progress"
on public.reading_progress
for select
using (auth.uid() = user_id);

create policy "Users can insert own reading progress"
on public.reading_progress
for insert
with check (auth.uid() = user_id);

create policy "Users can update own reading progress"
on public.reading_progress
for update
using (auth.uid() = user_id)
with check (auth.uid() = user_id);

create policy "Users can delete own reading progress"
on public.reading_progress
for delete
using (auth.uid() = user_id);

-- 创建 user_quota 表的只读策略
create policy "Users can select own quota"
on public.user_quota
for select
using (auth.uid() = user_id);

-- 创建新用户默认配额初始化函数
create function public.handle_new_user()
returns trigger
language plpgsql
security definer
set search_path = public
as $$
begin
  insert into public.user_quota (user_id)
  values (new.id)
  on conflict (user_id) do nothing;

  return new;
end;
$$;

-- 为 auth.users 绑定新用户配额初始化触发器
create trigger on_auth_user_created
after insert on auth.users
for each row
execute function public.handle_new_user();

-- 表级授权：RLS 只做行过滤，角色还需要表级权限才能访问（本地栈实测缺省无授权）
grant usage on schema public to authenticated, service_role;
grant select, insert, update, delete on public.books to authenticated;
grant select, insert, update, delete on public.reading_progress to authenticated;
grant select on public.user_quota to authenticated;
grant all on public.books, public.reading_progress, public.user_quota to service_role;
