// ══════════════════════════════════════════════════════════════════
//  Simplex noise 2D — isotropic, O(n²) coherent noise
// ══════════════════════════════════════════════════════════════════

// Gradients for 2D simplex noise (12 directions on a unit circle-ish)
const GRAD2: [[f32; 2]; 12] = [
    [1.0, 1.0],
    [-1.0, 1.0],
    [1.0, -1.0],
    [-1.0, -1.0],
    [1.0, 0.0],
    [-1.0, 0.0],
    [0.0, 1.0],
    [0.0, -1.0],
    [1.0, 1.0],
    [-1.0, 1.0],
    [1.0, -1.0],
    [-1.0, -1.0],
];

// Permutation table (doubled for wrapping)
const PERM: [u8; 512] = {
    const BASE: [u8; 256] = [
        151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225, 140, 36, 103, 30,
        69, 142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148, 247, 120, 234, 75, 0, 26, 197, 62, 94,
        252, 219, 203, 117, 35, 11, 32, 57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171,
        168, 68, 175, 74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122, 60,
        211, 133, 230, 220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54, 65, 25, 63, 161, 1,
        216, 80, 73, 209, 76, 132, 187, 208, 89, 18, 169, 200, 196, 135, 130, 116, 188, 159, 86,
        164, 100, 109, 198, 173, 186, 3, 64, 52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118,
        126, 255, 82, 85, 212, 207, 206, 59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170,
        213, 119, 248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43, 172, 9, 129, 22, 39,
        253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185, 112, 104, 218, 246, 97, 228, 251, 34,
        242, 193, 238, 210, 144, 12, 191, 179, 162, 241, 81, 51, 145, 235, 249, 14, 239, 107, 49,
        192, 214, 31, 181, 199, 106, 157, 184, 84, 204, 176, 115, 121, 50, 45, 127, 4, 150, 254,
        138, 236, 205, 93, 222, 114, 67, 29, 24, 72, 243, 141, 128, 195, 78, 66, 215, 61, 156, 180,
    ];
    let mut t = [0u8; 512];
    let mut i = 0;
    while i < 512 {
        t[i] = BASE[i & 255];
        i += 1;
    }
    t
};

/// 2D Simplex noise. Returns a value in approximately [-1, 1].
/// Isotropic, no grid-axis bias, O(n²) complexity.
pub fn simplex_noise2d(x: f32, y: f32) -> f32 {
    const F2: f32 = 0.366025403784; // (sqrt(3) - 1) / 2
    const G2: f32 = 0.211324865405; // (3 - sqrt(3)) / 6

    // Skew input space to simplex cell
    let s = (x + y) * F2;
    let i = (x + s).floor();
    let j = (y + s).floor();

    let tt = (i + j) * G2;
    let x0 = x - (i - tt);
    let y0 = y - (j - tt);

    // Determine which simplex triangle we're in
    let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };

    let x1 = x0 - i1 as f32 + G2;
    let y1 = y0 - j1 as f32 + G2;
    let x2 = x0 - 1.0 + 2.0 * G2;
    let y2 = y0 - 1.0 + 2.0 * G2;

    let ii = (i as i32 & 255) as usize;
    let jj = (j as i32 & 255) as usize;

    // Gradient indices via permutation table
    let gi0 = (PERM[ii + PERM[jj] as usize] % 12) as usize;
    let gi1 = (PERM[ii + i1 + PERM[jj + j1] as usize] % 12) as usize;
    let gi2 = (PERM[ii + 1 + PERM[jj + 1] as usize] % 12) as usize;

    // Contribution from three corners
    let mut n0 = 0.0_f32;
    let t0 = 0.5 - x0 * x0 - y0 * y0;
    if t0 > 0.0 {
        let t0 = t0 * t0;
        n0 = t0 * t0 * (GRAD2[gi0][0] * x0 + GRAD2[gi0][1] * y0);
    }

    let mut n1 = 0.0_f32;
    let t1 = 0.5 - x1 * x1 - y1 * y1;
    if t1 > 0.0 {
        let t1 = t1 * t1;
        n1 = t1 * t1 * (GRAD2[gi1][0] * x1 + GRAD2[gi1][1] * y1);
    }

    let mut n2 = 0.0_f32;
    let t2 = 0.5 - x2 * x2 - y2 * y2;
    if t2 > 0.0 {
        let t2 = t2 * t2;
        n2 = t2 * t2 * (GRAD2[gi2][0] * x2 + GRAD2[gi2][1] * y2);
    }

    // Scale to [-1, 1]
    70.0 * (n0 + n1 + n2)
}

/// Fractional Brownian motion — layered octaves of Simplex noise.
/// Returns a value roughly in [-amplitude, amplitude].
///
/// * `x`, `y`     — sample coordinates (position + time)
/// * `octaves`    — number of noise layers (3–6; 4 is ideal for 60fps)
/// * `lacunarity` — frequency multiplier per octave (~2.0)
/// * `gain`       — amplitude decay per octave (~0.5)
/// * `amplitude`  — initial amplitude scale
/// * `frequency`  — initial frequency scale
pub fn fbm2d(
    x: f32,
    y: f32,
    octaves: u32,
    lacunarity: f32,
    gain: f32,
    amplitude: f32,
    frequency: f32,
) -> f32 {
    let mut sum = 0.0_f32;
    let mut amp = amplitude;
    let mut freq = frequency;
    for _ in 0..octaves {
        sum += simplex_noise2d(x * freq, y * freq) * amp;
        freq *= lacunarity;
        amp *= gain;
    }
    sum
}

/// Domain-warped fBM — nested noise evaluations for liquid/swirling textures.
/// displacement = fbm(x + fbm(x + fbm(x, y), y), y)
/// Creates complex, organic movement that breaks the predictability of plain fBM.
pub fn fbm2d_warped(
    x: f32,
    y: f32,
    octaves: u32,
    lacunarity: f32,
    gain: f32,
    amplitude: f32,
    frequency: f32,
) -> f32 {
    // First warp layer — subtle coordinate distortion
    let warp1_x = fbm2d(x + 1.7, y + 9.2, octaves, lacunarity, gain, 1.0, frequency);
    let warp1_y = fbm2d(x + 8.3, y + 2.8, octaves, lacunarity, gain, 1.0, frequency);
    // Second warp layer — nested distortion for deeper liquid feel
    let warp2_x = fbm2d(
        x + warp1_x * 0.6 + 4.3,
        y + warp1_y * 0.6 + 1.7,
        octaves,
        lacunarity,
        gain,
        1.0,
        frequency,
    );
    let warp2_y = fbm2d(
        x + warp1_x * 0.6 + 7.1,
        y + warp1_y * 0.6 + 3.9,
        octaves,
        lacunarity,
        gain,
        1.0,
        frequency,
    );
    // Final evaluation at warped coordinates
    fbm2d(
        x + warp2_x * 0.4,
        y + warp2_y * 0.4,
        octaves,
        lacunarity,
        gain,
        amplitude,
        frequency,
    )
}
