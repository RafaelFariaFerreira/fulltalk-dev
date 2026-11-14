#!/usr/bin/env bash
# Regenerates docs/screenshots/*.png and docs/hero.png from the real app.
# Uses a throw-away FULLTALK_HOME seeded with sample history.
set -euo pipefail
cd "$(dirname "$0")/.."

demo="$(mktemp -d)"
trap 'rm -rf "$demo"' EXIT
cat > "$demo/history.json" <<'JSON'
{"entries":[
{"text":"Beleza, vou enviar o relatório ainda hoje, assim que fechar os números do trimestre.","created_at":0},
{"text":"A reunião ficou agendada para as 15h.","created_at":0},
{"text":"Pode me mandar o link do repositório, por favor?","created_at":0},
{"text":"git commit -m \"feat: adiciona transcrição local em pt-BR\"","created_at":0},
{"text":"Vou dar uma olhada e te retorno.","created_at":0}
]}
JSON

cargo build --release -p fulltalk --features screenshot
for scene in main recording toast settings; do
  FULLTALK_LOG=error FULLTALK_HOME="$demo" FULLTALK_SCREENSHOT="docs/screenshots/$scene.png" \
    FULLTALK_SCREENSHOT_SCENE="$scene" ./target/release/fulltalk
done
python3 scripts/make-hero.py
