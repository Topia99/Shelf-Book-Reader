-- P2-3：RLS 用户隔离测试（在本地栈上以 psql 执行；任何断言失败即报错退出）
-- 用法：psql "$DB_URL" -v ON_ERROR_STOP=1 -f supabase/tests/rls_isolation_test.sql
begin;

-- 准备两个测试用户（直接写 auth.users，仅本地栈可行）
insert into auth.users (id, instance_id, aud, role, email, encrypted_password, email_confirmed_at, created_at, updated_at)
values
  ('00000000-0000-0000-0000-00000000000a', '00000000-0000-0000-0000-000000000000', 'authenticated', 'authenticated', 'a@test.local', '', now(), now(), now()),
  ('00000000-0000-0000-0000-00000000000b', '00000000-0000-0000-0000-000000000000', 'authenticated', 'authenticated', 'b@test.local', '', now(), now(), now());

-- 断言 1：handle_new_user 触发器已为两个新用户自动建配额行
do $$
begin
  assert (select count(*) from public.user_quota
          where user_id in ('00000000-0000-0000-0000-00000000000a','00000000-0000-0000-0000-00000000000b')) = 2,
    '新用户配额行未自动创建';
end $$;

-- 以服务身份为两个用户各插一本书、一条进度
insert into public.books (user_id, sha256, title, updated_at)
values
  ('00000000-0000-0000-0000-00000000000a', 'hash_a', 'A 的书', now()),
  ('00000000-0000-0000-0000-00000000000b', 'hash_b', 'B 的书', now());
insert into public.reading_progress (user_id, sha256, page, updated_at)
values
  ('00000000-0000-0000-0000-00000000000a', 'hash_a', 10, now()),
  ('00000000-0000-0000-0000-00000000000b', 'hash_b', 20, now());

-- 断言 2：server_updated_at 触发器生效（非空且接近 now）
do $$
begin
  assert (select count(*) from public.books where server_updated_at is null) = 0,
    'server_updated_at 未由触发器填充';
end $$;

-- ===== 模拟用户 A 的会话 =====
set local role authenticated;
set local request.jwt.claims to '{"sub":"00000000-0000-0000-0000-00000000000a","role":"authenticated"}';

do $$
begin
  -- 断言 3：A 只能看到自己的书和进度
  assert (select count(*) from public.books) = 1, 'A 看到了别人的书';
  assert (select title from public.books) = 'A 的书', 'A 看到的书不对';
  assert (select count(*) from public.reading_progress) = 1, 'A 看到了别人的进度';
  -- 断言 4：A 只能看到自己的配额
  assert (select count(*) from public.user_quota) = 1, 'A 看到了别人的配额';
end $$;

-- 断言 5：A 无法把行写到 B 名下（with check 拦截）
do $$
declare ok boolean := false;
begin
  begin
    insert into public.books (user_id, sha256, title, updated_at)
    values ('00000000-0000-0000-0000-00000000000b', 'hash_evil', '越权书', now());
  exception when insufficient_privilege or check_violation then
    ok := true;
  end;
  assert ok, 'A 竟能向 B 名下插入行';
end $$;

-- 断言 6：A 无法改动 B 的行（using 过滤 → 0 行受影响）
update public.reading_progress set page = 999
where user_id = '00000000-0000-0000-0000-00000000000b';
do $$
begin
  set local role postgres;
  assert (select page from public.reading_progress
          where user_id = '00000000-0000-0000-0000-00000000000b') = 20,
    'A 竟改动了 B 的进度';
end $$;

-- 断言 7：普通用户无法直接改配额（表级无 UPDATE 授权 → 硬性 permission denied）
set local role authenticated;
set local request.jwt.claims to '{"sub":"00000000-0000-0000-0000-00000000000a","role":"authenticated"}';
do $$
declare denied boolean := false;
begin
  begin
    update public.user_quota set bytes_limit = 999999999999
    where user_id = '00000000-0000-0000-0000-00000000000a';
  exception when insufficient_privilege then
    denied := true;
  end;
  assert denied, '普通用户竟能修改自己的配额上限（应被表级权限拒绝）';
end $$;
do $$
begin
  set local role postgres;
  assert (select bytes_limit from public.user_quota
          where user_id = '00000000-0000-0000-0000-00000000000a') = 536870912,
    '配额值被意外修改';
end $$;

rollback;  -- 测试数据全部回滚，不留痕
select 'RLS 隔离测试全部通过' as result;
