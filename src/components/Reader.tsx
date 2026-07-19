import { useState } from "react";
import type { Book } from "../api";
import { isIos } from "../platform";
import ReaderDesktopShell from "./reader/ReaderDesktopShell";
import ReaderIosShell from "./reader/ReaderIosShell";
import { usePdfReaderController } from "./reader/usePdfReaderController";

interface Props {
  book: Book;
  onBack: () => void;
}

export default function Reader({ book, onBack }: Props) {
  const controller = usePdfReaderController({
    book,
    defaultZoom: isIos ? "fit-width" : "fit-page",
    onBack,
  });
  const ReaderShell = isIos ? ReaderIosShell : ReaderDesktopShell;

  if (controller.loadError) {
    return (
      <div className="reader-fallback">
        <p>{controller.loadError}</p>
        <button className="btn" onClick={onBack}>
          返回书库
        </button>
      </div>
    );
  }

  if (controller.needPassword) {
    return (
      <PasswordPrompt
        title={book.title}
        error={controller.passwordError}
        onSubmit={(password) => controller.loadDoc(password)}
        onCancel={onBack}
      />
    );
  }

  return <ReaderShell bookTitle={book.title} controller={controller} />;
}

function PasswordPrompt({
  title,
  error,
  onSubmit,
  onCancel,
}: {
  title: string;
  error: string;
  onSubmit: (pwd: string) => void;
  onCancel: () => void;
}) {
  const [pwd, setPwd] = useState("");
  return (
    <div className="reader-fallback">
      <h3>《{title}》已加密</h3>
      <p>请输入打开密码：</p>
      <input
        type="password"
        autoFocus
        value={pwd}
        onChange={(e) => setPwd(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && pwd) onSubmit(pwd);
          if (e.key === "Escape") onCancel();
        }}
      />
      {error && <p className="error-text">{error}</p>}
      <div className="modal-actions">
        <button className="btn" onClick={onCancel}>
          返回书库
        </button>
        <button className="btn primary" disabled={!pwd} onClick={() => onSubmit(pwd)}>
          打开
        </button>
      </div>
    </div>
  );
}
