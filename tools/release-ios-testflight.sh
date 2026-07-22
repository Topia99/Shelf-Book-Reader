#!/usr/bin/env bash
# P4-10 TestFlight 发布：tauri ios build（ASC API 密钥自动签名）→ fastlane pilot 上传。
# 凭证从仓库根的 .env.asc.local(gitignored) 读取，绝不入库。
#
# 用法：
#   tools/release-ios-testflight.sh validate   # 仅校验凭证 + App 是否就绪（快）
#   tools/release-ios-testflight.sh build       # 仅构建 App Store IPA
#   tools/release-ios-testflight.sh upload       # 仅上传已构建 IPA
#   tools/release-ios-testflight.sh release      # 构建 + 上传（默认）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
ACTION="${1:-release}"

[ -f .env.asc.local ] || { echo "缺少 .env.asc.local（从 .env.asc.local.example 复制填写）" >&2; exit 1; }
set -a; # shellcheck disable=SC1091
source .env.asc.local; set +a

# Tauri iOS 构建用 APPLE_API_* 做 xcodebuild 自动签名认证（免 Xcode GUI 登录）
export APPLE_API_KEY="${ASC_KEY_ID}"
export APPLE_API_ISSUER="${ASC_ISSUER_ID}"
export APPLE_API_KEY_PATH="${ASC_KEY_PATH}"

[ -f "$ASC_KEY_PATH" ] || { echo ".p8 私钥不存在：$ASC_KEY_PATH" >&2; exit 1; }

# build 号：单调递增整数。默认取「自 2020-01-01 起的分钟数」（约 7 位，天然递增、
# 每次上传唯一、CI/本地统一），也可用 BUILD_NUMBER 环境变量覆盖（CI 传入）。
BUILD_NUM="${BUILD_NUMBER:-$(( ($(date +%s) - 1577836800) / 60 ))}"

do_build() {
  echo "==> tauri ios build [App Store 导出 / API 自动签名 / build 号 ${BUILD_NUM}]"
  npm run tauri ios build -- --export-method app-store-connect --ci --build-number "${BUILD_NUM}"
}
do_upload() {
  echo "==> fastlane 上传 TestFlight"
  fastlane ios beta
}

case "$ACTION" in
  validate) fastlane ios validate ;;
  build)    do_build ;;
  upload)   do_upload ;;
  release)  do_build && do_upload ;;
  *) echo "未知动作：$ACTION（validate|build|upload|release）" >&2; exit 1 ;;
esac
