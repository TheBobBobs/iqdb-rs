use crate::Signature;

#[derive(Clone, Debug)]
pub struct ImageData {
    pub id: i64,
    pub avgl: (f64, f64, f64),
    pub sig: Vec<i16>,
}

#[derive(Clone, Copy, Debug)]
pub enum SqlSchema {
    V1,
    V2,
}

pub struct SqlDB {
    schema: SqlSchema,
    connection: sqlite::Connection,
}

impl SqlDB {
    pub fn new(connection: sqlite::Connection) -> Self {
        let query = "SELECT sql FROM sqlite_master WHERE name='images'";
        let mut schema = connection
            .prepare(query)
            .unwrap()
            .into_iter()
            .map(|row| {
                let values: Vec<sqlite::Value> = row.unwrap().into();
                let Some(sqlite::Value::String(sql)) = values.first() else {
                    panic!()
                };

                if sql.contains("post_id") {
                    SqlSchema::V1
                } else {
                    SqlSchema::V2
                }
            })
            .next();
        if schema.is_none() {
            let create = "
            CREATE TABLE IF NOT EXISTS 'images'
            (
                'id' INTEGER PRIMARY KEY NOT NULL ,
                'avglf1' REAL NOT NULL , 'avglf2' REAL NOT NULL , 'avglf3' REAL NOT NULL ,
                'sig' BLOB NOT NULL
            )";
            connection.execute(create).unwrap();
            schema = Some(SqlSchema::V2);
        }
        let schema = schema.unwrap();
        dbg!(schema);
        Self { schema, connection }
    }

    pub fn load(&self) -> impl IntoIterator<Item = ImageData> + '_ {
        let query = "SELECT * FROM images";
        self.connection
            .prepare(query)
            .unwrap()
            .into_iter()
            .map(|row| {
                let values: Vec<sqlite::Value> = row.unwrap().into();
                self.parse(values).unwrap()
            })
    }

    pub fn get_many(
        &self,
        ids: impl IntoIterator<Item = i64>,
    ) -> impl Iterator<Item = ImageData> + '_ {
        let ids: Vec<String> = ids.into_iter().map(|i| i.to_string()).collect();
        let query = match self.schema {
            SqlSchema::V1 => format!("SELECT * FROM images WHERE post_id IN ({})", ids.join(", ")),
            SqlSchema::V2 => format!("SELECT * FROM images WHERE id IN ({})", ids.join(", ")),
        };
        self.connection
            .prepare(query)
            .unwrap()
            .into_iter()
            .map(|row| {
                let values: Vec<sqlite::Value> = row.unwrap().into();
                self.parse(values).unwrap()
            })
    }

    pub fn insert(&self, id: i64, sig: &Signature) -> Result<(), sqlite::Error> {
        let sig_bytes: Vec<u8> = sig.sig.iter().flat_map(|i| i.to_le_bytes()).collect();

        let query = match self.schema {
            SqlSchema::V1 => {
                "INSERT INTO images (post_id, avglf1, avglf2, avglf3, sig)
                VALUES (:id, :avglf1, :avglf2, :avglf3, :sig)"
            }
            SqlSchema::V2 => {
                "INSERT INTO images (id, avglf1, avglf2, avglf3, sig)
                VALUES (:id, :avglf1, :avglf2, :avglf3, :sig)"
            }
        };
        let mut statement = self.connection.prepare(query).unwrap();
        statement
            .bind::<&[(_, sqlite::Value)]>(
                &[
                    (":id", id.into()),
                    (":avglf1", sig.avgl.0.into()),
                    (":avglf2", sig.avgl.1.into()),
                    (":avglf3", sig.avgl.2.into()),
                    (":sig", sig_bytes.into()),
                ][..],
            )
            .unwrap();
        if let Some(Err(error)) = statement.into_iter().next() {
            return Err(error);
        };

        Ok(())
    }

    pub fn delete(&mut self, id: i64) -> Result<Option<ImageData>, sqlite::Error> {
        let query = match self.schema {
            SqlSchema::V1 => "DELETE FROM images WHERE post_id = ? RETURNING *",
            SqlSchema::V2 => "DELETE FROM images WHERE id = ? RETURNING *",
        };
        let mut statement = self.connection.prepare(query).unwrap();
        statement.bind((1, id)).unwrap();
        let row = match statement.into_iter().next() {
            Some(Ok(row)) => row,
            Some(Err(e)) => return Err(e),
            None => return Ok(None),
        };
        let values: Vec<sqlite::Value> = row.into();
        let image = self.parse(values).unwrap();
        Ok(Some(image))
    }

    fn parse(&self, values: Vec<sqlite::Value>) -> Result<ImageData, ()> {
        use sqlite::Value::*;
        if values.len() < 5 {
            return Err(());
        }
        let mut iter = values.into_iter();
        if matches!(self.schema, SqlSchema::V1) {
            // Skip unused ID
            iter.next();
        }
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
                Ok(ImageData {
                    id,
                    avgl: (avglf1, avglf2, avglf3),
                    sig,
                })
            }
            _ => Err(()),
        }
    }
}
