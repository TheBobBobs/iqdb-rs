#![feature(stdarch_x86_avx512)]

use std::collections::HashMap;

#[cfg(feature = "multi-thread")]
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};

pub use haar::Signature;
use index::ImageIndex;
pub use sql::{ImageData, SqlDB, SqlSchema};

use crate::index::CHUNK_SIZE;

mod bucket;
mod haar;
mod index;
mod sql;

pub struct DB {
    indexes: Vec<ImageIndex>,
    index_to_id: Vec<i64>,
    id_to_index: HashMap<i64, u32>,
}

impl DB {
    pub fn new(images: impl IntoIterator<Item = ImageData>) -> Self {
        let mut db = Self {
            indexes: Vec::new(),
            index_to_id: Vec::new(),
            id_to_index: HashMap::new(),
        };
        for image in images.into_iter() {
            db.insert(image);
        }
        println!("TotalImages: {}", db.index_to_id.len());
        db
    }

    pub fn contains(&self, id: i64) -> bool {
        self.id_to_index.contains_key(&id)
    }

    pub fn image_count(&self) -> usize {
        self.id_to_index.len()
    }

    pub fn insert(&mut self, image: ImageData) {
        let index = self.index_to_id.len() as u32;
        self.index_to_id.push(image.id);
        self.id_to_index.insert(image.id, index);
        if self.indexes.is_empty() {
            self.indexes.push(ImageIndex::new(0));
        }
        let mut image_index = self.indexes.last_mut().unwrap();
        if image_index.is_full() {
            println!("Images: {}", self.index_to_id.len());
            self.indexes.push(ImageIndex::new(index));
            image_index = self.indexes.last_mut().unwrap();
        }
        let sig = Signature {
            avgl: image.avgl,
            sig: image.sig,
        };
        image_index.append(index, sig)
    }

    pub fn delete(&mut self, image: ImageData) {
        let sig = Signature {
            avgl: image.avgl,
            sig: image.sig,
        };

        let Some(index) = self.id_to_index.remove(&image.id) else {
            return;
        };
        let chunk_index = index / CHUNK_SIZE;
        if let Some(image_index) = self.indexes.get_mut(chunk_index as usize) {
            image_index.remove(index, sig);
        }
    }

    pub fn query(&self, sig: &Signature, limit: usize) -> Vec<(f32, i64)> {
        if limit == 0 {
            return Vec::new();
        }
        let index_to_id = &self.index_to_id;

        let query_index = |image_index: &ImageIndex| {
            let scores = image_index.query(sig, limit);
            scores
                .into_iter()
                .map(|(score, index)| (score, index_to_id[index as usize]))
                .collect::<Vec<_>>()
        };

        #[cfg(feature = "multi-thread")]
        let mut all_scores: Vec<_> = self.indexes.par_iter().map(query_index).flatten().collect();
        #[cfg(not(feature = "multi-thread"))]
        let mut all_scores: Vec<_> = self.indexes.iter().flat_map(query_index).collect();

        all_scores.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)).reverse());
        all_scores.truncate(limit);
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
        let img = image::open("138934.jpg").unwrap();
        let sig = Signature::from_image(&img);

        let start_time = std::time::Instant::now();
        let result = db.query(&sig, 20);
        let elapsed = start_time.elapsed().as_nanos();
        assert_eq!(result[0].0, 93.70242);
        assert_eq!(result[0].1, 138_934);
        println!("Query: {:.3}ms", elapsed as f64 / 1_000. / 1_000.,);
        let ids: Vec<_> = result.iter().map(|(_, id)| id.to_string()).collect();
        let ids = ids.join(",");
        println!("https://danbooru.donmai.us/posts?tags=order:custom+id:{ids}");
    }

    #[test]
    fn signature() {
        let expected = Signature {
            avgl: (
                0.76577718136597,
                -0.00011652168713282838,
                0.004947875142783265,
            ),
            #[rustfmt::skip]
            sig: vec![
                -1933,-1920,-1152,-1029,-1026,-782,-773,-768,-522,-387,-384,-258,-140,-133,-131,-128,-28,-26,-14,-13,-7,-3,1,2,5,10,12,130,138,141,256,259,386,512,770,1024,1027,1280,1925,2560,
                -4864,-2562,-1557,-1550,-1543,-1541,-1536,-1027,-1024,-919,-896,-645,-640,-512,-261,-258,-257,-133,128,131,134,141,256,259,384,646,901,908,1026,1029,1286,1290,1538,2560,2563,2694,4869,4876,5120,5123,
                -5120,-2694,-2563,-2560,-1290,-1286,-1027,-1024,-921,-918,-908,-901,-898,-646,-642,-407,-259,-256,-25,-12,-5,-2,3,13,128,133,140,258,389,396,406,640,643,896,899,919,922,2562,2566,2699,
            ],
        };
        let img = image::open("138934.jpg").unwrap();
        let sig = Signature::from_image(&img);
        assert!((sig.avgl.0 - expected.avgl.0).abs() < 0.0001);
        assert!((sig.avgl.1 - expected.avgl.1).abs() < 0.0001);
        assert!((sig.avgl.2 - expected.avgl.2).abs() < 0.0001);
        for (i, c) in sig.sig.iter().copied().enumerate() {
            assert_eq!(c, expected.sig[i], "{i}");
        }
    }

    #[test]
    fn hash() {
        let sig = Signature {
            avgl: (
                0.76577718136597,
                -0.00011652168713282838,
                0.004947875142783265,
            ),
            #[rustfmt::skip]
            sig: vec![
                -1933,-1920,-1152,-1029,-1026,-782,-773,-768,-522,-387,-384,-258,-140,-133,-131,-128,-28,-26,-14,-13,-7,-3,1,2,5,10,12,130,138,141,256,259,386,512,770,1024,1027,1280,1925,2560,
                -4864,-2562,-1557,-1550,-1543,-1541,-1536,-1027,-1024,-919,-896,-645,-640,-512,-261,-258,-257,-133,128,131,134,141,256,259,384,646,901,908,1026,1029,1286,1290,1538,2560,2563,2694,4869,4876,5120,5123,
                -5120,-2694,-2563,-2560,-1290,-1286,-1027,-1024,-921,-918,-908,-901,-898,-646,-642,-407,-259,-256,-25,-12,-5,-2,3,13,128,133,140,258,389,396,406,640,643,896,899,919,922,2562,2566,2699,
            ],
        };
        let hash = sig.to_string();
        assert_eq!(hash, "iqdb_3fe8813f25bfad46bf1e8ba3578fff323f7444391ec46274f873f880fb80fbfbfbfefcf2fcfbfd00fdf6fe7dfe80fefeff74ff7bff7dff80ffe4ffe6fff2fff3fff9fffd000100020005000a000c0082008a008d0100010301820200030204000403050007850a00ed00f5fef9ebf9f2f9f9f9fbfa00fbfdfc00fc69fc80fd7bfd80fe00fefbfefefeffff7b008000830086008d01000103018002860385038c040204050506050a06020a000a030a861305130c14001403ec00f57af5fdf600faf6fafafbfdfc00fc67fc6afc74fc7bfc7efd7afd7efe69fefdff00ffe7fff4fffbfffe0003000d00800085008c01020185018c019602800283038003830397039a0a020a060a8b");
        let parsed: Signature = hash.parse().unwrap();
        assert_eq!(sig, parsed);
    }
}
