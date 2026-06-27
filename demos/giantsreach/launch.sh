#!/bin/sh
# Giantsreach launcher. Starts the dependency-free Node server and opens the game.
cd "$(dirname "$0")"
PORT="${PORT:-8787}"
export PORT
echo "============================================================"
echo "  GIANTSREACH"
echo "  Raise your hold among the fallen giants."
echo "------------------------------------------------------------"
echo "  Server:  http://localhost:$PORT"
echo "  (Ctrl-C to stop)"
echo "============================================================"
# open the browser shortly after the server comes up (macOS/Linux)
( sleep 1.2; (command -v open >/dev/null 2>&1 && open "http://localhost:$PORT") || (command -v xdg-open >/dev/null 2>&1 && xdg-open "http://localhost:$PORT") ) >/dev/null 2>&1 &
exec node server/server.js
