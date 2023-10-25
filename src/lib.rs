#![feature(stdsimd)]

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};

use haar::Signature;
use index::ImageIndex;
use parse::ImageData;

use crate::index::CHUNK_SIZE;

mod bucket;
mod haar;
mod index;
mod parse;

pub struct DB {
    indexes: Vec<ImageIndex>,
    images: Vec<(u32, u32)>,
}

impl DB {
    pub fn new(images: impl IntoIterator<Item = ImageData>) -> Self {
        let mut db = Self {
            indexes: Vec::new(),
            images: Vec::new(),
        };
        for image in images.into_iter() {
            db.insert(image)
        }
        println!("TotalImages: {}", db.images.len());
        db
    }

    pub fn insert(&mut self, image: ImageData) {
        let index = self.images.len() as u32;
        self.images.push((image.id, image.post_id));
        if self.indexes.is_empty() {
            self.indexes.push(ImageIndex::new(0));
        }
        let mut image_index = self.indexes.last_mut().unwrap();
        if image_index.is_full() {
            println!("Images: {}", self.images.len());
            self.indexes.push(ImageIndex::new(index));
            image_index = self.indexes.last_mut().unwrap();
        }
        let sig = Signature {
            avgl: image.avgl,
            sig: image.sig,
        };
        image_index.append(index, sig)
    }

    pub fn delete(&mut self, id: u32, image: ImageData) {
        let sig = Signature {
            avgl: image.avgl,
            sig: image.sig,
        };
        let chunk_index = (id / CHUNK_SIZE) as usize;
        if let Some(image_index) = self.indexes.get_mut(chunk_index) {
            image_index.remove(id, sig);
        }
    }

    pub fn query(&self, sig: &Signature) -> Vec<(f32, u32)> {
        let images = &self.images;
        let mut all_scores: Vec<_> = self
            .indexes
            .par_iter()
            .map(|image_index| {
                let scores = image_index.query(sig);
                scores
                    .into_iter()
                    .map(|(score, index)| (score, images[index as usize].1))
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
        let connection = sqlite::open("iqdb.sqlite").unwrap();
        let db = {
            let query = "SELECT * FROM images";
            let parsed = connection.prepare(query).unwrap().into_iter().map(|row| {
                let values: Vec<sqlite::Value> = row.unwrap().into();
                ImageData::try_from(values).unwrap()
            });
            DB::new(parsed)
        };
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

    #[test]
    fn signature() {
        let sig = Signature::from_image("138934.jpg");
        println!("AVGL: {:?}", sig.avgl);
        println!("Sig1: {:?}", &sig.sig[0..40]);
        println!("Sig2: {:?}", &sig.sig[40..80]);
        println!("Sig3: {:?}", &sig.sig[80..120]);
    }
}
