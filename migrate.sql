CREATE TABLE 'temp_images'
(
	'id' INTEGER PRIMARY KEY NOT NULL ,
	'avglf1' REAL NOT NULL , 'avglf2' REAL NOT NULL , 'avglf3' REAL NOT NULL ,
	'sig' BLOB NOT NULL
);

INSERT INTO temp_images SELECT post_id, avglf1, avglf2, avglf3, sig FROM images;

ALTER TABLE `images` RENAME TO `old_images`;

ALTER TABLE `temp_images` RENAME TO `images`;
