#!/bin/bash
# 自动更新：拉取 GitHub 最新代码；server.js 有变动才重启应用（前端/文档免重启生效）
cd "$(dirname "$0")" || exit 1
export PATH="/home/lixiang/.nvm/versions/node/v24.18.0/bin:$PATH"
GIT() { git -c safe.directory='*' "$@"; }

GIT fetch origin main -q || { echo "[$(date '+%F %T')] fetch 失败（网络/仓库不可达）"; exit 0; }
LOCAL=$(GIT rev-parse HEAD 2>/dev/null || echo none)
REMOTE=$(GIT rev-parse origin/main 2>/dev/null)
[ "$LOCAL" = "$REMOTE" ] && exit 0

CHANGED=$(GIT diff --name-only "$LOCAL" "$REMOTE")
echo "[$(date '+%F %T')] 发现新版本（变更：$(echo "$CHANGED" | tr '\n' ' ')）"
GIT pull --ff-only origin main -q || { echo "pull 失败（本地有冲突改动？）"; exit 1; }
if echo "$CHANGED" | grep -q '^server\.js'; then
  ./start-server.sh
  echo "[$(date '+%F %T')] server.js 有变动，已重启应用"
else
  echo "[$(date '+%F %T')] 仅前端/文档变动，免重启生效"
fi
