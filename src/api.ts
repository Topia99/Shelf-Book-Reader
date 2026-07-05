import { invoke } from "@tauri-apps/api/core";

export interface Book {
  id: number;
  hash: string;
  title: string;
  file_path: string;
  cover_path: string | null;
  total_pages: number | null;
  current_page: number;
  added_at: string;
  last_opened_at: string | null;
}

export interface AddResult {
  path: string;
  status: "added" | "duplicate" | "error";
  message: string | null;
  book: Book | null;
}

export type SortKey = "recent" | "added" | "title";

export const listBooks = (sort: SortKey, query: string) =>
  invoke<Book[]>("list_books", { sort, query });

export const addBooks = (paths: string[]) =>
  invoke<AddResult[]>("add_books", { paths });

export const removeBook = (id: number) => invoke<void>("remove_book", { id });

export const updateProgress = (id: number, page: number) =>
  invoke<void>("update_progress", { id, page });

export const renameBook = (id: number, title: string) =>
  invoke<void>("rename_book", { id, title });

export const setTotalPages = (id: number, total: number) =>
  invoke<void>("set_total_pages", { id, total });

export const saveCover = (hash: string, data: number[]) =>
  invoke<string>("save_cover", { hash, data });
