use std::arch::x86_64::{_mm512_loadu_ps, _mm512_mask_sub_ps, _mm512_set1_ps, _mm512_storeu_ps};

use crate::{
    bucket::{Bucket, Ids, PACKED_SIZE},
    Signature,
};

pub(crate) struct ImageIndex {
    offset: usize,
    avgl_y: Vec<f32>,
    avgl_i: Vec<f32>,
    avgl_q: Vec<f32>,
    buckets: [[Vec<Bucket>; 2]; 3],
}

impl ImageIndex {
    pub(crate) fn new(
        offset: usize,
        avgl_y: Vec<f32>,
        avgl_i: Vec<f32>,
        avgl_q: Vec<f32>,
        colors: [[Vec<Vec<u32>>; 2]; 3],
    ) -> Self {
        Self {
            offset,
            avgl_y,
            avgl_i,
            avgl_q,
            buckets: colors.map(|sign| sign.map(|c| c.into_iter().map(Bucket::new).collect())),
        }
    }

    fn get_bucket(&self, color: usize, coef: i16) -> &Bucket {
        let sign = coef < 0;
        &self.buckets[color][sign as usize][coef.unsigned_abs() as usize]
    }

    pub(crate) fn query(&self, looking_for: &Signature) -> Vec<(f32, usize)> {
        const WEIGHTS: [[f32; 3]; 6] = [
            [5.00, 19.21, 34.37],
            [0.83, 1.26, 0.36],
            [1.01, 0.44, 0.45],
            [0.52, 0.53, 0.14],
            [0.47, 0.28, 0.18],
            [0.30, 0.14, 0.27],
        ];
        let total = self.avgl_y.len();

        let mut scale = 0.;
        let mut scores: Vec<f32> = vec![0.; total + PACKED_SIZE as usize];

        for i in 0..total {
            let mut score = 0.;
            score += WEIGHTS[0][0] * (self.avgl_y[i] - looking_for.avgl.0 as f32).abs();
            score += WEIGHTS[0][1] * (self.avgl_i[i] - looking_for.avgl.1 as f32).abs();
            score += WEIGHTS[0][2] * (self.avgl_q[i] - looking_for.avgl.2 as f32).abs();
            scores[i] = score;
        }

        for (coef_i, &coef) in looking_for.sig.iter().enumerate() {
            let bucket = self.get_bucket(coef_i / 40, coef);

            let w = coef.unsigned_abs();
            let w = (w / 128).max(w % 128).min(5) as usize;
            let weight = WEIGHTS[w][coef_i / 40];
            scale -= weight;

            match &bucket {
                Bucket::Ids(Ids::Array(ids)) => {
                    let index = ids[0] as usize;
                    *unsafe { scores.get_unchecked_mut(index) } -= weight;
                    for &id in &ids[1..] {
                        if id == 0 {
                            break;
                        }
                        let index = id as usize;
                        *unsafe { scores.get_unchecked_mut(index) } -= weight;
                    }
                }
                Bucket::Ids(Ids::Vec(ids)) => {
                    for &id in ids {
                        let index = id as usize;
                        *unsafe { scores.get_unchecked_mut(index) } -= weight;
                    }
                }
                Bucket::Mask(mask) => {
                    let m_weight = unsafe { _mm512_set1_ps(weight) };
                    for (index, &m) in mask.iter().enumerate() {
                        let index = index * 16;
                        let m_score = unsafe { _mm512_loadu_ps(scores.as_ptr().add(index)) };
                        let m_score = unsafe { _mm512_mask_sub_ps(m_score, m, m_score, m_weight) };
                        unsafe { _mm512_storeu_ps(scores.as_mut_ptr().add(index), m_score) };
                    }
                }
            }
        }

        let mut sorted = vec![(f32::MAX, 0); 21];
        for (index, score) in scores.into_iter().enumerate().take(total) {
            // is_deleted
            if self.avgl_y[index] == 0. {
                continue;
            }
            if score >= sorted[19].0 {
                continue;
            }
            let result = sorted.binary_search_by(|(s, i)| s.total_cmp(&score).then(i.cmp(&index)));
            match result {
                Ok(i) => sorted.insert(i, (score, index)),
                Err(i) => {
                    if i >= 20 {
                        continue;
                    }
                    sorted.insert(i, (score, index));
                }
            }
            sorted.truncate(20);
        }

        if scale != 0. {
            scale = 1. / scale;
        }
        sorted
            .into_iter()
            .map(|(score, index)| (score * 100. * scale, index + self.offset))
            .collect()
    }
}
