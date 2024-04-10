#[cfg(feature = "simd")]
use std::arch::x86_64::{_mm512_loadu_ps, _mm512_mask_sub_ps, _mm512_set1_ps, _mm512_storeu_ps};

use crate::{
    bucket::{Bucket, Packed},
    haar::Signature,
};

pub(crate) type ChunkId = u16;
pub(crate) const CHUNK_SIZE: u32 = ChunkId::MAX as u32 + 1;

pub(crate) struct ImageIndex {
    offset: u32,
    avgl_y: Vec<f32>,
    avgl_i: Vec<f32>,
    avgl_q: Vec<f32>,
    buckets: [[Vec<Bucket>; 2]; 3],
}

impl ImageIndex {
    pub(crate) fn new(offset: u32) -> Self {
        let buckets = {
            let vecs = vec![Bucket::new(); 128 * 128];
            let signs = [(); 2].map(|_| vecs.clone());
            [(); 3].map(|_| signs.clone())
        };
        Self {
            offset,
            avgl_y: Vec::with_capacity(CHUNK_SIZE as usize),
            avgl_i: Vec::with_capacity(CHUNK_SIZE as usize),
            avgl_q: Vec::with_capacity(CHUNK_SIZE as usize),
            buckets,
        }
    }

    pub(crate) fn is_full(&self) -> bool {
        self.avgl_y.len() == CHUNK_SIZE as usize
    }

    pub(crate) fn append(&mut self, index: u32, signature: Signature) {
        assert_eq!(self.offset + self.avgl_y.len() as u32, index, "Invalid ID");
        self.avgl_y.push(signature.avgl.0 as f32);
        self.avgl_i.push(signature.avgl.1 as f32);
        self.avgl_q.push(signature.avgl.2 as f32);
        if signature.avgl.0 == 0.0 {
            return;
        }
        let id = (index - self.offset) as ChunkId;
        for (coef_i, coef) in signature.sig.into_iter().enumerate() {
            let bucket = self.bucket_mut(coef_i / 40, coef);
            bucket.append(id);
        }
    }

    pub(crate) fn remove(&mut self, index: u32, signature: Signature) {
        let id = (index - self.offset) as usize;
        if id < self.avgl_y.len() {
            self.avgl_y[id] = 0.0;
            for (coef_i, coef) in signature.sig.into_iter().enumerate() {
                let bucket = self.bucket_mut(coef_i / 40, coef);
                bucket.remove(id as ChunkId);
            }
        }
    }

    fn bucket(&self, color: usize, coef: i16) -> &Bucket {
        let sign = coef < 0;
        &self.buckets[color][sign as usize][coef.unsigned_abs() as usize]
    }

    fn bucket_mut(&mut self, color: usize, coef: i16) -> &mut Bucket {
        let sign = coef < 0;
        &mut self.buckets[color][sign as usize][coef.unsigned_abs() as usize]
    }

    pub(crate) fn query(&self, looking_for: &Signature, limit: usize) -> Vec<(f32, u32)> {
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
        let mut scores: Vec<f32> = vec![0.; total + Packed::BITS as usize];

        assert!(total <= self.avgl_y.len());
        assert!(total <= self.avgl_i.len());
        assert!(total <= self.avgl_q.len());
        assert!(total <= scores.len());
        #[allow(clippy::needless_range_loop)]
        for i in 0..total {
            let mut score = 0.;
            score += WEIGHTS[0][0] * (self.avgl_y[i] - looking_for.avgl.0 as f32).abs();
            score += WEIGHTS[0][1] * (self.avgl_i[i] - looking_for.avgl.1 as f32).abs();
            score += WEIGHTS[0][2] * (self.avgl_q[i] - looking_for.avgl.2 as f32).abs();
            scores[i] = score;
        }

        for (coef_i, &coef) in looking_for.sig.iter().enumerate() {
            let bucket = self.bucket(coef_i / 40, coef);

            let w = coef.unsigned_abs();
            let w = (w / 128).max(w % 128).min(5) as usize;
            let weight = WEIGHTS[w][coef_i / 40];
            scale -= weight;

            match &bucket {
                Bucket::Empty => {}
                Bucket::Array(ids) => {
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
                Bucket::Vec(ids) => {
                    for &id in ids {
                        let index = id as usize;
                        *unsafe { scores.get_unchecked_mut(index) } -= weight;
                    }
                }
                Bucket::Mask(mask) => {
                    #[cfg(feature = "simd")]
                    {
                        let m_weight = unsafe { _mm512_set1_ps(weight) };
                        for (index, &m) in mask.iter().enumerate() {
                            let index = index * 16;
                            let m_score = unsafe { _mm512_loadu_ps(scores.as_ptr().add(index)) };
                            let m_score =
                                unsafe { _mm512_mask_sub_ps(m_score, m, m_score, m_weight) };
                            unsafe { _mm512_storeu_ps(scores.as_mut_ptr().add(index), m_score) };
                        }
                    }
                    #[cfg(not(feature = "simd"))]
                    {
                        let _ = mask;
                        unreachable!()
                    }
                }
            }
        }

        let mut sorted = vec![(f32::MAX, 0); limit + 1];
        for (index, score) in scores.into_iter().enumerate().take(total) {
            // is_deleted
            if self.avgl_y[index] == 0. {
                continue;
            }
            if score >= sorted[limit - 1].0 {
                continue;
            }
            let index = index as u32;
            let result = sorted.binary_search_by(|(s, i)| s.total_cmp(&score).then(i.cmp(&index)));
            match result {
                Ok(i) => sorted.insert(i, (score, index)),
                Err(i) => {
                    if i >= limit {
                        continue;
                    }
                    sorted.insert(i, (score, index));
                }
            }
            sorted.truncate(limit);
        }
        sorted.retain(|&(score, id)| !(id == 0 && score == f32::MAX));

        if scale != 0. {
            scale = 1. / scale;
        }
        sorted
            .into_iter()
            .map(|(score, index)| (score * 100. * scale, index + self.offset))
            .collect()
    }
}
