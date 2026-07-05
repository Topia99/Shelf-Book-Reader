import { convertFileSrc } from "@tauri-apps/api/core";
import type { Book } from "../api";

/** 有封面文件时显示图片，否则显示「纯色 + 书名」默认封面（颜色由 hash 决定，稳定不变） */
export default function BookCover({ book }: { book: Book }) {
  if (book.cover_path) {
    return (
      <div className="cover">
        <img src={convertFileSrc(book.cover_path)} alt={book.title} draggable={false} />
      </div>
    );
  }
  const hue = parseInt(book.hash.slice(0, 6), 16) % 360;
  return (
    <div
      className="cover cover-fallback"
      style={{ background: `linear-gradient(160deg, hsl(${hue} 45% 42%), hsl(${hue} 50% 26%))` }}
    >
      <span>{book.title}</span>
    </div>
  );
}
