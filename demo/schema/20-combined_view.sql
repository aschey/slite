CREATE VIEW vw_all AS
SELECT * FROM song s 
INNER JOIN artist ar on s.artist_id = ar.artist_id
INNER JOIN album al on al.album_id = s.album_id
INNER JOIN album_artist aa on aa.album_artist_id = al.album_artist_id