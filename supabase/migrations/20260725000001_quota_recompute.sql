-- P5-4 配额记账修复：从「签名预扣」增量模型改为「按实际存储重算」权威模型。
--
-- 旧模型缺陷（实测）：sign-url 在签发 PUT 地址时就 bytes_used += 文件大小，
-- 签了没传/重试重签/封面/重传都会重复累加，且无对账，已用只增不准、虚高、可误锁。
--
-- 新模型：bytes_used 恒等于该用户「真正传了文件（file_key 非空）且未删除」的
-- books.file_size 之和。幂等自愈——重试/重传/封面/多设备/删除都自动正确。
-- 由触发器在 books 的 file_key/file_size/deleted 变化时同步维护。

create or replace function public.recompute_user_quota()
returns trigger
language plpgsql
security definer
set search_path = public
as $$
declare
  uid uuid;
begin
  uid := coalesce(new.user_id, old.user_id);
  update public.user_quota q
  set bytes_used = coalesce((
    select sum(b.file_size)
    from public.books b
    where b.user_id = uid
      and b.file_key is not null
      and b.deleted = false
  ), 0)
  where q.user_id = uid;
  return null; -- AFTER 触发器，返回值被忽略
end;
$$;

-- 仅在影响配额的列变化时触发；title/进度等元数据 push 不触发，避免无谓重算。
drop trigger if exists recompute_quota_on_books on public.books;
create trigger recompute_quota_on_books
after insert or delete or update of file_key, file_size, deleted
on public.books
for each row
execute function public.recompute_user_quota();

-- 一次性回填：把历史预扣式累积的虚高值，全部改写为真实的文件大小之和。
update public.user_quota q
set bytes_used = coalesce((
  select sum(b.file_size)
  from public.books b
  where b.user_id = q.user_id
    and b.file_key is not null
    and b.deleted = false
), 0);
