#!/usr/bin/env python3
import argparse
import math
import pygame
import time


def clamp(v, lo, hi):
    return max(lo, min(hi, v))


def compact_text(text, max_words=8, max_chars=64):
    """Retained for back-compat; the viz no longer renders the caption,
    but other callers still import this helper."""
    words = text.strip().split()
    if not words:
        return ""
    compact = " ".join(words[:max_words])
    if len(compact) > max_chars:
        return compact[:max(0, max_chars - 3)].rstrip() + "..."
    return compact


def draw_gradient(screen, width, height, top, bottom):
    for y in range(height):
        t = y / max(1, height - 1)
        r = int(top[0] + (bottom[0] - top[0]) * t)
        g = int(top[1] + (bottom[1] - top[1]) * t)
        b = int(top[2] + (bottom[2] - top[2]) * t)
        pygame.draw.line(screen, (r, g, b), (0, y), (width, y))


# Backward-compat aliases for renamed presets.
PRESET_ALIASES = {
    "ribbons": "signal_drift",
    "rose": "azimuth",
}


def _canonical_preset(name):
    return PRESET_ALIASES.get(name, name)


def choose_preset(preset, mood, text):
    allowed = ["orbit", "signal_drift", "pulse", "constellation",
               "spiral", "fractal", "lissajous", "azimuth"]
    preset = _canonical_preset(preset)
    if preset in allowed:
        return preset
    mood_default = {
        "Serene": "signal_drift",
        "Alert": "orbit",
        "Stressed": "pulse",
        "Critical": "constellation",
    }
    if preset == "auto":
        base = mood_default.get(mood, "orbit")
        # Deterministic variation from text hash so the window feels alive.
        h = sum(ord(c) for c in text) % len(allowed)
        return allowed[h] if text.strip() else base
    return mood_default.get(mood, "orbit")


def draw_orbit(screen, t, width, height, color):
    # Centered drawing -- the window is now a mood canvas, no caption box.
    cx = int(width * 0.50)
    cy = int(height * 0.55)
    ring = int(min(width, height) * 0.32)
    pulse = 1.0 + 0.08 * math.sin(t * 2.2)
    orb_r = int(min(width, height) * 0.10 * pulse)

    overlay = pygame.Surface((width, height), pygame.SRCALPHA)
    pygame.draw.circle(overlay, (color[0], color[1], color[2], 42), (cx, cy), int(orb_r * 2.4))
    screen.blit(overlay, (0, 0))

    pygame.draw.circle(screen, color, (cx, cy), orb_r)
    pygame.draw.circle(screen, (240, 245, 255), (cx - int(orb_r * 0.28), cy - int(orb_r * 0.32)), max(3, int(orb_r * 0.18)))
    sat_x = cx + int(math.cos(t * 1.8) * ring)
    sat_y = cy + int(math.sin(t * 1.8) * ring * 0.7)
    pygame.draw.circle(screen, (220, 230, 255), (sat_x, sat_y), 5)


def draw_signal_drift(screen, t, width, height, color):
    """Jagged oscilloscope-style waveform — fBm-ish high-frequency drift."""
    overlay = pygame.Surface((width, height), pygame.SRCALPHA)

    # Three stacked traces at different vertical bands and frequencies.
    bands = (
        (0.30, 1.0, 0.35, 220),
        (0.52, 1.7, 0.22, 170),
        (0.74, 2.6, 0.14, 130),
    )
    span_x0 = int(width * 0.06)
    span_x1 = int(width * 0.96)
    step = 4  # px between samples — keeps line count bounded

    for band_y, freq, amp_scale, alpha in bands:
        cy = int(height * band_y)
        amp = int(min(width, height) * amp_scale * 0.5)
        col = (
            clamp(color[0], 0, 255),
            clamp(color[1] + 10, 0, 255),
            clamp(color[2] + 30, 0, 255),
            alpha,
        )
        prev = None
        x = span_x0
        while x <= span_x1:
            u = (x - span_x0) / max(1, span_x1 - span_x0)
            # Layered sines = cheap pseudo-fBm; high-freq term gives the jag.
            v = (
                math.sin(u * 9.0 * freq + t * 1.6)
                + 0.55 * math.sin(u * 21.0 * freq - t * 2.3)
                + 0.30 * math.sin(u * 47.0 * freq + t * 4.1)
            )
            y = cy + int(v * amp * 0.35)
            if prev is not None:
                pygame.draw.line(overlay, col, prev, (x, y), 1)
            prev = (x, y)
            x += step

    # Sparse vertical "sample tick" glints — telemetry feel.
    tick_alpha = 90
    for i in range(8):
        tx = span_x0 + int((i + (t * 0.15) % 1.0) / 8.0 * (span_x1 - span_x0))
        pygame.draw.line(overlay, (color[0], color[1], color[2], tick_alpha),
                         (tx, int(height * 0.22)), (tx, int(height * 0.82)), 1)

    screen.blit(overlay, (0, 0))


def draw_pulse(screen, t, width, height, color):
    cx = int(width * 0.50)
    cy = int(height * 0.55)
    max_r = int(min(width, height) * 0.45)
    overlay = pygame.Surface((width, height), pygame.SRCALPHA)
    for i in range(5):
        phase = (t * 0.8 + i * 0.20) % 1.0
        r = int(20 + phase * max_r)
        a = int((1.0 - phase) * 110)
        pygame.draw.circle(overlay, (color[0], color[1], color[2], a), (cx, cy), r)
    screen.blit(overlay, (0, 0))
    pygame.draw.circle(screen, color, (cx, cy), max(14, int(min(width, height) * 0.05)))


def draw_constellation(screen, t, width, height, color):
    nodes = []
    # Spread the constellation across the whole canvas now that there is
    # no caption box on the left.
    for i in range(10):
        px = int(width * (0.12 + (i % 5) * 0.18))
        py = int(height * (0.28 + (i // 5) * 0.36) + math.sin(t * 1.3 + i) * 8)
        nodes.append((px, py))
    overlay = pygame.Surface((width, height), pygame.SRCALPHA)
    for x, y in nodes:
        pygame.draw.circle(overlay, (90, 120, 170, 42), (x, y), 9)
    screen.blit(overlay, (0, 0))
    for i, (x, y) in enumerate(nodes):
        glow = int(2 + 1.5 * (1.0 + math.sin(t * 2.1 + i)))
        pygame.draw.circle(screen, (220, 230, 255), (x, y), glow)
        pygame.draw.circle(screen, color, (x, y), 2)


# ────────────────────────────────────────────────────────────────────
#  Turtle-style line-art presets
# ────────────────────────────────────────────────────────────────────
#  These mimic the python `turtle` module's vibe (a pen walking and
#  tracing geometric figures) but render through pygame so they coexist
#  with the rest of the HUD. Each preset draws a curve incrementally
#  (turtle "walks" forward over time, leaving a fading trail) and
#  parameters are seeded from the text so distinct messages produce
#  visually distinct figures.
# ────────────────────────────────────────────────────────────────────

def _turtle_seed(text):
    """Stable per-message integer seed."""
    return (sum((i + 1) * ord(c) for i, c in enumerate(text)) or 1) & 0xFFFFFFFF


def _trail_alpha(width, height):
    """Reusable RGBA surface for fading trails."""
    return pygame.Surface((width, height), pygame.SRCALPHA)


def _fade_trail(trail, fade=242):
    """Multiplicatively darken the persistent trail surface in place."""
    trail.fill((255, 255, 255, fade), special_flags=pygame.BLEND_RGBA_MULT)


def draw_spiral(screen, t, width, height, color, text="", trail=None, state=None):
    """Logarithmic spiral — only the newest segment is rasterised per frame."""
    cx = int(width * 0.50)
    cy = int(height * 0.55)
    seed = _turtle_seed(text)
    arms = 1 + (seed % 3)
    growth = 0.045 + (seed % 7) * 0.005

    _fade_trail(trail, 244)

    # Advance angular extent in lock-step with frame time.
    theta = min(t * 4.0, 28.0)
    prev_theta = state.get("theta", 0.0)
    # Sub-step so fast advances still produce smooth lines.
    sub = max(1, int((theta - prev_theta) * 20))
    for arm in range(arms):
        offset = arm * (2 * math.pi / arms)
        for s in range(sub):
            th0 = prev_theta + (theta - prev_theta) * (s / sub) + offset
            th1 = prev_theta + (theta - prev_theta) * ((s + 1) / sub) + offset
            r0 = 8 + math.exp(growth * (th0 - offset)) * 1.4
            r1 = 8 + math.exp(growth * (th1 - offset)) * 1.4
            p0 = (cx + r0 * math.cos(th0), cy + r0 * math.sin(th0))
            p1 = (cx + r1 * math.cos(th1), cy + r1 * math.sin(th1))
            pygame.draw.aaline(trail, (*color, 230), p0, p1)
        # Pen tip glint at leading end.
        rL = 8 + math.exp(growth * theta) * 1.4
        pL = (cx + rL * math.cos(theta + offset), cy + rL * math.sin(theta + offset))
        pygame.draw.circle(trail, (240, 245, 255, 230),
                           (int(pL[0]), int(pL[1])), 3)
    state["theta"] = theta
    screen.blit(trail, (0, 0))


def draw_fractal(screen, t, width, height, color, text=""):
    """Animated Koch-snowflake-ish recursion. Depth grows then resets."""
    cx, cy = int(width * 0.50), int(height * 0.55)
    size = int(min(width, height) * 0.40)

    seed = _turtle_seed(text)
    sides = 3 + (seed % 4)              # 3..6 base polygon
    # Depth grows over time, loops every ~6s
    cycle = (t * 0.6) % (4.0 + (seed % 3))
    depth = int(cycle)
    morph = cycle - depth               # 0..1 morphing the next level in

    def koch(p1, p2, level, out):
        if level == 0:
            out.append((p1, p2))
            return
        x1, y1 = p1
        x2, y2 = p2
        dx, dy = (x2 - x1) / 3.0, (y2 - y1) / 3.0
        a = (x1 + dx, y1 + dy)
        b = (x1 + 2 * dx, y1 + 2 * dy)
        # Bump apex — morph factor blends straight↔full bump for smooth growth
        ang = math.atan2(dy, dx) - math.pi / 3.0 * morph
        length = math.hypot(dx, dy)
        peak = (a[0] + length * math.cos(ang),
                a[1] + length * math.sin(ang))
        koch(p1, a, level - 1, out)
        koch(a, peak, level - 1, out)
        koch(peak, b, level - 1, out)
        koch(b, p2, level - 1, out)

    # Build base polygon vertices
    verts = []
    for i in range(sides):
        ang = -math.pi / 2.0 + i * (2 * math.pi / sides) + t * 0.15
        verts.append((cx + size * math.cos(ang), cy + size * math.sin(ang)))

    segs = []
    for i in range(sides):
        koch(verts[i], verts[(i + 1) % sides], min(depth, 3), segs)

    overlay = pygame.Surface((width, height), pygame.SRCALPHA)
    for p1, p2 in segs:
        pygame.draw.aaline(overlay, (*color, 200), p1, p2)
    screen.blit(overlay, (0, 0))
    # Vertex glints
    for x, y in verts:
        pygame.draw.circle(screen, (240, 245, 255), (int(x), int(y)), 2)


def draw_lissajous(screen, t, width, height, color, text="", trail=None, state=None):
    """Lissajous curve — only the new segment since last frame is drawn."""
    cx, cy = int(width * 0.50), int(height * 0.55)
    rx = int(min(width, height) * 0.40)
    ry = int(min(width, height) * 0.36)
    seed = _turtle_seed(text)
    a = 2 + (seed % 5)
    b = 3 + ((seed >> 3) % 5)
    delta = (seed % 360) * math.pi / 180.0

    _fade_trail(trail, 240)

    def sample(tt):
        d = delta + tt * 0.3
        u = (tt * 60.0 / 480) * 2 * math.pi  # head walks the loop
        return (cx + rx * math.sin(a * u + d),
                cy + ry * math.sin(b * u))

    prev_t = state.get("t", t)
    # Cap sub-steps to avoid catch-up storms after a stall.
    sub = max(1, min(120, int((t - prev_t) * 240)))
    for s in range(sub):
        ta = prev_t + (t - prev_t) * (s / sub)
        tb = prev_t + (t - prev_t) * ((s + 1) / sub)
        pygame.draw.aaline(trail, (*color, 220), sample(ta), sample(tb))
    head = sample(t)
    pygame.draw.circle(trail, (240, 245, 255, 240),
                       (int(head[0]), int(head[1])), 4)
    state["t"] = t
    screen.blit(trail, (0, 0))


def draw_azimuth(screen, t, width, height, color, text="", trail=None, state=None):
    """Polar radar projection (rhodonea-driven). High-contrast green/amber."""
    cx, cy = int(width * 0.50), int(height * 0.55)
    R = int(min(width, height) * 0.42)
    seed = _turtle_seed(text)
    n = 2 + (seed % 7)
    d = 1 + ((seed >> 4) % 3)
    k = n / d

    # Radar palette overrides mood color: high-contrast on near-black.
    sweep_col = (60, 230, 120)   # phosphor green
    target_col = (255, 190, 60)  # amber blip

    _fade_trail(trail, 232)

    # Concentric range rings + crosshairs are drawn directly (cheap, redrawn each frame).
    for ring in (0.33, 0.66, 1.0):
        pygame.draw.circle(screen, (20, 60, 35), (cx, cy), int(R * ring), 1)
    pygame.draw.line(screen, (20, 60, 35), (cx - R, cy), (cx + R, cy), 1)
    pygame.draw.line(screen, (20, 60, 35), (cx, cy - R), (cx, cy + R), 1)

    # Sweeping radar arm.
    sweep_ang = (t * 1.4) % (2 * math.pi)
    arm_end = (cx + R * math.cos(sweep_ang), cy + R * math.sin(sweep_ang))
    pygame.draw.aaline(trail, (*sweep_col, 200), (cx, cy), arm_end)

    # Rhodonea "target" trace — incremental head movement.
    prev_theta = state.get("theta", 0.0)
    theta = t * 1.6
    sub = max(1, min(80, int((theta - prev_theta) * 60)))
    for s in range(sub):
        ta = prev_theta + (theta - prev_theta) * (s / sub)
        tb = prev_theta + (theta - prev_theta) * ((s + 1) / sub)
        ra = R * math.cos(k * ta)
        rb = R * math.cos(k * tb)
        pa = (cx + ra * math.cos(ta + t * 0.2),
              cy + ra * math.sin(ta + t * 0.2))
        pb = (cx + rb * math.cos(tb + t * 0.2),
              cy + rb * math.sin(tb + t * 0.2))
        pygame.draw.aaline(trail, (*target_col, 230), pa, pb)
    state["theta"] = theta

    screen.blit(trail, (0, 0))
    # Bright blip at the head.
    rH = R * math.cos(k * theta)
    head = (int(cx + rH * math.cos(theta + t * 0.2)),
            int(cy + rH * math.sin(theta + t * 0.2)))
    pygame.draw.circle(screen, (255, 230, 140), head, 3)


# ────────────────────────────────────────────────────────────────────
#  Living-system layers
# ────────────────────────────────────────────────────────────────────
#  Cross-cutting effects that run on TOP of every preset to make the
#  window feel like a single breathing organism rather than a static
#  demo. Each helper is cheap and most of its work is precomputed
#  once at startup.
# ────────────────────────────────────────────────────────────────────

# Mood palettes — primary drives the drawing, accent tints the dust
# motes and preset tag, bg_top/bg_bot frame the canvas.
MOOD_PALETTES = {
    'Serene': {
        'primary':  (60, 180, 200),
        'accent':   (140, 210, 230),
        'bg_top':   (10, 18, 28),
        'bg_bot':   (5, 9, 16),
    },
    'Alert': {
        'primary':  (255, 180, 60),
        'accent':   (255, 215, 130),
        'bg_top':   (24, 18, 12),
        'bg_bot':   (12, 9, 6),
    },
    'Stressed': {
        'primary':  (255, 140, 60),
        'accent':   (255, 170, 110),
        'bg_top':   (28, 14, 12),
        'bg_bot':   (14, 7, 6),
    },
    'Critical': {
        'primary':  (220, 60, 60),
        'accent':   (255, 110, 110),
        'bg_top':   (30, 8, 10),
        'bg_bot':   (14, 4, 5),
    },
}


def _spawn_motes(width, height, count, seed):
    """Deterministic ambient dust — drifts on a low-freq sine field."""
    motes = []
    s = seed or 1
    for i in range(count):
        # Cheap LCG so we don't pull in `random` and stay deterministic
        # per-message — same seed → same starfield.
        s = (s * 1103515245 + 12345) & 0x7FFFFFFF
        x = (s % 10000) / 10000.0
        s = (s * 1103515245 + 12345) & 0x7FFFFFFF
        y = (s % 10000) / 10000.0
        s = (s * 1103515245 + 12345) & 0x7FFFFFFF
        phase = (s % 10000) / 10000.0 * 2 * math.pi
        s = (s * 1103515245 + 12345) & 0x7FFFFFFF
        speed = 0.05 + (s % 100) / 100.0 * 0.20
        s = (s * 1103515245 + 12345) & 0x7FFFFFFF
        size = 1 + (s % 3)
        motes.append({
            'x0': x * width,
            'y0': y * height,
            'phase': phase,
            'speed': speed,
            'size': size,
        })
    return motes


def _draw_motes(screen, motes, t, width, height, accent, breath):
    """Render dust motes as a single SRCALPHA overlay (one blit)."""
    overlay = pygame.Surface((width, height), pygame.SRCALPHA)
    # Brightness oscillates with respiration — the air "inhales" with the system.
    base_alpha = int(40 + 50 * breath)
    for m in motes:
        # Slow drift along a sine field; wraps softly off the edges.
        dx = math.sin(t * m['speed'] + m['phase']) * 24
        dy = math.cos(t * m['speed'] * 0.7 + m['phase'] * 1.3) * 18
        x = (m['x0'] + dx) % width
        y = (m['y0'] + dy) % height
        # Per-mote twinkle so the field never looks uniform.
        twinkle = 0.55 + 0.45 * math.sin(t * 1.7 + m['phase'] * 2.3)
        a = int(base_alpha * twinkle)
        pygame.draw.circle(overlay, (*accent, a), (int(x), int(y)), m['size'])
    screen.blit(overlay, (0, 0))


def _build_vignette(width, height, strength=110):
    """Pre-rendered radial darkening — frames the canvas without per-frame cost.

    Built as concentric ellipses with growing alpha toward the corners.
    Cheap and only allocated once.
    """
    surf = pygame.Surface((width, height), pygame.SRCALPHA)
    cx, cy = width // 2, height // 2
    max_r = math.hypot(cx, cy)
    # Stepped ellipses — coarse but invisible at typical window sizes.
    steps = 24
    for i in range(steps, 0, -1):
        f = i / steps                       # 1.0 at edge → 0 at center
        r = int(max_r * f)
        # Quadratic falloff: most darkening in the outer 30%.
        a = int(strength * (f ** 2.2))
        pygame.draw.ellipse(
            surf, (0, 0, 0, a),
            (cx - r, cy - int(r * (height / max(1, width))),
             r * 2, int(r * 2 * (height / max(1, width)))),
        )
    return surf


def _build_grain_tile(size, alpha=14):
    """Tiny film-grain tile — tiled across the screen and shifted per frame.

    Built deterministically (no `random` import) so the tile is stable
    but the per-frame offset gives it a shimmering, organic feel.
    """
    tile = pygame.Surface((size, size), pygame.SRCALPHA)
    s = 0xC0FFEE
    for y in range(size):
        for x in range(size):
            s = (s * 1664525 + 1013904223) & 0xFFFFFFFF
            # Sparse: only ~12% of pixels carry grain, otherwise transparent.
            if (s & 0xFF) < 32:
                v = (s >> 8) & 0xFF
                # Mix of darken and lighten flecks for filmic feel.
                if v < 128:
                    tile.set_at((x, y), (0, 0, 0, alpha))
                else:
                    tile.set_at((x, y), (255, 255, 255, alpha // 2))
    return tile


def run_visual(text, mood, duration, width, height, fps, preset):
    pygame.init()
    screen = pygame.display.set_mode((width, height))
    pygame.display.set_caption(f'AURA Mood — {mood}')
    clock = pygame.time.Clock()
    # Font fallback stack — SysFont accepts a comma-separated list and
    # picks the first installed family, mirroring CSS font-family fallback.
    mono_stack = 'consolas,dejavusansmono,liberationmono,menlo,monaco,monospace'
    # Refined typography: slightly smaller header so the drawing breathes.
    font = pygame.font.SysFont(mono_stack, 32, bold=True)
    font_sm = pygame.font.SysFont(mono_stack, 13)

    palette = MOOD_PALETTES.get(mood, MOOD_PALETTES['Serene'])
    color = palette['primary']
    preset = choose_preset(preset, mood, text)

    # Persistent trail surface for incremental "turtle" presets so we
    # never recompute the whole path each frame. Faded multiplicatively
    # by each draw routine.
    trail_surface = pygame.Surface((width, height), pygame.SRCALPHA)
    trail_state = {}

    # ── Living-system layers ──────────────────────────────────────
    # Drifting ambient dust motes — give the canvas a sense of air.
    motes = _spawn_motes(width, height, count=42, seed=_turtle_seed(text or mood))
    # Pre-rendered radial vignette darkens the corners ever so slightly,
    # framing the drawing without adding per-frame cost.
    vignette = _build_vignette(width, height, strength=110)
    # Pre-rendered film grain tile, tiled and shifted each frame.
    grain_tile = _build_grain_tile(96, alpha=14)

    first_frame = time.time()

    running = True
    while running:
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                running = False

        now = time.time()
        t = now - first_frame
        # Slow respiration — single source of truth for "aliveness".
        # 6s in, 6s out → period 12s. Returned in [0, 1].
        breath = 0.5 + 0.5 * math.sin(t * (2 * math.pi / 12.0))
        # Smooth fade-in (first 0.6s) and fade-out (last 0.8s) so the
        # window arrives and leaves like a held breath.
        fade_in = clamp(t / 0.6, 0.0, 1.0)
        fade_out = clamp((duration - t) / 0.8, 0.0, 1.0)
        envelope = fade_in * fade_out  # 0..1 master visibility

        # Background — mood-tinted gradient with a faint breathing brighten.
        # Azimuth keeps its near-black radar palette regardless of mood so
        # the phosphor green/amber stays high-contrast.
        if preset == 'azimuth':
            bg_top, bg_bot = (4, 10, 6), (2, 5, 3)
        else:
            bg_top, bg_bot = palette['bg_top'], palette['bg_bot']
        bg_lift = int(6 * breath)  # +6 lumens at peak inhale
        draw_gradient(
            screen, width, height,
            (bg_top[0] + bg_lift, bg_top[1] + bg_lift, bg_top[2] + bg_lift),
            bg_bot,
        )

        # Ambient dust drifts BEHIND the main drawing.
        _draw_motes(screen, motes, t, width, height, palette['accent'], breath)

        if preset == "orbit":
            draw_orbit(screen, t, width, height, color)
        elif preset == "signal_drift":
            draw_signal_drift(screen, t, width, height, color)
        elif preset == "pulse":
            draw_pulse(screen, t, width, height, color)
        elif preset == "spiral":
            draw_spiral(screen, t, width, height, color, text,
                        trail=trail_surface, state=trail_state)
        elif preset == "fractal":
            draw_fractal(screen, t, width, height, color, text)
        elif preset == "lissajous":
            draw_lissajous(screen, t, width, height, color, text,
                           trail=trail_surface, state=trail_state)
        elif preset == "azimuth":
            draw_azimuth(screen, t, width, height, color, text,
                         trail=trail_surface, state=trail_state)
        else:
            draw_constellation(screen, t, width, height, color)

        # ── Post layers (composited on top of the drawing) ─────────
        # Soft radial vignette frames the canvas.
        screen.blit(vignette, (0, 0))
        # Subtle film grain — shifted each frame so it shimmers rather
        # than tiling visibly.
        gx = -int((t * 73) % grain_tile.get_width())
        gy = -int((t * 91) % grain_tile.get_height())
        for ox in range(gx, width, grain_tile.get_width()):
            for oy in range(gy, height, grain_tile.get_height()):
                screen.blit(grain_tile, (ox, oy))

        # ── Mood header (bottom-left, lower-key, breathing) ───────
        # Header lives in the bottom-left corner so it never competes
        # with the centered drawing. Alpha breathes ±15% with the
        # respiration cycle and respects the global envelope.
        head_alpha = int(255 * envelope * (0.78 + 0.18 * breath))
        mood_label = f'Mood: {mood}'
        mood_surf = font.render(mood_label, True, color)
        mood_surf.set_alpha(head_alpha)
        # Soft drop shadow for legibility against any background.
        shadow = font.render(mood_label, True, (0, 0, 0))
        shadow.set_alpha(int(head_alpha * 0.6))
        hx = int(width * 0.05)
        hy = int(height - mood_surf.get_height() - height * 0.09)
        screen.blit(shadow, (hx + 2, hy + 2))
        screen.blit(mood_surf, (hx, hy))

        preset_surf = font_sm.render(preset.upper(), True, palette['accent'])
        preset_surf.set_alpha(int(head_alpha * 0.7))
        screen.blit(preset_surf, (hx + 2, hy + mood_surf.get_height() + 2))

        # ── Heartbeat indicator (bottom-right) ─────────────────────
        # Tiny pulsing dot + 1px ring — the system's "vital sign".
        # Beats roughly once per second, independent of the slow breath.
        beat = 0.5 + 0.5 * math.sin(t * 2 * math.pi * 1.05)
        beat = beat ** 2.2  # sharpen into a heartbeat-like spike
        hb_cx = int(width - width * 0.05)
        hb_cy = int(height - height * 0.05)
        hb_r = int(3 + 4 * beat)
        hb_alpha = int(220 * envelope)
        hb_surf = pygame.Surface((24, 24), pygame.SRCALPHA)
        pygame.draw.circle(hb_surf, (*color, hb_alpha), (12, 12), hb_r)
        pygame.draw.circle(hb_surf, (*palette['accent'], int(hb_alpha * 0.5)),
                           (12, 12), int(8 + 3 * beat), 1)
        screen.blit(hb_surf, (hb_cx - 12, hb_cy - 12))

        # ── Master envelope fade (in/out) ──────────────────────────
        # Black veil with inverted envelope alpha gives a smooth arrival
        # and departure. Skipped at full visibility for perf.
        if envelope < 0.999:
            veil_alpha = int(255 * (1.0 - envelope))
            veil = pygame.Surface((width, height), pygame.SRCALPHA)
            veil.fill((0, 0, 0, veil_alpha))
            screen.blit(veil, (0, 0))

        pygame.display.flip()
        clock.tick(fps)

        # Auto-exit after fixed duration from first frame.
        if now - first_frame > duration:
            running = False

    pygame.quit()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--text', '-t', default='')
    parser.add_argument('--mood', default='Serene')
    parser.add_argument('--duration', type=float, default=8.0)
    parser.add_argument('--width', type=int, default=520)
    parser.add_argument('--height', type=int, default=520)
    parser.add_argument('--fps', type=int, default=60)
    parser.add_argument('--preset', default='auto',
                        choices=['auto', 'orbit', 'signal_drift', 'pulse', 'constellation',
                                 'spiral', 'fractal', 'lissajous', 'azimuth',
                                 # Backward-compat aliases:
                                 'ribbons', 'rose'])
    args = parser.parse_args()
    run_visual(args.text, args.mood, args.duration, args.width, args.height, args.fps, args.preset)


if __name__ == '__main__':
    main()
