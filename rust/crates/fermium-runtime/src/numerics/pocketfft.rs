//! NumPy's complex FFT, ported operation for operation from pocketfft's C++ header (`pocketfft_hdronly.h`,
//! Max-Planck-Society, BSD-3-Clause), which `numpy.fft` 2.x uses: the FFTPACK-style passes with radix 2, 3,
//! 4, 5, 7, 8, 11 and a generic odd pass (`cfftp`), Bluestein's algorithm for lengths with a large prime factor
//! (`fftblue`), the choice between them (`pocketfft_c`) and the twiddle factors (`sincos_2pibyn`). v1's FFT
//! built-ins are numpy.fft calls, so this gives the same numbers to the last bit (up to libm's sin/cos).

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cx {
    pub r: f64,
    pub i: f64,
}

impl Cx {
    pub const fn new(r: f64, i: f64) -> Cx {
        Cx { r, i }
    }
}

impl std::ops::Add for Cx {
    type Output = Cx;
    fn add(self, o: Cx) -> Cx {
        Cx::new(self.r + o.r, self.i + o.i)
    }
}

impl std::ops::Sub for Cx {
    type Output = Cx;
    fn sub(self, o: Cx) -> Cx {
        Cx::new(self.r - o.r, self.i - o.i)
    }
}

impl std::ops::Mul<f64> for Cx {
    type Output = Cx;
    fn mul(self, o: f64) -> Cx {
        Cx::new(self.r * o, self.i * o)
    }
}

/// special_mul<fwd>: v1 × v2 (backward) or v1 × conj(v2) (forward)
#[inline]
fn smul(fwd: bool, v1: Cx, v2: Cx) -> Cx {
    if fwd {
        Cx::new(v1.r * v2.r + v1.i * v2.i, v1.i * v2.r - v1.r * v2.i)
    } else {
        Cx::new(v1.r * v2.r - v1.i * v2.i, v1.r * v2.i + v1.i * v2.r)
    }
}

#[inline]
fn pm(c: Cx, d: Cx) -> (Cx, Cx) {
    (c + d, c - d)
}

#[inline]
fn rotx90(fwd: bool, a: Cx) -> Cx {
    if fwd { Cx::new(a.i, -a.r) } else { Cx::new(-a.i, a.r) }
}

const HSQT2: f64 = 0.707106781186547524400844362104849;

#[inline]
fn rotx45(fwd: bool, a: Cx) -> Cx {
    if fwd { Cx::new(HSQT2 * (a.r + a.i), HSQT2 * (a.i - a.r)) } else { Cx::new(HSQT2 * (a.r - a.i), HSQT2 * (a.i + a.r)) }
}

#[inline]
fn rotx135(fwd: bool, a: Cx) -> Cx {
    if fwd {
        Cx::new(HSQT2 * (a.i - a.r), HSQT2 * (-a.r - a.i))
    } else {
        Cx::new(HSQT2 * (-a.r - a.i), HSQT2 * (a.r - a.i))
    }
}

// ------------------------------------------------------------------ twiddles (sincos_2pibyn)
struct SinCos2PiByN {
    n: usize,
    mask: usize,
    shift: usize,
    v1: Vec<Cx>,
    v2: Vec<Cx>,
}

impl SinCos2PiByN {
    fn calc(x: usize, n: usize, ang: f64) -> Cx {
        let mut x = x << 3;
        if x < 4 * n {
            if x < 2 * n {
                if x < n {
                    return Cx::new((x as f64 * ang).cos(), (x as f64 * ang).sin());
                }
                return Cx::new(((2 * n - x) as f64 * ang).sin(), ((2 * n - x) as f64 * ang).cos());
            }
            x -= 2 * n;
            if x < n {
                return Cx::new(-(x as f64 * ang).sin(), (x as f64 * ang).cos());
            }
            return Cx::new(-((2 * n - x) as f64 * ang).cos(), ((2 * n - x) as f64 * ang).sin());
        }
        x = 8 * n - x;
        if x < 2 * n {
            if x < n {
                return Cx::new((x as f64 * ang).cos(), -(x as f64 * ang).sin());
            }
            return Cx::new(((2 * n - x) as f64 * ang).sin(), -((2 * n - x) as f64 * ang).cos());
        }
        x -= 2 * n;
        if x < n {
            return Cx::new(-(x as f64 * ang).sin(), -(x as f64 * ang).cos());
        }
        Cx::new(-((2 * n - x) as f64 * ang).cos(), -((2 * n - x) as f64 * ang).sin())
    }

    fn new(n: usize) -> Self {
        // Thigh(0.25L*pi/n): the long double quotient rounded to double
        let ang = quarter_pi_over(n);
        let nval = (n + 2) / 2;
        let mut shift = 1;
        while (1usize << shift) * (1usize << shift) < nval {
            shift += 1;
        }
        let mask = (1usize << shift) - 1;
        let mut v1 = vec![Cx::new(1.0, 0.0); mask + 1];
        for (i, v) in v1.iter_mut().enumerate().skip(1) {
            *v = Self::calc(i, n, ang);
        }
        let mut v2 = vec![Cx::new(1.0, 0.0); nval.div_ceil(mask + 1)];
        for (i, v) in v2.iter_mut().enumerate().skip(1) {
            *v = Self::calc(i * (mask + 1), n, ang);
        }
        SinCos2PiByN { n, mask, shift, v1, v2 }
    }

    fn get(&self, idx: usize) -> Cx {
        if 2 * idx <= self.n {
            let (x1, x2) = (self.v1[idx & self.mask], self.v2[idx >> self.shift]);
            return Cx::new(x1.r * x2.r - x1.i * x2.i, x1.r * x2.i + x1.i * x2.r);
        }
        let idx = self.n - idx;
        let (x1, x2) = (self.v1[idx & self.mask], self.v2[idx >> self.shift]);
        Cx::new(x1.r * x2.r - x1.i * x2.i, -(x1.r * x2.i + x1.i * x2.r))
    }
}

/// (π/4)/n computed in x87 long double (64-bit mantissa) and rounded to double, as `Thigh(0.25L*pi/n)`.
fn quarter_pi_over(n: usize) -> f64 {
    // π/4 to ~106 bits as a double-double; the quotient by n in double-double, rounded once to double.
    // (Rounding long double → double twice could differ in rare ties; not observed for n < 2^40.)
    const QP_HI: f64 = 0.7853981633974483;
    const QP_LO: f64 = 3.061616997868383e-17;
    let nf = n as f64;
    let q1 = QP_HI / nf;
    let r = (-q1).mul_add(nf, QP_HI);
    q1 + (r + QP_LO) / nf
}

// ------------------------------------------------------------------ cfftp
struct Fct {
    fct: usize,
    tw: Vec<Cx>,
    tws: Vec<Cx>,
}

pub struct Cfftp {
    length: usize,
    fact: Vec<Fct>,
}

impl Cfftp {
    pub fn new(length: usize) -> Cfftp {
        let mut p = Cfftp { length, fact: vec![] };
        if length <= 1 {
            return p;
        }
        p.factorize();
        p.comp_twiddle();
        p
    }

    fn factorize(&mut self) {
        let mut len = self.length;
        let mut f: Vec<usize> = vec![];
        while len & 7 == 0 {
            f.push(8);
            len >>= 3;
        }
        while len & 3 == 0 {
            f.push(4);
            len >>= 2;
        }
        if len & 1 == 0 {
            len >>= 1;
            f.push(2);
            let last = f.len() - 1;
            f.swap(0, last); // factor 2 at the front
        }
        let mut d = 3;
        while d * d <= len {
            while len % d == 0 {
                f.push(d);
                len /= d;
            }
            d += 2;
        }
        if len > 1 {
            f.push(len);
        }
        self.fact = f.into_iter().map(|fct| Fct { fct, tw: vec![], tws: vec![] }).collect();
    }

    fn comp_twiddle(&mut self) {
        let tw = SinCos2PiByN::new(self.length);
        let mut l1 = 1;
        let n = self.length;
        for f in &mut self.fact {
            let ip = f.fct;
            let ido = n / (l1 * ip);
            f.tw = vec![Cx::default(); (ip - 1) * (ido - 1)];
            for j in 1..ip {
                for i in 1..ido {
                    f.tw[(j - 1) * (ido - 1) + i - 1] = tw.get(j * l1 * i);
                }
            }
            if ip > 11 {
                f.tws = (0..ip).map(|j| tw.get(j * l1 * ido)).collect();
            }
            l1 *= ip;
        }
    }

    /// The transform in place, times fct (forward: e^{−2πi jk/n}).
    pub fn exec(&self, c: &mut [Cx], fct: f64, fwd: bool) {
        let len = self.length;
        if len == 1 {
            c[0] = c[0] * fct;
            return;
        }
        let mut l1 = 1;
        let mut buf = vec![Cx::default(); len];
        // p1 is `c` when in_c, else `buf`
        let mut in_c = true;
        for f in &self.fact {
            let ip = f.fct;
            let l2 = ip * l1;
            let ido = len / l2;
            {
                let (p1, p2): (&mut [Cx], &mut [Cx]) = if in_c { (&mut *c, &mut buf) } else { (&mut buf, &mut *c) };
                match ip {
                    4 => pass4(fwd, ido, l1, p1, p2, &f.tw),
                    8 => pass8(fwd, ido, l1, p1, p2, &f.tw),
                    2 => pass2(fwd, ido, l1, p1, p2, &f.tw),
                    3 => pass3(fwd, ido, l1, p1, p2, &f.tw),
                    5 => pass5(fwd, ido, l1, p1, p2, &f.tw),
                    7 => pass7(fwd, ido, l1, p1, p2, &f.tw),
                    11 => pass11(fwd, ido, l1, p1, p2, &f.tw),
                    _ => {
                        passg(fwd, ido, ip, l1, p1, p2, &f.tw, &f.tws);
                        in_c = !in_c; // the result is back in p1
                    }
                }
            }
            in_c = !in_c;
            l1 = l2;
        }
        if !in_c {
            if fct != 1.0 {
                for i in 0..len {
                    c[i] = buf[i] * fct;
                }
            } else {
                c.copy_from_slice(&buf);
            }
        } else if fct != 1.0 {
            for x in c.iter_mut() {
                *x = *x * fct;
            }
        }
    }
}

fn pass2(fwd: bool, ido: usize, l1: usize, cc: &[Cx], ch: &mut [Cx], wa: &[Cx]) {
    let chi = |a: usize, b: usize, c: usize| a + ido * (b + l1 * c);
    let cci = |a: usize, b: usize, c: usize| a + ido * (b + 2 * c);
    let waf = |x: usize, i: usize| wa[i - 1 + x * (ido - 1)];
    for k in 0..l1 {
        ch[chi(0, k, 0)] = cc[cci(0, 0, k)] + cc[cci(0, 1, k)];
        ch[chi(0, k, 1)] = cc[cci(0, 0, k)] - cc[cci(0, 1, k)];
        for i in 1..ido {
            ch[chi(i, k, 0)] = cc[cci(i, 0, k)] + cc[cci(i, 1, k)];
            ch[chi(i, k, 1)] = smul(fwd, cc[cci(i, 0, k)] - cc[cci(i, 1, k)], waf(0, i));
        }
    }
}

fn pass3(fwd: bool, ido: usize, l1: usize, cc: &[Cx], ch: &mut [Cx], wa: &[Cx]) {
    let tw1r = -0.5;
    let tw1i = (if fwd { -1.0 } else { 1.0 }) * 0.8660254037844386467637231707529362;
    let chi = |a: usize, b: usize, c: usize| a + ido * (b + l1 * c);
    let cci = |a: usize, b: usize, c: usize| a + ido * (b + 3 * c);
    let waf = |x: usize, i: usize| wa[i - 1 + x * (ido - 1)];
    for k in 0..l1 {
        for i in 0..ido {
            let t0 = cc[cci(i, 0, k)];
            let (t1, t2) = pm(cc[cci(i, 1, k)], cc[cci(i, 2, k)]);
            ch[chi(i, k, 0)] = t0 + t1;
            let ca = t0 + t1 * tw1r;
            let cb = Cx::new(-t2.i * tw1i, t2.r * tw1i);
            if i == 0 {
                let (a, b) = pm(ca, cb);
                ch[chi(0, k, 1)] = a;
                ch[chi(0, k, 2)] = b;
            } else {
                ch[chi(i, k, 1)] = smul(fwd, ca + cb, waf(0, i));
                ch[chi(i, k, 2)] = smul(fwd, ca - cb, waf(1, i));
            }
        }
    }
}

fn pass4(fwd: bool, ido: usize, l1: usize, cc: &[Cx], ch: &mut [Cx], wa: &[Cx]) {
    let chi = |a: usize, b: usize, c: usize| a + ido * (b + l1 * c);
    let cci = |a: usize, b: usize, c: usize| a + ido * (b + 4 * c);
    let waf = |x: usize, i: usize| wa[i - 1 + x * (ido - 1)];
    for k in 0..l1 {
        {
            let (t2, t1) = pm(cc[cci(0, 0, k)], cc[cci(0, 2, k)]);
            let (t3, t4) = pm(cc[cci(0, 1, k)], cc[cci(0, 3, k)]);
            let t4 = rotx90(fwd, t4);
            let (a, b) = pm(t2, t3);
            ch[chi(0, k, 0)] = a;
            ch[chi(0, k, 2)] = b;
            let (a, b) = pm(t1, t4);
            ch[chi(0, k, 1)] = a;
            ch[chi(0, k, 3)] = b;
        }
        for i in 1..ido {
            let (cc0, cc1, cc2, cc3) = (cc[cci(i, 0, k)], cc[cci(i, 1, k)], cc[cci(i, 2, k)], cc[cci(i, 3, k)]);
            let (t2, t1) = pm(cc0, cc2);
            let (t3, t4) = pm(cc1, cc3);
            let t4 = rotx90(fwd, t4);
            ch[chi(i, k, 0)] = t2 + t3;
            ch[chi(i, k, 1)] = smul(fwd, t1 + t4, waf(0, i));
            ch[chi(i, k, 2)] = smul(fwd, t2 - t3, waf(1, i));
            ch[chi(i, k, 3)] = smul(fwd, t1 - t4, waf(2, i));
        }
    }
}

fn pass5(fwd: bool, ido: usize, l1: usize, cc: &[Cx], ch: &mut [Cx], wa: &[Cx]) {
    let s = if fwd { -1.0 } else { 1.0 };
    let tw1r = 0.3090169943749474241022934171828191;
    let tw1i = s * 0.9510565162951535721164393333793821;
    let tw2r = -0.8090169943749474241022934171828191;
    let tw2i = s * 0.5877852522924731291687059546390728;
    let chi = |a: usize, b: usize, c: usize| a + ido * (b + l1 * c);
    let cci = |a: usize, b: usize, c: usize| a + ido * (b + 5 * c);
    let waf = |x: usize, i: usize| wa[i - 1 + x * (ido - 1)];
    for k in 0..l1 {
        for i in 0..ido {
            let t0 = cc[cci(i, 0, k)];
            let (t1, t4) = pm(cc[cci(i, 1, k)], cc[cci(i, 4, k)]);
            let (t2, t3) = pm(cc[cci(i, 2, k)], cc[cci(i, 3, k)]);
            ch[chi(i, k, 0)] = Cx::new(t0.r + t1.r + t2.r, t0.i + t1.i + t2.i);
            // (u1, u2, twar, twbr, twai, twbi)
            for &(u1, u2, twar, twbr, twai, twbi) in
                &[(1usize, 4usize, tw1r, tw2r, tw1i, tw2i), (2, 3, tw2r, tw1r, tw2i, -tw1i)] {
                let ca = Cx::new(t0.r + twar * t1.r + twbr * t2.r, t0.i + twar * t1.i + twbr * t2.i);
                let cb = Cx::new(-(twai * t4.i + twbi * t3.i), twai * t4.r + twbi * t3.r);
                if i == 0 {
                    let (a, b) = pm(ca, cb);
                    ch[chi(0, k, u1)] = a;
                    ch[chi(0, k, u2)] = b;
                } else {
                    ch[chi(i, k, u1)] = smul(fwd, ca + cb, waf(u1 - 1, i));
                    ch[chi(i, k, u2)] = smul(fwd, ca - cb, waf(u2 - 1, i));
                }
            }
        }
    }
}

fn pass7(fwd: bool, ido: usize, l1: usize, cc: &[Cx], ch: &mut [Cx], wa: &[Cx]) {
    let s = if fwd { -1.0 } else { 1.0 };
    let tw1r = 0.6234898018587335305250048840042398;
    let tw1i = s * 0.7818314824680298087084445266740578;
    let tw2r = -0.2225209339563144042889025644967948;
    let tw2i = s * 0.9749279121818236070181316829939312;
    let tw3r = -0.9009688679024191262361023195074451;
    let tw3i = s * 0.433883739117558120475768332848359;
    let chi = |a: usize, b: usize, c: usize| a + ido * (b + l1 * c);
    let cci = |a: usize, b: usize, c: usize| a + ido * (b + 7 * c);
    let waf = |x: usize, i: usize| wa[i - 1 + x * (ido - 1)];
    let steps = [(1usize, 6usize, tw1r, tw2r, tw3r, tw1i, tw2i, tw3i), (2, 5, tw2r, tw3r, tw1r, tw2i, -tw3i, -tw1i),
                 (3, 4, tw3r, tw1r, tw2r, tw3i, -tw1i, tw2i)];
    for k in 0..l1 {
        for i in 0..ido {
            let t1 = cc[cci(i, 0, k)];
            let (t2, t7) = pm(cc[cci(i, 1, k)], cc[cci(i, 6, k)]);
            let (t3, t6) = pm(cc[cci(i, 2, k)], cc[cci(i, 5, k)]);
            let (t4, t5) = pm(cc[cci(i, 3, k)], cc[cci(i, 4, k)]);
            ch[chi(i, k, 0)] = Cx::new(t1.r + t2.r + t3.r + t4.r, t1.i + t2.i + t3.i + t4.i);
            for &(u1, u2, x1, x2, x3, y1, y2, y3) in &steps {
                let ca = Cx::new(t1.r + x1 * t2.r + x2 * t3.r + x3 * t4.r, t1.i + x1 * t2.i + x2 * t3.i + x3 * t4.i);
                let cb = Cx::new(-(y1 * t7.i + y2 * t6.i + y3 * t5.i), y1 * t7.r + y2 * t6.r + y3 * t5.r);
                let (da, db) = pm(ca, cb);
                if i == 0 {
                    ch[chi(0, k, u1)] = da;
                    ch[chi(0, k, u2)] = db;
                } else {
                    ch[chi(i, k, u1)] = smul(fwd, da, waf(u1 - 1, i));
                    ch[chi(i, k, u2)] = smul(fwd, db, waf(u2 - 1, i));
                }
            }
        }
    }
}

fn pass8(fwd: bool, ido: usize, l1: usize, cc: &[Cx], ch: &mut [Cx], wa: &[Cx]) {
    let chi = |a: usize, b: usize, c: usize| a + ido * (b + l1 * c);
    let cci = |a: usize, b: usize, c: usize| a + ido * (b + 8 * c);
    let waf = |x: usize, i: usize| wa[i - 1 + x * (ido - 1)];
    for k in 0..l1 {
        {
            let (a1, a5) = pm(cc[cci(0, 1, k)], cc[cci(0, 5, k)]);
            let (a3, a7) = pm(cc[cci(0, 3, k)], cc[cci(0, 7, k)]);
            let (a1, a3) = pm(a1, a3); // PMINPLACE
            let a3 = rotx90(fwd, a3);
            let a7 = rotx90(fwd, a7);
            let (a5, a7) = pm(a5, a7);
            let a5 = rotx45(fwd, a5);
            let a7 = rotx135(fwd, a7);
            let (a0, a4) = pm(cc[cci(0, 0, k)], cc[cci(0, 4, k)]);
            let (a2, a6) = pm(cc[cci(0, 2, k)], cc[cci(0, 6, k)]);
            let (x, y) = pm(a0 + a2, a1);
            ch[chi(0, k, 0)] = x;
            ch[chi(0, k, 4)] = y;
            let (x, y) = pm(a0 - a2, a3);
            ch[chi(0, k, 2)] = x;
            ch[chi(0, k, 6)] = y;
            let a6 = rotx90(fwd, a6);
            let (x, y) = pm(a4 + a6, a5);
            ch[chi(0, k, 1)] = x;
            ch[chi(0, k, 5)] = y;
            let (x, y) = pm(a4 - a6, a7);
            ch[chi(0, k, 3)] = x;
            ch[chi(0, k, 7)] = y;
        }
        for i in 1..ido {
            let (a1, a5) = pm(cc[cci(i, 1, k)], cc[cci(i, 5, k)]);
            let (a3, a7) = pm(cc[cci(i, 3, k)], cc[cci(i, 7, k)]);
            let a7 = rotx90(fwd, a7);
            let (a1, a3) = pm(a1, a3);
            let a3 = rotx90(fwd, a3);
            let (a5, a7) = pm(a5, a7);
            let a5 = rotx45(fwd, a5);
            let a7 = rotx135(fwd, a7);
            let (a0, a4) = pm(cc[cci(i, 0, k)], cc[cci(i, 4, k)]);
            let (a2, a6) = pm(cc[cci(i, 2, k)], cc[cci(i, 6, k)]);
            let (a0, a2) = pm(a0, a2);
            ch[chi(i, k, 0)] = a0 + a1;
            ch[chi(i, k, 4)] = smul(fwd, a0 - a1, waf(3, i));
            ch[chi(i, k, 2)] = smul(fwd, a2 + a3, waf(1, i));
            ch[chi(i, k, 6)] = smul(fwd, a2 - a3, waf(5, i));
            let a6 = rotx90(fwd, a6);
            let (a4, a6) = pm(a4, a6);
            ch[chi(i, k, 1)] = smul(fwd, a4 + a5, waf(0, i));
            ch[chi(i, k, 5)] = smul(fwd, a4 - a5, waf(4, i));
            ch[chi(i, k, 3)] = smul(fwd, a6 + a7, waf(2, i));
            ch[chi(i, k, 7)] = smul(fwd, a6 - a7, waf(6, i));
        }
    }
}

fn pass11(fwd: bool, ido: usize, l1: usize, cc: &[Cx], ch: &mut [Cx], wa: &[Cx]) {
    let s = if fwd { -1.0 } else { 1.0 };
    let (t1r, t1i) = (0.8412535328311811688618116489193677, s * 0.5406408174555975821076359543186917);
    let (t2r, t2i) = (0.4154150130018864255292741492296232, s * 0.9096319953545183714117153830790285);
    let (t3r, t3i) = (-0.1423148382732851404437926686163697, s * 0.9898214418809327323760920377767188);
    let (t4r, t4i) = (-0.6548607339452850640569250724662936, s * 0.7557495743542582837740358439723444);
    let (t5r, t5i) = (-0.9594929736144973898903680570663277, s * 0.2817325568414296977114179153466169);
    let chi = |a: usize, b: usize, c: usize| a + ido * (b + l1 * c);
    let cci = |a: usize, b: usize, c: usize| a + ido * (b + 11 * c);
    let waf = |x: usize, i: usize| wa[i - 1 + x * (ido - 1)];
    let steps: [(usize, usize, [f64; 5], [f64; 5]); 5] = [
        (1, 10, [t1r, t2r, t3r, t4r, t5r], [t1i, t2i, t3i, t4i, t5i]),
        (2, 9, [t2r, t4r, t5r, t3r, t1r], [t2i, t4i, -t5i, -t3i, -t1i]),
        (3, 8, [t3r, t5r, t2r, t1r, t4r], [t3i, -t5i, -t2i, t1i, t4i]),
        (4, 7, [t4r, t3r, t1r, t5r, t2r], [t4i, -t3i, t1i, t5i, -t2i]),
        (5, 6, [t5r, t1r, t4r, t2r, t3r], [t5i, -t1i, t4i, -t2i, t3i]),
    ];
    for k in 0..l1 {
        for i in 0..ido {
            let t1 = cc[cci(i, 0, k)];
            let (t2, t11) = pm(cc[cci(i, 1, k)], cc[cci(i, 10, k)]);
            let (t3, t10) = pm(cc[cci(i, 2, k)], cc[cci(i, 9, k)]);
            let (t4, t9) = pm(cc[cci(i, 3, k)], cc[cci(i, 8, k)]);
            let (t5, t8) = pm(cc[cci(i, 4, k)], cc[cci(i, 7, k)]);
            let (t6, t7) = pm(cc[cci(i, 5, k)], cc[cci(i, 6, k)]);
            ch[chi(i, k, 0)] = Cx::new(t1.r + t2.r + t3.r + t4.r + t5.r + t6.r, t1.i + t2.i + t3.i + t4.i + t5.i + t6.i);
            for (u1, u2, x, y) in &steps {
                let ca = t1 + t2 * x[0] + t3 * x[1] + t4 * x[2] + t5 * x[3] + t6 * x[4];
                let cb = Cx::new(-(y[0] * t11.i + y[1] * t10.i + y[2] * t9.i + y[3] * t8.i + y[4] * t7.i),
                                 y[0] * t11.r + y[1] * t10.r + y[2] * t9.r + y[3] * t8.r + y[4] * t7.r);
                let (da, db) = pm(ca, cb);
                if i == 0 {
                    ch[chi(0, k, *u1)] = da;
                    ch[chi(0, k, *u2)] = db;
                } else {
                    ch[chi(i, k, *u1)] = smul(fwd, da, waf(u1 - 1, i));
                    ch[chi(i, k, *u2)] = smul(fwd, db, waf(u2 - 1, i));
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn passg(fwd: bool, ido: usize, ip: usize, l1: usize, cc: &mut [Cx], ch: &mut [Cx], wa: &[Cx], csarr: &[Cx]) {
    let cdim = ip;
    let ipph = ip.div_ceil(2);
    let idl1 = ido * l1;
    let chi = |a: usize, b: usize, c: usize| a + ido * (b + l1 * c);
    let cci = |a: usize, b: usize, c: usize| a + ido * (b + cdim * c);
    let cxi = |a: usize, b: usize, c: usize| a + ido * (b + l1 * c);
    let cx2 = |a: usize, b: usize| a + idl1 * b;
    let ch2 = |a: usize, b: usize| a + idl1 * b;
    let mut wal = vec![Cx::new(1.0, 0.0); ip];
    for i in 1..ip {
        wal[i] = Cx::new(csarr[i].r, if fwd { -csarr[i].i } else { csarr[i].i });
    }
    for k in 0..l1 {
        for i in 0..ido {
            ch[chi(i, k, 0)] = cc[cci(i, 0, k)];
        }
    }
    let (mut j, mut jc) = (1, ip - 1);
    while j < ipph {
        for k in 0..l1 {
            for i in 0..ido {
                let (a, b) = pm(cc[cci(i, j, k)], cc[cci(i, jc, k)]);
                ch[chi(i, k, j)] = a;
                ch[chi(i, k, jc)] = b;
            }
        }
        j += 1;
        jc -= 1;
    }
    for k in 0..l1 {
        for i in 0..ido {
            let mut tmp = ch[chi(i, k, 0)];
            for j in 1..ipph {
                tmp = tmp + ch[chi(i, k, j)];
            }
            cc[cxi(i, k, 0)] = tmp;
        }
    }
    let (mut l, mut lc) = (1, ip - 1);
    while l < ipph {
        for ik in 0..idl1 {
            let (h0, h1, h2) = (ch[ch2(ik, 0)], ch[ch2(ik, 1)], ch[ch2(ik, 2)]);
            let (hm1, hm2) = (ch[ch2(ik, ip - 1)], ch[ch2(ik, ip - 2)]);
            cc[cx2(ik, l)] = Cx::new(h0.r + wal[l].r * h1.r + wal[2 * l].r * h2.r,
                                     h0.i + wal[l].r * h1.i + wal[2 * l].r * h2.i);
            cc[cx2(ik, lc)] = Cx::new(-wal[l].i * hm1.i - wal[2 * l].i * hm2.i, wal[l].i * hm1.r + wal[2 * l].i * hm2.r);
        }
        let mut iwal = 2 * l;
        let (mut j, mut jc) = (3, ip - 3);
        while j + 1 < ipph {
            iwal += l;
            if iwal > ip {
                iwal -= ip;
            }
            let xwal = wal[iwal];
            iwal += l;
            if iwal > ip {
                iwal -= ip;
            }
            let xwal2 = wal[iwal];
            for ik in 0..idl1 {
                let (hj, hj1, hjc, hjc1) = (ch[ch2(ik, j)], ch[ch2(ik, j + 1)], ch[ch2(ik, jc)], ch[ch2(ik, jc - 1)]);
                let a = &mut cc[cx2(ik, l)];
                a.r += hj.r * xwal.r + hj1.r * xwal2.r;
                a.i += hj.i * xwal.r + hj1.i * xwal2.r;
                let b = &mut cc[cx2(ik, lc)];
                b.r -= hjc.i * xwal.i + hjc1.i * xwal2.i;
                b.i += hjc.r * xwal.i + hjc1.r * xwal2.i;
            }
            j += 2;
            jc -= 2;
        }
        while j < ipph {
            iwal += l;
            if iwal > ip {
                iwal -= ip;
            }
            let xwal = wal[iwal];
            for ik in 0..idl1 {
                let (hj, hjc) = (ch[ch2(ik, j)], ch[ch2(ik, jc)]);
                let a = &mut cc[cx2(ik, l)];
                a.r += hj.r * xwal.r;
                a.i += hj.i * xwal.r;
                let b = &mut cc[cx2(ik, lc)];
                b.r -= hjc.i * xwal.i;
                b.i += hjc.r * xwal.i;
            }
            j += 1;
            jc -= 1;
        }
        l += 1;
        lc -= 1;
    }
    // shuffling and twiddling
    let (mut j, mut jc) = (1, ip - 1);
    while j < ipph {
        if ido == 1 {
            for ik in 0..idl1 {
                let (t1, t2) = (cc[cx2(ik, j)], cc[cx2(ik, jc)]);
                let (a, b) = pm(t1, t2);
                cc[cx2(ik, j)] = a;
                cc[cx2(ik, jc)] = b;
            }
        } else {
            for k in 0..l1 {
                let (t1, t2) = (cc[cxi(0, k, j)], cc[cxi(0, k, jc)]);
                let (a, b) = pm(t1, t2);
                cc[cxi(0, k, j)] = a;
                cc[cxi(0, k, jc)] = b;
                for i in 1..ido {
                    let (x1, x2) = pm(cc[cxi(i, k, j)], cc[cxi(i, k, jc)]);
                    cc[cxi(i, k, j)] = smul(fwd, x1, wa[(j - 1) * (ido - 1) + i - 1]);
                    cc[cxi(i, k, jc)] = smul(fwd, x2, wa[(jc - 1) * (ido - 1) + i - 1]);
                }
            }
        }
        j += 1;
        jc -= 1;
    }
}

// ------------------------------------------------------------------ Bluestein
fn largest_prime_factor(mut n: usize) -> usize {
    let mut res = 1;
    while n & 1 == 0 {
        res = 2;
        n >>= 1;
    }
    let mut x = 3;
    while x * x <= n {
        while n % x == 0 {
            res = x;
            n /= x;
        }
        x += 2;
    }
    if n > 1 {
        res = n;
    }
    res
}

fn cost_guess(mut n: usize) -> f64 {
    let lfp = 1.1;
    let ni = n;
    let mut result = 0.0;
    while n & 1 == 0 {
        result += 2.0;
        n >>= 1;
    }
    let mut x = 3;
    while x * x <= n {
        while n % x == 0 {
            result += if x <= 5 { x as f64 } else { lfp * x as f64 };
            n /= x;
        }
        x += 2;
    }
    if n > 1 {
        result += if n <= 5 { n as f64 } else { lfp * n as f64 };
    }
    result * ni as f64
}

/// The smallest composite of 2, 3, 5, 7 and 11 that is ≥ n.
fn good_size_cmplx(n: usize) -> usize {
    if n <= 12 {
        return n;
    }
    let mut bestfac = 2 * n;
    let mut f11 = 1;
    while f11 < bestfac {
        let mut f117 = f11;
        while f117 < bestfac {
            let mut f1175 = f117;
            while f1175 < bestfac {
                let mut x = f1175;
                while x < n {
                    x *= 2;
                }
                loop {
                    if x < n {
                        x *= 3;
                    } else if x > n {
                        if x < bestfac {
                            bestfac = x;
                        }
                        if x & 1 == 1 {
                            break;
                        }
                        x >>= 1;
                    } else {
                        return n;
                    }
                }
                f1175 *= 5;
            }
            f117 *= 7;
        }
        f11 *= 11;
    }
    bestfac
}

struct Blue {
    n: usize,
    n2: usize,
    plan: Cfftp,
    bk: Vec<Cx>,
    bkf: Vec<Cx>,
}

impl Blue {
    fn new(n: usize) -> Blue {
        let n2 = good_size_cmplx(n * 2 - 1);
        let plan = Cfftp::new(n2);
        let tmp = SinCos2PiByN::new(2 * n);
        let mut bk = vec![Cx::new(1.0, 0.0); n];
        let mut coeff = 0usize;
        for m in 1..n {
            coeff += 2 * m - 1;
            if coeff >= 2 * n {
                coeff -= 2 * n;
            }
            bk[m] = tmp.get(coeff);
        }
        let xn2 = 1.0 / n2 as f64;
        let mut tbkf = vec![Cx::default(); n2];
        tbkf[0] = bk[0] * xn2;
        for m in 1..n {
            tbkf[m] = bk[m] * xn2;
            tbkf[n2 - m] = tbkf[m];
        }
        plan.exec(&mut tbkf, 1.0, true);
        let bkf = tbkf[..n2 / 2 + 1].to_vec();
        Blue { n, n2, plan, bk, bkf }
    }

    fn exec(&self, c: &mut [Cx], fct: f64, fwd: bool) {
        let (n, n2) = (self.n, self.n2);
        let mut akf = vec![Cx::default(); n2];
        for m in 0..n {
            akf[m] = smul(fwd, c[m], self.bk[m]);
        }
        let zero = akf[0] * 0.0;
        for x in akf.iter_mut().skip(n) {
            *x = zero;
        }
        self.plan.exec(&mut akf, 1.0, true);
        akf[0] = smul(!fwd, akf[0], self.bkf[0]);
        for m in 1..n2.div_ceil(2) {
            akf[m] = smul(!fwd, akf[m], self.bkf[m]);
            akf[n2 - m] = smul(!fwd, akf[n2 - m], self.bkf[m]);
        }
        if n2 & 1 == 0 {
            akf[n2 / 2] = smul(!fwd, akf[n2 / 2], self.bkf[n2 / 2]);
        }
        self.plan.exec(&mut akf, 1.0, false);
        for m in 0..n {
            c[m] = smul(fwd, akf[m], self.bk[m]) * fct;
        }
    }
}

/// pocketfft_c: FFTPACK passes, or Bluestein when the length has a large prime factor and that is cheaper.
pub fn c2c(c: &mut [Cx], fct: f64, fwd: bool) {
    let length = c.len();
    if length == 0 {
        return;
    }
    let tmp = if length < 50 { 0 } else { largest_prime_factor(length) };
    if tmp * tmp <= length {
        return Cfftp::new(length).exec(c, fct, fwd);
    }
    let comp1 = cost_guess(length);
    let comp2 = 2.0 * cost_guess(good_size_cmplx(2 * length - 1)) * 1.5;
    if comp2 < comp1 {
        Blue::new(length).exec(c, fct, fwd)
    } else {
        Cfftp::new(length).exec(c, fct, fwd)
    }
}
