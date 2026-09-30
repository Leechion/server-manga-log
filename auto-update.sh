#!/bin/bash
# 自动更新：拉取 GitHub 最新代码；server.js 有变动才重启应用（前端/文档免重启生效）
# Node 查找顺序：PATH → nvm 最新版（本脚本已通用化，可随仓库分发到任意部署目录）
cd "$(dirname "$0")" || exit 1
NODE_BIN="$(command -v node || true)"
if [ -z "$NODE_BIN" ]; then
  NODE_BIN="$(ls "$HOME"/.nvm/versions/node/*/bin/node 2>/dev/null | sort -V | tail -1)"
fi
if [ -z "$NODE_BIN" ]; then
  echo "[x] 找不到 node（PATH 与 ~/.nvm 里都没有）"
  exit 1
fi
export PATH="$(dirname "$NODE_BIN"):$PATH"
GIT() { git -c safe.directory='*' "$@"; }

GIT fetch origin main -q || { echo "[$(date '+%F %T')] fetch 失败（网络/仓库不可达）"; exit 0; }
LOCAL=$(GIT rev-parse HEAD 2>/dev/null || echo none)
REMOTE=$(GIT rev-parse origin/main 2>/dev/null)
[ "$LOCAL" = "$REMOTE" ] && exit 0

CHANGED=$(GIT diff --name-only "$LOCAL" "$REMOTE")
echo "[$(date '+%F %T')] 发现新版本（变更：$(echo "$CHANGED" | tr '\n' ' ')）"
GIT pull --ff-only origin main -q || { echo "pull 失败（本地有冲突改动？）"; exit 1; }
if echo "$CHANGED" | grep -q '^server\.js'; then
  # 重启：优先用仓库里的 start-server.sh，没有就退回直接起 node（无需额外文件）
  if [ -x ./start-server.sh ]; then
    ./start-server.sh
  else
    pkill -f "node .*server\.js" 2>/dev/null
    sleep 1
    nohup "$NODE_BIN" server.js > server.log 2>&1 &
  fi
  echo "[$(date '+%F %T')] server.js 有变动，已重启应用"
else
  echo "[$(date '+%F %T')] 仅前端/文档变动，免重启生效"
fi
