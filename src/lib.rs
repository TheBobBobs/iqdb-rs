#![feature(stdsimd)]

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};

use index::ImageIndex;
use parse::ImageData;

use crate::bucket::CHUNK_SIZE;

mod bucket;
mod index;
mod parse;

#[derive(Clone)]
pub struct Signature {
    avgl: (f64, f64, f64),
    sig: Vec<i16>,
}

pub struct DB {
    indexes: Vec<ImageIndex>,
    images: Vec<(u32, u32)>,
}

impl DB {
    pub fn new(path: &str) -> Self {
        let connection = sqlite::open(path).unwrap();
        let query = "SELECT * FROM images";
        let parsed = connection.prepare(query).unwrap().into_iter().map(|row| {
            let values: Vec<sqlite::Value> = row.unwrap().into();
            ImageData::try_from(values).unwrap()
        });

        let mut avgl_y = Vec::with_capacity(CHUNK_SIZE);
        let mut avgl_i = Vec::with_capacity(CHUNK_SIZE);
        let mut avgl_q = Vec::with_capacity(CHUNK_SIZE);
        let mut colors = {
            let v = vec![Vec::new(); 128 * 128];
            let signs = [(); 2].map(|_| v.clone());
            [(); 3].map(|_| signs.clone())
        };

        let mut indexes = Vec::new();
        let mut images = Vec::new();
        let mut offset = 0;
        for (i, image) in parsed.into_iter().enumerate() {
            if i >= offset + CHUNK_SIZE {
                println!("{}", i);
                let image_index = ImageIndex::new(offset, avgl_y, avgl_i, avgl_q, colors);
                avgl_y = Vec::with_capacity(CHUNK_SIZE);
                avgl_i = Vec::with_capacity(CHUNK_SIZE);
                avgl_q = Vec::with_capacity(CHUNK_SIZE);
                colors = {
                    let v = vec![Vec::new(); 128 * 128];
                    let signs = [(); 2].map(|_| v.clone());
                    [(); 3].map(|_| signs.clone())
                };
                indexes.push(image_index);
                offset += CHUNK_SIZE;
            }
            images.push((image.id, image.post_id));

            avgl_y.push(image.avgl.0 as f32);
            avgl_i.push(image.avgl.1 as f32);
            avgl_q.push(image.avgl.2 as f32);
            for (coef_i, &coef) in image.sig.iter().enumerate() {
                let color = coef_i / 40;
                let sign = coef < 0;
                colors[color][sign as usize][coef.unsigned_abs() as usize]
                    .push((i - offset) as u32);
            }
        }
        Self { indexes, images }
    }

    pub fn query(&self, sig: &Signature) -> Vec<(f32, u32)> {
        let mut all_scores: Vec<_> = self
            .indexes
            .par_iter()
            .map(|image_index| {
                let scores = image_index.query(sig);
                scores
                    .into_iter()
                    .map(|(score, index)| (score, self.images[index].1))
                    .collect::<Vec<_>>()
            })
            .flatten()
            .collect();
        all_scores.sort_by(|a, b| a.partial_cmp(b).unwrap().reverse());
        all_scores.truncate(20);
        all_scores
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query() {
        let db = DB::new("iqdb.sqlite");
        // Post 138934
        let sig = Signature {
            avgl: (
                0.7636196999411157,
                -8.506085782950134e-5,
                0.004679521949641186,
            ),
            sig: vec![
                -1920, -1155, -1152, -1029, -1026, -782, -773, -768, -522, -387, -384, -258, -140,
                -133, -131, -128, -28, -26, -14, -13, -7, -3, 1, 2, 5, 10, 12, 130, 138, 141, 256,
                259, 386, 512, 770, 1024, 1027, 1280, 1925, 2560, -2562, -1557, -1550, -1543,
                -1541, -1536, -1027, -1024, -896, -645, -640, -512, -266, -261, -258, -257, -149,
                -133, -130, 12, 128, 131, 134, 141, 256, 259, 642, 646, 901, 908, 1026, 1029, 1286,
                1290, 1292, 2560, 2563, 2694, 5120, 5123, -5120, -2694, -2563, -2560, -1290, -1286,
                -1024, -921, -918, -908, -901, -898, -646, -642, -259, -256, -25, -12, -5, -2, 3,
                13, 128, 131, 133, 140, 258, 389, 396, 406, 640, 643, 651, 896, 899, 922, 1291,
                2562, 2566, 2699,
            ],
        };
        let start_time = std::time::Instant::now();
        let result = db.query(&sig);
        let elapsed = start_time.elapsed().as_nanos();
        assert_eq!(result[0].0, 100.0);
        assert_eq!(result[0].1, 138_934);
        println!("Query: {:.3}ms", elapsed as f64 / 1_000. / 1_000.,);
        let ids: Vec<_> = result.iter().map(|(_, id)| id.to_string()).collect();
        let ids = ids.join(",");
        println!("https://danbooru.donmai.us/posts?tags=order:custom+id:{ids}");
    }
}
