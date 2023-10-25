use std::f64::consts::FRAC_1_SQRT_2;

use image::imageops::FilterType;

const NUM_PIXELS: usize = 128;
const NUM_PIXELS_SQUARED: usize = NUM_PIXELS * NUM_PIXELS;
const NUM_COEFS: usize = 40;

#[derive(Clone, Debug)]
pub struct Signature {
    pub avgl: (f64, f64, f64),
    pub sig: Vec<i16>,
}

fn rgb_to_yiq(r: &mut [f64], g: &mut [f64], b: &mut [f64]) {
    for i in 0..NUM_PIXELS_SQUARED {
        let y = 0.299 * r[i] + 0.587 * g[i] + 0.114 * b[i];
        let i_ = 0.596 * r[i] - 0.275 * g[i] - 0.321 * b[i];
        let q = 0.212 * r[i] - 0.523 * g[i] + 0.311 * b[i];
        r[i] = y;
        g[i] = i_;
        b[i] = q;
    }
}

impl Signature {
    fn haar_2d(a: &mut [f64]) {
        let mut i = 0;
        let mut temp = [0.0; NUM_PIXELS >> 1];

        while i < NUM_PIXELS_SQUARED {
            let mut c = 1.0;

            let mut h = NUM_PIXELS;
            while h > 1 {
                let h1 = h >> 1;
                c *= FRAC_1_SQRT_2;

                let mut k = 0;
                let mut j1 = i;
                let mut j2 = i;
                while k < h1 {
                    let j21 = j2 + 1;

                    temp[k] = (a[j2] - a[j21]) * c;
                    a[j1] = a[j2] + a[j21];

                    k += 1;
                    j1 += 1;
                    j2 += 2;
                }
                a[i + h1..i + h1 + h1].copy_from_slice(&temp[..h1]);

                h = h1;
            }
            a[i] *= c;

            i += NUM_PIXELS;
        }

        for i in 0..NUM_PIXELS {
            let mut c = 1.0;

            let mut h = NUM_PIXELS;
            while h > 1 {
                let h1 = h >> 1;
                c *= FRAC_1_SQRT_2;

                let mut k = 0;
                let mut j1 = i;
                let mut j2 = i;
                while k < h1 {
                    let j21 = j2 + NUM_PIXELS;

                    temp[k] = (a[j2] - a[j21]) * c;
                    a[j1] = a[j2] + a[j21];

                    k += 1;
                    j1 += NUM_PIXELS;
                    j2 += 2 * NUM_PIXELS;
                }
                let mut k = 0;
                let mut j1 = i + h1 * NUM_PIXELS;
                while k < h1 {
                    a[j1] = temp[k];

                    k += 1;
                    j1 += NUM_PIXELS;
                }
                a[i] *= c;

                h = h1;
            }
        }
    }

    fn transform(r: &mut [f64], g: &mut [f64], b: &mut [f64]) {
        rgb_to_yiq(r, g, b);

        Self::haar_2d(r);
        Self::haar_2d(g);
        Self::haar_2d(b);
        r[0] /= 256.0 * 128.0;
        g[0] /= 256.0 * 128.0;
        b[0] /= 256.0 * 128.0;
    }

    fn get_m_largest(data: &[f64]) -> [i16; NUM_COEFS] {
        struct V {
            i: usize,
            d: i16,
        }
        impl PartialEq for V {
            fn eq(&self, other: &Self) -> bool {
                self.d == other.d
            }
        }
        impl Eq for V {}
        impl PartialOrd for V {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for V {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.d.cmp(&other.d).reverse()
            }
        }
        let mut heap = std::collections::BinaryHeap::with_capacity(NUM_COEFS);

        for i in 1..NUM_COEFS + 1 {
            let value = V {
                i,
                d: data[i].abs() as i16,
            };
            heap.push(value);
        }

        for i in NUM_COEFS + 1..NUM_PIXELS_SQUARED {
            let value = V {
                i,
                d: data[i].abs() as i16,
            };
            if value.d > heap.peek().unwrap().d {
                heap.pop();
                heap.push(value);
            }
        }

        let mut cnt = 0;
        let mut sig = [0; NUM_COEFS];
        while let Some(value) = heap.pop() {
            let mut c = value.i as i16;
            if data[value.i] <= 0.0 {
                c = -c;
            }
            sig[cnt] = c;
            cnt += 1;
        }
        assert_eq!(cnt, NUM_COEFS);
        sig
    }

    fn calc_haar(a: &[f64], b: &[f64], c: &[f64]) -> Signature {
        let avgl = (a[0], b[0], c[0]);
        let mut sig1 = Self::get_m_largest(a);
        let mut sig2 = Self::get_m_largest(b);
        let mut sig3 = Self::get_m_largest(c);
        sig1.sort();
        sig2.sort();
        sig3.sort();
        let mut sig = Vec::with_capacity(NUM_COEFS * 3);
        sig.extend(sig1);
        sig.extend(sig2);
        sig.extend(sig3);

        Signature { avgl, sig }
    }

    pub fn from_image(path: &str) -> Signature {
        let img = image::open(path).unwrap();
        // TOOD: Produces different values than IQDB.
        let img = img.resize_exact(NUM_PIXELS as u32, NUM_PIXELS as u32, FilterType::Nearest);
        let rgb = img.as_rgb8().unwrap();

        let mut a = vec![0.0; NUM_PIXELS_SQUARED];
        let mut b = vec![0.0; NUM_PIXELS_SQUARED];
        let mut c = vec![0.0; NUM_PIXELS_SQUARED];

        for y in 0..NUM_PIXELS {
            for x in 0..NUM_PIXELS {
                if let Some(pixel) = rgb.get_pixel_checked(x as u32, y as u32) {
                    let index = x + y * NUM_PIXELS;
                    a[index] = pixel[0] as f64;
                    b[index] = pixel[1] as f64;
                    c[index] = pixel[2] as f64;
                }
            }
        }

        Self::transform(&mut a, &mut b, &mut c);
        Self::calc_haar(&a, &b, &c)
    }
}
