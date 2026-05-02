#version 330

// ─────────────────────────────────────────────────────────────────
//  Earth half-globe — photoreal-ish satellite view (v4)
//
//  v4 additions:
//    • 3-D landmark spires for major cities/monuments (analytic
//      cylinder rendering with sun-side shading & pulse halo)
//    • Anti-aliased coastlines (fwidth-based smoothstep)
//    • Sharper, more directional sun specular with longer glitter tail
//    • Richer Rayleigh blue + cleaner ozone violet at the limb
//    • Slightly tightened cloud color grading at the terminator
//
//  Inherited from v3:
//    • 3-layer cloud system with self-shadowing & ITCZ band
//    • Multi-octave animated ocean waves with foam at coastlines
//    • Glitter ocean specular along sub-solar point
//    • Atmospheric scattering: Rayleigh blue rim + Mie warm halo
//    • Horizon-grazing twilight band (sunset belt)
//    • Subtle aurora ovals around the magnetic poles
//    • Snow accumulation by altitude AND latitude
//    • Higher-frequency ridge mountains, river valleys (dark veins)
//    • Continent-aware city lights (clustered, not uniform)
//    • Crisp Kenora pin: bright yellow dot + cyan ring + soft pulse
// ─────────────────────────────────────────────────────────────────

in  vec2 fragTexCoord;
out vec4 finalColor;

uniform vec2  u_resolution;
uniform vec2  u_center;
uniform float u_radius;
uniform float u_time;
uniform vec2  u_sun_dir;
uniform float u_alpha;
uniform float u_utc;
uniform float u_kenora_pulse;

const float PI = 3.14159265359;

// ── Hash & noise ────────────────────────────────────────────────
float hash(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}
float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    float a = hash(i);
    float b = hash(i + vec2(1.0, 0.0));
    float c = hash(i + vec2(0.0, 1.0));
    float d = hash(i + vec2(1.0, 1.0));
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(a, b, u.x) + (c - a) * u.y * (1.0 - u.x) + (d - b) * u.x * u.y;
}
float fbm(vec2 p, int oct) {
    float v = 0.0;
    float a = 0.5;
    mat2 r = mat2(0.8, -0.6, 0.6, 0.8);
    for (int i = 0; i < 8; i++) {
        if (i >= oct) break;
        v += a * vnoise(p);
        p = r * p * 2.02;
        a *= 0.5;
    }
    return v;
}
float ridged(vec2 p, int oct) {
    float v = 0.0;
    float a = 0.5;
    mat2 r = mat2(0.8, -0.6, 0.6, 0.8);
    for (int i = 0; i < 8; i++) {
        if (i >= oct) break;
        float n = 1.0 - abs(2.0 * vnoise(p) - 1.0);
        v += a * n * n;
        p = r * p * 2.04;
        a *= 0.5;
    }
    return v;
}
// Domain-warped FBM for organic landmasses.
float fbmWarped(vec2 p, int oct) {
    vec2 q = vec2(fbm(p + vec2(1.7, 9.2), 4),
                  fbm(p + vec2(8.3, 2.8), 4));
    return fbm(p + 1.8 * q, oct);
}

// ── Continents (more land-like than pure noise) ─────────────────
float continents(vec2 latlon) {
    float lat = latlon.x;
    float lon = latlon.y;
    // Stretch longitude so continents can span wider than they are tall.
    vec2 q = vec2(lon * 1.35, lat * 2.10);
    float h = fbmWarped(q + 4.2, 6);
    // Ridge enhancement for sharper coastlines.
    float r = 1.0 - abs(2.0 * fbm(q * 2.1 + 5.0, 4) - 1.0);
    h += 0.18 * r;
    // Slight equatorial bulge: more land likely near tropics + temperate.
    h += 0.10 * cos(lat * 1.4);
    // Continental drift bias (pole-ward thinning).
    h -= 0.06 * smoothstep(0.9, 1.4, abs(lat));
    return h;
}

// ── Three-band cloud sampler ────────────────────────────────────
//   x = low cumulus, y = mid, z = high cirrus
vec3 cloudLayers(vec2 latlon) {
    // Low cumulus (slow, dense)
    vec2 ql = vec2(latlon.y * 1.6 + u_time * 0.011, latlon.x * 2.4);
    float low  = fbmWarped(ql + 17.0, 5);
    low = smoothstep(0.55, 0.86, low);

    // Mid layer (fronts; wisps that wrap)
    vec2 qm = vec2(latlon.y * 2.4 + u_time * 0.018, latlon.x * 1.6);
    float mid = fbm(qm + 31.0, 5);
    mid = smoothstep(0.58, 0.80, mid) * 0.7;

    // High cirrus (fast, fibrous)
    vec2 qh = vec2(latlon.y * 3.6 + u_time * 0.034, latlon.x * 1.0);
    float high = fbm(qh + 5.0, 4);
    high = smoothstep(0.62, 0.80, high) * 0.55;

    // ITCZ (intertropical convergence zone) — equatorial cloud band
    float itcz = exp(-pow(latlon.x * 4.0, 2.0)) * 0.55;
    low = max(low, itcz * smoothstep(0.4, 0.9,
        fbm(vec2(latlon.y * 4.0 + u_time * 0.02, latlon.x * 6.0), 4)));

    return vec3(low, mid, high);
}

// ── 3-D landmark spires ─────────────────────────────────────────
//   Each landmark is rendered as an analytic vertical cylinder
//   rising radially from the sphere surface, plus a small luminous
//   cap and a soft pulsing halo. Sun-side shading uses the same
//   directional light that lights the surface so the towers stay
//   visually consistent with day/night.

// World→camera frame (inverse of the yaw(spin)·pitch(VIEW_LAT)
// applied to the surface normal at the top of main()).
vec3 worldToCam(vec3 pw, float spin, float pitch) {
    float cs = cos(-spin), sn = sin(-spin);
    vec3 a   = vec3(cs * pw.x + sn * pw.z, pw.y, -sn * pw.x + cs * pw.z);
    float c  = cos(-pitch), s = sin(-pitch);
    return vec3(a.x, c * a.y + s * a.z, -s * a.y + c * a.z);
}

// Analytic spire — modifies col in place (returned).
vec3 drawSpire(vec3 col, vec2 frag, vec2 cen, float R,
               float latDeg, float lonDeg, float height,
               vec3 capCol, vec3 sunDir, float spin, float pitch,
               float dayMask, float seed) {
    float la = radians(latDeg);
    float lo = radians(lonDeg);
    vec3  pw = vec3(cos(la) * sin(lo), sin(la), cos(la) * cos(lo));
    vec3  pc = worldToCam(pw, spin, pitch);
    // Hide on far side and at extreme limb (numerical instability).
    if (pc.z <= 0.06) return col;

    // Base & tip in screen space (note Y flip to match `frag` convention).
    vec2 base = cen + vec2(pc.x, -pc.y) * R;
    vec2 tip  = cen + vec2(pc.x, -pc.y) * R * (1.0 + height);

    // Distance from fragment to spire segment (anti-aliased).
    vec2 ab = tip - base;
    float t = clamp(dot(frag - base, ab) / max(dot(ab, ab), 1e-5), 0.0, 1.0);
    vec2 q = base + ab * t;
    float d = length(frag - q);

    // Spire silhouette — taper slightly toward the tip.
    float towerW = max(R * 0.0040, 1.1) * (1.0 - 0.35 * t);
    float aa     = 1.2;                      // pixels of feather
    float towerMask = smoothstep(towerW + aa, towerW - aa, d);

    // Sun-side shading: brighter facet toward sun, darker behind.
    float lit  = clamp(dot(normalize(pc), sunDir), -0.4, 1.0);
    float face = clamp(0.5 + 0.5 * lit, 0.0, 1.0);
    vec3 baseCol = vec3(0.78, 0.80, 0.88);   // brushed metal / concrete
    vec3 sunWarm = vec3(1.05, 0.85, 0.62);
    vec3 nightWarm = vec3(0.90, 0.55, 0.25);
    vec3 towerCol = mix(baseCol * 0.18,
                        mix(baseCol, sunWarm, 0.45),
                        face);
    // Window glow on the night side
    towerCol += nightWarm * (1.0 - face) * (1.0 - dayMask) * 0.55;

    col = mix(col, towerCol, towerMask * 0.92);

    // Glowing cap (slight sphere illusion).
    float dTip   = length(frag - tip);
    float capR   = max(R * 0.0085, 1.6);
    float capMask = smoothstep(capR + aa, capR - aa, dTip);
    vec3 capLit  = capCol * (0.55 + 0.55 * face);
    col = mix(col, capLit, capMask);

    // Pulsing halo around cap (per-landmark phase via `seed`).
    float pulse = 0.55 + 0.45 * sin(u_time * 2.4 + seed);
    float halo  = exp(-pow(dTip / (capR * 3.5), 2.0));
    col += capCol * halo * 0.30 * pulse;

    // Tiny lens-flare cross when cap is bright on day side.
    float flare = exp(-pow(dTip / (capR * 6.0), 2.0));
    col += capCol * flare * 0.08 * dayMask;

    return col;
}

void main() {
    vec2 frag = vec2(gl_FragCoord.x, u_resolution.y - gl_FragCoord.y);
    vec2 d    = frag - u_center;
    float dist = length(d);
    float R    = u_radius;

    float haloW = R * 0.26;
    if (dist > R + haloW) discard;

    // Sun direction in 3-D. Slight +Y bias so equator never goes black.
    vec3 sunDir = normalize(vec3(u_sun_dir.x, 0.30, u_sun_dir.y));

    // ───────────────── Atmosphere halo (outside disc) ─────────────────
    if (dist > R) {
        float t   = (dist - R) / haloW;
        // Two falloffs combined: tight bright inner ring + soft outer.
        float aIn  = pow(1.0 - t, 4.0);
        float aOut = pow(1.0 - t, 1.6) * 0.5;
        float a    = aIn + aOut;

        vec2  uv0  = d / max(dist, 0.0001);
        vec3  nE   = vec3(uv0.x, -uv0.y, 0.0);
        float litE = clamp(dot(nE, sunDir) * 0.5 + 0.5, 0.0, 1.0);
        // Twilight ring (sun grazing the limb)
        float twiBand = exp(-pow((litE - 0.5) * 4.5, 2.0));

        vec3 cool = vec3(0.30, 0.62, 1.10);
        vec3 warm = vec3(1.10, 0.55, 0.28);
        vec3 col  = mix(cool * 0.55, mix(cool, warm, 0.55), litE);
        col = mix(col, warm, twiBand * 0.65);

        // Slight purple at the very top of sunset belt (ozone layer feel).
        col += vec3(0.10, 0.05, 0.18) * twiBand * 0.4;

        finalColor = vec4(col * a * (0.55 + 0.45 * litE),
                          a * (0.55 + 0.40 * litE) * u_alpha);
        return;
    }

    // ─────────────────────── Surface sampling ───────────────────────
    vec2  uv = d / R;
    float nz = sqrt(max(0.0, 1.0 - dot(uv, uv)));
    vec3  n  = vec3(uv.x, -uv.y, nz);

    // ── View: satellite directly over Kenora ──
    // Reorient the surface so Kenora's lat/lon maps to the camera
    // axis (+Z). This makes the visible hemisphere the one centered
    // on Kenora, with the city marker sitting at disc-center where
    // it cannot be hidden by HUD chrome that hugs the upper edge.
    // A slow visual drift rotates the surface around Earth's polar
    // axis so the viewer still sees rotation. The day/night
    // terminator is independent (driven by sunDir from real
    // celestial geometry) so dawn/dusk sweep correctly across the
    // continents in real time.
    const float K_LAT_RAD = radians(49.7666);
    const float K_LON_RAD = radians(-94.4870);
    // Tilt the view by less than Kenora's full latitude — this keeps
    // Kenora in the upper half of the visible disc but leaves the
    // equator and a healthy slice of the southern hemisphere in
    // view. Looking *straight* down on Kenora would otherwise fill
    // the disc with polar ice.
    const float VIEW_LAT = radians(22.0);
    float spin = K_LON_RAD + u_time * (6.2831853 / 600.0); // 10-min drift
    float dummyUtc = u_utc * 0.0;

    // Step 1: pitch about X by VIEW_LAT (camera looks down at this lat).
    float clk = cos(VIEW_LAT), slk = sin(VIEW_LAT);
    vec3 nv = vec3(n.x,
                   clk * n.y + slk * n.z,
                  -slk * n.y + clk * n.z);
    // Step 2: yaw about Y (polar axis) → places K_LON and rotates
    //         Earth slowly underneath the camera over ~10 minutes.
    float cs = cos(spin), sn = sin(spin);
    vec3 nr = vec3( cs * nv.x + sn * nv.z,
                    nv.y,
                   -sn * nv.x + cs * nv.z);
    nr += vec3(dummyUtc); // suppress unused-uniform warnings

    float lat = asin(clamp(nr.y, -1.0, 1.0));
    float lon = atan(nr.x, nr.z);

    // Terrain heightfield with mountain ridges and river-valley dimples.
    float h    = continents(vec2(lat, lon));
    float ridge = ridged(vec2(lon * 3.4, lat * 3.4) + 9.0, 5);
    h += smoothstep(0.50, 0.62, h) * ridge * 0.20;
    float river = ridged(vec2(lon * 5.7, lat * 5.7) + 13.0, 5);
    h -= smoothstep(0.55, 0.70, h) * smoothstep(0.85, 1.0, river) * 0.06;

    float coast = smoothstep(0.46, 0.58, h);
    // Anti-aliased land mask: width adapts to local heightfield gradient
    // so coastlines stay crisp instead of dithering pixel-by-pixel.
    float hAA   = max(fwidth(h), 0.002);
    float land  = smoothstep(0.515 - hAA, 0.515 + hAA, h);

    float aLat   = abs(lat);
    float polarMask = smoothstep(1.10, 1.45, aLat * 1.6 + h * 0.20);
    float seaIce    = smoothstep(1.15, 1.40, aLat * 1.7) * (1.0 - land);

    // ── Animated ocean (small surface micro-detail used for shading) ──
    vec2 wq = vec2(lon * 18.0 + u_time * 0.18, lat * 18.0 - u_time * 0.12);
    float waves = fbm(wq, 4) * (1.0 - land);
    // Coastline foam: bright thin band at land/water boundary.
    float foam = smoothstep(0.49, 0.52, h) - smoothstep(0.52, 0.56, h);
    foam *= 0.85 + 0.5 * fbm(wq * 2.2, 3);

    // Three-tone ocean with hue variation per basin.
    vec3 deep    = vec3(0.005, 0.030, 0.115);
    vec3 mid     = vec3(0.020, 0.115, 0.290);
    vec3 shallow = vec3(0.085, 0.420, 0.585);
    float basinHue = fbm(vec2(lon * 1.0, lat * 1.0) + 21.0, 3) - 0.5;
    vec3 ocean   = mix(deep, mid, smoothstep(0.30, 0.42, h));
    ocean        = mix(ocean, shallow, smoothstep(0.30, 0.46, h) * coast);
    ocean       += vec3(-0.020, 0.010, 0.025) * basinHue;
    // Wave shimmer in shallow water
    ocean       += vec3(0.05, 0.10, 0.15) * waves * 0.10;

    // ── Biomes by latitude × moisture ──
    float moist  = fbm(vec2(lon * 2.5, lat * 2.5) + 11.0, 5);
    float arid   = smoothstep(0.35, 0.65, moist) * (1.0 - smoothstep(0.7, 1.2, aLat));
    vec3 jungle  = vec3(0.04, 0.26, 0.07);
    vec3 forest  = vec3(0.10, 0.30, 0.12);
    vec3 grass   = vec3(0.32, 0.42, 0.18);
    vec3 desert  = vec3(0.70, 0.55, 0.27);
    vec3 tundra  = vec3(0.45, 0.47, 0.42);
    vec3 snow    = vec3(0.95, 0.97, 1.00);

    vec3 landCol;
    if      (aLat < 0.35) landCol = mix(jungle, desert, arid);
    else if (aLat < 0.85) landCol = mix(forest, grass,  arid);
    else if (aLat < 1.15) landCol = mix(tundra, snow,
                                       smoothstep(0.95, 1.15, aLat));
    else                  landCol = snow;

    // Mountain snow caps: more aggressive at high altitude AND high lat.
    float mountain = smoothstep(0.78, 0.95, h);
    landCol = mix(landCol, snow,
                  mountain * (0.45 + 0.55 * smoothstep(0.4, 1.2, aLat)));

    // River-valley shading (slightly darker, greener).
    float riverMask = smoothstep(0.85, 1.0, river) * land;
    landCol = mix(landCol, vec3(0.06, 0.18, 0.12), riverMask * 0.45);

    // Beach/coast.
    float beach = smoothstep(0.50, 0.55, h) * (1.0 - smoothstep(0.55, 0.60, h));
    landCol = mix(landCol, vec3(0.86, 0.79, 0.56), beach * 0.55);

    vec3 surface = mix(ocean, landCol, land);
    // Foam ring (only on water side of coast).
    surface += vec3(1.0, 1.0, 1.0) * foam * (1.0 - land) * 0.6;
    surface = mix(surface, snow, clamp(polarMask + seaIce, 0.0, 1.0));

    // ── Clouds + cloud shadows ──
    vec3 cl = cloudLayers(vec2(lat, lon));
    float cloudDensity = clamp(cl.x + cl.y * 0.85 + cl.z * 0.55, 0.0, 1.2);
    vec2 shadOff = vec2(sunDir.x, sunDir.z) * 0.05;
    vec3 cs2 = cloudLayers(vec2(lat, lon) + shadOff);
    float shadow = clamp(cs2.x * 1.0 + cs2.y * 0.6 + cs2.z * 0.3, 0.0, 1.0);
    surface *= mix(1.0, 0.72, shadow * 0.7);

    // Cloud color tint by sun angle (golden at terminator, white in day).
    float ndlClouds = clamp(dot(n, sunDir), -0.2, 1.0);
    vec3 cloudColor = mix(vec3(0.55, 0.45, 0.40),
                          vec3(1.00, 0.99, 0.96),
                          smoothstep(-0.05, 0.50, ndlClouds));
    surface = mix(surface, cloudColor, clamp(cloudDensity, 0.0, 1.0) * 0.7);

    // ── Lighting ──
    float ndl   = dot(n, sunDir);
    float day   = smoothstep(-0.05, 0.40, ndl);
    float twi   = exp(-pow(ndl * 6.0, 2.0));
    vec3  twiC  = vec3(1.10, 0.55, 0.30);

    vec3 dayCol = surface * (0.20 + 1.00 * clamp(ndl, 0.0, 1.0));

    // Tight Mie ocean specular + glitter pattern.
    vec3  view  = vec3(0.0, 0.0, 1.0);
    vec3  halfv = normalize(sunDir + view);
    float ndh   = max(dot(n, halfv), 0.0);
    // Sharper sun glint with a subtle wave-modulated breakup.
    float waveJitter = 0.85 + 0.30 * waves;
    float spec  = pow(ndh, 320.0) * (1.0 - land) * day * (1.0 - cloudDensity * 0.85);
    // Wider, dimmer glitter ring (broader so it reads as "shimmer")
    float glit  = pow(ndh, 24.0)  * (1.0 - land) * day * (1.0 - cloudDensity * 0.85);
    dayCol += vec3(1.00, 0.98, 0.92) * spec * 2.4 * waveJitter;
    dayCol += vec3(1.00, 0.96, 0.85) * glit * 0.22 * (0.6 + 0.6 * waves);

    // Rayleigh blue scatter on day side (richer near the sub-solar point).
    float rayleigh = pow(clamp(ndl, 0.0, 1.0), 0.7);
    dayCol += vec3(0.10, 0.20, 0.36) * rayleigh * 0.20;
    dayCol += ocean * (1.0 - land) * day * 0.18;

    // ── Night side ──
    // Population density biased toward continents and coastlines.
    float coastBoost = smoothstep(0.50, 0.60, h) * (1.0 - smoothstep(0.60, 0.80, h));
    // Cluster lights more strongly than uniform noise.
    float urban = fbm(vec2(lon * 12.0, lat * 12.0) + u_time * 0.04, 5);
    urban = pow(smoothstep(0.55, 0.95, urban), 1.4);
    // Less city in deserts and at high latitudes.
    urban *= (1.0 - 0.4 * arid) * (1.0 - smoothstep(0.95, 1.30, aLat));
    float city = land * urban * (0.6 + 1.4 * coastBoost);

    vec3 nightCol = vec3(0.010, 0.018, 0.040)
                  + vec3(1.00, 0.78, 0.40) * city * 1.05
                  + vec3(0.10, 0.18, 0.30) * city * 0.30;
    nightCol += vec3(0.20, 0.25, 0.35) * cloudDensity * 0.18;

    // Aurora ovals at high latitudes (visible mostly on night side).
    float auroraBand = exp(-pow((aLat - 1.20) * 7.0, 2.0));
    float auroraNoise = fbm(vec2(lon * 4.0 + u_time * 0.05, lat * 8.0), 4);
    vec3  auroraCol = vec3(0.20, 1.00, 0.55) * 0.45
                    + vec3(0.40, 0.30, 1.00) * 0.25;
    nightCol += auroraCol * auroraBand * auroraNoise * 0.6 * (1.0 - day);

    vec3 col = mix(nightCol, dayCol, day);
    // Twilight rim glow on the surface itself
    col += twiC * twi * 0.45 * (1.0 - day);

    // Atmospheric rim (Rayleigh + Mie blend) — wider on day side.
    float rim    = pow(1.0 - nz, 2.4);
    float rimHi  = pow(1.0 - nz, 5.5);              // tight inner blue
    vec3  rimCol = mix(vec3(0.18, 0.32, 0.72), vec3(0.55, 0.78, 1.05), day);
    col += rimCol * rim   * (0.55 + 0.50 * day);
    col += vec3(0.30, 0.55, 1.10) * rimHi * (0.35 + 0.55 * day);
    // Ozone violet kiss right at the limb
    col += vec3(0.45, 0.30, 0.75) * pow(1.0 - nz, 8.0) * (0.4 + 0.6 * day) * 0.35;
    // Warm sunset rim near the terminator
    col += vec3(1.10, 0.55, 0.25) * rim * twi * 0.55;

    // Subtle limb darkening
    col *= mix(1.0, 0.78, smoothstep(0.85, 1.0, length(uv)));

    // ── 3-D landmark spires (drawn on top of the lit surface) ──
    // Each call: (lat°, lon°, height as fraction of R, cap color, seed phase).
    // Heights are exaggerated ~50× for visibility (real Burj Khalifa is ~0.0001 R).
    col = drawSpire(col, frag, u_center, R,  40.7128,  -74.0060, 0.030,
                    vec3(1.00, 0.55, 0.20), sunDir, spin, VIEW_LAT, day, 0.7);  // NYC
    col = drawSpire(col, frag, u_center, R,  48.8584,    2.2945, 0.026,
                    vec3(1.00, 0.85, 0.35), sunDir, spin, VIEW_LAT, day, 1.4);  // Eiffel
    col = drawSpire(col, frag, u_center, R,  51.5074,   -0.1278, 0.024,
                    vec3(0.95, 0.70, 0.30), sunDir, spin, VIEW_LAT, day, 2.1);  // London
    col = drawSpire(col, frag, u_center, R,  35.6586,  139.7454, 0.028,
                    vec3(1.00, 0.40, 0.45), sunDir, spin, VIEW_LAT, day, 2.8);  // Tokyo Tower
    col = drawSpire(col, frag, u_center, R, -33.8568,  151.2153, 0.022,
                    vec3(0.55, 0.90, 1.05), sunDir, spin, VIEW_LAT, day, 3.5);  // Sydney Opera
    col = drawSpire(col, frag, u_center, R, -22.9519,  -43.2105, 0.024,
                    vec3(1.00, 0.95, 0.80), sunDir, spin, VIEW_LAT, day, 4.2);  // Christ Redeemer
    col = drawSpire(col, frag, u_center, R,  29.9792,   31.1342, 0.022,
                    vec3(0.95, 0.80, 0.45), sunDir, spin, VIEW_LAT, day, 4.9);  // Giza Pyramids
    col = drawSpire(col, frag, u_center, R,  25.1972,   55.2744, 0.034,
                    vec3(0.65, 0.85, 1.10), sunDir, spin, VIEW_LAT, day, 5.6);  // Burj Khalifa
    col = drawSpire(col, frag, u_center, R,  27.9881,   86.9250, 0.038,
                    vec3(1.00, 1.00, 1.00), sunDir, spin, VIEW_LAT, day, 6.3);  // Everest
    col = drawSpire(col, frag, u_center, R,  37.8199, -122.4783, 0.020,
                    vec3(1.00, 0.45, 0.20), sunDir, spin, VIEW_LAT, day, 7.0);  // Golden Gate

    // ── Kenora pin (49.7666° N, 94.4870° W) ──
    const float K_LAT = radians(49.7666);
    const float K_LON = radians(-94.4870);
    vec3 kpos = vec3(cos(K_LAT) * sin(K_LON),
                     sin(K_LAT),
                     cos(K_LAT) * cos(K_LON));
    float kAlign = dot(nr, kpos);
    float kDot  = smoothstep(cos(radians(0.55)), cos(radians(0.18)), kAlign);
    float kHalo = smoothstep(cos(radians(2.50)), cos(radians(0.70)), kAlign);
    // Outer/inner ring annulus.
    float kRingOuter = smoothstep(cos(radians(1.10)), cos(radians(0.95)), kAlign);
    float kRingInner = smoothstep(cos(radians(0.85)), cos(radians(0.72)), kAlign);
    float kRing = clamp(kRingOuter - kRingInner, 0.0, 1.0);

    float kPulse = 0.55 + 0.45 * u_kenora_pulse;
    float kVis   = 0.65 + 0.35 * day; // visible day or night

    // Two-color pin: bright yellow core, cyan ring.
    vec3 kCore = vec3(1.00, 0.92, 0.45);
    vec3 kAura = vec3(1.00, 0.55, 0.35);
    vec3 kRingCol = vec3(0.55, 0.90, 1.05);

    col = mix(col, kAura, kHalo * 0.32 * kPulse * kVis);
    col += kRingCol * kRing * 1.10 * kVis;
    col += kCore * kDot * 1.40 * kVis;

    // Tonemap + gentle gamma
    col = col / (1.0 + col);
    col = pow(col, vec3(0.92));

    float edge = smoothstep(R, R - 1.5, dist);
    finalColor = vec4(col, edge * u_alpha);
}
