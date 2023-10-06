pub(crate) type Packed = u16;
pub(crate) const PACKED_SIZE: u32 = 16;

pub(crate) type ChunkId = u16;
pub(crate) const CHUNK_SIZE: usize = ChunkId::MAX as usize + 1;

pub(crate) enum Ids {
    Array([ChunkId; 12]),
    Vec(Vec<ChunkId>),
}

pub(crate) enum Bucket {
    Ids(Ids),
    Mask(Vec<Packed>),
}

impl Bucket {
    pub(crate) fn new(indices: Vec<u32>) -> Self {
        assert!(indices.len() <= CHUNK_SIZE);
        let last = indices.last().copied().unwrap_or(0);
        let last = last % CHUNK_SIZE as u32;
        let ids_size = std::mem::size_of::<ChunkId>() * indices.len();
        let mask_size = std::mem::size_of::<Packed>() * ((last / PACKED_SIZE) as usize + 1);
        if ids_size > mask_size {
            let mut mask = Vec::with_capacity((last / PACKED_SIZE) as usize + 1);
            for index in indices {
                let chunk_id = index as usize % CHUNK_SIZE;
                let offset = chunk_id % PACKED_SIZE as usize;
                let index = chunk_id / PACKED_SIZE as usize;
                while index >= mask.len() {
                    mask.push(0);
                }
                mask[index] |= 1 << offset;
            }
            Self::Mask(mask)
        } else if indices.len() <= 12 {
            let mut ids = [0; 12];
            indices
                .into_iter()
                .map(|index| (index % CHUNK_SIZE as u32) as ChunkId)
                .enumerate()
                .for_each(|(i, id)| ids[i] = id);
            Self::Ids(Ids::Array(ids))
        } else {
            let ids = indices
                .into_iter()
                .map(|index| (index % CHUNK_SIZE as u32) as ChunkId)
                .collect();
            Self::Ids(Ids::Vec(ids))
        }
    }
}
