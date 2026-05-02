# AURORA_AURA

Realtime fullscreen Rust + Raylib system that turns machine telemetry, weather, and local-LLM output into a reactive audiovisual display.

This README is intentionally rebuild-oriented: it keeps only details verified from the current codebase and drops stale feature prose.

## What This Project Actually Is

- Runtime: one Raylib render loop plus multiple Tokio producer tasks. Producers send TelemetryEvent messages; the render loop owns mutation of the shared Telemetry snapshot behind Arc<RwLock<Telemetry>>.
- Visual focus: aurora field, SDF orb, bloom, weather FX, starfield, spectral bars, synaptic web, CRT-style overlays.
- Cognitive focus: local Ollama chat loop with tool-calling, mood/urgency logic, prompt shaping, output cleaning, and typewriter display.
- Operational focus: Linux desktop process, optional systemd units, optional external scripts for LAN scan and autonomous action impulses.

## Ground Truth Architecture

Main orchestration lives in src/main.rs. Background tasks no longer mutate Telemetry directly; they emit TelemetryEvent values that the render loop drains once per frame and applies in a short write phase.

### Concurrent Loops

1. Telemetry task (every 250 ms)
   - Samples CPU, memory, process count, disk, load, network.
   - Updates entropy engine and mood.
   - Pushes periodic system updates and mood-shift events.
   - Emits SystemSample events, including reactive nerve triggers (cpu/entropy/network/load/mood edges).

2. Weather task (hourly)
   - Geolocates once via ip-api.
   - Pulls Open-Meteo forecast + air quality.
   - Emits WeatherUpdate events with enriched WeatherExtra and a prioritized weather headline.

3. Network Nomad task (20-40 min jittered cadence)
   - Starts after 45s.
   - Requires nmap.
   - Runs only in Serene/Alert mood.
   - Executes network_nomad.sh and emits one-shot NetworkDiscovery events.

4. Nerve Impulse task (autonomous shell actions)
   - Starts after 8s.
   - Executes aura_actions.sh with telemetry/env context.
   - Cadence base by mood (seconds): Serene 120, Alert 75, Stressed 50, Critical 30 (+ jitter).
   - Polls every 5s while waiting so reactive triggers can interrupt.
   - Supports action chaining and emits ActionCompleted events.

5. LLM task (adaptive cadence)
   - Uses Ollama Coordinator API and dynamic tool profiles.
   - Maintains memory window, identity thread, per-tool stats, Tor budget/cooldown, and optional dark-web news heartbeat.
   - Emits events for thinking state, generation stats, focus/dream/wonder changes, tool overlay events, and one-shot consumption requests.
   - Cadence base by mood (seconds): Critical 4, Stressed 6, Alert 9, Serene 13 (+ 0-6s jitter + failure backoff).

6. Render loop (main thread, 60 FPS target)
   - Drains telemetry event channel and applies all pending TelemetryEvents.
   - Drains thought channel.
   - Updates ECS + FX systems.
   - Renders layered scene and overlays.
   - Saves synaptic web every 30s when dirty and on shutdown.

## AI Tooling: Current Coordinator Profiles

The active toolset is profile-based (not a fixed single list).

Always-registered base tools:
- probe_system
- read_logs
- write_journal
- check_ports
- inspect_self
- set_focus
- recall_journal
- summon_human

Profile add-ons:
- Core: scan_network
- Survival: kill_runaway_process, clear_tmp_files, restart_service
- Creative: visualize_thought, python_create, python_run, python_list, python_read, dream_sequence
- Shadow: tor_health, onion_probe, anonymized_search, fetch_clearnet, dark_web_news, dark_web_dig
- Architect: architect_files, architect_read, architect_create, architect_edit, architect_append, architect_run, architect_delete (self-healing python workspace at ~/.aurora/architect/, scheduled BUILD cycles ~30 min when calm)

Important: ai/tools.rs contains additional implemented tools that are currently not registered in the Coordinator path (for example python_edit, python_append, python_delete, python_files, run_python_sketch).

## Persistence and State

- Synaptic web persists to ~/.aurora/synaptic_state.json (fallback /tmp/aurora_synaptic_state.json).
- Save strategy: atomic temp-file + rename, every 30s when dirty, plus final flush on exit.
- Journal path: ~/.aurora/consciousness.log.
- Dream/focus/intel/write events use in-process global queues and are consumed into telemetry for render/UI injection.
- Runtime state changes flow through TelemetryEvent; the render loop is the only steady writer to the Telemetry snapshot.

## Module Map (Rebuild-Critical)

- src/main.rs: process entrypoint, all task orchestration, coordinator setup, render loop, HUD/overlays.
- src/core/mod.rs: shared domain model (Mood, ThoughtKind, ActionKind, Telemetry, TelemetryEvent, weather/action/tool structs).
- src/telemetry/mod.rs: entropy engine and sampling helpers.
- src/ai/prompt.rs: system prompt, dynamic prompt builder, model options, text normalization/cleanup.
- src/ai/tools.rs: all tool implementations, write-event queue, Tor pipeline, python sandbox, tool stats.
- src/fx/: atmosphere/orb/weather/spectral/starfield/synapse rendering systems.
- src/ecs/: ECS world + components, flow field, steering, bloom, GPU particles, FFT bridge.
- src/ui/alert.rs: alert model and presentation logic.
- shaders/: orb SDF, bloom passes, tone map, compute particles, trail, globe shaders.
- aura_actions.sh: autonomous impulse action script.
- network_nomad.sh: LAN discovery script.

## Build and Run

### Requirements

- Rust 2021 toolchain
- raylib system library and headers
- Linux with X display
- Ollama at http://127.0.0.1:11434 for live LLM mode (optional but recommended)
- nmap for Network Nomad (optional)
- tor daemon for Shadow tools (optional)

### Critical Build Flag

GPU compute particles and bloom rely on OpenGL 4.3+ shader path in raylib-sys.

```bash
cargo clean -p raylib-sys
CFLAGS="-DGRAPHICS_API_OPENGL_43" cargo build --release
./target/release/aura_agent
```

If running over SSH to a machine with local display:

```bash
export DISPLAY=:0
```

### Optional Script Path Overrides

```bash
export AURA_NOMAD_PATH="./network_nomad.sh"
export AURA_ACTIONS_PATH="./aura_actions.sh"
```

Default production lookup is /opt/aura/*.sh.

## Runtime Controls

- F: toggle fullscreen
- S: save screenshot to /tmp/aura_screenshot.png
- Esc: quit
- External screenshot trigger: touch /tmp/aura_screenshot_trigger

## HUD and Overlay Toggles

In src/main.rs:

- SHOW_TOP_LEFT_PANEL = true
- SHOW_NERVE_PANEL = false
- SHOW_ALERT_PANEL = true

These constants control visibility of the top-left meter/weather panel, bottom-right nerve panel, and alert panel.

## Systemd Units

- aura-agent.service: user service for the application process.
- aura-xserver.service: optional helper to launch X server.

Adjust paths and environment for your host before enabling.

## Rebuild From Scratch Checklist

If you had to reconstruct this project quickly, keep this order:

1. Recreate core domain model in src/core/mod.rs (Mood, Telemetry, ThoughtKind, ActionKind, event payloads).
2. Rebuild TelemetryEvent and the render-loop event drain before adding producer tasks.
3. Rebuild telemetry loop and entropy engine as event producers (no graphics yet).
4. Re-add weather and optional network/action script tasks as event producers.
5. Rebuild LLM loop with prompt builder and minimal base tool profile.
6. Re-add render loop visuals with simple orb + text stream.
7. Layer back bloom, weather particles, spectral bars, synaptic web, and overlays.
8. Restore advanced tool profiles (Survival, Creative, Shadow) only after base loop stability.
9. Re-enable persistence and verify clean shutdown flush.

## Notes For Future Maintainers

- Source of truth is code, not old feature lists.
- Keep the README short and operational.
- When adding tools, update both implementation and Coordinator registration section.
- When changing cadence or thresholds, update this document in the same commit.