#!/bin/bash
# voice-lens-runner — one process per debate lens (poll by counting these, per debate-runner.md)
LENS="$1"
D=/Users/ronitnath/dev/portfolio/products/voice/debates/DEBATE-001-lenses
cd /Users/ronitnath/dev/portfolio
/Users/ronitnath/dev/pi-home/bin/pi-with-nexus-env -p \
  --provider openai-codex --model gpt-5.6-sol \
  "$(cat $D/prompts/$LENS.md)"
echo "voice-lens-runner done: $LENS rc=$?"
