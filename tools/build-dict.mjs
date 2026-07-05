// ECDICT → dict.db 构建脚本（构建期工具，不进产品代码）
//
// 用法（在任意临时目录执行，避免把 63MB 的 CSV 和 node_modules 留在仓库里）:
//   1. curl -L -o ecdict.csv https://raw.githubusercontent.com/skywind3000/ECDICT/master/ecdict.csv
//   2. npm install better-sqlite3 csv-parse
//   3. node build-dict.mjs 0 <仓库>/src-tauri/resources/dict.db
//
// 第一个参数为词频裁剪阈值，0 = 不按词频裁剪（推荐，产出约 36 万词条 / 24 MB）
// ECDICT: https://github.com/skywind3000/ECDICT (MIT License)
import { createReadStream } from "node:fs";
import { parse } from "csv-parse";
import Database from "better-sqlite3";
import { unlinkSync, existsSync, statSync } from "node:fs";

const FRQ_LIMIT = parseInt(process.argv[2] ?? "40000", 10);
const OUT = process.argv[3] ?? "dict.db";
const WORD_RE = /^[a-z][a-z'-]*$/;

// exchange 变形代码：p 过去式 d 过去分词 i 现在分词 3 三单 r 比较级 t 最高级 s 复数
const FORM_CODES = new Set(["p", "d", "i", "3", "r", "t", "s"]);

const rows = [];
const parser = createReadStream("ecdict.csv").pipe(
  parse({ columns: true, relax_quotes: true, relax_column_count: true, skip_records_with_error: true })
);

let total = 0;
for await (const r of parser) {
  total++;
  const word = (r.word ?? "").trim();
  const translation = (r.translation ?? "").trim();
  if (!WORD_RE.test(word) || !translation) continue;
  if (word.length > 48) continue;

  const collins = parseInt(r.collins || "0", 10) || 0;
  const oxford = parseInt(r.oxford || "0", 10) || 0;
  const tag = (r.tag ?? "").trim();
  const frq = parseInt(r.frq || "0", 10) || 0;
  const bnc = parseInt(r.bnc || "0", 10) || 0;

  // FRQ_LIMIT=0 表示不按词频裁剪（保留所有合法词条）
  const keep =
    FRQ_LIMIT === 0 ||
    collins >= 1 ||
    oxford === 1 ||
    tag !== "" ||
    (frq > 0 && frq <= FRQ_LIMIT) ||
    (bnc > 0 && bnc <= FRQ_LIMIT);
  if (!keep) continue;

  // 排序权重：越常用越靠前（frq/bnc 是词频排名，越小越常用；0 = 无数据）
  const rank = Math.min(frq > 0 ? frq : 1e9, bnc > 0 ? bnc : 1e9);
  rows.push({
    word,
    phonetic: (r.phonetic ?? "").trim() || null,
    translation: translation.replace(/\\n/g, "\n"),
    exchange: (r.exchange ?? "").trim(),
    rank,
  });
}

rows.sort((a, b) => a.rank - b.rank);

if (existsSync(OUT)) unlinkSync(OUT);
const db = new Database(OUT);
db.pragma("journal_mode = OFF");
db.pragma("synchronous = OFF");
db.exec(`
  CREATE TABLE entries (
    word        TEXT PRIMARY KEY,
    phonetic    TEXT,
    translation TEXT NOT NULL
  ) WITHOUT ROWID;
  CREATE TABLE forms (
    form  TEXT PRIMARY KEY,
    lemma TEXT NOT NULL
  ) WITHOUT ROWID;
`);

const insEntry = db.prepare("INSERT OR IGNORE INTO entries (word, phonetic, translation) VALUES (?, ?, ?)");
const insForm = db.prepare("INSERT OR IGNORE INTO forms (form, lemma) VALUES (?, ?)");

let entryCount = 0;
let formCount = 0;
db.transaction(() => {
  for (const r of rows) {
    if (insEntry.run(r.word, r.phonetic, r.translation).changes > 0) entryCount++;
  }
  // 常用词优先建立 form → lemma（同一变形归属多个词时，高频词胜出）
  for (const r of rows) {
    if (!r.exchange) continue;
    for (const part of r.exchange.split("/")) {
      const idx = part.indexOf(":");
      if (idx < 1) continue;
      const code = part.slice(0, idx);
      if (!FORM_CODES.has(code)) continue;
      for (const form of part.slice(idx + 1).split(",")) {
        const f = form.trim().toLowerCase();
        if (!WORD_RE.test(f) || f === r.word) continue;
        if (insForm.run(f, r.word).changes > 0) formCount++;
      }
    }
  }
})();
db.close();

const mb = (statSync(OUT).size / 1048576).toFixed(1);
console.log(`总词条扫描: ${total}`);
console.log(`entries: ${entryCount} 条`);
console.log(`forms:   ${formCount} 条`);
console.log(`dict.db: ${mb} MB (frqLimit=${FRQ_LIMIT})`);
