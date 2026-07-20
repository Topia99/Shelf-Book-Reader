#!/usr/bin/env bash

set -euo pipefail

ROOT="/Users/jasonzeng/Developer/shelf-book-reader"
DEVICE_ID="${1:-911AA2E1-084E-438F-83A5-FA19473D9B97}"
BUNDLE_ID="${BUNDLE_ID:-com.shelf.reader}"
SAMPLE_PDF="${SAMPLE_PDF:-$ROOT/sample-books/测试书籍 test book.pdf}"
TITLE="${TITLE:-测试书籍 test book}"

if [[ ! -f "$SAMPLE_PDF" ]]; then
  echo "sample PDF not found: $SAMPLE_PDF" >&2
  exit 1
fi

APP_CONTAINER="$(xcrun simctl get_app_container "$DEVICE_ID" "$BUNDLE_ID" data)"
BASE_DIR="$APP_CONTAINER/Library/Application Support/$BUNDLE_ID"
BOOKS_DIR="$BASE_DIR/books"
DB_PATH="$BASE_DIR/library.db"

if [[ ! -f "$DB_PATH" ]]; then
  echo "library.db not found: $DB_PATH" >&2
  exit 1
fi

mkdir -p "$BOOKS_DIR"

HASH="$(shasum -a 256 "$SAMPLE_PDF" | awk '{print $1}')"
DEST_REL="books/$HASH.pdf"
DEST_ABS="$BOOKS_DIR/$HASH.pdf"
UPDATED_AT="$(python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
)"

sqlite3 "$DB_PATH" <<SQL
INSERT INTO books (hash, title, file_path, added_at, updated_at, deleted, cloud_state)
SELECT '$HASH', '$TITLE', '$DEST_REL', datetime('now', 'localtime'), $UPDATED_AT, 0, 'local'
WHERE NOT EXISTS (SELECT 1 FROM books WHERE hash = '$HASH' AND deleted = 0);
UPDATE books
SET title = '$TITLE',
    file_path = '$DEST_REL',
    deleted = 0,
    cloud_state = 'local',
    updated_at = $UPDATED_AT
WHERE hash = '$HASH';
SQL

cp "$SAMPLE_PDF" "$DEST_ABS"

echo "seeded $TITLE into $DEVICE_ID"
