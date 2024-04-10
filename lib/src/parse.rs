#[derive(Clone, Debug)]
pub struct ImageData {
    pub id: i64,
    pub avgl: (f64, f64, f64),
    pub sig: Vec<i16>,
}

impl TryFrom<Vec<sqlite::Value>> for ImageData {
    type Error = ();

    fn try_from(value: Vec<sqlite::Value>) -> Result<Self, Self::Error> {
        use sqlite::Value::*;
        if value.len() < 5 {
            return Err(());
        }
        let mut iter = value.into_iter();
        let slice = [0u32; 5].map(|_| iter.next().unwrap());
        match slice {
            [Integer(id), Float(avglf1), Float(avglf2), Float(avglf3), Binary(sig_bytes)] => {
                assert_eq!(sig_bytes.len(), 240);
                let mut sig = Vec::with_capacity(120);
                for c in sig_bytes.chunks_exact(2) {
                    let bytes = [c[0], c[1]];
                    let i = i16::from_le_bytes(bytes);
                    sig.push(i);
                }
                sig[0..40].sort();
                sig[40..80].sort();
                sig[80..120].sort();
                Ok(Self {
                    id,
                    avgl: (avglf1, avglf2, avglf3),
                    sig,
                })
            }
            _ => Err(()),
        }
    }
}
