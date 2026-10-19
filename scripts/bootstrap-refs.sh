#!/usr/bin/env bash
# 重建 refs/ 第三方参考仓。
# 净室红线：refs/ 内容永不提交（.gitignore），仅 refs/REFS.md 与本脚本入库；
# 换机器/新服务器时运行本脚本一键还原约 200MB 参考仓。
#
# 用法:
#   scripts/bootstrap-refs.sh            # 按 refs/REFS.md §D 冻结的 commit 精确重建
#   scripts/bootstrap-refs.sh --latest   # 拉各仓最新 tip（之后请把新 tip 回写 REFS.md §D）
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REFS="$ROOT/refs"
mkdir -p "$REFS"
cd "$REFS"

LATEST="${1:-}"
if [ -n "$LATEST" ] && [ "$LATEST" != "--latest" ]; then
  echo "用法: scripts/bootstrap-refs.sh [--latest]" >&2
  exit 2
fi

# name|url|pinned-sha（与 refs/REFS.md §D 保持一致；更新先改那边再同步这里）
ENTRIES=(
  "vectorcraft|https://github.com/storytold/vectorcraft.git|a466625"
  "designcraft|https://github.com/storytold/designcraft.git|443ea46"
  "cadcraft|https://github.com/storytold/cadcraft.git|0b32c2e"
  "craft-libs|https://github.com/storytold/craft-libs.git|ae2f796"
  "craft-common-actions|https://github.com/storytold/craft-common-actions.git|d7bf8f0"
  "craft-fonts|https://github.com/storytold/craft-fonts.git|d1cf47d"
  "libcdr|https://github.com/LibreOffice/libcdr.git|22c3968"
  "cdr-spec|https://github.com/photopea/CDR-specification.git|f91b3c7"
  "uniconvertor|https://github.com/sk1project/uniconvertor.git|973d5b6"
  "rea|https://github.com/cyberkaida/reverse-engineering-assistant.git|a05a6b0"
  "morluto-rea|https://github.com/morluto/rea.git|ba866a2"
)

for e in "${ENTRIES[@]}"; do
  IFS='|' read -r name url pin <<<"$e"
  if [ -d "$name/.git" ]; then
    echo "== 跳过已存在: $name"
    continue
  fi
  if [ "$LATEST" = "--latest" ]; then
    echo "== 克隆最新 tip: $name"
    git clone --depth 1 --single-branch "$url" "$name"
  else
    echo "== 按冻结版本重建: $name @ ${pin}"
    git init -q "$name"
    git -C "$name" remote add origin "$url"
    if git -C "$name" fetch -q --depth 1 origin "$pin"; then
      git -C "$name" checkout -q --detach FETCH_HEAD
    else
      echo "   !! 冻结 commit 获取失败（远端历史可能改写），退回最新 tip" >&2
      rm -rf "$name"
      git clone --depth 1 --single-branch "$url" "$name"
    fi
  fi
done

echo "== 完成: $(ls -d */ 2>/dev/null | wc -l | tr -d ' ') 个参考仓"
echo "== 红线提醒: uniconvertor 为 AGPL——agents 不得读取其源码（refs/REFS.md §C）"
