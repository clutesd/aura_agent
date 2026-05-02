// ══════════════════════════════════════════════════════════════════
//  Celestial — real-time sun & moon driven by wall clock
// ══════════════════════════════════════════════════════════════════
//
//  Astronomical model (good to ~arc-minute for sun, ~degree for moon):
//
//    * Convert UNIX epoch → Julian Day (UT)
//    * Sun:  mean longitude → ecliptic longitude → RA/Dec
//    * Moon: simplified ELP series → ecliptic lon/lat → RA/Dec
//    * GMST → LST (using observer longitude)
//    * (RA,Dec) + LST + lat → (alt, az) with atmospheric refraction
//    * Phase angle from sun-moon ecliptic elongation → illumination
//
//  All inputs are real-world: epoch_secs from SystemTime, lat/lon
//  from the weather geo lookup. No reliance on the local wall-clock
//  hour, so timezone & DST cannot drift the geometry.

use raylib::prelude::*;
use std::f32::consts::PI;

const DEG2RAD_F: f32 = PI / 180.0;
const DEG2RAD: f64 = std::f64::consts::PI / 180.0;
const TWO_PI: f64 = 2.0 * std::f64::consts::PI;

#[derive(Default, Clone, Copy)]
pub struct Celestial {
    /// Sun altitude in degrees (>0 = above horizon, refraction-corrected).
    pub sun_alt: f32,
    /// Sun azimuth in degrees (0=N, 90=E, 180=S, 270=W).
    pub sun_az: f32,
    /// Moon altitude in degrees.
    pub moon_alt: f32,
    /// Moon azimuth in degrees.
    pub moon_az: f32,
    /// Lunar illumination fraction (0=new, 0.5=quarter, 1=full).
    pub moon_illum: f32,
    /// Synodic phase 0..1 (0=new, 0.25=first qtr, 0.5=full, 0.75=last qtr).
    pub moon_phase: f32,
    /// Position angle of the bright limb (radians, 0=up, +CCW).
    /// Used to orient the crescent correctly regardless of sky angle.
    pub moon_limb_angle: f32,
    lat_deg: f32,
    lon_deg: f32,
    last_epoch_secs: f64,
    initialized: bool,
}

impl Celestial {
    pub fn new() -> Self {
        Self {
            lat_deg: 45.0,
            lon_deg: 0.0,
            ..Default::default()
        }
    }

    /// Update celestial geometry from real time + observer location.
    /// `lat`/`lon` come from the weather geo lookup (degrees, +N/+E).
    /// `epoch_secs` is the current UNIX timestamp in UTC (fractional).
    pub fn update(&mut self, lat: Option<f64>, lon: Option<f64>, epoch_secs: f64) {
        if let Some(l) = lat {
            self.lat_deg = l as f32;
        }
        if let Some(l) = lon {
            self.lon_deg = l as f32;
        }

        // Julian Day (UT). Unix epoch (1970-01-01 00:00 UTC) = JD 2440587.5
        let jd = 2_440_587.5 + epoch_secs / 86_400.0;
        let d = jd - 2_451_545.0; // days since J2000.0

        // ── Sun (low-precision formulas, USNO Almanac) ──
        let l_sun = (280.460 + 0.9856474 * d).rem_euclid(360.0); // mean lon
        let g_sun = ((357.528 + 0.9856003 * d).rem_euclid(360.0)) * DEG2RAD; // mean anom
        let lam_sun = (l_sun + 1.915 * g_sun.sin() + 0.020 * (2.0 * g_sun).sin()).rem_euclid(360.0); // ecliptic lon
        let eps = 23.439 - 0.0000004 * d; // obliquity
        let (ra_sun, dec_sun) = ecliptic_to_equatorial(lam_sun, 0.0, eps);

        // ── Moon (simplified Meeus / ELP) ──
        let l_moon = (218.316 + 13.176396 * d).rem_euclid(360.0); // mean lon
        let m_moon = ((134.963 + 13.064993 * d).rem_euclid(360.0)) * DEG2RAD; // mean anom
        let f_moon = ((93.272 + 13.229350 * d).rem_euclid(360.0)) * DEG2RAD; // arg of lat
        let lam_moon = (l_moon + 6.289 * m_moon.sin()).rem_euclid(360.0); // ecliptic lon
        let beta_moon = 5.128 * f_moon.sin(); // ecliptic lat
        let (ra_moon, dec_moon) = ecliptic_to_equatorial(lam_moon, beta_moon, eps);

        // ── Sidereal time (GMST → LST) ──
        // Meeus eq. 12.4 (low precision, sufficient here)
        let gmst_deg = (280.46061837 + 360.98564736629 * d).rem_euclid(360.0);
        let lst_deg = (gmst_deg + self.lon_deg as f64).rem_euclid(360.0);

        // ── Hour angles ──
        let ha_sun = (lst_deg - ra_sun).rem_euclid(360.0);
        let ha_moon = (lst_deg - ra_moon).rem_euclid(360.0);

        // ── Horizon coordinates ──
        let lat = self.lat_deg as f64;
        let (mut sun_alt, sun_az) = eq_to_horizon(ha_sun, dec_sun, lat);
        let (mut moon_alt, moon_az) = eq_to_horizon(ha_moon, dec_moon, lat);

        // Atmospheric refraction (Bennett 1982) — lifts apparent altitude
        // most strongly near the horizon (~34' at h=0).
        sun_alt += refraction_deg(sun_alt);
        moon_alt += refraction_deg(moon_alt);

        let sun_alt_t = sun_alt as f32;
        let sun_az_t = sun_az as f32;
        let moon_alt_t = moon_alt as f32;
        let moon_az_t = moon_az as f32;

        // Temporal smoothing so celestial motion stays fluid at render FPS,
        // while still converging quickly after a pause or frame hitch.
        if !self.initialized {
            self.sun_alt = sun_alt_t;
            self.sun_az = sun_az_t;
            self.moon_alt = moon_alt_t;
            self.moon_az = moon_az_t;
            self.initialized = true;
        } else {
            let dt = (epoch_secs - self.last_epoch_secs).clamp(0.0, 0.25) as f32;
            let a = (dt * 4.5).clamp(0.05, 0.95);
            self.sun_alt += (sun_alt_t - self.sun_alt) * a;
            self.moon_alt += (moon_alt_t - self.moon_alt) * a;
            self.sun_az = lerp_angle_deg(self.sun_az, sun_az_t, a);
            self.moon_az = lerp_angle_deg(self.moon_az, moon_az_t, a);
        }
        self.last_epoch_secs = epoch_secs;

        // ── Phase / illumination ──
        // Phase angle ψ between sun-as-seen-from-earth and moon: just the
        // ecliptic elongation works to ~1° for visual purposes.
        let elong = (lam_moon - lam_sun).rem_euclid(360.0);
        let phase01 = (elong / 360.0) as f32; // 0=new, 0.5=full
        self.moon_phase = phase01;
        let psi = elong * DEG2RAD;
        self.moon_illum = (0.5 * (1.0 - psi.cos())) as f32; // (1-cosψ)/2

        // Position angle of bright limb (Meeus §48.5). Tells us which way
        // the crescent "points" on the sky — important for low-latitude
        // / equatorial observers where the moon can lie on its back.
        let dra = (ra_sun - ra_moon) * DEG2RAD;
        let ds = dec_sun * DEG2RAD;
        let dm = dec_moon * DEG2RAD;
        let chi =
            (ds.cos() * dra.sin()).atan2(ds.sin() * dm.cos() - ds.cos() * dm.sin() * dra.cos());
        // chi is measured from celestial north, eastward. For our 2D screen
        // we approximate "up" as celestial north — fine for visual fidelity.
        self.moon_limb_angle = chi as f32;
    }

    /// Map (alt, az) → screen-space (x,y). Observer faces south (north
    /// hemisphere) or north (south hemisphere) so the sun's daily arc
    /// runs left→right across the visible band. Azimuth window is
    /// 240° wide centred on the meridian behind the observer.
    fn project(&self, alt: f32, az: f32, w: f32, horizon_y: f32) -> (f32, f32) {
        // For +lat observers: looking south, east(90°) is on the LEFT,
        // south(180°) centre, west(270°) right → frac = (az-60)/240.
        // For -lat: facing north, east is on the RIGHT, so mirror.
        let frac_raw = if self.lat_deg >= 0.0 {
            (az - 60.0) / 240.0
        } else {
            // Facing north: east(90°) on right, west(270°) on left,
            // mapped via az' = 360-az → frac = (300-az)/240.
            (300.0 - az) / 240.0
        };
        // Clamp so the body stays fully on-screen even when it is at
        // the very edge of the visible azimuth window (just rising or
        // setting). Margin is in screen-fraction units and sized to
        // keep the body's halo (~5r) inside the frame.
        let margin = 0.13;
        let frac = frac_raw.clamp(margin, 1.0 - margin);
        let x = frac * w;
        // Vertical: 0° → horizon, 90° → 24px from top.
        let alt_clamped = alt.max(-15.0);
        let h_above = (alt_clamped / 90.0).clamp(-0.2, 1.0);
        let y = horizon_y - h_above * (horizon_y - 24.0);
        (x, y)
    }

    /// Public projection of the sun to screen space using the same
    /// mapping as the primary `draw()` path. Returned tuple is
    /// `(x, y, alt_deg)`. Lets callers reason about whether the sun
    /// disc is being occluded by other on-screen elements.
    pub fn sun_screen_pos(&self, w: i32, h: i32) -> (f32, f32, f32) {
        let wf = w as f32;
        let hf = h as f32;
        let horizon_y = hf * 0.62;
        // For below-horizon altitudes use a virtual lower horizon so
        // the projection stays continuous and the resulting direction
        // vector is still meaningful.
        let (x, y) = if self.sun_alt >= 0.0 {
            self.project(self.sun_alt, self.sun_az, wf, horizon_y)
        } else {
            self.project(self.sun_alt.max(-30.0), self.sun_az, wf, hf * 1.4)
        };
        (x, y, self.sun_alt)
    }

    /// Render a strong directional sun corona on the Earth globe's
    /// rim whenever the sun's actual screen position is occluded by
    /// (or near) the globe disc. The glow points from the globe's
    /// centre toward the true sun screen position, so its location
    /// naturally swings to the eastern limb at sunrise and the
    /// western limb at sunset. Intensity also tracks `sun_alt` so it
    /// peaks during twilight and softens (but never disappears) at
    /// deep night, giving the user a continuous "the sun is over
    /// there" cue.
    pub fn draw_globe_sun_glow(
        &self,
        d: &mut impl RaylibDraw,
        globe_cx: f32,
        globe_cy: f32,
        globe_r: f32,
        sun_x: f32,
        sun_y: f32,
    ) {
        // Vector globe → sun in screen space.
        let mut dx = sun_x - globe_cx;
        let mut dy = sun_y - globe_cy;
        let dist = (dx * dx + dy * dy).sqrt().max(1e-3);

        // Visual sun radius matches the sky-disc sizing in `draw()`.
        // Only signal when the sun is actually hidden behind / clipped
        // by the globe disc — otherwise it's already visible in the
        // sky and the regular corona is doing its job.
        let sun_r = globe_r * 0.30; // approximate halo footprint
        let occlusion_range = globe_r + sun_r * 0.6;
        if dist > occlusion_range {
            return;
        }

        dx /= dist;
        dy /= dist;

        // Anchor the glow on the globe's rim on the sun-facing side.
        let rim_x = globe_cx + dx * globe_r;
        let rim_y = globe_cy + dy * globe_r;

        // ── Color & intensity envelope ───────────────────────────────
        // Day: bright warm white. Twilight: amber/red. Night: dim ember.
        let alt = self.sun_alt;
        let day = (alt / 8.0).clamp(0.0, 1.0);
        let twilight = (1.0 - (alt.abs() / 10.0)).clamp(0.0, 1.0); // peaks at 0°
        let nightly = (1.0 - ((alt + 18.0).abs() / 18.0)).clamp(0.0, 1.0);
        // Always at least a faint ember so the sun is never lost.
        let intensity = (0.35 + 0.65 * (day.max(twilight).max(nightly * 0.4))).clamp(0.0, 1.0);

        // Color mix: white(day) / amber(twilight) / deep red(night).
        let warm = (1.0 - day).clamp(0.0, 1.0);
        let cr: u8 = 255;
        let cg = (90.0 + 165.0 * day + 90.0 * twilight * (1.0 - day)) as u8;
        let cb = (40.0 + 200.0 * day + 30.0 * twilight * (1.0 - day) + 20.0 * (1.0 - warm)) as u8;
        let cg = cg.min(245);
        let cb = cb.min(240);

        // ── Wide outer corona (atmospheric scatter halo) — restrained
        // Keep it visible but never overpowering the rest of the scene.
        let corona_r = globe_r * (1.4 + 0.5 * twilight + 0.3 * day);
        let corona_a = (110.0 * intensity).min(150.0) as u8;
        d.draw_circle_gradient(
            rim_x as i32,
            rim_y as i32,
            corona_r,
            Color::new(cr, cg, cb, corona_a),
            Color::new(cr, cg, cb, 0),
        );

        // ── Mid halo — softer, more compact ─────────────────────────
        let halo_r = globe_r * (0.75 + 0.30 * twilight + 0.15 * day);
        let halo_a = (150.0 * intensity).min(180.0) as u8;
        d.draw_circle_gradient(
            rim_x as i32,
            rim_y as i32,
            halo_r,
            Color::new(cr, cg, cb, halo_a),
            Color::new(cr, cg, cb, 0),
        );

        // ── Bright rim bead — light spilling around the limb ─────────
        let bead_x = globe_cx + dx * globe_r * 0.97;
        let bead_y = globe_cy + dy * globe_r * 0.97;
        let bead_r = globe_r * (0.16 + 0.08 * twilight + 0.06 * day);
        d.draw_circle_gradient(
            bead_x as i32,
            bead_y as i32,
            bead_r,
            Color::new(
                255,
                (cg as u16 + 30).min(255) as u8,
                (cb as u16 + 60).min(255) as u8,
                (180.0 * intensity).min(200.0) as u8,
            ),
            Color::new(cr, cg, cb, 0),
        );
        // Tiny hot core for the eye to lock onto.
        d.draw_circle(
            bead_x as i32,
            bead_y as i32,
            bead_r * 0.30,
            Color::new(255, 240, 220, (200.0 * intensity).min(220.0) as u8),
        );

        // ── Tangential twilight arc along the rim ────────────────────
        // Soft elongated bar perpendicular to the sun direction —
        // reads as the planet's terminator catching the sunlight.
        let tx = -dy;
        let ty = dx;
        let arc_len = globe_r * (0.55 + 0.40 * twilight);
        let arc_thk = globe_r * (0.05 + 0.04 * twilight);
        let arc_a = (130.0 * (0.45 + 0.55 * twilight) * intensity).min(170.0) as u8;
        for k in -4..=4 {
            let f = k as f32 / 4.0;
            let bx = bead_x + tx * arc_len * f * 0.5;
            let by = bead_y + ty * arc_len * f * 0.5;
            d.draw_circle_gradient(
                bx as i32,
                by as i32,
                arc_thk * 1.8,
                Color::new(cr, cg, cb, arc_a),
                Color::new(cr, cg, cb, 0),
            );
        }
    }

    /// Draw the dominant body. Above-horizon sun wins; otherwise moon
    /// if it is up. Both can be drawn in twilight (sun just set, moon
    /// already up) — useful for golden-hour scenes.
    pub fn draw(&self, d: &mut impl RaylibDraw, w: i32, h: i32) {
        let wf = w as f32;
        let hf = h as f32;
        let horizon_y = hf * 0.62;
        // Larger so the body reads clearly even with HUD chrome over it.
        let body_r = (wf.min(hf) * 0.13).max(96.0);

        // Strict day/night: sun rules whenever it is above the (refracted)
        // horizon — matches civil sunrise/sunset. Otherwise moon shows.
        let sun_up = self.sun_alt > 0.0;
        let moon_up = self.moon_alt > 0.0;

        if sun_up {
            let (x, y) = self.project(self.sun_alt, self.sun_az, wf, horizon_y);
            self.draw_sun(d, x, y, body_r);
        } else if moon_up {
            let (mx, my) = self.project(self.moon_alt, self.moon_az, wf, horizon_y);
            // Compute the sun's projected position even though it's below
            // the horizon — this gives us a robust direction toward the
            // actual sun in screen-space, which is the direction the moon's
            // bright limb must face. We use a virtual horizon below the
            // visible one so projection geometry remains continuous.
            let virtual_h = hf * 1.4;
            let (sx, sy) = self.project(self.sun_alt.max(-30.0), self.sun_az, wf, virtual_h);
            // Unit vector from moon → sun (screen)
            let mut dx = sx - mx;
            let mut dy = sy - my;
            let len = (dx * dx + dy * dy).sqrt().max(1e-3);
            dx /= len;
            dy /= len;
            self.draw_moon(d, mx, my, body_r * 0.9, dx, dy);
        }
    }

    fn draw_sun(&self, d: &mut impl RaylibDraw, x: f32, y: f32, r: f32) {
        // ── Color temperature by altitude ──────────────────────────
        // Three-stop gradient (deep red @ 0°, amber @ 8°, warm white @ 30°+)
        let alt = self.sun_alt;
        let lo = (1.0 - (alt / 8.0).clamp(0.0, 1.0)).powf(1.2);
        let hi = ((alt - 8.0) / 22.0).clamp(0.0, 1.0);
        let mid = (1.0 - lo - hi).max(0.0);
        let cr = (255.0 * lo + 255.0 * mid + 255.0 * hi) as u8;
        let cg = (80.0 * lo + 175.0 * mid + 245.0 * hi) as u8;
        let cb = (35.0 * lo + 100.0 * mid + 220.0 * hi) as u8;

        let cx = x as i32;
        let cy = y as i32;

        // ── Outer corona — wide, low alpha, slightly cooler ────────
        // Reads as scattered atmospheric light, biggest near horizon.
        let corona_scale = 5.0 + lo * 4.0;
        let corona_a = (28.0 + lo * 60.0).min(255.0) as u8;
        let corona_col = Color::new(
            cr,
            (cg as u16 * 4 / 5).min(255) as u8,
            (cb as u16 * 3 / 5).min(255) as u8,
            corona_a,
        );
        d.draw_circle_gradient(cx, cy, r * corona_scale, corona_col, Color::new(0, 0, 0, 0));

        // ── Diffraction / subtle lens-flare ring (axis-aligned) ────
        // Four soft elongated bands radiating from the disc — gives
        // the impression of intense brightness without being garish.
        let spike_r = r * (3.0 + lo * 2.5);
        let spike_a = (40.0 + lo * 70.0).min(255.0) as u8;
        let spike_col = Color::new(cr, cg, cb, spike_a);
        // Horizontal spike
        d.draw_ellipse(cx, cy, spike_r, r * 0.18, spike_col);
        // Vertical spike
        d.draw_ellipse(cx, cy, r * 0.18, spike_r, spike_col);
        // Diagonals — slightly shorter, dimmer
        let diag_a = (spike_a as u16 * 5 / 8) as u8;
        let diag_col = Color::new(cr, cg, cb, diag_a);
        let dr = spike_r * 0.78;
        // Approximate diagonals via overlaid rotated thin ellipses;
        // raylib has no rotated ellipse so use thin triangles instead.
        for sign in [1.0_f32, -1.0] {
            // Two triangles forming a thin rhombus per diagonal.
            let dx = dr * 0.7071;
            let dy = dr * 0.7071 * sign;
            let nx = r * 0.10 * 0.7071;
            let ny = r * 0.10 * 0.7071;
            let p_far_a = rvec2(x + dx, y + dy);
            let p_far_b = rvec2(x - dx, y - dy);
            let p_side_a = rvec2(x - ny, y + nx);
            let p_side_b = rvec2(x + ny, y - nx);
            d.draw_triangle(p_far_a, p_side_a, p_side_b, diag_col);
            d.draw_triangle(p_far_b, p_side_b, p_side_a, diag_col);
        }

        // ── Inner halo / chromatic ring ────────────────────────────
        // Saturated warm ring just outside the disc — mimics the
        // chromatic bloom you see around the real sun.
        d.draw_circle_gradient(
            cx,
            cy,
            r * 1.85,
            Color::new(cr, cg, cb, 210),
            Color::new(cr, cg, cb, 0),
        );

        // Slight red/orange outer chromatic edge
        let edge_r = ((cr as u16 + 20).min(255)) as u8;
        let edge_g = (cg as u16 * 3 / 4) as u8;
        let edge_b = (cb as u16 / 2) as u8;
        d.draw_circle_gradient(
            cx,
            cy,
            r * 1.18,
            Color::new(edge_r, edge_g, edge_b, 160),
            Color::new(edge_r, edge_g, edge_b, 0),
        );

        // ── Disc with limb darkening ──────────────────────────────
        // Real photospheric limb darkening: edges noticeably dimmer.
        // Approximated by stacking a slightly darker base disc, then
        // a brighter inner disc on top.
        let dark_r = (cr as f32 * 0.82) as u8;
        let dark_g = (cg as f32 * 0.78) as u8;
        let dark_b = (cb as f32 * 0.72) as u8;
        d.draw_circle_v(rvec2(x, y), r, Color::new(dark_r, dark_g, dark_b, 255));
        d.draw_circle_v(rvec2(x, y), r * 0.92, Color::new(cr, cg, cb, 255));

        // ── Hot inner core ────────────────────────────────────────
        // Off-centre highlight — gives a sense of solidity / 3D.
        let hi_r = ((cr as u16 + 0).min(255)) as u8;
        let hi_g = ((cg as u16 + 30).min(255)) as u8;
        let hi_b = ((cb as u16 + 30).min(255)) as u8;
        d.draw_circle_gradient(
            (x - r * 0.15) as i32,
            (y - r * 0.18) as i32,
            r * 0.7,
            Color::new(hi_r, hi_g, hi_b, 200),
            Color::new(255, 250, 235, 0),
        );
        // Tiny specular hot spot
        d.draw_circle_v(
            rvec2(x - r * 0.22, y - r * 0.22),
            r * 0.18,
            Color::new(255, 250, 240, 230),
        );
    }

    fn draw_moon(
        &self,
        d: &mut impl RaylibDraw,
        x: f32,
        y: f32,
        r: f32,
        // Unit vector pointing from moon toward sun in screen space.
        // The bright limb faces this direction; the shadow occluder is
        // offset opposite (-bx,-by). Caller computes this from the
        // sun's screen position so orientation is correct anywhere on
        // the sky regardless of celestial axis tilt.
        bx: f32,
        by: f32,
    ) {
        // ── Cool palette with slight warm tint (real moon is ~tan) ─
        let core = Color::new(232, 230, 220, 255); // lit highlands
        let warm = Color::new(245, 238, 220, 255); // sub-solar bright
        let dark = Color::new(150, 152, 158, 255); // mare basalt
        let shadow_col = Color::new(8, 12, 22, 255); // night sky

        let cx = x as i32;
        let cy = y as i32;

        // ── Outer halo / atmospheric corona ───────────────────────
        // Bigger when moon is near full (more light scatter).
        let phase_glow = 0.4 + 0.6 * self.moon_illum;
        d.draw_circle_gradient(
            cx,
            cy,
            r * 5.0,
            Color::new(150, 175, 220, (28.0 * phase_glow) as u8),
            Color::new(0, 0, 0, 0),
        );
        d.draw_circle_gradient(
            cx,
            cy,
            r * 2.6,
            Color::new(180, 200, 235, (60.0 * phase_glow) as u8),
            Color::new(0, 0, 0, 0),
        );
        // Tight bright halo — strongest near the limb
        d.draw_circle_gradient(
            cx,
            cy,
            r * 1.35,
            Color::new(220, 228, 245, (110.0 * phase_glow) as u8),
            Color::new(220, 228, 245, 0),
        );

        // ── Lit base disc ─────────────────────────────────────────
        d.draw_circle_v(rvec2(x, y), r, core);

        // Subtle limb darkening (darker thin edge)
        d.draw_ring(
            rvec2(x, y),
            r * 0.94,
            r,
            0.0,
            360.0,
            64,
            Color::new(180, 180, 175, 110),
        );

        // Sub-solar bright spot — along the bright-limb direction
        // (toward sun in screen space).
        let sub_x = x + bx * r * 0.35;
        let sub_y = y + by * r * 0.35;
        d.draw_circle_gradient(
            sub_x as i32,
            sub_y as i32,
            r * 0.65,
            Color::new(warm.r, warm.g, warm.b, 110),
            Color::new(warm.r, warm.g, warm.b, 0),
        );

        // ── Maria — actual lunar dark plains ──────────────────────
        // Positions in moon body frame as seen from Earth, normalised
        // to ±1 across the disc. Body "up" is celestial north, which
        // we approximate as screen up. (x_moon, y_moon, radius)
        // Sources: visual map of near-side maria.
        const MARIA: &[(f32, f32, f32, u8)] = &[
            // (mx, my, r_frac, alpha)
            (-0.30, -0.30, 0.32, 130), // Mare Imbrium
            (0.10, -0.32, 0.22, 120),  // Mare Serenitatis
            (0.32, -0.10, 0.22, 125),  // Mare Tranquillitatis
            (0.55, -0.05, 0.13, 110),  // Mare Crisium (small, near right limb)
            (-0.50, 0.05, 0.30, 115),  // Oceanus Procellarum (left)
            (-0.18, 0.30, 0.20, 110),  // Mare Nubium
            (0.05, 0.42, 0.16, 95),    // Mare Cognitum / Humorum area
            (0.30, 0.32, 0.14, 90),    // Mare Fecunditatis
            (0.00, -0.55, 0.36, 60),   // Mare Frigoris band (thin)
        ];
        // Clip everything we draw next to the moon's circle so maria
        // never spill outside the limb.
        for &(mx_n, my_n, mr_n, ma) in MARIA {
            let mx = x + mx_n * r;
            let my = y + my_n * r;
            let mr = mr_n * r;
            // Skip blobs whose centre falls outside disc (beyond limb)
            let d2 = (mx - x).powi(2) + (my - y).powi(2);
            if d2 > (r * 0.95).powi(2) {
                continue;
            }
            d.draw_circle_gradient(
                mx as i32,
                my as i32,
                mr,
                Color::new(dark.r, dark.g, dark.b, ma),
                Color::new(dark.r, dark.g, dark.b, 0),
            );
        }

        // ── Bright young craters (Tycho, Copernicus) — small specks
        //    with tiny ray systems ──────────────────────────────────
        const CRATERS: &[(f32, f32, f32)] = &[
            (-0.05, 0.55, 0.05), // Tycho (south, prominent rays)
            (-0.20, 0.05, 0.04), // Copernicus
            (0.40, -0.40, 0.03), // Aristarchus area
        ];
        for &(cx_n, cy_n, cr_n) in CRATERS {
            let mx = x + cx_n * r;
            let my = y + cy_n * r;
            let mr = cr_n * r;
            let d2 = (mx - x).powi(2) + (my - y).powi(2);
            if d2 > (r * 0.92).powi(2) {
                continue;
            }
            // Bright rim
            d.draw_circle_v(rvec2(mx, my), mr * 1.2, Color::new(255, 250, 235, 80));
            // Dark central pit
            d.draw_circle_v(rvec2(mx, my), mr * 0.6, Color::new(120, 120, 125, 140));
        }

        // ── Phase shadow ──────────────────────────────────────────
        // True moon-phase geometry: the apparent terminator on a
        // viewed sphere is an ellipse whose semi-axis along the
        // bright-limb direction equals r·|cos(α)|, where α is the
        // sun-moon-earth phase angle. With illumination fraction
        // k ∈ [0,1], the terminator x-coordinate (in a frame whose
        // +x points toward the sun in screen space) is
        //     x_t(y) = -(2k-1) · √(r²-y²)
        // and the dark region is the strip
        //     -√(r²-y²)  ≤  x  ≤  x_t(y).
        // We fill that strip with a triangle strip pairing points
        // on the dark limb with corresponding points on the
        // terminator ellipse — this gives the correct crescent /
        // quarter / gibbous shapes (a circle-as-occluder, as used
        // before, only matches the truth at full and new moon).
        let illum = self.moon_illum.clamp(0.0, 1.0);
        let t_phase = 2.0 * illum - 1.0; // -1 = new, +1 = full
                                         // perpendicular to the bright-limb direction (screen-space)
        let qx = -by;
        let qy = bx;
        const N_SEG: usize = 64;
        let mut strip: Vec<Vector2> = Vec::with_capacity((N_SEG + 1) * 2);
        for i in 0..=N_SEG {
            let q = -r + (2.0 * r) * (i as f32) / (N_SEG as f32);
            let s = (r * r - q * q).max(0.0).sqrt();
            // Point on the dark-side limb (always at p = -s)
            let p_limb = -s;
            let lx = x + p_limb * bx + q * qx;
            let ly = y + p_limb * by + q * qy;
            // Point on the terminator ellipse
            let p_term = -t_phase * s;
            let tx = x + p_term * bx + q * qx;
            let ty = y + p_term * by + q * qy;
            strip.push(rvec2(lx, ly));
            strip.push(rvec2(tx, ty));
        }
        d.draw_triangle_strip(&strip, shadow_col);

        // ── Soft terminator ───────────────────────────────────────
        // Two narrow translucent strips just inside the lit side of
        // the terminator soften the day/night line on the surface —
        // mimics the way real lunar topography blurs the edge.
        for (k_inset, alpha) in [(0.04_f32, 70u8), (0.10, 35)] {
            let mut soft: Vec<Vector2> = Vec::with_capacity((N_SEG + 1) * 2);
            for i in 0..=N_SEG {
                let q = -r + (2.0 * r) * (i as f32) / (N_SEG as f32);
                let s = (r * r - q * q).max(0.0).sqrt();
                let p_term = -t_phase * s;
                let p_inner = p_term + k_inset * r;
                // Two boundaries for the strip: terminator and inset toward lit
                let ax = x + p_term * bx + q * qx;
                let ay = y + p_term * by + q * qy;
                let bx2 = x + p_inner * bx + q * qx;
                let by2 = y + p_inner * by + q * qy;
                soft.push(rvec2(ax, ay));
                soft.push(rvec2(bx2, by2));
            }
            d.draw_triangle_strip(
                &soft,
                Color::new(shadow_col.r, shadow_col.g, shadow_col.b, alpha),
            );
        }

        // ── Earthshine ────────────────────────────────────────────
        // Faint blue-grey glow on the dark side, strongest near new
        // moon (when Earth appears "full" from the moon's vantage).
        // Drawn AFTER the hard shadow, clipped to the dark region by
        // re-using the same terminator-ellipse polygon with very low
        // alpha — so the unlit disc is not pitch black.
        let es_a = ((1.0 - illum).powf(1.4) * 36.0) as u8;
        if es_a > 3 {
            d.draw_triangle_strip(&strip, Color::new(60, 78, 110, es_a));
        }
    }
}

// ──────────────────────────────────────────────────────────────────
//  Geometry helpers (all degrees in / out unless noted)
// ──────────────────────────────────────────────────────────────────

/// Convert ecliptic (λ, β) to equatorial (RA, Dec). All in degrees.
fn ecliptic_to_equatorial(lam_deg: f64, beta_deg: f64, eps_deg: f64) -> (f64, f64) {
    let lam = lam_deg * DEG2RAD;
    let beta = beta_deg * DEG2RAD;
    let eps = eps_deg * DEG2RAD;
    let sin_dec = beta.sin() * eps.cos() + beta.cos() * eps.sin() * lam.sin();
    let dec = sin_dec.clamp(-1.0, 1.0).asin();
    let y = lam.sin() * eps.cos() - beta.tan() * eps.sin();
    let x = lam.cos();
    let mut ra = y.atan2(x);
    if ra < 0.0 {
        ra += TWO_PI;
    }
    (ra.to_degrees(), dec.to_degrees())
}

/// Convert hour-angle (deg west of meridian) + dec + lat to (alt, az).
/// Azimuth measured from north, clockwise (E=90, S=180, W=270).
fn eq_to_horizon(ha_deg: f64, dec_deg: f64, lat_deg: f64) -> (f64, f64) {
    let ha = ha_deg * DEG2RAD;
    let dec = dec_deg * DEG2RAD;
    let lat = lat_deg * DEG2RAD;
    let sin_alt = lat.sin() * dec.sin() + lat.cos() * dec.cos() * ha.cos();
    let alt = sin_alt.clamp(-1.0, 1.0).asin();
    // az = atan2(sin H, cos H sin φ - tan δ cos φ), measured from south
    // westward in some conventions; we want from north clockwise.
    let y = ha.sin();
    let x = ha.cos() * lat.sin() - dec.tan() * lat.cos();
    let mut az = y.atan2(x); // from south, +west
    az = az + std::f64::consts::PI; // shift to from north, +east
    if az < 0.0 {
        az += TWO_PI;
    }
    if az >= TWO_PI {
        az -= TWO_PI;
    }
    (alt.to_degrees(), az.to_degrees())
}

/// Bennett (1982) atmospheric refraction in degrees, given true altitude.
/// ~34' at h=0, decreasing rapidly with altitude. Returns 0 below -2°.
fn refraction_deg(alt_deg: f64) -> f64 {
    if alt_deg < -2.0 {
        return 0.0;
    }
    let h = alt_deg + 7.31 / (alt_deg + 4.4);
    // 1 / tan(h) in degrees, scaled to arc-minutes; convert to deg.
    let r_arcmin = 1.0 / (h * DEG2RAD).tan();
    r_arcmin / 60.0
}

// Silence unused-import warning for the f32 const if the file ever
// shrinks past its current uses.
#[allow(dead_code)]
const _DEG2RAD_F: f32 = DEG2RAD_F;

#[inline]
fn lerp_angle_deg(from: f32, to: f32, alpha: f32) -> f32 {
    let mut delta = (to - from).rem_euclid(360.0);
    if delta > 180.0 {
        delta -= 360.0;
    }
    (from + delta * alpha).rem_euclid(360.0)
}
