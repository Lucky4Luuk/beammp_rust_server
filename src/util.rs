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
