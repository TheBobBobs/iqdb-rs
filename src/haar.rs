use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};

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
    #[allow(clippy::approx_constant)]
    fn haar_2d(a: &mut [f64]) {
        let mut i = 0;
        let mut temp = [0.0; NUM_PIXELS >> 1];

        while i < NUM_PIXELS_SQUARED {
            let mut c = 1.0;

            let mut h = NUM_PIXELS;
            while h > 1 {
                let h1 = h >> 1;
                c *= 0.7071;

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
                c *= 0.7071;

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
                h = h1;
            }
            a[i] *= c;
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
            d: f64,
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
                self.d.total_cmp(&other.d).reverse()
            }
        }
        let mut heap = std::collections::BinaryHeap::with_capacity(NUM_COEFS);

        for i in 1..NUM_COEFS + 1 {
            let value = V {
                i,
                d: data[i].abs(),
            };
            heap.push(value);
        }

        for i in NUM_COEFS + 1..NUM_PIXELS_SQUARED {
            let value = V {
                i,
                d: data[i].abs(),
            };
            let min = heap.peek().unwrap();
            if value.d > min.d {
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

    pub fn from_image(img: &DynamicImage) -> Signature {
        let img = resized(img);

        let mut a = vec![0.0; NUM_PIXELS_SQUARED];
        let mut b = vec![0.0; NUM_PIXELS_SQUARED];
        let mut c = vec![0.0; NUM_PIXELS_SQUARED];

        for y in 0..NUM_PIXELS {
            for x in 0..NUM_PIXELS {
                if let Some(pixel) = img.get_pixel_checked(x as u32, y as u32) {
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

pub fn resized(img: &DynamicImage) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    const ALPHA_MAX: u8 = 127;
    let mut dst = image::ImageBuffer::<Rgba<u8>, _>::new(NUM_PIXELS as u32, NUM_PIXELS as u32);
    for y in 0..NUM_PIXELS {
        for x in 0..NUM_PIXELS {
            let mut spixels = 0.0;
            let (mut red, mut green, mut blue, mut alpha) = (0.0, 0.0, 0.0, 0.0);
            let (mut alpha_sum, mut contrib_sum) = (0.0, 0.0);
            let sy1 = y as f32 * img.height() as f32 / NUM_PIXELS as f32;
            let sy2 = (y + 1) as f32 * img.height() as f32 / NUM_PIXELS as f32;
            let mut sy = sy1;
            let (mut sx, mut sx1, mut sx2);
            while sy < sy2 {
                let mut yportion = 1.0;
                if sy.floor() == sy1.floor() {
                    yportion = 1.0 - (sy - sy.floor());
                    if yportion > sy2 - sy1 {
                        yportion = sy2 - sy1;
                    }
                    sy = sy.floor();
                } else if sy == sy2.floor() {
                    yportion = sy2 - sy2.floor();
                }
                sx1 = x as f32 * img.width() as f32 / NUM_PIXELS as f32;
                sx2 = (x + 1) as f32 * img.width() as f32 / NUM_PIXELS as f32;
                sx = sx1;
                while sx < sx2 {
                    let mut xportion = 1.0;
                    if sx.floor() == sx1.floor() {
                        xportion = 1.0 - (sx - sx.floor());
                        if xportion > sx2 - sx1 {
                            xportion = sx2 - sx1;
                        }
                        sx = sx.floor();
                    } else if sx == sx2.floor() {
                        xportion = sx2 - sx2.floor();
                    }
                    let pcontribution = xportion * yportion;
                    let Rgba([r, g, b, a]) = img.get_pixel(sx as u32, sy as u32);

                    let alpha_factor = (ALPHA_MAX - a) as f32 * pcontribution;
                    red += r as f32 * alpha_factor;
                    green += g as f32 * alpha_factor;
                    blue += b as f32 * alpha_factor;
                    alpha += a as f32 * alpha_factor;
                    alpha_sum += alpha_factor;
                    contrib_sum += pcontribution;
                    spixels += xportion * yportion;
                    sx += 1.0;
                }
                sy += 1.0;
            }

            if spixels != 0.0 {
                red /= spixels;
                green /= spixels;
                blue /= spixels;
                alpha /= spixels;
            }
            if alpha_sum != 0.0 {
                if contrib_sum != 0.0 {
                    alpha_sum /= contrib_sum;
                }
                red /= alpha_sum;
                green /= alpha_sum;
                blue /= alpha_sum;
            }

            red = red.round().clamp(0.0, 255.0);
            green = green.round().clamp(0.0, 255.0);
            blue = blue.round().clamp(0.0, 255.0);
            alpha = alpha.round().clamp(0.0, ALPHA_MAX as f32);

            let pixel = dst.get_pixel_mut(x as u32, y as u32);
            pixel.0 = [red as u8, green as u8, blue as u8, alpha as u8];
        }
    }
    dst
}
