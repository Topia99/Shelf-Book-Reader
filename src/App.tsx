import { useState } from "react";
import type { Book } from "./api";
import Library from "./components/Library";
import Reader from "./components/Reader";

export default function App() {
  const [readingBook, setReadingBook] = useState<Book | null>(null);

  if (readingBook) {
    return <Reader book={readingBook} onBack={() => setReadingBook(null)} />;
  }
  return <Library onOpenBook={setReadingBook} />;
}
