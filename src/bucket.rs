use crate::index::{ChunkId, CHUNK_SIZE};

pub(crate) type Packed = u16;
pub(crate) const PACKED_SIZE: u32 = 16;

const MAX_VEC_LEN: usize = CHUNK_SIZE as usize / (std::mem::size_of::<ChunkId>() * 8);

#[derive(Clone)]
pub(crate) enum Bucket {
    Empty,
    Array([ChunkId; 12]),
    Vec(Vec<ChunkId>),
    Mask(Vec<Packed>),
}

impl Bucket {
    pub(crate) fn new() -> Self {
        Self::Empty
    }

    pub(crate) fn append(&mut self, id: ChunkId) {
        match self {
            Self::Empty => {
                let mut array = [0; 12];
                array[0] = id;
                *self = Self::Array(array);
            }
            Self::Array(array) => {
                let mut index = 1;
                while index < array.len() {
                    if array[index] == 0 {
                        array[index] = id;
                        return;
                    }
                    index += 1;
                }
                let mut vec = array.to_vec();
                vec.push(id);
                *self = Self::Vec(vec);
            }
            Self::Vec(vec) => {
                vec.push(id);
                if vec.len() >= MAX_VEC_LEN {
                    let mut mask = Vec::with_capacity(MAX_VEC_LEN);
                    for &mut chunk_id in vec {
                        let offset = chunk_id % PACKED_SIZE as u16;
                        let index = chunk_id as usize / PACKED_SIZE as usize;
                        while index >= mask.len() {
                            mask.push(0);
                        }
                        mask[index] |= 1 << offset;
                    }
                    *self = Self::Mask(mask);
                }
            }
            Self::Mask(mask) => {
                let chunk_id = id as u32 % CHUNK_SIZE;
                let offset = chunk_id % PACKED_SIZE;
                let index = (chunk_id / PACKED_SIZE) as usize;
                while index >= mask.len() {
                    mask.push(0);
                }
                mask[index] |= 1 << offset;
            }
        }
    }

    pub(crate) fn remove(&mut self, id: ChunkId) {
        match self {
            Self::Empty => {}
            Self::Array(array) => {
                if let Some((index, _)) = array.iter().enumerate().find(|(_, i)| **i == id) {
                    if index == 11 || array[index + 1] == 0 {
                        array[index] = 0;
                        if index == 0 {
                            *self = Self::Empty;
                        }
                        return;
                    }
                    array.copy_within(index + 1.., index);
                    array[11] = 0;
                }
            }
            Self::Vec(vec) => {
                if let Ok(index) = vec.binary_search(&id) {
                    if vec.len() == 1 {
                        *self = Self::Empty;
                    } else {
                        vec.remove(index);
                    }
                }
            }
            Self::Mask(mask) => {
                let len: usize = mask.iter().map(|m| m.count_ones() as usize).sum();
                let offset = id % PACKED_SIZE as u16;
                let index = id as usize / PACKED_SIZE as usize;
                if index < mask.len() {
                    mask[index] &= !(1 << offset);
                }
                if len < MAX_VEC_LEN {
                    let mut vec = Vec::with_capacity(len);
                    for (index, m) in mask.iter().enumerate() {
                        let index = index as ChunkId * PACKED_SIZE as ChunkId;
                        for offset in 0..PACKED_SIZE as ChunkId {
                            if m & (1 << offset) != 0 {
                                vec.push(index + offset);
                            }
                        }
                    }
                    *self = Self::Vec(vec);
                }
            }
        }
    }
}
