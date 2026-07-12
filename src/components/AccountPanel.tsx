import { useEffect, useMemo, useRef, useState } from "react";
import {
  syncDeleteAccount,
  syncNow,
  syncSignIn,
  syncSignOut,
  syncSignUp,
  type SyncStatus,
} from "../api";

interface Props {
  open: boolean;
  status: SyncStatus;
  onClose: () => void;
  onRefreshStatus: () => Promise<void>;
}

export default function AccountPanel({ open, status, onClose, onRefreshStatus }: Props) {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleteText, setDeleteText] = useState("");
  const emailInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) {
      setError("");
      setPassword("");
      setConfirmDelete(false);
      setDeleteText("");
      return;
    }
    if (!status.signed_in) {
      setEmail(status.email ?? "");
      queueMicrotask(() => emailInputRef.current?.focus());
    }
  }, [open, status.email, status.signed_in]);

  const lastSyncText = useMemo(() => {
    if (status.syncing) return "同步中…";
    if (status.last_sync_ms == null) return "尚未同步";
    return `上次同步 ${new Date(status.last_sync_ms).toLocaleTimeString("zh-CN", {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    })}`;
  }, [status.last_sync_ms, status.syncing]);

  if (!open) return null;

  async function runAuth(mode: "sign_in" | "sign_up") {
    const normalizedEmail = email.trim();
    if (!normalizedEmail || !password) {
      setError("请输入邮箱和密码");
      return;
    }
    setSubmitting(true);
    setError("");
    try {
      if (mode === "sign_in") {
        await syncSignIn(normalizedEmail, password);
      } else {
        await syncSignUp(normalizedEmail, password);
      }
      setPassword("");
      await onRefreshStatus();
    } catch (e) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  }

  async function handleSyncNow() {
    setSubmitting(true);
    setError("");
    try {
      await syncNow();
      await onRefreshStatus();
    } catch (e) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  }

  async function handleSignOut() {
    setSubmitting(true);
    setError("");
    try {
      await syncSignOut();
      await onRefreshStatus();
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  }

  async function handleDeleteAccount() {
    setSubmitting(true);
    setError("");
    try {
      await syncDeleteAccount();
      setConfirmDelete(false);
      setDeleteText("");
      await onRefreshStatus();
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal account-modal" onClick={(e) => e.stopPropagation()}>
        <div className="account-header">
          <h3>账号与同步</h3>
          <button
            className="account-close"
            onClick={(e) => {
              e.stopPropagation();
              onClose();
            }}
            aria-label="关闭账号面板"
          >
            ×
          </button>
        </div>

        {!status.signed_in ? (
          <div className="account-section">
            <label className="account-label">
              邮箱
              <input
                ref={emailInputRef}
                className="account-input"
                type="email"
                autoComplete="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !submitting) {
                    void runAuth("sign_in");
                  }
                }}
              />
            </label>
            <label className="account-label">
              密码
              <input
                className="account-input"
                type="password"
                autoComplete="current-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !submitting) {
                    void runAuth("sign_in");
                  }
                }}
              />
            </label>
            <div className="account-actions">
              <button
                className="btn primary"
                disabled={submitting}
                onClick={() => void runAuth("sign_in")}
              >
                {submitting ? "登录中…" : "登录"}
              </button>
              <button className="btn" disabled={submitting} onClick={() => void runAuth("sign_up")}>
                注册新账号
              </button>
            </div>
            {error && <div className="account-error">{error}</div>}
          </div>
        ) : (
          <div className="account-section">
            <div className="account-row">
              <span className="account-row-label">当前账号</span>
              <span className="account-email">{status.email}</span>
            </div>
            <div className="account-row">
              <span className="account-row-label">同步状态</span>
              <span className="account-status-text">{lastSyncText}</span>
            </div>
            {status.last_error && <div className="account-error">{status.last_error}</div>}
            {error && <div className="account-error">{error}</div>}

            <div className="account-actions">
              <button
                className="btn primary"
                disabled={submitting || status.syncing}
                onClick={() => void handleSyncNow()}
              >
                {status.syncing ? "同步中…" : "立即同步"}
              </button>
              <button className="btn" disabled={submitting} onClick={() => void handleSignOut()}>
                退出登录
              </button>
            </div>

            <div className={"account-delete" + (confirmDelete ? " danger" : "")}>
              {!confirmDelete ? (
                <button
                  className="btn danger"
                  disabled={submitting}
                  onClick={() => setConfirmDelete(true)}
                >
                  删除账号
                </button>
              ) : (
                <>
                  {/* 二次确认必须手动输入 DELETE，避免误删云端数据。 */}
                  <p className="account-delete-text">
                    云端所有书籍与进度将被永久删除，本地文件保留。请输入 DELETE 以确认。
                  </p>
                  <input
                    className="account-input account-delete-input"
                    value={deleteText}
                    onChange={(e) => setDeleteText(e.target.value)}
                    placeholder="输入 DELETE"
                  />
                  <div className="account-actions">
                    <button
                      className="btn"
                      disabled={submitting}
                      onClick={() => {
                        setConfirmDelete(false);
                        setDeleteText("");
                      }}
                    >
                      取消
                    </button>
                    <button
                      className="btn danger"
                      disabled={submitting || deleteText !== "DELETE"}
                      onClick={() => void handleDeleteAccount()}
                    >
                      最终确认删除
                    </button>
                  </div>
                </>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
