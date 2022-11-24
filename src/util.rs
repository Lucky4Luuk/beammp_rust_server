pub fn lerp<const N: usize>(a: &[f64; N], b: &[f64; N], t: f64) -> [f64; N] {
    let mut out = [0f64; N];
    for i in 0..N {
        out[i] = a[i] + (b[i] - a[i]) * t;
    }
    out
}

pub fn lerp32<const N: usize>(a: &[f32; N], b: &[f32; N], t: f32) -> [f32; N] {
    let mut out = [0f32; N];
    for i in 0..N {
        out[i] = a[i] + (b[i] - a[i]) * t;
    }
    out
}

pub fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((b[0] - a[0]).powf(2.0) + (b[1] - a[1]).powf(2.0)).sqrt()
}

pub fn distance3d(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((b[0] - a[0]).powf(2.0) + (b[1] - a[1]).powf(2.0) + (b[2] - a[2]).powf(2.0)).sqrt()
}
