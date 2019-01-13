extern crate base64;
extern crate byteorder;
extern crate id3;
extern crate json;
extern crate metaflac;
extern crate openssl;

use byteorder::ByteOrder;
use byteorder::NativeEndian;
use openssl::symm::{decrypt, Cipher};
use std::io::prelude::*;
use std::io::SeekFrom;
use std::{env, mem};

const AES_CORE_KEY: &[u8; 16] = b"\x68\x7A\x48\x52\x41\x6D\x73\x6F\x35\x6B\x49\x6E\x62\x61\x78\x57";
const AES_MODIFY_KEY: &[u8; 16] =
    b"\x23\x31\x34\x6C\x6A\x6B\x5F\x21\x5C\x5D\x26\x30\x55\x3C\x27\x28";

fn build_key_box(key: &[u8]) -> [u8; 256] {
    let key_len = key.len();
    let mut tmpbox: [u8; 256] = [0; 256];

    for i in 0..256 {
        tmpbox[i] = i as u8;
    }
//    let mut swap: u8;
    let mut c: u64;
    let mut last_byte: u64 = 0;

    for i in 0..256 {
//        swap = tmpbox[i];
        c = (tmpbox[i] as u64 + last_byte + key[(i % key_len) as usize] as u64) & 0xff;
//        tmpbox[i] = tmpbox[c as usize];
//        tmpbox[c as usize] = swap;
        tmpbox.swap(i,c as usize);
        last_byte = c;
    }
    tmpbox
}

fn process_file(path: &std::path::Path) -> std::io::Result<()> {
    let mut ulen: u32;
    // let i: i32;

    let mut f = std::fs::File::open(path).expect("cannot open source file:");
    let mut buf = [0u8; mem::size_of::<u32>()];
    f.read(&mut buf)?;
    ulen = NativeEndian::read_u32(&buf);
    if ulen != 0x4e455443 {
        panic!("Not a netease music file.")
    }
    f.read(&mut buf)?;
    ulen = NativeEndian::read_u32(&buf);
    if ulen != 0x4d414446 {
        panic!("Not a netease music file.")
    }
    f.seek(SeekFrom::Current(2))?;
    let key_len: u32;
    f.read(&mut buf)?;
    key_len = NativeEndian::read_u32(&buf);
    let mut key_data: Vec<u8> = Vec::with_capacity(key_len as usize);
    key_data.resize(key_len as usize, 0);
    f.read_exact(&mut key_data)?;
    for i in 0..key_len {
        (&mut key_data)[i as usize] ^= 0x64;
    }

    let cipher = Cipher::aes_128_ecb();
    let de_key_data =
        decrypt(cipher, AES_CORE_KEY, None, &key_data).expect("error decrypting key data:");
    //    let de_key_len = de_key_data.len() as u32;
    f.read(&mut buf)?;
    ulen = NativeEndian::read_u32(&buf);
    let mut modify_data: Vec<u8> = Vec::with_capacity(ulen as usize);
    modify_data.resize(ulen as usize, 0);
    f.read_exact(&mut modify_data)?;
    for i in 0..ulen {
        modify_data.as_mut_slice()[i as usize] ^= 0x63;
    }
    // let data_len: usize;
    // let mut data: Vec<u8> = Vec::with_capacity(ulen as usize);
    // data.resize(ulen as usize, 0);
    // let mut dedata: Vec<u8> = Vec::with_capacity(ulen as usize);
    // dedata.resize(ulen as usize, 0);

    let data = base64::decode(&modify_data[22..]).expect("error decoding modify_data:");
    let dedata = decrypt(cipher, AES_MODIFY_KEY, None, &data).expect("error decrypting data:");

    let music_info =
        json::parse(std::str::from_utf8(&dedata[6..]).expect("music info is not valid utf-8:"))
            .expect("error parsing json:");
    let music_name = music_info["musicName"].as_str().unwrap();
    let album = music_info["album"].as_str().unwrap();
    let artist = &music_info["artist"];
    let _bitrate = music_info["bitrate"].as_i64().unwrap();
    let _duration = music_info["duration"].as_i64().unwrap();
    let format = music_info["format"].as_str().unwrap();
    let s = path.file_name().unwrap().to_str().unwrap();
    let mut music_filename = s.get(0..s.len() - 4).unwrap().to_owned() + "." + format;

    let mut filter = std::collections::HashMap::new();
    filter.insert("\\", "＼");
    filter.insert("/", "／");
    filter.insert(":", "：");
    filter.insert("*", "＊");
    filter.insert("\"", "＂");
    filter.insert("<", "＜");
    filter.insert(">", "＞");
    filter.insert("|", "｜");
    for (k, v) in filter.iter() {
        music_filename = music_filename.replace(k, v);
    }
    let filter_music_filename = music_filename;
    println!("{}", filter_music_filename);

    f.read(&mut buf)?;
    // ulen = NativeEndian::read_u32(&buf);
    f.seek(SeekFrom::Current(5))?;
    f.read(&mut buf)?;
    let img_len: u32 = NativeEndian::read_u32(&buf);
    let mut img_data: Vec<u8> = Vec::with_capacity(img_len as usize);
    img_data.resize(img_len as usize, 0);
    f.read_exact(&mut img_data)?;
    let kbox = build_key_box(&de_key_data[17..]);
    let mut n: usize = 0x8000;
    let mut buffer = [0u8; 0x8000];
    let mut fmusic = std::fs::File::create(std::path::Path::new(&filter_music_filename))?;
    while n > 1 {
        n = f.read(&mut buffer)?;
        for i in 0..n {
            let j = (i + 1) & 0xff;
            // box[(box[j] + box[(box[j] + j) & 0xff]) & 0xff];
            buffer[i] ^=
                kbox[(kbox[j] as usize + kbox[(kbox[j] as usize + j) & 0xff] as usize) & 0xff];
        }
        fmusic.write(&buffer)?;
    }
    drop(fmusic);
    drop(f);

    if format == "mp3" {
        let mut tag = id3::Tag::new();
        let picture = id3::frame::Picture {
            mime_type: "image/jpeg".to_string(),
            picture_type: id3::frame::PictureType::CoverFront,
            description: String::new(),
            data: img_data,
        };
        tag.add_frame(id3::frame::Frame::with_content(
            "APIC",
            id3::frame::Content::Picture(picture.clone()),
        ));
        tag.set_title(music_name);
        tag.set_album(album);
        let mut artists = String::from(artist[0][0].as_str().unwrap());
        for i in 1..artist.len() {
            artists += ";";
            artists += artist[i][0].as_str().unwrap();
        }
        tag.set_artist(artists);
        tag.write_to_path(
            std::path::Path::new(&filter_music_filename),
            id3::Version::Id3v24,
        )
        .expect("error writing MP3 file:");
    } else if format == "flac" {
        // flac
        let mut tag = metaflac::Tag::new();
        tag.add_picture(
            "image/jpeg",
            metaflac::block::PictureType::CoverFront,
            img_data,
        );
        let mut c = metaflac::block::VorbisComment::new();
        c.set_title(vec![music_name]);
        c.set_album(vec![album]);
        let mut artists: Vec<String> = Vec::new();
        for i in 0..artist.len() {
            artists.push(artist[i][0].as_str().unwrap().to_string());
        }
        c.set_artist(artists);
        tag.push_block(metaflac::block::Block::VorbisComment(c));
        tag.write_to_path(std::path::Path::new(&filter_music_filename))
            .expect("error writing flac file:");
    }

    Ok(())
}
fn main() {
    assert!(env::args().len() >= 2);
    let args: Vec<String> = env::args().collect();
    process_file(std::path::Path::new(&args[1])).expect("process error at main:");
}
