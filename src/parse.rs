#[derive(Clone, Debug)]
pub struct ImageData {
    pub id: u32,
    pub post_id: u32,
    pub avgl: (f64, f64, f64),
    pub sig: Vec<i16>,
}

impl TryFrom<Vec<sqlite::Value>> for ImageData {
    type Error = ();

    fn try_from(value: Vec<sqlite::Value>) -> Result<Self, Self::Error> {
        use sqlite::Value::*;
        if value.len() < 6 {
            return Err(());
        }
        let mut iter = value.into_iter();
        let slice = [0u32; 6].map(|_| iter.next().unwrap());
        match slice {
            [Integer(id), Integer(post_id), Float(avglf1), Float(avglf2), Float(avglf3), Binary(mut sig)] =>
            {
                sig.shrink_to_fit();
                let mut sig = std::mem::ManuallyDrop::new(sig);
                if (sig.capacity() % 2) != 0 {
                    return Err(());
                };
                let length = sig.len() / 2;
                let capacity = sig.capacity() / 2;
                let sig = sig.as_mut_ptr() as *mut i16;
                let mut sig = unsafe { Vec::from_raw_parts(sig, length, capacity) };
                if sig.len() != 120 {
                    panic!("Invalid signature len: {}", sig.len());
                }
                sig[0..40].sort();
                sig[40..80].sort();
                sig[80..120].sort();
                Ok(Self {
                    id: id as u32,
                    post_id: post_id as u32,
                    avgl: (avglf1, avglf2, avglf3),
                    sig,
                })
            }
            _ => Err(()),
        }
    }
}
