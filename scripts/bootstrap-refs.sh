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

# name|url|pinned-sha（与 refs/REFS.md §D 保持一致；必须用 40 位完整 SHA 供 git fetch --depth 1 精确拉取）
ENTRIES=(
  "vectorcraft|https://github.com/storytold/vectorcraft.git|a466625c8987550635cda951ebf36e08e1e80a56"
  "designcraft|https://github.com/storytold/designcraft.git|443ea4640ca368fcf0241b04813bd3817acd5df8"
  "cadcraft|https://github.com/storytold/cadcraft.git|0b32c2e6a14c761835ea62c01e7b20845971d781"
  "craft-libs|https://github.com/storytold/craft-libs.git|ae2f796b673f29cf3161f36eb6a8933f0a3fe028"
  "craft-common-actions|https://github.com/storytold/craft-common-actions.git|d7bf8f06e138618890d0ebe8a84a18b010bf3f66"
  "craft-fonts|https://github.com/storytold/craft-fonts.git|d1cf47d05e04426615fbda1d637b9ca6b1e7af35"
  "libcdr|https://github.com/LibreOffice/libcdr.git|22c396810e72d40b94f78c192eb06e6d09bc34a0"
  "cdr-spec|https://github.com/photopea/CDR-specification.git|f91b3c7e8c812b28473f6399e95a624a72a3d6b8"
  "uniconvertor|https://github.com/sk1project/uniconvertor.git|973d5b6f8fccce7e7bf9bc88e91bc80f9f9d9472"
  "rea|https://github.com/cyberkaida/reverse-engineering-assistant.git|a05a6b0a18649efd6d74ec8447bdadf8d468c365"
  "morluto-rea|https://github.com/morluto/rea.git|ba866a26ecbe179a717e746cbc0d1605ae616ad5"
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
