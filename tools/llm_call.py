#!/usr/bin/env python3
"""Tiny CLI wrapper around a local Ollama server with a graceful fallback.

Used by ad-hoc tooling and (optionally) by the visualizer pipeline. The
Rust `aura_agent` does NOT depend on this script — it talks to Ollama
directly via the `ollama_rs` crate.
"""
import argparse
import os
import subprocess
import sys
import time
from pathlib import Path

OLLAMA_HOST = os.environ.get("OLLAMA_HOST", "http://127.0.0.1:11434").rstrip("/")
SCRIPT_DIR = Path(__file__).resolve().parent


def _discover_model(requests_mod, timeout=2):
    """Return the first locally-installed Ollama model, or None."""
    try:
        r = requests_mod.get(f"{OLLAMA_HOST}/api/tags", timeout=timeout)
        if r.status_code != 200:
            return None
        models = r.json().get("models") or []
        names = [m.get("name") for m in models if m.get("name")]
        for preferred in ("llama3.2:latest", "llama3.2", "llama3:latest", "llama3"):
            if preferred in names:
                return preferred
        return names[0] if names else None
    except Exception:
        return None


def try_ollama(prompt, model=None, timeout=30):
    """Call a local Ollama server. Returns the generated text or None."""
    try:
        import requests
    except Exception:
        return None

    if not model:
        model = os.environ.get("OLLAMA_MODEL") or _discover_model(requests)
        if not model:
            return None

    # 1) Native Ollama generate API. `stream: False` is critical — otherwise
    #    the body is newline-delimited JSON and resp.json() will raise.
    try:
        r = requests.post(
            f"{OLLAMA_HOST}/api/generate",
            json={"model": model, "prompt": prompt, "stream": False},
            timeout=timeout,
        )
        if r.status_code == 200:
            data = r.json()
            if isinstance(data, dict):
                # Ollama uses `response`; keep other keys as a courtesy.
                for key in ("response", "text", "content", "output", "result"):
                    val = data.get(key)
                    if isinstance(val, str) and val:
                        return val
    except Exception:
        pass

    # 2) OpenAI-compatible chat endpoint as a fallback (newer Ollama builds).
    try:
        r = requests.post(
            f"{OLLAMA_HOST}/v1/chat/completions",
            json={
                "model": model,
                "messages": [{"role": "user", "content": prompt}],
                "stream": False,
            },
            timeout=timeout,
        )
        if r.status_code == 200:
            data = r.json()
            choices = data.get("choices") or []
            if choices:
                msg = choices[0].get("message") or {}
                content = msg.get("content")
                if isinstance(content, str) and content:
                    return content
    except Exception:
        pass

    return None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("prompt", nargs="?", default="Hello from LLM caller")
    parser.add_argument("--model", default=None)
    parser.add_argument("--visualize", action="store_true")
    parser.add_argument("--mood", default="Serene")
    parser.add_argument("--duration", type=float, default=3.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()

    text = try_ollama(args.prompt, model=args.model, timeout=args.timeout)
    if not text:
        text = f"[LOCAL-FALLBACK] {args.prompt} — {time.strftime('%Y-%m-%d %H:%M:%S')}"

    print(text)

    if args.visualize:
        visualizer = SCRIPT_DIR / "visualize.py"
        cmd = [
            sys.executable, str(visualizer),
            "--text", text,
            "--mood", args.mood,
            "--duration", str(args.duration),
        ]
        subprocess.run(cmd, check=False)


