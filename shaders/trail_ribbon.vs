#version 330
// ═══════════════════════════════════════════════════════════════
//  Trail Ribbon Vertex Shader — Catmull-Rom spline + perpendicular
//  width computed from ring buffer control points.
// ═══════════════════════════════════════════════════════════════

in vec3 vertexPosition;  // x=parameter along spine, y=side(-1/+1), z=unused
in vec2 vertexTexCoord;

// Ring buffer control points passed as a uniform array
// Each vec4 = (x, y, age, speed)
uniform vec4 u_trail_points[48];
uniform int  u_trail_head;
uniform int  u_trail_count;
uniform vec2 u_resolution;
uniform float u_max_age;
uniform float u_orb_scale;

out float fLife;
out float fSpeed;
out float fWidth;

// Catmull-Rom interpolation
vec2 catmull_rom(vec2 p0, vec2 p1, vec2 p2, vec2 p3, float t) {
    float t2 = t * t;
    float t3 = t2 * t;
    return 0.5 * (
        2.0 * p1 +
        (-p0 + p2) * t +
        (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 +
        (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3
    );
}

void main() {
    // vertexPosition.x = parameter [0, trail_count-1] (which segment)
    // vertexPosition.y = side: -1 or +1 (left/right of spine)
    int seg = int(vertexPosition.x);
    float sub_t = fract(vertexPosition.x);
    float side = vertexPosition.y;

    // Sample four control points for Catmull-Rom
    int i0 = max(seg - 1, 0);
    int i1 = seg;
    int i2 = min(seg + 1, u_trail_count - 1);
    int i3 = min(seg + 2, u_trail_count - 1);

    // Map indices through ring buffer (oldest first)
    int idx0 = (u_trail_head + i0) % u_trail_count;
    int idx1 = (u_trail_head + i1) % u_trail_count;
    int idx2 = (u_trail_head + i2) % u_trail_count;
    int idx3 = (u_trail_head + i3) % u_trail_count;

    vec2 p0 = u_trail_points[idx0].xy;
    vec2 p1 = u_trail_points[idx1].xy;
    vec2 p2 = u_trail_points[idx2].xy;
    vec2 p3 = u_trail_points[idx3].xy;

    // Interpolated spine position
    vec2 pos = catmull_rom(p0, p1, p2, p3, sub_t);

    // Tangent (derivative of Catmull-Rom)
    vec2 tangent = normalize(
        0.5 * ((-p0 + p2) +
               2.0 * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * sub_t +
               3.0 * (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * sub_t * sub_t)
    );

    // Perpendicular (normal)
    vec2 normal = vec2(-tangent.y, tangent.x);

    // Width modulated by speed and age
    float age = mix(u_trail_points[idx1].z, u_trail_points[idx2].z, sub_t);
    float speed = mix(u_trail_points[idx1].w, u_trail_points[idx2].w, sub_t);
    float life = clamp(age / u_max_age, 0.0, 1.0);
    float speed_factor = clamp(speed / 200.0, 0.0, 1.0);
    float width = (8.0 + speed_factor * 18.0) * (1.0 - life) * u_orb_scale;

    // Offset position along the perpendicular
    vec2 final_pos = pos + normal * side * width;

    // Convert to NDC
    vec2 ndc = (final_pos / u_resolution) * 2.0 - 1.0;
    ndc.y = -ndc.y;

    gl_Position = vec4(ndc, 0.0, 1.0);
    fLife = life;
    fSpeed = speed_factor;
    fWidth = side;
}
